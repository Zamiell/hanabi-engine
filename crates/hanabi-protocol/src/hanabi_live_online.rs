use core::fmt;
use std::collections::HashSet;

use hanabi_core::{
    Action, Card, CardId, Clue, ClueFacts, EndReason, GameStatus, ObservedCard, ObservedEvent,
    ObservedHistoryEntry, PlayerId, PlayerView, Rank, Suit,
};
use serde::{Deserialize, Serialize};

const STANDARD_DECK_SIZE: usize = 50;
const MAX_CLUE_TOKENS: u8 = 8;

/// A player-safe snapshot assembled from Hanabi Live init and scrubbed action
/// messages.
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HanabiLiveSnapshot {
    #[serde(rename = "tableID")]
    table_id: u64,
    player_names: Vec<String>,
    our_player_index: usize,
    #[serde(default)]
    spectating: bool,
    #[serde(default)]
    replay: bool,
    options: HanabiLiveOnlineOptions,
    actions: Vec<HanabiLiveOnlineAction>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct HanabiLiveOnlineOptions {
    variant_name: String,
}

#[derive(Clone, Copy, Debug, Deserialize)]
struct HanabiLiveOnlineClue {
    #[serde(rename = "type")]
    clue_type: u8,
    value: i16,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(tag = "type")]
enum HanabiLiveOnlineAction {
    #[serde(rename = "draw", rename_all = "camelCase")]
    Draw {
        player_index: usize,
        order: usize,
        suit_index: i16,
        rank: i16,
    },
    #[serde(rename = "clue", rename_all = "camelCase")]
    Clue {
        clue: HanabiLiveOnlineClue,
        giver: usize,
        list: Vec<usize>,
        target: usize,
        #[serde(default)]
        turn: Option<u32>,
    },
    #[serde(rename = "play", rename_all = "camelCase")]
    Play {
        player_index: usize,
        order: usize,
        suit_index: i16,
        rank: i16,
    },
    #[serde(rename = "discard", rename_all = "camelCase")]
    Discard {
        player_index: usize,
        order: usize,
        suit_index: i16,
        rank: i16,
        failed: bool,
    },
    #[serde(rename = "strike")]
    Strike { num: u8 },
    #[serde(rename = "status")]
    Status { clues: u8 },
    #[serde(rename = "turn", rename_all = "camelCase")]
    Turn { num: u32, current_player_index: i16 },
    #[serde(rename = "gameOver")]
    GameOver,
    #[serde(other)]
    Ignored,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum HanabiLiveSessionRequest {
    Initialize {
        snapshot: HanabiLiveSnapshot,
    },
    Append {
        #[serde(rename = "tableID")]
        table_id: u64,
        actions: Vec<HanabiLiveOnlineAction>,
    },
}

/// Incremental state for one persistent Hanabi Live engine process.
///
/// The first request initializes the session from the server's complete,
/// scrubbed action list. Later requests append only newly received actions,
/// avoiding replaying the full game history before every planning request.
#[derive(Clone, Debug, Default)]
pub struct HanabiLiveSessionState {
    session: Option<HanabiLiveSession>,
}

#[derive(Clone, Debug)]
struct HanabiLiveSession {
    table_id: u64,
    builder: LiveViewBuilder,
}

/// Wire payload accepted by the Hanabi Live action WebSocket command.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HanabiLiveActionCommand {
    #[serde(rename = "tableID")]
    pub table_id: u64,
    #[serde(rename = "type")]
    pub action_type: u8,
    pub target: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value: Option<u8>,
}

impl HanabiLiveSnapshot {
    /// Parses one live snapshot JSON document.
    ///
    /// # Errors
    ///
    /// Returns `LiveSnapshotError::Json` when the document does not match the
    /// live snapshot schema.
    pub fn from_json(json: &str) -> Result<Self, LiveSnapshotError> {
        serde_json::from_str(json).map_err(LiveSnapshotError::Json)
    }

    #[must_use]
    pub const fn table_id(&self) -> u64 {
        self.table_id
    }

    /// Reconstructs the exact observation legally available to the bot.
    ///
    /// The server scrubs the bot's own draws to -1. This adapter never accepts
    /// another player's hidden identity or fills an own identity from
    /// simulator truth.
    ///
    /// # Errors
    ///
    /// Returns `LiveSnapshotError` when the game is not a standard active-player
    /// game or the action stream is malformed or incomplete.
    pub fn player_view(&self) -> Result<PlayerView, LiveSnapshotError> {
        HanabiLiveSession::from_snapshot(self)?.player_view()
    }
}

impl HanabiLiveSessionState {
    /// Creates an empty session awaiting an `initialize` request.
    #[must_use]
    pub const fn new() -> Self {
        Self { session: None }
    }

    /// Applies one newline-delimited JSON session request and returns the
    /// current table and player view.
    ///
    /// Updates are transactional: malformed action batches leave the previous
    /// session state untouched so callers can restart or resynchronize safely.
    ///
    /// # Errors
    ///
    /// Returns [`LiveSnapshotError`] when the request is malformed, arrives
    /// before initialization, targets another table, or contains invalid game
    /// actions.
    pub fn apply_json(&mut self, json: &str) -> Result<(u64, PlayerView), LiveSnapshotError> {
        let request: HanabiLiveSessionRequest =
            serde_json::from_str(json).map_err(LiveSnapshotError::Json)?;
        match request {
            HanabiLiveSessionRequest::Initialize { snapshot } => {
                let candidate = HanabiLiveSession::from_snapshot(&snapshot)?;
                let view = candidate.player_view()?;
                let table_id = candidate.table_id;
                self.session = Some(candidate);
                Ok((table_id, view))
            }
            HanabiLiveSessionRequest::Append { table_id, actions } => {
                let current = self
                    .session
                    .as_ref()
                    .ok_or(LiveSnapshotError::SessionNotInitialized)?;
                if current.table_id != table_id {
                    return Err(LiveSnapshotError::SessionTableMismatch {
                        expected: current.table_id,
                        actual: table_id,
                    });
                }
                let mut candidate = current.clone();
                candidate.append(&actions)?;
                let view = candidate.player_view()?;
                self.session = Some(candidate);
                Ok((table_id, view))
            }
        }
    }
}

impl HanabiLiveSession {
    fn from_snapshot(snapshot: &HanabiLiveSnapshot) -> Result<Self, LiveSnapshotError> {
        if snapshot.spectating {
            return Err(LiveSnapshotError::Spectating);
        }
        if snapshot.replay {
            return Err(LiveSnapshotError::Replay);
        }
        if snapshot.options.variant_name != "No Variant" {
            return Err(LiveSnapshotError::UnsupportedVariant(
                snapshot.options.variant_name.clone(),
            ));
        }
        if !(2..=5).contains(&snapshot.player_names.len()) {
            return Err(LiveSnapshotError::InvalidPlayerCount(
                snapshot.player_names.len(),
            ));
        }
        if snapshot.our_player_index >= snapshot.player_names.len() {
            return Err(LiveSnapshotError::InvalidObserver(
                snapshot.our_player_index,
            ));
        }

        let mut builder =
            LiveViewBuilder::new(snapshot.player_names.len(), snapshot.our_player_index);
        for action in &snapshot.actions {
            builder.apply(action)?;
        }
        Ok(Self {
            table_id: snapshot.table_id,
            builder,
        })
    }

    fn append(&mut self, actions: &[HanabiLiveOnlineAction]) -> Result<(), LiveSnapshotError> {
        for action in actions {
            self.builder.apply(action)?;
        }
        Ok(())
    }

    fn player_view(&self) -> Result<PlayerView, LiveSnapshotError> {
        self.builder.player_view()
    }
}

impl HanabiLiveActionCommand {
    /// Converts an engine action into the payload expected by Hanabi Live.
    #[must_use]
    pub fn from_engine_action(table_id: u64, action: Action) -> Self {
        match action {
            Action::Play(card) => Self {
                table_id,
                action_type: 0,
                target: card.index(),
                value: None,
            },
            Action::Discard(card) => Self {
                table_id,
                action_type: 1,
                target: card.index(),
                value: None,
            },
            Action::Clue { target, clue } => match clue {
                Clue::Suit(suit) => Self {
                    table_id,
                    action_type: 2,
                    target: target.index(),
                    value: Some(match suit {
                        Suit::Red => 0,
                        Suit::Yellow => 1,
                        Suit::Green => 2,
                        Suit::Blue => 3,
                        Suit::Purple => 4,
                    }),
                },
                Clue::Rank(rank) => Self {
                    table_id,
                    action_type: 3,
                    target: target.index(),
                    value: Some(rank.number()),
                },
            },
        }
    }
}

#[derive(Clone, Debug)]
struct LiveViewBuilder {
    observer: PlayerId,
    current_player: PlayerId,
    turn: u32,
    hands: Vec<Vec<ObservedCard>>,
    deck_size: usize,
    play_stacks: [Vec<(CardId, Card)>; 5],
    discard_pile: Vec<(CardId, Card)>,
    clue_tokens: u8,
    strikes: u8,
    final_turns_remaining: Option<u8>,
    final_round_start_turn: Option<u32>,
    status: GameStatus,
    history: Vec<ObservedHistoryEntry>,
    seen_cards: [bool; STANDARD_DECK_SIZE],
    initial_deal_remaining: usize,
}

impl LiveViewBuilder {
    fn new(player_count: usize, observer: usize) -> Self {
        let hand_size = if player_count <= 3 { 5 } else { 4 };
        Self {
            observer: player_id(observer),
            current_player: player_id(0),
            turn: 0,
            hands: vec![Vec::with_capacity(hand_size); player_count],
            deck_size: STANDARD_DECK_SIZE,
            play_stacks: std::array::from_fn(|_| Vec::with_capacity(5)),
            discard_pile: Vec::new(),
            clue_tokens: MAX_CLUE_TOKENS,
            strikes: 0,
            final_turns_remaining: None,
            final_round_start_turn: None,
            status: GameStatus::InProgress,
            history: Vec::new(),
            seen_cards: [false; STANDARD_DECK_SIZE],
            initial_deal_remaining: hand_size * player_count,
        }
    }

    fn apply(&mut self, action: &HanabiLiveOnlineAction) -> Result<(), LiveSnapshotError> {
        match action {
            HanabiLiveOnlineAction::Draw {
                player_index,
                order,
                suit_index,
                rank,
            } => self.draw(*player_index, *order, *suit_index, *rank),
            HanabiLiveOnlineAction::Clue {
                clue,
                giver,
                list,
                target,
                turn,
            } => self.clue(*clue, *giver, list, *target, *turn),
            HanabiLiveOnlineAction::Play {
                player_index,
                order,
                suit_index,
                rank,
            } => self.play(*player_index, *order, *suit_index, *rank),
            HanabiLiveOnlineAction::Discard {
                player_index,
                order,
                suit_index,
                rank,
                failed,
            } => self.discard(*player_index, *order, *suit_index, *rank, *failed),
            HanabiLiveOnlineAction::Strike { num } => {
                if *num == 0 || *num > 3 {
                    return Err(LiveSnapshotError::InvalidStrikeCount(*num));
                }
                self.strikes = *num;
                Ok(())
            }
            HanabiLiveOnlineAction::Status { clues } => {
                if *clues > MAX_CLUE_TOKENS {
                    return Err(LiveSnapshotError::InvalidClueTokenCount(*clues));
                }
                self.clue_tokens = *clues;
                Ok(())
            }
            HanabiLiveOnlineAction::Turn {
                num,
                current_player_index,
            } => self.advance_turn(*num, *current_player_index),
            HanabiLiveOnlineAction::GameOver => {
                self.status = if self.strikes >= 3 {
                    GameStatus::Finished(EndReason::TooManyStrikes)
                } else if self.play_stacks.iter().map(Vec::len).sum::<usize>() == 25 {
                    GameStatus::Finished(EndReason::PerfectScore)
                } else {
                    GameStatus::Finished(EndReason::FinalRoundComplete)
                };
                Ok(())
            }
            HanabiLiveOnlineAction::Ignored => Ok(()),
        }
    }

    fn draw(
        &mut self,
        player_index: usize,
        order: usize,
        suit_index: i16,
        rank: i16,
    ) -> Result<(), LiveSnapshotError> {
        self.require_player(player_index)?;
        let card = self.new_card(order)?;
        let supplied_identity = parse_optional_identity(suit_index, rank)?;
        let identity = if player_index == self.observer.index() {
            None
        } else {
            Some(
                supplied_identity
                    .ok_or(LiveSnapshotError::HiddenTeammateCard { card, player_index })?,
            )
        };
        self.hands[player_index].push(ObservedCard {
            id: card,
            identity,
            clues: ClueFacts::default(),
        });
        self.deck_size = self
            .deck_size
            .checked_sub(1)
            .ok_or(LiveSnapshotError::TooManyDraws)?;

        if self.initial_deal_remaining > 0 {
            self.initial_deal_remaining -= 1;
        } else {
            self.history.push(ObservedHistoryEntry {
                turn: self.turn,
                event: ObservedEvent::Drew {
                    player: player_id(player_index),
                    card,
                    identity,
                },
            });
            if self.deck_size == 0 {
                let player_count = u8::try_from(self.hands.len())
                    .expect("standard Hanabi has at most five players");
                self.final_turns_remaining = Some(player_count);
                self.final_round_start_turn = Some(self.turn + 1);
            }
        }
        Ok(())
    }

    fn clue(
        &mut self,
        wire_clue: HanabiLiveOnlineClue,
        giver: usize,
        touched_orders: &[usize],
        target: usize,
        event_turn: Option<u32>,
    ) -> Result<(), LiveSnapshotError> {
        self.require_player(giver)?;
        self.require_player(target)?;
        let clue = parse_clue(wire_clue)?;
        let touched = touched_orders
            .iter()
            .copied()
            .map(checked_card_id)
            .collect::<Result<HashSet<_>, _>>()?;
        if touched.len() != touched_orders.len() {
            return Err(LiveSnapshotError::DuplicateTouchedCard);
        }

        let mut touched_cards = Vec::new();
        let mut untouched_cards = Vec::new();
        for card in &mut self.hands[target] {
            if touched.contains(&card.id) {
                card.clues.add_positive_clue(clue);
                touched_cards.push(card.id);
            } else {
                card.clues.add_negative_clue(clue);
                untouched_cards.push(card.id);
            }
        }
        if touched_cards.len() != touched.len() {
            return Err(LiveSnapshotError::TouchedCardNotInHand);
        }
        self.clue_tokens = self
            .clue_tokens
            .checked_sub(1)
            .ok_or(LiveSnapshotError::ClueWithoutToken)?;
        self.history.push(ObservedHistoryEntry {
            turn: event_turn.unwrap_or(self.turn),
            event: ObservedEvent::Clued {
                giver: player_id(giver),
                target: player_id(target),
                clue,
                touched: touched_cards,
                untouched: untouched_cards,
            },
        });
        Ok(())
    }

    fn play(
        &mut self,
        player_index: usize,
        order: usize,
        suit_index: i16,
        rank: i16,
    ) -> Result<(), LiveSnapshotError> {
        self.require_player(player_index)?;
        let card = checked_card_id(order)?;
        let identity = parse_required_identity(card, suit_index, rank)?;
        self.remove_card(player_index, card, identity)?;
        let stack = &mut self.play_stacks[identity.suit.index()];
        if usize::from(identity.rank.number()) != stack.len() + 1 {
            return Err(LiveSnapshotError::InvalidSuccessfulPlay { card, identity });
        }
        stack.push((card, identity));
        if identity.rank == Rank::Five {
            self.clue_tokens = self.clue_tokens.saturating_add(1).min(MAX_CLUE_TOKENS);
        }
        self.history.push(ObservedHistoryEntry {
            turn: self.turn,
            event: ObservedEvent::Played {
                player: player_id(player_index),
                card,
                identity,
                successful: true,
            },
        });
        Ok(())
    }

    fn discard(
        &mut self,
        player_index: usize,
        order: usize,
        suit_index: i16,
        rank: i16,
        failed: bool,
    ) -> Result<(), LiveSnapshotError> {
        self.require_player(player_index)?;
        let card = checked_card_id(order)?;
        let identity = parse_required_identity(card, suit_index, rank)?;
        self.remove_card(player_index, card, identity)?;
        self.discard_pile.push((card, identity));
        let event = if failed {
            ObservedEvent::Played {
                player: player_id(player_index),
                card,
                identity,
                successful: false,
            }
        } else {
            self.clue_tokens = self.clue_tokens.saturating_add(1).min(MAX_CLUE_TOKENS);
            ObservedEvent::Discarded {
                player: player_id(player_index),
                card,
                identity,
            }
        };
        self.history.push(ObservedHistoryEntry {
            turn: self.turn,
            event,
        });
        if failed && self.strikes >= 3 {
            self.status = GameStatus::Finished(EndReason::TooManyStrikes);
        }
        Ok(())
    }

    fn advance_turn(
        &mut self,
        turn: u32,
        current_player_index: i16,
    ) -> Result<(), LiveSnapshotError> {
        if turn < self.turn {
            return Err(LiveSnapshotError::TurnWentBackward {
                previous: self.turn,
                next: turn,
            });
        }
        self.turn = turn;
        if let Some(start) = self.final_round_start_turn {
            let elapsed = turn.saturating_sub(start);
            let player_count =
                u8::try_from(self.hands.len()).expect("standard Hanabi has at most five players");
            self.final_turns_remaining =
                Some(player_count.saturating_sub(u8::try_from(elapsed).unwrap_or(u8::MAX)));
        }
        if current_player_index >= 0 {
            let current = usize::try_from(current_player_index)
                .expect("a nonnegative player index fits in usize");
            self.require_player(current)?;
            self.current_player = player_id(current);
        }
        Ok(())
    }

    fn remove_card(
        &mut self,
        player_index: usize,
        card: CardId,
        revealed: Card,
    ) -> Result<(), LiveSnapshotError> {
        let hand = &mut self.hands[player_index];
        let position = hand
            .iter()
            .position(|candidate| candidate.id == card)
            .ok_or(LiveSnapshotError::CardNotInHand { card, player_index })?;
        if let Some(visible) = hand[position].identity {
            if visible != revealed {
                return Err(LiveSnapshotError::IdentityChanged {
                    card,
                    visible,
                    revealed,
                });
            }
        }
        hand.remove(position);
        Ok(())
    }

    fn new_card(&mut self, order: usize) -> Result<CardId, LiveSnapshotError> {
        let card = checked_card_id(order)?;
        if self.seen_cards[order] {
            return Err(LiveSnapshotError::DuplicateCard(card));
        }
        self.seen_cards[order] = true;
        Ok(card)
    }

    fn require_player(&self, player_index: usize) -> Result<(), LiveSnapshotError> {
        if player_index >= self.hands.len() {
            return Err(LiveSnapshotError::InvalidPlayerIndex(player_index));
        }
        Ok(())
    }

    fn player_view(&self) -> Result<PlayerView, LiveSnapshotError> {
        if self.initial_deal_remaining != 0 {
            return Err(LiveSnapshotError::IncompleteInitialDeal(
                self.initial_deal_remaining,
            ));
        }
        Ok(PlayerView {
            observer: self.observer,
            current_player: self.current_player,
            turn: self.turn,
            hands: self.hands.clone(),
            deck_size: self.deck_size,
            play_stacks: self.play_stacks.clone(),
            discard_pile: self.discard_pile.clone(),
            clue_tokens: self.clue_tokens,
            strikes: self.strikes,
            final_turns_remaining: self.final_turns_remaining,
            status: self.status,
            history: self.history.clone(),
        })
    }
}

fn player_id(index: usize) -> PlayerId {
    PlayerId::new(u8::try_from(index).expect("standard Hanabi has at most five players"))
}

fn checked_card_id(order: usize) -> Result<CardId, LiveSnapshotError> {
    if order >= STANDARD_DECK_SIZE {
        return Err(LiveSnapshotError::InvalidCardOrder(order));
    }
    Ok(CardId::new(order))
}

fn parse_optional_identity(suit_index: i16, rank: i16) -> Result<Option<Card>, LiveSnapshotError> {
    match (suit_index, rank) {
        (-1, -1) => Ok(None),
        (-1, _) | (_, -1) => Err(LiveSnapshotError::PartiallyHiddenIdentity { suit_index, rank }),
        _ => Ok(Some(Card::new(parse_suit(suit_index)?, parse_rank(rank)?))),
    }
}

fn parse_required_identity(
    card: CardId,
    suit_index: i16,
    rank: i16,
) -> Result<Card, LiveSnapshotError> {
    parse_optional_identity(suit_index, rank)?.ok_or(LiveSnapshotError::HiddenPublicCard(card))
}

fn parse_suit(value: i16) -> Result<Suit, LiveSnapshotError> {
    match value {
        0 => Ok(Suit::Red),
        1 => Ok(Suit::Yellow),
        2 => Ok(Suit::Green),
        3 => Ok(Suit::Blue),
        4 => Ok(Suit::Purple),
        _ => Err(LiveSnapshotError::InvalidSuit(value)),
    }
}

fn parse_rank(value: i16) -> Result<Rank, LiveSnapshotError> {
    match value {
        1 => Ok(Rank::One),
        2 => Ok(Rank::Two),
        3 => Ok(Rank::Three),
        4 => Ok(Rank::Four),
        5 => Ok(Rank::Five),
        _ => Err(LiveSnapshotError::InvalidRank(value)),
    }
}

fn parse_clue(clue: HanabiLiveOnlineClue) -> Result<Clue, LiveSnapshotError> {
    match clue.clue_type {
        0 => parse_suit(clue.value).map(Clue::Suit),
        1 => parse_rank(clue.value).map(Clue::Rank),
        other => Err(LiveSnapshotError::InvalidClueType(other)),
    }
}

/// Why a scrubbed Hanabi Live action stream could not become a player view.
#[derive(Debug)]
pub enum LiveSnapshotError {
    Json(serde_json::Error),
    Spectating,
    Replay,
    UnsupportedVariant(String),
    InvalidPlayerCount(usize),
    InvalidObserver(usize),
    InvalidPlayerIndex(usize),
    InvalidCardOrder(usize),
    DuplicateCard(CardId),
    TooManyDraws,
    IncompleteInitialDeal(usize),
    HiddenTeammateCard {
        card: CardId,
        player_index: usize,
    },
    HiddenPublicCard(CardId),
    PartiallyHiddenIdentity {
        suit_index: i16,
        rank: i16,
    },
    InvalidSuit(i16),
    InvalidRank(i16),
    InvalidClueType(u8),
    DuplicateTouchedCard,
    TouchedCardNotInHand,
    ClueWithoutToken,
    InvalidClueTokenCount(u8),
    InvalidStrikeCount(u8),
    CardNotInHand {
        card: CardId,
        player_index: usize,
    },
    IdentityChanged {
        card: CardId,
        visible: Card,
        revealed: Card,
    },
    InvalidSuccessfulPlay {
        card: CardId,
        identity: Card,
    },
    TurnWentBackward {
        previous: u32,
        next: u32,
    },
    SessionNotInitialized,
    SessionTableMismatch {
        expected: u64,
        actual: u64,
    },
}

impl fmt::Display for LiveSnapshotError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Json(error) => write!(formatter, "invalid live snapshot JSON: {error}"),
            Self::Spectating => formatter.write_str("the bot is spectating rather than playing"),
            Self::Replay => formatter.write_str("the table is a replay rather than a live game"),
            Self::UnsupportedVariant(variant) => {
                write!(formatter, "unsupported Hanabi Live variant {variant:?}")
            }
            Self::InvalidPlayerCount(count) => {
                write!(formatter, "live game has invalid player count {count}")
            }
            Self::InvalidObserver(index) => {
                write!(formatter, "bot player index {index} is out of range")
            }
            Self::InvalidPlayerIndex(index) => {
                write!(formatter, "action player index {index} is out of range")
            }
            Self::InvalidCardOrder(order) => {
                write!(formatter, "card order {order} is outside the standard deck")
            }
            Self::DuplicateCard(card) => write!(formatter, "card {card} was drawn more than once"),
            Self::TooManyDraws => formatter.write_str("action stream draws more than 50 cards"),
            Self::IncompleteInitialDeal(remaining) => {
                write!(
                    formatter,
                    "initial deal is missing {remaining} draw actions"
                )
            }
            Self::HiddenTeammateCard { card, player_index } => write!(
                formatter,
                "teammate card {card} in player {player_index}'s hand was unexpectedly hidden"
            ),
            Self::HiddenPublicCard(card) => {
                write!(formatter, "public card {card} has a hidden identity")
            }
            Self::PartiallyHiddenIdentity { suit_index, rank } => write!(
                formatter,
                "card identity is only partly hidden (suit {suit_index}, rank {rank})"
            ),
            Self::InvalidSuit(suit) => write!(formatter, "invalid standard suit index {suit}"),
            Self::InvalidRank(rank) => write!(formatter, "invalid standard rank {rank}"),
            Self::InvalidClueType(clue_type) => {
                write!(formatter, "invalid standard clue type {clue_type}")
            }
            Self::DuplicateTouchedCard => {
                formatter.write_str("clue lists the same touched card more than once")
            }
            Self::TouchedCardNotInHand => {
                formatter.write_str("clue touches a card outside the target hand")
            }
            Self::ClueWithoutToken => {
                formatter.write_str("clue was given with no clue token available")
            }
            Self::InvalidClueTokenCount(count) => {
                write!(
                    formatter,
                    "server reported invalid clue-token count {count}"
                )
            }
            Self::InvalidStrikeCount(count) => {
                write!(formatter, "server reported invalid strike count {count}")
            }
            Self::CardNotInHand { card, player_index } => {
                write!(
                    formatter,
                    "card {card} is not in player {player_index}'s hand"
                )
            }
            Self::IdentityChanged {
                card,
                visible,
                revealed,
            } => write!(
                formatter,
                "card {card} changed identity from {visible} to {revealed}"
            ),
            Self::InvalidSuccessfulPlay { card, identity } => {
                write!(
                    formatter,
                    "server reported {card} ({identity}) as a successful play"
                )
            }
            Self::TurnWentBackward { previous, next } => {
                write!(formatter, "turn went backward from {previous} to {next}")
            }
            Self::SessionNotInitialized => {
                formatter.write_str("live session received actions before initialization")
            }
            Self::SessionTableMismatch { expected, actual } => write!(
                formatter,
                "live session is for table {expected}, but update targets table {actual}"
            ),
        }
    }
}

