use core::{cmp::Ordering, fmt, str::FromStr};
use std::{borrow::Cow, collections::HashMap};

use hanabi_core::{Action, Clue, FullState, GameStatus, PlayerView, Rank, RuleError, Suit};

use crate::{
    ConventionAction, ConventionAnalysis, ConventionPolicyTier, EnumerateWorldsError,
    InformationSet, InformationSetError, LogicalDeductions, SupportedConvention, WorldCount,
    assess_card,
};

/// The result the planner should optimize during exact endgame analysis.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub enum PlanningObjective {
    /// Maximize expected official score.
    #[default]
    ExpectedScore,
    /// Maximize the chance of scoring 25 before preferring lesser outcomes.
    PerfectScore,
}

impl fmt::Display for PlanningObjective {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::ExpectedScore => "expected-score",
            Self::PerfectScore => "perfect-score",
        })
    }
}

impl FromStr for PlanningObjective {
    type Err = ParsePlanningObjectiveError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "expected-score" => Ok(Self::ExpectedScore),
            "perfect-score" => Ok(Self::PerfectScore),
            _ => Err(ParsePlanningObjectiveError(value.to_owned())),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ParsePlanningObjectiveError(String);

impl fmt::Display for ParsePlanningObjectiveError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "unknown planning objective {:?}; expected expected-score or perfect-score",
            self.0
        )
    }
}

impl std::error::Error for ParsePlanningObjectiveError {}

/// Highest score still reachable after accounting for exhausted identities.
#[must_use]
fn score_ceiling(state: &FullState) -> u8 {
    let mut discarded = [[0_u8; 5]; 5];
    for id in state.discard_pile() {
        if let Some(card) = state.card(*id) {
            discarded[card.suit.index()][card.rank.index()] += 1;
        }
    }
    Suit::ALL
        .iter()
        .map(|suit| {
            let played = state.play_stacks()[suit.index()].len();
            let blocked = Rank::ALL.iter().position(|rank| {
                rank.index() >= played && discarded[suit.index()][rank.index()] >= rank.copies()
            });
            u8::try_from(blocked.unwrap_or(5)).unwrap_or(5)
        })
        .sum()
}

/// Deterministic belief-state planner configuration.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PlannerConfig {
    pub objective: PlanningObjective,
    /// Maximum complete identity worlds admitted to the exact endgame.
    pub exact_world_limit: u64,
    /// Maximum observation-group/action nodes admitted to an exact solve.
    pub exact_node_limit: u64,
}

impl Default for PlannerConfig {
    fn default() -> Self {
        Self {
            objective: PlanningObjective::ExpectedScore,
            exact_world_limit: 4_096,
            exact_node_limit: 50_000,
        }
    }
}

/// Which representation produced a planner decision.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PlannerPhase {
    /// Unknown identities stayed as constrained domains and public counts.
    Symbolic,
    /// Every convention-consistent identity world was used for an exhaustive
    /// solve or a mathematically conclusive terminal-action proof.
    Exact,
}

/// Exact outcome distribution for one root action.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ExactActionValue {
    pub worlds: u64,
    pub perfect_worlds: u64,
    pub score_sum: u64,
    pub strikeout_worlds: u64,
    pub score_ceiling_sum: u64,
}

impl ExactActionValue {
    #[must_use]
    pub fn perfect_rate(self) -> f64 {
        ratio(self.perfect_worlds, self.worlds)
    }

    #[must_use]
    pub fn expected_score(self) -> f64 {
        ratio(self.score_sum, self.worlds)
    }

    #[must_use]
    pub fn strikeout_rate(self) -> f64 {
        ratio(self.strikeout_worlds, self.worlds)
    }

    #[must_use]
    pub fn expected_score_ceiling(self) -> f64 {
        ratio(self.score_ceiling_sum, self.worlds)
    }

    fn add(&mut self, other: Self) {
        self.worlds += other.worlds;
        self.perfect_worlds += other.perfect_worlds;
        self.score_sum += other.score_sum;
        self.strikeout_worlds += other.strikeout_worlds;
        self.score_ceiling_sum += other.score_ceiling_sum;
    }

    fn compare(self, other: Self, objective: PlanningObjective) -> Ordering {
        let primary = match objective {
            PlanningObjective::ExpectedScore => self
                .score_sum
                .cmp(&other.score_sum)
                .then_with(|| self.perfect_worlds.cmp(&other.perfect_worlds)),
            PlanningObjective::PerfectScore => self
                .perfect_worlds
                .cmp(&other.perfect_worlds)
                .then_with(|| self.score_sum.cmp(&other.score_sum)),
        };
        primary
            .then_with(|| other.strikeout_worlds.cmp(&self.strikeout_worlds))
            .then_with(|| self.score_ceiling_sum.cmp(&other.score_ceiling_sum))
    }
}

/// Deterministic evidence for one root candidate.
#[derive(Clone, Debug, PartialEq)]
pub struct PlannerActionEvaluation {
    pub action: Action,
    pub preference: crate::ActionPreference,
    pub certainly_playable: bool,
    pub certainly_useless: bool,
    pub newly_touched: u8,
    pub immediately_playable_touched: u8,
    pub critical_touched: u8,
    pub oldest_card_touched: bool,
    /// Convention-policy continuation with unresolved draws kept blank.
    pub symbolic_line: SymbolicLineOutcome,
    pub projection: crate::ProjectionEvidence,
    pub exact: Option<ExactActionValue>,
}

/// Consequences of the deterministic policy before an unresolved identity
/// or another explicit projection frontier interrupts its line.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct SymbolicLineOutcome {
    pub actions: u8,
    pub score_gain: u8,
    pub discards: u8,
    pub clues_spent: u8,
    pub clues_gained: u8,
    pub strikes: u8,
    pub stop_reason: SymbolicStopReason,
    /// Resource and opportunity assessment at the known projection frontier.
    pub position_value: Option<ProjectedPositionValue>,
    /// A shared elapsed-time comparison, even when full lines stop at
    /// different unknown identities later. This does not truncate search.
    pub first_rotation: Option<RotationCheckpoint>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RotationCheckpoint {
    pub actions: u8,
    pub discards: u8,
    pub value: ProjectedPositionValue,
}

/// Observable resources and conditional opportunities, not a sampled world.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ProjectedPositionValue {
    pub score: u8,
    pub clues: u8,
    pub exposed_critical_chops: u8,
    pub blocked_clued_cards: u8,
    pub secured_future_plays: u8,
    /// Distinct future points established by owner knowledge, with every
    /// predecessor already played or established too. Unlike secured cards,
    /// these do not need another identity/connection clue to become plays.
    pub committed_future_plays: u8,
    pub secured_card_quality: crate::SecuredCardQuality,
    /// Needed cards newly exposed by a worsening chop-protection exchange,
    /// including behind a queued play. Not an executed discard.
    pub exposed_chop_quality: crate::SecuredCardQuality,
    pub protected_bottom_deck_risks: u8,
    pub visible_successors: u8,
    pub finesse_opportunities: u8,
    /// Presence (0 or 1) of a checked hidden-successor alternative. Independent
    /// alternatives are not counted as simultaneous plays or probabilities.
    pub conditional_successors: u8,
    /// Bounded near-term reserve: a productive clue plus known save/repair needs.
    pub clue_demand: u8,
    pub save_pressure: u8,
    pub foregone_touch_opportunities: u8,
    /// Visible, unpromised playable identities currently on finesse position.
    /// Opportunities, not secured points or assumptions about blank draws.
    pub playable_finesse_opportunities: u8,
    /// Pang of Guilt: directly consuming an available positional blind play.
    pub foregone_blind_plays: u8,
    /// A saved connector could support a clue through a hidden successor.
    /// An option only, not an established card identity or future point.
    pub conditional_prompt_chains: u8,
}

impl ProjectedPositionValue {
    /// At the same elapsed turn, realized points and executable commitments
    /// outrank manufacturing surplus tokens. Protected cards and speculative
    /// connections are deliberately not counted as executable points.
    fn productive_preference(self, other: Self) -> bool {
        self.preserves_funded_progress(other)
            && (self.score > other.score
                || self.committed_future_plays > other.committed_future_plays)
    }

    fn preserves_funded_progress(self, other: Self) -> bool {
        // A funded pending critical Save is an obligation, not a lost card.
        // Allow only the excess pending Saves to account for a protection
        // deficit. This never adds executable points or changes reported facts.
        let pending = self
            .exposed_critical_chops
            .saturating_sub(other.exposed_critical_chops);
        self.clues >= self.clue_demand
            && other.clues >= other.clue_demand
            && self.score >= other.score
            && self.score.saturating_add(self.committed_future_plays)
                >= other.score.saturating_add(other.committed_future_plays)
            // The reserve already charges every pending critical Save.
            // Do not charge the same fully funded obligation again merely
            // because one line reaches the exposed chop earlier (reviewed
            // p4v0s1 turn 35). Actual losses are compared independently.
            && self.clue_demand >= self.exposed_critical_chops
            && other.clue_demand >= other.exposed_critical_chops
            && other
                .exposed_chop_quality
                .no_worse_than(self.exposed_chop_quality)
            && self.protected_bottom_deck_risks.saturating_add(pending) >= other.protected_bottom_deck_risks
            && self.score.saturating_add(self.secured_future_plays).saturating_add(pending)
                >= other.score.saturating_add(other.secured_future_plays)
            && self.save_pressure <= other.save_pressure
    }
    fn funded_completion_reserve(self, rotation: u8) -> Option<u8> {
        if rotation == 0
            || self.score.saturating_add(self.secured_future_plays) != 25
            || self.blocked_clued_cards != 0
            || self.exposed_critical_chops != 0
        {
            return None;
        }
        // Even waiting a complete rotation between each secured play needs
        // at most this many Burns. Do not reward discards for surplus tokens
        // after this conservative reserve has already been funded.
        Some(
            self.clue_demand
                .max(self.secured_future_plays.saturating_mul(rotation - 1)),
        )
    }
    /// At equal immediate resources, protecting an additional endangered
    /// future play is progress even if that newly saved card cannot play yet.
    /// Do not waive existing congestion: only the additional secured cards
    /// may account for the increase in blocked cards.
    fn protection_development_preference(self, other: Self) -> bool {
        self.score == other.score
            && other
                .exposed_chop_quality
                .no_worse_than(self.exposed_chop_quality)
            && self.clues >= other.clues
            && self.secured_future_plays > other.secured_future_plays
            && self.protected_bottom_deck_risks > other.protected_bottom_deck_risks
            && self.exposed_critical_chops < other.exposed_critical_chops
            && self
                .blocked_clued_cards
                .saturating_sub(other.blocked_clued_cards)
                <= self.secured_future_plays - other.secured_future_plays
            && self.visible_successors >= other.visible_successors
            && self.save_pressure <= other.save_pressure
            && self.foregone_touch_opportunities <= other.foregone_touch_opportunities
    }
    /// Heuristic access comparison at a common turn, not a proof of score.
    /// A ready positional card can be obtained efficiently; surplus tokens
    /// need not beat that opportunity or an already secured future play.
    pub(crate) fn development_preference(
        self,
        other: Self,
        discards: u8,
        other_discards: u8,
    ) -> bool {
        let demand = self.clue_demand.max(other.clue_demand);
        let accessible = |value: Self| {
            value
                .secured_future_plays
                .saturating_add(value.playable_finesse_opportunities)
        };
        self.score == other.score
            && other
                .exposed_chop_quality
                .no_worse_than(self.exposed_chop_quality)
            && accessible(self) >= accessible(other)
            && self.exposed_critical_chops <= other.exposed_critical_chops
            && self.blocked_clued_cards <= other.blocked_clued_cards
            && self.save_pressure <= other.save_pressure
            && self.foregone_touch_opportunities <= other.foregone_touch_opportunities
            && self.clues.min(demand) >= other.clues.min(demand)
            && self.playable_finesse_opportunities >= other.playable_finesse_opportunities
            && (accessible(self) > accessible(other)
                || self.clues.min(demand) > other.clues.min(demand)
                || self.playable_finesse_opportunities > other.playable_finesse_opportunities
                || (self.secured_future_plays >= other.secured_future_plays
                    && discards < other_discards))
    }
    fn without_speculative_finesse(mut self) -> Self {
        self.playable_finesse_opportunities = 0;
        self.finesse_opportunities = 0;
        self.conditional_successors = 0;
        self.clue_demand = 0;
        self
    }

    /// Compare developed points at equal elapsed time, including held useful
    /// cards. This is a strategic preference, not a guaranteed-score proof.
    /// A surplus refund must not outweigh an additional developed card; only
    /// new secured cards may account for additional hand congestion.
    /// Held cards do not compensate for less realized progress at this cutoff.
    /// Otherwise a one-action clue frontier can indefinitely postpone an
    /// available play merely by adding another card to the team's queue.
    /// This guards the development shortcut, not the general clue/play order:
    /// a clue can still win on safety, policy, or its longer observed line.
    fn developed_points_preference(self, other: Self) -> bool {
        self.score >= other.score
            && other
                .exposed_chop_quality
                .no_worse_than(self.exposed_chop_quality)
            && self.score.saturating_add(self.secured_future_plays)
                > other.score.saturating_add(other.secured_future_plays)
            && self.clues >= self.clue_demand
            && other.clues >= other.clue_demand
            && self.exposed_critical_chops <= other.exposed_critical_chops
            && self.save_pressure <= other.save_pressure
            && self.foregone_touch_opportunities <= other.foregone_touch_opportunities
            && self
                .blocked_clued_cards
                .saturating_sub(other.blocked_clued_cards)
                <= self
                    .secured_future_plays
                    .saturating_sub(other.secured_future_plays)
    }

    /// Unknown successors can break a genuine progress tie, but cannot buy
    /// away a token needed for a save, repair, or the follow-up clue itself.
    fn conditional_continuation_preference(self, other: Self) -> Option<bool> {
        let demand = self.clue_demand.max(other.clue_demand);
        if self.clues < demand
            || other.clues < demand
            || self.conditional_successors == other.conditional_successors
        {
            return None;
        }
        let mut left = self.without_speculative_finesse();
        let mut right = other.without_speculative_finesse();
        left.clues = 0;
        right.clues = 0;
        left.clue_demand = 0;
        right.clue_demand = 0;
        (left == right).then_some(self.conditional_successors > other.conditional_successors)
    }

    pub(crate) fn dominates(self, other: Self) -> bool {
        let (self_value, other) = (
            self.without_speculative_finesse(),
            other.without_speculative_finesse(),
        );
        self_value.dominates_resources(other)
    }

