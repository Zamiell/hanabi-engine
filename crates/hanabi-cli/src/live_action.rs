use std::io::{self, BufRead, Read, Write};

use hanabi_core::PlayerView;
use hanabi_protocol::{HanabiLiveActionCommand, HanabiLiveSessionState, HanabiLiveSnapshot};
use hanabi_search::{SearchConfig, best_move};
use serde::Serialize;

use crate::{CliError, LiveActionArguments, SearchMode};

pub(super) fn run(arguments: &LiveActionArguments) -> Result<(), CliError> {
    let mut json = String::new();
    io::stdin()
        .read_to_string(&mut json)
        .map_err(CliError::ReadLiveSnapshot)?;
    let snapshot = HanabiLiveSnapshot::from_json(&json).map_err(CliError::LiveSnapshot)?;
    let view = snapshot.player_view().map_err(CliError::LiveSnapshot)?;
    let command = decide(snapshot.table_id(), view, arguments)?;
    let output = serde_json::to_string(&command).map_err(CliError::SerializeReport)?;
    println!("{output}");
    Ok(())
}

pub(super) fn run_session(arguments: &LiveActionArguments) -> Result<(), CliError> {
    let stdin = io::stdin();
    let stdout = io::stdout();
    let mut output = stdout.lock();
    let mut session = HanabiLiveSessionState::new();

    for line in stdin.lock().lines() {
        let line = line.map_err(CliError::ReadLiveSnapshot)?;
        if line.trim().is_empty() {
            continue;
        }
        let response = session
            .apply_json(&line)
            .map_err(CliError::LiveSnapshot)
            .and_then(|(table_id, view)| decide(table_id, view, arguments));
        match response {
            Ok(command) => {
                serde_json::to_writer(&mut output, &command).map_err(CliError::SerializeReport)?;
            }
            Err(error) => {
                serde_json::to_writer(
                    &mut output,
                    &LiveSessionErrorResponse {
                        error: error.to_string(),
                    },
                )
                .map_err(CliError::SerializeReport)?;
            }
        }
        writeln!(output).map_err(CliError::WriteLiveSession)?;
        output.flush().map_err(CliError::WriteLiveSession)?;
    }
    Ok(())
}

fn decide(
    table_id: u64,
    view: PlayerView,
    arguments: &LiveActionArguments,
) -> Result<HanabiLiveActionCommand, CliError> {
    let config = match arguments.mode {
        SearchMode::Ismcts => SearchConfig::Ismcts(hanabi_search::IsmctsConfig {
            iterations: arguments.iterations,
            exploration: arguments.exploration,
            seed: arguments.seed,
        }),
        SearchMode::Flat => SearchConfig::Flat(hanabi_search::MonteCarloConfig {
            samples_per_action: arguments.samples,
            seed: arguments.seed,
        }),
    };
    let best = best_move(view, arguments.convention, config).map_err(CliError::BestMove)?;
    Ok(HanabiLiveActionCommand::from_engine_action(
        table_id,
        best.action,
    ))
}

#[derive(Serialize)]
struct LiveSessionErrorResponse {
    error: String,
}
