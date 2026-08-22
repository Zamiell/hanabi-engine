use core::fmt;
use std::{
    env, fs, io,
    path::{Path, PathBuf},
    process::ExitCode,
    str::FromStr,
    time::Instant,
};

use hanabi_core::{Action, CardId, Clue, FullState, PlayerView};
use hanabi_protocol::{HanabiLiveReplay, ReplayError};
use hanabi_search::{
    InformationSet, InformationSetError, IsmctsConfig, IsmctsError, MonteCarloConfig,
    SearchError as FlatSearchError, SupportedConvention, TreeActionStatistics, evaluate_actions,
    ismcts_search, select_best_action,
};

mod benchmark;

const DEFAULT_ITERATIONS: u32 = 1_000;
const DEFAULT_SAMPLES: u32 = 100;
const DEFAULT_SEED: u64 = 0;
const DEFAULT_TRIALS: u32 = 5;

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("error: {error}");
            if matches!(error, CliError::Usage(_)) {
                eprintln!();
                print_usage_to_stderr();
            }
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<(), CliError> {
    let Some(command) = parse_arguments()? else {
        print_usage();
        return Ok(());
    };

    match command {
        Command::Analyze(arguments) => run_analyze(&arguments),
        Command::Benchmark(arguments) => benchmark::run(&arguments),
    }
}

fn run_analyze(arguments: &AnalyzeArguments) -> Result<(), CliError> {
    let replay = read_replay(&arguments.replay)?;
    let state = replay
        .state_at_turn(arguments.turn)
        .map_err(CliError::Replay)?;
    if state.is_terminal() {
        return Err(CliError::TerminalPosition(arguments.turn));
    }

    let actor = state.current_player();
    let view = state
        .view_for(actor)
        .ok_or(CliError::InvalidCurrentPlayer)?;
    let information_set = InformationSet::new(view.clone()).map_err(CliError::InformationSet)?;

    print_position(&replay, &state, &view);
    println!("Convention: {}", arguments.convention);
    match arguments.mode {
        SearchMode::Ismcts => analyze_ismcts(arguments, &view, &replay.players, &information_set),
        SearchMode::Flat => analyze_flat(arguments, &view, &replay.players, &information_set),
    }
}

fn read_replay(path: &Path) -> Result<HanabiLiveReplay, CliError> {
    let json = fs::read_to_string(path).map_err(|source| CliError::ReadReplay {
        path: path.to_path_buf(),
        source,
    })?;
    HanabiLiveReplay::from_json(&json).map_err(CliError::Replay)
}

fn analyze_ismcts(
    arguments: &AnalyzeArguments,
    view: &PlayerView,
    players: &[String],
    information_set: &InformationSet,
) -> Result<(), CliError> {
    let started = Instant::now();
    let result = ismcts_search(
        information_set,
        &arguments.convention,
        IsmctsConfig {
            iterations: arguments.iterations,
            exploration: arguments.exploration,
            seed: arguments.seed,
        },
    )
    .map_err(CliError::Ismcts)?;
    let elapsed = started.elapsed();

    println!(
        "Search: ISMCTS, {} iterations, seed {}, exploration {:.4}",
        result.iterations, arguments.seed, arguments.exploration
    );
    println!(
        "Elapsed: {:.3}s ({:.0} iterations/s)",
        elapsed.as_secs_f64(),
        f64::from(result.iterations) / elapsed.as_secs_f64()
    );
    println!();
    println!(
        "   {:<42} {:>7} {:>7} {:>8} {:>7} {:>8} {:>10} {:>7}",
        "Action", "Visits", "Avail", "Official", "Raw", "Utility", "Strikeout", "Range"
    );

    let mut statistics = result.root_actions;
    statistics.sort_by(|left, right| {
        right.visits.cmp(&left.visits).then_with(|| {
            right
                .mean_utility
                .unwrap_or(f64::NEG_INFINITY)
                .total_cmp(&left.mean_utility.unwrap_or(f64::NEG_INFINITY))
        })
    });
    for entry in statistics {
        let marker = if entry.action == result.best_action {
            '*'
        } else {
            ' '
        };
        print_tree_row(marker, &action_label(view, players, entry.action), entry);
    }
    Ok(())
}

