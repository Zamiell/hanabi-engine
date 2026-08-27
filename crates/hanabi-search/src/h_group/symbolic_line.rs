use hanabi_core::{Action, Card, CardId, Clue, PlayerId, PlayerView};

use crate::{LogicalDeductions, SymbolicLineOutcome, SymbolicStopReason};

use super::{
    HGroupProfile, PerspectiveDepth, PerspectiveProjector, ProspectiveTransition,
    h_group_predictable_action, identity_of, infer_h_group_from_replay, is_playable_now,
};

/// Projects convention-determined public actions while leaving unknown draws
/// blank. The line stops at the first genuine choice or identity branch.
pub(crate) fn project_h_group_line(
    source: &PlayerView,
    profile: HGroupProfile,
    root: Action,
    limit: u8,
) -> SymbolicLineOutcome {
    let mut outcome = SymbolicLineOutcome::default();
    let mut public = source.clone();
    let mut action = Some(root);

    while let Some(current) = action {
        if outcome.actions >= limit {
            outcome.stop_reason = SymbolicStopReason::Limit;
            break;
        }
        let actor = public.current_player;
        let Some((actor_deductions, actor_replay)) = PerspectiveProjector::new(&public, profile)
            .project(actor, PerspectiveDepth::NestedRecipients)
        else {
            outcome.stop_reason = SymbolicStopReason::ProjectionUnavailable;
            break;
        };
        let actor_inferences = infer_h_group_from_replay(&actor_deductions, actor_replay, profile);
        let Some(after) = apply_symbolic_action(
            &public,
            &actor_deductions,
            &actor_inferences,
            actor,
            current,
            &mut outcome,
        ) else {
            outcome.stop_reason = SymbolicStopReason::UnknownIdentity;
            break;
        };
        public = after;
        outcome.actions = outcome.actions.saturating_add(1);
        let next = public.current_player;
        let Some((next_deductions, _)) = PerspectiveProjector::new(&public, profile)
            .project(next, PerspectiveDepth::NestedRecipients)
        else {
            outcome.stop_reason = SymbolicStopReason::ProjectionUnavailable;
            break;
        };
        action = h_group_predictable_action(&next_deductions, profile);
    }
    outcome
}

fn apply_symbolic_action(
    source: &PlayerView,
    actor_deductions: &LogicalDeductions,
    actor_inferences: &super::HGroupInferences,
    actor: PlayerId,
    action: Action,
    outcome: &mut SymbolicLineOutcome,
) -> Option<PlayerView> {
    match action {
        Action::Clue { target, clue } => {
            let touched = touched_cards(source, target, clue)?;
            outcome.clues_spent = outcome.clues_spent.saturating_add(1);
            Some(ProspectiveTransition::clue_by(
                source, actor, target, clue, &touched,
            ))
        }
        Action::Play(card) => {
            let identity = symbolic_identity(source, actor_deductions, actor_inferences, card)?;
            let successful = is_playable_now(source, identity);
            if successful {
                outcome.score_gain = outcome.score_gain.saturating_add(1);
                if identity.rank.number() == 5 {
                    outcome.clues_gained = outcome.clues_gained.saturating_add(1);
                }
            } else {
                outcome.strikes = outcome.strikes.saturating_add(1);
            }
            Some(ProspectiveTransition::play(
                source, actor, card, identity, successful,
            ))
        }
        Action::Discard(card) => {
            let identity = symbolic_identity(source, actor_deductions, actor_inferences, card)?;
            outcome.discards = outcome.discards.saturating_add(1);
            if source.clue_tokens < hanabi_core::MAX_CLUE_TOKENS {
                outcome.clues_gained = outcome.clues_gained.saturating_add(1);
            }
            Some(ProspectiveTransition::discard(
                source, actor, card, identity,
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
    source.hands.get(target.index()).map(|hand| {
        hand.iter()
            .filter(|card| card.identity.is_some_and(|identity| clue.matches(identity)))
            .map(|card| card.id)
            .collect()
    })
}

#[cfg(test)]
mod tests {
    use hanabi_core::{FullState, PlayerId, standard_deck};

    use super::*;

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
