//! Hanab Live's URL codec, not generic JSON/base64 compression.
//! Source: <https://github.com/Hanabi-Live/hanabi-live/blob/3a149d7c42e5c7ff79b61c949dccc5a419564b4a/packages/client/src/lobby/hypoCompress.ts>
//! This engine supports standard five-suit games (variant ID 0) only.

use std::path::PathBuf;

use hanabi_protocol::replay_link;

use crate::{CliError, next_value, parse_value, read_replay};

pub(super) struct Arguments {
    replay: PathBuf,
    turn: usize,
}

pub(super) fn parse(
    arguments: &mut impl Iterator<Item = String>,
) -> Result<Option<Arguments>, CliError> {
    let path = next_value(arguments, "replay-link replay JSON path")?;
    if path == "--help" || path == "-h" {
        return Ok(None);
    }
    let mut turn = 1;
    while let Some(flag) = arguments.next() {
        match flag.as_str() {
            "--turn" => turn = parse_value(&flag, &next_value(arguments, &flag)?)?,
            "--help" | "-h" => return Ok(None),
            _ => return Err(CliError::Usage(format!("unknown option {flag:?}"))),
        }
    }
    Ok(Some(Arguments {
        replay: path.into(),
        turn,
    }))
}

pub(super) fn run(arguments: &Arguments) -> Result<(), CliError> {
    let replay = read_replay(&arguments.replay)?;
    println!(
        "{}",
        replay_link(&replay, arguments.turn).map_err(CliError::Usage)?
    );
    Ok(())
}
