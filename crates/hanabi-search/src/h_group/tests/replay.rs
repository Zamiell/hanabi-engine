use super::*;

fn assert_expert_replay_matches_engine(replay: &HanabiLiveReplay) {
    for turn in 0..u32::try_from(replay.actions.len()).expect("replay fits in u32") {
        let state = replay.state_at_turn(turn).expect("fixture prefix is legal");
        let actor = state.current_player();
        let view = state.view_for(actor).expect("current player has a view");
        let analysis = crate::analyze_position(
            &view,
            crate::SupportedConvention::HGroup(HGroupProfile::Max),
            crate::PlannerConfig {
                objective: crate::PlanningObjective::PerfectScore,
                ..crate::PlannerConfig::default()
            },
        )
        .unwrap_or_else(|error| {
            panic!(
                "fixture position at move {} is analyzable: {error}",
                turn + 1
            )
        });
        let expected = replay_action_at_turn(replay, turn);
        assert_eq!(
            analysis.planner.best_action,
            expected,
            "engine disagrees at move {}; candidates: {:#?}",
            turn + 1,
            analysis.planner.root_actions,
        );
    }
}

/// Golden compatibility oracle: every position in the curated expert replay
/// must select its corresponding optimal action.
#[test]
fn optimized_expert_replay_matches_engine() {
    assert_expert_replay_matches_engine(&expert_replay_194321());
}

#[test]
fn second_expert_replay_matches_engine() {
    assert_expert_replay_matches_engine(&expert_replay_p4v0s9());
}