impl std::error::Error for LiveSnapshotError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Json(error) => Some(error),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use hanabi_core::{Clue, ObservedEvent, Rank};

    fn snapshot_json() -> String {
        serde_json::json!({
            "tableID": 17,
            "playerNames": ["Bot", "Alice"],
            "ourPlayerIndex": 0,
            "spectating": false,
            "replay": false,
            "options": {"variantName": "No Variant"},
            "actions": [
                {"type": "draw", "playerIndex": 0, "order": 0, "suitIndex": -1, "rank": -1},
                {"type": "draw", "playerIndex": 0, "order": 1, "suitIndex": -1, "rank": -1},
                {"type": "draw", "playerIndex": 0, "order": 2, "suitIndex": -1, "rank": -1},
                {"type": "draw", "playerIndex": 0, "order": 3, "suitIndex": -1, "rank": -1},
                {"type": "draw", "playerIndex": 0, "order": 4, "suitIndex": -1, "rank": -1},
                {"type": "draw", "playerIndex": 1, "order": 5, "suitIndex": 0, "rank": 1},
                {"type": "draw", "playerIndex": 1, "order": 6, "suitIndex": 0, "rank": 1},
                {"type": "draw", "playerIndex": 1, "order": 7, "suitIndex": 0, "rank": 1},
                {"type": "draw", "playerIndex": 1, "order": 8, "suitIndex": 0, "rank": 2},
                {"type": "draw", "playerIndex": 1, "order": 9, "suitIndex": 0, "rank": 2},
                {
                    "type": "clue",
                    "clue": {"type": 1, "value": 1},
                    "giver": 0,
                    "list": [5, 6, 7],
                    "target": 1,
                    "turn": 0
                },
                {"type": "status", "clues": 7, "score": 0, "maxScore": 25},
                {"type": "turn", "num": 1, "currentPlayerIndex": 1},
                {"type": "play", "playerIndex": 1, "order": 5, "suitIndex": 0, "rank": 1},
                {"type": "draw", "playerIndex": 1, "order": 10, "suitIndex": 2, "rank": 1},
                {"type": "status", "clues": 7, "score": 1, "maxScore": 25},
                {"type": "turn", "num": 2, "currentPlayerIndex": 0}
            ]
        })
        .to_string()
    }

    #[test]
    fn reconstructs_a_player_safe_live_view() {
        let snapshot = HanabiLiveSnapshot::from_json(&snapshot_json()).unwrap();
        let view = snapshot.player_view().unwrap();

        assert_eq!(snapshot.table_id(), 17);
        assert_eq!(view.observer, PlayerId::new(0));
        assert_eq!(view.current_player, PlayerId::new(0));
        assert_eq!(view.turn, 2);
        assert_eq!(view.deck_size, 39);
        assert_eq!(view.clue_tokens, 7);
        assert!(view.hands[0].iter().all(|card| card.identity.is_none()));
        assert_eq!(view.hands[1].last().unwrap().id, CardId::new(10));
        assert_eq!(view.play_stacks[Suit::Red.index()][0].0, CardId::new(5));
        assert_eq!(view.history.len(), 3);
        assert!(matches!(
            &view.history[0].event,
            ObservedEvent::Clued {
                clue: Clue::Rank(Rank::One),
                touched,
                ..
            } if touched == &[CardId::new(5), CardId::new(6), CardId::new(7)]
        ));
        assert!(matches!(
            view.history[1].event,
            ObservedEvent::Played {
                card,
                successful: true,
                ..
            } if card == CardId::new(5)
        ));
        assert!(matches!(
            view.history[2].event,
            ObservedEvent::Drew {
                card,
                identity: Some(Card {
                    suit: Suit::Green,
                    rank: Rank::One
                }),
                ..
            } if card == CardId::new(10)
        ));
    }

    #[test]
    fn live_session_appends_actions_incrementally_and_transactionally() {
        let mut snapshot: serde_json::Value = serde_json::from_str(&snapshot_json()).unwrap();
        let actions = snapshot["actions"].as_array_mut().unwrap();
        let appended = actions.split_off(13);
        let initialize = serde_json::json!({
            "kind": "initialize",
            "snapshot": snapshot,
        });
        let mut session = HanabiLiveSessionState::new();

        let (table_id, first) = session.apply_json(&initialize.to_string()).unwrap();
        assert_eq!(table_id, 17);
        assert_eq!(first.turn, 1);
        assert_eq!(first.current_player, PlayerId::new(1));

        let wrong_table = serde_json::json!({
            "kind": "append",
            "tableID": 99,
            "actions": appended,
        });
        assert!(matches!(
            session.apply_json(&wrong_table.to_string()),
            Err(LiveSnapshotError::SessionTableMismatch {
                expected: 17,
                actual: 99
            })
        ));

        let append = serde_json::json!({
            "kind": "append",
            "tableID": 17,
            "actions": wrong_table["actions"],
        });
        let (_, second) = session.apply_json(&append.to_string()).unwrap();
        assert_eq!(second.turn, 2);
        assert_eq!(second.current_player, PlayerId::new(0));
        assert_eq!(second.play_stacks[Suit::Red.index()].len(), 1);
        assert_eq!(second.hands[1].last().unwrap().id, CardId::new(10));
    }

    #[test]
    fn live_session_requires_initialization() {
        let mut session = HanabiLiveSessionState::new();
        let append = serde_json::json!({
            "kind": "append",
            "tableID": 17,
            "actions": [],
        });
        assert!(matches!(
            session.apply_json(&append.to_string()),
            Err(LiveSnapshotError::SessionNotInitialized)
        ));
    }

    #[test]
    fn engine_actions_map_to_server_wire_values() {
        assert_eq!(
            HanabiLiveActionCommand::from_engine_action(
                9,
                Action::Clue {
                    target: PlayerId::new(1),
                    clue: Clue::Suit(Suit::Purple),
                },
            ),
            HanabiLiveActionCommand {
                table_id: 9,
                action_type: 2,
                target: 1,
                value: Some(4),
            }
        );
        assert_eq!(
            HanabiLiveActionCommand::from_engine_action(9, Action::Play(CardId::new(12))),
            HanabiLiveActionCommand {
                table_id: 9,
                action_type: 0,
                target: 12,
                value: None,
            }
        );
    }
}
