use core::fmt;
use std::time::Instant;

use hanabi_core::{Action, EndReason, FullState, GameStatus, PlayerId, RuleError};
use rand::{Rng, RngExt, SeedableRng, rngs::StdRng};

use crate::rollout::rollout_for_search;
use crate::{
    ConventionFramework, InformationSet, InformationSetError, LogicalDeductions,
    MAX_TERMINAL_UTILITY, RolloutError, SampleError, SearchDiagnostics, terminal_utility,
};

const MAX_TREE_DEPTH: u32 = 512;
const MAX_LEGAL_ACTIONS: usize = 50;

/// Reproducible single-observer information-set MCTS configuration.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct IsmctsConfig {
    /// Number of independently determinized tree iterations.
    pub iterations: u32,
    /// UCB exploration coefficient. Exploitation scores are normalized to
    /// `0.0..=1.0`, so `sqrt(2)` is a conventional baseline.
    pub exploration: f64,
    /// Seed used for determinization and expansion selection.
    pub seed: u64,
}

/// Public root-edge statistics after an ISMCTS search.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TreeActionStatistics {
    pub action: Action,
    pub visits: u32,
    /// Iterations in which this action was legal in the sampled world.
    pub availability: u32,
    /// Mean official Hanabi score, including zero for strikeouts.
    pub mean_score: Option<f64>,
    /// Mean raw stack score, including progress made before strikeouts.
    pub mean_raw_score: Option<f64>,
    /// Mean terminal utility used by UCB and robust-child tie-breaking.
    pub mean_utility: Option<f64>,
    pub strikeout_rate: Option<f64>,
    pub min_score: Option<u8>,
    pub max_score: Option<u8>,
}

/// Root result of a completed ISMCTS search.
#[derive(Clone, Debug, PartialEq)]
pub struct IsmctsResult {
    pub iterations: u32,
    pub best_action: Action,
    pub root_actions: Vec<TreeActionStatistics>,
}

/// ISMCTS result plus measured search diagnostics.
#[derive(Clone, Debug)]
pub struct IsmctsReport {
    pub result: IsmctsResult,
    pub diagnostics: SearchDiagnostics,
}

/// Runs cooperative single-observer information-set Monte Carlo tree search.
///
/// Every iteration asks the selected [`ConventionFramework`] to sample a new
/// root-consistent [`FullState`], allowing convention beliefs to weight worlds.
/// Tree nodes are shared by action history, while edges track how often an
/// action was legally available across determinizations. Selection uses
/// availability-aware UCB, expansion adds one node, and the remaining position
/// is completed by the framework's rollout policy. All players backpropagate the same team
/// utility: official score first, with raw stack score as a terminal tie-break.
///
/// Tree decisions enumerate actions from the acting player's legal view. The
/// sampled authoritative state is used only by the simulation environment to
/// apply actions and expose the next legal view.
///
/// # Errors
///
/// Returns [`IsmctsError`] for invalid configuration, an unactionable root,
/// failed determinization, an invalid current player, an unexpectedly illegal
/// selected action, a failed rollout, or excessive tree depth.
pub fn ismcts_search<P: ConventionFramework>(
    information_set: &InformationSet,
    rollout_policy: &P,
    config: IsmctsConfig,
) -> Result<IsmctsResult, IsmctsError> {
    Ok(run_ismcts(information_set, rollout_policy, config, false)?.result)
}

/// Runs ISMCTS and records work and timing diagnostics.
///
/// Search results and random-number consumption are identical to
/// [`ismcts_search`]. Timing fields are observational and do not participate in
/// tree selection.
///
/// # Errors
///
/// Returns the same [`IsmctsError`] conditions as [`ismcts_search`].
pub fn ismcts_search_with_diagnostics<P: ConventionFramework>(
    information_set: &InformationSet,
    rollout_policy: &P,
    config: IsmctsConfig,
) -> Result<IsmctsReport, IsmctsError> {
    run_ismcts(information_set, rollout_policy, config, true)
}

