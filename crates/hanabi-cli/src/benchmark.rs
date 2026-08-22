use std::collections::BTreeMap;

use hanabi_core::{Action, Clue, PlayerView};
use hanabi_protocol::HanabiLiveReplay;
use hanabi_search::{
    InformationSet, IsmctsConfig, MonteCarloConfig, SearchDiagnostics,
    evaluate_actions_with_diagnostics, ismcts_search_with_diagnostics, select_best_action,
};
use serde::Serialize;

use crate::{BenchmarkArguments, CliError, action_label, read_replay};

const REPORT_SCHEMA_VERSION: u8 = 2;

#[derive(Serialize)]
struct BenchmarkReport {
    schema_version: u8,
    replay: String,
    policy: &'static str,
    convention: &'static str,
    base_seed: u64,
    positions: Vec<PositionReport>,
}

#[derive(Serialize)]
struct PositionReport {
    turn: u32,
    actor: String,
    actor_index: usize,
    starting_score: u8,
    clue_tokens: u8,
    strikes: u8,
    deck_size: usize,
    searches: Vec<SearchReport>,
}

#[derive(Serialize)]
struct SearchReport {
    mode: &'static str,
    budget_per_trial: u64,
    budget_unit: &'static str,
    trial_count: u32,
    action_stability: f64,
    selections: Vec<SelectionCount>,
    total_elapsed_seconds: f64,
    aggregate_throughput_per_second: f64,
    trials: Vec<TrialReport>,
}

#[derive(Clone, Serialize)]
struct SelectedAction {
    key: String,
    label: String,
}

#[derive(Serialize)]
struct SelectionCount {
    action: SelectedAction,
    count: u32,
    fraction: f64,
}

#[derive(Serialize)]
struct TrialReport {
    seed: u64,
    selected_action: SelectedAction,
    mean_official_score: f64,
    mean_raw_score: f64,
    mean_utility: f64,
    strikeout_rate: f64,
    work_units: u64,
    elapsed_seconds: f64,
    throughput_per_second: f64,
    diagnostics: DiagnosticsReport,
}

#[derive(Serialize)]
struct DiagnosticsReport {
    worlds_sampled: u64,
    candidate_state_clones: u64,
    tree_nodes_expanded: u64,
    search_actions_applied: u64,
    rollouts: u64,
    rollout_turns: u64,
    max_tree_depth: u32,
    timing_seconds: TimingReport,
}

#[derive(Serialize)]
struct TimingReport {
    total: f64,
    sampling: f64,
    tree: f64,
    rollout: f64,
    rollout_observation: f64,
    rollout_deduction: f64,
    rollout_policy: f64,
    rollout_apply: f64,
    rollout_other: f64,
}

pub(super) fn run(arguments: &BenchmarkArguments) -> Result<(), CliError> {
    let replay = read_replay(&arguments.replay)?;
    let report = build_report(arguments, &replay)?;
    let json = serde_json::to_string_pretty(&report).map_err(CliError::SerializeReport)?;
    println!("{json}");
    Ok(())
}

fn build_report(
    arguments: &BenchmarkArguments,
    replay: &HanabiLiveReplay,
) -> Result<BenchmarkReport, CliError> {
    let positions = arguments
        .turns
        .iter()
        .copied()
        .map(|turn| benchmark_position(arguments, replay, turn))
        .collect::<Result<Vec<_>, _>>()?;

    Ok(BenchmarkReport {
        schema_version: REPORT_SCHEMA_VERSION,
        replay: arguments.replay.display().to_string(),
        policy: arguments.convention.policy_id(),
        convention: arguments.convention.id(),
        base_seed: arguments.seed,
        positions,
    })
}

