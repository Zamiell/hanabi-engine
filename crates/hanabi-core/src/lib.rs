//! Deterministic rules and player-safe observations for standard Hanabi.
//!
//! [`FullState`] is authoritative simulator truth. Code that chooses an action
//! should consume a [`PlayerView`] so hidden card identities cannot leak into a
//! policy or search algorithm.

pub mod action;
pub mod card;
pub mod ids;
pub mod state;
pub mod view;

pub use action::{Action, Clue};
pub use card::{Card, Rank, Suit, standard_deck};
pub use ids::{CardId, PlayerId};
pub use state::{
    DeterminizationError, DeterminizationTemplate, EndReason, FullState, GameEvent, GameStatus,
    HistoryEntry, InvariantViolation, MAX_CLUE_TOKENS, MAX_STRIKES, RuleError, SetupError,
    TurnResult,
};
pub use view::{
    ClueFacts, ObservedCard, ObservedEvent, ObservedHistoryEntry, PlayerView, PolicyObservation,
};
