use std::sync::Arc;

use hanabi_core::{Card, CardId};

use super::{
    HGroupCardInference, HGroupIdentityStatus, HGroupPlayObligation, IdentitySet,
    LogicalDeductions, PromiseId,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
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
    SetFocused {
        card: CardId,
        focused: bool,
        source: KnowledgeSource,
    },
    SetSaved {
        card: CardId,
        saved: bool,
        source: KnowledgeSource,
    },
    SetFinessed {
        card: CardId,
        finessed: bool,
        source: KnowledgeSource,
    },
    SetPlayObligation {
        card: CardId,
        obligation: Option<HGroupPlayObligation>,
        source: KnowledgeSource,
    },
}

impl CardKnowledgeEffect {
    pub(super) const fn card(self) -> CardId {
        match self {
            Self::RestrictDomain { card, .. }
            | Self::ReplaceDomain { card, .. }
            | Self::SetPromise { card, .. }
            | Self::SetIdentityStatus { card, .. }
            | Self::SetFocused { card, .. }
            | Self::SetSaved { card, .. }
            | Self::SetFinessed { card, .. }
            | Self::SetPlayObligation { card, .. } => card,
        }
    }

    pub(super) const fn source(self) -> KnowledgeSource {
        match self {
            Self::RestrictDomain { source, .. }
            | Self::ReplaceDomain { source, .. }
            | Self::SetPromise { source, .. }
            | Self::SetIdentityStatus { source, .. }
            | Self::SetFocused { source, .. }
            | Self::SetSaved { source, .. }
            | Self::SetFinessed { source, .. }
            | Self::SetPlayObligation { source, .. } => source,
        }
    }
}

#[derive(Clone, Debug, Default)]
pub(super) struct ConventionKnowledge {
    effects: Arc<[CardKnowledgeEffect]>,
}

impl ConventionKnowledge {
    pub(super) fn new(effects: Vec<CardKnowledgeEffect>) -> Self {
        Self {
            effects: effects.into(),
        }
    }

    pub(super) fn effects(&self) -> &[CardKnowledgeEffect] {
        &self.effects
    }

    pub(super) fn project(&self, deductions: &LogicalDeductions) -> Vec<HGroupCardInference> {
        let mut cards = initial_card_inferences(deductions);
        CardKnowledgeReducer::apply(&mut cards, &self.effects);
        cards
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
                CardKnowledgeEffect::SetFocused { focused, .. } => card.focused = focused,
                CardKnowledgeEffect::SetSaved { saved, .. } => card.saved = saved,
                CardKnowledgeEffect::SetFinessed { finessed, .. } => {
                    card.finessed = finessed;
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
pub(super) fn effects_from_projection(
    deductions: &LogicalDeductions,
    projected: &[HGroupCardInference],
    source_for: impl Fn(CardId) -> KnowledgeSource,
) -> Vec<CardKnowledgeEffect> {
    let initial = initial_card_inferences(deductions);
    let mut effects = Vec::new();
    for after in projected {
        let Some(before) = initial.iter().find(|before| before.card == after.card) else {
            continue;
        };
        let source = source_for(after.card);
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
            effects.push(CardKnowledgeEffect::SetFocused {
                card: after.card,
                focused: after.focused,
                source,
            });
        }
        if before.saved != after.saved {
            effects.push(CardKnowledgeEffect::SetSaved {
                card: after.card,
                saved: after.saved,
                source,
            });
        }
        if before.finessed != after.finessed {
            effects.push(CardKnowledgeEffect::SetFinessed {
                card: after.card,
                finessed: after.finessed,
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
    }
    effects
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
