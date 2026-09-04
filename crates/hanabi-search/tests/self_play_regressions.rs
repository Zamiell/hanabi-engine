use hanabi_protocol::HanabiLiveReplay;
use hanabi_search::{HGroupProfile, InformationSet, SupportedConvention, WorldCount};

#[test]
fn independently_known_prompt_connector_does_not_force_an_ambiguous_card_first() {
    for (json, turn) in [
        (include_str!("fixtures/self-play-p4v0s107.json"), 48),
        (include_str!("fixtures/self-play-p4v0s116.json"), 38),
    ] {
        let replay = HanabiLiveReplay::from_json(json).unwrap();
        let state = replay.state_at_turn(turn).unwrap();
        let view = state.view_for(state.current_player()).unwrap();
        let information = InformationSet::new(&view).unwrap();
        let analysis =
            SupportedConvention::HGroup(HGroupProfile::Max).analyze(information.deductions());
        assert_ne!(
            information.world_count_up_to(&analysis.belief_constraints, 1),
            WorldCount::Exact(0),
            "turn {turn}: {json}"
        );
    }
}

#[test]
fn a_save_must_not_knowingly_touch_a_useful_duplicate() {
    let replay =
        HanabiLiveReplay::from_json(include_str!("fixtures/self-play-p4v0s34.json")).unwrap();
    let state = replay.state_at_turn(25).unwrap();
    let view = state.view_for(state.current_player()).unwrap();
    let information = InformationSet::new(&view).unwrap();
    let analysis =
        SupportedConvention::HGroup(HGroupProfile::Max).analyze(information.deductions());
    let bad = hanabi_core::Action::Clue {
        target: hanabi_core::PlayerId::new(2),
        clue: hanabi_core::Clue::Rank(hanabi_core::Rank::Four),
    };
    assert!(
        analysis
            .actions
            .iter()
            .all(|candidate| candidate.action != bad)
    );
}

#[test]
fn a_clue_cannot_continue_the_connection_it_just_created() {
    let replay =
        HanabiLiveReplay::from_json(include_str!("fixtures/self-play-p4v0s20.json")).unwrap();
    let state = replay.state_at_turn(13).unwrap();
    let view = state.view_for(state.current_player()).unwrap();
    let information = InformationSet::new(&view).unwrap();
    let analysis =
        SupportedConvention::HGroup(HGroupProfile::Max).analyze(information.deductions());
    let hanabi_search::ConventionInferences::HGroup(inferred) = analysis.inferences else {
        panic!("expected H-Group");
    };
    assert!(!inferred.signals.iter().any(|signal| signal.turn == 10
        && signal.kind == hanabi_search::HGroupMoveKind::ContinuationClue));
}

#[test]
fn transfer_and_clarity_do_not_invent_duplicate_cards() {
    for (json, turn) in [
        (include_str!("fixtures/self-play-p4v0s4.json"), 43),
        (include_str!("fixtures/self-play-p4v0s12.json"), 26),
        (include_str!("fixtures/self-play-p4v0s16-endgame.json"), 61),
        (include_str!("fixtures/self-play-p4v0s24.json"), 51),
        (include_str!("fixtures/self-play-p4v0s37.json"), 53),
        (include_str!("fixtures/self-play-p4v0s56.json"), 40),
    ] {
        let replay = HanabiLiveReplay::from_json(json).unwrap();
        let state = replay.state_at_turn(turn).unwrap();
        let view = state.view_for(state.current_player()).unwrap();
        let information = InformationSet::new(&view).unwrap();
        let analysis =
            SupportedConvention::HGroup(HGroupProfile::Max).analyze(information.deductions());
        assert_ne!(
            information.world_count_up_to(&analysis.belief_constraints, 1),
            WorldCount::Exact(0),
            "turn {turn}: {json}"
        );
    }
}

/// Diagnostic reproduction, not an expert assertion about optimal play.
#[test]
#[ignore = "Unresolved giver/recipient Clarity disagreement: blue at turn 50 swaps the two blue identities"]
fn self_play_blue_clarity_disagreement_remains_unresolved() {
    let replay =
        HanabiLiveReplay::from_json(include_str!("fixtures/self-play-p4v0s44.json")).unwrap();
    let state = replay.state_at_turn(59).unwrap();
    let view = state.view_for(state.current_player()).unwrap();
    let information = InformationSet::new(&view).unwrap();
    let analysis =
        SupportedConvention::HGroup(HGroupProfile::Max).analyze(information.deductions());
    assert_ne!(
        information.world_count_up_to(&analysis.belief_constraints, 1),
        WorldCount::Exact(0)
    );
}