    fn dominates_resources(self, other: Self) -> bool {
        self != other
            && other.exposed_chop_quality.no_worse_than(self.exposed_chop_quality)
            && self.foregone_blind_plays <= other.foregone_blind_plays
            && self.conditional_prompt_chains >= other.conditional_prompt_chains
            && self.score >= other.score
            && self.clues >= other.clues
            && self.exposed_critical_chops <= other.exposed_critical_chops
            && self.blocked_clued_cards <= other.blocked_clued_cards
            && self.score.saturating_add(self.secured_future_plays)
                >= other.score.saturating_add(other.secured_future_plays)
            && self.score.saturating_add(self.committed_future_plays)
                >= other.score.saturating_add(other.committed_future_plays)
            // Rank/distance preferences compare equal amounts of present
            // and future progress. They must not forbid converting a secured
            // card into a stack point or obtaining additional future plays.
            && (self.score != other.score
                || self.secured_future_plays != other.secured_future_plays
                || self.committed_future_plays != other.committed_future_plays
                || self.secured_card_quality.no_worse_than(other.secured_card_quality))
            && self.protected_bottom_deck_risks >= other.protected_bottom_deck_risks
            && self.visible_successors >= other.visible_successors
            && self.save_pressure <= other.save_pressure
            && self.foregone_touch_opportunities <= other.foregone_touch_opportunities
    }
}

/// Why a deterministic symbolic continuation stopped.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum SymbolicStopReason {
    /// The game ended; no subsequent action exists.
    Terminal,
    /// No convention-policy action remained.
    #[default]
    Choice,
    /// The next action depended on an unresolved card identity.
    UnknownIdentity,
    /// Hidden information can change the next player's convention interpretation.
    UnknownInterpretation,
    /// The configured symbolic-action bound was reached.
    Limit,
    /// A nested player perspective could not be reconstructed.
    ProjectionUnavailable,
}

impl SymbolicLineOutcome {
    fn compare(self, other: Self) -> Ordering {
        other
            .strikes
            .cmp(&self.strikes)
            .then_with(|| self.score_gain.cmp(&other.score_gain))
            .then_with(|| self.net_clues().cmp(&other.net_clues()))
            .then_with(|| other.discards.cmp(&self.discards))
            .then_with(|| self.actions.cmp(&other.actions))
    }

    fn net_clues(self) -> i16 {
        i16::from(self.clues_gained) - i16::from(self.clues_spent)
    }
}

/// Result of deterministic symbolic planning or an exhaustive endgame solve.
#[derive(Clone, Debug, PartialEq)]
pub struct PlannerResult {
    pub best_action: Action,
    pub phase: PlannerPhase,
    /// Exact belief size when known, otherwise the first count beyond the
    /// configured exact-world limit.
    pub world_count: WorldCount,
    pub exact_nodes: u64,
    /// Why exhaustive solving completed, was skipped, or was abandoned.
    pub exact_status: ExactSearchStatus,
    pub root_actions: Vec<PlannerActionEvaluation>,
    /// Pairwise symbolic comparisons, retained rather than reconstructed by diagnostics.
    pub comparisons: Vec<CandidateComparison>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ExactSearchStatus {
    TerminalProof,
    Completed,
    WorldLimit,
    SingleCandidate,
    PreflightLimit,
    NodeLimit,
    DepthLimit,
}

/// The strongest applicable dimension in a symbolic comparison.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ComparisonReason {
    SavePrinciple,
    BottomDeckRisk,
    ForecastBottomDeckRisk,
    RotationDevelopment,
    FundedProgress,
    ClueEfficiency,
    ProgressTiming,
    ProtectedDevelopment,
    PolicyTier,
    TerminalProgress,
    KnownStrikes,
    ConditionalStrikes,
    EndpointResources,
    ConventionPlayOrder,
    ConditionalOpportunity,
    SpeculativeFinesse,
    SavePressure,
    WaitingOpportunity,
    TeammateClueHandoff,
    ConventionPreference,
    PreferredAction,
    LineProgress,
    PlayCertainty,
    TrashCertainty,
    CriticalTouch,
    OldestTouch,
    PlayableTouch,
    NewTouch,
    StableOrder,
}

/// Partial endpoint order. Incomparable does not mean equivalent or inferior.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EndpointComparison {
    PreferLeft(ComparisonReason),
    PreferRight(ComparisonReason),
    Equivalent,
    Incomparable,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CandidateComparison {
    /// Exact checkpoint operands retained only when diagnostic tracing is enabled.
    pub basis: Option<ComparisonBasis>,
    pub left: Action,
    pub right: Action,
    pub endpoint: EndpointComparison,
    pub preferred: Action,
    pub reason: ComparisonReason,
    /// The preference graph contains a path back across this edge.
    pub in_cycle: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ComparisonBasis {
    pub stage: &'static str,
    pub horizon: usize,
    pub left: Vec<RotationCheckpoint>,
    pub right: Vec<RotationCheckpoint>,
    pub clue_cost_bounds: Option<((u8, u8), (u8, u8))>,
    pub scheduled_refunds: Option<(u8, u8)>,
}

fn retain_comparison_basis(
    basis: &mut Option<ComparisonBasis>,
    stage: &'static str,
    horizon: usize,
    left: impl FnOnce() -> Vec<RotationCheckpoint>,
    right: impl FnOnce() -> Vec<RotationCheckpoint>,
) {
    if crate::diagnostics::enabled() {
        *basis = Some(ComparisonBasis {
            stage,
            horizon,
            left: left(),
            right: right(),
            clue_cost_bounds: None,
            scheduled_refunds: None,
        });
    }
}

/// Plans from known information without random world construction.
///
/// Early and midgame positions remain symbolic: hidden cards are identity
/// domains constrained by clues, convention inferences, and remaining copy
/// counts. Once the complete belief fits inside `exact_world_limit`, every
/// identity permutation is enumerated and solved. Exact recursion groups
/// worlds by the acting player's observation before choosing an action, so it
/// never conditions a decision on simulator truth that the player cannot see.
///
/// # Errors
///
/// Returns [`PlannerError`] for invalid observations, convention failures,
/// illegal transitions, or an unactionable position.
pub fn plan_move(
    information_set: &InformationSet,
    convention: SupportedConvention,
    config: PlannerConfig,
) -> Result<PlannerResult, PlannerError> {
    let deductions = information_set.deductions();
    let _memo = crate::h_group::begin_analysis_replay_memo();
    let analysis = convention.analyze(deductions);
    plan_move_with_analysis(information_set, convention, &analysis, config)
}

pub(crate) fn plan_move_with_analysis(
    information_set: &InformationSet,
    convention: SupportedConvention,
    analysis: &ConventionAnalysis,
    config: PlannerConfig,
) -> Result<PlannerResult, PlannerError> {
    plan_move_with_control(
        information_set,
        convention,
        analysis,
        config,
        &crate::AnalysisControl::default(),
    )
}

pub(crate) fn plan_move_with_control(
    information_set: &InformationSet,
    convention: SupportedConvention,
    analysis: &ConventionAnalysis,
    config: PlannerConfig,
    control: &crate::AnalysisControl,
) -> Result<PlannerResult, PlannerError> {
    control.checkpoint().map_err(PlannerError::Stopped)?;
    let _memo = crate::h_group::begin_analysis_replay_memo();
    #[cfg(test)]
    let _profile = crate::test_profile::span("planner");
    let objective = config.objective;
    let deductions = information_set.deductions();
    let candidates = planning_candidates(analysis);
    if candidates.is_empty() {
        return Err(PlannerError::NoCandidateActions);
    }
    let preferred = analysis.preferred_action;
    let mut evaluations = candidates
        .iter()
        .copied()
        .map(|action| symbolic_evaluation(deductions, action))
        .collect::<Vec<_>>();

    let belief = &analysis.belief_constraints;
    let count = information_set
        .world_count_with_control(belief, config.exact_world_limit, control)
        .map_err(PlannerError::Stopped)?;
    let counted_worlds = count.worlds();
    if count == WorldCount::Exact(0) {
        return Err(PlannerError::ConventionBeliefConflict);
    }

    if count.is_exact()
        && counted_worlds > 0
        && objective == PlanningObjective::PerfectScore
        && information_set
            .view()
            .play_stacks
            .iter()
            .map(Vec::len)
            .sum::<usize>()
            == 24
    {
        let proof = try_terminal_perfect_proof(
            information_set,
            analysis,
            counted_worlds,
            &mut evaluations,
            preferred,
            control,
        )?;
        control.checkpoint().map_err(PlannerError::Stopped)?;
        if let Some((best_index, tested_actions)) = proof {
            return Ok(PlannerResult {
                best_action: evaluations[best_index].action,
                phase: PlannerPhase::Exact,
                world_count: count,
                exact_nodes: tested_actions,
                exact_status: ExactSearchStatus::TerminalProof,
                root_actions: evaluations,
                comparisons: Vec::new(),
            });
        }
    }
    // With one admissible action, searching its continuations cannot change
    // the choice. Still validate current beliefs and retain terminal proofs
    // above; only omit the otherwise unnecessary exhaustive continuation.
    let full_exact_search = candidates.len() > 1
        && count.is_exact()
        && counted_worlds > 0
        && exact_preflight(
            information_set.view(),
            counted_worlds,
            candidates.len(),
            config.exact_node_limit,
        );
    let mut exact_nodes = 0;
    let mut exact_status = if !count.is_exact() {
        ExactSearchStatus::WorldLimit
    } else if candidates.len() == 1 {
        ExactSearchStatus::SingleCandidate
    } else {
        ExactSearchStatus::PreflightLimit
    };
    if full_exact_search {
        let (best, nodes, status) = run_exact_search(
            information_set,
            convention,
            config,
            analysis,
            counted_worlds,
            &mut evaluations,
            control,
        )?;
        exact_nodes = nodes;
        exact_status = status;
        if let Some(index) = best {
            return Ok(PlannerResult {
                best_action: evaluations[index].action,
                phase: PlannerPhase::Exact,
                world_count: count,
                exact_nodes,
                exact_status,
                root_actions: evaluations,
                comparisons: Vec::new(),
            });
        }
    }

    project_symbolic_roots(deductions, convention, &mut evaluations, control)?;
    symbolic_result(evaluations, preferred, count, exact_nodes, exact_status)
}

fn run_exact_search(
    information: &InformationSet,
    convention: SupportedConvention,
    config: PlannerConfig,
    analysis: &ConventionAnalysis,
    counted_worlds: u64,
    evaluations: &mut [PlannerActionEvaluation],
    control: &crate::AnalysisControl,
) -> Result<(Option<usize>, u64, ExactSearchStatus), PlannerError> {
    #[cfg(test)]
    let _profile = crate::test_profile::span("exact_search");
    let worlds = information
        .collect_worlds_after_count(
            &analysis.belief_constraints,
            usize::try_from(counted_worlds).unwrap_or(usize::MAX),
            control,
        )
        .map_err(PlannerError::EnumerateWorlds)?;
    let mut budget = ExactBudget {
        used: 0,
        limit: config.exact_node_limit,
        control,
    };
    let status = match evaluate_exact_root(
        &worlds,
        convention,
        config.objective,
        evaluations,
        analysis.preferred_action,
        &mut budget,
    ) {
        Ok((values, proven)) => {
            for (evaluation, value) in evaluations.iter_mut().zip(values) {
                evaluation.exact = value;
            }
            let best = proven
                .or_else(|| {
                    best_exact_index(evaluations, config.objective, analysis.preferred_action)
                })
                .ok_or(PlannerError::NoCandidateActions)?;
            return Ok((Some(best), budget.used, ExactSearchStatus::Completed));
        }
        Err(ExactAbort::BudgetExceeded) => ExactSearchStatus::NodeLimit,
        Err(ExactAbort::DepthExceeded) => ExactSearchStatus::DepthLimit,
        Err(ExactAbort::InvalidCurrentPlayer | ExactAbort::NoCandidateActions) => {
            return Err(PlannerError::NoCandidateActions);
        }
        Err(ExactAbort::InformationSet(error)) => return Err(PlannerError::InformationSet(error)),
        Err(ExactAbort::Rule(error)) => return Err(PlannerError::Rule(error)),
        Err(ExactAbort::Stopped(error)) => return Err(PlannerError::Stopped(error)),
    };
    Ok((None, budget.used, status))
}

fn project_symbolic_roots(
    deductions: &LogicalDeductions,
    convention: SupportedConvention,
    evaluations: &mut [PlannerActionEvaluation],
    control: &crate::AnalysisControl,
) -> Result<(), PlannerError> {
    // Scores order candidates; they must not prevent testing their lines.
    for evaluation in evaluations {
        control.checkpoint().map_err(PlannerError::Stopped)?;
        (evaluation.symbolic_line, evaluation.projection) = convention
            .project_symbolic_projection(deductions.view(), evaluation.action, 32, control)
            .map_err(PlannerError::Stopped)?;
    }
    control.checkpoint().map_err(PlannerError::Stopped)
}

fn try_terminal_perfect_proof(
    information_set: &InformationSet,
    analysis: &ConventionAnalysis,
    world_count: u64,
    evaluations: &mut [PlannerActionEvaluation],
    preferred: Option<Action>,
    control: &crate::AnalysisControl,
) -> Result<Option<(usize, u64)>, PlannerError> {
    let worlds = information_set
        .collect_worlds_after_count(
            &analysis.belief_constraints,
            usize::try_from(world_count).unwrap_or(usize::MAX),
            control,
        )
        .map_err(PlannerError::EnumerateWorlds)?;
    prove_unanimous_terminal_perfect(&worlds, evaluations, preferred).map_err(PlannerError::Rule)
}

fn symbolic_result(
    evaluations: Vec<PlannerActionEvaluation>,
    preferred: Option<Action>,
    world_count: WorldCount,
    exact_nodes: u64,
    exact_status: ExactSearchStatus,
) -> Result<PlannerResult, PlannerError> {
    let (best_index, comparisons) = compare_symbolic_candidates(&evaluations, preferred);
    let best_index = best_index.ok_or(PlannerError::NoCandidateActions)?;
    Ok(PlannerResult {
        best_action: evaluations[best_index].action,
        phase: PlannerPhase::Symbolic,
        world_count,
        exact_nodes,
        exact_status,
        root_actions: evaluations,
        comparisons,
    })
}

/// One bounded strategic choice inside a forecast. Uses the same candidate
/// admission and endpoint comparator as the root; its leaf policy does not
/// recursively invoke this chooser.
pub(crate) fn choose_projected_follow_up(
    deductions: &LogicalDeductions,
    profile: crate::HGroupProfile,
    control: &crate::AnalysisControl,
) -> Result<Option<Action>, crate::AnalysisStopped> {
    let convention = SupportedConvention::HGroup(profile);
    let analysis = convention.analyze(deductions);
    let candidates = planning_candidates(&analysis);
    if candidates.len() == 1 {
        crate::diagnostics::record(
            deductions.view(),
            &analysis,
            &[],
            &[],
            Some(candidates[0].action),
        );
        return Ok(Some(candidates[0].action));
    }
    let mut evaluations = Vec::with_capacity(candidates.len());
    for candidate in candidates.iter().copied() {
        control.checkpoint()?;
        let mut evaluation = symbolic_evaluation(deductions, candidate);
        (evaluation.symbolic_line, evaluation.projection) =
            crate::h_group::symbolic_line::project_leaf_projection(
                deductions.view(),
                profile,
                candidate.action,
                control,
            )?;
        evaluations.push(evaluation);
    }
    let (best, comparisons) = compare_symbolic_candidates(&evaluations, analysis.preferred_action);
    let selected = best.map(|index| evaluations[index].action);
    crate::diagnostics::record(
        deductions.view(),
        &analysis,
        &evaluations,
        &comparisons,
        selected,
    );
    Ok(selected)
}

/// Applies convention-forced continuations identically at the root and at
/// every exact observation group. Borrowing the normal action list avoids an
/// allocation on the common path.
fn planning_candidates(analysis: &ConventionAnalysis) -> Cow<'_, [ConventionAction]> {
    analysis.forced_action.map_or_else(
        || Cow::Borrowed(analysis.actions.as_slice()),
        |forced| {
            Cow::Owned(vec![
                analysis
                    .actions
                    .iter()
                    .find(|candidate| candidate.action == forced)
                    .copied()
                    .unwrap_or(ConventionAction {
                        action: forced,
                        preference: crate::ActionPreference::new(0, false)
                            .with_policy_tier(ConventionPolicyTier::Required),
                        reason: crate::ConventionActionReason::Fallback,
                    }),
            ])
        },
    )
}

