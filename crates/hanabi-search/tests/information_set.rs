use std::collections::BTreeSet;

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
fn samples_preserve_the_complete_player_view() {
    let (truth, view) = clued_player_zero_view();
    let information_set = InformationSet::new(view.clone()).unwrap();
    assert!(information_set.hand_assignment_count() > 0);

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

    let visible_hand = truth.hand(PlayerId::new(1)).unwrap().to_vec();
    let visible_identities = visible_hand
        .iter()
        .map(|id| truth.card(*id).unwrap())
        .collect::<Vec<_>>();
    let mut sampled_rank_ones = BTreeSet::new();

    for seed in 0..64 {
        let sampled = information_set.sample_seeded(seed).unwrap();
        sampled.validate().unwrap();
        assert_eq!(sampled.view_for(PlayerId::new(0)).unwrap(), view);
        assert_eq!(sampled.card(rank_one).unwrap().rank, Rank::One);
        sampled_rank_ones.insert(sampled.card(rank_one).unwrap());

        for (id, expected) in visible_hand.iter().zip(&visible_identities) {
            assert_eq!(sampled.card(*id), Some(*expected));
        }
    }
    assert!(sampled_rank_ones.len() > 1);
}

#[test]
fn seeded_sampling_is_reproducible() {
    let (_, view) = clued_player_zero_view();
    let information_set = InformationSet::new(view).unwrap();

    assert_eq!(
        information_set.sample_seeded(867_5309).unwrap(),
        information_set.sample_seeded(867_5309).unwrap()
    );
}

#[test]
fn rejects_logically_impossible_clue_constraints() {
    let state = FullState::new_standard(2, standard_deck()).unwrap();
    let mut view = state.view_for(PlayerId::new(0)).unwrap();
    let hidden = &mut view.hands[0][0];
    hidden.clues.add_positive_clue(Clue::Suit(Suit::Red));
    hidden.clues.add_negative_clue(Clue::Suit(Suit::Red));

    assert_eq!(
        InformationSet::new(view),
        Err(InformationSetError::NoConsistentWorld)
    );
}
