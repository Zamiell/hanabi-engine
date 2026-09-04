//! Opt-in deterministic strength benchmark. See docs/self-play.md.
use std::{
    env, fs,
    io::{BufWriter, Write},
    path::PathBuf,
    sync::{
        atomic::{AtomicUsize, Ordering},
        mpsc,
    },
    thread,
    time::Instant,
};

use hanabi_core::{Action, Clue};
use hanabi_protocol::HanabiLiveReplay;
use hanabi_search::{
    H_GROUP_RULESET_REVISION, HGroupProfile, InformationSet, PlannerConfig, PlanningObjective,
    SupportedConvention, plan_move,
};
use serde::{Deserialize, Serialize};

const NAME: &str = "h_group_max_self_play_200";

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct GameResult {
    seed: String,
    score: u8,
    stack_score: u8,
    strikes: u8,
    turns: u32,
    elapsed_seconds: f64,
    error: Option<String>,
    actions: Vec<serde_json::Value>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Baseline {
    schema_version: u8,
    name: String,
    convention: String,
    objective: String,
    exact_world_limit: u64,
    exact_node_limit: u64,
    max_turns: u32,
    ruleset_revision: String,
    total_score: u32,
    perfect_games: usize,
    /// Nonzero means this is a provisional measurement, not a clean baseline.
    engine_errors: usize,
    scores: Vec<(String, u8)>,
}

fn config() -> PlannerConfig {
    PlannerConfig {
        objective: PlanningObjective::PerfectScore,
        ..PlannerConfig::default()
    }
}

fn action_json(action: Action) -> serde_json::Value {
    match action {
        Action::Play(card) => serde_json::json!({"type": 0, "target": card.index()}),
        Action::Discard(card) => serde_json::json!({"type": 1, "target": card.index()}),
        Action::Clue { target, clue } => match clue {
            Clue::Suit(suit) => {
                serde_json::json!({"type": 2, "target": target.index(), "value": suit.index()})
            }
            Clue::Rank(rank) => {
                serde_json::json!({"type": 3, "target": target.index(), "value": rank.number()})
            }
        },
    }
}

fn play(seed: usize) -> GameResult {
    let started = Instant::now();
    let seed = format!("p4v0s{seed}");
    let json = serde_json::json!({"seed": seed, "players": ["Alice", "Bob", "Cathy", "Donald"], "actions": []});
    let mut state = HanabiLiveReplay::from_json(&json.to_string())
        .unwrap()
        .state_at_turn(0)
        .unwrap();
    let mut actions = Vec::new();
    let mut error = None;
    // Only the simulator has the seed and FullState. Decisions receive the
    // acting player's permitted observation and deterministic search budgets.
    while !state.is_terminal() {
        if state.turn() >= 200 {
            error = Some("game exceeded 200 turns without reaching a terminal state".to_owned());
            break;
        }
        let view = state.view_for(state.current_player()).unwrap();
        let decision = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let convention = SupportedConvention::HGroup(HGroupProfile::Max);
            let information = InformationSet::new(&view).map_err(|error| error.to_string())?;
            plan_move(&information, convention, config()).map_err(|error| error.to_string())
        }));
        match decision {
            Ok(Ok(result)) => {
                actions.push(action_json(result.best_action));
                if let Err(cause) = state.apply(result.best_action) {
                    error = Some(format!("illegal engine action: {cause}"));
                    break;
                }
            }
            Ok(Err(cause)) => {
                error = Some(cause);
                break;
            }
            Err(_) => {
                error = Some("engine panicked (see test stderr)".to_owned());
                break;
            }
        }
    }
    GameResult {
        seed,
        score: state.final_score().unwrap_or(0),
        stack_score: state.score(),
        strikes: state.strikes(),
        turns: state.turn(),
        elapsed_seconds: started.elapsed().as_secs_f64(),
        error,
        actions,
    }
}

