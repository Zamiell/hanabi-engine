use core::fmt;
use std::collections::VecDeque;

use crate::{
    Action, Card, CardId, Clue, ClueFacts, ObservedEvent, ObservedHistoryEntry, PlayerId,
    PlayerView, Rank, Suit,
};

pub const MAX_CLUE_TOKENS: u8 = 8;
pub const MAX_STRIKES: u8 = 3;
const MIN_PLAYERS: u8 = 2;
const MAX_PLAYERS: u8 = 5;
const STANDARD_DECK_SIZE: usize = 50;

/// Why a game ended.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum EndReason {
    PerfectScore,
    TooManyStrikes,
    FinalRoundComplete,
}

/// Whether actions may still be applied to a game.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum GameStatus {
    InProgress,
    Finished(EndReason),
}

/// An authoritative event. Unlike an observed event, a draw can be resolved
/// through the state's private card table.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum GameEvent {
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
        successful: bool,
    },
    Discarded {
        player: PlayerId,
        card: CardId,
    },
    Drew {
        player: PlayerId,
        card: CardId,
    },
}

/// One event associated with a game turn.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HistoryEntry {
    pub turn: u32,
    pub event: GameEvent,
}

/// The result of applying one player action.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TurnResult {
    pub actor: PlayerId,
    pub action: Action,
    pub drawn: Option<CardId>,
    pub status: GameStatus,
}

/// Complete simulator truth for a standard game.
///
/// Policies and planning algorithms should use [`crate::PlayerView`] rather than
/// this type. A `FullState` contains every hidden identity and the exact deck
/// order.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FullState {
    cards: Vec<Card>,
    draw_pile: VecDeque<CardId>,
    hands: Vec<Vec<CardId>>,
    play_stacks: [Vec<CardId>; 5],
    discard_pile: Vec<CardId>,
    clue_tokens: u8,
    strikes: u8,
    current_player: PlayerId,
    turn: u32,
    final_turns_remaining: Option<u8>,
    status: GameStatus,
    history: Vec<HistoryEntry>,
    clue_facts: Vec<ClueFacts>,
}

/// Reusable public-state structure for constructing multiple worlds
/// of the same [`PlayerView`].
///
/// Building a template validates card locations and reconstructs authoritative
/// history and clue facts once. Each instantiation still validates the supplied
/// standard deck and every visible identity and direct clue constraint.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WorldTemplate {
    hands: Vec<Vec<CardId>>,
    play_stacks: [Vec<CardId>; 5],
    discard_pile: Vec<CardId>,
    draw_pile: VecDeque<CardId>,
    observed_cards: Vec<(CardId, Option<Card>, ClueFacts)>,
    observed_history: Vec<ObservedHistoryEntry>,
    clue_tokens: u8,
    strikes: u8,
    current_player: PlayerId,
    turn: u32,
    final_turns_remaining: Option<u8>,
    status: GameStatus,
    history: Vec<HistoryEntry>,
    clue_facts: Vec<ClueFacts>,
}

impl FullState {
    /// Creates a dealt standard game from a complete, ordered 50-card deck.
    /// The first card in `deck` is drawn first.
    ///
    /// # Errors
    ///
    /// Returns [`SetupError`] when the player count or standard-deck
    /// multiplicities are invalid.
    ///
    /// # Panics
    ///
    /// Panics only if validated standard-deck constants cannot satisfy the
    /// initial deal, which would indicate an internal rules bug.
    pub fn new_standard(num_players: u8, deck: Vec<Card>) -> Result<Self, SetupError> {
        if !(MIN_PLAYERS..=MAX_PLAYERS).contains(&num_players) {
            return Err(SetupError::InvalidPlayerCount(num_players));
        }
        validate_standard_deck(&deck)?;

        let cards = deck;
        let mut draw_pile = (0..cards.len()).map(CardId::new).collect::<VecDeque<_>>();
        let mut hands = vec![Vec::with_capacity(hand_size(num_players)); num_players.into()];

        // Hanabi Live assigns card orders by dealing a complete hand to each
        // player in index order rather than dealing round-robin.
        for player in 0..num_players {
            for _ in 0..hand_size(num_players) {
                let card = draw_pile
                    .pop_front()
                    .expect("a validated standard deck contains enough cards for the initial deal");
                hands[usize::from(player)].push(card);
            }
        }

        let clue_facts = vec![ClueFacts::default(); cards.len()];
        let state = Self {
            cards,
            draw_pile,
            hands,
            play_stacks: std::array::from_fn(|_| Vec::with_capacity(5)),
            discard_pile: Vec::new(),
            clue_tokens: MAX_CLUE_TOKENS,
            strikes: 0,
            current_player: PlayerId::new(0),
            turn: 0,
            final_turns_remaining: None,
            status: GameStatus::InProgress,
            history: Vec::new(),
            clue_facts,
        };

        debug_assert!(state.validate().is_ok());
        Ok(state)
    }