fn run_ismcts<P: ConventionFramework>(
    information_set: &InformationSet,
    rollout_policy: &P,
    config: IsmctsConfig,
    measure_timing: bool,
) -> Result<IsmctsReport, IsmctsError> {
    let search_started = measure_timing.then(Instant::now);
    validate_config(config)?;
    if rollout_policy.candidate_actions(information_set).is_empty() {
        return Err(IsmctsError::NoLegalActions);
    }

    let mut root = Node::default();
    let mut rng = StdRng::seed_from_u64(config.seed);
    let mut diagnostics = SearchDiagnostics::default();
    let mut legal_actions = Vec::with_capacity(MAX_LEGAL_ACTIONS);
    for iteration in 0..config.iterations {
        let sampling_started = measure_timing.then(Instant::now);
        let sampled = rollout_policy.sample_root_world(information_set, &mut rng);
        if let Some(started) = sampling_started {
            diagnostics.sampling_time += started.elapsed();
        }
        let state = sampled.map_err(|source| IsmctsError::Sample { iteration, source })?;
        diagnostics.worlds_sampled += 1;
        let mut context = SimulationContext {
            rollout_policy,
            exploration: config.exploration,
            rng: &mut rng,
            diagnostics: &mut diagnostics,
            legal_actions: &mut legal_actions,
            measure_timing,
        };
        simulate(&mut root, state, 0, &mut context)?;
    }

    let root_actions = root.edges.iter().map(Edge::statistics).collect::<Vec<_>>();
    let best_action = robust_child(&root.edges).ok_or(IsmctsError::NoVisitedActions)?;
    let result = IsmctsResult {
        iterations: config.iterations,
        best_action,
        root_actions,
    };
    if let Some(started) = search_started {
        diagnostics.finish_timing(started.elapsed());
    }
    Ok(IsmctsReport {
        result,
        diagnostics,
    })
}

fn validate_config(config: IsmctsConfig) -> Result<(), IsmctsError> {
    if config.iterations == 0 {
        return Err(IsmctsError::ZeroIterations);
    }
    if !config.exploration.is_finite() || config.exploration < 0.0 {
        return Err(IsmctsError::InvalidExploration(config.exploration));
    }
    Ok(())
}

#[derive(Default)]
struct Node {
    visits: u32,
    edges: Vec<Edge>,
}

struct Edge {
    action: Action,
    availability: u32,
    visits: u32,
    score_sum: f64,
    raw_score_sum: f64,
    utility_sum: f64,
    strikeouts: u32,
    min_score: u8,
    max_score: u8,
    child: Option<Box<Node>>,
}

impl Edge {
    const fn new(action: Action) -> Self {
        Self {
            action,
            availability: 0,
            visits: 0,
            score_sum: 0.0,
            raw_score_sum: 0.0,
            utility_sum: 0.0,
            strikeouts: 0,
            min_score: u8::MAX,
            max_score: u8::MIN,
            child: None,
        }
    }

    fn observe(&mut self, reward: Reward) {
        self.visits += 1;
        self.score_sum += f64::from(reward.score);
        self.raw_score_sum += f64::from(reward.raw_score);
        self.utility_sum += f64::from(reward.utility());
        self.min_score = self.min_score.min(reward.score);
        self.max_score = self.max_score.max(reward.score);
        if reward.strikeout {
            self.strikeouts += 1;
        }
    }

    fn mean_score(&self) -> f64 {
        self.score_sum / f64::from(self.visits)
    }

    fn mean_raw_score(&self) -> f64 {
        self.raw_score_sum / f64::from(self.visits)
    }

    fn mean_utility(&self) -> f64 {
        self.utility_sum / f64::from(self.visits)
    }

    fn statistics(&self) -> TreeActionStatistics {
        let visited = self.visits > 0;
        TreeActionStatistics {
            action: self.action,
            visits: self.visits,
            availability: self.availability,
            mean_score: visited.then(|| self.mean_score()),
            mean_raw_score: visited.then(|| self.mean_raw_score()),
            mean_utility: visited.then(|| self.mean_utility()),
            strikeout_rate: visited.then(|| f64::from(self.strikeouts) / f64::from(self.visits)),
            min_score: visited.then_some(self.min_score),
            max_score: visited.then_some(self.max_score),
        }
    }
}

#[derive(Clone, Copy)]
struct Reward {
    score: u8,
    raw_score: u8,
    strikeout: bool,
}

impl Reward {
    const fn new(score: u8, raw_score: u8, strikeout: bool) -> Self {
        Self {
            score,
            raw_score,
            strikeout,
        }
    }

    const fn utility(self) -> u16 {
        terminal_utility(self.score, self.raw_score)
    }
}

struct SimulationContext<'a, P, R: ?Sized> {
    rollout_policy: &'a P,
    exploration: f64,
    rng: &'a mut R,
    diagnostics: &'a mut SearchDiagnostics,
    legal_actions: &'a mut Vec<Action>,
    measure_timing: bool,
}

