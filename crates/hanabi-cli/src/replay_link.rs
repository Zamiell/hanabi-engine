//! Hanab Live's URL codec, not generic JSON/base64 compression.
//! Source: <https://github.com/Hanabi-Live/hanabi-live/blob/3a149d7c42e5c7ff79b61c949dccc5a419564b4a/packages/client/src/lobby/hypoCompress.ts>
//! This engine supports standard five-suit games (variant ID 0) only.

use std::path::PathBuf;

use hanabi_protocol::HanabiLiveReplay;

use crate::{CliError, next_value, parse_value, read_replay};

const BASE62: &[u8] = b"abcdefghijklmnopqrstuvwxyz0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZ";

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
    // Validate the entire supplied game, including card ownership and clues.
    // Seed-only fixtures are expanded by the existing protocol parser.
    replay.replay().map_err(CliError::Replay)?;
    if arguments.turn == 0 || arguments.turn > replay.actions.len() + 1 {
        return Err(CliError::Usage(format!(
            "replay-link --turn must be between 1 and {} (Hanab Live numbering)",
            replay.actions.len() + 1
        )));
    }
    println!(
        "https://hanab.live/shared-replay-json/{}#{}",
        compress(&replay)?,
        arguments.turn
    );
    Ok(())
}

fn digit(index: usize) -> Result<char, CliError> {
    BASE62.get(index).copied().map(char::from).ok_or_else(|| {
        CliError::Usage(format!(
            "replay value {index} cannot fit Hanab Live's URL format"
        ))
    })
}

fn compress(replay: &HanabiLiveReplay) -> Result<String, CliError> {
    // Validation above guarantees a full standard deck, hence ranks 1..5.
    let mut encoded = format!("{}15", replay.players.len());
    for card in &replay.deck {
        encoded.push(digit(
            usize::from(card.suit_index) * 5 + usize::from(card.rank - 1),
        )?);
    }
    encoded.push(',');
    let min = replay
        .actions
        .iter()
        .map(|action| action.action_type.code())
        .min()
        .unwrap_or(0);
    let max = replay
        .actions
        .iter()
        .map(|action| action.action_type.code())
        .max()
        .unwrap_or(0);
    encoded.push(char::from(b'0' + min));
    encoded.push(char::from(b'0' + max));
    let range = usize::from(max - min + 1);
    for action in &replay.actions {
        if action.action_type.code() > 3 {
            return Err(CliError::Usage(
                "replay-link accepts game actions only; remove game-over/control markers"
                    .to_owned(),
            ));
        }
        // Protocol parsing normalizes an omitted play/discard value to zero.
        // Both representations have identical game semantics in Hanab Live.
        encoded.push(digit(
            (usize::from(action.value) + 1) * range + usize::from(action.action_type.code() - min),
        )?);
        encoded.push(digit(action.target)?);
    }
    encoded.push_str(",0");
    // The upstream codec inserts hyphens every 20 characters for wrapping.
    Ok(encoded
        .as_bytes()
        .chunks(20)
        .map(|chunk| std::str::from_utf8(chunk).expect("the codec emits ASCII"))
        .collect::<Vec<_>>()
        .join("-"))
}