    #[must_use]
    /// # Panics
    ///
    /// Panics only if an internally constructed state exceeds five players.
    pub fn num_players(&self) -> u8 {
        self.hands
            .len()
            .try_into()
            .expect("standard Hanabi has at most five players")
    }

    #[must_use]
    pub const fn current_player(&self) -> PlayerId {
        self.current_player
    }

    #[must_use]
    pub const fn turn(&self) -> u32 {
        self.turn
    }

    #[must_use]
    pub const fn status(&self) -> GameStatus {
        self.status
    }

    #[must_use]
    pub const fn is_terminal(&self) -> bool {
        matches!(self.status, GameStatus::Finished(_))
    }

    #[must_use]
    pub const fn clue_tokens(&self) -> u8 {
        self.clue_tokens
    }

    #[must_use]
    pub const fn strikes(&self) -> u8 {
        self.strikes
    }

    #[must_use]
    pub const fn final_turns_remaining(&self) -> Option<u8> {
        self.final_turns_remaining
    }

    #[must_use]
    pub fn deck_size(&self) -> usize {
        self.draw_pile.len()
    }

    #[must_use]
    pub fn hand(&self, player: PlayerId) -> Option<&[CardId]> {
        self.hands.get(player.index()).map(Vec::as_slice)
    }

    #[must_use]
    pub fn hands(&self) -> &[Vec<CardId>] {
        &self.hands
    }

    #[must_use]
    pub fn card(&self, id: CardId) -> Option<Card> {
        self.cards.get(id.index()).copied()
    }

    #[must_use]
    pub const fn play_stacks(&self) -> &[Vec<CardId>; 5] {
        &self.play_stacks
    }

    #[must_use]
    pub fn discard_pile(&self) -> &[CardId] {
        &self.discard_pile
    }

    #[must_use]
    pub fn history(&self) -> &[HistoryEntry] {
        &self.history
    }

    #[must_use]
    /// # Panics
    ///
    /// Panics only if an internally constructed standard stack exceeds five
    /// cards.
    pub fn score(&self) -> u8 {
        self.play_stacks
            .iter()
            .map(|stack| {
                u8::try_from(stack.len()).expect("a standard stack has at most five cards")
            })
            .sum()
    }

    /// Returns the official final score, where three strikes score zero.
    #[must_use]
    pub fn final_score(&self) -> Option<u8> {
        match self.status {
            GameStatus::InProgress => None,
            GameStatus::Finished(EndReason::TooManyStrikes) => Some(0),
            GameStatus::Finished(_) => Some(self.score()),
        }
    }

    /// Legal actions in deterministic order for the current player.
    ///
    /// # Panics
    ///
    /// Panics only if the state's current-player invariant is broken.
    #[must_use]
    pub fn legal_actions(&self) -> Vec<Action> {
        let mut actions = Vec::with_capacity(50);
        self.legal_actions_into(&mut actions);
        actions
    }

    /// Replaces `actions` with the legal actions in deterministic order while
    /// retaining its allocation for reuse by planner traversals.
    ///
    /// # Panics
    ///
    /// Panics only if the state's current-player invariant is broken.
    pub fn legal_actions_into(&self, actions: &mut Vec<Action>) {
        actions.clear();
        if self.is_terminal() {
            return;
        }

        let own_hand = &self.hands[self.current_player.index()];
        actions.reserve(own_hand.len() * 2 + 40);
        actions.extend(own_hand.iter().copied().map(Action::Play));
        if self.clue_tokens < MAX_CLUE_TOKENS {
            actions.extend(own_hand.iter().copied().map(Action::Discard));
        }
        if self.clue_tokens > 0 {
            for (target_index, target_hand) in self.hands.iter().enumerate() {
                if target_index == self.current_player.index() {
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
                        .any(|card| self.cards[card.index()].suit == suit)
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
                        .any(|card| self.cards[card.index()].rank == rank)
                    {
                        actions.push(Action::Clue {
                            target,
                            clue: Clue::Rank(rank),
                        });
                    }
                }
            }
        }
    }

