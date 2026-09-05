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
    AnalyzePositionError, HGroupProfile, PlannerConfig, PlanningObjective, SupportedConvention,
    WorldCount, analyze_position,
};

mod live_action;
mod replay_link;

const DEFAULT_EXACT_WORLD_LIMIT: u64 = 4_096;
const DEFAULT_EXACT_NODE_LIMIT: u64 = 50_000;

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
        Command::LiveAction(arguments) => live_action::run(&arguments),
        Command::LiveSession(arguments) => live_action::run_session(&arguments),
        Command::ReplayLink(arguments) => replay_link::run(&arguments),
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
    print_position(&replay, &state, &view);
    println!("Convention: {}", arguments.convention);
    println!("Objective: {}", arguments.objective);
    if let Some(revision) = arguments.convention.ruleset_revision() {
        println!("Convention ruleset revision: {revision}");
    }
    analyze_planner(arguments, &view, &replay.players)
}

fn analyze_planner(
    arguments: &AnalyzeArguments,
    view: &PlayerView,
    players: &[String],
) -> Result<(), CliError> {
    let started = Instant::now();
    let analysis = analyze_position(
        view,
        arguments.convention,
        PlannerConfig {
            objective: arguments.objective,
            exact_world_limit: arguments.exact_world_limit,
            exact_node_limit: arguments.exact_node_limit,
        },
    )
    .map_err(CliError::AnalyzePosition)?;
    let result = analysis.planner;
    let (world_prefix, worlds) = match result.world_count {
        WorldCount::Exact(worlds) => ("", worlds),
        WorldCount::LowerBound(worlds) => (">=", worlds),
    };
    println!(
        "Planning: deterministic {:?} planner, {}{} consistent worlds",
        result.phase, world_prefix, worlds,
    );
    println!(
        "Elapsed: {:.3}s; exact nodes: {}",
        started.elapsed().as_secs_f64(),
        result.exact_nodes
    );
    println!();
    for evaluation in result.root_actions {
        let marker = if evaluation.action == result.best_action {
            '*'
        } else {
            ' '
        };
        if let Some(exact) = evaluation.exact {
            println!(
                "{marker}  {:<42} perfect {:>6.2}%  score {:>6.3}  strikeout {:>6.2}%",
                action_label(view, players, evaluation.action),
                exact.perfect_rate() * 100.0,
                exact.expected_score(),
                exact.strikeout_rate() * 100.0,
            );
        } else {
            println!(
                "{marker}  {:<42} priority {:>6.2}  playable {}  critical {}  new {}  line {}/+{}",
                action_label(view, players, evaluation.action),
                evaluation.convention_priority,
                evaluation.immediately_playable_touched,
                evaluation.critical_touched,
                evaluation.newly_touched,
                evaluation.symbolic_line.actions,
                evaluation.symbolic_line.score_gain,
            );
        }
    }
    Ok(())
}

fn read_replay(path: &Path) -> Result<HanabiLiveReplay, CliError> {
    let json = fs::read_to_string(path).map_err(|source| CliError::ReadReplay {
        path: path.to_path_buf(),
        source,
    })?;
    HanabiLiveReplay::from_json(&json).map_err(CliError::Replay)
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

#[derive(Clone, Copy, Default)]
enum ConventionChoice {
    #[default]
    None,
    HGroup,
}

impl FromStr for ConventionChoice {
    type Err = CliError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "none" => Ok(Self::None),
            "h-group" => Ok(Self::HGroup),
            _ => Err(CliError::Usage(format!(
                "unknown convention {value:?}; expected none or h-group"
            ))),
        }
    }
}

fn select_convention(
    choice: ConventionChoice,
    h_group_profile: Option<HGroupProfile>,
) -> Result<SupportedConvention, CliError> {
    match (choice, h_group_profile) {
        (ConventionChoice::None, None) => Ok(SupportedConvention::None),
        (ConventionChoice::None, Some(_)) => Err(CliError::Usage(
            "--h-group-level requires --convention h-group".to_owned(),
        )),
        (ConventionChoice::HGroup, Some(profile)) => Ok(SupportedConvention::HGroup(profile)),
        (ConventionChoice::HGroup, None) => Err(CliError::Usage(
            "--h-group-level is required when --convention h-group".to_owned(),
        )),
    }
}

