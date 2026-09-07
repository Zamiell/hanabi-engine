use hanabi_core::{Action, Card, CardId, Clue, PlayerId, PlayerView};

use crate::{LogicalDeductions, SymbolicLineOutcome};

use super::{
    ConditionalPlan, HGroupProfile, PerspectiveDepth, PerspectiveProjector, PlanFrontier,
    ProjectedAction, ProjectedConsequences, ProspectiveTransition, identity_of,
    infer_h_group_from_replay, is_playable_now, select_h_group_action,
};

/// Projects the convention policy's actions while leaving unknown draws blank.
/// Strategic choices continue under that policy; unresolved identities stop
/// the line rather than being filled using the actual hidden hand or deck.
pub(crate) fn project_h_group_line(
    source: &PlayerView,
    profile: HGroupProfile,
    root: Action,
    limit: u8,
) -> SymbolicLineOutcome {
    project_h_group_plan(source, profile, root, limit).summarize()
}

fn project_h_group_plan(
    source: &PlayerView,
    profile: HGroupProfile,
    root: Action,
    limit: u8,
) -> ConditionalPlan {
    let mut plan = ConditionalPlan::default();
    let mut public = source.clone();
    let mut action = Some(root);

    while let Some(current) = action {
        if public.status != hanabi_core::GameStatus::InProgress {
            plan.stop_at(PlanFrontier::Terminal);
            break;
        }
        if plan.len() >= usize::from(limit) {
            plan.stop_at(PlanFrontier::Limit);
            break;
        }
        let actor = public.current_player;
        let Some((actor_deductions, actor_replay)) = PerspectiveProjector::new(&public, profile)
            .project(actor, PerspectiveDepth::NestedRecipients)
        else {
            plan.stop_at(PlanFrontier::ProjectionUnavailable);
            break;
        };
        let actor_inferences = infer_h_group_from_replay(&actor_deductions, actor_replay, profile);
        if charm_depends_on_hidden_connector(&public, &actor_inferences, current) {
            plan.stop_at(PlanFrontier::InterpretationBranch);
            break;
        }
        let Some((after, consequences)) = apply_symbolic_action(
            &public,
            &actor_deductions,
            &actor_inferences,
            actor,
            current,
        ) else {
            plan.stop_at(PlanFrontier::IdentityBranch);
            break;
        };
        plan.push(
            ProjectedAction {
                actor,
                action: current,
            },
            consequences,
        );
        public = after;
        if public.status != hanabi_core::GameStatus::InProgress {
            plan.stop_at(PlanFrontier::Terminal);
            break;
        }
        let next = public.current_player;
        let Some((next_deductions, _)) = PerspectiveProjector::new(&public, profile)
            .project(next, PerspectiveDepth::NestedRecipients)
        else {
            plan.stop_at(PlanFrontier::ProjectionUnavailable);
            break;
        };
        action = select_h_group_action(&next_deductions, profile);
    }
    plan.assess(super::frontier_value::evaluate(
        source, &public, profile, root,
    ));
    plan
}

/// A blank in the giver's hand is visible to the reactor. It cannot prove
/// that three blind plays are required in the reactor's hand.
/// Source: <https://hanabi.github.io/level-23/#the-4-charm>
fn charm_depends_on_hidden_connector(
    source: &PlayerView,
    inferred: &super::HGroupInferences,
    action: Action,
) -> bool {
    let Action::Play(card) = action else {
        return false;
    };
    let Some(signal) = inferred.signals.iter().rev().find(|signal| {
        signal.turn + 1 == source.turn
            && signal.kind == super::HGroupMoveKind::Charm
            && signal.target == Some(source.current_player)
            && signal.cards.contains(&card)
    }) else {
        return false;
    };
    let Some(clue) = inferred.clues.iter().find(|clue| clue.turn == signal.turn) else {
        return false;
    };
    let Some(focus) = identity_of(source, clue.focus) else {
        return true;
    };
    let Ok(deductions) = LogicalDeductions::new(source.clone()) else {
        return true;
    };
    let explicitly_clued = source
        .hands
        .iter()
        .flatten()
        .filter(|card| super::was_clued_before(source, signal.turn, card.id))
        .map(|card| card.id)
        .collect();
    for (owner, hand) in source.hands.iter().enumerate() {
        if owner == source.current_player.index() {
            continue;
        }
        for (slot, card) in hand
            .iter()
            .enumerate()
            .filter(|(_, card)| card.identity.is_none())
        {
            let Some(domain) = deductions.possible_identities(card.id) else {
                return true;
            };
            for identity in domain
                .iter()
                .filter(|identity| identity.suit == focus.suit && identity.rank < focus.rank)
            {
                // A possibility probe, not a determinized rollout: no points
                // or future actions from this assignment are credited.
                let mut possible = source.clone();
                possible.hands[owner][slot].identity = Some(identity);
                if super::recognition::four_charm_blind_plays(
                    &possible,
                    source.current_player,
                    focus,
                    clue.stack_heights,
                    &explicitly_clued,
                    signal.turn,
                ) < 3
                {
                    return true;
                }
            }
        }
    }
    false
}

