use hanabi_core::PlayerId;

use super::HGroupState;

/// Why one complete convention interpretation exists.
///
/// Alternatives own their connections, promises, and identity claims as one
/// correlated state. They must never be merged card-by-card.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum InterpretationSource {
    Ordinary,
    BlindReverseEmpathy,
}

/// One correlated interpretation of the public history.
pub(super) struct InterpretationHypothesis {
    source: InterpretationSource,
    state: HGroupState,
}

impl InterpretationHypothesis {
    pub(super) const fn new(source: InterpretationSource, state: HGroupState) -> Self {
        Self { source, state }
    }

    fn gives_actor_a_live_connection(&self, actor: PlayerId) -> bool {
        self.state.pending_connections.actor_has_active(actor)
    }
}

/// Explicit set of mutually exclusive whole-history interpretations.
///
/// The current engine resolves empathy only when it creates the acting
/// player's otherwise-missing obligation. Representing both compilations as
/// hypotheses makes that precedence auditable and prevents future code from
/// accidentally combining facts from incompatible interpretations.
pub(super) struct InterpretationHypotheses {
    alternatives: Vec<InterpretationHypothesis>,
}

impl InterpretationHypotheses {
    pub(super) fn ordinary(state: HGroupState) -> Self {
        Self {
            alternatives: vec![InterpretationHypothesis::new(
                InterpretationSource::Ordinary,
                state,
            )],
        }
    }

    pub(super) fn ordinary_gives_actor_a_live_connection(&self, actor: PlayerId) -> bool {
        self.alternatives
            .first()
            .is_some_and(|hypothesis| hypothesis.gives_actor_a_live_connection(actor))
    }

    pub(super) fn add(&mut self, source: InterpretationSource, state: HGroupState) {
        self.alternatives
            .push(InterpretationHypothesis::new(source, state));
    }

    pub(super) fn resolve_for_actor(mut self, actor: PlayerId) -> HGroupState {
        let selected = self
            .alternatives
            .iter()
            .position(|hypothesis| {
                hypothesis.source == InterpretationSource::BlindReverseEmpathy
                    && hypothesis.gives_actor_a_live_connection(actor)
            })
            .unwrap_or(0);
        self.alternatives.swap_remove(selected).state
    }
}