struct AnalyzeArguments {
    replay: PathBuf,
    turn: u32,
    exact_world_limit: u64,
    exact_node_limit: u64,
    convention: SupportedConvention,
    objective: PlanningObjective,
}

struct LiveActionArguments {
    exact_world_limit: u64,
    exact_node_limit: u64,
    convention: SupportedConvention,
    objective: PlanningObjective,
    include_planning_details: bool,
}

enum Command {
    ReplayLink(replay_link::Arguments),
    Analyze(AnalyzeArguments),
    LiveAction(LiveActionArguments),
    LiveSession(LiveActionArguments),
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
        "replay-link" => {
            replay_link::parse(&mut arguments).map(|value| value.map(Command::ReplayLink))
        }
        "analyze" => {
            parse_analyze_arguments(&mut arguments).map(|value| value.map(Command::Analyze))
        }
        "live-action" => {
            parse_live_action_arguments(&mut arguments).map(|value| value.map(Command::LiveAction))
        }
        "live-session" => {
            parse_live_action_arguments(&mut arguments).map(|value| value.map(Command::LiveSession))
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
    let mut exact_world_limit = DEFAULT_EXACT_WORLD_LIMIT;
    let mut exact_node_limit = DEFAULT_EXACT_NODE_LIMIT;
    let mut convention = Some(ConventionChoice::default());
    let mut h_group_profile = None;
    let mut objective = PlanningObjective::ExpectedScore;

    while let Some(flag) = arguments.next() {
        if parse_planning_option(
            &flag,
            arguments,
            &mut exact_world_limit,
            &mut exact_node_limit,
            &mut convention,
            &mut h_group_profile,
            &mut objective,
        )? {
            continue;
        }
        match flag.as_str() {
            "--turn" => {
                turn = Some(parse_value("--turn", &next_value(arguments, "--turn")?)?);
            }
            "--help" | "-h" => return Ok(None),
            _ => return Err(CliError::Usage(format!("unknown option {flag:?}"))),
        }
    }

    let convention = select_convention(
        convention.expect("analyze convention has a default"),
        h_group_profile,
    )?;
    Ok(Some(AnalyzeArguments {
        replay: replay.into(),
        turn: turn.ok_or_else(|| CliError::Usage("missing required --turn".to_owned()))?,
        exact_world_limit,
        exact_node_limit,
        convention,
        objective,
    }))
}

fn parse_live_action_arguments(
    arguments: &mut impl Iterator<Item = String>,
) -> Result<Option<LiveActionArguments>, CliError> {
    let mut exact_world_limit = DEFAULT_EXACT_WORLD_LIMIT;
    let mut exact_node_limit = DEFAULT_EXACT_NODE_LIMIT;
    let mut convention = Some(ConventionChoice::HGroup);
    let mut h_group_profile = None;
    let mut include_planning_details = false;
    let mut objective = PlanningObjective::PerfectScore;

    while let Some(flag) = arguments.next() {
        if parse_planning_option(
            &flag,
            arguments,
            &mut exact_world_limit,
            &mut exact_node_limit,
            &mut convention,
            &mut h_group_profile,
            &mut objective,
        )? {
            continue;
        }
        match flag.as_str() {
            "--include-planning-details" => include_planning_details = true,
            "--help" | "-h" => return Ok(None),
            _ => return Err(CliError::Usage(format!("unknown option {flag:?}"))),
        }
    }

    let convention = match (
        convention.expect("live convention has a default"),
        h_group_profile,
    ) {
        (ConventionChoice::None, None) => SupportedConvention::None,
        (ConventionChoice::None, Some(_)) => {
            return Err(CliError::Usage(
                "--h-group-level requires --convention h-group".to_owned(),
            ));
        }
        (ConventionChoice::HGroup, profile) => {
            SupportedConvention::HGroup(profile.unwrap_or(HGroupProfile::Max))
        }
    };
    Ok(Some(LiveActionArguments {
        exact_world_limit,
        exact_node_limit,
        convention,
        objective,
        include_planning_details,
    }))
}

#[allow(clippy::too_many_arguments)]
fn parse_planning_option(
    flag: &str,
    arguments: &mut impl Iterator<Item = String>,
    exact_world_limit: &mut u64,
    exact_node_limit: &mut u64,
    convention: &mut Option<ConventionChoice>,
    h_group_profile: &mut Option<HGroupProfile>,
    objective: &mut PlanningObjective,
) -> Result<bool, CliError> {
    match flag {
        "--exact-world-limit" => {
            *exact_world_limit = parse_value(flag, &next_value(arguments, flag)?)?;
        }
        "--exact-node-limit" => {
            *exact_node_limit = parse_value(flag, &next_value(arguments, flag)?)?;
        }
        "--convention" => {
            *convention = Some(parse_value(flag, &next_value(arguments, flag)?)?);
        }
        "--h-group-level" => {
            *h_group_profile = Some(parse_value(flag, &next_value(arguments, flag)?)?);
        }
        "--objective" => {
            *objective = parse_value(flag, &next_value(arguments, flag)?)?;
        }
        _ => return Ok(false),
    }
    Ok(true)
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
     hanabi-engine replay-link <replay.json> [--turn <N>]\n  \
     hanabi-engine live-action [options] < live-snapshot.json\n\n\
     hanabi-engine live-session [options] < session-requests.ndjson\n\n\
     Turn N is the position after N completed game actions; turn 0 is the initial deal.\n\n\
     Exception: replay-link uses Hanab Live turns (1 = initial deal; default: 1).\n\n\
     Analyze options:\n  --exact-world-limit <N>  Worlds allowed in exact endgame (default: 4096)\n  \
     --exact-node-limit <N>   Nodes allowed in exact endgame (default: 50000)\n  \
     --objective <expected-score|perfect-score>  Exact-planning objective (default: expected-score)\n  \
     --convention <none|h-group>  Convention framework (default: none)\n  \
     --h-group-level <1-25|max>   Required H-Group cumulative profile\n\n\
     Live-action options:\n  --exact-world-limit <N>  Worlds allowed in exact endgame (default: 4096)\n  \
     --exact-node-limit <N>   Nodes allowed in exact endgame (default: 50000)\n  \
     --objective <expected-score|perfect-score>  Exact-planning objective (default: perfect-score)\n  \
     --convention <none|h-group>  Convention framework (default: h-group)\n  \
     --h-group-level <1-25|max>   H-Group profile (default: max)\n\n\
     --include-planning-details     Emit an action envelope with diagnostic evidence\n\n\
     Live-session accepts one initialize request followed by append requests as NDJSON.\n\
     It emits one action or error JSON object per input line."
}

#[derive(Debug)]
enum CliError {
    Usage(String),
    ReadReplay { path: PathBuf, source: io::Error },
    Replay(ReplayError),
    TerminalPosition(u32),
    InvalidCurrentPlayer,
    ReadLiveSnapshot(io::Error),
    WriteLiveSession(io::Error),
    LiveSnapshot(hanabi_protocol::LiveSnapshotError),
    AnalyzePosition(AnalyzePositionError),
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
            Self::ReadLiveSnapshot(error) => {
                write!(
                    formatter,
                    "could not read live snapshot from standard input: {error}"
                )
            }
            Self::WriteLiveSession(error) => {
                write!(formatter, "could not write live session response: {error}")
            }
            Self::LiveSnapshot(error) => write!(formatter, "invalid live snapshot: {error}"),
            Self::AnalyzePosition(error) => write!(formatter, "analysis failed: {error}"),
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
            Self::ReadLiveSnapshot(error) | Self::WriteLiveSession(error) => Some(error),
            Self::LiveSnapshot(error) => Some(error),
            Self::AnalyzePosition(error) => Some(error),
            Self::SerializeReport(error) => Some(error),
            Self::Usage(_) | Self::TerminalPosition(_) | Self::InvalidCurrentPlayer => None,
        }
    }
}
