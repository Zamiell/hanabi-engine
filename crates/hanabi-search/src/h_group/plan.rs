use hanabi_core::{Action, PlayerId};

use crate::{SymbolicLineOutcome, SymbolicStopReason};

/// An action predicted from one player's convention state. This is not an
/// authoritative replay event and cannot be applied without an explicit
/// prospective transition.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct ProjectedAction {
    pub(super) actor: PlayerId,
    pub(super) action: Action,
}

/// Public consequences of one projected action.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(super) struct ProjectedConsequences {
    pub(super) score_gain: u8,
    pub(super) discards: u8,
    pub(super) clues_spent: u8,
    pub(super) clues_gained: u8,
    pub(super) strikes: u8,
}

/// One node in the convention-forced plan. `depends_on` makes sequencing
/// explicit and leaves room for future conditional branches without treating
/// the projection as an authoritative list of actual actions.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct PlanStep {
    pub(super) projected: ProjectedAction,
    pub(super) depends_on: Option<usize>,
    pub(super) consequences: ProjectedConsequences,
}

/// Unresolved frontier at which deterministic projection stopped.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(super) enum PlanFrontier {
    Terminal,
    #[default]
    Choice,
    IdentityBranch,
    Limit,
    ProjectionUnavailable,
}

/// A partial-order-ready convention plan. The present projector emits a
/// single dependency chain; representing that chain as nodes prevents actual
/// replay actions and observer-relative forecasts from sharing a type.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(super) struct ConditionalPlan {
    steps: Vec<PlanStep>,
    frontier: PlanFrontier,
}

impl ConditionalPlan {
    pub(super) fn push(&mut self, projected: ProjectedAction, consequences: ProjectedConsequences) {
        let depends_on = self.steps.len().checked_sub(1);
        self.steps.push(PlanStep {
            projected,
            depends_on,
            consequences,
        });
    }

    pub(super) fn len(&self) -> usize {
        self.steps.len()
    }

    pub(super) const fn stop_at(&mut self, frontier: PlanFrontier) {
        self.frontier = frontier;
    }

    pub(super) fn summarize(&self) -> SymbolicLineOutcome {
        let mut outcome = SymbolicLineOutcome {
            actions: u8::try_from(self.steps.len()).unwrap_or(u8::MAX),
            stop_reason: match self.frontier {
                PlanFrontier::Terminal => SymbolicStopReason::Terminal,
                PlanFrontier::Choice => SymbolicStopReason::Choice,
                PlanFrontier::IdentityBranch => SymbolicStopReason::UnknownIdentity,
                PlanFrontier::Limit => SymbolicStopReason::Limit,
                PlanFrontier::ProjectionUnavailable => SymbolicStopReason::ProjectionUnavailable,
            },
            ..SymbolicLineOutcome::default()
        };
        for step in &self.steps {
            outcome.score_gain = outcome
                .score_gain
                .saturating_add(step.consequences.score_gain);
            outcome.discards = outcome.discards.saturating_add(step.consequences.discards);
            outcome.clues_spent = outcome
                .clues_spent
                .saturating_add(step.consequences.clues_spent);
            outcome.clues_gained = outcome
                .clues_gained
                .saturating_add(step.consequences.clues_gained);
            outcome.strikes = outcome.strikes.saturating_add(step.consequences.strikes);
        }
        outcome
    }
}

#[cfg(test)]
mod tests {
    use hanabi_core::CardId;

    use super::*;

    #[test]
    fn projected_steps_record_dependencies_and_summarize_consequences() {
        let mut plan = ConditionalPlan::default();
        plan.push(
            ProjectedAction {
                actor: PlayerId::new(0),
                action: Action::Play(CardId::new(1)),
            },
            ProjectedConsequences {
                score_gain: 1,
                ..ProjectedConsequences::default()
            },
        );
        plan.push(
            ProjectedAction {
                actor: PlayerId::new(1),
                action: Action::Discard(CardId::new(5)),
            },
            ProjectedConsequences {
                discards: 1,
                clues_gained: 1,
                ..ProjectedConsequences::default()
            },
        );
        plan.stop_at(PlanFrontier::IdentityBranch);

        assert_eq!(plan.steps[0].depends_on, None);
        assert_eq!(plan.steps[1].depends_on, Some(0));
        assert_eq!(
            plan.summarize(),
            SymbolicLineOutcome {
                actions: 2,
                score_gain: 1,
                discards: 1,
                clues_gained: 1,
                stop_reason: SymbolicStopReason::UnknownIdentity,
                ..SymbolicLineOutcome::default()
            }
        );
    }
}
