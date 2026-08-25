use super::{
    Action, ClueCandidate, ConventionRejectionReason, HGroupProfile, HGroupState,
    LogicalDeductions, PlayerView, RejectedConventionAction, chop, focus, identity_of,
    prospective_clue_signal_kinds,
};

/// Classifies every legal clue excluded from the convention action set.
/// Candidate generation remains the source of admissibility; this pass makes
/// every exclusion inspectable instead of collapsing all failures to a
/// missing action.
pub(crate) fn h_group_rejected_clues_from_replay(
    deductions: &LogicalDeductions,
    _profile: HGroupProfile,
    replay: &HGroupState,
    admitted: &[Action],
) -> Vec<RejectedConventionAction> {
    let view = deductions.view();
    let promptable = replay.promptable();
    let gotten = replay.gotten_from(&promptable);
    view.legal_actions()
        .into_iter()
        .filter_map(|action| {
            let Action::Clue { target, clue } = action else {
                return None;
            };
            if admitted.contains(&action) {
                return None;
            }
            let hand = &view.hands[target.index()];
            let touched = hand
                .iter()
                .filter(|card| card.identity.is_some_and(|identity| clue.matches(identity)))
                .map(|card| card.id)
                .collect::<Vec<_>>();
            let adds_information = hand
                .iter()
                .any(|card| touched.contains(&card.id) && !card.clues.has_positive_clue(clue));
            let reason = if adds_information {
                let old_chop = chop(&replay.hands[target.index()], &gotten);
                let Some(focus) = focus(&replay.hands[target.index()], &touched, old_chop, &gotten)
                else {
                    return Some(RejectedConventionAction {
                        action,
                        reason: ConventionRejectionReason::NoFocus,
                    });
                };
                let identity = identity_of(view, focus);
                if gotten.contains(&focus)
                    && identity.is_some()
                    && replay.cards.facts.known_identity(focus) == identity
                {
                    ConventionRejectionReason::RepeatsKnownIdentity
                } else if replay.cards.already_playing.contains(&focus)
                    && touched.iter().all(|card| gotten.contains(card))
                {
                    ConventionRejectionReason::RedundantOutcome
                } else {
                    ConventionRejectionReason::NoConventionMeaning
                }
            } else {
                ConventionRejectionReason::NoNewInformation
            };
            Some(RejectedConventionAction { action, reason })
        })
        .collect()
}

pub(in crate::h_group) fn recipient_replay_recognizes_candidate(
    view: &PlayerView,
    profile: HGroupProfile,
    candidate: &ClueCandidate,
) -> bool {
    let Action::Clue { target, clue } = candidate.action else {
        return false;
    };
    let touched = view.hands[target.index()]
        .iter()
        .filter(|card| card.identity.is_some_and(|identity| clue.matches(identity)))
        .map(|card| card.id)
        .collect::<Vec<_>>();
    let signals = prospective_clue_signal_kinds(view, profile, target, clue, &touched);
    !signals.is_empty()
}
