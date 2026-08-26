use hanabi_core::{Card, CardId, ObservedCard, PlayerView, Rank};

use super::CardSet;

pub(super) fn chop(hand: &[CardId], gotten: &CardSet) -> Option<CardId> {
    hand.iter().copied().find(|card| !gotten.contains(card))
}

pub(super) fn finesse_position<'a>(
    hand: &'a [ObservedCard],
    gotten: &CardSet,
    position: usize,
) -> Option<&'a ObservedCard> {
    hand.iter()
        .rev()
        .filter(|card| !gotten.contains(&card.id))
        .nth(position)
}

pub(super) fn finesse_position_id(
    hand: &[CardId],
    gotten: &CardSet,
    position: usize,
) -> Option<CardId> {
    hand.iter()
        .rev()
        .filter(|card| !gotten.contains(card))
        .nth(position)
        .copied()
}

pub(super) fn five_pulled_card(
    hand: &[CardId],
    touched: &[CardId],
    gotten: &CardSet,
) -> Option<CardId> {
    let five_position = touched
        .iter()
        .copied()
        .filter(|card| !gotten.contains(card))
        .filter_map(|card| {
            hand.iter()
                .position(|candidate| *candidate == card)
                .map(|position| (position, card))
        })
        .max_by_key(|(position, _)| *position)
        .map(|(position, _)| position)?;
    hand[..five_position]
        .iter()
        .rev()
        .copied()
        .find(|card| !gotten.contains(card))
}

/// Returns the single card moved by a
/// [5's Chop Move](https://hanabi.github.io/level-4/#the-5s-chop-move-5cm).
///
/// The rightmost touched 5 is the boundary. Counting only cards that were
/// previously unclued or unmoved, it is a 5CM exactly when the next card to
/// its right is the recipient's current chop.
pub(super) fn five_chop_moved_card(
    hand: &[CardId],
    touched: &[CardId],
    gotten: &CardSet,
) -> Option<CardId> {
    let boundary = touched
        .iter()
        .filter_map(|card| hand.iter().position(|candidate| candidate == card))
        .min()?;
    let moved = hand[..boundary]
        .iter()
        .rev()
        .copied()
        .find(|card| !gotten.contains(card))?;
    (chop(hand, gotten) == Some(moved)).then_some(moved)
}

pub(super) fn focus(
    hand: &[CardId],
    touched: &[CardId],
    chop: Option<CardId>,
    gotten: &CardSet,
) -> Option<CardId> {
    let newly_touched = touched
        .iter()
        .copied()
        .filter(|card| !gotten.contains(card))
        .collect::<Vec<_>>();
    match newly_touched.as_slice() {
        [] => hand
            .iter()
            .rev()
            .copied()
            .find(|card| touched.contains(card)),
        [only] => Some(*only),
        _ if chop.is_some_and(|card| touched.contains(&card)) => chop,
        _ => hand
            .iter()
            .rev()
            .copied()
            .find(|card| newly_touched.contains(card)),
    }
}

pub(super) fn is_critical(view: &PlayerView, identity: Card) -> bool {
    identity.rank != Rank::Five
        && view
            .discard_pile
            .iter()
            .filter(|(_, discarded)| *discarded == identity)
            .count()
            + 1
            == usize::from(identity.rank.copies())
}

pub(super) fn remove_card(hand: &mut Vec<CardId>, card: CardId) {
    if let Some(position) = hand.iter().position(|candidate| *candidate == card) {
        hand.remove(position);
    }
}
