use std::collections::HashSet;

use hanabi_core::{Action, FullState, PlayerId, standard_deck};
use hanabi_search::{
    ActionEvaluation, ConventionAgnosticPolicy, InformationSet, MAX_TERMINAL_UTILITY,
    MonteCarloConfig, SearchError, evaluate_actions, evaluate_actions_with_diagnostics,
    select_best_action,
};

fn initial_information_set() -> InformationSet {
    let state = FullState::new_standard(2, standard_deck()).unwrap();
    InformationSet::new(state.view_for(PlayerId::new(0)).unwrap()).unwrap()
}

#[test]
fn evaluates_every_legal_action_on_the_same_number_of_worlds() {
    let information_set = initial_information_set();
    let legal_actions = information_set.view().legal_actions();
    let evaluations = evaluate_actions(
        &information_set,
        &ConventionAgnosticPolicy,
        MonteCarloConfig {
            samples_per_action: 8,
            seed: 867_5309,
            objective: hanabi_search::SearchObjective::ExpectedScore,
        },
    )
    .unwrap();

    assert_eq!(
        evaluations
            .iter()
            .map(|evaluation| evaluation.action)
            .collect::<Vec<_>>(),
        legal_actions
    );
    assert!(evaluations.iter().all(|evaluation| evaluation.samples == 8));
    assert!(evaluations.iter().all(|evaluation| {
        evaluation.min_score <= evaluation.max_score
            && (0.0..=25.0).contains(&evaluation.mean_score)
            && (0.0..=25.0).contains(&evaluation.mean_raw_score)
            && (0.0..=f64::from(MAX_TERMINAL_UTILITY)).contains(&evaluation.mean_utility)
            && evaluation.score_variance >= 0.0
            && (0.0..=1.0).contains(&evaluation.strikeout_rate)
    }));

    let unique_actions = evaluations
        .iter()
        .map(|evaluation| evaluation.action)
        .collect::<HashSet<_>>();
    assert_eq!(unique_actions.len(), evaluations.len());
}

#[test]
fn evaluation_is_reproducible_for_a_seed() {
    let information_set = initial_information_set();
    let config = MonteCarloConfig {
        samples_per_action: 12,
        seed: 42,
        objective: hanabi_search::SearchObjective::ExpectedScore,
    };

    assert_eq!(
        evaluate_actions(&information_set, &ConventionAgnosticPolicy, config).unwrap(),
        evaluate_actions(&information_set, &ConventionAgnosticPolicy, config).unwrap()
    );
}

#[test]
fn diagnostics_account_for_flat_search_work() {
    let information_set = initial_information_set();
    let action_count = information_set.view().legal_actions().len() as u64;
    let report = evaluate_actions_with_diagnostics(
        &information_set,
        &ConventionAgnosticPolicy,
        MonteCarloConfig {
            samples_per_action: 3,
            seed: 7,
            objective: hanabi_search::SearchObjective::ExpectedScore,
        },
    )
    .unwrap();
    let diagnostics = report.diagnostics;
    assert_eq!(
        report.evaluations,
        evaluate_actions(
            &information_set,
            &ConventionAgnosticPolicy,
            MonteCarloConfig {
                samples_per_action: 3,
                seed: 7,
                objective: hanabi_search::SearchObjective::ExpectedScore,
            },
        )
        .unwrap()
    );

    assert_eq!(diagnostics.worlds_sampled, 3);
    assert_eq!(diagnostics.candidate_state_clones, 3 * action_count);
    assert_eq!(diagnostics.tree_nodes_expanded, 0);
    assert_eq!(diagnostics.search_actions_applied, 3 * action_count);
    assert_eq!(diagnostics.rollouts, 3 * action_count);
    assert!(diagnostics.rollout_turns >= diagnostics.rollouts);
    assert_eq!(diagnostics.max_tree_depth, 1);
    assert_eq!(
        diagnostics.total_time,
        diagnostics.sampling_time + diagnostics.tree_time + diagnostics.rollout_time
    );
}

#[test]
fn rejects_a_zero_sample_budget() {
    assert_eq!(
        evaluate_actions(
            &initial_information_set(),
            &ConventionAgnosticPolicy,
            MonteCarloConfig {
                samples_per_action: 0,
                seed: 1,
                objective: hanabi_search::SearchObjective::ExpectedScore,
            },
        ),
        Err(SearchError::ZeroSamples)
    );
}

#[test]
fn best_action_uses_terminal_utility_and_stable_ties() {
    let first = Action::Play(hanabi_core::CardId::new(3));
    let second = Action::Discard(hanabi_core::CardId::new(4));
    let evaluations = [
        evaluation(first, 8.0, 9.0, 217.0),
        evaluation(second, 8.0, 10.0, 218.0),
        evaluation(first, 9.0, 9.0, 218.0),
    ];

    assert_eq!(select_best_action(&evaluations), Some(second));
    assert_eq!(select_best_action(&[]), None);
}

fn evaluation(
    action: Action,
    mean_score: f64,
    mean_raw_score: f64,
    mean_utility: f64,
) -> ActionEvaluation {
    ActionEvaluation {
        action,
        samples: 4,
        mean_score,
        mean_raw_score,
        mean_utility,
        perfect_rate: 0.0,
        mean_score_ceiling: 25.0,
        mean_clue_actions: 0.0,
        mean_clue_efficiency: 0.0,
        mean_tempo_clues: 0.0,
        mean_critical_discards: 0.0,
        mean_bottom_deck_risk: 0.0,
        mean_clue_debt: 0.0,
        mean_predictable_turns: 0.0,
        score_variance: 0.0,
        strikeout_rate: 0.0,
        min_score: 0,
        max_score: 0,
        principal_variation: vec![action],
    }
}
