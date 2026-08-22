use core::fmt;
use std::time::Instant;

use hanabi_core::{Action, EndReason, FullState, GameStatus, RuleError};
use rand::{SeedableRng, rngs::StdRng};

use crate::rollout::rollout_for_search;
use crate::{
    ConventionFramework, InformationSet, RolloutError, RolloutPolicy, SampleError,
    SearchDiagnostics, SearchObjective, StrategicMetrics, evaluation,
};

/// Reproducible budget for flat Monte Carlo action evaluation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MonteCarloConfig {
    /// Number of root-consistent worlds used for every legal candidate.
    pub samples_per_action: u32,
    /// Seed used to sample hidden hands and deck orders.
    pub seed: u64,
    pub objective: SearchObjective,
}

/// Terminal rollout statistics for one root action.
#[derive(Clone, Debug, PartialEq)]
pub struct ActionEvaluation {
    pub action: Action,
    pub samples: u32,
    /// Mean official Hanabi score, including zero for strikeouts.
    pub mean_score: f64,
    /// Mean raw stack score, including progress made before strikeouts.
    pub mean_raw_score: f64,
    /// Mean terminal utility used to rank actions.
    pub mean_utility: f64,
    pub perfect_rate: f64,
    pub mean_score_ceiling: f64,
    pub mean_clue_actions: f64,
    pub mean_clue_efficiency: f64,
    pub mean_tempo_clues: f64,
    pub mean_critical_discards: f64,
    pub mean_bottom_deck_risk: f64,
    pub mean_clue_debt: f64,
    pub mean_predictable_turns: f64,
    pub score_variance: f64,
    pub strikeout_rate: f64,
    pub min_score: u8,
    pub max_score: u8,
    /// Highest-valued sampled continuation, including the root action.
    pub principal_variation: Vec<Action>,
}

/// Flat Monte Carlo evaluations plus measured search diagnostics.
#[derive(Clone, Debug)]
pub struct MonteCarloReport {
    pub evaluations: Vec<ActionEvaluation>,
    pub diagnostics: SearchDiagnostics,
}

/// Evaluates every legal root action using the same sampled hidden worlds.
///
/// Reusing determinizations across candidates is a common-random-numbers
/// variance reduction technique: action comparisons are not distorted by one
/// candidate receiving luckier hidden cards or deck orders than another. The
/// selected [`ConventionFramework`] supplies both those root worlds and the
/// rollout behavior used to evaluate them. Candidate generation uses the root
/// [`hanabi_core::PlayerView`]. Only the simulation driver sees sampled
/// authoritative states; convention decisions continue to receive only legal
/// deductions for the acting player.
///
/// # Errors
///
/// Returns [`SearchError`] for a zero budget, a root with no legal actions, a
/// failed determinization, an unexpectedly illegal root action, or a failed
/// rollout.
pub fn evaluate_actions<P: ConventionFramework>(
    information_set: &InformationSet,
    policy: &P,
    config: MonteCarloConfig,
) -> Result<Vec<ActionEvaluation>, SearchError> {
    Ok(run_evaluation(information_set, policy, config, false)?.evaluations)
}

/// Evaluates every legal root action and records work and timing diagnostics.
///
/// Candidate statistics and random-number consumption are identical to
/// [`evaluate_actions`]. Timing fields are observational and do not participate
/// in candidate selection.
///
/// # Errors
///
/// Returns the same [`SearchError`] conditions as [`evaluate_actions`].
pub fn evaluate_actions_with_diagnostics<P: ConventionFramework>(
    information_set: &InformationSet,
    policy: &P,
    config: MonteCarloConfig,
) -> Result<MonteCarloReport, SearchError> {
    run_evaluation(information_set, policy, config, true)
}