fn benchmark_position(
    arguments: &BenchmarkArguments,
    replay: &HanabiLiveReplay,
    turn: u32,
) -> Result<PositionReport, CliError> {
    let state = replay.state_at_turn(turn).map_err(CliError::Replay)?;
    if state.is_terminal() {
        return Err(CliError::TerminalPosition(turn));
    }
    let actor = state.current_player();
    let view = state
        .view_for(actor)
        .ok_or(CliError::InvalidCurrentPlayer)?;
    let information_set = InformationSet::new(view.clone()).map_err(CliError::InformationSet)?;
    let actor_name = replay
        .players
        .get(actor.index())
        .map_or("<unknown>", String::as_str)
        .to_owned();

    let searches = vec![
        benchmark_ismcts(arguments, replay, &view, &information_set)?,
        benchmark_flat(arguments, replay, &view, &information_set)?,
    ];

    Ok(PositionReport {
        turn,
        actor: actor_name,
        actor_index: actor.index(),
        starting_score: state.score(),
        clue_tokens: state.clue_tokens(),
        strikes: state.strikes(),
        deck_size: state.deck_size(),
        searches,
    })
}

fn benchmark_ismcts(
    arguments: &BenchmarkArguments,
    replay: &HanabiLiveReplay,
    view: &PlayerView,
    information_set: &InformationSet,
) -> Result<SearchReport, CliError> {
    let mut trials = Vec::new();
    for trial in 0..arguments.trials {
        let seed = arguments.seed.wrapping_add(u64::from(trial));
        let report = ismcts_search_with_diagnostics(
            information_set,
            &arguments.convention,
            IsmctsConfig {
                iterations: arguments.iterations,
                exploration: arguments.exploration,
                seed,
            },
        )
        .map_err(CliError::Ismcts)?;
        let elapsed_seconds = report.diagnostics.total_time.as_secs_f64();
        let diagnostics = DiagnosticsReport::from(report.diagnostics);
        let result = report.result;
        let statistics = result
            .root_actions
            .iter()
            .find(|statistics| statistics.action == result.best_action)
            .ok_or(CliError::NoBestAction)?;
        let work_units = u64::from(result.iterations);
        trials.push(TrialReport {
            seed,
            selected_action: selected_action(view, &replay.players, result.best_action),
            mean_official_score: statistics.mean_score.ok_or(CliError::NoBestAction)?,
            mean_raw_score: statistics.mean_raw_score.ok_or(CliError::NoBestAction)?,
            mean_utility: statistics.mean_utility.ok_or(CliError::NoBestAction)?,
            strikeout_rate: statistics.strikeout_rate.ok_or(CliError::NoBestAction)?,
            work_units,
            elapsed_seconds,
            throughput_per_second: throughput(work_units, elapsed_seconds),
            diagnostics,
        });
    }

    Ok(summarize(
        "ismcts",
        u64::from(arguments.iterations),
        "iterations",
        trials,
    ))
}

fn benchmark_flat(
    arguments: &BenchmarkArguments,
    replay: &HanabiLiveReplay,
    view: &PlayerView,
    information_set: &InformationSet,
) -> Result<SearchReport, CliError> {
    let mut trials = Vec::new();
    for trial in 0..arguments.trials {
        let seed = arguments.seed.wrapping_add(u64::from(trial));
        let report = evaluate_actions_with_diagnostics(
            information_set,
            &arguments.convention,
            MonteCarloConfig {
                samples_per_action: arguments.samples,
                seed,
            },
        )
        .map_err(CliError::Flat)?;
        let elapsed_seconds = report.diagnostics.total_time.as_secs_f64();
        let diagnostics = DiagnosticsReport::from(report.diagnostics);
        let evaluations = report.evaluations;
        let best_action = select_best_action(&evaluations).ok_or(CliError::NoBestAction)?;
        let evaluation = evaluations
            .iter()
            .find(|evaluation| evaluation.action == best_action)
            .ok_or(CliError::NoBestAction)?;
        let action_count = u64::try_from(evaluations.len())
            .expect("a standard position has fewer than u64 actions");
        let work_units = u64::from(arguments.samples) * action_count;
        trials.push(TrialReport {
            seed,
            selected_action: selected_action(view, &replay.players, best_action),
            mean_official_score: evaluation.mean_score,
            mean_raw_score: evaluation.mean_raw_score,
            mean_utility: evaluation.mean_utility,
            strikeout_rate: evaluation.strikeout_rate,
            work_units,
            elapsed_seconds,
            throughput_per_second: throughput(work_units, elapsed_seconds),
            diagnostics,
        });
    }

    Ok(summarize(
        "flat",
        u64::from(arguments.samples),
        "samples_per_action",
        trials,
    ))
}

