use core::fmt;

use hanabi_core::{
    Action, Card, CardId, Clue, FullState, GameStatus, PlayerId, Rank, RuleError, SetupError, Suit,
};
use serde::Deserialize;

const ACTION_PLAY: u8 = 0;
const ACTION_DISCARD: u8 = 1;
const ACTION_COLOR_CLUE: u8 = 2;
const ACTION_RANK_CLUE: u8 = 3;
const ACTION_GAME_OVER: u8 = 4;

/// The compact JSON replay exported by Hanabi Live's `/copy` command.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
pub struct HanabiLiveReplay {
    pub players: Vec<String>,
    pub deck: Vec<HanabiLiveCard>,
    pub actions: Vec<HanabiLiveAction>,
    #[serde(default)]
    pub options: Option<HanabiLiveOptions>,
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
    pub action_type: u8,
    pub target: usize,
    #[serde(default)]
    pub value: u8,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
pub struct HanabiLiveOptions {
    pub variant: String,
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
            if action.action_type == ACTION_GAME_OVER {
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
            if action.action_type == ACTION_GAME_OVER {
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
        ACTION_PLAY => Ok(Action::Play(CardId::new(action.target))),
        ACTION_DISCARD => Ok(Action::Discard(CardId::new(action.target))),
        ACTION_COLOR_CLUE => Ok(Action::Clue {
            target: player_from_target(action.target)?,
            clue: Clue::Suit(suit_from_index(action.value)?),
        }),
        ACTION_RANK_CLUE => Ok(Action::Clue {
            target: player_from_target(action.target)?,
            clue: Clue::Rank(rank_from_number(action.value)?),
        }),
        other => Err(ReplayError::UnknownActionType(other)),
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