    /// Applies one complete player action and any resulting draw.
    ///
    /// # Errors
    ///
    /// Returns [`RuleError`] when `action` is illegal in the current state.
    ///
    /// # Panics
    ///
    /// Panics only if a previously validated state violates an internal
    /// location or final-round invariant.
    pub fn apply(&mut self, action: Action) -> Result<TurnResult, RuleError> {
        self.validate_action(action)?;

        let actor = self.current_player;
        let deck_was_empty = self.draw_pile.is_empty();
        let mut drawn = None;

        match action {
            Action::Clue { target, clue } => self.apply_clue(actor, target, clue),
            Action::Play(card) => {
                self.remove_from_current_hand(card);
                let identity = self.cards[card.index()];
                let expected_rank = self.play_stacks[identity.suit.index()].len() + 1;
                let successful = expected_rank == usize::from(identity.rank.number());

                if successful {
                    self.play_stacks[identity.suit.index()].push(card);
                    if identity.rank == Rank::Five {
                        self.clue_tokens = self.clue_tokens.saturating_add(1).min(MAX_CLUE_TOKENS);
                    }
                } else {
                    self.discard_pile.push(card);
                    self.strikes += 1;
                }

                self.push_event(GameEvent::Played {
                    player: actor,
                    card,
                    successful,
                });

                if self.strikes == MAX_STRIKES {
                    self.status = GameStatus::Finished(EndReason::TooManyStrikes);
                } else if self.score() == 25 {
                    self.status = GameStatus::Finished(EndReason::PerfectScore);
                } else {
                    drawn = self.draw_for(actor);
                }
            }
            Action::Discard(card) => {
                self.remove_from_current_hand(card);
                self.discard_pile.push(card);
                self.clue_tokens += 1;
                self.push_event(GameEvent::Discarded {
                    player: actor,
                    card,
                });
                drawn = self.draw_for(actor);
            }
        }

        if self.status == GameStatus::InProgress && deck_was_empty {
            let remaining = self
                .final_turns_remaining
                .as_mut()
                .expect("an empty deck starts the final round");
            *remaining -= 1;
            if *remaining == 0 {
                self.status = GameStatus::Finished(EndReason::FinalRoundComplete);
            }
        }

        self.turn += 1;
        if self.status == GameStatus::InProgress {
            let next = (actor.index() + 1) % self.hands.len();
            self.current_player = PlayerId::new(
                next.try_into()
                    .expect("standard Hanabi has at most five players"),
            );
        }

        debug_assert!(self.validate().is_ok());
        Ok(TurnResult {
            actor,
            action,
            drawn,
            status: self.status,
        })
    }

