use core::ops::Deref;

use hanabi_core::{Card, CardId, PlayerId};

use super::HGroupConnectionKind;

/// Stable identity for one concrete connection promise lifecycle.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(super) struct PromiseId(u32);

impl PromiseId {
    pub(super) const UNASSIGNED: Self = Self(u32::MAX);

    #[cfg(test)]
    pub(super) const fn from_raw(value: u32) -> Self {
        Self(value)
    }
}

/// Immutable origin of a connection promise. Current status lives in the
/// manager; provenance survives completion and invalidation for explanation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct PromiseProvenance {
    pub(super) id: PromiseId,
    pub(super) created_turn: u32,
    pub(super) actor: PlayerId,
    pub(super) focus: CardId,
    pub(super) expected: Card,
    pub(super) kind: HGroupConnectionKind,
}

/// One active, typed step in a Prompt or Finesse chain.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct ConnectionObligation {
    pub(super) promise: PromiseId,
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
    IdentityRevealed,
    LayerExtended,
}

/// Auditable lifecycle event for a connection. These records explain current
/// state; they are not themselves queried as current convention facts.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct ConnectionTransition {
    pub(super) promise: PromiseId,
    pub(super) turn: u32,
    pub(super) focus: CardId,
    pub(super) actor: PlayerId,
    pub(super) expected: Card,
    pub(super) focus_identity: Card,
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
    provenance: Vec<PromiseProvenance>,
    next_promise: u32,
}

impl Deref for ConnectionManager {
    type Target = [ConnectionObligation];

    fn deref(&self) -> &Self::Target {
        &self.active
    }
}

impl ConnectionManager {
    pub(super) fn transitions(&self) -> &[ConnectionTransition] {
        &self.transitions
    }

    pub(super) fn provenance(&self, id: PromiseId) -> Option<PromiseProvenance> {
        self.provenance
            .iter()
            .copied()
            .find(|origin| origin.id == id)
    }

    pub(super) fn was_created_on(&self, connection: &ConnectionObligation, turn: u32) -> bool {
        self.provenance(connection.promise)
            .is_some_and(|provenance| provenance.created_turn == turn)
    }

    /// Whether an actor was already carrying a different live connection
    /// immediately before `turn`. This historical query is used when
    /// compiling a later clue: a loaded player retains the clue's direct and
    /// delayed superposition until public actions disambiguate it.
    pub(super) fn actor_had_pending_before(
        &self,
        actor: PlayerId,
        turn: u32,
        excluded_focus: CardId,
    ) -> bool {
        self.provenance
            .iter()
            .filter(|origin| {
                origin.actor == actor
                    && origin.focus != excluded_focus
                    && origin.created_turn < turn
            })
            .any(|origin| {
                self.transitions
                    .iter()
                    .enumerate()
                    .filter(|(_, transition)| {
                        transition.promise == origin.id && transition.turn < turn
                    })
                    .max_by_key(|(index, transition)| (transition.turn, *index))
                    .is_some_and(|(_, transition)| {
                        matches!(
                            transition.to,
                            ConnectionStatus::Pending | ConnectionStatus::AwaitingFix
                        )
                    })
            })
    }

    /// Returns the focus identity only after an earlier blind play has
    /// demonstrated that the still-active layered connection is real.
    /// Scheduling a connection is not evidence by itself: until a layer
    /// actually plays, the focus may remain in a direct/delayed
    /// superposition.
    pub(super) fn demonstrated_focus_identity(&self, focus: CardId) -> Option<Card> {
        self.active
            .iter()
            .find_map(|connection| {
                (connection.focus == focus
                    && self.promise_was_demonstrated_after(connection.promise, 0))
                .then_some(connection.focus_identity)
            })
            .or_else(|| {
                self.transitions
                    .iter()
                    .rev()
                    .find(|transition| transition.focus == focus)
                    .filter(|transition| transition.to == ConnectionStatus::Satisfied)
                    .map(|transition| transition.focus_identity)
            })
    }

