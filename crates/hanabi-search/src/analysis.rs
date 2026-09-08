use core::fmt;

use hanabi_core::PlayerView;

use crate::{
    AnalysisControl, AnalysisStopped, ConventionAnalysis, InformationSet, InformationSetError,
    PlannerConfig, PlannerError, PlannerResult, SupportedConvention,
    planner::plan_move_with_control,
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
    analyze_position_with_control(view, convention, config, &AnalysisControl::default())
}

/// Cooperative counterpart to [`analyze_position`]. No move is returned if
/// cancellation or a request-wide budget interrupts candidate evaluation.
///
/// # Errors
/// Returns an analysis error or the specific cancellation/budget reason.
pub fn analyze_position_with_control(
    view: &PlayerView,
    convention: SupportedConvention,
    config: PlannerConfig,
    control: &AnalysisControl,
) -> Result<PositionAnalysis, AnalyzePositionError> {
    control
        .checkpoint()
        .map_err(AnalyzePositionError::Stopped)?;
    let information = InformationSet::new(view).map_err(AnalyzePositionError::InformationSet)?;
    control
        .checkpoint()
        .map_err(AnalyzePositionError::Stopped)?;
    let convention_analysis = convention.analyze(information.deductions());
    control
        .checkpoint()
        .map_err(AnalyzePositionError::Stopped)?;
    let planner = plan_move_with_control(
        &information,
        convention,
        &convention_analysis,
        config,
        control,
    )
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
    Stopped(AnalysisStopped),
    InformationSet(InformationSetError),
    Planner(PlannerError),
}

impl fmt::Display for AnalyzePositionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Stopped(error) => error.fmt(formatter),
            Self::InformationSet(error) => write!(formatter, "invalid observation: {error}"),
            Self::Planner(error) => write!(formatter, "planner failed: {error}"),
        }
    }
}

impl std::error::Error for AnalyzePositionError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Stopped(error) => Some(error),
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
    fn cancellation_and_limits_never_return_partial_decisions() {
        let token = crate::CancellationToken::default();
        token.clone().cancel();
        let cancelled = AnalysisControl::new(token, None, u64::MAX);
        let expired = AnalysisControl::new(
            crate::CancellationToken::default(),
            Some(std::time::Instant::now()),
            u64::MAX,
        );
        let limited = AnalysisControl::new(crate::CancellationToken::default(), None, 0);
        for (control, reason) in [
            (&cancelled, AnalysisStopped::Cancelled),
            (&expired, AnalysisStopped::Deadline),
            (&limited, AnalysisStopped::WorkLimit),
        ] {
            assert_eq!(
                analyze_position_with_control(
                    &initial_view(),
                    SupportedConvention::None,
                    PlannerConfig::default(),
                    control
                ),
                Err(AnalyzePositionError::Stopped(reason))
            );
        }
        let control = AnalysisControl::default();
        let controlled = analyze_position_with_control(
            &initial_view(),
            SupportedConvention::None,
            PlannerConfig::default(),
            &control,
        )
        .unwrap();
        assert_eq!(
            controlled,
            analyze_position(
                &initial_view(),
                SupportedConvention::None,
                PlannerConfig::default()
            )
            .unwrap()
        );
        assert!(
            control.used() > 3,
            "counting and planning share the request's work counter"
        );
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
