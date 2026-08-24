use hanabi_core::{Action, Card, Clue, FullState, PlayerId, Rank, Suit, standard_deck};
use hanabi_search::{ConventionAgnosticPolicy, InformationSet, assess_card};

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

fn select(state: &FullState) -> Action {
    let actor = state.current_player();
    let view = state.view_for(actor).unwrap();
    let information_set = InformationSet::new(&view).unwrap();
    ConventionAgnosticPolicy
        .select_action(information_set.deductions())
        .unwrap()
}

#[test]
fn plays_a_card_that_direct_information_proves_is_playable() {
    let prefix = [
        card(Suit::Red, Rank::One),
        card(Suit::Blue, Rank::Two),
        card(Suit::Green, Rank::Three),
        card(Suit::Yellow, Rank::Four),
        card(Suit::Purple, Rank::Five),
        card(Suit::Red, Rank::Five),
        card(Suit::Blue, Rank::Five),
        card(Suit::Green, Rank::Five),
        card(Suit::Yellow, Rank::Five),
        card(Suit::Purple, Rank::Four),
    ];
    let mut state = FullState::new_standard(2, deck_with_prefix(&prefix)).unwrap();
    let playable = state.hand(PlayerId::new(0)).unwrap()[0];

    state
        .apply(Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Suit(Suit::Red),
        })
        .unwrap();
    state
        .apply(Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Rank(Rank::One),
        })
        .unwrap();

    assert_eq!(select(&state), Action::Play(playable));
}

#[test]
fn discards_a_card_that_direct_information_proves_is_useless() {
    let prefix = [
        card(Suit::Red, Rank::One),
        card(Suit::Red, Rank::One),
        card(Suit::Blue, Rank::Two),
        card(Suit::Green, Rank::Three),
        card(Suit::Yellow, Rank::Four),
        card(Suit::Purple, Rank::Five),
        card(Suit::Blue, Rank::Five),
        card(Suit::Green, Rank::Five),
        card(Suit::Yellow, Rank::Five),
        card(Suit::Purple, Rank::Four),
    ];
    let mut state = FullState::new_standard(2, deck_with_prefix(&prefix)).unwrap();
    let played = state.hand(PlayerId::new(0)).unwrap()[0];
    let useless = state.hand(PlayerId::new(0)).unwrap()[1];
    state.apply(Action::Play(played)).unwrap();
    state
        .apply(Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Rank(Rank::One),
        })
        .unwrap();

    let mut view = state.view_for(PlayerId::new(0)).unwrap();
    view.hands[0]
        .iter_mut()
        .find(|observed| observed.id == useless)
        .unwrap()
        .clues
        .add_positive_clue(Clue::Suit(Suit::Red));
    let information_set = InformationSet::new(&view).unwrap();

    assert!(
        assess_card(information_set.deductions(), useless)
            .unwrap()
            .certainly_useless
    );
    assert_eq!(
        ConventionAgnosticPolicy
            .select_action(information_set.deductions())
            .unwrap(),
        Action::Discard(useless)
    );
}

#[test]
fn falls_back_to_discarding_the_oldest_card() {
    let mut state = FullState::new_standard(2, standard_deck()).unwrap();
    state
        .apply(Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Suit(Suit::Red),
        })
        .unwrap();
    let oldest = state.hand(PlayerId::new(1)).unwrap()[0];

    assert_eq!(select(&state), Action::Discard(oldest));
}

#[test]
fn blind_plays_the_newest_card_when_clues_are_full() {
    let state = FullState::new_standard(2, standard_deck()).unwrap();
    let newest = *state.hand(PlayerId::new(0)).unwrap().last().unwrap();

    assert_eq!(select(&state), Action::Play(newest));
}

#[test]
fn policy_can_advance_a_game_without_selecting_clues() {
    let mut state = FullState::new_standard(2, standard_deck()).unwrap();
    let mut turns = 0;
    while !state.is_terminal() && turns < 512 {
        let action = select(&state);
        assert!(!matches!(action, Action::Clue { .. }));
        state.apply(action).unwrap();
        turns += 1;
    }
    assert!(state.is_terminal());
    assert!(turns > 0);
}