    /// Whether a queued promise for this identity was publicly demonstrated
    /// after a later clue was given. This is what lets a recipient upgrade a
    /// new direct interpretation to one queued behind an older connection.
    pub(super) fn identity_was_demonstrated_after(&self, identity: Card, turn: u32) -> bool {
        self.transitions.iter().any(|transition| {
            transition.focus_identity == identity
                && transition.turn > turn
                && transition.from == ConnectionStatus::Pending
                && matches!(
                    transition.reason,
                    ConnectionTransitionReason::PlayedAlternative
                        | ConnectionTransitionReason::PlayedExpectedCard
                )
        })
    }

    #[cfg(test)]
    pub(super) fn identity_was_queued_at(&self, identity: Card, turn: u32) -> bool {
        self.transitions
            .iter()
            .filter(|transition| {
                transition.turn <= turn
                    && transition.reason == ConnectionTransitionReason::Scheduled
                    && (transition.expected == identity || transition.focus_identity == identity)
            })
            .any(|scheduled| {
                self.transitions
                    .iter()
                    .enumerate()
                    .filter(|(_, transition)| {
                        transition.promise == scheduled.promise && transition.turn <= turn
                    })
                    .max_by_key(|(index, transition)| (transition.turn, *index))
                    .is_some_and(|(_, transition)| transition.to == ConnectionStatus::Pending)
            })
    }

