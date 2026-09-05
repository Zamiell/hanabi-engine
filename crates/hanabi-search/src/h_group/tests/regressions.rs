#[test]
fn a_finessed_player_still_has_an_unclued_chop() {
    let state = expert_replay_p4v0s415()
        .state_at_turn(2)
        .expect("the position after the opening rank-2 clue exists");
    let donald = PlayerId::new(3);
    let deductions = LogicalDeductions::new(state.view_for(donald).expect("Donald has a view"))
        .expect("Donald's deductions are valid");
    let replay = replay_h_group(&deductions, HGroupProfile::Max);
    let inferred = infer_h_group_from_replay(&deductions, replay.clone(), HGroupProfile::Max);

    assert_eq!(
        inferred.chops[donald.index()],
        Some(CardId::new(12)),
        "a blind-play obligation does not remove the next unclued discard: promptable={:?}; invisible={:?}; chop_moved={:?}; replay={replay:#?}",
        replay.promptable(),
        inferred.invisibly_clued,
        inferred.chop_moved,
    );
}
