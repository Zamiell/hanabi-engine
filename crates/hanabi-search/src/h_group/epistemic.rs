use hanabi_core::{Card, CardId, PlayerId};

use super::{HGroupInferences, IdentitySet, LogicalDeductions};

/// Why an observer is allowed to retain a card identity domain.
///
/// Keeping provenance next to the domain prevents downstream strategy code
/// from silently substituting simulator-visible truth for an owner's belief.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum BeliefProvenance {
    Visible,
    Logical,
    Convention,
}

/// Everything one observer is entitled to believe about one current card.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct CardBelief {
    pub(super) card: CardId,
    pub(super) owner: PlayerId,
    pub(super) observer: PlayerId,
    pub(super) identities: IdentitySet,
    pub(super) provenance: BeliefProvenance,
}

impl CardBelief {
    pub(super) fn known_identity(self) -> Option<Card> {
        (self.identities.len() == 1)
            .then(|| self.identities.iter().next())
            .flatten()
    }
}

/// Observer-relative knowledge used by convention strategy and outcome
/// comparison. This type deliberately contains no deck order or hidden-card
/// truth accessor.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct EpistemicState {
    observer: PlayerId,
    cards: Vec<CardBelief>,
}

impl EpistemicState {
    pub(super) fn from_analysis(
        deductions: &LogicalDeductions,
        inferred: &HGroupInferences,
    ) -> Self {
        let view = deductions.view();
        let observer = view.observer;
        let mut cards = Vec::with_capacity(view.hands.iter().map(Vec::len).sum());

        for (owner_index, hand) in view.hands.iter().enumerate() {
            let owner = PlayerId::new(
                u8::try_from(owner_index).expect("standard Hanabi has at most five players"),
            );
            for observed in hand {
                let convention = inferred.cards.iter().find(|note| note.card == observed.id);
                let (identities, provenance) = if owner == observer {
                    let logical = deductions
                        .possible_identities(observed.id)
                        .unwrap_or_default();
                    if let Some(note) = convention {
                        let narrowed = logical.intersection(note.identities);
                        (
                            if narrowed.is_empty() {
                                logical
                            } else {
                                narrowed
                            },
                            BeliefProvenance::Convention,
                        )
                    } else {
                        (logical, BeliefProvenance::Logical)
                    }
                } else if let Some(identity) = observed.identity {
                    (IdentitySet::singleton(identity), BeliefProvenance::Visible)
                } else {
                    // Nested projections may intentionally leave another
                    // player's card unresolved. Do not fill it from the
                    // source observer or simulator.
                    continue;
                };
                cards.push(CardBelief {
                    card: observed.id,
                    owner,
                    observer,
                    identities,
                    provenance,
                });
            }
        }
        Self { observer, cards }
    }

    pub(super) fn belief(&self, card: CardId) -> Option<CardBelief> {
        self.cards
            .iter()
            .copied()
            .find(|belief| belief.card == card)
    }

    pub(super) fn own_beliefs(&self) -> impl Iterator<Item = CardBelief> + '_ {
        self.cards
            .iter()
            .copied()
            .filter(|belief| belief.owner == self.observer)
    }
}