fn analyze_flat(
    arguments: &AnalyzeArguments,
    view: &PlayerView,
    players: &[String],
    information_set: &InformationSet,
) -> Result<(), CliError> {
    let started = Instant::now();
    let mut evaluations = evaluate_actions(
        information_set,
        &arguments.convention,
        MonteCarloConfig {
            samples_per_action: arguments.samples,
            seed: arguments.seed,
        },
    )
    .map_err(CliError::Flat)?;
    let elapsed = started.elapsed();
    let best = select_best_action(&evaluations).ok_or(CliError::NoBestAction)?;
    let action_count =
        u32::try_from(evaluations.len()).expect("a standard position has fewer than u32 actions");
    let simulations = u64::from(arguments.samples) * u64::from(action_count);

    println!(
        "Search: flat Monte Carlo, {} samples/action, seed {} ({simulations} rollouts)",
        arguments.samples, arguments.seed,
    );
    println!(
        "Elapsed: {:.3}s ({:.0} rollouts/s)",
        elapsed.as_secs_f64(),
        f64::from(arguments.samples) * f64::from(action_count) / elapsed.as_secs_f64()
    );
    println!();
    println!(
        "   {:<42} {:>8} {:>7} {:>8} {:>10} {:>10} {:>7}",
        "Action", "Official", "Raw", "Utility", "Variance", "Strikeout", "Range"
    );

    evaluations.sort_by(|left, right| right.mean_utility.total_cmp(&left.mean_utility));
    for entry in evaluations {
        let marker = if entry.action == best { '*' } else { ' ' };
        let range = format!("{}-{}", entry.min_score, entry.max_score);
        println!(
            "{marker}  {:<42} {:>8.3} {:>7.3} {:>8.3} {:>10.3} {:>9.1}% {range:>7}",
            action_label(view, players, entry.action),
            entry.mean_score,
            entry.mean_raw_score,
            entry.mean_utility,
            entry.score_variance,
            entry.strikeout_rate * 100.0,
        );
    }
    Ok(())
}

fn print_position(replay: &HanabiLiveReplay, state: &FullState, view: &PlayerView) {
    let actor = state.current_player();
    let actor_name = replay
        .players
        .get(actor.index())
        .map_or("<unknown>", String::as_str);
    println!("Hanabi Live position");
    println!(
        "Turn: {}  Actor: {} ({})  Score: {}  Clues: {}  Strikes: {}  Deck: {}",
        state.turn(),
        actor_name,
        actor,
        state.score(),
        state.clue_tokens(),
        state.strikes(),
        state.deck_size()
    );
    println!(
        "Own hand: {} cards; slot 1 is newest",
        view.hands[view.observer.index()].len()
    );
    println!();
}

fn print_tree_row(marker: char, label: &str, entry: TreeActionStatistics) {
    let official = entry
        .mean_score
        .map_or_else(|| "-".to_owned(), |value| format!("{value:.3}"));
    let raw = entry
        .mean_raw_score
        .map_or_else(|| "-".to_owned(), |value| format!("{value:.3}"));
    let utility = entry
        .mean_utility
        .map_or_else(|| "-".to_owned(), |value| format!("{value:.3}"));
    let strikeout = entry
        .strikeout_rate
        .map_or_else(|| "-".to_owned(), |value| format!("{:.1}%", value * 100.0));
    let range = match (entry.min_score, entry.max_score) {
        (Some(minimum), Some(maximum)) => format!("{minimum}-{maximum}"),
        _ => "-".to_owned(),
    };
    println!(
        "{marker}  {label:<42} {:>7} {:>7} {official:>8} {raw:>7} {utility:>8} {strikeout:>10} {range:>7}",
        entry.visits, entry.availability
    );
}

fn action_label(view: &PlayerView, players: &[String], action: Action) -> String {
    match action {
        Action::Play(card) => format_card_action("Play", view, card),
        Action::Discard(card) => format_card_action("Discard", view, card),
        Action::Clue { target, clue } => {
            let clue = match clue {
                Clue::Suit(suit) => suit.to_string(),
                Clue::Rank(rank) => rank.to_string(),
            };
            let name = players
                .get(target.index())
                .map_or("<unknown>", String::as_str);
            format!("Clue {clue} to {name} ({target})")
        }
    }
}

fn format_card_action(verb: &str, view: &PlayerView, card: CardId) -> String {
    let hand = &view.hands[view.observer.index()];
    let Some(index) = hand.iter().position(|observed| observed.id == card) else {
        return format!("{verb} {card}");
    };
    let slot = hand.len() - index;
    let age = if hand.len() == 1 {
        "only card".to_owned()
    } else if index == 0 {
        "oldest".to_owned()
    } else if index + 1 == hand.len() {
        "newest".to_owned()
    } else {
        format!("age {}/{}", index + 1, hand.len())
    };
    format!("{verb} slot {slot} ({age}, {card})")
}

#[derive(Clone, Copy)]
enum SearchMode {
    Ismcts,
    Flat,
}

impl FromStr for SearchMode {
    type Err = CliError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "ismcts" => Ok(Self::Ismcts),
            "flat" => Ok(Self::Flat),
            _ => Err(CliError::Usage(format!(
                "unknown search mode {value:?}; expected ismcts or flat"
            ))),
        }
    }
}

