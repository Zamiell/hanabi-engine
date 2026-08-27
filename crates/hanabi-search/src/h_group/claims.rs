use hanabi_core::{Card, CardId, PlayerId, PlayerView};

use super::{
    CardSet, ClueFacts, ConventionFacts, HGroupClueInterpretation, HGroupState, HistoricalView,
    IdentitySet,
};

/// Observer-safe authority for exact convention identity claims. Relational
/// `OneOf` claims deliberately do not become exact claims on every candidate.
pub(super) struct IdentityClaims<'a> {
    view: &'a PlayerView,
    replay: &'a HGroupState,
}

impl<'a> IdentityClaims<'a> {
    pub(super) const fn new(view: &'a PlayerView, replay: &'a HGroupState) -> Self {
        Self { view, replay }
    }

    pub(super) fn exact_identity(&self, card: CardId) -> Option<Card> {
        self.view
            .hands
            .iter()
            .flatten()
            .find(|observed| observed.id == card)
            .and_then(|observed| observed.identity)
            .or_else(|| self.replay.cards.facts.known_identity(card))
            .or_else(|| {
                let clue = self
                    .replay
                    .clues
                    .iter()
                    .rev()
                    .find(|clue| clue.focus == card)?;
                (clue.focus_identities.len() == 1)
                    .then(|| clue.focus_identities.iter().next())
                    .flatten()
            })
    }

    pub(super) fn identity_claimed_elsewhere(&self, excluded: CardId, identity: Card) -> bool {
        self.replay
            .cards
            .already_playing
            .iter()
            .any(|card| *card != excluded && self.exact_identity(*card) == Some(identity))
            || self.replay.pending_connections.iter().any(|connection| {
                connection.expected == identity && !connection.cards.contains(&excluded)
            })
            || self.replay.hands.iter().flatten().copied().any(|card| {
                card != excluded
                    && !self.replay.cards.facts.fixed_cards().contains(&card)
                    && !self.replay.cards.invalidated_focuses.contains(&card)
                    && self
                        .replay
                        .clues
                        .iter()
                        .rev()
                        .find(|clue| clue.focus == card)
                        .is_some_and(|clue| {
                            clue.play_identities == IdentitySet::singleton(identity)
                        })
            })
    }
}

/// Exact Good Touch claims visible while a clue is being interpreted. The
/// full replay state does not exist until the event loop completes, so this
/// boundary accepts the canonical components directly.
#[allow(clippy::too_many_arguments)]
pub(super) fn claimed_identities_at_clue(
    focus: CardId,
    giver: PlayerId,
    hands: &[Vec<CardId>],
    historical: &HistoricalView<'_>,
    clue_facts: &[ClueFacts],
    convention_facts: &ConventionFacts,
    clues: &[HGroupClueInterpretation],
    gotten: &CardSet,
) -> IdentitySet {
    let live_cards = hands.iter().flatten().copied().collect::<CardSet>();
    gotten
        .iter()
        .copied()
        .filter(|card| {
            *card != focus
                && live_cards.contains(card)
                && !convention_facts.fixed_cards().contains(card)
        })
        .fold(IdentitySet::default(), |claimed, card| {
            let giver_holds_card = hands[giver.index()].contains(&card);
            let identity = (!giver_holds_card)
                .then(|| historical.identity(card))
                .flatten()
                .or_else(|| {
                    let logical = IdentitySet::from_mask(clue_facts[card.index()].identity_mask());
                    (logical.len() == 1)
                        .then(|| logical.iter().next())
                        .flatten()
                })
                .or_else(|| {
                    let prior = clues.iter().rev().find(|prior| prior.focus == card)?;
                    // Good Touch reserves exact playing promises. A Save is
                    // protection, not an assertion that another clue cannot
                    // retain the same identity in a Play/Save superposition.
                    (prior.play_identities.len() == 1)
                        .then(|| prior.play_identities.iter().next())
                        .flatten()
                });
            identity.map_or(claimed, |identity| {
                claimed.union(IdentitySet::singleton(identity))
            })
        })
}