fn simulate<P: ConventionFramework, R: Rng + ?Sized>(
    node: &mut Node,
    mut state: FullState,
    depth: u32,
    context: &mut SimulationContext<'_, P, R>,
) -> Result<Reward, IsmctsError> {
    context.diagnostics.observe_tree_depth(depth);
    if state.is_terminal() {
        node.visits += 1;
        return terminal_reward(&state).ok_or(IsmctsError::NonTerminalOutcome);
    }
    if depth >= MAX_TREE_DEPTH {
        return Err(IsmctsError::TreeDepthExceeded);
    }

    let actor = state.current_player();
    let view = state
        .view_for(actor)
        .ok_or(IsmctsError::InvalidCurrentPlayer(actor))?;
    let deductions = LogicalDeductions::new(view)
        .map_err(|source| IsmctsError::TreeInformationSet { depth, source })?;
    context.legal_actions.clear();
    context
        .legal_actions
        .extend(context.rollout_policy.candidate_actions(&deductions));
    if context.legal_actions.is_empty() {
        return Err(IsmctsError::NoLegalTreeActions { depth, actor });
    }

    let legal_edges = register_available_actions(node, context.legal_actions);
    let selected = select_edge(
        node,
        legal_edges.as_slice(),
        context.exploration,
        context.rng,
    );
    let action = node.edges[selected].action;
    state
        .apply(action)
        .map_err(|source| IsmctsError::TreeAction {
            depth,
            action,
            source,
        })?;
    context.diagnostics.search_actions_applied += 1;
    context
        .diagnostics
        .observe_tree_depth(depth.saturating_add(1));

    let reward = if node.edges[selected].child.is_none() {
        let report = rollout_for_search(state, context.rollout_policy, context.measure_timing)
            .map_err(|source| IsmctsError::Rollout {
                depth,
                action,
                source,
            })?;
        if context.measure_timing {
            context.diagnostics.add_rollout_timing(report.diagnostics);
        }
        let outcome = report.outcome;
        context.diagnostics.rollouts += 1;
        context.diagnostics.rollout_turns +=
            u64::try_from(outcome.turns()).expect("a rollout turn count fits in u64");
        context.diagnostics.tree_nodes_expanded += 1;
        node.edges[selected].child = Some(Box::new(Node {
            visits: 1,
            edges: Vec::new(),
        }));
        Reward::new(
            outcome.score(),
            outcome.raw_score(),
            outcome.final_state().status() == GameStatus::Finished(EndReason::TooManyStrikes),
        )
    } else {
        simulate(
            node.edges[selected]
                .child
                .as_deref_mut()
                .expect("the expanded child was checked as present"),
            state,
            depth + 1,
            context,
        )?
    };

    node.visits += 1;
    node.edges[selected].observe(reward);
    Ok(reward)
}

struct LegalEdges {
    indices: [usize; MAX_LEGAL_ACTIONS],
    len: usize,
}

impl LegalEdges {
    fn as_slice(&self) -> &[usize] {
        &self.indices[..self.len]
    }
}

fn register_available_actions(node: &mut Node, legal_actions: &[Action]) -> LegalEdges {
    let mut legal_edges = LegalEdges {
        indices: [0; MAX_LEGAL_ACTIONS],
        len: 0,
    };
    for action in legal_actions {
        let index = node
            .edges
            .iter()
            .position(|edge| edge.action == *action)
            .unwrap_or_else(|| {
                node.edges.push(Edge::new(*action));
                node.edges.len() - 1
            });
        node.edges[index].availability += 1;
        legal_edges.indices[legal_edges.len] = index;
        legal_edges.len += 1;
    }
    legal_edges
}

fn select_edge<R: Rng + ?Sized>(
    node: &Node,
    legal_edges: &[usize],
    exploration: f64,
    rng: &mut R,
) -> usize {
    let unexpanded_count = legal_edges
        .iter()
        .copied()
        .filter(|index| node.edges[*index].child.is_none())
        .count();
    if unexpanded_count > 0 {
        let selected = rng.random_range(0..unexpanded_count);
        return legal_edges
            .iter()
            .copied()
            .filter(|index| node.edges[*index].child.is_none())
            .nth(selected)
            .expect("the selected unexpanded edge was counted");
    }

    let mut selected = legal_edges[0];
    let mut best_value = f64::NEG_INFINITY;
    for index in legal_edges.iter().copied() {
        let edge = &node.edges[index];
        let availability = f64::from(edge.availability);
        let exploration_bonus = exploration * (availability.ln() / f64::from(edge.visits)).sqrt();
        let value = edge.mean_utility() / f64::from(MAX_TERMINAL_UTILITY) + exploration_bonus;
        if value > best_value {
            selected = index;
            best_value = value;
        }
    }
    selected
}

fn robust_child(edges: &[Edge]) -> Option<Action> {
    let mut best: Option<&Edge> = None;
    for edge in edges.iter().filter(|edge| edge.visits > 0) {
        let replace = best.is_none_or(|current| {
            edge.visits > current.visits
                || (edge.visits == current.visits && edge.mean_utility() > current.mean_utility())
        });
        if replace {
            best = Some(edge);
        }
    }
    best.map(|edge| edge.action)
}

fn terminal_reward(state: &FullState) -> Option<Reward> {
    Some(Reward::new(
        state.final_score()?,
        state.score(),
        state.status() == GameStatus::Finished(EndReason::TooManyStrikes),
    ))
}

