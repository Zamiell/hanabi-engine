use hanabi_core::{Action, Card, CardId, MAX_CLUE_TOKENS, PlayerId};

use crate::{SymbolicLineOutcome, SymbolicStopReason};

/// An action predicted from one player's convention state. This is not an
/// authoritative replay event and cannot be applied without an explicit
/// prospective transition.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ProjectedAction {
    pub actor: PlayerId,
    pub action: Action,
}

/// Public consequences of one projected action.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ProjectedConsequences {
    pub score_gain: u8,
    pub discards: u8,
    pub clues_spent: u8,
    pub clues_gained: u8,
    pub strikes: u8,
    pub save_principle_violation: Option<SavePrincipleViolation>,
    /// A still-needed identity discarded without a known replacement. This
    /// records bottom-deck risk, not a guaranteed loss or an illegal action.
    pub bottom_deck_risk: Option<Card>,
}

/// Important-card loss established in the projection's source perspective.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SavePrincipleViolation {
    CriticalCard,
    UniqueTwo,
    UniquePlayable,
    UniqueDelayedPlayable,
}

/// One node in a projected plan. `depends_on` makes sequencing explicit;
/// conditional continuations are not authoritative replay actions.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PlanStep {
    /// Zero-based engine turn; UI diagnostics render turn + 1.
    pub turn: u32,
    pub projected: ProjectedAction,
    pub depends_on: Option<usize>,
    pub consequences: ProjectedConsequences,
}

/// Unresolved frontier at which deterministic projection stopped.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum PlanFrontier {
    Terminal,
    #[default]
    Choice,
    IdentityBranch,
    InterpretationBranch,
    Limit,
    ProjectionUnavailable,
}

/// A possibility supported by the observer's domain, never a convention fact.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct HiddenCardCondition {
    pub observer: PlayerId,
    pub owner: PlayerId,
    pub card: CardId,
    pub identity: Card,
}

/// A checked alternative at one scheduling window. Other alternatives are not
/// assumed true together; the ordinary projected continuation remains separate.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ConditionalAlternative {
    pub after_step: usize,
    pub condition: HiddenCardCondition,
    pub follow_up: PlanStep,
    pub latest_turn: u32,
    pub resources: ResourceSchedule,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TokenTransition {
    pub turn: u32,
    pub before: u8,
    pub spent: u8,
    pub gained: u8,
    pub after: u8,
}

/// Prefix funding with the real token cap. Future refunds cannot pay for an
/// earlier clue. Conditional alternatives keep independent ledgers.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ResourceSchedule {
    pub initial_tokens: u8,
    pub tokens: u8,
    pub transitions: Vec<TokenTransition>,
    pub unfunded_turn: Option<u32>,
}

impl ResourceSchedule {
    pub(crate) fn discard_then_clue(tokens: u8, turn: u32) -> Option<Self> {
        if tokens >= MAX_CLUE_TOKENS {
            return None;
        }
        let mut schedule = Self::new(tokens);
        (schedule.apply(turn, 0, 1) && schedule.apply(turn + 1, 1, 0)).then_some(schedule)
    }

    pub(crate) fn new(tokens: u8) -> Self {
        Self {
            initial_tokens: tokens,
            tokens,
            ..Self::default()
        }
    }

    pub(crate) fn apply(&mut self, turn: u32, spent: u8, gained: u8) -> bool {
        if self.unfunded_turn.is_some() || spent > self.tokens {
            self.unfunded_turn.get_or_insert(turn);
            return false;
        }
        let before = self.tokens;
        self.tokens = (self.tokens - spent)
            .saturating_add(gained)
            .min(MAX_CLUE_TOKENS);
        self.transitions.push(TokenTransition {
            turn,
            before,
            spent,
            gained,
            after: self.tokens,
        });
        true
    }

    pub(crate) fn reserve(critical_saves: u8, mandatory_clues: u8, consecutive_saves: bool) -> u8 {
        1_u8.saturating_add(critical_saves)
            .saturating_add(mandatory_clues)
            .saturating_add(if consecutive_saves { 2 } else { 0 })
    }

