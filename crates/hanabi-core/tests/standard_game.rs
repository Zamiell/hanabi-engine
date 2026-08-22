use hanabi_core::{
    Action, Card, Clue, EndReason, FullState, GameEvent, GameStatus, MAX_CLUE_TOKENS, PlayerId,
    Rank, RuleError, SetupError, Suit, standard_deck,
};

fn card(suit: Suit, rank: Rank) -> Card {
    Card::new(suit, rank)
}

fn deck_with_prefix(prefix: &[Card]) -> Vec<Card> {
    let mut deck = standard_deck();
    for (index, wanted) in prefix.iter().enumerate() {
        let found = deck[index..]
            .iter()
            .position(|candidate| candidate == wanted)
            .map(|offset| index + offset)
            .expect("test prefix must not exceed standard card multiplicities");
        deck.swap(index, found);
    }
    deck
}

fn first_clue(state: &FullState) -> Action {
    state
        .legal_actions()
        .into_iter()
        .find(|action| matches!(action, Action::Clue { .. }))
        .expect("every nonempty visible hand has a legal clue")
}

#[test]
fn validates_setup_and_deals_standard_hand_sizes() {
    assert_eq!(standard_deck().len(), 50);
    assert!(matches!(
        FullState::new_standard(1, standard_deck()),
        Err(SetupError::InvalidPlayerCount(1))
    ));

    for players in 2..=5 {
        let state = FullState::new_standard(players, standard_deck()).unwrap();
        let expected_hand_size = if players <= 3 { 5 } else { 4 };
        assert_eq!(state.hands().len(), usize::from(players));
        assert!(
            state
                .hands()
                .iter()
                .all(|hand| hand.len() == expected_hand_size)
        );
        assert_eq!(
            state.deck_size(),
            50 - usize::from(players) * expected_hand_size
        );
        state.validate().unwrap();
    }
}

#[test]
fn player_view_hides_exactly_the_observers_hand() {
    let state = FullState::new_standard(3, standard_deck()).unwrap();

    for observer_index in 0..3 {
        let observer = PlayerId::new(observer_index);
        let view = state.view_for(observer).unwrap();
        for (player_index, hand) in view.hands.iter().enumerate() {
            for observed in hand {
                assert_eq!(
                    observed.identity.is_none(),
                    player_index == observer.index()
                );
                assert!(observed.clues.positive_suits.is_empty());
                assert!(observed.clues.negative_suits.is_empty());
                assert!(observed.clues.positive_ranks.is_empty());
                assert!(observed.clues.negative_ranks.is_empty());
            }
        }
    }
}

#[test]
fn direct_clue_facts_are_derived_from_history() {
    let prefix = [
        card(Suit::White, Rank::One),
        card(Suit::Yellow, Rank::One),
        card(Suit::Green, Rank::One),
        card(Suit::White, Rank::Two),
        card(Suit::Yellow, Rank::Two),
        card(Suit::Red, Rank::One),
        card(Suit::Red, Rank::Two),
        card(Suit::Blue, Rank::One),
    ];
    let mut state = FullState::new_standard(2, deck_with_prefix(&prefix)).unwrap();
    let target = PlayerId::new(1);
    let target_hand_before = state.hand(target).unwrap().to_vec();

    state
        .apply(Action::Clue {
            target,
            clue: Clue::Suit(Suit::Red),
        })
        .unwrap();

    assert_eq!(state.clue_tokens(), MAX_CLUE_TOKENS - 1);
    let target_view = state.view_for(target).unwrap();
    for observed in &target_view.hands[target.index()] {
        let identity = state.card(observed.id).unwrap();
        if identity.suit == Suit::Red {
            assert_eq!(observed.clues.positive_suits, vec![Suit::Red]);
            assert!(observed.clues.negative_suits.is_empty());
            assert!(!observed.clues.allows(card(Suit::Blue, identity.rank)));
        } else {
            assert!(observed.clues.positive_suits.is_empty());
            assert_eq!(observed.clues.negative_suits, vec![Suit::Red]);
            assert!(!observed.clues.allows(card(Suit::Red, identity.rank)));
        }
        assert!(observed.clues.allows(identity));
    }

    let GameEvent::Clued {
        touched, untouched, ..
    } = &state.history()[0].event
    else {
        panic!("expected a clue event");
    };
    let expected_touched = target_hand_before
        .iter()
        .filter(|id| state.card(**id).unwrap().suit == Suit::Red)
        .count();
    assert_eq!(touched.len(), expected_touched);
    assert_eq!(touched.len() + untouched.len(), target_hand_before.len());
}