fn exact_preflight(view: &PlayerView, worlds: u64, root_actions: usize, node_limit: u64) -> bool {
    let remaining_turns = view.final_turns_remaining.map_or_else(
        || view.hands.len().saturating_add(view.deck_size),
        usize::from,
    );
    // A forced root can reveal several legal continuations. Eight is a
    // conservative floor without forbidding every tractable final-round solve.
    let branching = u64::try_from(root_actions.max(8)).unwrap_or(u64::MAX);
    let mut estimate = worlds;
    let mut frontier = worlds;
    for _ in 0..remaining_turns {
        frontier = frontier.saturating_mul(branching);
        estimate = estimate.saturating_add(frontier);
        if estimate > node_limit {
            return false;
        }
    }
    true
}

/// Finds a root play that ends every convention-consistent world at 25.
///
/// A unanimous perfect terminal result is globally maximal under the
/// perfect-score objective. Non-play actions cannot immediately increase a
/// score of 24, so only admitted plays need to be checked. Among multiple
/// proofs, ordinary convention ordering remains the deterministic tie-break.
fn prove_unanimous_terminal_perfect(
    worlds: &[FullState],
    evaluations: &mut [PlannerActionEvaluation],
    preferred: Option<Action>,
) -> Result<Option<(usize, u64)>, RuleError> {
    let mut tested_actions = 0_u64;
    let mut best: Option<(usize, crate::ActionPreference, bool)> = None;
    for (index, evaluation) in evaluations.iter_mut().enumerate() {
        if !matches!(evaluation.action, Action::Play(_)) {
            continue;
        }
        tested_actions += 1;
        let mut value = ExactActionValue {
            worlds: 0,
            perfect_worlds: 0,
            score_sum: 0,
            strikeout_worlds: 0,
            score_ceiling_sum: 0,
        };
        let mut unanimous = true;
        for world in worlds {
            let mut advanced = world.clone();
            advanced.apply(evaluation.action)?;
            if advanced.final_score() != Some(25) {
                unanimous = false;
                break;
            }
            value.add(terminal_value(&advanced));
        }
        if !unanimous {
            continue;
        }
        evaluation.exact = Some(value);
        let priority = evaluation.preference;
        let is_preferred = preferred == Some(evaluation.action);
        let replace =
            best.as_ref()
                .is_none_or(|(current_index, current_priority, current_preferred)| {
                    priority
                        .cmp(current_priority)
                        .then_with(|| is_preferred.cmp(current_preferred))
                        .then_with(|| current_index.cmp(&index))
                        == Ordering::Greater
                });
        if replace {
            best = Some((index, priority, is_preferred));
        }
    }
    Ok(best.map(|(index, _, _)| (index, tested_actions)))
}

#[allow(clippy::cast_precision_loss)]
fn ratio(numerator: u64, denominator: u64) -> f64 {
    if denominator == 0 {
        0.0
    } else {
        numerator as f64 / denominator as f64
    }
}

fn symbolic_evaluation(
    deductions: &LogicalDeductions,
    convention_action: ConventionAction,
) -> PlannerActionEvaluation {
    let action = convention_action.action;
    let view = deductions.view();
    let assessment = match action {
        Action::Play(card) | Action::Discard(card) => assess_card(deductions, card),
        Action::Clue { .. } => None,
    };
    let (newly_touched, immediately_playable_touched, critical_touched, oldest_card_touched) =
        match action {
            Action::Clue { target, clue } => clue_effects(view, target.index(), clue),
            Action::Play(_) | Action::Discard(_) => (0, 0, 0, false),
        };
    PlannerActionEvaluation {
        action,
        preference: convention_action.preference,
        certainly_playable: assessment.is_some_and(|value| value.certainly_playable),
        certainly_useless: assessment.is_some_and(|value| value.certainly_useless),
        newly_touched,
        immediately_playable_touched,
        critical_touched,
        oldest_card_touched,
        symbolic_line: SymbolicLineOutcome::default(),
        projection: crate::ProjectionEvidence::default(),
        exact: None,
    }
}

fn clue_effects(view: &PlayerView, target: usize, clue: Clue) -> (u8, u8, u8, bool) {
    let Some(hand) = view.hands.get(target) else {
        return (0, 0, 0, false);
    };
    let oldest = hand.first().map(|card| card.id);
    hand.iter()
        .filter(|card| card.identity.is_some_and(|identity| clue.matches(identity)))
        .fold((0_u8, 0_u8, 0_u8, false), |value, card| {
            let (new, playable, critical, touched_oldest) = value;
            let identity = card.identity.expect("another player's hand is visible");
            (
                new + u8::from(!card.clues.has_positive_clue(clue)),
                playable
                    + u8::from(
                        identity.rank.number()
                            == u8::try_from(view.play_stacks[identity.suit.index()].len())
                                .expect("a standard stack has at most five cards")
                                + 1,
                    ),
                critical + u8::from(is_publicly_critical(view, identity)),
                touched_oldest || oldest == Some(card.id),
            )
        })
}

fn is_publicly_critical(view: &PlayerView, identity: hanabi_core::Card) -> bool {
    if view.play_stacks[identity.suit.index()].len() >= usize::from(identity.rank.number()) {
        return false;
    }
    let discarded = view
        .discard_pile
        .iter()
        .filter(|(_, card)| *card == identity)
        .count();
    discarded + 1 >= usize::from(identity.rank.copies())
}

#[cfg(test)]
fn best_symbolic_index(
    evaluations: &[PlannerActionEvaluation],
    preferred: Option<Action>,
) -> Option<usize> {
    compare_symbolic_candidates(evaluations, preferred).0
}

fn compare_symbolic_candidates(
    evaluations: &[PlannerActionEvaluation],
    preferred: Option<Action>,
) -> (Option<usize>, Vec<CandidateComparison>) {
    let count = evaluations.len();
    let mut reaches = vec![vec![false; count]; count];
    let mut evidence_reaches = reaches.clone();
    let mut comparisons = Vec::new();
    for left in 0..count {
        reaches[left][left] = true;
        evidence_reaches[left][left] = true;
        for right in left + 1..count {
            let a = &evaluations[left];
            let b = &evaluations[right];
            let (endpoint, basis) = compare_endpoints_with_basis(a, b);
            match endpoint {
                EndpointComparison::PreferLeft(
                    ComparisonReason::RotationDevelopment
                    | ComparisonReason::FundedProgress
                    | ComparisonReason::ClueEfficiency
                    | ComparisonReason::ProgressTiming
                    | ComparisonReason::ProtectedDevelopment
                    | ComparisonReason::BottomDeckRisk,
                ) => {
                    evidence_reaches[left][right] = true;
                }
                EndpointComparison::PreferRight(
                    ComparisonReason::RotationDevelopment
                    | ComparisonReason::FundedProgress
                    | ComparisonReason::ClueEfficiency
                    | ComparisonReason::ProgressTiming
                    | ComparisonReason::ProtectedDevelopment
                    | ComparisonReason::BottomDeckRisk,
                ) => {
                    evidence_reaches[right][left] = true;
                }
                _ => {}
            }
            let (ordering, reason) = match endpoint {
                EndpointComparison::PreferLeft(reason) => (Ordering::Greater, reason),
                EndpointComparison::PreferRight(reason) => (Ordering::Less, reason),
                EndpointComparison::Equivalent | EndpointComparison::Incomparable => {
                    symbolic_fallback_comparison(a, b, preferred)
                }
            };
            reaches[left][right] = ordering.is_gt();
            reaches[right][left] = !ordering.is_gt();
            comparisons.push(CandidateComparison {
                basis,
                left: a.action,
                right: b.action,
                endpoint,
                preferred: if ordering.is_gt() { a.action } else { b.action },
                reason,
                in_cycle: false,
            });
        }
    }
    // Preserve the established cycle-safe selection. A partial endpoint order
    // plus heuristic fallback is not assumed to be transitive.
    for via in 0..count {
        for from in 0..count {
            for to in 0..count {
                reaches[from][to] |= reaches[from][via] && reaches[via][to];
                evidence_reaches[from][to] |=
                    evidence_reaches[from][via] && evidence_reaches[via][to];
            }
        }
    }
    let mut edge = 0;
    for (left, row) in reaches.iter().enumerate() {
        for (right, other) in reaches.iter().enumerate().skip(left + 1) {
            comparisons[edge].in_cycle = row[right] && other[left];
            edge += 1;
        }
    }
    // Start with the policy's ordering, then improve it using direct line
    // comparisons. A weak fallback edge must not form a cycle that restores
    // a candidate explicitly beaten by its projected alternative. If the
    // evidence itself cycles, retain its SCC and use the stable fallback.
    let fallback = |left: &usize, right: &usize| {
        symbolic_fallback_comparison(&evaluations[*left], &evaluations[*right], preferred).0
    };
    let baseline = (0..count)
        .filter(|index| reaches[*index].iter().all(|reachable| *reachable))
        .max_by(fallback);
    let selected = baseline.and_then(|seed| {
        (0..count)
            .filter(|index| evidence_reaches[*index][seed])
            .filter(|index| {
                (0..count).all(|other| {
                    !evidence_reaches[other][*index] || evidence_reaches[*index][other]
                })
            })
            .max_by(fallback)
    });
    (selected, comparisons)
}

#[cfg(test)]
fn compare_endpoints(
    left: &PlannerActionEvaluation,
    right: &PlannerActionEvaluation,
) -> EndpointComparison {
    compare_endpoints_with_basis(left, right).0
}

fn compare_endpoints_with_basis(
    left: &PlannerActionEvaluation,
    right: &PlannerActionEvaluation,
) -> (EndpointComparison, Option<ComparisonBasis>) {
    let mut basis = None;
    let comparison = compare_endpoint_evidence(left, right, &mut basis);
    let horizon = left
        .projection
        .risk_horizon()
        .min(right.projection.risk_horizon())
        .max(1);
    let risk = left
        .projection
        .recorded_bottom_deck_risks_at(horizon)
        .cmp(&right.projection.recorded_bottom_deck_risks_at(horizon));
    // A resource snapshot cannot dominate by trimming away a recorded loss.
    // Unexecuted discard possibilities belong to the assessed-risk comparison,
    // not this guard: reaching equal uncertainty earlier is not itself a loss.
    let guarded = match comparison {
        EndpointComparison::PreferLeft(
            ComparisonReason::EndpointResources
            | ComparisonReason::FundedProgress
            | ComparisonReason::ClueEfficiency
            | ComparisonReason::ProgressTiming,
        ) if risk == Ordering::Greater => EndpointComparison::Incomparable,
        EndpointComparison::PreferRight(
            ComparisonReason::EndpointResources
            | ComparisonReason::FundedProgress
            | ComparisonReason::ClueEfficiency
            | ComparisonReason::ProgressTiming,
        ) if risk == Ordering::Less => EndpointComparison::Incomparable,
        _ => comparison,
    };
    if guarded != comparison {
        basis = None;
    }
    (guarded, basis)
}