    /// Checks structural and standard-rule invariants. This is deliberately
    /// public so simulations and property tests can validate sampled states.
    ///
    /// # Errors
    ///
    /// Returns [`InvariantViolation`] describing the first broken invariant.
    pub fn validate(&self) -> Result<(), InvariantViolation> {
        if !(usize::from(MIN_PLAYERS)..=usize::from(MAX_PLAYERS)).contains(&self.hands.len()) {
            return Err(InvariantViolation("player count is outside 2..=5".into()));
        }
        if self.cards.len() != 50 {
            return Err(InvariantViolation(
                "a standard game must contain 50 cards".into(),
            ));
        }
        if self.clue_facts != derive_clue_facts(self.cards.len(), &self.history) {
            return Err(InvariantViolation(
                "cached clue facts disagree with authoritative history".into(),
            ));
        }
        if self.current_player.index() >= self.hands.len() {
            return Err(InvariantViolation("current player is out of range".into()));
        }
        if self.clue_tokens > MAX_CLUE_TOKENS {
            return Err(InvariantViolation("clue-token count exceeds eight".into()));
        }
        if self.strikes > MAX_STRIKES {
            return Err(InvariantViolation("strike count exceeds three".into()));
        }
        if self.final_turns_remaining.is_some() != self.draw_pile.is_empty() {
            return Err(InvariantViolation(
                "final-round counter must exist exactly when the deck is empty".into(),
            ));
        }
        if let Some(remaining) = self.final_turns_remaining {
            if remaining > self.num_players() {
                return Err(InvariantViolation(
                    "final-round counter is too large".into(),
                ));
            }
            if self.status == GameStatus::InProgress && remaining == 0 {
                return Err(InvariantViolation(
                    "an in-progress final round must have a remaining turn".into(),
                ));
            }
        }

        let mut locations = vec![0_u8; self.cards.len()];
        for id in self
            .draw_pile
            .iter()
            .chain(self.hands.iter().flatten())
            .chain(self.play_stacks.iter().flatten())
            .chain(&self.discard_pile)
        {
            let Some(count) = locations.get_mut(id.index()) else {
                return Err(InvariantViolation(format!("unknown card identifier {id}")));
            };
            *count += 1;
        }
        if locations.iter().any(|count| *count != 1) {
            return Err(InvariantViolation(
                "every card must occur in exactly one location".into(),
            ));
        }

        for suit in Suit::ALL {
            let stack = &self.play_stacks[suit.index()];
            if stack.len() > 5 {
                return Err(InvariantViolation(format!(
                    "{suit} stack exceeds rank five"
                )));
            }
            for (index, id) in stack.iter().enumerate() {
                let card = self.cards[id.index()];
                if card.suit != suit || usize::from(card.rank.number()) != index + 1 {
                    return Err(InvariantViolation(format!(
                        "{suit} stack is not an ordered sequence"
                    )));
                }
            }
        }

        match self.status {
            GameStatus::Finished(EndReason::PerfectScore) if self.score() != 25 => {
                return Err(InvariantViolation(
                    "perfect-score ending requires a score of 25".into(),
                ));
            }
            GameStatus::Finished(EndReason::TooManyStrikes) if self.strikes != MAX_STRIKES => {
                return Err(InvariantViolation(
                    "strike ending requires exactly three strikes".into(),
                ));
            }
            GameStatus::Finished(EndReason::FinalRoundComplete)
                if self.final_turns_remaining != Some(0) =>
            {
                return Err(InvariantViolation(
                    "final-round ending requires zero remaining turns".into(),
                ));
            }
            _ => {}
        }

        Ok(())
    }

    fn validate_action(&self, action: Action) -> Result<(), RuleError> {
        if self.is_terminal() {
            return Err(RuleError::GameAlreadyFinished);
        }
        match action {
            Action::Play(card) => self.require_current_players_card(card),
            Action::Discard(card) => {
                self.require_current_players_card(card)?;
                if self.clue_tokens == MAX_CLUE_TOKENS {
                    Err(RuleError::DiscardAtMaximumClues)
                } else {
                    Ok(())
                }
            }
            Action::Clue { target, clue } => {
                if self.clue_tokens == 0 {
                    return Err(RuleError::NoClueTokens);
                }
                if target.index() >= self.hands.len() {
                    return Err(RuleError::InvalidPlayer(target));
                }
                if target == self.current_player {
                    return Err(RuleError::CannotClueSelf);
                }
                let touches = self.hands[target.index()]
                    .iter()
                    .any(|id| clue.matches(self.cards[id.index()]));
                if !touches {
                    return Err(RuleError::ClueTouchesNoCards);
                }
                Ok(())
            }
        }
    }

    fn require_current_players_card(&self, card: CardId) -> Result<(), RuleError> {
        if self.hands[self.current_player.index()].contains(&card) {
            Ok(())
        } else {
            Err(RuleError::CardNotInCurrentHand(card))
        }
    }

    fn remove_from_current_hand(&mut self, card: CardId) {
        let hand = &mut self.hands[self.current_player.index()];
        let index = hand
            .iter()
            .position(|candidate| *candidate == card)
            .expect("the action was validated before mutation");
        hand.remove(index);
    }

    fn apply_clue(&mut self, giver: PlayerId, target: PlayerId, clue: Clue) {
        self.clue_tokens -= 1;
        let (touched, untouched): (Vec<CardId>, Vec<CardId>) = self.hands[target.index()]
            .iter()
            .copied()
            .partition(|id| clue.matches(self.cards[id.index()]));
        for card in &touched {
            self.clue_facts[card.index()].record(clue, true);
        }
        for card in &untouched {
            self.clue_facts[card.index()].record(clue, false);
        }
        self.push_event(GameEvent::Clued {
            giver,
            target,
            clue,
            touched,
            untouched,
        });
    }

    pub(crate) fn clue_facts(&self, card: CardId) -> &ClueFacts {
        &self.clue_facts[card.index()]
    }

