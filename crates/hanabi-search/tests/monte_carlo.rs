use std::collections::HashSet;

use hanabi_core::{Action, FullState, PlayerId, standard_deck};
use hanabi_search::{
    ActionEvaluation, ConventionAgnosticPolicy, InformationSet, MonteCarloConfig, SearchError,
    evaluate_actions, select_best_action,
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
    };

    assert_eq!(
        evaluate_actions(&information_set, &ConventionAgnosticPolicy, config).unwrap(),
        evaluate_actions(&information_set, &ConventionAgnosticPolicy, config).unwrap()
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
            },
        ),
        Err(SearchError::ZeroSamples)
    );
}

#[test]
fn best_action_uses_mean_score_and_stable_ties() {
    let first = Action::Play(hanabi_core::CardId::new(3));
    let second = Action::Discard(hanabi_core::CardId::new(4));
    let evaluations = [
        evaluation(first, 7.5),
        evaluation(second, 9.0),
        evaluation(first, 9.0),
    ];

    assert_eq!(select_best_action(&evaluations), Some(second));
    assert_eq!(select_best_action(&[]), None);
}

fn evaluation(action: Action, mean_score: f64) -> ActionEvaluation {
    ActionEvaluation {
        action,
        samples: 4,
        mean_score,
        score_variance: 0.0,
        strikeout_rate: 0.0,
        min_score: 0,
        max_score: 0,
    }
}