fn setting(name: &str, default: usize) -> usize {
    env::var(name).map_or(default, |value| {
        value.parse().expect("benchmark setting must be an integer")
    })
}

fn baseline(games: &[GameResult]) -> Baseline {
    Baseline {
        schema_version: 1,
        name: NAME.to_owned(),
        convention: "h-group:max".to_owned(),
        objective: "perfect-score".to_owned(),
        exact_world_limit: config().exact_world_limit,
        exact_node_limit: config().exact_node_limit,
        max_turns: 200,
        ruleset_revision: H_GROUP_RULESET_REVISION.to_owned(),
        total_score: games.iter().map(|game| u32::from(game.score)).sum(),
        perfect_games: games.iter().filter(|game| game.score == 25).count(),
        engine_errors: games.iter().filter(|game| game.error.is_some()).count(),
        scores: games
            .iter()
            .map(|game| (game.seed.clone(), game.score))
            .collect(),
    }
}

#[test]
#[ignore = "200 complete self-play games; run scripts/check-self-play.sh"]
fn h_group_max_self_play_200() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let start = setting("HANABI_SELF_PLAY_START", 1);
    let count = setting("HANABI_SELF_PLAY_GAMES", 200);
    let workers = setting("HANABI_SELF_PLAY_WORKERS", 4).min(count);
    assert!(count > 0 && workers > 0);
    let update = env::var_os("HANABI_SELF_PLAY_UPDATE").is_some();
    assert!(
        !update || (start == 1 && count == 200),
        "only the full suite can update the baseline"
    );
    let output = env::var_os("HANABI_SELF_PLAY_REPORT").map_or_else(
        || root.join("target/self-play/report.json"),
        |path| root.join(path),
    );
    fs::create_dir_all(output.parent().unwrap()).unwrap();
    let mut checkpoint = BufWriter::new(fs::File::create(output.with_extension("ndjson")).unwrap());
    let next = AtomicUsize::new(start);
    let started = Instant::now();
    let mut games = thread::scope(|scope| {
        let (tx, rx) = mpsc::channel();
        for _ in 0..workers {
            let tx = tx.clone();
            let next = &next;
            scope.spawn(move || {
                loop {
                    let seed = next.fetch_add(1, Ordering::Relaxed);
                    if seed >= start + count {
                        break;
                    }
                    if tx.send(play(seed)).is_err() {
                        break;
                    }
                }
            });
        }
        drop(tx);
        let mut results = Vec::with_capacity(count);
        for result in rx {
            serde_json::to_writer(&mut checkpoint, &result).unwrap();
            writeln!(checkpoint).unwrap();
            checkpoint.flush().unwrap();
            eprintln!(
                "[{}/{}] {}: score {} ({} turns, {:.2}s){}",
                results.len() + 1,
                count,
                result.seed,
                result.score,
                result.turns,
                result.elapsed_seconds,
                result
                    .error
                    .as_ref()
                    .map_or(String::new(), |error| format!(" ERROR: {error}"))
            );
            results.push(result);
        }
        results
    });
    games.sort_by_key(|game| {
        game.seed
            .trim_start_matches("p4v0s")
            .parse::<usize>()
            .unwrap()
    });
    finish_report(
        &root,
        &output,
        &games,
        count,
        workers,
        started.elapsed().as_secs_f64(),
        update,
        start == 1 && count == 200,
    );
}

