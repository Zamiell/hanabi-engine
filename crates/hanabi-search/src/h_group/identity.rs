use hanabi_core::{Card, CardId, ObservedEvent, PlayerView, Rank};

use super::{CardSet, ConventionFacts, HGroupCardInference};

/// Identity visible in the source view at its current turn. Historical rule
/// interpretation must use `HistoricalView` instead.
pub(super) fn identity_of(view: &PlayerView, card: CardId) -> Option<Card> {
    view.hands
        .iter()
        .flatten()
        .find(|candidate| candidate.id == card)
        .and_then(|candidate| candidate.identity)
        .or_else(|| {
            view.play_stacks
                .iter()
                .flatten()
                .chain(view.discard_pile.iter())
                .find_map(|(candidate, identity)| (*candidate == card).then_some(*identity))
        })
        .or_else(|| {
            view.history.iter().find_map(|entry| match entry.event {
                ObservedEvent::Played {
                    card: candidate,
                    identity,
                    ..
                }
                | ObservedEvent::Discarded {
                    card: candidate,
                    identity,
                    ..
                } if candidate == card => Some(identity),
                ObservedEvent::Drew {
                    card: candidate,
                    identity: Some(identity),
                    ..
                } if candidate == card => Some(identity),
                _ => None,
            })
        })
}

pub(super) fn is_playable_now(view: &PlayerView, identity: Card) -> bool {
    identity.rank.number()
        == u8::try_from(view.play_stacks[identity.suit.index()].len())
            .expect("a standard stack has at most five cards")
            + 1
}

pub(super) fn is_playable_at(stack_heights: [u8; 5], identity: Card) -> bool {
    identity.rank.number() == stack_heights[identity.suit.index()] + 1
}

pub(super) fn is_trash_at(stack_heights: [u8; 5], identity: Card) -> bool {
    identity.rank.number() <= stack_heights[identity.suit.index()]
}

pub(super) fn card_is_trash(view: &PlayerView, identity: Card) -> bool {
    usize::from(identity.rank.number()) <= view.play_stacks[identity.suit.index()].len()
}

pub(super) fn is_eventually_useful(view: &PlayerView, identity: Card) -> bool {
    let stack_height = view.play_stacks[identity.suit.index()].len();
    if usize::from(identity.rank.number()) <= stack_height {
        return false;
    }
    Rank::ALL
        .iter()
        .copied()
        .filter(|rank| {
            usize::from(rank.number()) > stack_height && rank.number() < identity.rank.number()
        })
        .all(|rank| {
            let lower = Card::new(identity.suit, rank);
            view.discard_pile
                .iter()
                .filter(|(_, card)| *card == lower)
                .count()
                < usize::from(rank.copies())
        })
}

pub(super) fn is_convention_trash(
    view: &PlayerView,
    identity: Card,
    gotten: &CardSet,
    own_notes: &[HGroupCardInference],
) -> bool {
    if !is_eventually_useful(view, identity) {
        return true;
    }
    view.hands
        .iter()
        .flatten()
        .filter(|card| gotten.contains(&card.id))
        .filter(|card| {
            card.identity == Some(identity)
                || (card.identity.is_none()
                    && own_notes.iter().any(|note| {
                        note.card == card.id
                            && note.identities.len() == 1
                            && note.identities.contains(identity)
                    }))
        })
        .take(2)
        .count()
        >= 2
}

/// Whether this specific card is useless because its identity is either
/// already played or already represented by another protected card.
///
/// Unlike [`is_convention_trash`], this form is suitable while compiling a
/// clue event: the newly touched card may still have several possible
/// identities, and each possibility must be checked against exact convention
/// facts established elsewhere on the board.
pub(super) fn is_card_identity_accounted_trash(
    view: &PlayerView,
    card: CardId,
    identity: Card,
    stack_heights: [u8; 5],
    gotten: &CardSet,
    convention_facts: &ConventionFacts,
) -> bool {
    identity.rank.number() <= stack_heights[identity.suit.index()]
        || gotten.iter().copied().any(|other| {
            other != card
                && (identity_of(view, other) == Some(identity)
                    || convention_facts.known_identity(other) == Some(identity))
        })
}

pub(super) fn is_unique_visible(view: &PlayerView, excluded: CardId, identity: Card) -> bool {
    !view
        .hands
        .iter()
        .flatten()
        .any(|card| card.id != excluded && card.identity == Some(identity))
}
