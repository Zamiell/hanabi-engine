use std::{fs, path::PathBuf};

use hanabi_core::{CardId, Clue, EndReason, GameEvent, GameStatus, Suit};
use hanabi_protocol::{HanabiLiveReplay, ReplayError};

#[test]
fn partial_options_default_to_no_variant() {
    let json = r#"{
        "players":["A","B"],
        "deck":[
            {"suitIndex":0,"rank":1},{"suitIndex":0,"rank":1},
            {"suitIndex":0,"rank":1},{"suitIndex":0,"rank":2},
            {"suitIndex":0,"rank":2},{"suitIndex":0,"rank":3},
            {"suitIndex":0,"rank":3},{"suitIndex":0,"rank":4},
            {"suitIndex":0,"rank":4},{"suitIndex":0,"rank":5}
        ],
        "actions":[],
        "options":{"deckPlays":true}
    }"#;

    let replay = HanabiLiveReplay::from_json(json).unwrap();
    assert_eq!(replay.options.unwrap().variant, "No Variant");
}

#[test]
fn replays_hanabi_live_no_variant_fixture() {
    let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../../hanabi-live/packages/client/test_data/no_variant.json");
    if !fixture.exists() {
        eprintln!(
            "skipping sibling-repository compatibility test; {} is absent",
            fixture.display()
        );
        return;
    }

    let json = fs::read_to_string(&fixture).unwrap();
    let state = HanabiLiveReplay::from_json(&json)
        .unwrap()
        .replay()
        .unwrap();

    // These values were independently produced by Hanabi Live's loadGameJSON
    // reducer for this exact fixture.
    assert_eq!(state.turn(), 53);
    assert_eq!(state.score(), 23);
    assert_eq!(state.clue_tokens(), 4);
    assert_eq!(state.strikes(), 0);
    assert_eq!(state.deck_size(), 0);
    assert_eq!(
        state.status(),
        GameStatus::Finished(EndReason::FinalRoundComplete)
    );
    assert_eq!(
        state.play_stacks().each_ref().map(Vec::len),
        [3, 5, 5, 5, 5]
    );
    assert_eq!(
        state.hands(),
        &[
            vec![CardId::new(0), CardId::new(33), CardId::new(48)],
            vec![CardId::new(27), CardId::new(34), CardId::new(41)],
            vec![CardId::new(35), CardId::new(38), CardId::new(42)],
            vec![CardId::new(29), CardId::new(39), CardId::new(43)],
            vec![
                CardId::new(30),
                CardId::new(40),
                CardId::new(44),
                CardId::new(47),
            ],
        ]
    );
    state.validate().unwrap();
}

#[test]
fn reconstructs_actionable_replay_prefixes_by_game_turn() {
    let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../../hanabi-live/packages/client/test_data/no_variant.json");
    if !fixture.exists() {
        return;
    }

    let json = fs::read_to_string(&fixture).unwrap();
    let replay = HanabiLiveReplay::from_json(&json).unwrap();

    let initial = replay.state_at_turn(0).unwrap();
    assert_eq!(initial.turn(), 0);
    assert_eq!(initial.score(), 0);
    assert_eq!(initial.current_player(), hanabi_core::PlayerId::new(0));
    let purple_card = replay
        .deck
        .iter()
        .position(|card| card.suit_index == 4)
        .unwrap();
    assert_eq!(
        initial.card(CardId::new(purple_card)).unwrap().suit,
        Suit::Purple
    );

    let middle = replay.state_at_turn(17).unwrap();
    assert_eq!(middle.turn(), 17);
    assert_eq!(middle.current_player(), hanabi_core::PlayerId::new(2));
    assert_eq!(middle.status(), GameStatus::InProgress);

    let purple_clue_turn = replay
        .actions
        .iter()
        .position(|action| action.action_type == 2 && action.value == 4)
        .unwrap();
    let after_purple_clue = replay
        .state_at_turn(u32::try_from(purple_clue_turn + 1).unwrap())
        .unwrap();
    assert!(matches!(
        &after_purple_clue.history().last().unwrap().event,
        GameEvent::Clued {
            clue: Clue::Suit(Suit::Purple),
            ..
        }
    ));

    assert_eq!(replay.state_at_turn(53).unwrap(), replay.replay().unwrap());
    assert!(matches!(
        replay.state_at_turn(54),
        Err(ReplayError::TurnOutOfRange {
            requested: 54,
            available: 53
        })
    ));
}