struct AnalyzeArguments {
    replay: PathBuf,
    turn: u32,
    mode: SearchMode,
    iterations: u32,
    samples: u32,
    seed: u64,
    exploration: f64,
    convention: SupportedConvention,
}

struct BenchmarkArguments {
    replay: PathBuf,
    turns: Vec<u32>,
    trials: u32,
    iterations: u32,
    samples: u32,
    seed: u64,
    exploration: f64,
    convention: SupportedConvention,
}

enum Command {
    Analyze(AnalyzeArguments),
    Benchmark(BenchmarkArguments),
}

fn parse_arguments() -> Result<Option<Command>, CliError> {
    let mut arguments = env::args().skip(1);
    let Some(command) = arguments.next() else {
        return Ok(None);
    };
    if command == "help" || command == "--help" || command == "-h" {
        return Ok(None);
    }
    match command.as_str() {
        "analyze" => {
            parse_analyze_arguments(&mut arguments).map(|value| value.map(Command::Analyze))
        }
        "benchmark" => {
            parse_benchmark_arguments(&mut arguments).map(|value| value.map(Command::Benchmark))
        }
        _ => Err(CliError::Usage(format!("unknown command {command:?}"))),
    }
}

fn parse_analyze_arguments(
    arguments: &mut impl Iterator<Item = String>,
) -> Result<Option<AnalyzeArguments>, CliError> {
    let Some(replay) = arguments.next() else {
        return Err(CliError::Usage("missing replay JSON path".to_owned()));
    };
    if replay == "--help" || replay == "-h" {
        return Ok(None);
    }

    let mut turn = None;
    let mut mode = SearchMode::Ismcts;
    let mut iterations = DEFAULT_ITERATIONS;
    let mut samples = DEFAULT_SAMPLES;
    let mut seed = DEFAULT_SEED;
    let mut exploration = core::f64::consts::SQRT_2;
    let mut convention = SupportedConvention::default();

    while let Some(flag) = arguments.next() {
        match flag.as_str() {
            "--turn" => {
                turn = Some(parse_value("--turn", &next_value(arguments, "--turn")?)?);
            }
            "--mode" => mode = next_value(arguments, "--mode")?.parse()?,
            "--iterations" => {
                iterations = parse_value("--iterations", &next_value(arguments, "--iterations")?)?;
            }
            "--samples" => {
                samples = parse_value("--samples", &next_value(arguments, "--samples")?)?;
            }
            "--seed" => seed = parse_value("--seed", &next_value(arguments, "--seed")?)?,
            "--exploration" => {
                exploration =
                    parse_value("--exploration", &next_value(arguments, "--exploration")?)?;
            }
            "--convention" => {
                convention = parse_value("--convention", &next_value(arguments, "--convention")?)?;
            }
            "--help" | "-h" => return Ok(None),
            _ => return Err(CliError::Usage(format!("unknown option {flag:?}"))),
        }
    }

    Ok(Some(AnalyzeArguments {
        replay: replay.into(),
        turn: turn.ok_or_else(|| CliError::Usage("missing required --turn".to_owned()))?,
        mode,
        iterations,
        samples,
        seed,
        exploration,
        convention,
    }))
}

fn parse_benchmark_arguments(
    arguments: &mut impl Iterator<Item = String>,
) -> Result<Option<BenchmarkArguments>, CliError> {
    let Some(replay) = arguments.next() else {
        return Err(CliError::Usage("missing replay JSON path".to_owned()));
    };
    if replay == "--help" || replay == "-h" {
        return Ok(None);
    }

    let mut turns = Vec::new();
    let mut trials = DEFAULT_TRIALS;
    let mut iterations = DEFAULT_ITERATIONS;
    let mut samples = DEFAULT_SAMPLES;
    let mut seed = DEFAULT_SEED;
    let mut exploration = core::f64::consts::SQRT_2;
    let mut convention = SupportedConvention::default();

    while let Some(flag) = arguments.next() {
        match flag.as_str() {
            "--turn" => turns.push(parse_value("--turn", &next_value(arguments, "--turn")?)?),
            "--trials" => {
                trials = parse_value("--trials", &next_value(arguments, "--trials")?)?;
            }
            "--iterations" => {
                iterations = parse_value("--iterations", &next_value(arguments, "--iterations")?)?;
            }
            "--samples" => {
                samples = parse_value("--samples", &next_value(arguments, "--samples")?)?;
            }
            "--seed" => seed = parse_value("--seed", &next_value(arguments, "--seed")?)?,
            "--exploration" => {
                exploration =
                    parse_value("--exploration", &next_value(arguments, "--exploration")?)?;
            }
            "--convention" => {
                convention = parse_value("--convention", &next_value(arguments, "--convention")?)?;
            }
            "--help" | "-h" => return Ok(None),
            _ => return Err(CliError::Usage(format!("unknown option {flag:?}"))),
        }
    }

    if turns.is_empty() {
        return Err(CliError::Usage(
            "missing required --turn; repeat it to benchmark multiple positions".to_owned(),
        ));
    }
    if trials == 0 {
        return Err(CliError::Usage("--trials must be positive".to_owned()));
    }

    Ok(Some(BenchmarkArguments {
        replay: replay.into(),
        turns,
        trials,
        iterations,
        samples,
        seed,
        exploration,
        convention,
    }))
}