fn summarize(
    mode: &'static str,
    budget_per_trial: u64,
    budget_unit: &'static str,
    trials: Vec<TrialReport>,
) -> SearchReport {
    let trial_count = u32::try_from(trials.len()).expect("trial count originated as u32");
    let total_elapsed_seconds = trials.iter().map(|trial| trial.elapsed_seconds).sum();
    let total_work = trials.iter().map(|trial| trial.work_units).sum();
    let mut counts = BTreeMap::<String, (String, u32)>::new();
    for trial in &trials {
        let entry = counts
            .entry(trial.selected_action.key.clone())
            .or_insert_with(|| (trial.selected_action.label.clone(), 0));
        entry.1 += 1;
    }
    let denominator = f64::from(trial_count);
    let mut selections = counts
        .into_iter()
        .map(|(key, (label, count))| SelectionCount {
            action: SelectedAction { key, label },
            count,
            fraction: f64::from(count) / denominator,
        })
        .collect::<Vec<_>>();
    selections.sort_by(|left, right| {
        right
            .count
            .cmp(&left.count)
            .then_with(|| left.action.key.cmp(&right.action.key))
    });
    let action_stability = selections
        .first()
        .map_or(0.0, |selection| selection.fraction);

    SearchReport {
        mode,
        budget_per_trial,
        budget_unit,
        trial_count,
        action_stability,
        selections,
        total_elapsed_seconds,
        aggregate_throughput_per_second: throughput(total_work, total_elapsed_seconds),
        trials,
    }
}

impl From<SearchDiagnostics> for DiagnosticsReport {
    fn from(diagnostics: SearchDiagnostics) -> Self {
        Self {
            worlds_sampled: diagnostics.worlds_sampled,
            candidate_state_clones: diagnostics.candidate_state_clones,
            tree_nodes_expanded: diagnostics.tree_nodes_expanded,
            search_actions_applied: diagnostics.search_actions_applied,
            rollouts: diagnostics.rollouts,
            rollout_turns: diagnostics.rollout_turns,
            max_tree_depth: diagnostics.max_tree_depth,
            timing_seconds: TimingReport {
                total: diagnostics.total_time.as_secs_f64(),
                sampling: diagnostics.sampling_time.as_secs_f64(),
                tree: diagnostics.tree_time.as_secs_f64(),
                rollout: diagnostics.rollout_time.as_secs_f64(),
                rollout_observation: diagnostics.rollout_observation_time.as_secs_f64(),
                rollout_deduction: diagnostics.rollout_deduction_time.as_secs_f64(),
                rollout_policy: diagnostics.rollout_policy_time.as_secs_f64(),
                rollout_apply: diagnostics.rollout_apply_time.as_secs_f64(),
                rollout_other: diagnostics.rollout_other_time.as_secs_f64(),
            },
        }
    }
}

fn selected_action(view: &PlayerView, players: &[String], action: Action) -> SelectedAction {
    SelectedAction {
        key: action_key(action),
        label: action_label(view, players, action),
    }
}

fn action_key(action: Action) -> String {
    match action {
        Action::Play(card) => format!("play:{}", card.index()),
        Action::Discard(card) => format!("discard:{}", card.index()),
        Action::Clue { target, clue } => match clue {
            Clue::Suit(suit) => format!("clue:suit:{suit}:{}", target.index()),
            Clue::Rank(rank) => format!("clue:rank:{rank}:{}", target.index()),
        },
    }
}

#[allow(clippy::cast_precision_loss)]
fn throughput(work_units: u64, elapsed_seconds: f64) -> f64 {
    if elapsed_seconds > 0.0 {
        work_units as f64 / elapsed_seconds
    } else {
        0.0
    }
}
