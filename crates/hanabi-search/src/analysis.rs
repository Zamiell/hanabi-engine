use core::fmt;

use hanabi_core::{Action, PlayerView};

use crate::{
    ActionEvaluation, InformationSet, InformationSetError, IsmctsConfig, IsmctsError, IsmctsResult,
    MonteCarloConfig, SearchError, SupportedConvention, evaluate_actions, ismcts_search,
    select_best_action,
};

/// Search algorithm and reproducible budget for a best-move request.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum SearchConfig {
    Ismcts(IsmctsConfig),
    Flat(MonteCarloConfig),
}

/// Algorithm-specific evidence supporting a best-move result.
#[derive(Clone, Debug, PartialEq)]
pub enum SearchDetails {
    Ismcts(IsmctsResult),
    Flat(Vec<ActionEvaluation>),
}

/// Best move for an arbitrary legal player observation under one selected
/// convention framework.
#[derive(Clone, Debug, PartialEq)]
pub struct BestMove {
    pub convention: SupportedConvention,
    pub objective: crate::SearchObjective,
    pub action: Action,
    pub details: SearchDetails,
}

/// Finds the best move visible to the acting player in `view`.
///
/// This is the high-level convention-safe entry point for applications. It
/// derives logical information, dispatches the selected search algorithm, and
/// supplies the selected [`SupportedConvention`] to both belief sampling and
/// rollout decisions.
///
/// # Errors
///
/// Returns [`BestMoveError`] if the observation has no consistent information
/// set, is not actionable, cannot be sampled, or search fails.
pub fn best_move(
    view: PlayerView,
    convention: SupportedConvention,
    config: SearchConfig,
) -> Result<BestMove, BestMoveError> {
    let information_set = InformationSet::new(view).map_err(BestMoveError::InformationSet)?;
    match config {
        SearchConfig::Ismcts(config) => {
            let result = ismcts_search(&information_set, &convention, config)
                .map_err(BestMoveError::Ismcts)?;
            Ok(BestMove {
                convention,
                objective: config.objective,
                action: result.best_action,
                details: SearchDetails::Ismcts(result),
            })
        }
        SearchConfig::Flat(config) => {
            let evaluations = evaluate_actions(&information_set, &convention, config)
                .map_err(BestMoveError::Flat)?;
            let action = select_best_action(&evaluations).ok_or(BestMoveError::NoBestAction)?;
            Ok(BestMove {
                convention,
                objective: config.objective,
                action,
                details: SearchDetails::Flat(evaluations),
            })
        }
    }
}

/// Why a high-level best-move request could not be completed.
#[derive(Debug, PartialEq)]
pub enum BestMoveError {
    InformationSet(InformationSetError),
    Ismcts(IsmctsError),
    Flat(SearchError),
    NoBestAction,
}

impl fmt::Display for BestMoveError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InformationSet(error) => write!(formatter, "invalid observation: {error}"),
            Self::Ismcts(error) => write!(formatter, "ISMCTS failed: {error}"),
            Self::Flat(error) => write!(formatter, "flat search failed: {error}"),
            Self::NoBestAction => formatter.write_str("search returned no best action"),
        }
    }
}

impl std::error::Error for BestMoveError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::InformationSet(error) => Some(error),
            Self::Ismcts(error) => Some(error),
            Self::Flat(error) => Some(error),
            Self::NoBestAction => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{HGroupLevel, HGroupProfile};
    use hanabi_core::{FullState, PlayerId, standard_deck};

    fn initial_view() -> PlayerView {
        FullState::new_standard(2, standard_deck())
            .unwrap()
            .view_for(PlayerId::new(0))
            .unwrap()
    }

    #[test]
    fn arbitrary_views_dispatch_both_search_modes_with_selected_convention() {
        let ismcts = best_move(
            initial_view(),
            SupportedConvention::None,
            SearchConfig::Ismcts(IsmctsConfig {
                iterations: 8,
                exploration: core::f64::consts::SQRT_2,
                seed: 42,
                objective: crate::SearchObjective::ExpectedScore,
            }),
        )
        .unwrap();
        assert_eq!(ismcts.convention, SupportedConvention::None);
        assert!(matches!(ismcts.details, SearchDetails::Ismcts(_)));

        let flat = best_move(
            initial_view(),
            SupportedConvention::None,
            SearchConfig::Flat(MonteCarloConfig {
                samples_per_action: 2,
                seed: 42,
                objective: crate::SearchObjective::ExpectedScore,
            }),
        )
        .unwrap();
        assert_eq!(flat.convention, SupportedConvention::None);
        assert!(matches!(flat.details, SearchDetails::Flat(_)));

        let h_group = SupportedConvention::HGroup(HGroupProfile::Level(HGroupLevel::Level4));
        let result = best_move(
            initial_view(),
            h_group,
            SearchConfig::Ismcts(IsmctsConfig {
                iterations: 1,
                exploration: core::f64::consts::SQRT_2,
                seed: 42,
                objective: crate::SearchObjective::ExpectedScore,
            }),
        )
        .unwrap();
        assert_eq!(result.convention, h_group);
    }
}