    fn draw_for(&mut self, player: PlayerId) -> Option<CardId> {
        let card = self.draw_pile.pop_front()?;
        self.hands[player.index()].push(card);
        self.push_event(GameEvent::Drew { player, card });
        if self.draw_pile.is_empty() {
            self.final_turns_remaining = Some(self.num_players());
        }
        Some(card)
    }

    fn push_event(&mut self, event: GameEvent) {
        self.history.push(HistoryEntry {
            turn: self.turn,
            event,
        });
    }
}

impl WorldTemplate {
    /// Validates and compiles the public structure shared by all hidden worlds
    /// represented by `view`.
    ///
    /// # Errors
    ///
    /// Returns [`WorldConstructionError`] for invalid or duplicate card
    /// locations, a mismatched deck size, or an invalid public rules state.
    pub fn new(view: &PlayerView) -> Result<Self, WorldConstructionError> {
        let mut used = [false; STANDARD_DECK_SIZE];
        let mut observed_cards = Vec::new();
        let mut hands = Vec::with_capacity(view.hands.len());
        for hand in &view.hands {
            let mut ids = Vec::with_capacity(hand.len());
            for observed in hand {
                occupy(&mut used, observed.id)?;
                observed_cards.push((observed.id, observed.identity, observed.clues));
                ids.push(observed.id);
            }
            hands.push(ids);
        }

        let mut play_stacks = std::array::from_fn(|_| Vec::new());
        for (suit_index, observed_stack) in view.play_stacks.iter().enumerate() {
            for (id, identity) in observed_stack {
                occupy(&mut used, *id)?;
                observed_cards.push((*id, Some(*identity), ClueFacts::default()));
                play_stacks[suit_index].push(*id);
            }
        }

        let mut discard_pile = Vec::with_capacity(view.discard_pile.len());
        for (id, identity) in &view.discard_pile {
            occupy(&mut used, *id)?;
            observed_cards.push((*id, Some(*identity), ClueFacts::default()));
            discard_pile.push(*id);
        }

        let draw_pile = used
            .iter()
            .enumerate()
            .filter_map(|(index, occupied)| (!occupied).then_some(CardId::new(index)))
            .collect::<VecDeque<_>>();
        if draw_pile.len() != view.deck_size {
            return Err(WorldConstructionError::DeckSizeMismatch {
                observed: view.deck_size,
                reconstructed: draw_pile.len(),
            });
        }

        validate_history_card_ids(&view.history, STANDARD_DECK_SIZE)?;
        let history = reconstruct_history(&view.history);
        let clue_facts = derive_clue_facts(STANDARD_DECK_SIZE, &history);
        let template = Self {
            hands,
            play_stacks,
            discard_pile,
            draw_pile,
            observed_cards,
            observed_history: view.history.clone(),
            clue_tokens: view.clue_tokens,
            strikes: view.strikes,
            current_player: view.current_player,
            turn: view.turn,
            final_turns_remaining: view.final_turns_remaining,
            status: view.status,
            history,
            clue_facts,
        };

        // Known stack identities are sufficient for validating every
        // card-dependent structural invariant. All other placeholder
        // identities are ignored by `FullState::validate`.
        let mut placeholder_cards = vec![Card::new(Suit::Red, Rank::One); STANDARD_DECK_SIZE];
        for (id, identity, _) in &template.observed_cards {
            if let Some(identity) = identity {
                placeholder_cards[id.index()] = *identity;
            }
        }
        template
            .build_state(placeholder_cards)
            .validate()
            .map_err(WorldConstructionError::InvalidState)?;
        Ok(template)
    }

    /// Creates a complete world using the template's already-validated public
    /// structure.
    ///
    /// # Errors
    ///
    /// Returns [`WorldConstructionError`] if `cards` is not a standard deck or
    /// conflicts with a visible identity, direct clue, or observed history.
    pub fn instantiate(&self, cards: Vec<Card>) -> Result<FullState, WorldConstructionError> {
        validate_standard_deck(&cards).map_err(WorldConstructionError::InvalidDeck)?;
        for (id, identity, clues) in &self.observed_cards {
            let supplied = identity_at(&cards, *id)?;
            if let Some(identity) = identity {
                require_identity(*id, *identity, supplied)?;
            } else if !clues.allows(supplied) {
                return Err(WorldConstructionError::ViolatesClues {
                    card: *id,
                    supplied,
                });
            }
        }
        validate_history_identities(&self.observed_history, &cards)?;
        Ok(self.build_state(cards))
    }

