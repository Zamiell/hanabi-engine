use crate::{
    Action, Card, CardId, Clue, FullState, GameEvent, GameStatus, HistoryEntry, MAX_CLUE_TOKENS,
    PlayerId, Rank, Suit,
};

/// Objective positive and negative clue facts attached to one physical card.
/// These are derived from authoritative history rather than stored as a second
/// source of truth.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ClueFacts {
    pub positive_suits: Vec<Suit>,
    pub negative_suits: Vec<Suit>,
    pub positive_ranks: Vec<Rank>,
    pub negative_ranks: Vec<Rank>,
}

impl ClueFacts {
    /// Whether an identity is consistent with direct positive and negative clues.
    #[must_use]
    pub fn allows(&self, card: Card) -> bool {
        (self.positive_suits.is_empty() || self.positive_suits.contains(&card.suit))
            && !self.negative_suits.contains(&card.suit)
            && (self.positive_ranks.is_empty() || self.positive_ranks.contains(&card.rank))
            && !self.negative_ranks.contains(&card.rank)
    }

    fn from_history(card: CardId, history: &[HistoryEntry]) -> Self {
        let mut facts = Self::default();
        for entry in history {
            let GameEvent::Clued {
                clue,
                touched,
                untouched,
                ..
            } = &entry.event
            else {
                continue;
            };

            if touched.contains(&card) {
                facts.record(*clue, true);
            } else if untouched.contains(&card) {
                facts.record(*clue, false);
            }
        }
        facts
    }

    fn record(&mut self, clue: Clue, positive: bool) {
        match (clue, positive) {
            (Clue::Suit(value), true) => push_unique(&mut self.positive_suits, value),
            (Clue::Suit(value), false) => push_unique(&mut self.negative_suits, value),
            (Clue::Rank(value), true) => push_unique(&mut self.positive_ranks, value),
            (Clue::Rank(value), false) => push_unique(&mut self.negative_ranks, value),
        }
    }
}

fn push_unique<T: Copy + Eq>(values: &mut Vec<T>, value: T) {
    if !values.contains(&value) {
        values.push(value);
    }
}

/// A card as legally observed by one player.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ObservedCard {
    pub id: CardId,
    /// `None` only for a card in the observer's own hand.
    pub identity: Option<Card>,
    pub clues: ClueFacts,
}

/// A history event projected for one player.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ObservedEvent {
    Clued {
        giver: PlayerId,
        target: PlayerId,
        clue: Clue,
        touched: Vec<CardId>,
        untouched: Vec<CardId>,
    },
    Played {
        player: PlayerId,
        card: CardId,
        identity: Card,
        successful: bool,
    },
    Discarded {
        player: PlayerId,
        card: CardId,
        identity: Card,
    },
    Drew {
        player: PlayerId,
        card: CardId,
        /// Hidden when the observer drew the card; visible to every other player.
        identity: Option<Card>,
    },
}

/// A turn-numbered observed event.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ObservedHistoryEntry {
    pub turn: u32,
    pub event: ObservedEvent,
}

/// Everything one player is legally permitted to observe.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PlayerView {
    pub observer: PlayerId,
    pub current_player: PlayerId,
    pub turn: u32,
    pub hands: Vec<Vec<ObservedCard>>,
    pub deck_size: usize,
    pub play_stacks: [Vec<(CardId, Card)>; 5],
    pub discard_pile: Vec<(CardId, Card)>,
    pub clue_tokens: u8,
    pub strikes: u8,
    pub final_turns_remaining: Option<u8>,
    pub status: GameStatus,
    pub history: Vec<ObservedHistoryEntry>,
}

