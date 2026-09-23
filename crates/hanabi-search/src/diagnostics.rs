//! Opt-in, thread-local capture of evidence produced by the real planner.
//! This observes decisions; it never invokes inference or changes selection.
use std::cell::RefCell;

use hanabi_core::{Action, PlayerView};

use crate::{CandidateComparison, ConventionAnalysis, PlannerActionEvaluation};

#[derive(Clone, Debug)]
pub struct ProjectedDecision {
    pub root: Option<Action>,
    pub view: PlayerView,
    pub convention: ConventionAnalysis,
    pub candidates: Vec<PlannerActionEvaluation>,
    pub comparisons: Vec<CandidateComparison>,
    pub selected: Option<Action>,
}

#[derive(Default)]
struct Capture {
    root: Option<Action>,
    decisions: Vec<ProjectedDecision>,
    meanings: std::collections::HashMap<(PlayerView, Action), crate::HGroupClueInterpretation>,
}

pub(crate) fn record_meaning(
    view: &PlayerView,
    action: Action,
    meaning: &crate::HGroupClueInterpretation,
) {
    CAPTURE.with(|state| {
        if let Some(capture) = state.borrow_mut().as_mut() {
            capture
                .meanings
                .insert((view.clone(), action), meaning.clone());
        }
    });
}

pub(crate) fn meaning(
    view: &PlayerView,
    action: Action,
) -> Option<crate::HGroupClueInterpretation> {
    CAPTURE.with(|state| {
        state
            .borrow()
            .as_ref()?
            .meanings
            .get(&(view.clone(), action))
            .cloned()
    })
}

thread_local! {
    static CAPTURE: RefCell<Option<Capture>> = const { RefCell::new(None) };
}

pub(crate) fn enabled() -> bool {
    CAPTURE.with(|state| state.borrow().is_some())
}

/// Collect detailed evidence only for this closure, restoring any enclosing
/// capture even if analysis panics. Ordinary gameplay allocates no trace.
///
/// # Panics
/// Propagates a panic from `analyze`, restoring the enclosing capture first.
pub fn capture_decisions<T>(analyze: impl FnOnce() -> T) -> (T, Vec<ProjectedDecision>) {
    struct Restore(Option<Capture>);
    impl Drop for Restore {
        fn drop(&mut self) {
            CAPTURE.with(|state| *state.borrow_mut() = self.0.take());
        }
    }
    let _restore = Restore(CAPTURE.with(|state| state.replace(Some(Capture::default()))));
    let result = analyze();
    let decisions = CAPTURE.with(|state| {
        std::mem::take(
            &mut state
                .borrow_mut()
                .as_mut()
                .expect("active capture")
                .decisions,
        )
    });
    (result, decisions)
}

pub(crate) struct RootScope(Option<Action>);

pub(crate) fn root(action: Action) -> RootScope {
    RootScope(CAPTURE.with(|state| {
        state
            .borrow_mut()
            .as_mut()
            .and_then(|capture| capture.root.replace(action))
    }))
}

impl Drop for RootScope {
    fn drop(&mut self) {
        CAPTURE.with(|state| {
            if let Some(capture) = state.borrow_mut().as_mut() {
                capture.root = self.0;
            }
        });
    }
}

pub(crate) fn record(
    view: &PlayerView,
    convention: &ConventionAnalysis,
    candidates: &[PlannerActionEvaluation],
    comparisons: &[CandidateComparison],
    selected: Option<Action>,
) {
    CAPTURE.with(|state| {
        if let Some(capture) = state.borrow_mut().as_mut() {
            capture.decisions.push(ProjectedDecision {
                root: capture.root,
                view: view.clone(),
                convention: convention.clone(),
                candidates: candidates.to_vec(),
                comparisons: comparisons.to_vec(),
                selected,
            });
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capture_is_scoped_and_restored_after_panic() {
        assert!(!enabled());
        capture_decisions(|| {
            assert!(enabled());
            let panic = std::panic::catch_unwind(|| capture_decisions(|| panic!("test")));
            assert!(panic.is_err());
            assert!(enabled());
        });
        assert!(!enabled());
    }

    #[test]
    fn tracing_preserves_reviewed_position_decision_and_hidden_information() {
        let replay = hanabi_protocol::HanabiLiveReplay::from_json(include_str!(
            "h_group/tests/fixtures/game-p4v0s1-before-turn18-revision.json"
        ))
        .unwrap();
        let state = replay.state_at_turn(2).unwrap();
        let view = state.view_for(state.current_player()).unwrap();
        let run = || {
            crate::analyze_position(
                &view,
                crate::SupportedConvention::HGroup(crate::HGroupProfile::Max),
                crate::PlannerConfig {
                    exact_world_limit: 1,
                    ..Default::default()
                },
            )
            .unwrap()
        };
        let plain = run();
        assert!(plain.convention_analysis.clue_explanations.is_empty());
        let (mut traced, decisions) = capture_decisions(run);
        for comparison in &mut traced.planner.comparisons {
            comparison.basis = None;
        }
        assert_eq!(plain.planner, traced.planner);
        assert_eq!(
            plain.convention_analysis.inferences,
            traced.convention_analysis.inferences
        );
        assert!(!traced.convention_analysis.clue_explanations.is_empty());
        assert!(!decisions.is_empty());
        for decision in decisions {
            assert!(
                decision.view.hands[decision.view.observer.index()]
                    .iter()
                    .all(|card| card.identity.is_none())
            );
        }
        assert!(!enabled());
    }
}