    fn build_state(&self, cards: Vec<Card>) -> FullState {
        FullState {
            cards,
            draw_pile: self.draw_pile.clone(),
            hands: self.hands.clone(),
            play_stacks: self.play_stacks.clone(),
            discard_pile: self.discard_pile.clone(),
            clue_tokens: self.clue_tokens,
            strikes: self.strikes,
            current_player: self.current_player,
            turn: self.turn,
            final_turns_remaining: self.final_turns_remaining,
            status: self.status,
            history: self.history.clone(),
            clue_facts: self.clue_facts.clone(),
        }
    }
}

#[must_use]
const fn hand_size(num_players: u8) -> usize {
    if num_players <= 3 { 5 } else { 4 }
}

fn validate_standard_deck(deck: &[Card]) -> Result<(), SetupError> {
    if deck.len() != STANDARD_DECK_SIZE {
        return Err(SetupError::InvalidDeckSize {
            expected: STANDARD_DECK_SIZE,
            actual: deck.len(),
        });
    }

    let mut counts = [[0_u8; 5]; 5];
    for card in deck {
        counts[card.suit.index()][card.rank.index()] += 1;
    }
    for suit in Suit::ALL {
        for rank in Rank::ALL {
            let actual = counts[suit.index()][rank.index()];
            if actual != rank.copies() {
                return Err(SetupError::InvalidCardMultiplicity {
                    card: Card::new(suit, rank),
                    expected: rank.copies(),
                    actual,
                });
            }
        }
    }
    Ok(())
}

fn occupy(used: &mut [bool], card: CardId) -> Result<(), WorldConstructionError> {
    let Some(occupied) = used.get_mut(card.index()) else {
        return Err(WorldConstructionError::InvalidCardId(card));
    };
    if *occupied {
        return Err(WorldConstructionError::DuplicateLocation(card));
    }
    *occupied = true;
    Ok(())
}

fn require_identity(
    card: CardId,
    observed: Card,
    supplied: Card,
) -> Result<(), WorldConstructionError> {
    if observed == supplied {
        Ok(())
    } else {
        Err(WorldConstructionError::ConflictingIdentity {
            card,
            observed,
            supplied,
        })
    }
}

fn identity_at(cards: &[Card], card: CardId) -> Result<Card, WorldConstructionError> {
    cards
        .get(card.index())
        .copied()
        .ok_or(WorldConstructionError::InvalidCardId(card))
}

fn validate_history_identities(
    history: &[ObservedHistoryEntry],
    cards: &[Card],
) -> Result<(), WorldConstructionError> {
    for entry in history {
        match &entry.event {
            ObservedEvent::Played { card, identity, .. }
            | ObservedEvent::Discarded { card, identity, .. } => {
                require_identity(*card, *identity, identity_at(cards, *card)?)?;
            }
            ObservedEvent::Drew {
                card,
                identity: Some(identity),
                ..
            } => require_identity(*card, *identity, identity_at(cards, *card)?)?,
            ObservedEvent::Clued { .. } | ObservedEvent::Drew { identity: None, .. } => {}
        }
    }
    Ok(())
}

fn validate_history_card_ids(
    history: &[ObservedHistoryEntry],
    card_count: usize,
) -> Result<(), WorldConstructionError> {
    for entry in history {
        match &entry.event {
            ObservedEvent::Clued {
                touched, untouched, ..
            } => {
                for card in touched.iter().chain(untouched) {
                    if card.index() >= card_count {
                        return Err(WorldConstructionError::InvalidCardId(*card));
                    }
                }
            }
            ObservedEvent::Played { card, .. }
            | ObservedEvent::Discarded { card, .. }
            | ObservedEvent::Drew { card, .. } => {
                if card.index() >= card_count {
                    return Err(WorldConstructionError::InvalidCardId(*card));
                }
            }
        }
    }
    Ok(())
}