impl PlayerView {
    /// Legal actions in stable order. Returns no actions when this view does
    /// not belong to the current player or the game has ended.
    ///
    /// # Panics
    ///
    /// Panics only if a view constructed by the core contains an invalid
    /// player count or omits the observer's hand.
    #[must_use]
    pub fn legal_actions(&self) -> Vec<Action> {
        if self.observer != self.current_player || self.status != GameStatus::InProgress {
            return Vec::new();
        }

        let own_hand = &self.hands[self.observer.index()];
        let mut actions = Vec::with_capacity(own_hand.len() * 2 + 40);
        actions.extend(own_hand.iter().map(|card| Action::Play(card.id)));
        if self.clue_tokens < MAX_CLUE_TOKENS {
            actions.extend(own_hand.iter().map(|card| Action::Discard(card.id)));
        }

        if self.clue_tokens > 0 {
            for (target_index, target_hand) in self.hands.iter().enumerate() {
                if target_index == self.observer.index() {
                    continue;
                }
                let target = PlayerId::new(
                    target_index
                        .try_into()
                        .expect("standard Hanabi has at most five players"),
                );

                for suit in Suit::ALL {
                    if target_hand
                        .iter()
                        .any(|card| card.identity.is_some_and(|identity| identity.suit == suit))
                    {
                        actions.push(Action::Clue {
                            target,
                            clue: Clue::Suit(suit),
                        });
                    }
                }
                for rank in Rank::ALL {
                    if target_hand
                        .iter()
                        .any(|card| card.identity.is_some_and(|identity| identity.rank == rank))
                    {
                        actions.push(Action::Clue {
                            target,
                            clue: Clue::Rank(rank),
                        });
                    }
                }
            }
        }
        actions
    }
}

impl FullState {
    /// Projects authoritative state into the legal observation for `observer`.
    ///
    /// # Panics
    ///
    /// Panics only if the full state violates its internal card-location
    /// invariant.
    #[must_use]
    pub fn view_for(&self, observer: PlayerId) -> Option<PlayerView> {
        if observer.index() >= usize::from(self.num_players()) {
            return None;
        }

        let hands = self
            .hands()
            .iter()
            .enumerate()
            .map(|(player_index, hand)| {
                hand.iter()
                    .map(|id| ObservedCard {
                        id: *id,
                        identity: (player_index != observer.index())
                            .then(|| self.card(*id).expect("located cards have identities")),
                        clues: ClueFacts::from_history(*id, self.history()),
                    })
                    .collect()
            })
            .collect();

        let play_stacks = std::array::from_fn(|suit| {
            self.play_stacks()[suit]
                .iter()
                .map(|id| (*id, self.card(*id).expect("located cards have identities")))
                .collect()
        });
        let discard_pile = self
            .discard_pile()
            .iter()
            .map(|id| (*id, self.card(*id).expect("located cards have identities")))
            .collect();
        let history = self
            .history()
            .iter()
            .map(|entry| ObservedHistoryEntry {
                turn: entry.turn,
                event: observed_event(self, observer, &entry.event),
            })
            .collect();

        Some(PlayerView {
            observer,
            current_player: self.current_player(),
            turn: self.turn(),
            hands,
            deck_size: self.deck_size(),
            play_stacks,
            discard_pile,
            clue_tokens: self.clue_tokens(),
            strikes: self.strikes(),
            final_turns_remaining: self.final_turns_remaining(),
            status: self.status(),
            history,
        })
    }
}

fn observed_event(state: &FullState, observer: PlayerId, event: &GameEvent) -> ObservedEvent {
    match event {
        GameEvent::Clued {
            giver,
            target,
            clue,
            touched,
            untouched,
        } => ObservedEvent::Clued {
            giver: *giver,
            target: *target,
            clue: *clue,
            touched: touched.clone(),
            untouched: untouched.clone(),
        },
        GameEvent::Played {
            player,
            card,
            successful,
        } => ObservedEvent::Played {
            player: *player,
            card: *card,
            identity: state.card(*card).expect("history cards have identities"),
            successful: *successful,
        },
        GameEvent::Discarded { player, card } => ObservedEvent::Discarded {
            player: *player,
            card: *card,
            identity: state.card(*card).expect("history cards have identities"),
        },
        GameEvent::Drew { player, card } => ObservedEvent::Drew {
            player: *player,
            card: *card,
            identity: (*player != observer)
                .then(|| state.card(*card).expect("history cards have identities")),
        },
    }
}
