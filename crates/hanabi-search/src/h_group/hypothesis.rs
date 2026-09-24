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

/// A hypothesis can depend on a response that has not happened yet. It cannot
/// be exported as evidence to the very decision on which that hypothesis rests.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AssumptionDependency {
    pub origin_turn: u32,
    pub focus: super::CardId,
    pub response_actor: PlayerId,
    pub decision_turn: u32,
}

impl AssumptionDependency {
    pub(super) fn would_prove_itself(self, observer: PlayerId, turn: u32) -> bool {
        self.response_actor == observer && turn <= self.decision_turn
    }
}

/// Compile the dependency at the semantic owner of provisional connections.
/// The projector consumes dependencies; it does not recognize convention kinds.
/// <https://hanabi.github.io/level-11/#bobs-truth-principle-part-1>
pub(super) fn identity_dependencies(
    source: &super::PlayerView,
    replay: &HGroupState,
    card: super::CardId,
) -> Vec<AssumptionDependency> {
    replay
        .pending_connections
        .iter()
        .filter_map(|connection| {
            if !connection.cards.contains(&card)
                || connection.kind != super::HGroupConnectionKind::Finesse
            {
                return None;
            }
            let origin = replay.pending_connections.provenance(connection.promise)?;
            let giver = source.history.iter().find_map(|entry| {
                if entry.turn != origin.created_turn {
                    return None;
                }
                match entry.event {
                    super::ObservedEvent::Clued { giver, .. } => Some(giver),
                    _ => None,
                }
            })?;
            let reactor = super::next_player(giver, source.hands.len());
            (connection.actor != reactor).then_some(AssumptionDependency {
                origin_turn: origin.created_turn,
                focus: origin.focus,
                response_actor: reactor,
                decision_turn: origin.created_turn.saturating_add(1),
            })
        })
        .collect()
}

#[cfg(test)]
mod dependency_tests {
    use super::*;

    #[test]
    fn only_the_unresolved_prerequisite_decision_is_blocked() {
        // Dependency evaluation is an algorithmic invariant. The reviewed
        // bluff regression in perspective.rs supplies the actual provenance.
        for actor in 0..4 {
            let dependency = AssumptionDependency {
                origin_turn: 18,
                focus: super::super::CardId::new(2),
                response_actor: PlayerId::new(actor),
                decision_turn: 19,
            };
            for observer in 0..4 {
                assert_eq!(
                    dependency.would_prove_itself(PlayerId::new(observer), 19),
                    actor == observer
                );
                assert!(!dependency.would_prove_itself(PlayerId::new(observer), 20));
            }
        }
    }
}
