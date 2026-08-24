use core::fmt;

use hanabi_core::PlayerView;

use crate::{
    ConventionAnalysis, InformationSet, InformationSetError, PlannerConfig, PlannerError,
    PlannerResult, SupportedConvention, planner::plan_move_with_analysis,
};

/// Complete, internally consistent analysis of one player observation.
#[derive(Clone, Debug, PartialEq)]
pub struct PositionAnalysis {
    pub convention: SupportedConvention,
    pub information: InformationSet,
    pub convention_analysis: ConventionAnalysis,
    pub planner: PlannerResult,
}

/// Analyzes the best move visible to the acting player in `view`.
///
/// This is the high-level convention-safe entry point for applications. It
/// derives logical information and supplies the selected
/// [`SupportedConvention`] to the deterministic planner.
///
/// # Errors
///
/// Returns [`AnalyzePositionError`] if the observation has no consistent information
/// set, is not actionable, or planning fails.
pub fn analyze_position(
    view: &PlayerView,
    convention: SupportedConvention,
    config: PlannerConfig,
) -> Result<PositionAnalysis, AnalyzePositionError> {
    let information = InformationSet::new(view).map_err(AnalyzePositionError::InformationSet)?;
    let convention_analysis = convention.analyze(information.deductions());
    let planner = plan_move_with_analysis(&information, convention, &convention_analysis, config)
        .map_err(AnalyzePositionError::Planner)?;
    Ok(PositionAnalysis {
        convention,
        information,
        convention_analysis,
        planner,
    })
}

/// Why a high-level best-move request could not be completed.
#[derive(Debug, PartialEq)]
pub enum AnalyzePositionError {
    InformationSet(InformationSetError),
    Planner(PlannerError),
}

impl fmt::Display for AnalyzePositionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InformationSet(error) => write!(formatter, "invalid observation: {error}"),
            Self::Planner(error) => write!(formatter, "planner failed: {error}"),
        }
    }
}

impl std::error::Error for AnalyzePositionError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::InformationSet(error) => Some(error),
            Self::Planner(error) => Some(error),
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
    fn arbitrary_views_use_the_planner_with_selected_convention() {
        let result = analyze_position(
            &initial_view(),
            SupportedConvention::None,
            PlannerConfig::default(),
        )
        .unwrap();
        assert_eq!(result.convention, SupportedConvention::None);

        let h_group = SupportedConvention::HGroup(HGroupProfile::Level(HGroupLevel::Level4));
        let result = analyze_position(&initial_view(), h_group, PlannerConfig::default()).unwrap();
        assert_eq!(result.convention, h_group);
    }
}
