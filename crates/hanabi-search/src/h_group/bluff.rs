//! Shared Bluff semantics used by candidate generation and history replay.

use super::{
    Card, CardId, CardSet, Clue, ConnectionManager, HGroupConnectionKind, PlayerId, PlayerView,
    Rank, identity_of,
};

/// Literal identity established by the clue itself is safe evidence for an
/// already-clued intermediate. Do not use later clues or the clue's own
/// speculative convention claims to justify its prerequisites.
pub(super) fn literal_identity_through(view: &PlayerView, card: CardId, turn: u32) -> Option<Card> {
    let mut facts = hanabi_core::ClueFacts::default();
    for entry in view.history.iter().filter(|entry| entry.turn <= turn) {
        if let hanabi_core::ObservedEvent::Clued {
            clue,
            touched,
            untouched,
            ..
        } = &entry.event
        {
            if touched.contains(&card) {
                facts.add_positive_clue(*clue);
            } else if untouched.contains(&card) {
                facts.add_negative_clue(*clue);
            }
        }
    }
    let identities = crate::IdentitySet::from_mask(facts.identity_mask());
    (identities.len() == 1)
        .then(|| identities.iter().next())
        .flatten()
}

/// A connector that is already due makes the clue a truthful continuation,
/// not a reason for the next player to invent another blind connector.
/// <https://hanabi.github.io/level-11/#bobs-truth-principle-part-1>
pub(super) fn bluff_connector_is_promised(
    view: &PlayerView,
    hands: &[Vec<CardId>],
    already_playing: &CardSet,
    pending: &ConnectionManager,
    connector: Card,
    current_focus: Option<CardId>,
) -> bool {
    hands
        .iter()
        .flatten()
        .any(|card| already_playing.contains(card) && identity_of(view, *card) == Some(connector))
        || pending.iter().any(|connection| {
            // A Prompt selected by this clue also supplies the truthful
            // connector. Excluding every same-focus connection lets Bluff
            // recognition append an unrelated blind play to a normal Prompt.
            // Same-clue speculative Finesses remain excluded: those are the
            // interpretations whose bluff alternative is being considered.
            (Some(connection.focus) != current_focus
                || connection.kind == HGroupConnectionKind::Prompt)
                && connection.expected == connector
                && pending.is_active(connection)
        })
}

/// A Bluff cannot wait behind an older Finesse, but an ordinary clued play
/// does not prevent the next player from immediately demonstrating a Bluff.
/// During clue replay, exclude connections established by this very clue.
/// <https://hanabi.github.io/level-11/#queued-bluffs-illegal>
pub(super) fn bluff_is_queued(
    pending: &ConnectionManager,
    actor: PlayerId,
    current_focus: Option<CardId>,
) -> bool {
    pending.iter().any(|connection| {
        connection.actor == actor
            && connection.kind == HGroupConnectionKind::Finesse
            && Some(connection.focus) != current_focus
            && pending.is_active(connection)
    })
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum BluffTargetKind {
    Ordinary,
    Three,
}

/// The missing first connector may be bluffed through already-clued higher
/// connectors. Availability must come from the caller's dated perspective.
/// <https://hanabi.github.io/level-11/#bluffs-through-already-clued-cards>
pub(super) fn bluff_through_clued_cards(
    height: usize,
    focus: Card,
    available: impl Fn(Card) -> bool,
) -> bool {
    let rank = usize::from(focus.rank.number());
    rank > height + 1
        // The missing low connector must really be missing. An existing
        // clued connector makes this a truthful chain, not another Bluff.
        && (rank == height + 2 || !available(Card::new(focus.suit, Rank::ALL[height])))
        && ((height + 2)..rank)
            .all(|needed| available(Card::new(focus.suit, Rank::ALL[needed - 1])))
}

/// Classifies targets using only clue-time public stacks.
///
/// Ordinary Bluffs target a card one rank beyond playable. Level 13 also lets
/// a rank-3 clue target any still-useful future 3; this is the rule that makes
/// a Hard 3 Bluff possible from empty stacks.
pub(super) fn bluff_target_kind_at(
    stack_heights: [u8; 5],
    clue: Clue,
    focus: Card,
) -> Option<BluffTargetKind> {
    let height = stack_heights[focus.suit.index()];
    if focus.rank.number() == height.saturating_add(2) {
        Some(BluffTargetKind::Ordinary)
    } else if clue == Clue::Rank(Rank::Three)
        && focus.rank == Rank::Three
        && focus.rank.number() > height.saturating_add(1)
    {
        Some(BluffTargetKind::Three)
    } else {
        None
    }
}

/// A Self-Bluff is legal with a rank clue. Suit Self-Bluffs are a separate
/// max-level family and must not be admitted by the ordinary Bluff rule.
pub(super) fn bluff_target_order_is_legal(clue: Clue, actor: PlayerId, target: PlayerId) -> bool {
    actor != target || matches!(clue, Clue::Rank(_))
}

/// Whether the blind play connects to the clue instead of demonstrating a
/// Bluff. Color clues connect by suit. Rank clues connect only consecutive
/// ranks, so a 1 does not connect to a rank-3 clue even when both cards share
/// a suit (the Hard 3 Bluff case).
pub(super) fn bluff_play_connects(clue: Clue, played: Card) -> bool {
    match clue {
        Clue::Suit(suit) => played.suit == suit,
        Clue::Rank(rank) => played.rank.number().saturating_add(1) == rank.number(),
    }
}

#[cfg(test)]
mod tests {
    use hanabi_core::Suit;

    use super::*;

    #[test]
    fn rank_bluffs_only_connect_on_consecutive_ranks() {
        assert!(!bluff_play_connects(
            Clue::Rank(Rank::Three),
            Card::new(Suit::Yellow, Rank::One),
        ));
        assert!(bluff_play_connects(
            Clue::Rank(Rank::Three),
            Card::new(Suit::Red, Rank::Two),
        ));
    }

    #[test]
    fn an_empty_stack_rank_three_is_the_special_future_target() {
        assert_eq!(
            bluff_target_kind_at(
                [0; 5],
                Clue::Rank(Rank::Three),
                Card::new(Suit::Yellow, Rank::Three),
            ),
            Some(BluffTargetKind::Three),
        );
    }
}