fn reconstruct_history(history: &[ObservedHistoryEntry]) -> Vec<HistoryEntry> {
    history
        .iter()
        .map(|entry| HistoryEntry {
            turn: entry.turn,
            event: match &entry.event {
                ObservedEvent::Clued {
                    giver,
                    target,
                    clue,
                    touched,
                    untouched,
                } => GameEvent::Clued {
                    giver: *giver,
                    target: *target,
                    clue: *clue,
                    touched: touched.clone(),
                    untouched: untouched.clone(),
                },
                ObservedEvent::Played {
                    player,
                    card,
                    successful,
                    ..
                } => GameEvent::Played {
                    player: *player,
                    card: *card,
                    successful: *successful,
                },
                ObservedEvent::Discarded { player, card, .. } => GameEvent::Discarded {
                    player: *player,
                    card: *card,
                },
                ObservedEvent::Drew { player, card, .. } => GameEvent::Drew {
                    player: *player,
                    card: *card,
                },
            },
        })
        .collect()
}

fn derive_clue_facts(card_count: usize, history: &[HistoryEntry]) -> Vec<ClueFacts> {
    let mut facts = vec![ClueFacts::default(); card_count];
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
        for card in touched {
            facts[card.index()].record(*clue, true);
        }
        for card in untouched {
            facts[card.index()].record(*clue, false);
        }
    }
    facts
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SetupError {
    InvalidPlayerCount(u8),
    InvalidDeckSize {
        expected: usize,
        actual: usize,
    },
    InvalidCardMultiplicity {
        card: Card,
        expected: u8,
        actual: u8,
    },
}

impl fmt::Display for SetupError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidPlayerCount(count) => {
                write!(
                    formatter,
                    "standard Hanabi requires 2 to 5 players, got {count}"
                )
            }
            Self::InvalidDeckSize { expected, actual } => {
                write!(formatter, "expected a {expected}-card deck, got {actual}")
            }
            Self::InvalidCardMultiplicity {
                card,
                expected,
                actual,
            } => write!(
                formatter,
                "expected {expected} copies of {card}, got {actual}"
            ),
        }
    }
}

impl std::error::Error for SetupError {}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum WorldConstructionError {
    InvalidDeck(SetupError),
    InvalidCardId(CardId),
    DuplicateLocation(CardId),
    ConflictingIdentity {
        card: CardId,
        observed: Card,
        supplied: Card,
    },
    ViolatesClues {
        card: CardId,
        supplied: Card,
    },
    DeckSizeMismatch {
        observed: usize,
        reconstructed: usize,
    },
    InvalidState(InvariantViolation),
}

impl fmt::Display for WorldConstructionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidDeck(error) => write!(formatter, "invalid sampled deck: {error}"),
            Self::InvalidCardId(card) => write!(formatter, "unknown card identifier {card}"),
            Self::DuplicateLocation(card) => {
                write!(formatter, "card {card} occurs in more than one location")
            }
            Self::ConflictingIdentity {
                card,
                observed,
                supplied,
            } => write!(
                formatter,
                "sampled identity {supplied} for {card} conflicts with observed {observed}"
            ),
            Self::ViolatesClues { card, supplied } => {
                write!(
                    formatter,
                    "sampled identity {supplied} for {card} violates its clues"
                )
            }
            Self::DeckSizeMismatch {
                observed,
                reconstructed,
            } => write!(
                formatter,
                "observed deck has {observed} cards but reconstruction found {reconstructed}"
            ),
            Self::InvalidState(error) => write!(formatter, "sampled state is invalid: {error}"),
        }
    }
}

impl std::error::Error for WorldConstructionError {}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RuleError {
    GameAlreadyFinished,
    CardNotInCurrentHand(CardId),
    DiscardAtMaximumClues,
    NoClueTokens,
    InvalidPlayer(PlayerId),
    CannotClueSelf,
    ClueTouchesNoCards,
}

impl fmt::Display for RuleError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::GameAlreadyFinished => formatter.write_str("the game is already finished"),
            Self::CardNotInCurrentHand(card) => {
                write!(formatter, "{card} is not in the current player's hand")
            }
            Self::DiscardAtMaximumClues => {
                formatter.write_str("discarding is illegal at eight clue tokens")
            }
            Self::NoClueTokens => formatter.write_str("giving a clue requires a clue token"),
            Self::InvalidPlayer(player) => write!(formatter, "invalid target player {player}"),
            Self::CannotClueSelf => formatter.write_str("a player cannot clue their own hand"),
            Self::ClueTouchesNoCards => formatter.write_str("a clue must touch at least one card"),
        }
    }
}

impl std::error::Error for RuleError {}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InvariantViolation(pub String);

impl fmt::Display for InvariantViolation {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl std::error::Error for InvariantViolation {}
