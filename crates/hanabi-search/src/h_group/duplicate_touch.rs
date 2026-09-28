//! A duplicate touch needs an executable, owner-readable repair path.
//! User-reviewed p4v0s415 T32: a teammate obtains the missing intermediate
//! before the recipient can misplay collateral; leftmost order then consumes
//! the real successor and renders the extra touched copy harmless.
use std::cell::Cell;

use super::{
    Action, Card, CardId, ClueProposal, HGroupProfile, IdentitySet, PerspectiveDepth,
    PerspectiveProjector, PlayerId, PlayerView, compiled_prospective_clue, identity_of,
    infer_h_group_from_replay, is_eventually_useful, is_playable_now, ordered_playable_cards,
    select_h_group_action, was_clued_before,
};

thread_local! {
    static ACTIVE: Cell<bool> = const { Cell::new(false) };
}

struct ProofGuard;
impl Drop for ProofGuard {
    fn drop(&mut self) {
        ACTIVE.set(false);
    }
}

/// This is a sufficient certificate, not permission for arbitrary bad touches.
/// Follow the ordinary policy for at most two rotations, without recursively
/// assuming other future-repair exceptions. Unresolved identities fail proof.
pub(super) fn certify(
    source: &PlayerView,
    profile: HGroupProfile,
    proposal: &ClueProposal,
) -> Option<crate::ClueSafetyContinuation> {
    if ACTIVE.get() || source.clue_tokens == 0 {
        return None;
    }
    let Action::Clue { target, clue } = proposal.action else {
        return None;
    };
    let touched = source.hands[target.index()]
        .iter()
        .filter(|card| card.identity.is_some_and(|identity| clue.matches(identity)))
        .map(|card| card.id)
        .collect::<Vec<_>>();
    let compiled = compiled_prospective_clue(source, profile, target, clue, &touched)?;
    let recipient = compiled.projection(target)?;
    let focus = recipient
        .inferred
        .clues
        .iter()
        .find(|meaning| meaning.turn == source.turn && meaning.target == target)?
        .focus;
    let duplicate = identity_of(source, focus)?;
    if !is_playable_now(source, duplicate) {
        return None;
    }
    // This bounded proof handles a newly acquired focus with one future
    // successor rank and extra copies of the focus. Other duplicate patterns
    // retain ordinary Good Touch admission until independently justified.
    if touched
        .iter()
        .any(|card| was_clued_before(source, source.turn, *card))
    {
        return None;
    }
    let collateral = touched
        .iter()
        .copied()
        .filter(|card| *card != focus)
        .collect::<Vec<_>>();
    let future = collateral
        .iter()
        .filter_map(|card| identity_of(source, *card))
        .filter(|identity| *identity != duplicate)
        .collect::<Vec<_>>();
    let successor = *future.first()?;
    if successor.suit != duplicate.suit
        || successor.rank <= duplicate.rank
        || future.iter().any(|identity| *identity != successor)
        || !collateral
            .iter()
            .any(|card| identity_of(source, *card) == Some(duplicate))
        || collateral.iter().any(|card| {
            identity_of(source, *card)
                .is_none_or(|identity| !is_eventually_useful(source, identity))
        })
    {
        return None;
    }
    prove_continuation(
        source,
        profile,
        proposal.action,
        compiled.after(),
        target,
        &collateral,
        successor,
    )
}

fn prove_continuation(
    source: &PlayerView,
    profile: HGroupProfile,
    action: Action,
    after: &PlayerView,
    target: PlayerId,
    collateral: &[CardId],
    successor: Card,
) -> Option<crate::ClueSafetyContinuation> {
    ACTIVE.set(true);
    let _guard = ProofGuard;
    super::inverse_planning::baseline(|| {
        let mut public = after.clone();
        let mut actions = vec![super::ProjectedAction {
            actor: source.current_player,
            action,
        }];
        for _ in 0..source.hands.len() * 2 {
            crate::budget::checkpoint();
            if public.status != hanabi_core::GameStatus::InProgress {
                return None;
            }
            if let Some(card) = safe_successor(&public, profile, target, collateral, successor) {
                return Some(crate::ClueSafetyContinuation {
                    first_turn: source.turn,
                    collateral: collateral.to_vec(),
                    expected: successor,
                    actions,
                    successor: card,
                });
            }
            let actor = public.current_player;
            let (d, replay) = PerspectiveProjector::new(&public, profile)
                .project(actor, PerspectiveDepth::NestedRecipients)?;
            let inferred = infer_h_group_from_replay(&d, replay, profile);
            let action = select_h_group_action(&d, profile)?;
            if let Action::Clue { .. } = action {
                if public.clue_tokens == 0 {
                    return None;
                }
            }
            if let Action::Play(card) | Action::Discard(card) = action {
                // Future draws stay blank even if a convention hypothesizes
                // their identity. No proof may depend on the recorded deck.
                if !source.hands.iter().flatten().any(|held| held.id == card) {
                    return None;
                }
                if matches!(action, Action::Discard(_))
                    && identity_of(&public, card).is_none_or(|id| is_eventually_useful(&public, id))
                {
                    return None;
                }
            }
            let (after, consequences) =
                super::symbolic_line::apply_symbolic_action(&public, &d, &inferred, actor, action)?;
            if consequences.strikes != 0 {
                return None;
            }
            actions.push(super::ProjectedAction { actor, action });
            public = after;
        }
        None
    })
}