#[allow(clippy::too_many_arguments)]
fn finish_report(
    root: &std::path::Path,
    output: &std::path::Path,
    games: &[GameResult],
    count: usize,
    workers: usize,
    seconds: f64,
    update: bool,
    full_suite: bool,
) {
    let measured = baseline(games);
    let errors = games.iter().filter(|game| game.error.is_some()).count();
    let strikeouts = games.iter().filter(|game| game.strikes == 3).count();
    let baseline_path =
        root.join("crates/hanabi-search/tests/fixtures/h_group_max_self_play_200.json");
    let previous = if baseline_path.exists() {
        Some(
            serde_json::from_str::<Baseline>(&fs::read_to_string(&baseline_path).unwrap()).unwrap(),
        )
    } else {
        None
    };
    let differences = previous.as_ref().map_or_else(Vec::new, |previous| {
        games
            .iter()
            .filter_map(|game| {
                previous
                    .scores
                    .iter()
                    .find(|(seed, _)| seed == &game.seed)
                    .filter(|(_, score)| *score != game.score)
                    .map(|(_, score)| {
                        serde_json::json!({"seed": game.seed, "before": score, "after": game.score,
                    "delta": i16::from(game.score) - i16::from(*score)})
                    })
            })
            .collect::<Vec<_>>()
    });
    let report = serde_json::json!({
        "summary": measured, "elapsedSeconds": seconds, "workers": workers,
        "engineErrors": errors, "strikeouts": strikeouts,
        "differences": differences, "games": games,
    });
    fs::write(
        output,
        serde_json::to_string_pretty(&report).unwrap() + "\n",
    )
    .unwrap();
    eprintln!(
        "{}: total {}, perfect {}/{}, strikeouts {}, engine errors {}, elapsed {:.2}s; report {}",
        NAME,
        measured.total_score,
        measured.perfect_games,
        count,
        strikeouts,
        errors,
        seconds,
        output.display()
    );
    assert_eq!(games.len(), count, "every requested seed must finish");
    if update {
        fs::create_dir_all(baseline_path.parent().unwrap()).unwrap();
        fs::write(
            &baseline_path,
            serde_json::to_string_pretty(&measured).unwrap() + "\n",
        )
        .unwrap();
    } else if full_suite {
        let previous =
            previous.expect("no baseline; explicitly run with HANABI_SELF_PLAY_UPDATE=1");
        assert_no_regression(&previous, &measured);
    }
    assert_eq!(
        errors, 0,
        "engine errors invalidate a clean strength baseline; inspect the report (aborted games receive zero credit)"
    );
}

fn assert_no_regression(previous: &Baseline, measured: &Baseline) {
    assert_eq!(previous.schema_version, measured.schema_version);
    assert_eq!(previous.convention, measured.convention);
    assert_eq!(previous.objective, measured.objective);
    assert_eq!(previous.exact_world_limit, measured.exact_world_limit);
    assert_eq!(previous.exact_node_limit, measured.exact_node_limit);
    assert_eq!(previous.max_turns, measured.max_turns);
    assert_eq!(
        previous
            .scores
            .iter()
            .map(|(seed, _)| seed)
            .collect::<Vec<_>>(),
        measured
            .scores
            .iter()
            .map(|(seed, _)| seed)
            .collect::<Vec<_>>()
    );
    assert!(
        measured.total_score >= previous.total_score,
        "total score decreased: {} -> {}; inspect per-seed differences",
        previous.total_score,
        measured.total_score
    );
    assert!(
        measured.perfect_games >= previous.perfect_games,
        "perfect games decreased: {} -> {}",
        previous.perfect_games,
        measured.perfect_games
    );
}

#[test]
fn benchmark_rejects_score_and_perfect_game_decreases() {
    let mut previous = baseline(&[]);
    previous.total_score = 100;
    previous.perfect_games = 3;
    let mut measured = baseline(&[]);
    measured.total_score = 101;
    measured.perfect_games = 3;
    assert_no_regression(&previous, &measured);
    measured.total_score = 99;
    assert!(std::panic::catch_unwind(|| assert_no_regression(&previous, &measured)).is_err());
    measured.total_score = 101;
    measured.perfect_games = 2;
    assert!(std::panic::catch_unwind(|| assert_no_regression(&previous, &measured)).is_err());
    measured.perfect_games = 3;
    measured.exact_node_limit = 1;
    assert!(std::panic::catch_unwind(|| assert_no_regression(&previous, &measured)).is_err());
}
