use std::io::{self, Read};

use hanabi_protocol::{HanabiLiveActionCommand, HanabiLiveSnapshot};
use hanabi_search::{SearchConfig, best_move};

use crate::{CliError, LiveActionArguments, SearchMode};

pub(super) fn run(arguments: &LiveActionArguments) -> Result<(), CliError> {
    let mut json = String::new();
    io::stdin()
        .read_to_string(&mut json)
        .map_err(CliError::ReadLiveSnapshot)?;
    let snapshot = HanabiLiveSnapshot::from_json(&json).map_err(CliError::LiveSnapshot)?;
    let view = snapshot.player_view().map_err(CliError::LiveSnapshot)?;
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
    let command = HanabiLiveActionCommand::from_engine_action(snapshot.table_id(), best.action);
    let output = serde_json::to_string(&command).map_err(CliError::SerializeReport)?;
    println!("{output}");
    Ok(())
}