/// The future clue has removed every lower alternative. Every prerequisite is
/// already established in its owner's knowledge. All the recipient's peers
/// have the same remaining note, and their ordinary order plays a real copy
/// first. Once that identity is played, the remaining peers are obsolete.
fn safe_successor(
    public: &PlayerView,
    profile: HGroupProfile,
    target: PlayerId,
    collateral: &[CardId],
    successor: Card,
) -> Option<CardId> {
    if !is_eventually_useful(public, successor) {
        return None;
    }
    let (d, replay) = PerspectiveProjector::new(public, profile)
        .project(target, PerspectiveDepth::NestedRecipients)?;
    let mut inferred = infer_h_group_from_replay(&d, replay, profile);
    if collateral.iter().any(|card| {
        !inferred
            .cards
            .iter()
            .any(|note| note.card == *card && note.identities == IdentitySet::singleton(successor))
    }) {
        return None;
    }
    let mut established = IdentitySet::default();
    for player in 0..public.hands.len() {
        let (d, replay) = PerspectiveProjector::new(public, profile).project(
            PlayerId::new(u8::try_from(player).ok()?),
            PerspectiveDepth::NestedRecipients,
        )?;
        let notes = infer_h_group_from_replay(&d, replay, profile);
        for note in &notes.cards {
            if note.identities.len() == 1 {
                let identity = note.identities.iter().next()?;
                if identity_of(public, note.card).is_none_or(|actual| actual == identity) {
                    established = established.union(note.identities);
                }
            }
        }
    }
    for rank in
        (public.play_stacks[successor.suit.index()].len() + 1)..usize::from(successor.rank.number())
    {
        if !established.contains(Card::new(successor.suit, super::Rank::ALL[rank - 1])) {
            return None;
        }
    }
    // Compare these indistinguishable notes with the normal play-order code;
    // a wrong first copy must fail, not be silently skipped using visible faces.
    inferred.playable_now = collateral.to_vec();
    let first = *ordered_playable_cards(d.view(), &inferred, profile).first()?;
    if identity_of(public, first) != Some(successor) {
        return None;
    }
    if collateral.iter().any(|card| {
        *card != first
            && identity_of(public, *card).is_none_or(|identity| {
                identity != successor && is_eventually_useful(public, identity)
            })
    }) {
        return None;
    }
    Some(first)
}

#[cfg(test)]
mod tests {
    use super::super::{Clue, LogicalDeductions, Rank, Suit};
    use super::*;

    fn reviewed_source() -> PlayerView {
        let fixture = hanabi_protocol::HanabiLiveReplay::from_json(include_str!(
            "../../../hanabi-protocol/tests/fixtures/game-p4v0s415.json"
        ))
        .unwrap();
        fixture
            .state_at_turn(31)
            .unwrap()
            .view_for(PlayerId::new(3))
            .unwrap()
    }

    fn proposal(source: &PlayerView) -> ClueProposal {
        let d = LogicalDeductions::new(source.clone()).unwrap();
        let replay = super::super::replay_h_group(&d, HGroupProfile::Max);
        super::super::interpretation::h_group_clue_candidates_from_replay_inner(
            &d,
            HGroupProfile::Max,
            &replay,
        )
        .into_iter()
        .find(|candidate| {
            candidate.action
                == Action::Clue {
                    target: PlayerId::new(1),
                    clue: Clue::Suit(Suit::Purple),
                }
        })
        .unwrap()
    }

    #[test]
    fn reviewed_duplicate_touch_has_funded_safe_continuation() {
        let source = reviewed_source();
        let candidate = proposal(&source);
        let witness =
            certify(&source, HGroupProfile::Max, &candidate).expect("reviewed safe lookahead");
        assert_eq!(witness.successor, CardId::new(34));
        assert_eq!(witness.expected, Card::new(Suit::Purple, Rank::Four));
        assert_eq!(
            witness
                .actions
                .iter()
                .map(|step| (step.actor.index(), step.action))
                .collect::<Vec<_>>(),
            vec![
                (3, candidate.action),
                (0, Action::Play(CardId::new(27))),
                (1, Action::Play(CardId::new(5))),
                (
                    2,
                    Action::Clue {
                        target: PlayerId::new(0),
                        clue: Clue::Suit(Suit::Purple)
                    }
                ),
            ]
        );
        assert!(source.hands[3].iter().all(|card| card.identity.is_none()));
        // Evidence ablations and an unsafe-order counterfactual from the same
        // reviewed position. None may earn the prospective exception.
        for control in 0..3 {
            let mut branch = source.clone();
            if control == 0 {
                branch.clue_tokens = 0;
            } else if control == 1 {
                branch.hands[0]
                    .iter_mut()
                    .find(|card| card.id == CardId::new(33))
                    .unwrap()
                    .identity = None;
                for entry in &mut branch.history {
                    if let super::super::ObservedEvent::Drew { card, identity, .. } =
                        &mut entry.event
                    {
                        if *card == CardId::new(33) {
                            *identity = None;
                        }
                    }
                }
            } else {
                for card in branch.hands.iter_mut().flatten() {
                    if card.id == CardId::new(6) {
                        card.identity = Some(Card::new(Suit::Purple, Rank::Four));
                    }
                    if card.id == CardId::new(34) {
                        card.identity = Some(Card::new(Suit::Purple, Rank::Two));
                    }
                }
                for entry in &mut branch.history {
                    if let super::super::ObservedEvent::Drew { card, identity, .. } =
                        &mut entry.event
                    {
                        if *card == CardId::new(6) {
                            *identity = Some(Card::new(Suit::Purple, Rank::Four));
                        }
                        if *card == CardId::new(34) {
                            *identity = Some(Card::new(Suit::Purple, Rank::Two));
                        }
                    }
                }
            }
            assert!(
                certify(&branch, HGroupProfile::Max, &candidate).is_none(),
                "control {control}"
            );
        }
    }
}