    pub(super) fn promise_was_demonstrated_after(&self, promise: PromiseId, turn: u32) -> bool {
        self.transitions.iter().any(|transition| {
            transition.promise == promise
                && transition.turn > turn
                && transition.from == ConnectionStatus::Pending
                && transition.to == ConnectionStatus::Pending
                && transition.reason == ConnectionTransitionReason::PlayedAlternative
        })
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
            if self.provenance(connection.promise).is_none() {
                return Err("active connection has no promise provenance".to_owned());
            }
        }
        Ok(())
    }

    pub(super) fn start(&mut self, turn: u32, mut obligation: ConnectionObligation) -> PromiseId {
        if self.active.iter().any(|active| {
            active.actor == obligation.actor
                && active.cards == obligation.cards
                && active.expected == obligation.expected
                && active.focus_identity == obligation.focus_identity
                && active.kind == obligation.kind
                && active.focus == obligation.focus
                && active.step == obligation.step
        }) {
            return self
                .active
                .iter()
                .find(|active| {
                    active.actor == obligation.actor
                        && active.cards == obligation.cards
                        && active.expected == obligation.expected
                        && active.focus_identity == obligation.focus_identity
                        && active.kind == obligation.kind
                        && active.focus == obligation.focus
                        && active.step == obligation.step
                })
                .map_or(PromiseId::UNASSIGNED, |active| active.promise);
        }
        obligation.promise = PromiseId(self.next_promise);
        self.next_promise = self.next_promise.saturating_add(1);
        self.provenance.push(PromiseProvenance {
            id: obligation.promise,
            created_turn: turn,
            actor: obligation.actor,
            focus: obligation.focus,
            expected: obligation.expected,
            kind: obligation.kind,
        });
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
        let promise = obligation.promise;
        self.active.push(obligation);
        promise
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

    /// Removes a card from incompatible connection alternatives after public
    /// convention evidence establishes its exact identity. This is distinct
    /// from a Fix: the original clue was not repaired; one branch of its
    /// superposition became impossible.
    pub(super) fn reveal_identity(
        &mut self,
        turn: u32,
        actor: PlayerId,
        card: CardId,
        identity: Card,
    ) {
        let mut narrowed = Vec::new();
        for connection in &mut self.active {
            if connection.actor != actor
                || connection.expected == identity
                || !connection.cards.contains(&card)
            {
                continue;
            }
            connection.cards.retain(|candidate| *candidate != card);
            if !connection.cards.is_empty() {
                narrowed.push(connection.clone());
            }
        }
        for connection in narrowed {
            self.record(
                turn,
                &connection,
                ConnectionStatus::Pending,
                ConnectionStatus::Pending,
                ConnectionTransitionReason::IdentityRevealed,
            );
        }
        self.cancel_where(
            turn,
            ConnectionTransitionReason::IdentityRevealed,
            |connection| connection.cards.is_empty(),
        );
    }

    /// Prepends newly demonstrated blind plays to an existing Finesse.
    ///
    /// A later clue can layer one or more currently playable cards in front
    /// of a connection that was already queued. Keeping that refinement on
    /// the original promise preserves its provenance and makes advancement
    /// release the layers in the same order in which they must be played.
    pub(super) fn prepend_layers(
        &mut self,
        turn: u32,
        promise: PromiseId,
        layers: &[CardId],
    ) -> Option<ConnectionObligation> {
        if layers.is_empty() {
            return self
                .active
                .iter()
                .find(|connection| connection.promise == promise)
                .cloned();
        }
        let connection = self
            .active
            .iter_mut()
            .find(|connection| connection.promise == promise)?;
        let mut cards = layers
            .iter()
            .copied()
            .filter(|card| !connection.cards.contains(card))
            .collect::<Vec<_>>();
        if cards.is_empty() {
            return Some(connection.clone());
        }
        cards.append(&mut connection.cards);
        connection.cards = cards;
        let updated = connection.clone();
        self.record(
            turn,
            &updated,
            ConnectionStatus::Pending,
            ConnectionStatus::Pending,
            ConnectionTransitionReason::LayerExtended,
        );
        Some(updated)
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
            } else {
                // A successful alternative is the public demonstration of a
                // layered connection. The promise remains pending on its next
                // candidate, but this state change must still be journaled:
                // convention knowledge may now rule out a simultaneous direct
                // interpretation of the focus.
                transitions.push((
                    obligation,
                    ConnectionStatus::Pending,
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
            promise: obligation.promise,
            turn,
            focus: obligation.focus,
            actor: obligation.actor,
            expected: obligation.expected,
            focus_identity: obligation.focus_identity,
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
            promise: PromiseId::UNASSIGNED,
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

        assert_eq!(manager.len(), 1);
        assert_eq!(manager[0].cards, replacement.cards);
        assert_eq!(manager[0].expected, replacement.expected);
        assert_ne!(manager[0].promise, PromiseId::UNASSIGNED);
        assert_eq!(
            manager
                .provenance(manager[0].promise)
                .map(|origin| origin.created_turn),
            Some(3)
        );
        assert!(manager.validate().is_ok());
        assert!(manager.transitions().iter().any(|transition| {
            transition.reason == ConnectionTransitionReason::Superseded
                && transition.to == ConnectionStatus::Cancelled
        }));
    }

    #[test]
    fn a_successful_layer_records_demonstration_without_completing_the_promise() {
        let mut manager = ConnectionManager::default();
        let focus = CardId::new(9);
        manager.start(3, obligation(vec![CardId::new(5), CardId::new(7)]));

        manager.advance_play(
            4,
            PlayerId::new(1),
            CardId::new(5),
            Card::new(Suit::Blue, Rank::One),
            true,
        );

        assert_eq!(manager.len(), 1);
        assert_eq!(manager[0].cards, [CardId::new(7)]);
        assert_eq!(
            manager.demonstrated_focus_identity(focus),
            Some(Card::new(Suit::Red, Rank::Three))
        );
        assert!(manager.transitions().iter().any(|transition| {
            transition.reason == ConnectionTransitionReason::PlayedAlternative
                && transition.from == ConnectionStatus::Pending
                && transition.to == ConnectionStatus::Pending
        }));
    }

    #[test]
    fn historical_queue_state_survives_later_completion() {
        let mut manager = ConnectionManager::default();
        let red_two = Card::new(Suit::Red, Rank::Two);
        let red_three = Card::new(Suit::Red, Rank::Three);
        manager.start(3, obligation(vec![CardId::new(5)]));
        manager.advance_play(5, PlayerId::new(1), CardId::new(5), red_two, true);

        assert!(manager.identity_was_queued_at(red_two, 4));
        assert!(manager.identity_was_queued_at(red_three, 4));
        assert!(!manager.identity_was_queued_at(red_two, 5));
    }
}
