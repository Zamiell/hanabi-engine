use core::fmt;

use hanabi_core::{
    Action, Card, CardId, Clue, FullState, GameStatus, PlayerId, Rank, RuleError, SetupError, Suit,
};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// The compact JSON replay exported by Hanabi Live's `/copy` command.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HanabiLiveReplay {
    pub players: Vec<String>,
    pub deck: Vec<HanabiLiveCard>,
    pub actions: Vec<HanabiLiveAction>,
    pub options: Option<HanabiLiveOptions>,
}

impl<'de> Deserialize<'de> for HanabiLiveReplay {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        struct WireReplay {
            players: Vec<String>,
            deck: Option<Vec<HanabiLiveCard>>,
            seed: Option<String>,
            actions: Vec<HanabiLiveAction>,
            options: Option<HanabiLiveOptions>,
        }
        let wire = WireReplay::deserialize(deserializer)?;
        // Explicit decks remain authoritative, including custom/edited deals
        // whose original seed is still present as descriptive metadata.
        let deck = if let Some(deck) = wire.deck {
            deck
        } else {
            if wire
                .options
                .as_ref()
                .is_some_and(|options| options.variant != "No Variant")
            {
                return Err(serde::de::Error::custom(
                    "seed generation only supports No Variant",
                ));
            }
            let seed = wire
                .seed
                .as_deref()
                .ok_or_else(|| serde::de::Error::custom("replay requires a deck or seed"))?;
            crate::seed::deck_from_seed(seed, wire.players.len())
                .map_err(serde::de::Error::custom)?
        };
        Ok(Self {
            players: wire.players,
            deck,
            actions: wire.actions,
            options: wire.options,
        })
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq)]
pub struct HanabiLiveCard {
    #[serde(rename = "suitIndex")]
    pub suit_index: u8,
    pub rank: u8,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
pub struct HanabiLiveAction {
    #[serde(rename = "type")]
    pub action_type: HanabiLiveActionType,
    pub target: usize,
    #[serde(default)]
    pub value: u8,
}

/// Numeric action discriminant used by Hanabi Live replay and action payloads.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HanabiLiveActionType {
    Play,
    Discard,
    SuitClue,
    RankClue,
    GameOver,
    /// Retained until semantic validation so callers receive a precise
    /// unsupported-action error rather than a generic JSON error.
    Unknown(u8),
}

impl HanabiLiveActionType {
    #[must_use]
    pub const fn from_code(code: u8) -> Self {
        match code {
            0 => Self::Play,
            1 => Self::Discard,
            2 => Self::SuitClue,
            3 => Self::RankClue,
            4 => Self::GameOver,
            other => Self::Unknown(other),
        }
    }

    #[must_use]
    pub const fn code(self) -> u8 {
        match self {
            Self::Play => 0,
            Self::Discard => 1,
            Self::SuitClue => 2,
            Self::RankClue => 3,
            Self::GameOver => 4,
            Self::Unknown(code) => code,
        }
    }
}

impl Serialize for HanabiLiveActionType {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_u8(self.code())
    }
}

impl<'de> Deserialize<'de> for HanabiLiveActionType {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        u8::deserialize(deserializer).map(Self::from_code)
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
pub struct HanabiLiveOptions {
    #[serde(default = "default_variant")]
    pub variant: String,
}

fn default_variant() -> String {
    "No Variant".to_owned()
}

impl HanabiLiveReplay {
    /// Parses a compact Hanabi Live replay.
    ///
    /// # Errors
    ///
    /// Returns [`ReplayError::Json`] if the JSON does not match the export
    /// schema.
    pub fn from_json(json: &str) -> Result<Self, ReplayError> {
        serde_json::from_str(json).map_err(ReplayError::Json)
    }

    /// Replays standard five-color actions through `hanabi-core`.
    ///
    /// Both current exports and legacy fixtures without an `options` object
    /// are accepted. A missing variant is interpreted as `No Variant`.
    ///
    /// # Errors
    ///
    /// Returns [`ReplayError`] for unsupported variants or values, malformed
    /// decks, illegal actions, or an external game-over action that occurs
    /// before the rules engine reaches a terminal state.
    pub fn replay(&self) -> Result<FullState, ReplayError> {
        let mut state = self.initial_state()?;

        for (turn, action) in self.actions.iter().enumerate() {
            if action.action_type == HanabiLiveActionType::GameOver {
                if state.status() == GameStatus::InProgress {
                    return Err(ReplayError::ExternalGameOver { turn });
                }
                break;
            }

            let engine_action = action_from_live(action)?;
            state
                .apply(engine_action)
                .map_err(|source| ReplayError::IllegalAction { turn, source })?;
        }
        Ok(state)
    }

    /// Replays exactly `turn` game actions and returns the position before the
    /// next action. Turn zero is the initial deal.
    ///
    /// Unlike [`Self::replay`], this intentionally does not validate actions
    /// after the requested prefix. This allows analysis of an otherwise
    /// incomplete or subsequently malformed replay.
    ///
    /// # Errors
    ///
    /// Returns [`ReplayError`] for invalid setup or prefix actions, or
    /// [`ReplayError::TurnOutOfRange`] if the replay ends before `turn`.
    pub fn state_at_turn(&self, turn: u32) -> Result<FullState, ReplayError> {
        let mut state = self.initial_state()?;
        if turn == 0 {
            return Ok(state);
        }

        for (action_index, action) in self.actions.iter().enumerate() {
            if action.action_type == HanabiLiveActionType::GameOver {
                if state.status() == GameStatus::InProgress {
                    return Err(ReplayError::ExternalGameOver { turn: action_index });
                }
                break;
            }

            let engine_action = action_from_live(action)?;
            state
                .apply(engine_action)
                .map_err(|source| ReplayError::IllegalAction {
                    turn: action_index,
                    source,
                })?;
            if state.turn() == turn {
                return Ok(state);
            }
        }

        Err(ReplayError::TurnOutOfRange {
            requested: turn,
            available: state.turn(),
        })
    }

