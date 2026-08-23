use core::fmt;
use std::time::Instant;

use hanabi_core::{Action, FullState, ObservedEvent, ObservedHistoryEntry, PlayerId, RuleError};
use rand::{Rng, RngExt, SeedableRng, rngs::StdRng};

use crate::rollout::rollout_for_search;
use crate::{
    ConventionFramework, InformationSet, InformationSetError, LogicalDeductions, RolloutError,
    SampleError, SearchDiagnostics, SearchObjective, StrategicMetrics, evaluation,
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
    pub objective: SearchObjective,
}

/// Public root-edge statistics after an ISMCTS search.
#[derive(Clone, Debug, PartialEq)]
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
    pub perfect_rate: Option<f64>,
    pub mean_score_ceiling: Option<f64>,
    pub mean_clue_actions: Option<f64>,
    pub mean_clue_efficiency: Option<f64>,
    pub mean_tempo_clues: Option<f64>,
    pub mean_critical_discards: Option<f64>,
    pub mean_bottom_deck_risk: Option<f64>,
    pub mean_clue_debt: Option<f64>,
    pub mean_predictable_turns: Option<f64>,
    pub strikeout_rate: Option<f64>,
    pub min_score: Option<u8>,
    pub max_score: Option<u8>,
    pub prior: f64,
    pub principal_variation: Vec<Action>,
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

/// Persistent search tree for a single live game.
#[derive(Default)]
pub struct IsmctsSession {
    root: Node,
    history: Vec<ObservedHistoryEntry>,
    initialized: bool,
    reuse: TreeReuseDiagnostics,
}

/// Evidence that a live search successfully retained a previously explored
/// subtree after observed actions were applied.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct TreeReuseDiagnostics {
    pub advanced_actions: u32,
    pub reused_root_visits: u32,
    pub reused_nodes: u32,
}

impl IsmctsSession {
    #[must_use]
    pub const fn new() -> Self {
        Self {
            root: Node {
                visits: 0,
                edges: Vec::new(),
            },
            history: Vec::new(),
            initialized: false,
            reuse: TreeReuseDiagnostics {
                advanced_actions: 0,
                reused_root_visits: 0,
                reused_nodes: 0,
            },
        }
    }

    #[must_use]
    pub const fn reuse_diagnostics(&self) -> TreeReuseDiagnostics {
        self.reuse
    }

    /// Advances through the actions observed since the previous call, then
    /// adds fresh search evidence to the retained subtree.
    ///
    /// A history mismatch or an unexplored action resets the tree safely.
    ///
    /// # Errors
    ///
    /// Returns the same errors as [`parallel_ismcts_search_until`].
    pub fn search_until<P: ConventionFramework + Sync>(
        &mut self,
        information_set: &InformationSet,
        rollout_policy: &P,
        config: IsmctsConfig,
        threads: usize,
        deadline: Instant,
    ) -> Result<IsmctsResult, IsmctsError> {
        if threads == 0 {
            return Err(IsmctsError::ZeroThreads);
        }
        self.advance_to(information_set.view().history.as_slice());
        let result = run_batched_ismcts_with_root(
            &mut self.root,
            information_set,
            rollout_policy,
            config,
            threads,
            Some(deadline),
        )?;
        self.history.clone_from(&information_set.view().history);
        self.initialized = true;
        Ok(result)
    }

    fn advance_to(&mut self, history: &[ObservedHistoryEntry]) {
        self.reuse = TreeReuseDiagnostics::default();
        if !self.initialized {
            return;
        }
        let Some(suffix) = history.strip_prefix(self.history.as_slice()) else {
            self.reset_tree();
            return;
        };
        let actions = suffix
            .iter()
            .filter_map(|entry| observed_action(&entry.event))
            .collect::<Vec<_>>();
        if actions.is_empty() {
            return;
        }
        let mut advanced = 0_u32;
        for action in actions {
            let Some(index) = self
                .root
                .edges
                .iter()
                .position(|edge| edge.action == action)
            else {
                self.reset_tree();
                return;
            };
            let Some(child) = self.root.edges[index].child.take() else {
                self.reset_tree();
                return;
            };
            self.root = *child;
            advanced += 1;
        }
        self.reuse = TreeReuseDiagnostics {
            advanced_actions: advanced,
            reused_root_visits: self.root.visits,
            reused_nodes: self.root.node_count(),
        };
    }

    fn reset_tree(&mut self) {
        self.root = Node::default();
        self.reuse = TreeReuseDiagnostics::default();
    }
}

