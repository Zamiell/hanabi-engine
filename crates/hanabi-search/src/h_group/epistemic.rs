use hanabi_core::{Card, CardId, PlayerId};

use super::{
    ConventionKnowledge, HGroupIdentityStatus, HGroupInferences, HGroupPlayObligation, IdentitySet,
    KnowledgeSource, LogicalDeductions, is_convention_trash, is_eventually_useful,
};

/// Canonical owner-relative read model used by diagnostics and regressions.
///
/// It keeps the logical base, effective convention result, provenance, and
/// derived action classifications separate. Serializers must consume this
/// model instead of independently reimplementing convention semantics.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct OwnerCardKnowledge {
    pub(super) card: CardId,
    pub(super) logical_identities: IdentitySet,
    pub(super) convention_identities: IdentitySet,
    pub(super) sources: Vec<KnowledgeSource>,
    pub(super) identity_status: HGroupIdentityStatus,
    pub(super) facts: OwnerConventionFacts,
    pub(super) classifications: OwnerCardClassifications,
    pub(super) play_obligation: Option<HGroupPlayObligation>,
    pub(super) position: OwnerCardPosition,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct OwnerConventionFacts {
    pub(super) focused: bool,
    pub(super) saved: bool,
    pub(super) finessed: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct OwnerCardClassifications {
    pub(super) playable: bool,
    pub(super) convention_only_trash: bool,
    pub(super) discard_now: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct OwnerCardPosition {
    pub(super) chop: bool,
    pub(super) chop_moved: bool,
}

pub(super) fn owner_knowledge_read_model(
    deductions: &LogicalDeductions,
    knowledge: &ConventionKnowledge,
    inferred: &HGroupInferences,
) -> Vec<OwnerCardKnowledge> {
    let view = deductions.view();
    let gotten = inferred.gotten();
    view.hands[view.observer.index()]
        .iter()
        .filter_map(|observed| {
            let logical = deductions.possible_identities(observed.id)?;
            let note = inferred
                .cards
                .iter()
                .find(|note| note.card == observed.id)?;
            let convention = note
                .promised_identity
                .map_or(note.identities, IdentitySet::singleton);
            let logically_trash = !logical.is_empty()
                && logical
                    .iter()
                    .all(|identity| !is_eventually_useful(view, identity));
            let convention_trash = note.identity_status != HGroupIdentityStatus::Provisional
                && !note.identities.is_empty()
                && note
                    .identities
                    .iter()
                    .all(|identity| is_convention_trash(view, identity, &gotten, &inferred.cards));
            let playable = inferred.playable_now.contains(&observed.id)
                || inferred
                    .connection
                    .is_some_and(|connection| connection.card == observed.id);
            let sources = knowledge
                .effects_for(observed.id)
                .map(|effect| effect.source())
                .collect();
            Some(OwnerCardKnowledge {
                card: observed.id,
                logical_identities: logical,
                convention_identities: convention,
                sources,
                identity_status: note.identity_status,
                facts: OwnerConventionFacts {
                    focused: note.focused,
                    saved: note.saved,
                    finessed: note.finessed,
                },
                classifications: OwnerCardClassifications {
                    playable,
                    convention_only_trash: convention_trash && !logically_trash,
                    discard_now: inferred.discard_now.contains(&observed.id),
                },
                play_obligation: note.play_obligation,
                position: OwnerCardPosition {
                    chop: inferred.chops[view.observer.index()] == Some(observed.id),
                    chop_moved: inferred.chop_moved.contains(&observed.id),
                },
            })
        })
        .collect::<Vec<_>>()
}

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