fn next_value(
    arguments: &mut impl Iterator<Item = String>,
    flag: &str,
) -> Result<String, CliError> {
    arguments
        .next()
        .ok_or_else(|| CliError::Usage(format!("missing value for {flag}")))
}

fn parse_value<T>(flag: &str, value: &str) -> Result<T, CliError>
where
    T: FromStr,
    T::Err: fmt::Display,
{
    value
        .parse()
        .map_err(|error| CliError::Usage(format!("invalid value for {flag}: {error}")))
}

fn print_usage() {
    println!("{}", usage());
}

fn print_usage_to_stderr() {
    eprintln!("{}", usage());
}

fn usage() -> &'static str {
    "Usage:\n  hanabi-engine analyze <replay.json> --turn <N> [options]\n  \
     hanabi-engine benchmark <replay.json> --turn <N> [--turn <N> ...] [options]\n\n\
     Turn N is the position after N completed game actions; turn 0 is the initial deal.\n\n\
     Analyze options:\n  --mode <ismcts|flat>   Search mode (default: ismcts)\n  \
     --iterations <N>       ISMCTS iterations (default: 1000)\n  \
     --samples <N>          Flat Monte Carlo samples/action (default: 100)\n  \
     --seed <N>             Reproducible random seed (default: 0)\n  \
     --exploration <X>      ISMCTS UCB coefficient (default: sqrt(2))\n  \
     --convention <none>    Convention framework (default: none)\n\n\
     Benchmark options:\n  --turn <N>             Position to benchmark; may be repeated\n  \
     --trials <N>           Consecutive seeds per mode (default: 5)\n  \
     --iterations <N>       ISMCTS iterations/trial (default: 1000)\n  \
     --samples <N>          Flat Monte Carlo samples/action/trial (default: 100)\n  \
     --seed <N>             Base seed; trial N uses seed + N (default: 0)\n  \
     --exploration <X>      ISMCTS UCB coefficient (default: sqrt(2))\n  \
     --convention <none>    Convention framework (default: none)\n\n\
     Benchmark writes a versioned JSON report to standard output."
}

#[derive(Debug)]
enum CliError {
    Usage(String),
    ReadReplay { path: PathBuf, source: io::Error },
    Replay(ReplayError),
    TerminalPosition(u32),
    InvalidCurrentPlayer,
    InformationSet(InformationSetError),
    Flat(FlatSearchError),
    Ismcts(IsmctsError),
    NoBestAction,
    SerializeReport(serde_json::Error),
}

impl fmt::Display for CliError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Usage(message) => formatter.write_str(message),
            Self::ReadReplay { path, source } => {
                write!(formatter, "could not read {}: {source}", path.display())
            }
            Self::Replay(error) => write!(formatter, "could not reconstruct replay: {error}"),
            Self::TerminalPosition(turn) => {
                write!(
                    formatter,
                    "turn {turn} is terminal and has no action to analyze"
                )
            }
            Self::InvalidCurrentPlayer => formatter.write_str("current player is invalid"),
            Self::InformationSet(error) => write!(formatter, "invalid information set: {error}"),
            Self::Flat(error) => write!(formatter, "flat Monte Carlo search failed: {error}"),
            Self::Ismcts(error) => write!(formatter, "ISMCTS failed: {error}"),
            Self::NoBestAction => formatter.write_str("search returned no best action"),
            Self::SerializeReport(error) => {
                write!(formatter, "could not serialize report: {error}")
            }
        }
    }
}

impl std::error::Error for CliError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::ReadReplay { source, .. } => Some(source),
            Self::Replay(error) => Some(error),
            Self::InformationSet(error) => Some(error),
            Self::Flat(error) => Some(error),
            Self::Ismcts(error) => Some(error),
            Self::SerializeReport(error) => Some(error),
            Self::Usage(_)
            | Self::TerminalPosition(_)
            | Self::InvalidCurrentPlayer
            | Self::NoBestAction => None,
        }
    }
}
