use super::*;

#[test]
fn every_expert_replay_prefix_satisfies_h_group_state_invariants() {
    let fixture = expert_replay_194321();
    for turn in 0..=u32::try_from(fixture.actions.len()).expect("replay fits in u32") {
        let state = fixture.state_at_turn(turn).expect("turn exists");
        for observer in 0..state.num_players() {
            let observer = PlayerId::new(observer);
            let deductions =
                LogicalDeductions::new(state.view_for(observer).expect("observer exists"))
                    .expect("valid deductions");
            let replay = replay_h_group_inner(
                &deductions,
                HGroupProfile::Max,
                PerspectiveDepth::ObserverOnly,
            );
            assert_eq!(
                replay.validate(),
                Ok(()),
                "invalid replay at turn {turn} for {observer:?}"
            );
        }
    }
}
