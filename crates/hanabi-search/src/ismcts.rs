use core::fmt;

use hanabi_core::{Action, EndReason, FullState, GameStatus, PlayerId, RuleError};
use rand::{Rng, RngExt, SeedableRng, rngs::StdRng};

use crate::{InformationSet, RolloutError, RolloutPolicy, SampleError, rollout_to_terminal};

const MAX_TREE_DEPTH: u32 = 512;
const PERFECT_SCORE: f64 = 25.0;

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
    pub mean_score: Option<f64>,
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

/// Runs cooperative single-observer information-set Monte Carlo tree search.
///
/// Every iteration samples a new root-consistent [`FullState`]. Tree nodes are
/// shared by action history, while edges track how often an action was legally
/// available across determinizations. Selection uses availability-aware UCB,
/// expansion adds one node, and the remaining position is completed by
/// `rollout_policy`. All players backpropagate the same official team score.
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
pub fn ismcts_search<P: RolloutPolicy>(
    information_set: &InformationSet,
    rollout_policy: &P,
    config: IsmctsConfig,
) -> Result<IsmctsResult, IsmctsError> {
    validate_config(config)?;
    if information_set.view().legal_actions().is_empty() {
        return Err(IsmctsError::NoLegalActions);
    }

    let mut root = Node::default();
    let mut rng = StdRng::seed_from_u64(config.seed);
    for iteration in 0..config.iterations {
        let state = information_set
            .sample(&mut rng)
            .map_err(|source| IsmctsError::Sample { iteration, source })?;
        simulate(
            &mut root,
            state,
            rollout_policy,
            config.exploration,
            &mut rng,
            0,
        )?;
    }

    let root_actions = root.edges.iter().map(Edge::statistics).collect::<Vec<_>>();
    let best_action = robust_child(&root.edges).ok_or(IsmctsError::NoVisitedActions)?;
    Ok(IsmctsResult {
        iterations: config.iterations,
        best_action,
        root_actions,
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
            strikeouts: 0,
            min_score: u8::MAX,
            max_score: u8::MIN,
            child: None,
        }
    }

    fn observe(&mut self, reward: Reward) {
        self.visits += 1;
        self.score_sum += f64::from(reward.score);
        self.min_score = self.min_score.min(reward.score);
        self.max_score = self.max_score.max(reward.score);
        if reward.strikeout {
            self.strikeouts += 1;
        }
    }

    fn mean_score(&self) -> f64 {
        self.score_sum / f64::from(self.visits)
    }

    fn statistics(&self) -> TreeActionStatistics {
        let visited = self.visits > 0;
        TreeActionStatistics {
            action: self.action,
            visits: self.visits,
            availability: self.availability,
            mean_score: visited.then(|| self.mean_score()),
            strikeout_rate: visited.then(|| f64::from(self.strikeouts) / f64::from(self.visits)),
            min_score: visited.then_some(self.min_score),
            max_score: visited.then_some(self.max_score),
        }
    }
}

#[derive(Clone, Copy)]
struct Reward {
    score: u8,
    strikeout: bool,
}

fn simulate<P: RolloutPolicy, R: Rng + ?Sized>(
    node: &mut Node,
    mut state: FullState,
    rollout_policy: &P,
    exploration: f64,
    rng: &mut R,
    depth: u32,
) -> Result<Reward, IsmctsError> {
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
    let legal_actions = view.legal_actions();
    if legal_actions.is_empty() {
        return Err(IsmctsError::NoLegalTreeActions { depth, actor });
    }

    let legal_edges = register_available_actions(node, &legal_actions);
    let selected = select_edge(node, &legal_edges, exploration, rng);
    let action = node.edges[selected].action;
    state
        .apply(action)
        .map_err(|source| IsmctsError::TreeAction {
            depth,
            action,
            source,
        })?;

    let reward = if node.edges[selected].child.is_none() {
        let outcome =
            rollout_to_terminal(state, rollout_policy).map_err(|source| IsmctsError::Rollout {
                depth,
                action,
                source,
            })?;
        node.edges[selected].child = Some(Box::new(Node {
            visits: 1,
            edges: Vec::new(),
        }));
        Reward {
            score: outcome.score(),
            strikeout: outcome.final_state().status()
                == GameStatus::Finished(EndReason::TooManyStrikes),
        }
    } else {
        simulate(
            node.edges[selected]
                .child
                .as_deref_mut()
                .expect("the expanded child was checked as present"),
            state,
            rollout_policy,
            exploration,
            rng,
            depth + 1,
        )?
    };

    node.visits += 1;
    node.edges[selected].observe(reward);
    Ok(reward)
}

fn register_available_actions(node: &mut Node, legal_actions: &[Action]) -> Vec<usize> {
    legal_actions
        .iter()
        .map(|action| {
            let index = node
                .edges
                .iter()
                .position(|edge| edge.action == *action)
                .unwrap_or_else(|| {
                    node.edges.push(Edge::new(*action));
                    node.edges.len() - 1
                });
            node.edges[index].availability += 1;
            index
        })
        .collect()
}

fn select_edge<R: Rng + ?Sized>(
    node: &Node,
    legal_edges: &[usize],
    exploration: f64,
    rng: &mut R,
) -> usize {
    let unexpanded = legal_edges
        .iter()
        .copied()
        .filter(|index| node.edges[*index].child.is_none())
        .collect::<Vec<_>>();
    if !unexpanded.is_empty() {
        return unexpanded[rng.random_range(0..unexpanded.len())];
    }

    let mut selected = legal_edges[0];
    let mut best_value = f64::NEG_INFINITY;
    for index in legal_edges.iter().copied() {
        let edge = &node.edges[index];
        let availability = f64::from(edge.availability);
        let exploration_bonus = exploration * (availability.ln() / f64::from(edge.visits)).sqrt();
        let value = edge.mean_score() / PERFECT_SCORE + exploration_bonus;
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
                || (edge.visits == current.visits && edge.mean_score() > current.mean_score())
        });
        if replace {
            best = Some(edge);
        }
    }
    best.map(|edge| edge.action)
}

fn terminal_reward(state: &FullState) -> Option<Reward> {
    Some(Reward {
        score: state.final_score()?,
        strikeout: state.status() == GameStatus::Finished(EndReason::TooManyStrikes),
    })
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
    fn robust_child_prefers_visits_then_score_then_stable_order() {
        let first = Action::Play(CardId::new(1));
        let second = Action::Play(CardId::new(2));
        let mut edges = vec![Edge::new(first), Edge::new(second)];

        edges[0].observe(Reward {
            score: 20,
            strikeout: false,
        });
        edges[1].observe(Reward {
            score: 25,
            strikeout: false,
        });
        assert_eq!(robust_child(&edges), Some(second));

        edges[0].observe(Reward {
            score: 0,
            strikeout: true,
        });
        assert_eq!(robust_child(&edges), Some(first));

        edges[1].observe(Reward {
            score: 0,
            strikeout: true,
        });
        assert_eq!(robust_child(&edges), Some(second));

        let mut tied = vec![Edge::new(first), Edge::new(second)];
        for edge in &mut tied {
            edge.observe(Reward {
                score: 20,
                strikeout: false,
            });
        }
        assert_eq!(robust_child(&tied), Some(first));
    }
}