fn run_evaluation<P: ConventionFramework>(
    information_set: &InformationSet,
    policy: &P,
    config: MonteCarloConfig,
    measure_timing: bool,
) -> Result<MonteCarloReport, SearchError> {
    let search_started = measure_timing.then(Instant::now);
    if config.samples_per_action == 0 {
        return Err(SearchError::ZeroSamples);
    }

    let actions = policy.candidate_actions(information_set);
    if actions.is_empty() {
        return Err(SearchError::NoLegalActions);
    }

    let mut rng = StdRng::seed_from_u64(config.seed);
    let mut accumulators = actions
        .into_iter()
        .map(|action| ActionAccumulator::new(action, config.objective))
        .collect::<Vec<_>>();
    let mut diagnostics = SearchDiagnostics::default();
    for sample in 0..config.samples_per_action {
        let sampling_started = measure_timing.then(Instant::now);
        let sampled = policy.sample_root_world(information_set, &mut rng);
        if let Some(started) = sampling_started {
            diagnostics.sampling_time += started.elapsed();
        }
        let world = sampled.map_err(|source| SearchError::Sample { sample, source })?;
        diagnostics.worlds_sampled += 1;
        for accumulator in &mut accumulators {
            accumulator.observe(&world, policy, sample, &mut diagnostics, measure_timing)?;
        }
    }

    let evaluations = accumulators
        .into_iter()
        .map(|accumulator| accumulator.finish(config.samples_per_action))
        .collect();
    if let Some(started) = search_started {
        diagnostics.finish_timing(started.elapsed());
    }
    Ok(MonteCarloReport {
        evaluations,
        diagnostics,
    })
}

struct ActionAccumulator {
    action: Action,
    score_sum: f64,
    raw_score_sum: f64,
    utility_sum: f64,
    perfects: u32,
    score_ceiling_sum: f64,
    clue_actions_sum: f64,
    clue_efficiency_sum: f64,
    tempo_clues_sum: f64,
    critical_discards_sum: f64,
    bottom_deck_risk_sum: f64,
    clue_debt_sum: f64,
    predictable_turns_sum: f64,
    squared_score_sum: f64,
    strikeouts: u32,
    min_score: u8,
    max_score: u8,
    objective: SearchObjective,
    best_utility: f64,
    principal_variation: Vec<Action>,
}

impl ActionAccumulator {
    const fn new(action: Action, objective: SearchObjective) -> Self {
        Self {
            action,
            score_sum: 0.0,
            raw_score_sum: 0.0,
            utility_sum: 0.0,
            perfects: 0,
            score_ceiling_sum: 0.0,
            clue_actions_sum: 0.0,
            clue_efficiency_sum: 0.0,
            tempo_clues_sum: 0.0,
            critical_discards_sum: 0.0,
            bottom_deck_risk_sum: 0.0,
            clue_debt_sum: 0.0,
            predictable_turns_sum: 0.0,
            squared_score_sum: 0.0,
            strikeouts: 0,
            min_score: u8::MAX,
            max_score: u8::MIN,
            objective,
            best_utility: f64::NEG_INFINITY,
            principal_variation: Vec::new(),
        }
    }

    fn observe<P: RolloutPolicy>(
        &mut self,
        world: &FullState,
        policy: &P,
        sample: u32,
        diagnostics: &mut SearchDiagnostics,
        measure_timing: bool,
    ) -> Result<(), SearchError> {
        let mut candidate = world.clone();
        let mut root_metrics = StrategicMetrics::default();
        evaluation::observe_action(&candidate, self.action, &mut root_metrics);
        diagnostics.candidate_state_clones += 1;
        candidate
            .apply(self.action)
            .map_err(|source| SearchError::RootAction {
                action: self.action,
                sample,
                source,
            })?;
        diagnostics.search_actions_applied += 1;
        diagnostics.observe_tree_depth(1);
        let report = rollout_for_search(candidate, policy, measure_timing).map_err(|source| {
            SearchError::Rollout {
                action: self.action,
                sample,
                source,
            }
        })?;
        if measure_timing {
            diagnostics.add_rollout_timing(report.diagnostics);
        }
        let outcome = report.outcome;
        diagnostics.rollouts += 1;
        diagnostics.rollout_turns +=
            u64::try_from(outcome.turns()).expect("a rollout turn count fits in u64");

        let score = outcome.score();
        let mut metrics = outcome.strategic_metrics();
        metrics.clue_actions = metrics
            .clue_actions
            .saturating_add(root_metrics.clue_actions);
        metrics.newly_touched_cards = metrics
            .newly_touched_cards
            .saturating_add(root_metrics.newly_touched_cards);
        metrics.tempo_clues = metrics.tempo_clues.saturating_add(root_metrics.tempo_clues);
        metrics.critical_discards = metrics
            .critical_discards
            .saturating_add(root_metrics.critical_discards);
        metrics.bottom_deck_risk += root_metrics.bottom_deck_risk;
        let utility = self.objective.utility(score, outcome.raw_score(), metrics);
        let score_float = f64::from(score);
        let raw_score_float = f64::from(outcome.raw_score());
        self.score_sum += score_float;
        self.raw_score_sum += raw_score_float;
        self.utility_sum += utility;
        self.perfects += u32::from(metrics.perfect);
        self.score_ceiling_sum += f64::from(metrics.score_ceiling);
        self.clue_actions_sum += f64::from(metrics.clue_actions);
        self.clue_efficiency_sum += if metrics.clue_actions == 0 {
            0.0
        } else {
            f64::from(metrics.newly_touched_cards) / f64::from(metrics.clue_actions)
        };
        self.tempo_clues_sum += f64::from(metrics.tempo_clues);
        self.critical_discards_sum += f64::from(metrics.critical_discards);
        self.bottom_deck_risk_sum += metrics.bottom_deck_risk;
        self.clue_debt_sum += metrics.clue_debt;
        self.predictable_turns_sum += f64::from(metrics.predictable_turns);
        if utility > self.best_utility {
            self.best_utility = utility;
            self.principal_variation.clear();
            self.principal_variation.push(self.action);
            self.principal_variation
                .extend_from_slice(outcome.actions());
        }
        self.squared_score_sum += score_float * score_float;
        self.min_score = self.min_score.min(score);
        self.max_score = self.max_score.max(score);
        if outcome.final_state().status() == GameStatus::Finished(EndReason::TooManyStrikes) {
            self.strikeouts += 1;
        }
        Ok(())
    }