fn apply_symbolic_action(
    source: &PlayerView,
    actor_deductions: &LogicalDeductions,
    actor_inferences: &super::HGroupInferences,
    actor: PlayerId,
    action: Action,
) -> Option<(PlayerView, ProjectedConsequences)> {
    let mut consequences = ProjectedConsequences::default();
    match action {
        Action::Clue { target, clue } => {
            let touched = touched_cards(source, target, clue)?;
            consequences.clues_spent = 1;
            Some((
                ProspectiveTransition::clue_by(source, actor, target, clue, &touched),
                consequences,
            ))
        }
        Action::Play(card) => {
            let identity = symbolic_identity(source, actor_deductions, actor_inferences, card)?;
            let successful = is_playable_now(source, identity);
            if successful {
                consequences.score_gain = 1;
                if identity.rank.number() == 5 {
                    consequences.clues_gained = 1;
                }
            } else {
                consequences.strikes = 1;
            }
            Some((
                ProspectiveTransition::play(source, actor, card, identity, successful),
                consequences,
            ))
        }
        Action::Discard(card) => {
            let identity = symbolic_identity(source, actor_deductions, actor_inferences, card)?;
            consequences.discards = 1;
            if source.clue_tokens < hanabi_core::MAX_CLUE_TOKENS {
                consequences.clues_gained = 1;
            }
            Some((
                ProspectiveTransition::discard(source, actor, card, identity),
                consequences,
            ))
        }
    }
}

fn symbolic_identity(
    source: &PlayerView,
    deductions: &LogicalDeductions,
    inferences: &super::HGroupInferences,
    card: CardId,
) -> Option<Card> {
    identity_of(source, card).or_else(|| {
        inferences
            .cards
            .iter()
            .find(|inference| inference.card == card)
            .and_then(|inference| {
                (inference.identities.len() == 1)
                    .then(|| inference.identities.iter().next())
                    .flatten()
            })
            .or_else(|| {
                deductions
                    .possible_identities(card)
                    .filter(|identities| identities.len() == 1)
                    .and_then(|identities| identities.iter().next())
            })
    })
}

fn touched_cards(source: &PlayerView, target: PlayerId, clue: Clue) -> Option<Vec<CardId>> {
    let hand = source.hands.get(target.index())?;
    let mut touched = Vec::new();
    for card in hand {
        let matches = if let Some(identity) = card.identity {
            clue.matches(identity)
        } else {
            let identities = crate::IdentitySet::from_mask(card.clues.identity_mask());
            if identities.is_empty() {
                return None;
            }
            if identities.iter().all(|identity| clue.matches(identity)) {
                true
            } else if identities.iter().all(|identity| !clue.matches(identity)) {
                false
            } else {
                return None;
            }
        };
        if matches {
            touched.push(card.id);
        }
    }
    (!touched.is_empty()).then_some(touched)
}

#[cfg(test)]
mod tests {
    use crate::SymbolicStopReason;
    use hanabi_core::{FullState, PlayerId, standard_deck};

    use super::*;

    #[test]
    fn excluded_connector_identities_do_not_create_a_charm_branch() {
        let replay = hanabi_protocol::HanabiLiveReplay::from_json(include_str!(
            "../../../hanabi-protocol/tests/fixtures/game-p4v0s1.json"
        ))
        .unwrap();
        let state = replay.state_at_turn(0).unwrap();
        let view = state.view_for(state.current_player()).unwrap();
        let target = PlayerId::new(2);
        let clue = Clue::Rank(hanabi_core::Rank::Four);
        let mut public = ProspectiveTransition::clue_by(
            &view,
            view.current_player,
            target,
            clue,
            &touched_cards(&view, target, clue).unwrap(),
        );
        let (d, r) = PerspectiveProjector::new(&public, HGroupProfile::Max)
            .project(public.current_player, PerspectiveDepth::NestedRecipients)
            .unwrap();
        let inferred = infer_h_group_from_replay(&d, r, HGroupProfile::Max);
        let action = Action::Play(CardId::new(4));
        assert!(charm_depends_on_hidden_connector(
            &public, &inferred, action
        ));
        // Algorithmic domain boundary, not a proposed game continuation:
        // ruling out blue on every blank rules out the external blue connector.
        for card in public
            .hands
            .iter_mut()
            .flatten()
            .filter(|card| card.identity.is_none())
        {
            card.clues
                .add_negative_clue(Clue::Suit(hanabi_core::Suit::Blue));
        }
        assert!(!charm_depends_on_hidden_connector(
            &public, &inferred, action
        ));
    }

    #[test]
    fn reviewed_save_continues_through_the_next_policy_choice() {
        // p4v0s2 turn 8: test projection mechanics, not optimality of the Save.
        let replay = hanabi_protocol::HanabiLiveReplay::from_json(include_str!(
            "../../../hanabi-protocol/tests/fixtures/game-p4v0s2.json"
        ))
        .unwrap();
        let state = replay.state_at_turn(7).unwrap();
        let view = state.view_for(state.current_player()).unwrap();
        let root = Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Rank(hanabi_core::Rank::Two),
        };
        let plan = project_h_group_plan(&view, HGroupProfile::Max, root, 32);
        assert!(plan.len() > 1, "must advance beyond the root clue");
        assert!(plan.len() <= 32);
    }

    #[test]
    fn unknown_root_identity_ends_the_line_at_a_symbolic_branch() {
        let state = FullState::new_standard(2, standard_deck()).unwrap();
        let view = state.view_for(PlayerId::new(0)).unwrap();
        let outcome = project_h_group_line(
            &view,
            HGroupProfile::Max,
            Action::Play(view.hands[0][0].id),
            32,
        );

        assert_eq!(outcome.actions, 0);
        assert_eq!(outcome.stop_reason, SymbolicStopReason::UnknownIdentity);
    }

    #[test]
    fn symbolic_action_limit_has_a_distinct_stop_reason() {
        let state = FullState::new_standard(2, standard_deck()).unwrap();
        let view = state.view_for(PlayerId::new(0)).unwrap();
        let outcome = project_h_group_line(
            &view,
            HGroupProfile::Max,
            Action::Play(view.hands[0][0].id),
            0,
        );

        assert_eq!(outcome.actions, 0);
        assert_eq!(outcome.stop_reason, SymbolicStopReason::Limit);
    }
}