#[test]
fn public_clues_and_forced_discards_do_not_leave_impossible_elimination_claims() {
    for (json, turn) in [
        (include_str!("fixtures/self-play-p4v0s11.json"), 35),
        (include_str!("fixtures/self-play-p4v0s29.json"), 36),
    ] {
        let replay = HanabiLiveReplay::from_json(json).unwrap();
        let state = replay.state_at_turn(turn).unwrap();
        let view = state.view_for(state.current_player()).unwrap();
        let information = InformationSet::new(&view).unwrap();
        let analysis =
            SupportedConvention::HGroup(HGroupProfile::Max).analyze(information.deductions());
        assert_ne!(
            information.world_count_up_to(&analysis.belief_constraints, 1),
            WorldCount::Exact(0)
        );
    }
}

#[test]
fn impossible_clue_obligation_retains_an_emergency_action() {
    let replay =
        HanabiLiveReplay::from_json(include_str!("fixtures/self-play-p4v0s25.json")).unwrap();
    let state = replay.state_at_turn(23).unwrap();
    let view = state.view_for(state.current_player()).unwrap();
    let information = InformationSet::new(&view).unwrap();
    let analysis =
        SupportedConvention::HGroup(HGroupProfile::Max).analyze(information.deductions());
    assert!(!analysis.actions.is_empty());
    assert!(
        analysis
            .actions
            .iter()
            .all(|candidate| view.legal_actions().contains(&candidate.action))
    );
}

#[test]
fn hard_three_bluff_reinterprets_both_focus_and_collateral() {
    let replay =
        HanabiLiveReplay::from_json(include_str!("fixtures/self-play-p4v0s14.json")).unwrap();
    let state = replay.state_at_turn(16).unwrap();
    let view = state.view_for(state.current_player()).unwrap();
    let information = InformationSet::new(&view).unwrap();
    let analysis =
        SupportedConvention::HGroup(HGroupProfile::Max).analyze(information.deductions());
    let hanabi_search::ConventionInferences::HGroup(inferred) = analysis.inferences else {
        panic!("expected H-Group")
    };
    let red_three = hanabi_core::Card::new(hanabi_core::Suit::Red, hanabi_core::Rank::Three);
    assert!(
        !inferred
            .cards
            .iter()
            .find(|card| card.card.index() == 2)
            .unwrap()
            .identities
            .contains(red_three)
    );
    assert!(
        inferred
            .cards
            .iter()
            .find(|card| card.card.index() == 3)
            .unwrap()
            .identities
            .contains(red_three)
    );
}

#[test]
fn pending_connection_does_not_remove_required_clues() {
    let replay =
        HanabiLiveReplay::from_json(include_str!("fixtures/self-play-p4v0s16.json")).unwrap();
    let state = replay.state_at_turn(23).unwrap();
    let view = state.view_for(state.current_player()).unwrap();
    let information = InformationSet::new(&view).unwrap();
    let analysis =
        SupportedConvention::HGroup(HGroupProfile::Max).analyze(information.deductions());
    assert!(!analysis.actions.is_empty());
    assert!(
        analysis
            .actions
            .iter()
            .any(|candidate| Some(candidate.action) == analysis.preferred_action)
    );
}

#[test]
fn discarding_two_trash_ones_does_not_promise_a_third_copy() {
    let replay =
        HanabiLiveReplay::from_json(include_str!("fixtures/self-play-p4v0s5.json")).unwrap();
    let state = replay.state_at_turn(51).unwrap();
    let view = state.view_for(state.current_player()).unwrap();
    let information = InformationSet::new(&view).unwrap();
    let analysis =
        SupportedConvention::HGroup(HGroupProfile::Max).analyze(information.deductions());
    assert_ne!(
        information.world_count_up_to(&analysis.belief_constraints, 4096),
        WorldCount::Exact(0)
    );
}