    pub(crate) fn funds_final_fives(tokens: u8, secured: usize, remaining: usize) -> bool {
        let mut schedule = Self::new(tokens);
        // The first clue secures all these plays; later clocks here are
        // dependency offsets, not predictions of absolute game turns.
        schedule.apply(0, 1, 0)
            && schedule.apply(1, 0, u8::try_from(secured).unwrap_or(u8::MAX))
            && schedule.tokens as usize >= remaining
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ProjectionEvidence {
    /// Promised identities assumed while modeling another player's view.
    /// These are not observed identities or exhaustive-world proofs.
    pub assumptions: Vec<PerspectiveAssumption>,
    pub dependencies: Vec<super::DependencyAssessment>,
    pub steps: Vec<PlanStep>,
    pub alternatives: Vec<ConditionalAlternative>,
    pub frontier: PlanFrontier,
    /// The next selected discard has an unknown identity and has not executed.
    /// A false BDR assessment excludes bottom-deck risk, not last-copy risk.
    pub unresolved_discard: Option<UnresolvedDiscard>,
    pub resources: ResourceSchedule,
    pub windows: Vec<super::ActionWindow>,
    /// Equal elapsed-turn evaluations; the full continuation is retained.
    pub checkpoints: Vec<crate::RotationCheckpoint>,
    /// Exhaustive clue-touch alternatives. These are mutually exclusive,
    /// not extra actions appended to the unconditional prefix.
    pub clue_branches: Vec<ClueTouchBranch>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct UnresolvedDiscard {
    pub card: CardId,
    pub bottom_deck_risk: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ClueTouchBranch {
    pub turn: u32,
    pub touched: Vec<CardId>,
    pub outcome: SymbolicLineOutcome,
    pub continuation: ProjectionEvidence,
}

impl ProjectionEvidence {
    /// Unbranched risk up to an assessed discard frontier. If a forecast stops
    /// on an unresolved clue/play, absence of a recorded loss is not evidence
    /// of safety. A zero-risk unknown discard is a usable local alternative,
    /// but does not certify anything after that discard or its unknown draw.
    pub(crate) fn forecast_discard_risk(&self) -> Option<usize> {
        let known = self
            .steps
            .iter()
            .filter(|step| step.consequences.bottom_deck_risk.is_some())
            .count();
        match self.unresolved_discard {
            Some(discard) => Some(known + usize::from(discard.bottom_deck_risk)),
            None if known > 0 || self.frontier == PlanFrontier::Terminal => Some(known),
            None => None,
        }
    }

    #[cfg(test)]
    pub(crate) fn maximum_bottom_deck_risks(&self) -> usize {
        let prefix = self
            .steps
            .iter()
            .filter(|step| step.consequences.bottom_deck_risk.is_some())
            .count();
        self.clue_branches
            .iter()
            .map(|branch| branch.continuation.maximum_bottom_deck_risks())
            .max()
            .unwrap_or(0)
            .max(prefix)
    }

    /// Compare strategic risks within a shared lookahead horizon. A longer
    /// forecast reaching a loss is not evidence that a shorter, unfinished
    /// forecast avoids it. Full tail evidence remains available to diagnostics;
    /// immediate Save Principle violations are assessed independently.
    pub(crate) fn bottom_deck_risks_at(&self, horizon: usize) -> usize {
        let prefix = self
            .steps
            .iter()
            .take(horizon)
            .filter(|step| step.consequences.bottom_deck_risk.is_some())
            .count()
            // The unexecuted discard would be the next action, not the last
            // completed action in this prefix.
            + usize::from(self.unresolved_discard.is_some_and(|discard| discard.bottom_deck_risk)
                && self.steps.len() < horizon);
        self.clue_branches
            .iter()
            .map(|branch| branch.continuation.bottom_deck_risks_at(horizon))
            .max()
            .unwrap_or(0)
            .max(prefix)
    }

    /// Includes every modeled tail, even when summaries trim to a shared
    /// horizon. Conditional losses are hazards, not guaranteed outcomes.
    pub(crate) fn maximum_save_violations(&self) -> usize {
        self.save_violations_at(usize::MAX)
    }

    /// Losses observed within a comparable prefix, not in a longer forecast's
    /// speculative tail. The full diagnostic count remains available above.
    pub(crate) fn save_violations_at(&self, horizon: usize) -> usize {
        let prefix = self
            .steps
            .iter()
            .take(horizon)
            .filter(|step| step.consequences.save_principle_violation.is_some())
            .count();
        self.clue_branches
            .iter()
            .map(|branch| branch.continuation.save_violations_at(horizon))
            .max()
            .unwrap_or(0)
            .max(prefix)
    }
    pub(crate) fn common_horizon(&self) -> u8 {
        if self.clue_branches.is_empty() {
            self.checkpoints
                .last()
                .map_or(0, |checkpoint| checkpoint.actions)
        } else {
            self.clue_branches
                .iter()
                .map(|branch| branch.continuation.common_horizon())
                .min()
                .unwrap_or(0)
        }
    }

    pub(crate) fn checkpoints_at(&self, actions: u8) -> Vec<crate::RotationCheckpoint> {
        if self.clue_branches.is_empty() {
            self.checkpoints
                .iter()
                .filter(|checkpoint| checkpoint.actions == actions)
                .copied()
                .collect()
        } else {
            self.clue_branches
                .iter()
                .flat_map(|branch| branch.continuation.checkpoints_at(actions))
                .collect()
        }
    }

    /// Worst modeled branch, not a claim that a conditional strike occurs in
    /// every world. A common-horizon comparison must never hide this tail.
    pub(crate) fn maximum_strikes(&self) -> u8 {
        self.clue_branches
            .iter()
            .map(|branch| branch.continuation.maximum_strikes())
            .max()
            .unwrap_or_else(|| {
                self.steps.iter().fold(0_u8, |sum, step| {
                    sum.saturating_add(step.consequences.strikes)
                })
            })
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PerspectiveAssumption {
    pub turn: u32,
    pub source_observer: PlayerId,
    pub modeled_observer: PlayerId,
    pub card: CardId,
    pub identity: Card,
}

/// A convention plan with dependency chains and conditional clue branches.
/// Actual replay actions and observer-relative forecasts do not share a type.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(super) struct ConditionalPlan {
    evidence: ProjectionEvidence,
    position_value: Option<crate::ProjectedPositionValue>,
    first_rotation: Option<crate::RotationCheckpoint>,
}

impl ConditionalPlan {
    pub(super) fn record_assumptions(&mut self, assumptions: &[PerspectiveAssumption]) {
        for assumption in assumptions {
            if !self.evidence.assumptions.contains(assumption) {
                self.evidence.assumptions.push(*assumption);
            }
        }
    }
    pub(super) fn assess_dependencies(
        &mut self,
        dependencies: Vec<super::DependencyAssessment>,
    ) -> bool {
        let supported = dependencies
            .iter()
            .all(|assessment| assessment.status == super::DependencyStatus::Supported);
        self.evidence.dependencies.extend(dependencies);
        supported
    }
    pub(super) fn record_window(&mut self, window: super::ActionWindow) {
        self.evidence.windows.push(window);
    }
    pub(super) fn new(tokens: u8) -> Self {
        Self {
            evidence: ProjectionEvidence {
                resources: ResourceSchedule::new(tokens),
                ..ProjectionEvidence::default()
            },
            ..Self::default()
        }
    }

    pub(super) fn into_evidence(self) -> ProjectionEvidence {
        self.evidence
    }

    pub(super) fn add_alternative(&mut self, mut alternative: ConditionalAlternative) {
        alternative.after_step = self.evidence.steps.len();
        alternative.follow_up.depends_on = Some(alternative.after_step);
        self.evidence.alternatives.push(alternative);
    }

    pub(super) fn assess(&mut self, value: Option<crate::ProjectedPositionValue>) {
        self.position_value = value;
    }
    pub(super) fn record_rotation(&mut self, value: Option<crate::ProjectedPositionValue>) {
        let summary = self.summarize();
        self.first_rotation = value.map(|value| crate::RotationCheckpoint {
            actions: summary.actions,
            discards: summary.discards,
            value,
        });
    }
    pub(super) fn record_checkpoint(&mut self, value: Option<crate::ProjectedPositionValue>) {
        let summary = self.summarize();
        if let Some(value) = value {
            self.evidence.checkpoints.push(crate::RotationCheckpoint {
                actions: summary.actions,
                discards: summary.discards,
                value,
            });
        }
    }
    pub(super) fn add_clue_branch(&mut self, turn: u32, touched: Vec<CardId>, plan: Self) {
        self.evidence.clue_branches.push(ClueTouchBranch {
            turn,
            touched,
            outcome: plan.summarize(),
            continuation: plan.into_evidence(),
        });
    }
    pub(super) fn push(
        &mut self,
        turn: u32,
        projected: ProjectedAction,
        consequences: ProjectedConsequences,
    ) {
        let depends_on = self.evidence.steps.len().checked_sub(1);
        assert!(
            self.evidence.resources.apply(
                turn,
                consequences.clues_spent,
                consequences.clues_gained
            ),
            "a projected action must be funded before it is applied"
        );
        self.evidence.steps.push(PlanStep {
            turn,
            projected,
            depends_on,
            consequences,
        });
    }

    pub(super) fn len(&self) -> usize {
        self.evidence.steps.len()
    }

    pub(super) const fn stop_at(&mut self, frontier: PlanFrontier) {
        self.evidence.frontier = frontier;
    }

    pub(super) fn record_unresolved_discard(&mut self, card: CardId, bottom_deck_risk: bool) {
        self.evidence.unresolved_discard = Some(UnresolvedDiscard {
            card,
            bottom_deck_risk,
        });
    }

    pub(super) fn summarize(&self) -> SymbolicLineOutcome {
        if !self.evidence.clue_branches.is_empty() {
            return self.summarize_branches();
        }
        let mut outcome = SymbolicLineOutcome {
            position_value: self.position_value,
            first_rotation: self.first_rotation,
            actions: u8::try_from(self.evidence.steps.len()).unwrap_or(u8::MAX),
            stop_reason: match self.evidence.frontier {
                PlanFrontier::Terminal => SymbolicStopReason::Terminal,
                PlanFrontier::Choice => SymbolicStopReason::Choice,
                PlanFrontier::IdentityBranch => SymbolicStopReason::UnknownIdentity,
                PlanFrontier::InterpretationBranch => SymbolicStopReason::UnknownInterpretation,
                PlanFrontier::Limit => SymbolicStopReason::Limit,
                PlanFrontier::ProjectionUnavailable => SymbolicStopReason::ProjectionUnavailable,
            },
            ..SymbolicLineOutcome::default()
        };
        for step in &self.evidence.steps {
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
        if let Some(value) = &mut outcome.position_value {
            // Presence is a tiebreak; mutually exclusive alternatives never
            // accumulate into supposedly guaranteed additional plays.
            value.conditional_successors = u8::from(!self.evidence.alternatives.is_empty());
        }
        outcome
    }

    fn summarize_branches(&self) -> SymbolicLineOutcome {
        let branches = &self.evidence.clue_branches;
        let mut shared = self.clone();
        shared.evidence.clue_branches.clear();
        let mut steps = branches[0].continuation.steps.clone();
        for branch in &branches[1..] {
            let common = steps
                .iter()
                .zip(&branch.continuation.steps)
                .take_while(|(a, b)| a == b)
                .count();
            steps.truncate(common);
        }
        shared.evidence.steps = steps;
        let actions = u8::try_from(shared.len()).unwrap_or(u8::MAX);
        // Different branch states must not be advertised as one exact state.
        // Keep an endpoint only when all branches agree on its full value.
        let values = branches
            .iter()
            .map(|branch| {
                branch
                    .continuation
                    .checkpoints
                    .iter()
                    .find(|checkpoint| checkpoint.actions == actions)
                    .map(|checkpoint| checkpoint.value)
            })
            .collect::<Vec<_>>();
        shared.position_value =
            values[0].filter(|value| values.iter().all(|other| *other == Some(*value)));
        shared.first_rotation = None;
        shared.evidence.frontier = if branches.iter().all(|branch| {
            branch.outcome.stop_reason == SymbolicStopReason::Terminal
                && branch.outcome.actions == actions
        }) {
            PlanFrontier::Terminal
        } else {
            PlanFrontier::IdentityBranch
        };
        shared.summarize()
    }
}

#[cfg(test)]
mod tests {
    use hanabi_core::CardId;

    use super::*;

    #[test]
    fn unknown_discard_risk_is_not_a_loss_beyond_the_shared_horizon() {
        let mut unknown = ConditionalPlan::new(2);
        assert_eq!(unknown.evidence.forecast_discard_risk(), None);
        unknown.record_unresolved_discard(CardId::new(5), false);
        assert_eq!(unknown.evidence.forecast_discard_risk(), Some(0));
        let mut plan = ConditionalPlan::new(2);
        plan.push(
            0,
            ProjectedAction {
                actor: PlayerId::new(0),
                action: Action::Discard(CardId::new(4)),
            },
            ProjectedConsequences {
                bottom_deck_risk: Some(Card::new(
                    hanabi_core::Suit::Blue,
                    hanabi_core::Rank::Three,
                )),
                ..Default::default()
            },
        );
        plan.record_unresolved_discard(CardId::new(5), true);
        let evidence = plan.into_evidence();
        assert_eq!(evidence.maximum_bottom_deck_risks(), 1);
        assert_eq!(evidence.forecast_discard_risk(), Some(2));
        assert_eq!(
            evidence.bottom_deck_risks_at(0),
            0,
            "tail risk is retained but not charged before the comparison horizon"
        );
        assert_eq!(
            evidence.bottom_deck_risks_at(1),
            1,
            "the next unknown discard is outside a one-action prefix"
        );
        assert_eq!(
            evidence.bottom_deck_risks_at(2),
            2,
            "unknown next discard is a possible hazard only when reached"
        );
        let mut prefix = ConditionalPlan::new(2);
        prefix.add_clue_branch(
            0,
            vec![CardId::new(4)],
            ConditionalPlan {
                evidence,
                ..Default::default()
            },
        );
        prefix.add_clue_branch(0, vec![CardId::new(5)], ConditionalPlan::new(2));
        let evidence = prefix.into_evidence();
        assert_eq!(
            evidence.bottom_deck_risks_at(2),
            2,
            "mutually exclusive branches use a maximum, not a sum"
        );
        assert_eq!(
            evidence.forecast_discard_risk(),
            None,
            "a hypothetical clue outcome is not an unbranched forecast loss"
        );
    }

    #[test]
    fn shared_checkpoints_retain_conditional_tail_risk() {
        let mut prefix = ConditionalPlan::new(2);
        prefix.push(
            0,
            ProjectedAction {
                actor: PlayerId::new(0),
                action: Action::Clue {
                    target: PlayerId::new(1),
                    clue: hanabi_core::Clue::Rank(hanabi_core::Rank::Two),
                },
            },
            ProjectedConsequences {
                clues_spent: 1,
                ..Default::default()
            },
        );
        prefix.record_checkpoint(Some(crate::ProjectedPositionValue::default()));
        let safe = prefix.clone();
        let mut risky = prefix.clone();
        risky.push(
            1,
            ProjectedAction {
                actor: PlayerId::new(1),
                action: Action::Play(CardId::new(4)),
            },
            ProjectedConsequences {
                strikes: 1,
                save_principle_violation: Some(SavePrincipleViolation::UniqueDelayedPlayable),
                ..Default::default()
            },
        );
        risky.record_checkpoint(Some(crate::ProjectedPositionValue::default()));
        prefix.add_clue_branch(0, vec![CardId::new(4)], safe);
        prefix.add_clue_branch(0, vec![CardId::new(4), CardId::new(5)], risky);
        let evidence = prefix.into_evidence();
        assert_eq!(evidence.common_horizon(), 1);
        assert_eq!(evidence.checkpoints_at(1).len(), 2);
        assert_eq!(evidence.maximum_strikes(), 1);
        assert_eq!(evidence.maximum_save_violations(), 1);
        assert_eq!(evidence.save_violations_at(1), 0);
        assert_eq!(evidence.save_violations_at(2), 1);
    }

    #[test]
    fn resource_schedules_require_prefix_funding_and_cap_refunds() {
        let mut unfunded = ResourceSchedule::new(0);
        assert!(!unfunded.apply(4, 1, 1));
        assert!(!unfunded.apply(5, 0, 1));
        assert_eq!(unfunded.unfunded_turn, Some(4));
        assert!(unfunded.transitions.is_empty());
        let mut funded = ResourceSchedule::new(MAX_CLUE_TOKENS);
        assert!(funded.apply(4, 0, 1));
        assert_eq!(funded.tokens, MAX_CLUE_TOKENS);
        assert!(funded.apply(5, 1, 0));
        assert_eq!(funded.tokens, MAX_CLUE_TOKENS - 1);
        assert!(ResourceSchedule::discard_then_clue(MAX_CLUE_TOKENS, 4).is_none());
        assert_eq!(ResourceSchedule::discard_then_clue(0, 4).unwrap().tokens, 0);
        assert!(!ResourceSchedule::funds_final_fives(0, 2, 1));
        assert!(ResourceSchedule::funds_final_fives(1, 2, 2));
    }

    #[test]
    fn incompatible_alternatives_do_not_accumulate_guaranteed_progress() {
        let mut plan = ConditionalPlan::new(1);
        plan.assess(Some(crate::ProjectedPositionValue::default()));
        for suit in [hanabi_core::Suit::Red, hanabi_core::Suit::Blue] {
            plan.add_alternative(ConditionalAlternative {
                after_step: 0,
                condition: HiddenCardCondition {
                    observer: PlayerId::new(0),
                    owner: PlayerId::new(0),
                    card: CardId::new(1),
                    identity: Card::new(suit, hanabi_core::Rank::Two),
                },
                follow_up: PlanStep {
                    turn: 1,
                    projected: ProjectedAction {
                        actor: PlayerId::new(1),
                        action: Action::Clue {
                            target: PlayerId::new(0),
                            clue: hanabi_core::Clue::Suit(suit),
                        },
                    },
                    depends_on: None,
                    consequences: ProjectedConsequences {
                        clues_spent: 1,
                        ..Default::default()
                    },
                },
                latest_turn: 1,
                resources: ResourceSchedule::new(1),
            });
        }
        plan.push(
            0,
            ProjectedAction {
                actor: PlayerId::new(0),
                action: Action::Play(CardId::new(2)),
            },
            ProjectedConsequences {
                score_gain: 1,
                ..Default::default()
            },
        );
        let summary = plan.summarize();
        assert_eq!(summary.actions, 1);
        assert_eq!(summary.score_gain, 1);
        assert_eq!(summary.clues_spent, 0);
        assert_eq!(summary.position_value.unwrap().conditional_successors, 1);
        assert_eq!(plan.evidence.resources.tokens, 1);
        assert!(
            plan.evidence
                .alternatives
                .iter()
                .all(|branch| branch.follow_up.depends_on == Some(0))
        );
    }

    #[test]
    fn projected_steps_record_dependencies_and_summarize_consequences() {
        let mut plan = ConditionalPlan::default();
        plan.push(
            0,
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
            1,
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

        assert_eq!(plan.evidence.steps[0].depends_on, None);
        assert_eq!(plan.evidence.steps[1].depends_on, Some(0));
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
