//! Cache invariants and differential checks use reviewed replay inputs, not
//! newly invented convention expectations.
use super::*;

struct BypassGuard(bool);

impl BypassGuard {
    fn new(bypass: bool) -> Self {
        Self(BYPASS_REQUEST_REPLAY_MEMO.replace(bypass))
    }
}

impl Drop for BypassGuard {
    fn drop(&mut self) {
        BYPASS_REQUEST_REPLAY_MEMO.set(self.0);
    }
}

fn compare_positions(turns: &[u32]) {
    let fixture = expert_replay_p4v0s2();
    let run = |bypass| {
        let _bypass = BypassGuard::new(bypass);
        inverse_planning::clear_test_caches();
        let started = std::time::Instant::now();
        let results = turns
            .iter()
            .map(|&turn| {
                let state = fixture.state_at_turn(turn).unwrap();
                let view = state.view_for(state.current_player()).unwrap();
                let analysis = crate::analyze_position(
                    &view,
                    crate::SupportedConvention::HGroup(HGroupProfile::Max),
                    crate::PlannerConfig {
                        objective: crate::PlanningObjective::PerfectScore,
                        ..crate::PlannerConfig::default()
                    },
                )
                .unwrap();
                let replay = replay_h_group(analysis.information.deductions(), HGroupProfile::Max);
                assert!(H_GROUP_REPLAY_MEMO.with_borrow(Option::is_none));
                (
                    analysis,
                    replay.knowledge.effects().to_vec(),
                    replay.strategic_deductions,
                )
            })
            .collect::<Vec<_>>();
        eprintln!("request memo bypass={bypass}: {:?}", started.elapsed());
        results
    };
    let before = run(true);
    let after = run(false);
    for ((turn, before), after) in turns.iter().zip(before).zip(after) {
        assert_eq!(
            before,
            after,
            "complete analysis/effects/proofs differ at turn {}",
            turn + 1
        );
    }
}

#[test]
fn request_memo_preserves_complete_reviewed_analyses() {
    compare_positions(&[0, 12]);
}

#[test]
#[ignore = "full cold-cache differential; run when changing history memoization"]
fn request_memo_preserves_full_replay_analyses() {
    compare_positions(&(0..47).collect::<Vec<_>>());
}

#[test]
fn request_memo_is_bounded_nested_and_cleared_on_unwind() {
    let fixture = expert_replay_p4v0s2();
    let state = fixture.state_at_turn(1).unwrap();
    let expected = (0..4)
        .map(|observer| {
            let deductions =
                LogicalDeductions::new(state.view_for(PlayerId::new(observer)).unwrap()).unwrap();
            infer_h_group(&deductions, HGroupProfile::Max)
        })
        .collect::<Vec<_>>();
    let result = std::panic::catch_unwind(|| {
        let _outer = begin_replay_memo(2);
        for (observer, expected) in expected.iter().enumerate() {
            {
                let _inner = begin_replay_memo(100);
                let deductions = LogicalDeductions::new(
                    state
                        .view_for(PlayerId::new(u8::try_from(observer).unwrap()))
                        .unwrap(),
                )
                .unwrap();
                assert_eq!(&infer_h_group(&deductions, HGroupProfile::Max), expected);
            }
            H_GROUP_REPLAY_MEMO.with_borrow(|memo| {
                let memo = memo.as_ref().unwrap();
                assert_eq!(memo.limit, 2);
                assert!(memo.entries.len() <= 2);
            });
        }
        panic!("exercise unwinding cleanup");
    });
    let panic = result.unwrap_err();
    assert_eq!(
        panic.downcast_ref::<&str>(),
        Some(&"exercise unwinding cleanup")
    );
    assert!(H_GROUP_REPLAY_MEMO.with_borrow(Option::is_none));
}

#[test]
fn cancellation_does_not_leave_a_request_memo() {
    let fixture = expert_replay_p4v0s2();
    let state = fixture.state_at_turn(0).unwrap();
    let cancellation = crate::CancellationToken::default();
    cancellation.cancel();
    let control = crate::AnalysisControl::new(cancellation, None, u64::MAX);
    assert!(
        crate::analyze_position_with_control(
            &state.view_for(state.current_player()).unwrap(),
            crate::SupportedConvention::HGroup(HGroupProfile::Max),
            crate::PlannerConfig::default(),
            &control,
        )
        .is_err()
    );
    assert!(H_GROUP_REPLAY_MEMO.with_borrow(Option::is_none));
}

#[test]
fn memo_key_distinguishes_observer_and_reasoning_stage() {
    let fixture = expert_replay_p4v0s2();
    let state = fixture.state_at_turn(1).unwrap();
    let key = ReplayMemoKey {
        view: state.view_for(PlayerId::new(0)).unwrap(),
        profile: HGroupProfile::Max,
        perspective_depth: PerspectiveDepth::NestedRecipients,
        allow_blind_reverse_empathy: false,
        counterfactual: false,
    };
    let mut keys = std::collections::HashSet::from([key.clone()]);
    let mut other = key.clone();
    other.view = state.view_for(PlayerId::new(1)).unwrap();
    assert!(keys.insert(other));
    let mut other = key.clone();
    other.counterfactual = true;
    assert!(keys.insert(other));
    let mut other = key.clone();
    other.allow_blind_reverse_empathy = true;
    assert!(keys.insert(other));
    let mut other = key.clone();
    other.perspective_depth = PerspectiveDepth::ObserverOnly;
    assert!(keys.insert(other));
    let mut other = key.clone();
    other.profile = HGroupProfile::Level(HGroupLevel::Level1);
    assert!(keys.insert(other));
    let mut other = key;
    other.view = fixture
        .state_at_turn(2)
        .unwrap()
        .view_for(PlayerId::new(0))
        .unwrap();
    assert!(keys.insert(other));
}
