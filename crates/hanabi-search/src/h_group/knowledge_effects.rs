use std::collections::HashMap;
use std::hash::BuildHasherDefault;
use std::sync::Arc;

use hanabi_core::{Card, CardId};

use super::{
    CompactIdHasher, ConventionTransitionDelta, ConventionTransitionResult, HGroupCardInference,
    HGroupIdentityStatus, HGroupPlayObligation, IdentitySet, LogicalDeductions, PromiseId,
};

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(super) enum KnowledgeSource {
    Clue(u32),
    Reinterpretation(u32),
    Promise { id: PromiseId, turn: u32 },
    ForcedPlay(u32),
    ImplicitSave(u32),
    CurrentFocus(u32),
    ReplayClosure(u32),
}

impl KnowledgeSource {
    pub(super) const fn turn(self) -> u32 {
        match self {
            Self::Clue(turn)
            | Self::Reinterpretation(turn)
            | Self::Promise { turn, .. }
            | Self::ForcedPlay(turn)
            | Self::ImplicitSave(turn)
            | Self::CurrentFocus(turn)
            | Self::ReplayClosure(turn) => turn,
        }
    }
}

/// Typed owner-relative epistemic effect. Convention recognition emits these;
/// owner projection only reduces them and contains no move-recognition rules.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum CardKnowledgeEffect {
    RestrictDomain {
        card: CardId,
        allowed: IdentitySet,
        source: KnowledgeSource,
    },
    /// Explicit replacement is reserved for a Fix or retraction. Ordinary
    /// convention deductions must use `RestrictDomain`.
    ReplaceDomain {
        card: CardId,
        identities: IdentitySet,
        source: KnowledgeSource,
    },
    SetPromise {
        card: CardId,
        identity: Option<Card>,
        source: KnowledgeSource,
    },
    SetIdentityStatus {
        card: CardId,
        status: HGroupIdentityStatus,
        source: KnowledgeSource,
    },
    SetFact {
        card: CardId,
        fact: OwnerKnowledgeFact,
        change: KnowledgeFactChange,
        source: KnowledgeSource,
    },
    SetPlayObligation {
        card: CardId,
        obligation: Option<HGroupPlayObligation>,
        source: KnowledgeSource,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum OwnerKnowledgeFact {
    /// Event-local annotation for the latest clue only.
    Focus,
    /// Persistent protection established by a Save interpretation.
    Save,
    /// Membership in a live deterministic Finesse chain.
    Finesse,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum KnowledgeFactChange {
    Added,
    Removed,
}

impl CardKnowledgeEffect {
    pub(super) const fn card(self) -> CardId {
        match self {
            Self::RestrictDomain { card, .. }
            | Self::ReplaceDomain { card, .. }
            | Self::SetPromise { card, .. }
            | Self::SetIdentityStatus { card, .. }
            | Self::SetFact { card, .. }
            | Self::SetPlayObligation { card, .. } => card,
        }
    }

    pub(super) const fn source(self) -> KnowledgeSource {
        match self {
            Self::RestrictDomain { source, .. }
            | Self::ReplaceDomain { source, .. }
            | Self::SetPromise { source, .. }
            | Self::SetIdentityStatus { source, .. }
            | Self::SetFact { source, .. }
            | Self::SetPlayObligation { source, .. } => source,
        }
    }
}

#[derive(Clone, Debug, Default)]
pub(super) struct ConventionKnowledge {
    effects: Arc<[CardKnowledgeEffect]>,
    /// Source index for explanation, retraction, and transition validation.
    /// Projection deliberately consumes the ordered program; consumers that
    /// need provenance must not reverse-engineer it from the materialized
    /// card note.
    by_card: Arc<HashMap<CardId, Vec<usize>, BuildHasherDefault<CompactIdHasher>>>,
}

impl ConventionKnowledge {
    pub(super) fn new(effects: Vec<CardKnowledgeEffect>) -> Self {
        let mut by_card = HashMap::default();
        for (index, effect) in effects.iter().enumerate() {
            by_card
                .entry(effect.card())
                .or_insert_with(Vec::new)
                .push(index);
        }
        Self {
            effects: effects.into(),
            by_card: Arc::new(by_card),
        }
    }

    pub(super) fn effects(&self) -> &[CardKnowledgeEffect] {
        &self.effects
    }

    pub(super) fn effects_for(&self, card: CardId) -> impl Iterator<Item = &CardKnowledgeEffect> {
        self.by_card
            .get(&card)
            .into_iter()
            .flatten()
            .filter_map(|index| self.effects.get(*index))
    }

    pub(super) fn project(&self, deductions: &LogicalDeductions) -> Vec<HGroupCardInference> {
        let mut cards = initial_card_inferences(deductions);
        CardKnowledgeReducer::apply(&mut cards, &self.effects);
        cards
    }

    /// Partitions the canonical knowledge program by its causal event turn.
    /// The replay orchestrator delegates this operation instead of guessing a
    /// source for a final materialized card note.
    pub(super) fn attach_to_transitions(&self, transitions: &mut Vec<ConventionTransitionResult>) {
        for effect in self.effects.iter().copied() {
            let turn = effect.source().turn();
            if let Some(transition) = transitions
                .iter_mut()
                .find(|transition| transition.turn == turn)
            {
                transition.delta.knowledge_changes.push(effect);
            } else {
                transitions.push(ConventionTransitionResult {
                    turn,
                    proposals: Vec::new(),
                    delta: ConventionTransitionDelta {
                        card_changes: Vec::new(),
                        knowledge_changes: vec![effect],
                    },
                });
            }
        }
        transitions.sort_by_key(|transition| transition.turn);
    }
}

pub(super) struct CardKnowledgeReducer;

impl CardKnowledgeReducer {
    pub(super) fn apply(cards: &mut [HGroupCardInference], effects: &[CardKnowledgeEffect]) {
        for effect in effects {
            let Some(card) = cards.iter_mut().find(|card| card.card == effect.card()) else {
                continue;
            };
            match *effect {
                CardKnowledgeEffect::RestrictDomain { allowed, .. } => {
                    let narrowed = card.identities.intersection(allowed);
                    if !narrowed.is_empty() {
                        card.identities = narrowed;
                    }
                }
                CardKnowledgeEffect::ReplaceDomain { identities, .. } => {
                    if !identities.is_empty() {
                        card.identities = identities;
                    }
                }
                CardKnowledgeEffect::SetPromise { identity, .. } => {
                    card.promised_identity = identity;
                }
                CardKnowledgeEffect::SetIdentityStatus { status, .. } => {
                    card.identity_status = status;
                }
                CardKnowledgeEffect::SetFact { fact, change, .. } => {
                    let active = change == KnowledgeFactChange::Added;
                    match fact {
                        OwnerKnowledgeFact::Focus => card.focused = active,
                        OwnerKnowledgeFact::Save => card.saved = active,
                        OwnerKnowledgeFact::Finesse => card.finessed = active,
                    }
                }
                CardKnowledgeEffect::SetPlayObligation { obligation, .. } => {
                    card.play_obligation = obligation;
                }
            }
        }
    }
}

pub(super) fn initial_card_inferences(deductions: &LogicalDeductions) -> Vec<HGroupCardInference> {
    let view = deductions.view();
    view.hands[view.observer.index()]
        .iter()
        .filter_map(|card| {
            deductions
                .possible_identities(card.id)
                .map(|identities| HGroupCardInference {
                    card: card.id,
                    identities,
                    promised_identity: None,
                    identity_status: HGroupIdentityStatus::Settled,
                    focused: false,
                    saved: false,
                    finessed: false,
                    play_obligation: None,
                })
        })
        .collect()
}

/// Turns one completed semantic projection into a typed replay-owned program.
/// A non-subset is surfaced as `ReplaceDomain`, never hidden in a restriction.
pub(super) fn effects_between(
    before: HGroupCardInference,
    after: HGroupCardInference,
    source: KnowledgeSource,
) -> Vec<CardKnowledgeEffect> {
    let mut effects = Vec::new();
    if before.identities != after.identities {
        if after.identities.without(before.identities).is_empty() {
            effects.push(CardKnowledgeEffect::RestrictDomain {
                card: after.card,
                allowed: after.identities,
                source,
            });
        } else {
            effects.push(CardKnowledgeEffect::ReplaceDomain {
                card: after.card,
                identities: after.identities,
                source,
            });
        }
    }
    if before.promised_identity != after.promised_identity {
        effects.push(CardKnowledgeEffect::SetPromise {
            card: after.card,
            identity: after.promised_identity,
            source,
        });
    }
    if before.identity_status != after.identity_status {
        effects.push(CardKnowledgeEffect::SetIdentityStatus {
            card: after.card,
            status: after.identity_status,
            source,
        });
    }
    if before.focused != after.focused {
        effects.push(CardKnowledgeEffect::SetFact {
            card: after.card,
            fact: OwnerKnowledgeFact::Focus,
            change: fact_change(after.focused),
            source,
        });
    }
    if before.saved != after.saved {
        effects.push(CardKnowledgeEffect::SetFact {
            card: after.card,
            fact: OwnerKnowledgeFact::Save,
            change: fact_change(after.saved),
            source,
        });
    }
    if before.finessed != after.finessed {
        effects.push(CardKnowledgeEffect::SetFact {
            card: after.card,
            fact: OwnerKnowledgeFact::Finesse,
            change: fact_change(after.finessed),
            source,
        });
    }
    if before.play_obligation != after.play_obligation {
        effects.push(CardKnowledgeEffect::SetPlayObligation {
            card: after.card,
            obligation: after.play_obligation,
            source,
        });
    }
    effects
}

const fn fact_change(active: bool) -> KnowledgeFactChange {
    if active {
        KnowledgeFactChange::Added
    } else {
        KnowledgeFactChange::Removed
    }
}

#[cfg(test)]
mod tests {
    use hanabi_core::{Card, CardId, Rank, Suit};

    use super::*;

    #[test]
    fn reducer_keeps_restrictions_distinct_from_promises() {
        let card_id = CardId::new(1);
        let mut card = HGroupCardInference {
            card: card_id,
            identities: IdentitySet::all(),
            promised_identity: None,
            identity_status: HGroupIdentityStatus::Settled,
            focused: false,
            saved: false,
            finessed: false,
            play_obligation: None,
        };
        let yellow_one = Card::new(Suit::Yellow, Rank::One);
        CardKnowledgeReducer::apply(
            core::slice::from_mut(&mut card),
            &[
                CardKnowledgeEffect::RestrictDomain {
                    card: card_id,
                    allowed: IdentitySet::singleton(yellow_one),
                    source: KnowledgeSource::ReplayClosure(0),
                },
                CardKnowledgeEffect::SetPromise {
                    card: card_id,
                    identity: Some(yellow_one),
                    source: KnowledgeSource::ReplayClosure(0),
                },
            ],
        );
        assert_eq!(card.identities, IdentitySet::singleton(yellow_one));
        assert_eq!(card.promised_identity, Some(yellow_one));
    }
}