fn observed_action(event: &ObservedEvent) -> Option<Action> {
    match event {
        ObservedEvent::Clued { target, clue, .. } => Some(Action::Clue {
            target: *target,
            clue: *clue,
        }),
        ObservedEvent::Played { card, .. } => Some(Action::Play(*card)),
        ObservedEvent::Discarded { card, .. } => Some(Action::Discard(*card)),
        ObservedEvent::Drew { .. } => None,
    }
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

/// Runs one shared ISMCTS tree while evaluating terminal rollouts in parallel.
///
/// The configured iteration budget remains exact. Each batch reserves leaves
/// in the shared tree with a virtual visit, evaluates those independent movies
/// concurrently, and then backpropagates their results before selecting the
/// next batch.
///
/// # Errors
///
/// Returns [`IsmctsError`] for an invalid worker count, failed sampling or
/// rollout, or a worker panic.
pub fn parallel_ismcts_search<P: ConventionFramework + Sync>(
    information_set: &InformationSet,
    rollout_policy: &P,
    config: IsmctsConfig,
    threads: usize,
) -> Result<IsmctsResult, IsmctsError> {
    if threads == 0 {
        return Err(IsmctsError::ZeroThreads);
    }
    if threads == 1 {
        return ismcts_search(information_set, rollout_policy, config);
    }
    run_batched_ismcts(information_set, rollout_policy, config, threads, None)
}

/// Runs parallel ISMCTS until the iteration cap or wall-clock deadline.
///
/// At least one tree batch is completed after the paired-root prepass so a
/// legal move is always returned. `IsmctsResult::iterations` reports the
/// number of completed tree iterations, which can be lower than the configured
/// cap when the deadline wins.
///
/// # Errors
///
/// Returns the same errors as [`parallel_ismcts_search`].
pub fn parallel_ismcts_search_until<P: ConventionFramework + Sync>(
    information_set: &InformationSet,
    rollout_policy: &P,
    config: IsmctsConfig,
    threads: usize,
    deadline: Instant,
) -> Result<IsmctsResult, IsmctsError> {
    if threads == 0 {
        return Err(IsmctsError::ZeroThreads);
    }
    run_batched_ismcts(
        information_set,
        rollout_policy,
        config,
        threads,
        Some(deadline),
    )
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
    if rollout_policy.uses_paired_root_evaluation() {
        run_paired_root_evaluation(
            information_set,
            rollout_policy,
            config,
            &mut root,
            &mut rng,
            &mut diagnostics,
            measure_timing,
        )?;
    }
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
            objective: config.objective,
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

fn run_batched_ismcts<P: ConventionFramework + Sync>(
    information_set: &InformationSet,
    rollout_policy: &P,
    config: IsmctsConfig,
    threads: usize,
    deadline: Option<Instant>,
) -> Result<IsmctsResult, IsmctsError> {
    let mut root = Node::default();
    run_batched_ismcts_with_root(
        &mut root,
        information_set,
        rollout_policy,
        config,
        threads,
        deadline,
    )
}

fn run_batched_ismcts_with_root<P: ConventionFramework + Sync>(
    root: &mut Node,
    information_set: &InformationSet,
    rollout_policy: &P,
    config: IsmctsConfig,
    threads: usize,
    deadline: Option<Instant>,
) -> Result<IsmctsResult, IsmctsError> {
    validate_config(config)?;
    let root_candidates = rollout_policy.candidate_actions(information_set);
    if root_candidates.is_empty() {
        return Err(IsmctsError::NoLegalActions);
    }

    let mut rng = StdRng::seed_from_u64(config.seed);
    let mut diagnostics = SearchDiagnostics::default();
    let mut legal_actions = Vec::with_capacity(MAX_LEGAL_ACTIONS);
    if rollout_policy.uses_paired_root_evaluation() {
        run_parallel_paired_root_evaluation(
            information_set,
            rollout_policy,
            config,
            root,
            &mut rng,
            &mut diagnostics,
            threads,
        )?;
    }

    let batch_capacity = threads.min(config.iterations as usize);
    let mut completed = 0_u32;
    while completed < config.iterations {
        if completed > 0 && deadline.is_some_and(|deadline| Instant::now() >= deadline) {
            break;
        }
        let batch_size = batch_capacity.min((config.iterations - completed) as usize);
        let mut pending = Vec::with_capacity(batch_size);
        for offset in 0..batch_size {
            let iteration = completed
                + u32::try_from(offset).expect("a batch is bounded by the u32 iteration budget");
            let mut state = rollout_policy
                .sample_root_world(information_set, &mut rng)
                .map_err(|source| IsmctsError::Sample { iteration, source })?;
            diagnostics.worlds_sampled += 1;
            let mut context = SimulationContext {
                rollout_policy,
                exploration: config.exploration,
                objective: config.objective,
                rng: &mut rng,
                diagnostics: &mut diagnostics,
                legal_actions: &mut legal_actions,
                measure_timing: false,
            };
            let mut path = Vec::new();
            let requires_rollout =
                reserve_simulation(root, &mut state, 0, &mut context, &mut path)?;
            pending.push(ReservedSimulation {
                state,
                path,
                requires_rollout,
            });
        }

        let completed_batch = std::thread::scope(|scope| {
            let workers = pending
                .into_iter()
                .map(|simulation| {
                    scope.spawn(move || evaluate_reserved(simulation, rollout_policy, config))
                })
                .collect::<Vec<_>>();
            workers
                .into_iter()
                .map(|worker| worker.join().map_err(|_| IsmctsError::WorkerPanicked)?)
                .collect::<Result<Vec<_>, IsmctsError>>()
        })?;
        for completed_simulation in completed_batch {
            if completed_simulation.was_rollout {
                diagnostics.rollouts += 1;
                diagnostics.rollout_turns += completed_simulation.rollout_turns;
            }
            let mut reward = completed_simulation.reward;
            complete_reserved(
                root,
                &completed_simulation.path,
                &mut reward,
                config.objective,
            );
        }
        completed +=
            u32::try_from(batch_size).expect("a batch is bounded by the u32 iteration budget");
    }

    let root_actions = root
        .edges
        .iter()
        .filter(|edge| root_candidates.contains(&edge.action))
        .map(Edge::statistics)
        .collect::<Vec<_>>();
    let best_action = robust_child_for_actions(&root.edges, &root_candidates)
        .ok_or(IsmctsError::NoVisitedActions)?;
    Ok(IsmctsResult {
        iterations: completed,
        best_action,
        root_actions,
    })
}

fn run_parallel_paired_root_evaluation<P: ConventionFramework + Sync>(
    information_set: &InformationSet,
    policy: &P,
    config: IsmctsConfig,
    root: &mut Node,
    rng: &mut StdRng,
    diagnostics: &mut SearchDiagnostics,
    threads: usize,
) -> Result<(), IsmctsError> {
    let paired_samples = (config.iterations / 100).clamp(1, 8);
    let mut legal_actions = Vec::with_capacity(MAX_LEGAL_ACTIONS);
    for sample in 0..paired_samples {
        let world = policy
            .sample_root_world(information_set, rng)
            .map_err(|source| IsmctsError::Sample {
                iteration: sample,
                source,
            })?;
        diagnostics.worlds_sampled += 1;
        let actor = world.current_player();
        let view = world
            .view_for(actor)
            .ok_or(IsmctsError::InvalidCurrentPlayer(actor))?;
        let deductions = LogicalDeductions::new(view)
            .map_err(|source| IsmctsError::TreeInformationSet { depth: 0, source })?;
        legal_actions.extend(policy.candidate_actions(&deductions));
        let indices = register_available_actions(root, &legal_actions, &deductions, policy);
        let candidates = indices
            .as_slice()
            .iter()
            .copied()
            .map(|index| (index, root.edges[index].action))
            .collect::<Vec<_>>();
        let clone_count = candidates.len().saturating_sub(1);
        let mut states = (0..clone_count).map(|_| world.clone()).collect::<Vec<_>>();
        states.push(world);
        let mut jobs = candidates
            .into_iter()
            .zip(states)
            .map(|((index, action), state)| (index, action, state))
            .collect::<Vec<_>>();
        diagnostics.candidate_state_clones +=
            u64::try_from(clone_count).expect("a root action count fits in u64");

        while !jobs.is_empty() {
            let chunk_size = threads.min(jobs.len());
            let chunk = jobs.drain(..chunk_size).collect::<Vec<_>>();
            let completed = std::thread::scope(|scope| {
                let workers = chunk
                    .into_iter()
                    .map(|(index, action, mut candidate)| {
                        scope.spawn(move || {
                            let mut metrics = StrategicMetrics::default();
                            evaluation::observe_action(&candidate, action, &mut metrics);
                            candidate
                                .apply(action)
                                .map_err(|source| IsmctsError::TreeAction {
                                    depth: 0,
                                    action,
                                    source,
                                })?;
                            let report =
                                rollout_for_search(candidate, policy, false).map_err(|source| {
                                    IsmctsError::Rollout {
                                        depth: 0,
                                        action,
                                        source,
                                    }
                                })?;
                            let outcome = report.outcome;
                            let turns = u64::try_from(outcome.turns())
                                .expect("a rollout turn count fits in u64");
                            let mut reward = Reward::new(
                                outcome.score(),
                                outcome.raw_score(),
                                evaluation::is_strikeout(outcome.final_state()),
                                outcome.strategic_metrics(),
                                config.objective,
                                outcome.actions().to_vec(),
                            );
                            reward.prepend_action(action, metrics, config.objective);
                            Ok((index, reward, turns))
                        })
                    })
                    .collect::<Vec<_>>();
                workers
                    .into_iter()
                    .map(|worker| worker.join().map_err(|_| IsmctsError::WorkerPanicked)?)
                    .collect::<Result<Vec<_>, IsmctsError>>()
            })?;
            for (index, reward, turns) in completed {
                diagnostics.search_actions_applied += 1;
                diagnostics.rollouts += 1;
                diagnostics.rollout_turns += turns;
                root.edges[index].observe(&reward);
                root.visits += 1;
            }
        }
        legal_actions.clear();
    }
    Ok(())
}

#[derive(Clone, Copy)]
struct ReservedStep {
    edge: usize,
    action: Action,
    metrics: StrategicMetrics,
}

struct ReservedSimulation {
    state: FullState,
    path: Vec<ReservedStep>,
    requires_rollout: bool,
}

struct CompletedSimulation {
    reward: Reward,
    path: Vec<ReservedStep>,
    rollout_turns: u64,
    was_rollout: bool,
}

fn reserve_simulation<P: ConventionFramework, R: Rng + ?Sized>(
    node: &mut Node,
    state: &mut FullState,
    depth: u32,
    context: &mut SimulationContext<'_, P, R>,
    path: &mut Vec<ReservedStep>,
) -> Result<bool, IsmctsError> {
    context.diagnostics.observe_tree_depth(depth);
    if state.is_terminal() {
        node.visits += 1;
        return Ok(false);
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
    let legal_edges = register_available_actions(
        node,
        context.legal_actions,
        &deductions,
        context.rollout_policy,
    );
    let selected = select_edge(
        node,
        legal_edges.as_slice(),
        context.exploration,
        context.objective,
        context.rng,
    );
    let action = node.edges[selected].action;
    let mut metrics = StrategicMetrics::default();
    evaluation::observe_action(state, action, &mut metrics);
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
    node.visits += 1;
    node.edges[selected].reserve();
    path.push(ReservedStep {
        edge: selected,
        action,
        metrics,
    });

    if node.edges[selected].child.is_none() {
        node.edges[selected].child = Some(Box::new(Node {
            visits: 1,
            edges: Vec::new(),
        }));
        context.diagnostics.tree_nodes_expanded += 1;
        return Ok(true);
    }
    reserve_simulation(
        node.edges[selected]
            .child
            .as_deref_mut()
            .expect("the expanded child was checked as present"),
        state,
        depth + 1,
        context,
        path,
    )
}

fn evaluate_reserved<P: ConventionFramework>(
    simulation: ReservedSimulation,
    rollout_policy: &P,
    config: IsmctsConfig,
) -> Result<CompletedSimulation, IsmctsError> {
    let ReservedSimulation {
        state,
        path,
        requires_rollout,
    } = simulation;
    if !requires_rollout {
        return Ok(CompletedSimulation {
            reward: terminal_reward(&state, config.objective)
                .ok_or(IsmctsError::NonTerminalOutcome)?,
            path,
            rollout_turns: 0,
            was_rollout: false,
        });
    }
    let depth = u32::try_from(path.len().saturating_sub(1)).unwrap_or(u32::MAX);
    let action = path
        .last()
        .expect("a reserved rollout follows at least one tree action")
        .action;
    let report = rollout_for_search(state, rollout_policy, false).map_err(|source| {
        IsmctsError::Rollout {
            depth,
            action,
            source,
        }
    })?;
    let outcome = report.outcome;
    let rollout_turns = u64::try_from(outcome.turns()).expect("a rollout turn count fits in u64");
    Ok(CompletedSimulation {
        reward: Reward::new(
            outcome.score(),
            outcome.raw_score(),
            evaluation::is_strikeout(outcome.final_state()),
            outcome.strategic_metrics(),
            config.objective,
            outcome.actions().to_vec(),
        ),
        path,
        rollout_turns,
        was_rollout: true,
    })
}

fn complete_reserved(
    node: &mut Node,
    path: &[ReservedStep],
    reward: &mut Reward,
    objective: SearchObjective,
) {
    let Some((step, remaining)) = path.split_first() else {
        return;
    };
    if !remaining.is_empty() {
        complete_reserved(
            node.edges[step.edge]
                .child
                .as_deref_mut()
                .expect("a reserved path has expanded children"),
            remaining,
            reward,
            objective,
        );
    }
    reward.prepend_action(step.action, step.metrics, objective);
    node.edges[step.edge].observe_reserved(reward);
}

/// Gives every root action several matched determinizations before ordinary
/// tree growth. This common-random-numbers prepass removes a large source of
/// root noise: competing clues see the same hands and deck order.
fn run_paired_root_evaluation<P: ConventionFramework>(
    information_set: &InformationSet,
    policy: &P,
    config: IsmctsConfig,
    root: &mut Node,
    rng: &mut StdRng,
    diagnostics: &mut SearchDiagnostics,
    measure_timing: bool,
) -> Result<(), IsmctsError> {
    let paired_samples = (config.iterations / 100).clamp(1, 8);
    let mut legal_actions = Vec::with_capacity(MAX_LEGAL_ACTIONS);
    for sample in 0..paired_samples {
        let sampling_started = measure_timing.then(Instant::now);
        let world = policy
            .sample_root_world(information_set, rng)
            .map_err(|source| IsmctsError::Sample {
                iteration: sample,
                source,
            })?;
        if let Some(started) = sampling_started {
            diagnostics.sampling_time += started.elapsed();
        }
        diagnostics.worlds_sampled += 1;
        let actor = world.current_player();
        let view = world
            .view_for(actor)
            .ok_or(IsmctsError::InvalidCurrentPlayer(actor))?;
        let deductions = LogicalDeductions::new(view)
            .map_err(|source| IsmctsError::TreeInformationSet { depth: 0, source })?;
        legal_actions.extend(policy.candidate_actions(&deductions));
        let indices = register_available_actions(root, &legal_actions, &deductions, policy);
        for index in indices.as_slice().iter().copied() {
            let action = root.edges[index].action;
            let mut candidate = world.clone();
            diagnostics.candidate_state_clones += 1;
            let mut action_metrics = StrategicMetrics::default();
            evaluation::observe_action(&candidate, action, &mut action_metrics);
            candidate
                .apply(action)
                .map_err(|source| IsmctsError::TreeAction {
                    depth: 0,
                    action,
                    source,
                })?;
            diagnostics.search_actions_applied += 1;
            let report =
                rollout_for_search(candidate, policy, measure_timing).map_err(|source| {
                    IsmctsError::Rollout {
                        depth: 0,
                        action,
                        source,
                    }
                })?;
            if measure_timing {
                diagnostics.add_rollout_timing(report.diagnostics);
            }
            diagnostics.rollouts += 1;
            diagnostics.rollout_turns +=
                u64::try_from(report.outcome.turns()).expect("a rollout turn count fits in u64");
            let outcome = report.outcome;
            let mut reward = Reward::new(
                outcome.score(),
                outcome.raw_score(),
                evaluation::is_strikeout(outcome.final_state()),
                outcome.strategic_metrics(),
                config.objective,
                outcome.actions().to_vec(),
            );
            reward.prepend_action(action, action_metrics, config.objective);
            root.edges[index].observe(&reward);
            root.visits += 1;
        }
        legal_actions.clear();
    }
    Ok(())
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

impl Node {
    fn node_count(&self) -> u32 {
        1 + self
            .edges
            .iter()
            .filter_map(|edge| edge.child.as_deref())
            .map(Self::node_count)
            .sum::<u32>()
    }
}

struct Edge {
    action: Action,
    availability: u32,
    visits: u32,
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
    strikeouts: u32,
    min_score: u8,
    max_score: u8,
    child: Option<Box<Node>>,
    prior_sum: f64,
    best_utility: f64,
    principal_variation: Vec<Action>,
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
            perfects: 0,
            score_ceiling_sum: 0.0,
            clue_actions_sum: 0.0,
            clue_efficiency_sum: 0.0,
            tempo_clues_sum: 0.0,
            critical_discards_sum: 0.0,
            bottom_deck_risk_sum: 0.0,
            clue_debt_sum: 0.0,
            predictable_turns_sum: 0.0,
            strikeouts: 0,
            min_score: u8::MAX,
            max_score: u8::MIN,
            child: None,
            prior_sum: 0.0,
            best_utility: f64::NEG_INFINITY,
            principal_variation: Vec::new(),
        }
    }

    fn observe(&mut self, reward: &Reward) {
        self.visits += 1;
        self.observe_reserved(reward);
    }

    fn reserve(&mut self) {
        self.visits += 1;
    }

    fn observe_reserved(&mut self, reward: &Reward) {
        self.score_sum += f64::from(reward.score);
        self.raw_score_sum += f64::from(reward.raw_score);
        self.utility_sum += reward.utility;
        self.perfects += u32::from(reward.metrics.perfect);
        self.score_ceiling_sum += f64::from(reward.metrics.score_ceiling);
        self.clue_actions_sum += f64::from(reward.metrics.clue_actions);
        self.clue_efficiency_sum += if reward.metrics.clue_actions == 0 {
            0.0
        } else {
            f64::from(reward.metrics.newly_touched_cards) / f64::from(reward.metrics.clue_actions)
        };
        self.tempo_clues_sum += f64::from(reward.metrics.tempo_clues);
        self.critical_discards_sum += f64::from(reward.metrics.critical_discards);
        self.bottom_deck_risk_sum += reward.metrics.bottom_deck_risk;
        self.clue_debt_sum += reward.metrics.clue_debt;
        self.predictable_turns_sum += f64::from(reward.metrics.predictable_turns);
        self.min_score = self.min_score.min(reward.score);
        self.max_score = self.max_score.max(reward.score);
        if reward.strikeout {
            self.strikeouts += 1;
        }
        if reward.utility > self.best_utility {
            self.best_utility = reward.utility;
            self.principal_variation.clone_from(&reward.line);
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
            perfect_rate: visited.then(|| f64::from(self.perfects) / f64::from(self.visits)),
            mean_score_ceiling: visited.then(|| self.score_ceiling_sum / f64::from(self.visits)),
            mean_clue_actions: visited.then(|| self.clue_actions_sum / f64::from(self.visits)),
            mean_clue_efficiency: visited
                .then(|| self.clue_efficiency_sum / f64::from(self.visits)),
            mean_tempo_clues: visited.then(|| self.tempo_clues_sum / f64::from(self.visits)),
            mean_critical_discards: visited
                .then(|| self.critical_discards_sum / f64::from(self.visits)),
            mean_bottom_deck_risk: visited
                .then(|| self.bottom_deck_risk_sum / f64::from(self.visits)),
            mean_clue_debt: visited.then(|| self.clue_debt_sum / f64::from(self.visits)),
            mean_predictable_turns: visited
                .then(|| self.predictable_turns_sum / f64::from(self.visits)),
            strikeout_rate: visited.then(|| f64::from(self.strikeouts) / f64::from(self.visits)),
            min_score: visited.then_some(self.min_score),
            max_score: visited.then_some(self.max_score),
            prior: if self.availability == 0 {
                0.0
            } else {
                self.prior_sum / f64::from(self.availability)
            },
            principal_variation: self.principal_variation.clone(),
        }
    }
}

#[derive(Clone)]
struct Reward {
    score: u8,
    raw_score: u8,
    strikeout: bool,
    metrics: StrategicMetrics,
    utility: f64,
    line: Vec<Action>,
}

impl Reward {
    fn new(
        score: u8,
        raw_score: u8,
        strikeout: bool,
        metrics: StrategicMetrics,
        objective: SearchObjective,
        line: Vec<Action>,
    ) -> Self {
        Self {
            score,
            raw_score,
            strikeout,
            metrics,
            utility: objective.utility(score, raw_score, metrics),
            line,
        }
    }

    fn prepend_action(
        &mut self,
        action: Action,
        action_metrics: StrategicMetrics,
        objective: SearchObjective,
    ) {
        self.metrics.clue_actions = self
            .metrics
            .clue_actions
            .saturating_add(action_metrics.clue_actions);
        self.metrics.newly_touched_cards = self
            .metrics
            .newly_touched_cards
            .saturating_add(action_metrics.newly_touched_cards);
        self.metrics.tempo_clues = self
            .metrics
            .tempo_clues
            .saturating_add(action_metrics.tempo_clues);
        self.metrics.critical_discards = self
            .metrics
            .critical_discards
            .saturating_add(action_metrics.critical_discards);
        self.metrics.bottom_deck_risk += action_metrics.bottom_deck_risk;
        self.metrics.clue_debt += action_metrics.clue_debt;
        self.metrics.evaluated_positions = self
            .metrics
            .evaluated_positions
            .saturating_add(action_metrics.evaluated_positions);
        self.utility = objective.utility(self.score, self.raw_score, self.metrics);
        self.line.insert(0, action);
    }
}

struct SimulationContext<'a, P, R: ?Sized> {
    rollout_policy: &'a P,
    exploration: f64,
    objective: SearchObjective,
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
        return terminal_reward(&state, context.objective).ok_or(IsmctsError::NonTerminalOutcome);
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

    let legal_edges = register_available_actions(
        node,
        context.legal_actions,
        &deductions,
        context.rollout_policy,
    );
    let selected = select_edge(
        node,
        legal_edges.as_slice(),
        context.exploration,
        context.objective,
        context.rng,
    );
    let action = node.edges[selected].action;
    let mut action_metrics = StrategicMetrics::default();
    evaluation::observe_action(&state, action, &mut action_metrics);
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
            evaluation::is_strikeout(outcome.final_state()),
            outcome.strategic_metrics(),
            context.objective,
            outcome.actions().to_vec(),
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

    let mut reward = reward;
    reward.prepend_action(action, action_metrics, context.objective);
    node.visits += 1;
    node.edges[selected].observe(&reward);
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

fn register_available_actions<P: ConventionFramework>(
    node: &mut Node,
    legal_actions: &[Action],
    deductions: &LogicalDeductions,
    policy: &P,
) -> LegalEdges {
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
        node.edges[index].prior_sum += policy.action_prior(deductions, *action).max(0.0);
        legal_edges.indices[legal_edges.len] = index;
        legal_edges.len += 1;
    }
    legal_edges
}

fn select_edge<R: Rng + ?Sized>(
    node: &Node,
    legal_edges: &[usize],
    exploration: f64,
    objective: SearchObjective,
    rng: &mut R,
) -> usize {
    let unexpanded_count = legal_edges
        .iter()
        .copied()
        .filter(|index| node.edges[*index].child.is_none())
        .count();
    if unexpanded_count > 0 {
        let unexpanded = legal_edges
            .iter()
            .copied()
            .filter(|index| node.edges[*index].child.is_none())
            .collect::<Vec<_>>();
        let total = unexpanded
            .iter()
            .map(|index| node.edges[*index].prior_sum.max(0.001))
            .sum::<f64>();
        let mut draw = rng.random::<f64>() * total;
        for index in unexpanded.iter().copied() {
            draw -= node.edges[index].prior_sum.max(0.001);
            if draw <= 0.0 {
                return index;
            }
        }
        return *unexpanded.last().expect("unexpanded edges were counted");
    }

    let total_prior = legal_edges
        .iter()
        .map(|index| node.edges[*index].prior_sum / f64::from(node.edges[*index].availability))
        .sum::<f64>()
        .max(f64::EPSILON);
    let mut selected = legal_edges[0];
    let mut best_value = f64::NEG_INFINITY;
    for index in legal_edges.iter().copied() {
        let edge = &node.edges[index];
        let prior = (edge.prior_sum / f64::from(edge.availability)) / total_prior;
        let exploration_bonus = exploration * prior * f64::from(node.visits.max(1)).sqrt()
            / (1.0 + f64::from(edge.visits));
        let value = edge.mean_utility() / objective.normalization() + exploration_bonus;
        if value > best_value {
            selected = index;
            best_value = value;
        }
    }
    selected
}

fn robust_child(edges: &[Edge]) -> Option<Action> {
    robust_child_matching(edges, |_| true)
}

fn robust_child_for_actions(edges: &[Edge], actions: &[Action]) -> Option<Action> {
    robust_child_matching(edges, |edge| actions.contains(&edge.action))
}

fn robust_child_matching(edges: &[Edge], include: impl Fn(&Edge) -> bool) -> Option<Action> {
    let mut best: Option<&Edge> = None;
    for edge in edges.iter().filter(|edge| edge.visits > 0 && include(edge)) {
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

fn terminal_reward(state: &FullState, objective: SearchObjective) -> Option<Reward> {
    let metrics = evaluation::finish_metrics(state, StrategicMetrics::default());
    Some(Reward::new(
        state.final_score()?,
        state.score(),
        evaluation::is_strikeout(state),
        metrics,
        objective,
        Vec::new(),
    ))
}

/// Why ISMCTS could not complete.
#[derive(Debug, PartialEq)]
pub enum IsmctsError {
    ZeroIterations,
    ZeroThreads,
    WorkerPanicked,
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
            Self::ZeroThreads => formatter.write_str("search threads must be positive"),
            Self::WorkerPanicked => formatter.write_str("a parallel search worker panicked"),
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
            | Self::ZeroThreads
            | Self::WorkerPanicked
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
    use hanabi_core::{CardId, PlayerId, standard_deck};

    fn deductions() -> LogicalDeductions {
        LogicalDeductions::new(
            FullState::new_standard(2, standard_deck())
                .unwrap()
                .view_for(PlayerId::new(0))
                .unwrap(),
        )
        .unwrap()
    }

    fn reward(score: u8, raw_score: u8, strikeout: bool) -> Reward {
        Reward::new(
            score,
            raw_score,
            strikeout,
            StrategicMetrics {
                perfect: score == 25,
                score_ceiling: 25,
                ..StrategicMetrics::default()
            },
            SearchObjective::ExpectedScore,
            Vec::new(),
        )
    }

    #[test]
    fn parallel_batches_preserve_the_exact_iteration_cap() {
        let deductions = deductions();
        let config = IsmctsConfig {
            iterations: 17,
            exploration: core::f64::consts::SQRT_2,
            seed: 7,
            objective: SearchObjective::ExpectedScore,
        };
        let first = parallel_ismcts_search(
            &InformationSet::new(deductions.view().clone()).unwrap(),
            &crate::ConventionAgnosticPolicy,
            config,
            2,
        )
        .unwrap();
        let second = parallel_ismcts_search(
            &InformationSet::new(deductions.view().clone()).unwrap(),
            &crate::ConventionAgnosticPolicy,
            config,
            2,
        )
        .unwrap();
        assert_eq!(first.iterations, 17);
        assert_eq!(first, second);
    }

    #[test]
    fn deadline_returns_the_first_completed_batch() {
        let deductions = deductions();
        let result = parallel_ismcts_search_until(
            &InformationSet::new(deductions.view().clone()).unwrap(),
            &crate::ConventionAgnosticPolicy,
            IsmctsConfig {
                iterations: 100,
                exploration: core::f64::consts::SQRT_2,
                seed: 7,
                objective: SearchObjective::ExpectedScore,
            },
            2,
            Instant::now(),
        )
        .unwrap();
        assert_eq!(result.iterations, 2);
    }

    #[test]
    fn live_session_reroots_through_observed_actions() {
        let mut state = FullState::new_standard(2, standard_deck()).unwrap();
        let initial = state.view_for(PlayerId::new(0)).unwrap();
        let first = state
            .legal_actions()
            .into_iter()
            .find(|action| matches!(action, Action::Clue { .. }))
            .unwrap();
        state.apply(first).unwrap();
        let second = state
            .legal_actions()
            .into_iter()
            .find(|action| matches!(action, Action::Clue { .. }))
            .unwrap();
        state.apply(second).unwrap();

        let retained = Node {
            visits: 7,
            edges: Vec::new(),
        };
        let mut second_edge = Edge::new(second);
        second_edge.child = Some(Box::new(retained));
        second_edge.visits = 7;
        let middle = Node {
            visits: 7,
            edges: vec![second_edge],
        };
        let mut first_edge = Edge::new(first);
        first_edge.child = Some(Box::new(middle));
        first_edge.visits = 7;
        let mut session = IsmctsSession {
            root: Node {
                visits: 7,
                edges: vec![first_edge],
            },
            history: initial.history,
            initialized: true,
            reuse: TreeReuseDiagnostics::default(),
        };
        let current = state.view_for(PlayerId::new(0)).unwrap();
        session.advance_to(&current.history);
        assert_eq!(
            session.reuse_diagnostics(),
            TreeReuseDiagnostics {
                advanced_actions: 2,
                reused_root_visits: 7,
                reused_nodes: 1,
            }
        );
    }

    #[test]
    fn availability_counts_only_worlds_where_an_action_is_legal() {
        let first = Action::Play(CardId::new(1));
        let shared = Action::Play(CardId::new(2));
        let last = Action::Discard(CardId::new(3));
        let mut node = Node::default();

        let deductions = deductions();
        register_available_actions(
            &mut node,
            &[first, shared],
            &deductions,
            &crate::ConventionAgnosticPolicy,
        );
        register_available_actions(
            &mut node,
            &[shared, last],
            &deductions,
            &crate::ConventionAgnosticPolicy,
        );

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

        edges[0].observe(&reward(20, 20, false));
        edges[1].observe(&reward(25, 25, false));
        assert_eq!(robust_child(&edges), Some(second));

        edges[0].observe(&reward(0, 0, true));
        assert_eq!(robust_child(&edges), Some(first));

        edges[1].observe(&reward(0, 0, true));
        assert_eq!(robust_child(&edges), Some(second));

        let mut raw_progress = vec![Edge::new(first), Edge::new(second)];
        raw_progress[0].observe(&reward(0, 8, true));
        raw_progress[1].observe(&reward(0, 9, true));
        assert_eq!(robust_child(&raw_progress), Some(second));

        let mut tied = vec![Edge::new(first), Edge::new(second)];
        for edge in &mut tied {
            edge.observe(&reward(20, 20, false));
        }
        assert_eq!(robust_child(&tied), Some(first));
    }
}