#[test]
fn play_and_discard_follow_standard_token_rules() {
    let prefix = [card(Suit::Red, Rank::One), card(Suit::Blue, Rank::Two)];
    let mut state = FullState::new_standard(2, deck_with_prefix(&prefix)).unwrap();
    let first_card = state.hand(PlayerId::new(0)).unwrap()[0];

    assert_eq!(
        state.apply(Action::Discard(first_card)),
        Err(RuleError::DiscardAtMaximumClues)
    );

    let result = state.apply(Action::Play(first_card)).unwrap();
    assert!(result.drawn.is_some());
    assert_eq!(state.score(), 1);
    assert_eq!(state.play_stacks()[Suit::Red.index()], [first_card]);
    assert_eq!(state.hand(PlayerId::new(0)).unwrap().len(), 5);
    assert_eq!(state.current_player(), PlayerId::new(1));

    state.apply(first_clue(&state)).unwrap();
    assert_eq!(state.clue_tokens(), MAX_CLUE_TOKENS - 1);
    let discard = state.hand(PlayerId::new(0)).unwrap()[0];
    state.apply(Action::Discard(discard)).unwrap();
    assert_eq!(state.clue_tokens(), MAX_CLUE_TOKENS);
    assert!(state.discard_pile().contains(&discard));
    state.validate().unwrap();
}

#[test]
fn misplay_adds_a_strike_and_publicly_reveals_the_card() {
    let prefix = [card(Suit::Red, Rank::Two)];
    let mut state = FullState::new_standard(2, deck_with_prefix(&prefix)).unwrap();
    let played = state.hand(PlayerId::new(0)).unwrap()[0];

    state.apply(Action::Play(played)).unwrap();

    assert_eq!(state.strikes(), 1);
    assert_eq!(state.score(), 0);
    assert!(state.discard_pile().contains(&played));
    let view = state.view_for(PlayerId::new(0)).unwrap();
    assert!(view.history.iter().any(|entry| matches!(
        entry.event,
        hanabi_core::ObservedEvent::Played {
            card: event_card,
            identity: Card {
                suit: Suit::Red,
                rank: Rank::Two,
            },
            successful: false,
            ..
        } if event_card == played
    )));
}

#[test]
fn a_player_cannot_observe_the_identity_of_their_own_draw() {
    let prefix = [card(Suit::Red, Rank::One)];
    let mut state = FullState::new_standard(2, deck_with_prefix(&prefix)).unwrap();
    let played = state.hand(PlayerId::new(0)).unwrap()[0];
    let drawn = state.apply(Action::Play(played)).unwrap().drawn.unwrap();

    let own_view = state.view_for(PlayerId::new(0)).unwrap();
    assert!(own_view.history.iter().any(|entry| matches!(
        entry.event,
        hanabi_core::ObservedEvent::Drew {
            card,
            identity: None,
            ..
        } if card == drawn
    )));

    let teammate_view = state.view_for(PlayerId::new(1)).unwrap();
    assert!(teammate_view.history.iter().any(|entry| matches!(
        entry.event,
        hanabi_core::ObservedEvent::Drew {
            card,
            identity: Some(_),
            ..
        } if card == drawn
    )));
}

#[test]
fn third_misplay_ends_the_game_without_drawing() {
    let prefix = [
        card(Suit::Red, Rank::Two),
        card(Suit::Green, Rank::Two),
        card(Suit::Yellow, Rank::Two),
        card(Suit::White, Rank::Two),
        card(Suit::Red, Rank::Three),
        card(Suit::Blue, Rank::Two),
    ];
    let mut state = FullState::new_standard(2, deck_with_prefix(&prefix)).unwrap();
    let first = state.hand(PlayerId::new(0)).unwrap()[0];
    let second = state.hand(PlayerId::new(1)).unwrap()[0];
    let third = state.hand(PlayerId::new(0)).unwrap()[1];

    state.apply(Action::Play(first)).unwrap();
    state.apply(Action::Play(second)).unwrap();
    let result = state.apply(Action::Play(third)).unwrap();

    assert_eq!(result.drawn, None);
    assert_eq!(
        state.status(),
        GameStatus::Finished(EndReason::TooManyStrikes)
    );
    assert_eq!(state.final_score(), Some(0));
    assert!(state.legal_actions().is_empty());
    assert_eq!(
        state.apply(Action::Play(state.hand(PlayerId::new(0)).unwrap()[0])),
        Err(RuleError::GameAlreadyFinished)
    );
    state.validate().unwrap();
}