    fn finish(self, sample_count: u32) -> ActionEvaluation {
        let denominator = f64::from(sample_count);
        let mean_score = self.score_sum / denominator;
        let score_variance =
            (self.squared_score_sum / denominator - mean_score * mean_score).max(0.0);

        ActionEvaluation {
            action: self.action,
            samples: sample_count,
            mean_score,
            mean_raw_score: self.raw_score_sum / denominator,
            mean_utility: self.utility_sum / denominator,
            perfect_rate: f64::from(self.perfects) / denominator,
            mean_score_ceiling: self.score_ceiling_sum / denominator,
            mean_clue_actions: self.clue_actions_sum / denominator,
            mean_clue_efficiency: self.clue_efficiency_sum / denominator,
            mean_tempo_clues: self.tempo_clues_sum / denominator,
            mean_critical_discards: self.critical_discards_sum / denominator,
            mean_bottom_deck_risk: self.bottom_deck_risk_sum / denominator,
            mean_clue_debt: self.clue_debt_sum / denominator,
            mean_predictable_turns: self.predictable_turns_sum / denominator,
            score_variance,
            strikeout_rate: f64::from(self.strikeouts) / denominator,
            min_score: self.min_score,
            max_score: self.max_score,
            principal_variation: self.principal_variation,
        }
    }
}

/// Selects the first evaluation with the highest expected terminal utility.
///
/// Since evaluations retain `PlayerView::legal_actions` order, exact ties are
/// deterministic and independent of hash-map ordering.
#[must_use]
pub fn select_best_action(evaluations: &[ActionEvaluation]) -> Option<Action> {
    let mut best = evaluations.first()?;
    for evaluation in &evaluations[1..] {
        if evaluation.mean_utility > best.mean_utility {
            best = evaluation;
        }
    }
    Some(best.action)
}

/// Why flat Monte Carlo evaluation could not complete.
#[derive(Debug, PartialEq)]
pub enum SearchError {
    ZeroSamples,
    NoLegalActions,
    Sample {
        sample: u32,
        source: SampleError,
    },
    RootAction {
        action: Action,
        sample: u32,
        source: RuleError,
    },
    Rollout {
        action: Action,
        sample: u32,
        source: RolloutError,
    },
}

impl fmt::Display for SearchError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ZeroSamples => formatter.write_str("samples_per_action must be positive"),
            Self::NoLegalActions => formatter.write_str("the root view has no legal actions"),
            Self::Sample { sample, source } => {
                write!(formatter, "failed to sample root world {sample}: {source}")
            }
            Self::RootAction {
                action,
                sample,
                source,
            } => write!(
                formatter,
                "root action {action:?} was illegal in sample {sample}: {source}"
            ),
            Self::Rollout {
                action,
                sample,
                source,
            } => write!(
                formatter,
                "rollout for action {action:?} failed in sample {sample}: {source}"
            ),
        }
    }
}

impl std::error::Error for SearchError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Sample { source, .. } => Some(source),
            Self::RootAction { source, .. } => Some(source),
            Self::Rollout { source, .. } => Some(source),
            Self::ZeroSamples | Self::NoLegalActions => None,
        }
    }
}