#[allow(clippy::too_many_lines)]
fn compare_endpoint_evidence(
    left: &PlannerActionEvaluation,
    right: &PlannerActionEvaluation,
    basis: &mut Option<ComparisonBasis>,
) -> EndpointComparison {
    match compare_save_principle_risks(left, right) {
        Ordering::Less => return EndpointComparison::PreferLeft(ComparisonReason::SavePrinciple),
        Ordering::Greater => {
            return EndpointComparison::PreferRight(ComparisonReason::SavePrinciple);
        }
        Ordering::Equal => {}
    }
    if left.preference.policy_tier() != right.preference.policy_tier()
        || left.preference.advances_terminal_plan() != right.preference.advances_terminal_plan()
        || left.symbolic_line.strikes != right.symbolic_line.strikes
        || left.projection.maximum_strikes() != right.projection.maximum_strikes()
    {
        return EndpointComparison::Incomparable;
    }
    match compare_bottom_deck_risks(left, right) {
        Ordering::Less => return EndpointComparison::PreferLeft(ComparisonReason::BottomDeckRisk),
        Ordering::Greater => {
            return EndpointComparison::PreferRight(ComparisonReason::BottomDeckRisk);
        }
        Ordering::Equal => {}
    }
    // Globally known transferred cards follow ordinary Priority, not urgent
    // blind-play priority. Preserve the convention's scheduled order rather
    // than letting endpoint resources silently reverse it.
    // https://hanabi.github.io/level-25/#3-playing-globally-known-cards
    match left.preference.compare_play_order(right.preference) {
        Ordering::Greater => {
            return EndpointComparison::PreferLeft(ComparisonReason::ConventionPlayOrder);
        }
        Ordering::Less => {
            return EndpointComparison::PreferRight(ComparisonReason::ConventionPlayOrder);
        }
        Ordering::Equal => {}
    }
    let horizon = left
        .projection
        .common_horizon()
        .min(right.projection.common_horizon());
    let left_values = left.projection.checkpoints_at(horizon);
    let right_values = right.projection.checkpoints_at(horizon);
    if horizon > 0 && !left_values.is_empty() && !right_values.is_empty() {
        let refunds = (
            left.projection.play_refunds_after(horizon),
            right.projection.play_refunds_after(horizon),
        );
        let funded = |mut value: ProjectedPositionValue, refund: u8| {
            // A future 5 refund can fund discretionary follow-up work, but
            // cannot excuse an exposed critical card or an immediate Save.
            if value.exposed_critical_chops == 0 && value.save_pressure == 0 {
                value.clues = value.clues.saturating_add(refund);
            }
            value
        };
        let wins = |a: &[RotationCheckpoint], b: &[RotationCheckpoint], refunds: (u8, u8)| {
            a.iter().all(|a| {
                b.iter().all(|b| {
                    funded(a.value, refunds.0).productive_preference(funded(b.value, refunds.1))
                })
            })
        };
        let reason = if wins(&left_values, &right_values, refunds) {
            Some(EndpointComparison::PreferLeft(
                ComparisonReason::FundedProgress,
            ))
        } else if wins(&right_values, &left_values, (refunds.1, refunds.0)) {
            Some(EndpointComparison::PreferRight(
                ComparisonReason::FundedProgress,
            ))
        } else {
            None
        };
        if let Some(reason) = reason {
            retain_comparison_basis(
                basis,
                "fundedProgress",
                usize::from(horizon),
                || left_values,
                || right_values,
            );
            if let Some(basis) = basis {
                basis.scheduled_refunds = Some(refunds);
            }
            return reason;
        }
        if let Some((a_cost, b_cost)) = left
            .projection
            .clue_cost_at(horizon)
            .zip(right.projection.clue_cost_at(horizon))
        {
            let efficient = |a: &[RotationCheckpoint], b: &[RotationCheckpoint]| {
                a.iter().all(|a| {
                    b.iter().all(|b| {
                        a.value.score == b.value.score
                            && a.value.committed_future_plays == b.value.committed_future_plays
                            && a.value.preserves_funded_progress(b.value)
                    })
                })
            };
            // Include outstanding critical Saves in the clue bill. Deferring
            // a funded Save is not losing protection, but neither is it free
            // efficiency: compare completed plus still-required work.
            let costs = |values: &[RotationCheckpoint], bounds: (u8, u8)| {
                (
                    bounds.0.saturating_add(
                        values
                            .iter()
                            .map(|v| v.value.exposed_critical_chops)
                            .min()
                            .unwrap_or(0),
                    ),
                    bounds.1.saturating_add(
                        values
                            .iter()
                            .map(|v| v.value.exposed_critical_chops)
                            .max()
                            .unwrap_or(0),
                    ),
                )
            };
            let (a_cost, b_cost) = (costs(&left_values, a_cost), costs(&right_values, b_cost));
            let preference = if a_cost.1 < b_cost.0 && efficient(&left_values, &right_values) {
                Some(EndpointComparison::PreferLeft(
                    ComparisonReason::ClueEfficiency,
                ))
            } else if b_cost.1 < a_cost.0 && efficient(&right_values, &left_values) {
                Some(EndpointComparison::PreferRight(
                    ComparisonReason::ClueEfficiency,
                ))
            } else {
                None
            };
            if let Some(preference) = preference {
                retain_comparison_basis(
                    basis,
                    "clueEfficiency",
                    usize::from(horizon),
                    || left_values,
                    || right_values,
                );
                if let Some(basis) = basis {
                    basis.clue_cost_bounds = Some((a_cost, b_cost));
                }
                return preference;
            }
            // A later equal score must not erase an earlier lead. Require
            // no score deficit at ANY shared checkpoint, strictly earlier
            // progress somewhere, and no extra completed/pending clue cost.
            let timing = |a: &PlannerActionEvaluation, b: &PlannerActionEvaluation| {
                // Earlier points are a scheduling preference, not permission
                // to replace an endpoint's ready successors with blocked work.
                if !a.projection.checkpoints_at(horizon).iter().all(|a| {
                    b.projection.checkpoints_at(horizon).iter().all(|b| {
                        a.value.blocked_clued_cards <= b.value.blocked_clued_cards
                            && a.value.visible_successors >= b.value.visible_successors
                            && (a.value.secured_future_plays != b.value.secured_future_plays
                                || a.value
                                    .secured_card_quality
                                    .no_worse_than(b.value.secured_card_quality))
                    })
                }) {
                    return false;
                }
                let mut ahead = false;
                for turn in 1..=horizon {
                    let a = a.projection.checkpoints_at(turn);
                    let b = b.projection.checkpoints_at(turn);
                    if a.is_empty() || b.is_empty() {
                        return false;
                    }
                    let minimum = a.iter().map(|v| v.value.score).min().unwrap();
                    let maximum = b.iter().map(|v| v.value.score).max().unwrap();
                    if minimum < maximum {
                        return false;
                    }
                    ahead |= minimum > maximum;
                }
                ahead
            };
            let preference = if a_cost.1 <= b_cost.0
                && efficient(&left_values, &right_values)
                && timing(left, right)
            {
                Some(EndpointComparison::PreferLeft(
                    ComparisonReason::ProgressTiming,
                ))
            } else if b_cost.1 <= a_cost.0
                && efficient(&right_values, &left_values)
                && timing(right, left)
            {
                Some(EndpointComparison::PreferRight(
                    ComparisonReason::ProgressTiming,
                ))
            } else {
                None
            };
            if let Some(preference) = preference {
                retain_comparison_basis(
                    basis,
                    "progressTiming",
                    usize::from(horizon),
                    || left_values,
                    || right_values,
                );
                if let Some(basis) = basis {
                    basis.clue_cost_bounds = Some((a_cost, b_cost));
                }
                return preference;
            }
        }
    }
    // Positional-access development schedules a held play versus spending
    // the turn on a clue. Clue-versus-clue comparisons below additionally
    // require strictly more realized points: speculative access alone must
    // not replace a 2-for-1 clue with a speculative 1-for-1.
    let schedules_play_and_clue = matches!(
        (left.action, right.action),
        (Action::Play(_), Action::Clue { .. }) | (Action::Clue { .. }, Action::Play(_))
    );
    let compares_clues = matches!(
        (left.action, right.action),
        (Action::Clue { .. }, Action::Clue { .. })
    );
    if let Some((a, b)) = left
        .symbolic_line
        .first_rotation
        .zip(right.symbolic_line.first_rotation)
    {
        if schedules_play_and_clue && a.actions == b.actions {
            let prefer_left = a
                .value
                .development_preference(b.value, a.discards, b.discards);
            let prefer_right = b
                .value
                .development_preference(a.value, b.discards, a.discards);
            if prefer_left && !prefer_right {
                retain_comparison_basis(
                    basis,
                    "firstRotation",
                    usize::from(a.actions),
                    || vec![a],
                    || vec![b],
                );
                return EndpointComparison::PreferLeft(ComparisonReason::RotationDevelopment);
            }
            if prefer_right && !prefer_left {
                retain_comparison_basis(
                    basis,
                    "firstRotation",
                    usize::from(a.actions),
                    || vec![a],
                    || vec![b],
                );
                return EndpointComparison::PreferRight(ComparisonReason::RotationDevelopment);
            }
        }
    }
    if left.symbolic_line.actions != right.symbolic_line.actions
        || left.projection.has_branches()
        || right.projection.has_branches()
    {
        // Compare at the latest shared elapsed turn, retaining both tails.
        // A later observed strike is checked above and cannot be trimmed away.
        let horizon = left
            .projection
            .common_horizon()
            .min(right.projection.common_horizon());
        let left_values = left.projection.checkpoints_at(horizon);
        let right_values = right.projection.checkpoints_at(horizon);
        if !left_values.is_empty() && !right_values.is_empty() {
            retain_comparison_basis(
                basis,
                "sharedHorizon",
                usize::from(horizon),
                || left_values.clone(),
                || right_values.clone(),
            );
            let every_pair =
                |predicate: fn(ProjectedPositionValue, ProjectedPositionValue) -> bool| {
                    left_values
                        .iter()
                        .all(|a| right_values.iter().all(|b| predicate(a.value, b.value)))
                };
            // Clue-versus-clue development requires strictly more *realized*
            // points in every branch, not speculative access or a longer queue.
            if (schedules_play_and_clue || (compares_clues && every_pair(|a, b| a.score > b.score)))
                && every_pair(ProjectedPositionValue::developed_points_preference)
            {
                return EndpointComparison::PreferLeft(ComparisonReason::RotationDevelopment);
            }
            if (schedules_play_and_clue || (compares_clues && every_pair(|a, b| b.score > a.score)))
                && every_pair(|a, b| b.developed_points_preference(a))
            {
                return EndpointComparison::PreferRight(ComparisonReason::RotationDevelopment);
            }
            if every_pair(ProjectedPositionValue::dominates) {
                return EndpointComparison::PreferLeft(ComparisonReason::EndpointResources);
            }
            if every_pair(|a, b| b.dominates(a)) {
                return EndpointComparison::PreferRight(ComparisonReason::EndpointResources);
            }
        }
    }
    if left.symbolic_line.actions != right.symbolic_line.actions
        || left.symbolic_line.stop_reason != right.symbolic_line.stop_reason
    {
        *basis = None;
        return EndpointComparison::Incomparable;
    }
    let Some((mut a, mut b)) = left
        .symbolic_line
        .position_value
        .zip(right.symbolic_line.position_value)
    else {
        return EndpointComparison::Incomparable;
    };
    if let Some((left_rotation, right_rotation)) = left
        .symbolic_line
        .first_rotation
        .zip(right.symbolic_line.first_rotation)
    {
        if left_rotation.actions == right_rotation.actions && a.score == b.score {
            if let Some((left_reserve, right_reserve)) = a
                .funded_completion_reserve(left_rotation.actions)
                .zip(b.funded_completion_reserve(right_rotation.actions))
            {
                let reserve = left_reserve.max(right_reserve);
                if a.clues >= reserve && b.clues >= reserve {
                    a.clues = reserve;
                    b.clues = reserve;
                }
            }
        }
    }
    retain_comparison_basis(
        basis,
        "normalizedEndpoints",
        usize::from(left.symbolic_line.actions),
        || {
            vec![RotationCheckpoint {
                actions: left.symbolic_line.actions,
                discards: left.symbolic_line.discards,
                value: a,
            }]
        },
        || {
            vec![RotationCheckpoint {
                actions: right.symbolic_line.actions,
                discards: right.symbolic_line.discards,
                value: b,
            }]
        },
    );
    if a.protection_development_preference(b) {
        return EndpointComparison::PreferLeft(ComparisonReason::ProtectedDevelopment);
    }
    if b.protection_development_preference(a) {
        return EndpointComparison::PreferRight(ComparisonReason::ProtectedDevelopment);
    }
    if compares_clues && a.score > b.score && a.developed_points_preference(b) {
        return EndpointComparison::PreferLeft(ComparisonReason::RotationDevelopment);
    }
    if compares_clues && b.score > a.score && b.developed_points_preference(a) {
        return EndpointComparison::PreferRight(ComparisonReason::RotationDevelopment);
    }
    if let Some(prefers_left) = a.conditional_continuation_preference(b) {
        return if prefers_left {
            EndpointComparison::PreferLeft(ComparisonReason::ConditionalOpportunity)
        } else {
            EndpointComparison::PreferRight(ComparisonReason::ConditionalOpportunity)
        };
    }
    if a.dominates(b) {
        return EndpointComparison::PreferLeft(ComparisonReason::EndpointResources);
    }
    if b.dominates(a) {
        return EndpointComparison::PreferRight(ComparisonReason::EndpointResources);
    }
    if left.preference == right.preference
        && a.without_speculative_finesse() == b.without_speculative_finesse()
        && a.finesse_opportunities != b.finesse_opportunities
    {
        return if a.finesse_opportunities > b.finesse_opportunities {
            EndpointComparison::PreferLeft(ComparisonReason::SpeculativeFinesse)
        } else {
            EndpointComparison::PreferRight(ComparisonReason::SpeculativeFinesse)
        };
    }
    if a == b {
        EndpointComparison::Equivalent
    } else {
        EndpointComparison::Incomparable
    }
}

fn compare_identified_losses(
    left: &PlannerActionEvaluation,
    right: &PlannerActionEvaluation,
    horizon: usize,
) -> Ordering {
    // A possible loss at an unknown future discard is not the same evidence
    // as discarding an identified needed card. When total risk counts tie,
    // retain this distinction instead of letting speculative risk erase
    // demonstrated protection. Do not credit merely delaying a known loss
    // beyond the shared horizon, a required sacrifice, or a root discard.
    // Reviewed example: p4v0s1 turn 14, green protects Donald's p4 while red
    // discards it. This does not assume favorable future draws or claim that
    // the protected line is risk-free forever.
    let known_prefix = |candidate: &PlannerActionEvaluation| {
        candidate
            .projection
            .steps
            .iter()
            .take(horizon)
            .filter(|step| step.consequences.bottom_deck_risk.is_some())
            .count()
    };
    let known = known_prefix(left).cmp(&known_prefix(right));
    let preserves_protection = |candidate: &PlannerActionEvaluation| {
        !candidate.projection.steps.is_empty()
            && candidate
                .projection
                .unresolved_discard
                .is_some_and(|discard| discard.bottom_deck_risk && !discard.required_protection)
            && candidate.symbolic_line.position_value.is_some_and(|value| {
                value.exposed_chop_quality == crate::SecuredCardQuality::default()
            })
    };
    if known != Ordering::Equal
        && known
            == left
                .projection
                .maximum_bottom_deck_risks()
                .cmp(&right.projection.maximum_bottom_deck_risks())
        && match known {
            Ordering::Less => preserves_protection(left),
            Ordering::Greater => preserves_protection(right),
            Ordering::Equal => false,
        }
    {
        known
    } else {
        Ordering::Equal
    }
}

fn compare_bottom_deck_risks(
    left: &PlannerActionEvaluation,
    right: &PlannerActionEvaluation,
) -> Ordering {
    fn assessed_tail(evidence: &crate::ProjectionEvidence, immediate_discard_risk: bool) -> usize {
        let local = evidence
            .unresolved_discard
            .filter(|discard| {
                discard.required_protection
                    || evidence.steps.is_empty()
                    || (discard.strategically_selected
                        && (!immediate_discard_risk || evidence.resources.tokens == 0))
            })
            .and_then(|_| evidence.forecast_discard_risk())
            .unwrap_or(0);
        evidence
            .branches()
            .map(|branch| assessed_tail(branch, immediate_discard_risk))
            .max()
            .unwrap_or(0)
            .max(local)
    }
    let horizon = left
        .projection
        .risk_horizon()
        .min(right.projection.risk_horizon())
        .max(1);
    // Optional unknown discards at different forecast frontiers are not
    // like-for-like decisions. One line may still clue instead; reaching
    // uncertainty sooner does not demonstrate an avoidable card loss.
    // Immediate root risk and convention-required discards remain relevant.
    let aligned_risk_frontiers = left.projection.risk_horizon() == right.projection.risk_horizon();
    let left_prefix = left
        .projection
        .comparable_bottom_deck_risks_at(horizon, aligned_risk_frontiers);
    let right_prefix = right
        .projection
        .comparable_bottom_deck_risks_at(horizon, aligned_risk_frontiers);
    let prefix = left_prefix.cmp(&right_prefix);
    if prefix == Ordering::Equal {
        let identified = compare_identified_losses(left, right, horizon);
        if identified != Ordering::Equal {
            return identified;
        }
    }
    // A worsened chop remains a liability when a short projection stops
    // before discarding it. It cannot prove risk avoidance, but also must
    // not be counted as an executed loss.
    let unresolved_exposure = |candidate: &PlannerActionEvaluation| {
        candidate.projection.forecast_discard_risk().is_none()
            || candidate.symbolic_line.position_value.is_some_and(|value| {
                value.exposed_chop_quality != crate::SecuredCardQuality::default()
            })
    };
    if (prefix == Ordering::Less && unresolved_exposure(left))
        || (prefix == Ordering::Greater && unresolved_exposure(right))
    {
        return Ordering::Equal;
    }
    // A shared prefix prevents penalizing a longer forecast merely for seeing
    // farther. But it must not certify avoidance when the allegedly safer line
    // already predicts the same committed loss just beyond that cutoff.
    // Choosing an action in a bounded future forecast is not a commitment
    // by that player. When comparing a discard that takes risk NOW against
    // a reversible future choice, a still-available clue matters. Do not use
    // that future choice to cancel the immediate risk. When neither root
    // action takes that risk yet, retain the ordinary like-for-like forecast
    // comparison: simply postponing a forecast is not evidence of avoidance.
    // Preserve actual modeled losses and required protection discards, and
    // keep a selected discard when there is no token to spend instead.
    // Keep unresolved discard risk already inside the prefix: it is risk, not
    // a recorded known-card loss, so the full known-loss counter omits it.
    let immediate_discard_risk = [left, right].iter().any(|candidate| {
        matches!(candidate.action, Action::Discard(_))
            && candidate.projection.bottom_deck_risks_at(1) > 0
    });
    let complete = left
        .projection
        .maximum_bottom_deck_risks()
        .max(assessed_tail(&left.projection, immediate_discard_risk))
        .max(left_prefix)
        .cmp(
            &right
                .projection
                .maximum_bottom_deck_risks()
                .max(assessed_tail(&right.projection, immediate_discard_risk))
                .max(right_prefix),
        );
    if prefix == complete {
        prefix
    } else {
        Ordering::Equal
    }
}

