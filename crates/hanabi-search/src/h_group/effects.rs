use core::ops::Deref;
use std::collections::HashMap;
use std::hash::BuildHasherDefault;

use hanabi_core::CardId;

use super::{
    CompactIdHasher, ConventionFacts, DeclinedAlternativeInference, HGroupMoveKind, HGroupSignal,
};

/// Indexed, append-only explanation history.
///
/// Production code uses these typed queries instead of repeatedly scanning
/// the audit log and accidentally confusing an old explanation with current
/// convention truth.
#[derive(Clone, Debug, Default)]
pub(super) struct SignalHistory {
    signals: Vec<HGroupSignal>,
    by_kind: HashMap<HGroupMoveKind, Vec<usize>, BuildHasherDefault<CompactIdHasher>>,
}

impl SignalHistory {
    fn push(&mut self, signal: HGroupSignal) {
        let index = self.signals.len();
        self.by_kind.entry(signal.kind).or_default().push(index);
        self.signals.push(signal);
    }

    pub(super) fn of_kind(
        &self,
        kind: HGroupMoveKind,
    ) -> impl DoubleEndedIterator<Item = &HGroupSignal> {
        self.by_kind
            .get(&kind)
            .into_iter()
            .flatten()
            .filter_map(|index| self.signals.get(*index))
    }

    pub(super) fn at_turn(
        &self,
        turn: u32,
        kind: HGroupMoveKind,
    ) -> impl Iterator<Item = &HGroupSignal> {
        self.by_kind
            .get(&kind)
            .into_iter()
            .flatten()
            .filter_map(|index| self.signals.get(*index))
            .filter(move |signal| signal.turn == turn)
    }

    pub(super) fn latest(&self, kind: HGroupMoveKind) -> Option<&HGroupSignal> {
        self.by_kind
            .get(&kind)
            .and_then(|indices| indices.last())
            .and_then(|index| self.signals.get(*index))
    }

    pub(super) fn has_at_turn(&self, turn: u32, kind: HGroupMoveKind) -> bool {
        self.at_turn(turn, kind).next().is_some()
    }

    pub(super) fn into_vec(self) -> Vec<HGroupSignal> {
        self.signals
    }
}

impl Deref for SignalHistory {
    type Target = [HGroupSignal];

    fn deref(&self) -> &Self::Target {
        &self.signals
    }
}

/// Typed output from convention recognition. Reducers own mutation and
/// idempotence; rule recognizers only describe what they observed.
#[derive(Clone, Debug)]
pub(super) enum ConventionEffect {
    /// Preserve the human-readable explanation/provenance log.
    RecordSignal(HGroupSignal),
    SetFixed {
        cards: Vec<CardId>,
        fixed: bool,
    },
    SetPriority {
        cards: Vec<CardId>,
        active: bool,
    },
    ClaimIdentity(HGroupSignal),
    RecordDeclinedAlternative(DeclinedAlternativeInference),
}

#[derive(Clone, Debug, Default)]
pub(super) struct EffectBatch {
    effects: Vec<ConventionEffect>,
}

impl EffectBatch {
    pub(super) fn recognized(signal: HGroupSignal) -> Self {
        let mut effects = Vec::with_capacity(3);
        match signal.kind {
            HGroupMoveKind::FixClue => effects.push(ConventionEffect::SetFixed {
                cards: signal.cards.clone(),
                fixed: true,
            }),
            HGroupMoveKind::PlayClue => effects.push(ConventionEffect::SetFixed {
                cards: signal.cards.clone(),
                fixed: false,
            }),
            HGroupMoveKind::Priority => effects.push(ConventionEffect::SetPriority {
                cards: signal.cards.clone(),
                active: true,
            }),
            HGroupMoveKind::Retraction => effects.push(ConventionEffect::SetPriority {
                cards: signal.cards.clone(),
                active: false,
            }),
            _ => {}
        }
        if signal.identity.is_some() {
            effects.push(ConventionEffect::ClaimIdentity(signal.clone()));
        }
        effects.push(ConventionEffect::RecordSignal(signal));
        Self { effects }
    }

    pub(super) fn declined_alternative(inference: DeclinedAlternativeInference) -> Self {
        Self {
            effects: vec![ConventionEffect::RecordDeclinedAlternative(inference)],
        }
    }
}

/// Append-only explanations paired with incrementally maintained current
/// convention truth. Consumers may inspect the explanation slice, but only
/// the reducer can append to it or change materialized facts.
#[derive(Clone, Debug, Default)]
pub(super) struct ConventionJournal {
    signals: SignalHistory,
    facts: ConventionFacts,
}

impl ConventionJournal {
    pub(super) const fn facts(&self) -> &ConventionFacts {
        &self.facts
    }

    pub(super) fn into_parts(self) -> (SignalHistory, ConventionFacts) {
        (self.signals, self.facts)
    }

    pub(super) fn len(&self) -> usize {
        self.signals.len()
    }

    /// Iterates provenance when a rule genuinely needs a cross-kind causal
    /// history query. Current-state consumers must use `facts`; same-kind and
    /// same-turn recognizers should use the indexed methods below.
    pub(super) fn iter(&self) -> core::slice::Iter<'_, HGroupSignal> {
        self.signals.iter()
    }

    pub(super) fn of_kind(
        &self,
        kind: HGroupMoveKind,
    ) -> impl DoubleEndedIterator<Item = &HGroupSignal> {
        self.signals.of_kind(kind)
    }

    pub(super) fn latest(&self, kind: HGroupMoveKind) -> Option<&HGroupSignal> {
        self.signals.latest(kind)
    }

    pub(super) fn has_at_turn(&self, turn: u32, kind: HGroupMoveKind) -> bool {
        self.signals.has_at_turn(turn, kind)
    }
}

pub(super) struct ConventionReducer;

impl ConventionReducer {
    pub(super) fn apply(batch: EffectBatch, journal: &mut ConventionJournal) {
        for effect in batch.effects {
            match effect {
                ConventionEffect::SetFixed { cards, fixed } => {
                    journal.facts.set_fixed(&cards, fixed);
                }
                ConventionEffect::SetPriority { cards, active } => {
                    journal.facts.set_priority(&cards, active);
                }
                ConventionEffect::ClaimIdentity(signal) => {
                    journal.facts.apply_identity_effect(&signal);
                }
                ConventionEffect::RecordDeclinedAlternative(inference) => {
                    journal.facts.add_declined_alternative(inference);
                }
                ConventionEffect::RecordSignal(signal) => {
                    let duplicate =
                        journal
                            .signals
                            .at_turn(signal.turn, signal.kind)
                            .any(|existing| {
                                existing.turn == signal.turn
                                    && existing.actor == signal.actor
                                    && existing.kind == signal.kind
                                    && existing.cards == signal.cards
                            });
                    if !duplicate {
                        journal.signals.push(signal);
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use hanabi_core::{CardId, PlayerId};

    use super::*;
    use crate::h_group::HGroupMoveKind;

    #[test]
    fn reducer_is_idempotent_for_the_same_recognized_effect() {
        let signal = HGroupSignal {
            turn: 4,
            actor: PlayerId::new(0),
            target: None,
            kind: HGroupMoveKind::Priority,
            cards: vec![CardId::new(2)],
            identity: None,
        };
        let mut journal = ConventionJournal::default();
        ConventionReducer::apply(EffectBatch::recognized(signal.clone()), &mut journal);
        ConventionReducer::apply(EffectBatch::recognized(signal), &mut journal);
        assert_eq!(journal.len(), 1);
        assert!(journal.facts().active_priority().contains(&CardId::new(2)));
    }
}
