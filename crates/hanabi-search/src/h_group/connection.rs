use core::ops::Deref;

use hanabi_core::{Card, CardId, PlayerId};

use super::HGroupConnectionKind;

/// One active, typed step in a Prompt or Finesse chain.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct ConnectionObligation {
    pub(super) actor: PlayerId,
    pub(super) cards: Vec<CardId>,
    pub(super) expected: Card,
    /// Identity ultimately promised for `focus`.
    pub(super) focus_identity: Card,
    pub(super) kind: HGroupConnectionKind,
    pub(super) focus: CardId,
    /// Zero-based position in a multi-connection chain.
    pub(super) step: u8,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum ConnectionStatus {
    Pending,
    AwaitingFix,
    Satisfied,
    Invalidated,
    Cancelled,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum ConnectionTransitionReason {
    Scheduled,
    PlayedExpectedCard,
    PlayedAlternative,
    Misplayed,
    Fixed,
    DisplacedByClue,
    Superseded,
    FocusInvalidated,
    IdentitySatisfiedElsewhere,
}

/// Auditable lifecycle event for a connection. These records explain current
/// state; they are not themselves queried as current convention facts.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct ConnectionTransition {
    pub(super) turn: u32,
    pub(super) focus: CardId,
    pub(super) actor: PlayerId,
    pub(super) expected: Card,
    pub(super) from: ConnectionStatus,
    pub(super) to: ConnectionStatus,
    pub(super) reason: ConnectionTransitionReason,
}

#[derive(Clone, Debug, Default)]
pub(super) struct ConnectionAdvance {
    pub(super) failed_focuses: Vec<CardId>,
    pub(super) released_candidates: Vec<CardId>,
}

/// Canonical owner of every live Prompt/Finesse obligation.
///
/// Callers can borrow the active slice for recognition, but all mutation goes
/// through lifecycle methods so removal and advancement are recorded once.
#[derive(Clone, Debug, Default)]
pub(super) struct ConnectionManager {
    active: Vec<ConnectionObligation>,
    transitions: Vec<ConnectionTransition>,
}

impl Deref for ConnectionManager {
    type Target = [ConnectionObligation];

    fn deref(&self) -> &Self::Target {
        &self.active
    }
}

impl ConnectionManager {
    #[cfg(test)]
    pub(super) fn transitions(&self) -> &[ConnectionTransition] {
        &self.transitions
    }

    pub(super) fn validate(&self) -> Result<(), String> {
        if self
            .active
            .iter()
            .any(|connection| connection.cards.is_empty())
        {
            return Err("active connection has no candidates".to_owned());
        }
        for (index, connection) in self.active.iter().enumerate() {
            if self.active[..index].iter().any(|other| {
                other.actor == connection.actor
                    && other.focus == connection.focus
                    && other.step == connection.step
            }) {
                return Err(format!(
                    "duplicate active connection step: {other:?} and {connection:?}",
                    other = self.active[..index]
                        .iter()
                        .find(|other| {
                            other.actor == connection.actor
                                && other.focus == connection.focus
                                && other.step == connection.step
                        })
                        .expect("duplicate was just detected")
                ));
            }
            if !self.transitions.iter().any(|transition| {
                transition.focus == connection.focus
                    && transition.actor == connection.actor
                    && transition.reason == ConnectionTransitionReason::Scheduled
            }) {
                return Err("active connection has no scheduled transition".to_owned());
            }
        }
        Ok(())
    }

    pub(super) fn start(&mut self, turn: u32, obligation: ConnectionObligation) {
        if self.active.contains(&obligation) {
            return;
        }
        // One actor can have only one live interpretation for a particular
        // step of a focus connection. More-specific rules run after the
        // general connection scheduler, so a later interpretation replaces
        // the earlier one atomically instead of leaving contradictory duties
        // for action selection to reconcile.
        self.cancel_where(turn, ConnectionTransitionReason::Superseded, |active| {
            active.actor == obligation.actor
                && active.focus == obligation.focus
                && active.step == obligation.step
        });
        self.record(
            turn,
            &obligation,
            ConnectionStatus::Pending,
            ConnectionStatus::Pending,
            ConnectionTransitionReason::Scheduled,
        );
        self.active.push(obligation);
    }

    pub(super) fn cancel_where(
        &mut self,
        turn: u32,
        reason: ConnectionTransitionReason,
        mut predicate: impl FnMut(&ConnectionObligation) -> bool,
    ) {
        let mut retained = Vec::with_capacity(self.active.len());
        for obligation in core::mem::take(&mut self.active) {
            if predicate(&obligation) {
                let status = match reason {
                    ConnectionTransitionReason::FocusInvalidated => ConnectionStatus::Invalidated,
                    ConnectionTransitionReason::Fixed => ConnectionStatus::AwaitingFix,
                    _ => ConnectionStatus::Cancelled,
                };
                self.record(turn, &obligation, ConnectionStatus::Pending, status, reason);
            } else {
                retained.push(obligation);
            }
        }
        self.active = retained;
    }

    /// Applies a public Fix to every obligation owned by `actor`. Candidate
    /// mutation and lifecycle recording remain atomic inside the manager.
    pub(super) fn repair_actor(
        &mut self,
        turn: u32,
        actor: PlayerId,
        mut should_remove: impl FnMut(CardId) -> bool,
        mut replacement: impl FnMut(&ConnectionObligation) -> Option<CardId>,
    ) {
        let mut repaired = Vec::new();
        for connection in &mut self.active {
            if connection.actor != actor {
                continue;
            }
            let before = connection.cards.len();
            connection.cards.retain(|card| !should_remove(*card));
            if connection.cards.is_empty() {
                if let Some(next) = replacement(connection) {
                    connection.cards.push(next);
                }
            }
            if connection.cards.len() != before && !connection.cards.is_empty() {
                repaired.push(connection.clone());
            }
        }
        for connection in repaired {
            self.record(
                turn,
                &connection,
                ConnectionStatus::Pending,
                ConnectionStatus::Pending,
                ConnectionTransitionReason::Fixed,
            );
        }
        self.cancel_where(turn, ConnectionTransitionReason::Fixed, |connection| {
            connection.cards.is_empty()
        });
    }

