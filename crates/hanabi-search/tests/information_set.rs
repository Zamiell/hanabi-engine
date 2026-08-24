use hanabi_core::{Action, Card, Clue, FullState, PlayerId, Rank, Suit, standard_deck};
use hanabi_search::{InformationSet, InformationSetError};

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

fn clued_player_zero_view() -> (FullState, hanabi_core::PlayerView) {
    let prefix = [
        card(Suit::Red, Rank::One),
        card(Suit::Blue, Rank::Two),
        card(Suit::Green, Rank::Three),
        card(Suit::Yellow, Rank::Four),
        card(Suit::Purple, Rank::Five),
        card(Suit::Red, Rank::Five),
        card(Suit::Blue, Rank::One),
        card(Suit::Green, Rank::Two),
        card(Suit::Yellow, Rank::Three),
        card(Suit::Purple, Rank::Four),
    ];
    let mut state = FullState::new_standard(2, deck_with_prefix(&prefix)).unwrap();
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
    let view = state.view_for(PlayerId::new(0)).unwrap();
    (state, view)
}

#[test]
fn direct_clues_and_counts_restrict_hidden_identities() {
    let (truth, view) = clued_player_zero_view();
    let information_set = InformationSet::new(&view).unwrap();

    let own_hand = truth.hand(PlayerId::new(0)).unwrap();
    let rank_one = own_hand[0];
    let red_five = card(Suit::Red, Rank::Five);
    assert!(
        information_set
            .possible_identities(rank_one)
            .unwrap()
            .iter()
            .all(|identity| identity.rank == Rank::One && identity != red_five)
    );
    for card_id in &own_hand[1..] {
        assert!(
            information_set
                .possible_identities(*card_id)
                .unwrap()
                .iter()
                .all(|identity| identity.rank != Rank::One && identity != red_five)
        );
    }
}

#[test]
fn rejects_logically_impossible_clue_constraints() {
    let state = FullState::new_standard(2, standard_deck()).unwrap();
    let mut view = state.view_for(PlayerId::new(0)).unwrap();
    let hidden = &mut view.hands[0][0];
    hidden.clues.add_positive_clue(Clue::Suit(Suit::Red));
    hidden.clues.add_negative_clue(Clue::Suit(Suit::Red));

    assert_eq!(
        InformationSet::new(&view),
        Err(InformationSetError::NoConsistentWorld)
    );
}
