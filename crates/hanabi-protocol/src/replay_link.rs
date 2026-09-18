//! Hanab Live replay URL encoding shared by the CLI and replay diagnostics.
//! Source: <https://github.com/Hanabi-Live/hanabi-live/blob/3a149d7c42e5c7ff79b61c949dccc5a419564b4a/packages/client/src/lobby/hypoCompress.ts>
//! Includes the fourth seed field implemented in the sibling Hanab Live codec.

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
    let payload = compress(replay)?;
    if !payload_matches_replay(&payload, replay) {
        return Err("replay-link encoding failed its round-trip verification".to_owned());
    }
    Ok(format!(
        "https://hanab.live/shared-replay-json/{payload}#{turn}"
    ))
}

/// Independently decode the fields represented by Hanab Live's codec. Names,
/// notes and other metadata are not encoded; compare game data, not JSON text.
/// Keep this check in production so the script fails before printing a bad URL.
fn payload_matches_replay(payload: &str, replay: &HanabiLiveReplay) -> bool {
    let fields = payload.split(',').collect::<Vec<_>>();
    let [deck, actions, variant, seed] = fields.as_slice() else {
        return false;
    };
    if *seed != replay.seed.as_deref().unwrap_or_default() || variant.replace('-', "") != "0" {
        return false;
    }
    let deck = deck.replace('-', "");
    let bytes = deck.as_bytes();
    if bytes.len() != replay.deck.len() + 3
        || bytes
            .first()
            .and_then(|byte| char::from(*byte).to_digit(10))
            != u32::try_from(replay.players.len()).ok()
        || bytes.get(1..3) != Some(b"15")
    {
        return false;
    }
    if !bytes[3..].iter().zip(&replay.deck).all(|(byte, card)| {
        BASE62
            .iter()
            .position(|digit| digit == byte)
            .is_some_and(|code| {
                code / 5 == usize::from(card.suit_index) && code % 5 + 1 == usize::from(card.rank)
            })
    }) {
        return false;
    }
    let actions = actions.replace('-', "");
    let bytes = actions.as_bytes();
    if bytes.len() != replay.actions.len() * 2 + 2 {
        return false;
    }
    let Some(min) = char::from(bytes[0]).to_digit(10) else {
        return false;
    };
    let Some(max) = char::from(bytes[1]).to_digit(10) else {
        return false;
    };
    if min > max || max > 3 {
        return false;
    }
    let range = usize::try_from(max - min + 1).expect("action range fits usize");
    bytes[2..]
        .chunks_exact(2)
        .zip(&replay.actions)
        .all(|(pair, action)| {
            let Some(code) = BASE62.iter().position(|digit| *digit == pair[0]) else {
                return false;
            };
            let target = BASE62.iter().position(|digit| *digit == pair[1]);
            code % range + usize::try_from(min).expect("action code fits usize")
                == usize::from(action.action_type.code())
                && (code / range).checked_sub(1) == Some(usize::from(action.value))
                && target == Some(action.target)
        })
}

fn digit(index: usize) -> Result<char, String> {
    BASE62
        .get(index)
        .copied()
        .map(char::from)
        .ok_or_else(|| format!("replay value {index} cannot fit Hanab Live's URL format"))
}

fn compress(replay: &HanabiLiveReplay) -> Result<String, String> {
    let seed = replay.seed.as_deref().unwrap_or_default();
    if !seed
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
    {
        return Err(
            "replay-link seed must contain only ASCII letters, digits, and hyphens".to_owned(),
        );
    }
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
    let wrapped = encoded
        .as_bytes()
        .chunks(20)
        .map(|chunk| std::str::from_utf8(chunk).expect("the codec emits ASCII"))
        .collect::<Vec<_>>()
        .join("-");
    // Seed hyphens are meaningful, unlike the wrapping in the first three fields.
    Ok(format!("{wrapped},{seed}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_detects_transcription_and_field_corruption() {
        let replay =
            HanabiLiveReplay::from_json(include_str!("../tests/fixtures/game-p4v0s1.json"))
                .unwrap();
        let payload = compress(&replay).unwrap();
        assert!(payload_matches_replay(&payload, &replay));
        // The reported missing-character mistake must fail even if the
        // remaining characters still belong to the codec alphabet.
        let mut missing_character = payload.clone();
        missing_character.remove(payload.find(',').unwrap() + 3);
        assert!(!payload_matches_replay(&missing_character, &replay));
        for index in [0, 3, payload.find(',').unwrap() + 3, payload.len() - 1] {
            let mut damaged = payload.clone();
            let replacement = if &damaged[index..=index] == "a" {
                "b"
            } else {
                "a"
            };
            damaged.replace_range(index..=index, replacement);
            assert!(!payload_matches_replay(&damaged, &replay));
        }
        assert!(!payload_matches_replay("", &replay));
    }

    #[test]
    fn round_trip_preserves_seed_hyphens_and_empty_action_lists() {
        let mut replay =
            HanabiLiveReplay::from_json(include_str!("../tests/fixtures/game-p4v0s1.json"))
                .unwrap();
        replay.seed = Some("custom-seed-1".to_owned());
        replay.actions.clear();
        assert!(payload_matches_replay(&compress(&replay).unwrap(), &replay));
        replay.seed = None;
        assert!(payload_matches_replay(&compress(&replay).unwrap(), &replay));
    }
}
