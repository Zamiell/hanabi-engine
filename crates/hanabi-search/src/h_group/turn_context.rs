use hanabi_core::{Card, CardId, ClueFacts, ObservedEvent, ObservedHistoryEntry, PlayerView};

use super::{CardSet, identity_of};

/// Identity information legally available immediately before one history turn.
///
/// This deliberately has no escape hatch to the current/future identity table.
/// Rules that interpret an old event receive this view, which prevents a later
/// play, discard, or draw from changing the meaning assigned to that event.
#[derive(Clone, Copy)]
pub(super) struct HistoricalView<'a> {
    source: &'a PlayerView,
    turn: u32,
}

impl<'a> HistoricalView<'a> {
    pub(super) const fn new(source: &'a PlayerView, turn: u32) -> Self {
        Self { source, turn }
    }

    pub(super) fn identity(self, card: CardId) -> Option<Card> {
        let hand_size = if self.source.hands.len() <= 3 { 5 } else { 4 };
        let initial_cards = hand_size * self.source.hands.len();
        let draw = self
            .source
            .history
            .iter()
            .find_map(|entry| match entry.event {
                ObservedEvent::Drew {
                    player,
                    card: candidate,
                    ..
                } if candidate == card => Some((entry.turn, player)),
                _ => None,
            });
        let owner = match draw {
            Some((draw_turn, _)) if draw_turn >= self.turn => return None,
            Some((_, player)) => player,
            None if card.index() < initial_cards => hanabi_core::PlayerId::new(
                u8::try_from(card.index() / hand_size)
                    .expect("standard Hanabi has at most five players"),
            ),
            None => return None,
        };

        if owner == self.source.observer {
            return self.source.history.iter().find_map(|entry| {
                (entry.turn < self.turn).then(|| match entry.event {
                    ObservedEvent::Played {
                        card: candidate,
                        identity,
                        ..
                    }
                    | ObservedEvent::Discarded {
                        card: candidate,
                        identity,
                        ..
                    } if candidate == card => Some(identity),
                    _ => None,
                })?
            });
        }

        identity_of(self.source, card)
    }

    /// Whether an unknown card can still be this identity at this history turn.
    /// Count only then-visible cards and already revealed plays/discards.
    pub(super) fn has_unseen_copy(self, identity: Card, hands: &[Vec<CardId>]) -> bool {
        let visible = hands
            .iter()
            .flatten()
            .filter(|card| self.identity(**card) == Some(identity))
            .count();
        let revealed = self
            .source
            .history
            .iter()
            .filter(|entry| {
                entry.turn < self.turn
                    && matches!(entry.event,
                ObservedEvent::Played { identity: card, .. }
                    | ObservedEvent::Discarded { identity: card, .. } if card == identity)
            })
            .count();
        visible + revealed < usize::from(identity.rank.copies())
    }
}

/// Public convention-relevant state at one side of an observed event.
#[derive(Clone, Debug)]
pub(super) struct HGroupTurnSnapshot {
    pub(super) hands: Vec<Vec<CardId>>,
    pub(super) facts: Vec<ClueFacts>,
    pub(super) stack_heights: [u8; 5],
    pub(super) clue_tokens: u8,
    pub(super) deck_size: usize,
    pub(super) early_game: bool,
    /// Convention play commitments established strictly before this event.
    /// Rule recognizers compare this snapshot with their mutable post-event
    /// effects instead of reconstructing a subtly different baseline.
    pub(super) already_playing: CardSet,
    /// Positional/Ignition and other blind-play obligations before removal.
    pub(super) forced_playable: CardSet,
    /// Cards whose convention play obligations predate the immediately
    /// preceding clue. A play from this set proves the older obligation; it
    /// cannot retroactively demonstrate a Bluff or Ejection from that clue.
    pub(super) older_play_obligations: CardSet,
}

/// Borrowed convention state after an observed event has been reduced.
///
/// Unlike the pre-event snapshot this does not clone the replay's hot hand and
/// clue-fact tables. Rule evaluation is complete before the next event mutates
/// either table.
pub(super) struct HGroupTurnView<'a> {
    pub(super) hands: &'a [Vec<CardId>],
    pub(super) facts: &'a [ClueFacts],
    pub(super) stack_heights: [u8; 5],
    pub(super) clue_tokens: u8,
    pub(super) deck_size: usize,
    pub(super) early_game: bool,
}

/// Knowledge and action context available to the actor immediately before an
/// event. Keeping this projection together prevents recognizers from mixing
/// observer-visible simulator truth with what the acting player knew.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(super) struct ActorBeliefBefore {
    /// Whether the actor considered the discarded card their ordinary chop.
    pub(super) normal_chop_discard: bool,
    /// Exact identity the actor knew for the discarded card, if any.
    pub(super) discarded_identity: Option<Card>,
}

/// One event with explicit pre- and post-event convention state.
///
/// Convention rules must select the side they require rather than depending
/// on where an effect function happens to be called in the replay loop.
pub(super) struct HGroupTurnContext<'a> {
    pub(super) entry: &'a ObservedHistoryEntry,
    pub(super) historical: HistoricalView<'a>,
    pub(super) before: HGroupTurnSnapshot,
    pub(super) after: HGroupTurnView<'a>,
    pub(super) actor_before: ActorBeliefBefore,
}

#[cfg(test)]
mod tests {
    use hanabi_core::{ClueFacts, GameStatus, ObservedCard, PlayerId, Rank, Suit};

    use super::*;

    #[test]
    fn future_reveals_and_draws_do_not_leak_into_an_earlier_turn() {
        let own = Card::new(Suit::Red, Rank::One);
        let future_draw = Card::new(Suit::Blue, Rank::Two);
        let view = PlayerView {
            observer: PlayerId::new(0),
            current_player: PlayerId::new(0),
            turn: 5,
            hands: vec![
                vec![ObservedCard {
                    id: CardId::new(1),
                    identity: None,
                    clues: ClueFacts::default(),
                }],
                vec![ObservedCard {
                    id: CardId::new(10),
                    identity: Some(future_draw),
                    clues: ClueFacts::default(),
                }],
            ],
            deck_size: 38,
            play_stacks: std::array::from_fn(|_| Vec::new()),
            discard_pile: vec![(CardId::new(0), own)],
            clue_tokens: 8,
            strikes: 0,
            final_turns_remaining: None,
            status: GameStatus::InProgress,
            history: vec![
                ObservedHistoryEntry {
                    turn: 3,
                    event: ObservedEvent::Played {
                        player: PlayerId::new(0),
                        card: CardId::new(0),
                        identity: own,
                        successful: true,
                    },
                },
                ObservedHistoryEntry {
                    turn: 3,
                    event: ObservedEvent::Drew {
                        player: PlayerId::new(1),
                        card: CardId::new(10),
                        identity: Some(future_draw),
                    },
                },
            ],
        };

        let before = HistoricalView::new(&view, 2);
        assert_eq!(before.identity(CardId::new(0)), None);
        assert_eq!(before.identity(CardId::new(10)), None);

        let after = HistoricalView::new(&view, 4);
        assert_eq!(after.identity(CardId::new(0)), Some(own));
        assert_eq!(after.identity(CardId::new(10)), Some(future_draw));
    }
}
