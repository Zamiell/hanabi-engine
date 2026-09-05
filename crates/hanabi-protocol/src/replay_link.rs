//! Hanab Live replay URL encoding shared by the CLI and replay diagnostics.
//! Source: <https://github.com/Hanabi-Live/hanabi-live/blob/3a149d7c42e5c7ff79b61c949dccc5a419564b4a/packages/client/src/lobby/hypoCompress.ts>

use crate::HanabiLiveReplay;

const BASE62: &[u8] = b"abcdefghijklmnopqrstuvwxyz0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZ";

/// Generates a validated standard-game replay URL at a one-based Hanab Live turn.
///
/// # Errors
/// Returns an explanation if the replay is invalid, the turn is out of range,
/// or an action cannot be represented by Hanab Live's URL codec.
pub fn replay_link(replay: &HanabiLiveReplay, turn: usize) -> Result<String, String> {
    replay.replay().map_err(|error| error.to_string())?;
    if turn == 0 || turn > replay.actions.len() + 1 {
        return Err(format!(
            "replay-link --turn must be between 1 and {} (Hanab Live numbering)",
            replay.actions.len() + 1
        ));
    }
    Ok(format!(
        "https://hanab.live/shared-replay-json/{}#{turn}",
        compress(replay)?
    ))
}

fn digit(index: usize) -> Result<char, String> {
    BASE62
        .get(index)
        .copied()
        .map(char::from)
        .ok_or_else(|| format!("replay value {index} cannot fit Hanab Live's URL format"))
}

fn compress(replay: &HanabiLiveReplay) -> Result<String, String> {
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
            return Err(
                "replay-link accepts game actions only; remove game-over/control markers"
                    .to_owned(),
            );
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
