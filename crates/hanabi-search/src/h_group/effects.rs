use super::HGroupSignal;

/// Typed output from convention recognition. Reducers own mutation and
/// idempotence; rule recognizers only describe what they observed.
#[derive(Clone, Debug)]
pub(super) enum ConventionEffect {
    RecordSignal(HGroupSignal),
}

#[derive(Clone, Debug, Default)]
pub(super) struct EffectBatch {
    effects: Vec<ConventionEffect>,
}

impl EffectBatch {
    pub(super) fn one(effect: ConventionEffect) -> Self {
        Self {
            effects: vec![effect],
        }
    }
}

pub(super) struct ConventionReducer;

impl ConventionReducer {
    pub(super) fn apply(batch: EffectBatch, signals: &mut Vec<HGroupSignal>) {
        for effect in batch.effects {
            match effect {
                ConventionEffect::RecordSignal(signal) => {
                    let duplicate = signals.iter().any(|existing| {
                        existing.turn == signal.turn
                            && existing.actor == signal.actor
                            && existing.kind == signal.kind
                            && existing.cards == signal.cards
                    });
                    if !duplicate {
                        signals.push(signal);
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
        let mut signals = Vec::new();
        ConventionReducer::apply(
            EffectBatch::one(ConventionEffect::RecordSignal(signal.clone())),
            &mut signals,
        );
        ConventionReducer::apply(
            EffectBatch::one(ConventionEffect::RecordSignal(signal)),
            &mut signals,
        );
        assert_eq!(signals.len(), 1);
    }
}
