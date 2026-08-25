//! Shared Bluff semantics used by candidate generation and history replay.

use super::{Card, Clue, PlayerId, Rank};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum BluffTargetKind {
    Ordinary,
    Three,
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