#[test]
fn completing_a_stack_restores_a_spent_clue_token() {
    // P0 clues. Then P1, P0, P1, P0, P1 play red 1 through red 5.
    let prefix = [
        card(Suit::Red, Rank::Two),
        card(Suit::Red, Rank::Four),
        card(Suit::Blue, Rank::One),
        card(Suit::Green, Rank::One),
        card(Suit::White, Rank::One),
        card(Suit::Red, Rank::One),
        card(Suit::Red, Rank::Three),
        card(Suit::Red, Rank::Five),
    ];
    let mut state = FullState::new_standard(2, deck_with_prefix(&prefix)).unwrap();
    let red_cards = [
        state.hand(PlayerId::new(1)).unwrap()[0],
        state.hand(PlayerId::new(0)).unwrap()[0],
        state.hand(PlayerId::new(1)).unwrap()[1],
        state.hand(PlayerId::new(0)).unwrap()[1],
        state.hand(PlayerId::new(1)).unwrap()[2],
    ];

    state
        .apply(Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Suit(Suit::Red),
        })
        .unwrap();
    assert_eq!(state.clue_tokens(), 7);

    for card in red_cards {
        state.apply(Action::Play(card)).unwrap();
    }

    assert_eq!(state.score(), 5);
    assert_eq!(state.clue_tokens(), 8);
    state.validate().unwrap();
}

#[test]
fn final_round_grants_every_player_one_more_action() {
    let mut state = FullState::new_standard(3, standard_deck()).unwrap();

    while state.deck_size() > 0 {
        let action = state
            .legal_actions()
            .into_iter()
            .find(|action| matches!(action, Action::Discard(_)))
            .unwrap_or_else(|| first_clue(&state));
        state.apply(action).unwrap();
    }

    assert_eq!(state.final_turns_remaining(), Some(3));
    let turn_after_last_draw = state.turn();
    while !state.is_terminal() {
        let action = state
            .legal_actions()
            .into_iter()
            .find(|action| matches!(action, Action::Discard(_)))
            .unwrap_or_else(|| first_clue(&state));
        state.apply(action).unwrap();
    }

    assert_eq!(state.turn() - turn_after_last_draw, 3);
    assert_eq!(
        state.status(),
        GameStatus::Finished(EndReason::FinalRoundComplete)
    );
    assert_eq!(state.final_turns_remaining(), Some(0));
    assert!(state.legal_actions().is_empty());
    state.validate().unwrap();
}

#[test]
fn cloned_states_replay_deterministically() {
    let mut left = FullState::new_standard(2, standard_deck()).unwrap();
    let mut right = left.clone();

    for _ in 0..20 {
        if left.is_terminal() {
            break;
        }
        let action = left.legal_actions()[0];
        assert_eq!(left.apply(action), right.apply(action));
        assert_eq!(left, right);
    }
}

#[test]
fn many_random_legal_games_preserve_invariants() {
    for seed in 1..=64_u64 {
        let mut rng = TestRng(seed);
        let mut deck = standard_deck();
        for upper in (1..deck.len()).rev() {
            let lower = rng.index(upper + 1);
            deck.swap(upper, lower);
        }

        let players = 2 + u8::try_from(seed % 4).unwrap();
        let mut state = FullState::new_standard(players, deck).unwrap();
        for _ in 0..1_000 {
            state.validate().unwrap();
            if state.is_terminal() {
                break;
            }
            let actions = state.legal_actions();
            let action = actions[rng.index(actions.len())];
            state.apply(action).unwrap();
        }
        assert!(state.is_terminal(), "seed {seed} did not finish");
        state.validate().unwrap();
    }
}

struct TestRng(u64);

impl TestRng {
    fn index(&mut self, upper: usize) -> usize {
        self.0 = self
            .0
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        usize::try_from(self.0 % u64::try_from(upper).unwrap()).unwrap()
    }
}
