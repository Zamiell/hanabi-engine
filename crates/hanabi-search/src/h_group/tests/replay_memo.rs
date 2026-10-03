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
                assert!(memo.entries.len() + memo.previous.len() <= 2);
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

struct LegacyGuard(bool);

#[test]
fn generations_promote_hits_and_evict_only_older_entries() {
    let state = expert_replay_p4v0s2().state_at_turn(0).unwrap();
    let deductions = LogicalDeductions::new(state.view_for(PlayerId::new(0)).unwrap()).unwrap();
    let value = replay_h_group(&deductions, HGroupProfile::Max);
    let keys = (0..4)
        .map(|observer| ReplayMemoKey {
            view: state.view_for(PlayerId::new(observer)).unwrap(),
            profile: HGroupProfile::Max,
            perspective_depth: PerspectiveDepth::ObserverOnly,
            allow_blind_reverse_empathy: false,
            counterfactual: false,
        })
        .collect::<Vec<_>>();
    let mut memo = ReplayMemo {
        entries: HashMap::new(),
        previous: HashMap::new(),
        limit: 4,
        legacy: false,
    };
    for key in &keys[..3] {
        memo.insert(key.clone(), value.clone());
    }
    assert_eq!(memo.previous.len(), 2);
    assert!(memo.get(&keys[0]).is_some()); // Promote the frequently reused entry.
    memo.insert(keys[3].clone(), value.clone());
    assert!(memo.get(&keys[1]).is_none()); // Only the unreferenced old entry is gone.
    for key in [&keys[0], &keys[2], &keys[3]] {
        assert!(memo.get(key).is_some());
        assert!(memo.entries.len() + memo.previous.len() <= memo.limit);
    }
    // Replacement must not leave a stale duplicate in the older generation.
    memo.insert(keys[0].clone(), value.clone());
    assert!(!memo.previous.contains_key(&keys[0]));
    assert!(memo.entries.contains_key(&keys[0]));
    for limit in [1, 2, 3] {
        memo.entries.clear();
        memo.previous.clear();
        memo.limit = limit;
        for key in &keys {
            memo.insert(key.clone(), value.clone());
            assert!(memo.get(key).is_some());
            assert!(memo.entries.len() + memo.previous.len() <= limit);
        }
    }
}

impl LegacyGuard {
    fn new(legacy: bool) -> Self {
        Self(LEGACY_REPLAY_MEMO.replace(legacy))
    }
}

impl Drop for LegacyGuard {
    fn drop(&mut self) {
        LEGACY_REPLAY_MEMO.set(self.0);
    }
}

#[test]
#[ignore = "sequential fixed-work cache benchmark; run explicitly with --nocapture"]
fn benchmark_reviewed_replay_cache() {
    // No strategic expectations: compare complete inference and projections
    // from identical reviewed positions under the two cache policies.
    let fixture = expert_replay_p4v0s9();
    let turns = std::env::var("HANABI_CACHE_BENCH_TURNS").unwrap_or_else(|_| "20,31,32,41".into());
    let horizon =
        std::env::var("HANABI_CACHE_BENCH_HORIZON").map_or(4, |value| value.parse().unwrap());
    for turn in turns.split(',').map(|value| value.parse::<u32>().unwrap()) {
        let state = fixture.state_at_turn(turn - 1).unwrap();
        let view = state.view_for(state.current_player()).unwrap();
        let convention = crate::SupportedConvention::HGroup(HGroupProfile::Max);
        let run = |legacy| {
            let _legacy = LegacyGuard::new(legacy);
            inverse_planning::clear_test_caches();
            let profiling = std::env::var_os("HANABI_PROFILE_REPLAY").is_some();
            if profiling {
                crate::test_profile::start();
            }
            let started = std::time::Instant::now();
            let _memo = begin_analysis_replay_memo();
            let deductions = LogicalDeductions::new(view.clone()).unwrap();
            let analysis = convention.analyze(&deductions);
            let control = crate::AnalysisControl::default();
            let projections = analysis
                .actions
                .iter()
                .map(|candidate| {
                    convention
                        .project_symbolic_projection(&view, candidate.action, horizon, &control)
                        .unwrap()
                })
                .collect::<Vec<_>>();
            let elapsed = started.elapsed();
            eprintln!(
                "CACHE_BENCH\tturn={turn}\tlegacy={legacy}\thorizon={horizon}\tactions={}\tseconds={:.6}",
                analysis.actions.len(),
                elapsed.as_secs_f64()
            );
            if profiling {
                crate::test_profile::finish("p4v0s9", turn, elapsed);
            }
            (analysis, projections)
        };
        let repeats =
            std::env::var("HANABI_CACHE_BENCH_REPEATS").map_or(3, |value| value.parse().unwrap());
        for repeat in 0..repeats {
            let first = run(repeat % 2 == 0);
            let second = run(repeat % 2 != 0);
            assert_eq!(
                first, second,
                "cache changed analysis/projections at turn {turn}"
            );
        }
    }
}

#[test]
#[ignore = "full move cache benchmark, including the normal two-minute deadline"]
fn benchmark_reviewed_move_cache() {
    let fixture = expert_replay_p4v0s9();
    let turns = std::env::var("HANABI_CACHE_BENCH_TURNS").unwrap_or_else(|_| "20,31,32,41".into());
    for turn in turns.split(',').map(|value| value.parse::<u32>().unwrap()) {
        let state = fixture.state_at_turn(turn - 1).unwrap();
        let view = state.view_for(state.current_player()).unwrap();
        let run = |legacy| {
            let _legacy = LegacyGuard::new(legacy);
            inverse_planning::clear_test_caches();
            let started = std::time::Instant::now();
            let analysis = crate::analyze_position(
                &view,
                crate::SupportedConvention::HGroup(HGroupProfile::Max),
                crate::PlannerConfig {
                    objective: crate::PlanningObjective::PerfectScore,
                    ..crate::PlannerConfig::default()
                },
            )
            .unwrap();
            let planner = &analysis.planner;
            eprintln!(
                "MOVE_CACHE_BENCH\tturn={turn}\tlegacy={legacy}\tseconds={:.6}\texhausted={}\tcompleted={}/{}\tbest={:?}",
                started.elapsed().as_secs_f64(),
                planner.budget_exhausted,
                planner
                    .root_actions
                    .iter()
                    .filter(|candidate| candidate.projection_evaluated)
                    .count(),
                planner.root_actions.len(),
                planner.best_action
            );
            analysis
        };
        let before = run(true);
        let after = run(false);
        // A timed-out run may finish different candidates; it cannot establish
        // full semantic equivalence. Never mask those differences as equality.
        if !before.planner.budget_exhausted && !after.planner.budget_exhausted {
            assert_eq!(before, after, "complete analysis differs at turn {turn}");
        }
        eprintln!(
            "MOVE_CACHE_BENCH\tturn={turn}\tsame_action={}",
            before.planner.best_action == after.planner.best_action
        );
    }
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
