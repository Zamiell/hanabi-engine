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
    plan
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