/// Why ISMCTS could not complete.
#[derive(Debug, PartialEq)]
pub enum IsmctsError {
    ZeroIterations,
    InvalidExploration(f64),
    NoLegalActions,
    NoVisitedActions,
    Sample {
        iteration: u32,
        source: SampleError,
    },
    InvalidCurrentPlayer(PlayerId),
    NoLegalTreeActions {
        depth: u32,
        actor: PlayerId,
    },
    TreeInformationSet {
        depth: u32,
        source: InformationSetError,
    },
    TreeAction {
        depth: u32,
        action: Action,
        source: RuleError,
    },
    Rollout {
        depth: u32,
        action: Action,
        source: RolloutError,
    },
    TreeDepthExceeded,
    NonTerminalOutcome,
}

impl fmt::Display for IsmctsError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ZeroIterations => formatter.write_str("iterations must be positive"),
            Self::InvalidExploration(value) => {
                write!(
                    formatter,
                    "exploration must be finite and nonnegative, got {value}"
                )
            }
            Self::NoLegalActions => formatter.write_str("the root view has no legal actions"),
            Self::NoVisitedActions => formatter.write_str("search visited no root action"),
            Self::Sample { iteration, source } => {
                write!(
                    formatter,
                    "failed to sample iteration {iteration}: {source}"
                )
            }
            Self::InvalidCurrentPlayer(player) => {
                write!(
                    formatter,
                    "cannot construct a view for current player {player}"
                )
            }
            Self::NoLegalTreeActions { depth, actor } => {
                write!(
                    formatter,
                    "player {actor} has no legal action at tree depth {depth}"
                )
            }
            Self::TreeInformationSet { depth, source } => {
                write!(
                    formatter,
                    "invalid information set at tree depth {depth}: {source}"
                )
            }
            Self::TreeAction {
                depth,
                action,
                source,
            } => write!(
                formatter,
                "tree action {action:?} was illegal at depth {depth}: {source}"
            ),
            Self::Rollout {
                depth,
                action,
                source,
            } => write!(
                formatter,
                "rollout after {action:?} at depth {depth} failed: {source}"
            ),
            Self::TreeDepthExceeded => {
                write!(formatter, "tree traversal exceeded {MAX_TREE_DEPTH} plies")
            }
            Self::NonTerminalOutcome => {
                formatter.write_str("tree simulation stopped without a terminal score")
            }
        }
    }
}

impl std::error::Error for IsmctsError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Sample { source, .. } => Some(source),
            Self::TreeInformationSet { source, .. } => Some(source),
            Self::TreeAction { source, .. } => Some(source),
            Self::Rollout { source, .. } => Some(source),
            Self::ZeroIterations
            | Self::InvalidExploration(_)
            | Self::NoLegalActions
            | Self::NoVisitedActions
            | Self::InvalidCurrentPlayer(_)
            | Self::NoLegalTreeActions { .. }
            | Self::TreeDepthExceeded
            | Self::NonTerminalOutcome => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use hanabi_core::CardId;

    #[test]
    fn availability_counts_only_worlds_where_an_action_is_legal() {
        let first = Action::Play(CardId::new(1));
        let shared = Action::Play(CardId::new(2));
        let last = Action::Discard(CardId::new(3));
        let mut node = Node::default();

        register_available_actions(&mut node, &[first, shared]);
        register_available_actions(&mut node, &[shared, last]);

        assert_eq!(
            node.edges
                .iter()
                .map(|edge| (edge.action, edge.availability))
                .collect::<Vec<_>>(),
            vec![(first, 1), (shared, 2), (last, 1)]
        );
    }

    #[test]
    fn robust_child_prefers_visits_then_utility_then_stable_order() {
        let first = Action::Play(CardId::new(1));
        let second = Action::Play(CardId::new(2));
        let mut edges = vec![Edge::new(first), Edge::new(second)];

        edges[0].observe(Reward::new(20, 20, false));
        edges[1].observe(Reward::new(25, 25, false));
        assert_eq!(robust_child(&edges), Some(second));

        edges[0].observe(Reward::new(0, 0, true));
        assert_eq!(robust_child(&edges), Some(first));

        edges[1].observe(Reward::new(0, 0, true));
        assert_eq!(robust_child(&edges), Some(second));

        let mut raw_progress = vec![Edge::new(first), Edge::new(second)];
        raw_progress[0].observe(Reward::new(0, 8, true));
        raw_progress[1].observe(Reward::new(0, 9, true));
        assert_eq!(robust_child(&raw_progress), Some(second));

        let mut tied = vec![Edge::new(first), Edge::new(second)];
        for edge in &mut tied {
            edge.observe(Reward::new(20, 20, false));
        }
        assert_eq!(robust_child(&tied), Some(first));
    }
}
