//! Hidden-information modeling and deterministic planning for Hanabi.

mod analysis;
mod control;
pub use control::{AnalysisControl, AnalysisStopped, CancellationToken};
mod baseline;
mod convention;
mod h_group;
mod information_set;
mod planner;

pub use analysis::{
    AnalyzePositionError, PositionAnalysis, analyze_position, analyze_position_with_control,
};
pub use baseline::{CardAssessment, ConventionAgnosticPolicy, PolicyError, assess_card};
pub use convention::{
    ConventionAction, ConventionActionReason, ConventionAnalysis, ConventionInferences,
    ConventionPolicyTier, ConventionRejectionReason, H_GROUP_RULESET_REVISION, HGroupLevel,
    HGroupProfile, ParseConventionError, ParseHGroupProfileError, RejectedConventionAction,
    SupportedConvention,
};
pub use h_group::{
    ActionPreference, ActionWindow, ConditionalAlternative, DependencyAssessment, DependencyStatus,
    H_GROUP_LEVELS, HGroupCardInference, HGroupClueInterpretation, HGroupClueKind,
    HGroupConnection, HGroupConnectionKind, HGroupConnectionPromise, HGroupIdentityStatus,
    HGroupInferences, HGroupLevelDescriptor, HGroupMoveKind, HGroupPhase, HGroupPlayObligation,
    HGroupSaveKind, HGroupSignal, HiddenCardCondition, PerspectiveAssumption, PlanFrontier,
    PlanStep, ProjectedAction, ProjectedConsequences, ProjectionEvidence, ProjectionRequirement,
    ProjectionRequirementKind, ResourceSchedule, TokenTransition, TurnCommitment, infer_h_group,
};
pub use information_set::{
    BeliefConstraints, EnumerateWorldsError, IdentitySet, InformationSet, InformationSetError,
    LogicalDeductions, WorldCount,
};
pub use planner::{
    CandidateComparison, ComparisonReason, EndpointComparison, ExactActionValue, ExactSearchStatus,
    ParsePlanningObjectiveError, PlannerActionEvaluation, PlannerConfig, PlannerError,
    PlannerPhase, PlannerResult, PlanningObjective, ProjectedPositionValue, SymbolicLineOutcome,
    SymbolicStopReason, plan_move,
};
