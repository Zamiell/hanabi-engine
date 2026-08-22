use hanabi_core::{FullState, PlayerId, standard_deck};
use hanabi_search::{
    ConventionAgnosticPolicy, InformationSet, IsmctsConfig, IsmctsError, MAX_TERMINAL_UTILITY,
    ismcts_search, ismcts_search_with_diagnostics,
};

fn initial_information_set() -> InformationSet {
    let state = FullState::new_standard(2, standard_deck()).unwrap();
    InformationSet::new(state.view_for(PlayerId::new(0)).unwrap()).unwrap()
}

fn config(iterations: u32) -> IsmctsConfig {
    IsmctsConfig {
        iterations,
        exploration: core::f64::consts::SQRT_2,
        seed: 2026,
        objective: hanabi_search::SearchObjective::ExpectedScore,
    }
}

#[test]
fn root_statistics_conserve_iterations_and_action_availability() {
    let information_set = initial_information_set();
    let legal_actions = information_set.view().legal_actions();
    let result = ismcts_search(&information_set, &ConventionAgnosticPolicy, config(48)).unwrap();

    assert_eq!(result.iterations, 48);
    assert_eq!(
        result
            .root_actions
            .iter()
            .map(|statistics| statistics.action)
            .collect::<Vec<_>>(),
        legal_actions
    );
    assert_eq!(
        result
            .root_actions
            .iter()
            .map(|statistics| statistics.visits)
            .sum::<u32>(),
        result.iterations
    );
    assert!(
        result
            .root_actions
            .iter()
            .all(|statistics| statistics.availability == result.iterations)
    );
    assert!(legal_actions.contains(&result.best_action));
}

#[test]
fn search_is_reproducible_for_a_seed() {
    let information_set = initial_information_set();

    assert_eq!(
        ismcts_search(&information_set, &ConventionAgnosticPolicy, config(32)).unwrap(),
        ismcts_search(&information_set, &ConventionAgnosticPolicy, config(32)).unwrap()
    );
}

#[test]
fn diagnostics_account_for_tree_search_work() {
    let information_set = initial_information_set();
    let report =
        ismcts_search_with_diagnostics(&information_set, &ConventionAgnosticPolicy, config(32))
            .unwrap();
    let diagnostics = report.diagnostics;

    assert_eq!(
        report.result,
        ismcts_search(&information_set, &ConventionAgnosticPolicy, config(32)).unwrap()
    );
    assert_eq!(diagnostics.worlds_sampled, 32);
    assert_eq!(diagnostics.candidate_state_clones, 0);
    assert_eq!(diagnostics.tree_nodes_expanded, diagnostics.rollouts);
    assert!(diagnostics.search_actions_applied >= diagnostics.tree_nodes_expanded);
    assert!(diagnostics.rollouts > 0);
    assert!(diagnostics.rollout_turns >= diagnostics.rollouts);
    assert!(diagnostics.max_tree_depth > 0);
    assert_eq!(
        diagnostics.total_time,
        diagnostics.sampling_time + diagnostics.tree_time + diagnostics.rollout_time
    );
}

#[test]
fn visits_are_backpropagated_beyond_initial_expansion() {
    let result = ismcts_search(
        &initial_information_set(),
        &ConventionAgnosticPolicy,
        config(64),
    )
    .unwrap();

    assert!(
        result
            .root_actions
            .iter()
            .all(|statistics| statistics.visits > 0)
    );
    assert!(
        result
            .root_actions
            .iter()
            .any(|statistics| statistics.visits > 1)
    );
    assert!(result.root_actions.iter().all(|statistics| {
        statistics
            .mean_score
            .is_some_and(|mean| (0.0..=25.0).contains(&mean))
            && statistics
                .mean_raw_score
                .is_some_and(|mean| (0.0..=25.0).contains(&mean))
            && statistics
                .mean_utility
                .is_some_and(|mean| (0.0..=f64::from(MAX_TERMINAL_UTILITY)).contains(&mean))
            && statistics
                .strikeout_rate
                .is_some_and(|rate| (0.0..=1.0).contains(&rate))
            && statistics.min_score <= statistics.max_score
    }));
}

#[test]
fn rejects_invalid_search_configuration() {
    let information_set = initial_information_set();
    assert_eq!(
        ismcts_search(&information_set, &ConventionAgnosticPolicy, config(0)),
        Err(IsmctsError::ZeroIterations)
    );
    assert_eq!(
        ismcts_search(
            &information_set,
            &ConventionAgnosticPolicy,
            IsmctsConfig {
                iterations: 1,
                exploration: -0.5,
                seed: 0,
                objective: hanabi_search::SearchObjective::ExpectedScore,
            },
        ),
        Err(IsmctsError::InvalidExploration(-0.5))
    );
}