fn compare_save_principle_risks(
    left: &PlannerActionEvaluation,
    right: &PlannerActionEvaluation,
) -> Ordering {
    // An unexpanded unknown discard is not proof of avoiding a loss several
    // turns down another line. Compare equal elapsed time, retaining direct
    // first-action Save Principle violations even at an unknown frontier.
    let horizon = usize::from(
        left.projection
            .common_horizon()
            .min(right.projection.common_horizon()),
    )
    .max(1);
    // A critical-card discard makes the corresponding score unattainable.
    // Do not equate it with risking an otherwise recoverable copy merely
    // because each line contains one Save Principle violation.
    left.projection
        .critical_losses_at(horizon)
        .cmp(&right.projection.critical_losses_at(horizon))
        .then_with(|| {
            left.projection
                .save_violations_at(horizon)
                .cmp(&right.projection.save_violations_at(horizon))
        })
}

/// Prefer taking a certain play when the next teammate can spend their turn
/// on a funded clue with a demonstrated scoring response. This is only a
/// scheduling fallback: endpoint evidence, safety, and urgent policy tiers
/// still take precedence. An unknown or merely promised response earns nothing.
fn has_productive_teammate_handoff(candidate: &PlannerActionEvaluation) -> bool {
    if !candidate.certainly_playable || !matches!(candidate.action, Action::Play(_)) {
        return false;
    }
    let [play, clue, response, ..] = candidate.projection.steps.as_slice() else {
        return false;
    };
    play.projected.action == candidate.action
        && play.consequences.score_gain == 1
        && clue.projected.actor != play.projected.actor
        && matches!(clue.projected.action, Action::Clue { .. })
        && clue.turn == play.turn + 1
        && response.turn == clue.turn + 1
        && response.projected.actor != play.projected.actor
        && matches!(response.projected.action, Action::Play(_))
        && response.consequences.score_gain == 1
        && [play, clue, response].iter().all(|step| {
            step.consequences.strikes == 0
                && step.consequences.bottom_deck_risk.is_none()
                && step.consequences.save_principle_violation.is_none()
        })
        && candidate
            .projection
            .resources
            .transitions
            .iter()
            .any(|transition| {
                transition.turn == clue.turn && transition.spent == 1 && transition.before >= 1
            })
}

fn compare_teammate_handoff(
    left: &PlannerActionEvaluation,
    right: &PlannerActionEvaluation,
) -> Ordering {
    match (left.action, right.action) {
        (Action::Play(_), Action::Clue { .. }) if has_productive_teammate_handoff(left) => {
            Ordering::Greater
        }
        (Action::Clue { .. }, Action::Play(_)) if has_productive_teammate_handoff(right) => {
            Ordering::Less
        }
        _ => Ordering::Equal,
    }
}

#[allow(clippy::too_many_lines)]
fn symbolic_fallback_comparison(
    left: &PlannerActionEvaluation,
    right: &PlannerActionEvaluation,
    preferred: Option<Action>,
) -> (Ordering, ComparisonReason) {
    let resources = left
        .symbolic_line
        .position_value
        .zip(right.symbolic_line.position_value);
    let dimensions = [
        (
            compare_save_principle_risks(right, left),
            ComparisonReason::SavePrinciple,
        ),
        (
            left.preference
                .policy_tier()
                .cmp(&right.preference.policy_tier()),
            ComparisonReason::PolicyTier,
        ),
        (
            right
                .projection
                .maximum_strikes()
                .cmp(&left.projection.maximum_strikes()),
            ComparisonReason::ConditionalStrikes,
        ),
        (
            right.symbolic_line.strikes.cmp(&left.symbolic_line.strikes),
            ComparisonReason::KnownStrikes,
        ),
        (
            compare_bottom_deck_risks(left, right).reverse(),
            ComparisonReason::BottomDeckRisk,
        ),
        (
            left.preference
                .advances_terminal_plan()
                .cmp(&right.preference.advances_terminal_plan()),
            ComparisonReason::TerminalProgress,
        ),
        (
            left.preference.compare_play_order(right.preference),
            ComparisonReason::ConventionPlayOrder,
        ),
        (
            resources.map_or(Ordering::Equal, |(a, b)| compare_save_pressure(a, b)),
            ComparisonReason::SavePressure,
        ),
        (
            resources.map_or(Ordering::Equal, |(a, b)| {
                b.foregone_touch_opportunities
                    .cmp(&a.foregone_touch_opportunities)
            }),
            ComparisonReason::WaitingOpportunity,
        ),
        (
            // A checked discard frontier provides a local risk assessment;
            // an unfinished clue/play is not evidence of zero future losses.
            match (
                left.projection.forecast_discard_risk(),
                right.projection.forecast_discard_risk(),
            ) {
                // These totals cover the whole forecast, unlike the shared-
                // horizon comparison above. Comparing different durations
                // rewards stopping early at an unknown card. Keep full-tail
                // diagnostics without using them to bypass that cutoff.
                (Some(a), Some(b))
                    if left.projection.risk_horizon() == right.projection.risk_horizon() =>
                {
                    b.cmp(&a)
                }
                _ => Ordering::Equal,
            },
            ComparisonReason::ForecastBottomDeckRisk,
        ),
        (
            compare_teammate_handoff(left, right),
            ComparisonReason::TeammateClueHandoff,
        ),
        (
            left.preference
                .within_category()
                .cmp(&right.preference.within_category()),
            ComparisonReason::ConventionPreference,
        ),
        (
            (preferred == Some(left.action)).cmp(&(preferred == Some(right.action))),
            ComparisonReason::PreferredAction,
        ),
        (
            left.symbolic_line.compare(right.symbolic_line),
            ComparisonReason::LineProgress,
        ),
        (
            left.certainly_playable.cmp(&right.certainly_playable),
            ComparisonReason::PlayCertainty,
        ),
        (
            left.certainly_useless.cmp(&right.certainly_useless),
            ComparisonReason::TrashCertainty,
        ),
        (
            left.critical_touched.cmp(&right.critical_touched),
            ComparisonReason::CriticalTouch,
        ),
        (
            left.oldest_card_touched.cmp(&right.oldest_card_touched),
            ComparisonReason::OldestTouch,
        ),
        (
            left.immediately_playable_touched
                .cmp(&right.immediately_playable_touched),
            ComparisonReason::PlayableTouch,
        ),
        (
            left.newly_touched.cmp(&right.newly_touched),
            ComparisonReason::NewTouch,
        ),
        (
            stable_action_key(right.action).cmp(&stable_action_key(left.action)),
            ComparisonReason::StableOrder,
        ),
    ];
    dimensions
        .into_iter()
        .find(|(order, _)| *order != Ordering::Equal)
        .unwrap_or((Ordering::Equal, ComparisonReason::StableOrder))
}

fn compare_save_pressure(a: ProjectedPositionValue, b: ProjectedPositionValue) -> Ordering {
    let preference = b.save_pressure.cmp(&a.save_pressure);
    // Pressure measures possible follow-up Saves. Avoiding that prospective
    // cost by leaving more *current critical chops* exposed is not an
    // improvement. Incomparable positions must use the remaining evidence.
    // Reproduced in p4v0s1 turn 23's rank-2 projection, at projected turn 34.
    // https://hanabi.github.io/beginner/save-principle/
    match preference {
        Ordering::Greater if a.exposed_critical_chops > b.exposed_critical_chops => Ordering::Equal,
        Ordering::Less if b.exposed_critical_chops > a.exposed_critical_chops => Ordering::Equal,
        _ => preference,
    }
}

fn stable_action_key(action: Action) -> (u8, usize, usize) {
    match action {
        Action::Play(card) => (0, card.index(), 0),
        Action::Discard(card) => (1, card.index(), 0),
        Action::Clue {
            target,
            clue: Clue::Suit(suit),
        } => (2, target.index(), suit.index()),
        Action::Clue {
            target,
            clue: Clue::Rank(rank),
        } => (3, target.index(), rank.index()),
    }
}

fn best_exact_index(
    evaluations: &[PlannerActionEvaluation],
    objective: PlanningObjective,
    preferred: Option<Action>,
) -> Option<usize> {
    evaluations
        .iter()
        .enumerate()
        .max_by(|(left_index, left), (right_index, right)| {
            let exact = left
                .exact
                .expect("exact selection follows a complete root solve")
                .compare(
                    right
                        .exact
                        .expect("exact selection follows a complete root solve"),
                    objective,
                );
            exact
                .then_with(|| {
                    left.preference
                        .policy_tier()
                        .cmp(&right.preference.policy_tier())
                })
                .then_with(|| left.preference.cmp(&right.preference))
                .then_with(|| {
                    (preferred == Some(left.action)).cmp(&(preferred == Some(right.action)))
                })
                .then_with(|| right_index.cmp(left_index))
        })
        .map(|(index, _)| index)
}

type ExactRootValues = (Vec<Option<ExactActionValue>>, Option<usize>);

fn evaluate_exact_root(
    worlds: &[FullState],
    convention: SupportedConvention,
    objective: PlanningObjective,
    candidates: &[PlannerActionEvaluation],
    preferred: Option<Action>,
    budget: &mut ExactBudget,
) -> Result<ExactRootValues, ExactAbort> {
    let mut values = vec![None; candidates.len()];
    let upper = exact_value_upper_bound(worlds);
    let mut ordered = candidates.iter().enumerate().collect::<Vec<_>>();
    ordered.sort_by_key(|(index, candidate)| {
        (
            core::cmp::Reverse(candidate.preference.policy_tier()),
            core::cmp::Reverse(candidate.preference),
            core::cmp::Reverse(preferred == Some(candidate.action)),
            *index,
        )
    });
    // Exact branches repeatedly converge on the same public observation.
    // Convention interpretation is a pure function of that observation, so
    // compile it once for the whole solve instead of replaying H-Group history
    // independently in every identity world and branch.
    let mut analysis_cache = ConventionAnalysisCache::default();
    for (index, candidate) in ordered {
        budget.consume()?;
        let mut advanced = Vec::with_capacity(worlds.len());
        for world in worlds {
            let mut state = world.clone();
            state.apply(candidate.action).map_err(ExactAbort::Rule)?;
            advanced.push(state);
        }
        let value = solve_partitioned(
            advanced,
            convention,
            objective,
            budget,
            &mut analysis_cache,
            1,
        )?;
        values[index] = Some(value);
        if value.compare(upper, objective) == Ordering::Equal {
            return Ok((values, Some(index)));
        }
    }
    Ok((values, None))
}

fn solve_partitioned(
    worlds: Vec<FullState>,
    convention: SupportedConvention,
    objective: PlanningObjective,
    budget: &mut ExactBudget,
    analysis_cache: &mut ConventionAnalysisCache,
    depth: u16,
) -> Result<ExactActionValue, ExactAbort> {
    if depth > 512 {
        return Err(ExactAbort::DepthExceeded);
    }
    let mut terminal = ExactActionValue {
        worlds: 0,
        perfect_worlds: 0,
        score_sum: 0,
        strikeout_worlds: 0,
        score_ceiling_sum: 0,
    };
    let mut groups: Vec<(PlayerView, Vec<FullState>)> = Vec::new();
    let mut group_indices: HashMap<PlayerView, usize> = HashMap::new();
    for world in worlds {
        if world.is_terminal() {
            terminal.add(terminal_value(&world));
            continue;
        }
        let view = world
            .view_for(world.current_player())
            .ok_or(ExactAbort::InvalidCurrentPlayer)?;
        if let Some(index) = group_indices.get(&view).copied() {
            groups[index].1.push(world);
        } else {
            group_indices.insert(view.clone(), groups.len());
            groups.push((view, vec![world]));
        }
    }
    for (view, group) in groups {
        terminal.add(solve_observation_group(
            view,
            &group,
            convention,
            objective,
            budget,
            analysis_cache,
            depth,
        )?);
    }
    Ok(terminal)
}

fn solve_observation_group(
    view: PlayerView,
    worlds: &[FullState],
    convention: SupportedConvention,
    objective: PlanningObjective,
    budget: &mut ExactBudget,
    analysis_cache: &mut ConventionAnalysisCache,
    depth: u16,
) -> Result<ExactActionValue, ExactAbort> {
    if let Some(value) = equivalent_terminal_actions(&view, worlds, objective)? {
        return Ok(value);
    }
    budget.control.checkpoint().map_err(ExactAbort::Stopped)?;
    let analysis = analysis_cache.compile(view, convention)?;
    budget.control.checkpoint().map_err(ExactAbort::Stopped)?;
    let preferred = analysis.preferred_action;
    let candidates = planning_candidates(&analysis);
    if candidates.is_empty() {
        return Err(ExactAbort::NoCandidateActions);
    }

    // No continuation can recover a lost stack or score above its current
    // ceiling. Visit tie-break-preferred actions first, so reaching this bound
    // proves that remaining candidates cannot improve either value or ties.
    let upper_bound = exact_value_upper_bound(worlds);
    let mut ordered = candidates.iter().copied().enumerate().collect::<Vec<_>>();
    ordered.sort_by_key(|(index, candidate)| {
        (
            core::cmp::Reverse(candidate.preference),
            core::cmp::Reverse(preferred == Some(candidate.action)),
            *index,
        )
    });
    let mut best: Option<(ExactActionValue, crate::ActionPreference, bool, usize)> = None;
    for (index, candidate) in ordered {
        let action = candidate.action;
        budget.consume()?;
        let mut advanced = Vec::with_capacity(worlds.len());
        for world in worlds {
            let mut state = world.clone();
            state.apply(action).map_err(ExactAbort::Rule)?;
            advanced.push(state);
        }
        let value = solve_partitioned(
            advanced,
            convention,
            objective,
            budget,
            analysis_cache,
            depth + 1,
        )?;
        let priority = candidate.preference;
        let is_preferred = preferred == Some(action);
        let replace = best.as_ref().is_none_or(
            |(current, current_priority, current_preferred, current_index)| {
                value
                    .compare(*current, objective)
                    .then_with(|| priority.cmp(current_priority))
                    .then_with(|| is_preferred.cmp(current_preferred))
                    .then_with(|| current_index.cmp(&index))
                    == Ordering::Greater
            },
        );
        if replace {
            best = Some((value, priority, is_preferred, index));
        }
        if value.compare(upper_bound, objective) == Ordering::Equal {
            break;
        }
    }
    best.map(|(value, _, _, _)| value)
        .ok_or(ExactAbort::NoCandidateActions)
}