    fn initial_state(&self) -> Result<FullState, ReplayError> {
        if let Some(options) = &self.options {
            if options.variant != "No Variant" {
                return Err(ReplayError::UnsupportedVariant(options.variant.clone()));
            }
        }

        let player_count = u8::try_from(self.players.len())
            .map_err(|_| ReplayError::InvalidPlayerCount(self.players.len()))?;
        let deck = self
            .deck
            .iter()
            .copied()
            .map(card_from_live)
            .collect::<Result<Vec<_>, _>>()?;
        FullState::new_standard(player_count, deck).map_err(ReplayError::Setup)
    }
}

fn card_from_live(card: HanabiLiveCard) -> Result<Card, ReplayError> {
    Ok(Card::new(
        suit_from_index(card.suit_index)?,
        rank_from_number(card.rank)?,
    ))
}

fn action_from_live(action: &HanabiLiveAction) -> Result<Action, ReplayError> {
    match action.action_type {
        HanabiLiveActionType::Play => Ok(Action::Play(CardId::new(action.target))),
        HanabiLiveActionType::Discard => Ok(Action::Discard(CardId::new(action.target))),
        HanabiLiveActionType::SuitClue => Ok(Action::Clue {
            target: player_from_target(action.target)?,
            clue: Clue::Suit(suit_from_index(action.value)?),
        }),
        HanabiLiveActionType::RankClue => Ok(Action::Clue {
            target: player_from_target(action.target)?,
            clue: Clue::Rank(rank_from_number(action.value)?),
        }),
        HanabiLiveActionType::GameOver => unreachable!("game-over actions are handled by replay"),
        HanabiLiveActionType::Unknown(other) => Err(ReplayError::UnknownActionType(other)),
    }
}

fn player_from_target(target: usize) -> Result<PlayerId, ReplayError> {
    u8::try_from(target)
        .map(PlayerId::new)
        .map_err(|_| ReplayError::InvalidPlayerTarget(target))
}

fn suit_from_index(index: u8) -> Result<Suit, ReplayError> {
    match index {
        0 => Ok(Suit::Red),
        1 => Ok(Suit::Yellow),
        2 => Ok(Suit::Green),
        3 => Ok(Suit::Blue),
        4 => Ok(Suit::Purple),
        _ => Err(ReplayError::InvalidSuit(index)),
    }
}

fn rank_from_number(rank: u8) -> Result<Rank, ReplayError> {
    match rank {
        1 => Ok(Rank::One),
        2 => Ok(Rank::Two),
        3 => Ok(Rank::Three),
        4 => Ok(Rank::Four),
        5 => Ok(Rank::Five),
        _ => Err(ReplayError::InvalidRank(rank)),
    }
}

#[derive(Debug)]
pub enum ReplayError {
    Json(serde_json::Error),
    UnsupportedVariant(String),
    InvalidPlayerCount(usize),
    InvalidPlayerTarget(usize),
    InvalidSuit(u8),
    InvalidRank(u8),
    UnknownActionType(u8),
    Setup(SetupError),
    IllegalAction { turn: usize, source: RuleError },
    ExternalGameOver { turn: usize },
    TurnOutOfRange { requested: u32, available: u32 },
}

impl fmt::Display for ReplayError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Json(error) => write!(formatter, "invalid Hanabi Live JSON: {error}"),
            Self::UnsupportedVariant(variant) => {
                write!(formatter, "unsupported Hanabi Live variant: {variant}")
            }
            Self::InvalidPlayerCount(count) => write!(formatter, "invalid player count: {count}"),
            Self::InvalidPlayerTarget(target) => {
                write!(formatter, "invalid player target: {target}")
            }
            Self::InvalidSuit(suit) => write!(formatter, "invalid standard suit index: {suit}"),
            Self::InvalidRank(rank) => write!(formatter, "invalid standard rank: {rank}"),
            Self::UnknownActionType(action) => {
                write!(formatter, "unknown Hanabi Live action type: {action}")
            }
            Self::Setup(error) => write!(formatter, "could not set up replay: {error}"),
            Self::IllegalAction { turn, source } => {
                write!(formatter, "illegal replay action at turn {turn}: {source}")
            }
            Self::ExternalGameOver { turn } => write!(
                formatter,
                "external game-over action at turn {turn} is not a standard rules ending"
            ),
            Self::TurnOutOfRange {
                requested,
                available,
            } => write!(
                formatter,
                "requested turn {requested}, but the replay contains only {available} game turns"
            ),
        }
    }
}

impl std::error::Error for ReplayError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Json(error) => Some(error),
            Self::Setup(error) => Some(error),
            Self::IllegalAction { source, .. } => Some(source),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn action_types_round_trip_as_numeric_wire_values() {
        assert_eq!(
            serde_json::to_string(&HanabiLiveActionType::SuitClue).unwrap(),
            "2"
        );
        assert_eq!(
            serde_json::from_str::<HanabiLiveActionType>("9").unwrap(),
            HanabiLiveActionType::Unknown(9)
        );
        let action = HanabiLiveAction {
            action_type: HanabiLiveActionType::Unknown(9),
            target: 0,
            value: 0,
        };
        assert!(matches!(
            action_from_live(&action),
            Err(ReplayError::UnknownActionType(9))
        ));
    }
}
