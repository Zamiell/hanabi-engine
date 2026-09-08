//! Rules shared by authoritative and unknown-card public transitions.
//! These functions neither inspect card identities nor perform convention inference.

use crate::{EndReason, GameStatus, MAX_CLUE_TOKENS, MAX_STRIKES, PlayerId};

#[must_use]
pub fn refunded_clues(tokens: u8) -> u8 {
    tokens.saturating_add(1).min(MAX_CLUE_TOKENS)
}

#[must_use]
pub fn status_after_play(score: usize, strikes: u8) -> GameStatus {
    if strikes >= MAX_STRIKES {
        GameStatus::Finished(EndReason::TooManyStrikes)
    } else if score == 25 {
        GameStatus::Finished(EndReason::PerfectScore)
    } else {
        GameStatus::InProgress
    }
}

/// Advance the clock only after card effects and a possible draw. The counter
/// at the *start* of the turn is used so drawing the last card does not consume
/// one of the newly started final-round turns.
///
/// # Panics
/// Panics if `players` is zero; callers supply a validated game's player count.
#[must_use]
pub fn finish_public_turn(
    actor: PlayerId,
    players: u8,
    mut status: GameStatus,
    previous_remaining: Option<u8>,
    mut remaining: Option<u8>,
) -> (PlayerId, GameStatus, Option<u8>) {
    if status == GameStatus::InProgress {
        if let Some(previous) = previous_remaining {
            let next = previous.saturating_sub(1);
            remaining = Some(next);
            if next == 0 {
                status = GameStatus::Finished(EndReason::FinalRoundComplete);
            }
        }
    }
    let next = if status == GameStatus::InProgress {
        PlayerId::new(
            u8::try_from((actor.index() + 1) % usize::from(players))
                .expect("the remainder fits in the player-count type"),
        )
    } else {
        actor
    };
    (next, status, remaining)
}