/// Avoid convention compilation only when every rules-legal action ends every
/// world with the same utility. Any convention-admissible subset must then have
/// that value too; this does not relax the root player's convention constraints.
fn equivalent_terminal_actions(
    view: &PlayerView,
    worlds: &[FullState],
    objective: PlanningObjective,
) -> Result<Option<ExactActionValue>, ExactAbort> {
    if view.final_turns_remaining != Some(1) {
        return Ok(None);
    }
    let mut common: Option<ExactActionValue> = None;
    for action in view.legal_actions() {
        let mut value = ExactActionValue {
            worlds: 0,
            perfect_worlds: 0,
            score_sum: 0,
            strikeout_worlds: 0,
            score_ceiling_sum: 0,
        };
        for world in worlds {
            let mut advanced = world.clone();
            advanced.apply(action).map_err(ExactAbort::Rule)?;
            if !advanced.is_terminal() {
                return Ok(None);
            }
            value.add(terminal_value(&advanced));
        }
        if common.is_some_and(|previous| value.compare(previous, objective) != Ordering::Equal) {
            return Ok(None);
        }
        common = Some(value);
    }
    Ok(common)
}

/// Optimistic final-round score with omniscient players and free passes.
/// Each remaining player acts at most once and there are no further draws.
/// This is only an upper bound: actual decisions still share observations
/// across worlds and must obey their convention constraints.
fn exact_value_upper_bound(worlds: &[FullState]) -> ExactActionValue {
    let mut bound = ExactActionValue {
        worlds: 0,
        perfect_worlds: 0,
        score_sum: 0,
        strikeout_worlds: 0,
        score_ceiling_sum: 0,
    };
    for world in worlds {
        let ceiling = u64::from(score_ceiling(world));
        let reachable = final_round_score_bound(world).map_or(ceiling, |value| ceiling.min(value));
        bound.worlds += 1;
        bound.perfect_worlds += u64::from(reachable == 25);
        bound.score_sum += reachable;
        bound.score_ceiling_sum += ceiling;
    }
    bound
}

fn final_round_score_bound(world: &FullState) -> Option<u64> {
    let remaining = world.final_turns_remaining()?;
    let heights = world
        .play_stacks()
        .each_ref()
        .map(|stack| u8::try_from(stack.len()).expect("standard stack"));
    Some(
        u64::from(world.score())
            + u64::from(final_round_max_plays(
                world,
                heights,
                world.current_player().index(),
                remaining,
            )),
    )
}

fn final_round_max_plays(world: &FullState, heights: [u8; 5], actor: usize, remaining: u8) -> u8 {
    if remaining == 0 {
        return 0;
    }
    let next = (actor + 1) % world.hands().len();
    let mut best = final_round_max_plays(world, heights, next, remaining - 1);
    for card in &world.hands()[actor] {
        let identity = world.card(*card).expect("world hand card");
        if identity.rank.number() == heights[identity.suit.index()] + 1 {
            let mut advanced = heights;
            advanced[identity.suit.index()] += 1;
            best = best.max(1 + final_round_max_plays(world, advanced, next, remaining - 1));
        }
    }
    best
}

/// Per-solve cache for the pure observer-relative convention compiler.
/// Keeping this local to one exact search avoids global mutable state while
/// allowing identity-world branches with the same observation to share the
/// expensive history reduction.
#[derive(Default)]
struct ConventionAnalysisCache {
    entries: HashMap<PlayerView, std::rc::Rc<ConventionAnalysis>>,
    #[cfg(test)]
    compilations: usize,
}

impl ConventionAnalysisCache {
    fn compile(
        &mut self,
        view: PlayerView,
        convention: SupportedConvention,
    ) -> Result<std::rc::Rc<ConventionAnalysis>, ExactAbort> {
        if let Some(cached) = self.entries.get(&view) {
            return Ok(cached.clone());
        }
        let deductions =
            LogicalDeductions::new(view.clone()).map_err(ExactAbort::InformationSet)?;
        let compiled = std::rc::Rc::new(convention.analyze(&deductions));
        self.entries.insert(view, compiled.clone());
        #[cfg(test)]
        {
            self.compilations += 1;
        }
        Ok(compiled)
    }
}

fn terminal_value(state: &FullState) -> ExactActionValue {
    let score = state
        .final_score()
        .expect("terminal states have an official score");
    ExactActionValue {
        worlds: 1,
        perfect_worlds: u64::from(score == 25),
        score_sum: u64::from(score),
        strikeout_worlds: u64::from(matches!(
            state.status(),
            GameStatus::Finished(hanabi_core::EndReason::TooManyStrikes)
        )),
        score_ceiling_sum: u64::from(score_ceiling(state)),
    }
}

struct ExactBudget<'a> {
    used: u64,
    limit: u64,
    control: &'a crate::AnalysisControl,
}

impl ExactBudget<'_> {
    fn consume(&mut self) -> Result<(), ExactAbort> {
        self.control.checkpoint().map_err(ExactAbort::Stopped)?;
        if self.used >= self.limit {
            return Err(ExactAbort::BudgetExceeded);
        }
        self.used += 1;
        Ok(())
    }
}

#[derive(Debug)]
enum ExactAbort {
    Stopped(crate::AnalysisStopped),
    BudgetExceeded,
    DepthExceeded,
    InvalidCurrentPlayer,
    NoCandidateActions,
    InformationSet(InformationSetError),
    Rule(RuleError),
}

/// Failure returned by deterministic planning.
#[derive(Debug, PartialEq)]
pub enum PlannerError {
    Stopped(crate::AnalysisStopped),
    ConventionBeliefConflict,
    NoCandidateActions,
    EnumerateWorlds(EnumerateWorldsError),
    InformationSet(InformationSetError),
    Rule(RuleError),
}

impl fmt::Display for PlannerError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Stopped(error) => error.fmt(formatter),
            Self::ConventionBeliefConflict => formatter.write_str(
                "convention identity constraints contradict the logical information set",
            ),
            Self::NoCandidateActions => formatter.write_str("position has no candidate actions"),
            Self::EnumerateWorlds(error) => write!(formatter, "cannot enumerate belief: {error}"),
            Self::InformationSet(error) => write!(formatter, "invalid observation: {error}"),
            Self::Rule(error) => write!(formatter, "planned action was illegal: {error}"),
        }
    }
}

impl std::error::Error for PlannerError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reviewed_turn_eighteen_leaves_the_clue_to_a_teammate() {
        let replay = hanabi_protocol::HanabiLiveReplay::from_json(include_str!(
            "../../hanabi-protocol/tests/fixtures/game-p4v0s1.json"
        ))
        .unwrap();
        let state = replay.state_at_turn(17).unwrap();
        let view = state.view_for(state.current_player()).unwrap();
        let analysis = crate::analyze_position(
            &view,
            SupportedConvention::HGroup(crate::HGroupProfile::Max),
            PlannerConfig {
                objective: PlanningObjective::PerfectScore,
                ..PlannerConfig::default()
            },
        )
        .unwrap();
        let play = Action::Play(hanabi_core::CardId::new(17));
        let candidate = analysis
            .planner
            .root_actions
            .iter()
            .find(|candidate| candidate.action == play)
            .unwrap();
        // User-reviewed p4v0s1 turn 18: Bob plays b5; Cathy's funded
        // 4s-to-Alice bluff gets Donald's r1 played without using hidden draws.
        assert!(candidate.certainly_playable);
        assert_eq!(
            candidate.projection.steps[1].projected.actor,
            hanabi_core::PlayerId::new(2)
        );
        assert_eq!(
            candidate.projection.steps[1].projected.action,
            Action::Clue {
                target: hanabi_core::PlayerId::new(0),
                clue: Clue::Rank(Rank::Four),
            }
        );
        assert_eq!(
            candidate.projection.steps[2].projected.action,
            Action::Play(hanabi_core::CardId::new(23))
        );
        assert_eq!(candidate.projection.steps[2].consequences.score_gain, 1);
        let purple = analysis
            .planner
            .root_actions
            .iter()
            .find(|candidate| {
                candidate.action
                    == Action::Clue {
                        target: hanabi_core::PlayerId::new(3),
                        clue: Clue::Suit(Suit::Purple),
                    }
            })
            .unwrap();
        assert_eq!(
            symbolic_fallback_comparison(candidate, purple, None),
            (Ordering::Greater, ComparisonReason::TeammateClueHandoff)
        );
        assert_eq!(
            symbolic_fallback_comparison(purple, candidate, None),
            (Ordering::Less, ComparisonReason::TeammateClueHandoff)
        );
        assert_eq!(
            best_symbolic_index(&[purple.clone(), candidate.clone()], None),
            Some(1)
        );