    /// Removes a discarded candidate even when its obligation is blocked by
    /// an earlier step, then invalidates every exhausted or discarded focus.
    pub(super) fn discard(&mut self, turn: u32, player: PlayerId, card: CardId) {
        for connection in &mut self.active {
            if connection.actor != player {
                continue;
            }
            if connection.cards.first() == Some(&card) {
                connection.cards.clear();
            } else {
                connection.cards.retain(|candidate| *candidate != card);
            }
        }
        self.cancel_where(
            turn,
            ConnectionTransitionReason::FocusInvalidated,
            |connection| connection.cards.is_empty() || connection.focus == card,
        );
    }

    pub(super) fn advance_play(
        &mut self,
        turn: u32,
        player: PlayerId,
        card: CardId,
        identity: Card,
        successful: bool,
    ) -> ConnectionAdvance {
        let mut result = ConnectionAdvance::default();
        let active = self
            .active
            .iter()
            .enumerate()
            .filter(|(_, item)| {
                item.actor == player
                    && !self.active.iter().any(|other| {
                        other.focus == item.focus
                            && other.step < item.step
                            && !other.cards.is_empty()
                    })
            })
            .map(|(index, _)| index)
            .collect::<Vec<_>>();
        let mut transitions = Vec::new();
        for index in active {
            let connection = &mut self.active[index];
            if connection.cards.first() != Some(&card) {
                connection.cards.retain(|candidate| *candidate != card);
                continue;
            }
            connection.cards.remove(0);
            let obligation = connection.clone();
            if identity == connection.expected || !successful {
                result
                    .released_candidates
                    .extend(connection.cards.iter().copied());
                connection.cards.clear();
                transitions.push((
                    obligation,
                    if successful {
                        ConnectionStatus::Satisfied
                    } else {
                        ConnectionStatus::Invalidated
                    },
                    if successful {
                        ConnectionTransitionReason::PlayedExpectedCard
                    } else {
                        ConnectionTransitionReason::Misplayed
                    },
                ));
            } else if connection.cards.is_empty() {
                result.failed_focuses.push(connection.focus);
                transitions.push((
                    obligation,
                    ConnectionStatus::Invalidated,
                    ConnectionTransitionReason::PlayedAlternative,
                ));
            }
        }
        for (obligation, status, reason) in transitions {
            self.record(turn, &obligation, ConnectionStatus::Pending, status, reason);
        }
        // A card can disappear while a later step for the same focus is still
        // blocked by an earlier step. It can no longer be a candidate when
        // that later step activates, so purge it from every obligation rather
        // than only the obligations that were active on this turn.
        for connection in &mut self.active {
            connection.cards.retain(|candidate| *candidate != card);
        }
        let failed = &result.failed_focuses;
        self.active.retain(|connection| {
            !connection.cards.is_empty()
                && connection.focus != card
                && !failed.contains(&connection.focus)
        });
        result
    }

    fn record(
        &mut self,
        turn: u32,
        obligation: &ConnectionObligation,
        from: ConnectionStatus,
        to: ConnectionStatus,
        reason: ConnectionTransitionReason,
    ) {
        self.transitions.push(ConnectionTransition {
            turn,
            focus: obligation.focus,
            actor: obligation.actor,
            expected: obligation.expected,
            from,
            to,
            reason,
        });
    }
}

#[cfg(test)]
mod tests {
    use hanabi_core::{Rank, Suit};

    use super::*;

    fn obligation(cards: Vec<CardId>) -> ConnectionObligation {
        ConnectionObligation {
            actor: PlayerId::new(1),
            cards,
            expected: Card::new(Suit::Red, Rank::Two),
            focus_identity: Card::new(Suit::Red, Rank::Three),
            kind: HGroupConnectionKind::Prompt,
            focus: CardId::new(9),
            step: 0,
        }
    }

    #[test]
    fn successful_connection_releases_alternatives_and_records_completion() {
        let mut manager = ConnectionManager::default();
        manager.start(3, obligation(vec![CardId::new(5), CardId::new(7)]));
        let result = manager.advance_play(
            4,
            PlayerId::new(1),
            CardId::new(5),
            Card::new(Suit::Red, Rank::Two),
            true,
        );
        assert_eq!(result.released_candidates, [CardId::new(7)]);
        assert!(manager.is_empty());
        assert_eq!(
            manager.transitions().last().map(|transition| transition.to),
            Some(ConnectionStatus::Satisfied)
        );
    }

    #[test]
    fn a_new_interpretation_supersedes_the_same_connection_step() {
        let mut manager = ConnectionManager::default();
        let original = obligation(vec![CardId::new(5), CardId::new(7)]);
        manager.start(3, original.clone());
        let replacement = ConnectionObligation {
            cards: vec![CardId::new(8)],
            expected: Card::new(Suit::Blue, Rank::One),
            ..original
        };

        manager.start(3, replacement.clone());

        assert_eq!(&*manager, &[replacement]);
        assert!(manager.validate().is_ok());
        assert!(manager.transitions().iter().any(|transition| {
            transition.reason == ConnectionTransitionReason::Superseded
                && transition.to == ConnectionStatus::Cancelled
        }));
    }
}