        // Evidence ablations of this reviewed position, not invented histories.
        // Unknown cards and unfunded/unfinished continuations cannot justify
        // handing off a clue. Nor does an ordinary discard earn this preference.
        for missing in [
            "certainty",
            "clue",
            "response",
            "funding",
            "scoring",
            "safety",
        ] {
            let mut incomplete = candidate.clone();
            match missing {
                "certainty" => incomplete.certainly_playable = false,
                "clue" => {
                    incomplete.projection.steps[1].projected.action =
                        Action::Discard(hanabi_core::CardId::new(9));
                }
                "response" => incomplete.projection.steps.truncate(2),
                "funding" => incomplete.projection.resources.transitions.clear(),
                "scoring" => incomplete.projection.steps[2].consequences.score_gain = 0,
                "safety" => incomplete.projection.steps[2].consequences.strikes = 1,
                _ => unreachable!(),
            }
            assert!(!has_productive_teammate_handoff(&incomplete), "{missing}");
            assert_eq!(
                compare_teammate_handoff(&incomplete, purple),
                Ordering::Equal,
                "{missing}"
            );
        }
        let mut urgent = purple.clone();
        urgent.preference = urgent
            .preference
            .with_policy_tier(ConventionPolicyTier::Required);
        assert_eq!(
            symbolic_fallback_comparison(candidate, &urgent, None),
            (Ordering::Less, ComparisonReason::PolicyTier)
        );
        assert_eq!(analysis.planner.best_action, play);
    }

    #[test]
    fn pending_save_credit_requires_funding_and_never_creates_playable_points() {
        // Arithmetic invariant, not a synthetic convention position.
        let saved = ProjectedPositionValue {
            score: 17,
            secured_future_plays: 4,
            protected_bottom_deck_risks: 6,
            clues: 3,
            ..ProjectedPositionValue::default()
        };
        let deferred = ProjectedPositionValue {
            secured_future_plays: 3,
            protected_bottom_deck_risks: 5,
            exposed_critical_chops: 1,
            clue_demand: 1,
            ..saved
        };
        assert!(deferred.preserves_funded_progress(saved));
        assert!(!deferred.productive_preference(saved));
        for invalid in [
            ProjectedPositionValue {
                clues: 0,
                ..deferred
            },
            ProjectedPositionValue {
                clue_demand: 0,
                ..deferred
            },
            ProjectedPositionValue {
                exposed_critical_chops: 0,
                ..deferred
            },
            ProjectedPositionValue {
                secured_future_plays: 2,
                ..deferred
            },
            ProjectedPositionValue {
                save_pressure: 1,
                ..deferred
            },
        ] {
            assert!(!invalid.preserves_funded_progress(saved), "{invalid:?}");
        }
        assert!(!deferred.preserves_funded_progress(ProjectedPositionValue {
            committed_future_plays: 1,
            ..saved
        }));
    }

    #[test]
    fn funded_progress_precedes_surplus_tokens_but_not_required_funding() {
        let productive = ProjectedPositionValue {
            score: 19,
            clues: 2,
            clue_demand: 1,
            ..ProjectedPositionValue::default()
        };
        let discard = ProjectedPositionValue {
            score: 17,
            clues: 4,
            clue_demand: 1,
            ..ProjectedPositionValue::default()
        };
        assert!(productive.productive_preference(discard));
        assert!(
            ProjectedPositionValue {
                clues: 0,
                clue_demand: 0,
                ..productive
            }
            .productive_preference(discard)
        );
        assert!(!discard.productive_preference(productive));
        assert!(!productive.productive_preference(ProjectedPositionValue {
            protected_bottom_deck_risks: 1,
            ..discard
        }));
        assert!(
            !ProjectedPositionValue {
                clues: 0,
                ..productive
            }
            .productive_preference(discard)
        );
        assert!(
            ProjectedPositionValue {
                exposed_critical_chops: 1,
                ..productive
            }
            .productive_preference(discard)
        );
        assert!(
            !ProjectedPositionValue {
                exposed_critical_chops: 1,
                clues: 0,
                ..productive
            }
            .productive_preference(discard)
        );
        assert!(
            !ProjectedPositionValue {
                exposed_critical_chops: 2,
                ..productive
            }
            .productive_preference(discard)
        );
        assert!(
            !ProjectedPositionValue {
                secured_future_plays: 2,
                ..discard
            }
            .productive_preference(discard)
        );
        assert!(
            !ProjectedPositionValue {
                finesse_opportunities: 2,
                ..discard
            }
            .productive_preference(discard)
        );
    }

    #[test]
    fn save_pressure_cannot_reward_leaving_a_critical_chop_exposed() {
        let saved = ProjectedPositionValue {
            save_pressure: 1,
            ..ProjectedPositionValue::default()
        };
        let exposed = ProjectedPositionValue {
            exposed_critical_chops: 1,
            ..ProjectedPositionValue::default()
        };
        assert_eq!(compare_save_pressure(saved, exposed), Ordering::Equal);
        assert_eq!(compare_save_pressure(exposed, saved), Ordering::Equal);
        let equally_protected = ProjectedPositionValue::default();
        assert_eq!(
            compare_save_pressure(equally_protected, saved),
            Ordering::Greater
        );
        assert_eq!(
            compare_save_pressure(saved, equally_protected),
            Ordering::Less
        );
    }

    #[test]
    fn reviewed_turn_fourteen_preserves_known_card_over_speculative_discard() {
        // User-reviewed p4v0s1 turn 14: green protects p4. At future
        // efficiency 0.71 there is no need to sacrifice it for red's extra
        // efficiency. Analyze only Bob's view, not future deck identities.
        let replay = hanabi_protocol::HanabiLiveReplay::from_json(include_str!(
            "h_group/tests/fixtures/game-p4v0s1-before-turn18-revision.json"
        ))
        .unwrap();
        let state = replay.state_at_turn(13).unwrap();
        let analysis = crate::analyze_position(
            &state.view_for(state.current_player()).unwrap(),
            SupportedConvention::HGroup(crate::HGroupProfile::Max),
            PlannerConfig::default(),
        )
        .unwrap();
        let clue = |suit| Action::Clue {
            target: hanabi_core::PlayerId::new(3),
            clue: hanabi_core::Clue::Suit(suit),
        };
        let candidate = |suit| {
            analysis
                .planner
                .root_actions
                .iter()
                .find(|c| c.action == clue(suit))
                .unwrap()
        };
        let red = candidate(hanabi_core::Suit::Red);
        let green = candidate(hanabi_core::Suit::Green);
        assert!(red.projection.steps.iter().any(|step| {
            step.projected.action == Action::Discard(hanabi_core::CardId::new(12))
                && step.consequences.bottom_deck_risk.is_some()
        }));
        assert_eq!(green.projection.maximum_bottom_deck_risks(), 0);
        assert_eq!(compare_bottom_deck_risks(red, green), Ordering::Greater);
        assert_eq!(compare_bottom_deck_risks(green, red), Ordering::Less);
        assert_eq!(analysis.planner.best_action, clue(hanabi_core::Suit::Green));
    }

    #[test]
    fn reviewed_turn_eleven_optional_future_discard_does_not_cancel_immediate_risk() {
        // User-reviewed p4v0s1 turn 11: Cathy risks an unknown card now;
        // Donald's later unknown discard remains optional with a clue left.
        // No hidden hand identity or deck order is supplied to planning.
        let replay = hanabi_protocol::HanabiLiveReplay::from_json(include_str!(
            "h_group/tests/fixtures/game-p4v0s1-before-turn18-revision.json"
        ))
        .unwrap();
        let state = replay.state_at_turn(10).unwrap();
        let view = state.view_for(state.current_player()).unwrap();
        let analysis = crate::analyze_position(
            &view,
            SupportedConvention::HGroup(crate::HGroupProfile::Max),
            PlannerConfig::default(),
        )
        .unwrap();
        assert!(
            analysis
                .information
                .deductions()
                .possible_identities(hanabi_core::CardId::new(9))
                .unwrap()
                .contains(hanabi_core::Card::new(
                    hanabi_core::Suit::Green,
                    hanabi_core::Rank::Three
                ))
        );
        let discard = analysis
            .planner
            .root_actions
            .iter()
            .find(|c| c.action == Action::Discard(hanabi_core::CardId::new(9)))
            .unwrap();
        let stall_action = Action::Clue {
            target: hanabi_core::PlayerId::new(1),
            clue: hanabi_core::Clue::Rank(hanabi_core::Rank::Five),
        };
        let stall = analysis
            .planner
            .root_actions
            .iter()
            .find(|c| c.action == stall_action)
            .unwrap();
        assert!(discard.projection.steps.is_empty());
        assert!(
            discard
                .projection
                .unresolved_discard
                .unwrap()
                .bottom_deck_risk
        );
        assert_eq!(stall.projection.resources.tokens, 1);
        let future = stall.projection.unresolved_discard.unwrap();
        assert!(future.bottom_deck_risk);
        assert!(!future.required_protection);
        assert_eq!(compare_bottom_deck_risks(discard, stall), Ordering::Greater);
        assert_eq!(compare_bottom_deck_risks(stall, discard), Ordering::Less);
        assert_eq!(analysis.planner.best_action, stall_action);
        // This distinction must not erase a required future sacrifice or a
        // discard selected at a frontier where no clue token remains.
        let mut required = stall.clone();
        required
            .projection
            .unresolved_discard
            .as_mut()
            .unwrap()
            .required_protection = true;
        assert_eq!(
            compare_bottom_deck_risks(discard, &required),
            Ordering::Equal
        );
        let mut no_token = stall.clone();
        no_token.projection.resources.tokens = 0;
        assert_eq!(
            compare_bottom_deck_risks(discard, &no_token),
            Ordering::Equal
        );
    }

    #[test]
    fn held_card_development_cannot_buy_away_realized_progress() {
        // Comparator invariant, not an invented convention history.
        let played = ProjectedPositionValue {
            score: 3,
            secured_future_plays: 2,
            clues: 5,
            clue_demand: 1,
            ..Default::default()
        };
        let queued = ProjectedPositionValue {
            score: 2,
            secured_future_plays: 4,
            clues: 4,
            ..played
        };
        assert!(!queued.developed_points_preference(played));
        assert!(!played.developed_points_preference(queued));
        // Once actual plays catch up, additional secured cards still matter.
        assert!(ProjectedPositionValue { score: 3, ..queued }.developed_points_preference(played));
    }

    #[test]
    #[allow(clippy::too_many_lines)]
    fn delayed_known_bdr_is_not_avoidance_at_the_shared_cutoff() {
        // Comparison invariant, not an invented convention/strategy history.
        let replay = hanabi_protocol::HanabiLiveReplay::from_json(include_str!(
            "../../hanabi-protocol/tests/fixtures/game-p4v0s2.json"
        ))
        .unwrap();
        let state = replay.state_at_turn(1).unwrap();
        let d = LogicalDeductions::new(state.view_for(state.current_player()).unwrap()).unwrap();
        let mut early = symbolic_evaluation(
            &d,
            ConventionAction {
                action: Action::Discard(hanabi_core::CardId::new(4)),
                preference: crate::ActionPreference::new(0, false),
                reason: crate::ConventionActionReason::Fallback,
            },
        );
        let mut delayed = early.clone();
        let risk = crate::PlanStep {
            interpreted_identities: None,
            turn: 1,
            projected: crate::ProjectedAction {
                actor: state.current_player(),
                action: early.action,
            },
            depends_on: None,
            consequences: crate::ProjectedConsequences {
                bottom_deck_risk: Some(hanabi_core::Card::new(
                    hanabi_core::Suit::Red,
                    hanabi_core::Rank::Four,
                )),
                ..Default::default()
            },
        };
        early.projection.steps = vec![risk];
        early.projection.checkpoints = vec![RotationCheckpoint {
            actions: 1,
            discards: 1,
            value: ProjectedPositionValue::default(),
        }];
        delayed.projection.steps = vec![
            crate::PlanStep {
                consequences: crate::ProjectedConsequences::default(),
                ..risk
            },
            risk,
        ];
        delayed.projection.checkpoints = vec![RotationCheckpoint {
            actions: 2,
            discards: 1,
            value: ProjectedPositionValue::default(),
        }];
        assert_eq!(compare_bottom_deck_risks(&early, &delayed), Ordering::Equal);
        assert_eq!(compare_bottom_deck_risks(&delayed, &early), Ordering::Equal);
        // Both forecasts contain a loss, but the shorter one reaches it
        // first. Better resources before that loss cannot certify dominance.
        let mut resource_rich = early.clone();
        let mut safer_prefix = delayed.clone();
        resource_rich.symbolic_line.actions = 1;
        safer_prefix.symbolic_line.actions = 2;
        resource_rich.projection.checkpoints[0].value.score = 1;
        safer_prefix.projection.checkpoints.insert(
            0,
            RotationCheckpoint {
                actions: 1,
                discards: 0,
                value: ProjectedPositionValue::default(),
            },
        );
        assert_eq!(
            compare_endpoint_evidence(&resource_rich, &safer_prefix, &mut None),
            EndpointComparison::PreferLeft(ComparisonReason::FundedProgress)
        );
        assert_eq!(
            compare_endpoints(&resource_rich, &safer_prefix),
            EndpointComparison::Incomparable
        );
        assert_eq!(
            compare_endpoints(&safer_prefix, &resource_rich),
            EndpointComparison::Incomparable
        );
        // Reaching an unresolved discard sooner is not a recorded loss.
        // Both continuations assess the same possible BDR; its later arrival
        // must not erase already demonstrated progress (p4v0s1 turn 23).
        let mut uncertain_early = resource_rich.clone();
        let mut uncertain_later = safer_prefix.clone();
        let (_, unknown) = crate::h_group::symbolic_line::project_leaf_projection(
            d.view(),
            crate::HGroupProfile::Max,
            early.action,
            &crate::AnalysisControl::default(),
        )
        .unwrap();
        for candidate in [&mut uncertain_early, &mut uncertain_later] {
            for step in &mut candidate.projection.steps {
                step.consequences.bottom_deck_risk = None;
            }
            candidate.projection.unresolved_discard = unknown.unresolved_discard;
            let discard = candidate.projection.unresolved_discard.as_mut().unwrap();
            discard.bottom_deck_risk = true;
            discard.required_protection = false;
            discard.strategically_selected = true;
        }
        assert_eq!(
            compare_endpoints(&uncertain_early, &uncertain_later),
            EndpointComparison::PreferLeft(ComparisonReason::FundedProgress)
        );
        delayed.projection.steps[1].consequences.bottom_deck_risk = None;
        delayed.projection.checkpoints[0].value.exposed_chop_quality =
            crate::SecuredCardQuality::from_cards([
                crate::future_card_quality::FutureCardQuality {
                    rank: hanabi_core::Rank::Three,
                    missing_predecessors: 2,
                    visible_successor: false,
                },
            ]);
        delayed.symbolic_line.position_value = Some(delayed.projection.checkpoints[0].value);
        assert_eq!(
            compare_bottom_deck_risks(&early, &delayed),
            Ordering::Equal,
            "stopping before a newly exposed chop is discarded does not remove its risk"
        );
        delayed.projection.checkpoints[0].value.exposed_chop_quality =
            crate::SecuredCardQuality::default();
        delayed.symbolic_line.position_value = Some(delayed.projection.checkpoints[0].value);
        delayed.projection.frontier = crate::PlanFrontier::Terminal;
        assert_eq!(
            compare_bottom_deck_risks(&early, &delayed),
            Ordering::Greater
        );
        early.projection.checkpoints[0].actions = 0;
        early.projection.steps[0].consequences.bottom_deck_risk = None;
        delayed.projection.steps[1].consequences.bottom_deck_risk =
            risk.consequences.bottom_deck_risk;
        assert_eq!(
            compare_bottom_deck_risks(&early, &delayed),
            Ordering::Equal,
            "a longer speculative tail alone must not penalize a candidate"
        );
        assert!(
            unknown
                .unresolved_discard
                .is_some_and(|discard| discard.bottom_deck_risk)
        );
        delayed.projection = unknown;
        early.projection.frontier = crate::PlanFrontier::Terminal;
        assert_eq!(
            compare_bottom_deck_risks(&early, &delayed),
            Ordering::Less,
            "preserve risk at an unresolved discard inside the shared prefix"
        );
    }

    #[test]
    fn reviewed_turn_eleven_self_bluff_avoids_the_blue_three_bottom_deck_risk() {
        let replay = hanabi_protocol::HanabiLiveReplay::from_json(include_str!(
            "../../hanabi-protocol/tests/fixtures/game-p4v0s3.json"
        ))
        .unwrap();
        let state = replay.state_at_turn(10).unwrap();
        let view = state.view_for(state.current_player()).unwrap();
        let information = InformationSet::new(&view).unwrap();
        let result = plan_move(
            &information,
            SupportedConvention::HGroup(crate::HGroupProfile::Max),
            PlannerConfig::default(),
        )
        .unwrap();
        let three = Action::Clue {
            target: hanabi_core::PlayerId::new(3),
            clue: Clue::Rank(hanabi_core::Rank::Three),
        };
        let four = Action::Clue {
            target: hanabi_core::PlayerId::new(1),
            clue: Clue::Rank(hanabi_core::Rank::Four),
        };
        let find = |action| {
            result
                .root_actions
                .iter()
                .find(|root| root.action == action)
                .unwrap()
        };
        assert_eq!(find(three).projection.maximum_bottom_deck_risks(), 0);
        assert!(find(four).projection.maximum_bottom_deck_risks() > 0);
        let unknown_discard = find(Action::Discard(hanabi_core::CardId::new(9)));
        assert!(unknown_discard.projection.steps.is_empty());
        assert_eq!(
            unknown_discard
                .projection
                .unresolved_discard
                .map(|discard| (discard.card, discard.bottom_deck_risk)),
            Some((hanabi_core::CardId::new(9), true))
        );
        assert_eq!(
            unknown_discard.projection.maximum_bottom_deck_risks(),
            0,
            "an unknown identity must not become a known loss"
        );
        assert!(matches!(
            compare_endpoints(find(three), find(four)),
            EndpointComparison::PreferLeft(
                ComparisonReason::BottomDeckRisk | ComparisonReason::SavePrinciple
            )
        ));
        assert_eq!(result.best_action, three, "{:#?}", result.comparisons);
    }

    #[test]
    fn save_violation_cannot_be_outvoted_by_priority_or_a_trimmed_summary() {
        let replay = hanabi_protocol::HanabiLiveReplay::from_json(include_str!(
            "../../hanabi-protocol/tests/fixtures/game-p4v0s3.json"
        ))
        .unwrap();
        let state = replay.state_at_turn(13).unwrap();
        let d = LogicalDeductions::new(state.view_for(state.current_player()).unwrap()).unwrap();
        let safe = symbolic_evaluation(
            &d,
            ConventionAction {
                action: Action::Clue {
                    target: hanabi_core::PlayerId::new(2),
                    clue: Clue::Suit(hanabi_core::Suit::Green),
                },
                preference: crate::ActionPreference::new(1, false),
                reason: crate::ConventionActionReason::Fallback,
            },
        );
        let mut unsafe_line = safe.clone();
        unsafe_line.action = Action::Clue {
            target: hanabi_core::PlayerId::new(2),
            clue: Clue::Rank(hanabi_core::Rank::Two),
        };
        unsafe_line.preference = crate::ActionPreference::new(9999, false);
        // Algorithm invariant: this tail lies beyond the aggregate summary.
        unsafe_line.projection.steps.push(crate::PlanStep {
            interpreted_identities: None,
            turn: 14,
            projected: crate::ProjectedAction {
                actor: hanabi_core::PlayerId::new(2),
                action: Action::Discard(hanabi_core::CardId::new(10)),
            },
            depends_on: None,
            consequences: crate::ProjectedConsequences {
                save_principle_violation: Some(
                    crate::SavePrincipleViolation::UniqueDelayedPlayable,
                ),
                ..Default::default()
            },
        });
        assert_eq!(best_symbolic_index(&[safe, unsafe_line], None), Some(0));
    }

    #[test]
    fn completed_plan_token_reserve_requires_all_points_and_unblocked_plays() {
        let mut value = ProjectedPositionValue {
            score: 23,
            secured_future_plays: 2,
            clue_demand: 1,
            ..ProjectedPositionValue::default()
        };
        assert_eq!(value.funded_completion_reserve(4), Some(6));
        value.score -= 1;
        assert_eq!(value.funded_completion_reserve(4), None);
        value.score += 1;
        value.blocked_clued_cards = 1;
        assert_eq!(value.funded_completion_reserve(4), None);
    }

    #[test]
    fn additional_protection_cannot_hide_preexisting_congestion_or_token_cost() {
        let exposed = ProjectedPositionValue {
            score: 19,
            clues: 4,
            exposed_critical_chops: 1,
            blocked_clued_cards: 3,
            secured_future_plays: 3,
            protected_bottom_deck_risks: 3,
            ..ProjectedPositionValue::default()
        };
        let mut protected = ProjectedPositionValue {
            exposed_critical_chops: 0,
            blocked_clued_cards: 4,
            secured_future_plays: 4,
            protected_bottom_deck_risks: 4,
            ..exposed
        };
        assert!(protected.protection_development_preference(exposed));
        protected.blocked_clued_cards += 1;
        assert!(!protected.protection_development_preference(exposed));
        protected.blocked_clued_cards -= 1;
        protected.clues -= 1;
        assert!(!protected.protection_development_preference(exposed));
    }

    #[test]
    fn rotation_development_preserves_resources_and_separates_opportunity_from_proof() {
        let efficient = ProjectedPositionValue {
            score: 10,
            secured_future_plays: 4,
            clues: 1,
            clue_demand: 1,
            ..ProjectedPositionValue::default()
        };
        let mut extra_discard = efficient;
        extra_discard.clues = 2;
        assert!(efficient.development_preference(extra_discard, 1, 2));
        extra_discard.clue_demand = 2;
        assert!(!efficient.development_preference(extra_discard, 1, 2));
        let mut opportunity = efficient;
        opportunity.playable_finesse_opportunities = 1;
        assert!(
            !opportunity.dominates(efficient),
            "an option is not a secured point"
        );
        assert!(!efficient.dominates(opportunity));
        opportunity.score -= 1;
        assert!(!opportunity.development_preference(efficient, 1, 2));
    }

    #[test]
    fn conditional_opportunities_cannot_spend_needed_tokens_or_known_progress() {
        let opportunity = ProjectedPositionValue {
            clues: 2,
            clue_demand: 1,
            conditional_successors: 1,
            ..ProjectedPositionValue::default()
        };
        let mut refund = ProjectedPositionValue {
            clues: 3,
            clue_demand: 1,
            ..ProjectedPositionValue::default()
        };
        assert_eq!(
            opportunity.conditional_continuation_preference(refund),
            Some(true)
        );
        assert_eq!(
            refund.conditional_continuation_preference(opportunity),
            Some(false)
        );
        refund.clue_demand = 3;
        assert_eq!(
            opportunity.conditional_continuation_preference(refund),
            None
        );
        assert!(refund.dominates(opportunity));
        refund.clue_demand = 1;
        refund.score = 1;
        assert_eq!(
            opportunity.conditional_continuation_preference(refund),
            None
        );
        assert!(refund.dominates(opportunity));
    }
    use crate::SupportedConvention;
    use hanabi_core::{PlayerId, standard_deck};

    #[test]
    fn partial_endpoint_pruning_preserves_the_reviewed_yellow_clue() {
        // Reviewed p4v0s415 turn 26: y4 gives Cathy a y5 continuation.
        // A partial b5 endpoint's extra token must not eliminate y4 and
        // thereby hand the decision to an unprojected discard.
        let replay = hanabi_protocol::HanabiLiveReplay::from_json(include_str!(
            "../../hanabi-protocol/tests/fixtures/game-p4v0s415.json"
        ))
        .unwrap();
        let state = replay.state_at_turn(25).unwrap();
        let information =
            InformationSet::new(&state.view_for(state.current_player()).unwrap()).unwrap();
        let result = plan_move(
            &information,
            SupportedConvention::HGroup(crate::HGroupProfile::Max),
            PlannerConfig::default(),
        )
        .unwrap();
        let yellow = Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Suit(Suit::Yellow),
        };
        let blue = Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Suit(Suit::Blue),
        };
        assert_eq!(result.best_action, yellow);
        // A genuinely acyclic pair retains endpoint preference. The fix is
        // cycle handling, not globally disabling endpoint comparison.
        let pair = result
            .root_actions
            .iter()
            .filter(|candidate| candidate.action == yellow || candidate.action == blue)
            .cloned()
            .collect::<Vec<_>>();
        assert_eq!(pair[best_symbolic_index(&pair, None).unwrap()].action, blue);
        // Algorithmic invariants: this cycle resolves to the reviewed winner,
        // independently of enumeration order or removal of its blue member.
        let mut candidates = result.root_actions.clone();
        candidates.retain(|candidate| candidate.action != blue);
        assert_eq!(
            candidates[best_symbolic_index(&candidates, None).unwrap()].action,
            yellow
        );
        candidates = result.root_actions;
        candidates.reverse();
        assert_eq!(
            candidates[best_symbolic_index(&candidates, None).unwrap()].action,
            yellow
        );
    }

    #[test]
    fn reviewed_two_for_one_beats_a_speculative_finesse_tiebreaker() {
        // User-reviewed p4v0s1 turn 3: blue to Bob is a 2-for-1;
        // 1s to Alice is a 1-for-1 with a speculative future y2 finesse.
        let replay = hanabi_protocol::HanabiLiveReplay::from_json(include_str!(
            "h_group/tests/fixtures/game-p4v0s1-before-turn18-revision.json"
        ))
        .unwrap();
        let state = replay.state_at_turn(2).unwrap();
        let information =
            InformationSet::new(&state.view_for(state.current_player()).unwrap()).unwrap();
        let result = plan_move(
            &information,
            SupportedConvention::HGroup(crate::HGroupProfile::Max),
            PlannerConfig::default(),
        )
        .unwrap();
        let blue = Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Suit(Suit::Blue),
        };
        let ones = Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Rank(Rank::One),
        };
        assert_eq!(result.best_action, blue);
        let blue = result
            .root_actions
            .iter()
            .find(|c| c.action == blue)
            .unwrap();
        let ones = result
            .root_actions
            .iter()
            .find(|c| c.action == ones)
            .unwrap();
        assert_eq!(blue.newly_touched, 2);
        assert_eq!(ones.newly_touched, 1);
        let b = blue.symbolic_line.position_value.unwrap();
        let o = ones.symbolic_line.position_value.unwrap();
        assert_eq!(
            b.without_speculative_finesse(),
            o.without_speculative_finesse()
        );
        assert!(o.finesse_opportunities > b.finesse_opportunities);
        assert!(!o.dominates(b));
        assert!(!b.dominates(o));

        // Algorithmic ordering check, not a changed replay expectation:
        // on a true priority tie the opportunity may decide the result.
        let mut tied = [blue.clone(), ones.clone()];
        tied[1].preference = tied[0].preference;
        assert_eq!(best_symbolic_index(&tied, None), Some(1));
        tied[1].symbolic_line.position_value.as_mut().unwrap().clues -= 1;
        assert_eq!(best_symbolic_index(&tied, None), Some(0));
    }

    #[test]
    fn every_reviewed_root_is_projected_despite_unequal_priorities() {
        fn check_branches(evidence: &crate::ProjectionEvidence) {
            for branch in evidence.branches() {
                assert!(branch.steps.starts_with(&evidence.steps));
                assert!(branch.resources.unfunded_turn.is_none());
                check_branches(branch);
            }
        }

        // p4v0s2 turn 8 reproduces the old unique-best projection bypass.
        // This asserts planner mechanics, not that the Save is optimal.
        let replay = hanabi_protocol::HanabiLiveReplay::from_json(include_str!(
            "../../hanabi-protocol/tests/fixtures/game-p4v0s2.json"
        ))
        .unwrap();
        let state = replay.state_at_turn(7).unwrap();
        let view = state.view_for(state.current_player()).unwrap();
        let information = InformationSet::new(&view).unwrap();
        let result = plan_move(
            &information,
            SupportedConvention::HGroup(crate::HGroupProfile::Max),
            PlannerConfig::default(),
        )
        .unwrap();
        assert!(result.root_actions.len() >= 3);
        // Algorithmic contracts on real candidates: ordering and legacy
        // diagnostic numbers cannot change the semantic decision.
        let (selected, comparisons) = compare_symbolic_candidates(&result.root_actions, None);
        let selected_action = result.root_actions[selected.unwrap()].action;
        assert_eq!(
            comparisons.len(),
            result.root_actions.len() * (result.root_actions.len() - 1) / 2
        );
        for offset in 0..result.root_actions.len() {
            let mut reordered = result.root_actions.clone();
            reordered.rotate_left(offset);
            reordered.reverse();
            let (index, _) = compare_symbolic_candidates(&reordered, None);
            assert_eq!(reordered[index.unwrap()].action, selected_action);
        }
        for root in &result.root_actions {
            // Branching evidence stores the pre-branch prefix at its root.
            // The summary may additionally include actions common to every
            // child; it must not count a branch-specific continuation.
            let common_steps = if let Some(first) = root.projection.clue_branches.first() {
                first
                    .continuation
                    .steps
                    .iter()
                    .enumerate()
                    .take_while(|(index, step)| {
                        root.projection
                            .clue_branches
                            .iter()
                            .all(|branch| branch.continuation.steps.get(*index) == Some(*step))
                    })
                    .count()
            } else {
                root.projection.steps.len()
            };
            assert_eq!(common_steps, usize::from(root.symbolic_line.actions));
            assert!(root.projection.resources.unfunded_turn.is_none());
            check_branches(&root.projection);
        }
        let mut incomparable = result.root_actions[0].clone();
        incomparable.symbolic_line.actions = incomparable.symbolic_line.actions.saturating_add(1);
        assert_eq!(
            compare_endpoints(&result.root_actions[0], &incomparable),
            EndpointComparison::Incomparable
        );
        assert!(
            result
                .root_actions
                .iter()
                .all(|root| root.symbolic_line.actions > 0)
        );
        assert!(
            result
                .root_actions
                .iter()
                .any(|root| root.symbolic_line.actions > 1)
        );
        let mut alternatives = vec![result.root_actions[0].clone(); 2];
        alternatives[0].preference = crate::ActionPreference::new(1000, false);
        alternatives[0].symbolic_line.strikes = 1;
        alternatives[1].preference = crate::ActionPreference::new(1, false);
        alternatives[1].symbolic_line.strikes = 0;
        assert_eq!(
            best_symbolic_index(&alternatives, None),
            Some(1),
            "a larger heuristic must not hide a projected misplay"
        );
    }

    #[test]
    fn opening_planning_is_deterministic_and_symbolic() {
        let state = FullState::new_standard(2, standard_deck()).unwrap();
        let information = InformationSet::new(&state.view_for(PlayerId::new(0)).unwrap()).unwrap();
        let first = plan_move(
            &information,
            SupportedConvention::None,
            PlannerConfig::default(),
        )
        .unwrap();
        let second = plan_move(
            &information,
            SupportedConvention::None,
            PlannerConfig::default(),
        )
        .unwrap();
        assert_eq!(first, second);
        assert_eq!(first.phase, PlannerPhase::Symbolic);
        assert!(!first.world_count.is_exact());
        assert_eq!(first.best_action, Action::Play(hanabi_core::CardId::new(4)));
    }

    #[test]
    fn forced_continuation_is_the_only_planning_candidate() {
        let first = Action::Play(hanabi_core::CardId::new(0));
        let forced = Action::Play(hanabi_core::CardId::new(1));
        let analysis = ConventionAnalysis {
            actions: vec![
                ConventionAction {
                    action: first,
                    preference: crate::ActionPreference::new(900, false),
                    reason: crate::ConventionActionReason::PromisedPlay,
                },
                ConventionAction {
                    action: forced,
                    preference: crate::ActionPreference::new(400, false)
                        .with_policy_tier(ConventionPolicyTier::Required),
                    reason: crate::ConventionActionReason::PromisedPlay,
                },
            ],
            forced_action: Some(forced),
            ..ConventionAnalysis::default()
        };

        assert_eq!(
            planning_candidates(&analysis).as_ref(),
            &[ConventionAction {
                action: forced,
                preference: crate::ActionPreference::new(400, false)
                    .with_policy_tier(ConventionPolicyTier::Required),
                reason: crate::ConventionActionReason::PromisedPlay,
            }]
        );
    }

    #[test]
    fn required_policy_tier_outweighs_a_larger_heuristic_number() {
        let state = FullState::new_standard(2, standard_deck()).unwrap();
        let deductions = LogicalDeductions::new(state.view_for(PlayerId::new(0)).unwrap()).unwrap();
        let legal = deductions.view().legal_actions();
        let low_required = ConventionAction {
            action: legal[0],
            preference: crate::ActionPreference::new(1, false)
                .with_policy_tier(ConventionPolicyTier::Required),
            reason: crate::ConventionActionReason::PromisedPlay,
        };
        let high_admitted = ConventionAction {
            action: legal[1],
            preference: crate::ActionPreference::new(10_000, false),
            reason: crate::ConventionActionReason::OtherClue,
        };
        let evaluations = [
            symbolic_evaluation(&deductions, low_required),
            symbolic_evaluation(&deductions, high_admitted),
        ];

        assert_eq!(best_symbolic_index(&evaluations, None), Some(0));
    }

    #[test]
    fn exact_solver_compiles_each_public_observation_once() {
        let state = FullState::new_standard(2, standard_deck()).unwrap();
        let view = state.view_for(PlayerId::new(0)).unwrap();
        let mut cache = ConventionAnalysisCache::default();

        cache
            .compile(view.clone(), SupportedConvention::None)
            .unwrap();
        cache.compile(view, SupportedConvention::None).unwrap();

        assert_eq!(cache.compilations, 1);
        assert_eq!(cache.entries.len(), 1);
    }

    #[test]
    fn exact_solver_respects_observation_groups() {
        let mut state = FullState::new_standard(2, standard_deck()).unwrap();
        while state.deck_size() > 0 && !state.is_terminal() {
            let playable = state
                .hand(state.current_player())
                .unwrap()
                .iter()
                .find(|card| {
                    let identity = state.card(**card).unwrap();
                    identity.rank.number()
                        == u8::try_from(state.play_stacks()[identity.suit.index()].len()).unwrap()
                            + 1
                });
            let action = playable.map_or_else(
                || {
                    state
                        .legal_actions()
                        .into_iter()
                        .find(|action| matches!(action, Action::Discard(_)))
                        .unwrap_or_else(|| {
                            state
                                .legal_actions()
                                .into_iter()
                                .find(|action| matches!(action, Action::Clue { .. }))
                                .unwrap()
                        })
                },
                |card| Action::Play(*card),
            );
            state.apply(action).unwrap();
        }
        assert!(!state.is_terminal());
        let information =
            InformationSet::new(&state.view_for(state.current_player()).unwrap()).unwrap();
        let result = plan_move(
            &information,
            SupportedConvention::None,
            PlannerConfig {
                objective: PlanningObjective::ExpectedScore,
                exact_world_limit: 100_000,
                exact_node_limit: 1_000_000,
            },
        )
        .unwrap();
        assert_eq!(result.phase, PlannerPhase::Exact);
        assert!(result.world_count.is_exact());
        let best = result
            .root_actions
            .iter()
            .find(|evaluation| evaluation.action == result.best_action)
            .unwrap()
            .exact
            .unwrap();
        if result
            .root_actions
            .iter()
            .any(|evaluation| evaluation.exact.is_none())
        {
            let worlds = information
                .collect_worlds_after_count(
                    &crate::BeliefConstraints::default(),
                    usize::try_from(result.world_count.worlds()).unwrap(),
                    &crate::AnalysisControl::default(),
                )
                .unwrap();
            assert_eq!(
                best,
                exact_value_upper_bound(&worlds),
                "unsearched roots require a proven global bound"
            );
        }
        let mut forced = SupportedConvention::None.analyze(information.deductions());
        forced.actions.truncate(1);
        let only_action = forced.actions[0].action;
        forced.preferred_action = Some(only_action);
        let single = plan_move_with_analysis(
            &information,
            SupportedConvention::None,
            &forced,
            PlannerConfig {
                exact_world_limit: 100_000,
                exact_node_limit: 1_000_000,
                ..PlannerConfig::default()
            },
        )
        .unwrap();
        assert_eq!(single.best_action, only_action);
        assert!(single.world_count.is_exact());
        assert_eq!(
            single.exact_nodes, 0,
            "a forced action needs no continuation search"
        );
        assert_eq!(single.phase, PlannerPhase::Symbolic);
    }
}
