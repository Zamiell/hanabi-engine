//! Hidden-information modeling and deterministic planning for Hanabi.

mod analysis;
mod baseline;
mod convention;
mod h_group;
mod information_set;
mod planner;

pub use analysis::{AnalyzePositionError, PositionAnalysis, analyze_position};
pub use baseline::{CardAssessment, ConventionAgnosticPolicy, PolicyError, assess_card};
pub use convention::{
    ConventionAction, ConventionActionReason, ConventionAnalysis, ConventionInferences,
    ConventionRejectionReason, H_GROUP_RULESET_REVISION, HGroupLevel, HGroupProfile,
    ParseConventionError, ParseHGroupProfileError, RejectedConventionAction, SupportedConvention,
};
pub use h_group::{
    H_GROUP_DOCUMENTATION_SECTIONS, H_GROUP_LEVELS, HGroupCardInference, HGroupClueInterpretation,
    HGroupClueKind, HGroupConnection, HGroupConnectionKind, HGroupConnectionPromise,
    HGroupDocumentationSection, HGroupInferences, HGroupLevelDescriptor, HGroupMoveKind,
    HGroupPhase, HGroupSaveKind, HGroupSignal, infer_h_group,
};
pub use information_set::{
    BeliefConstraints, EnumerateWorldsError, IdentitySet, InformationSet, InformationSetError,
    LogicalDeductions, WorldCount,
};
pub use planner::{
    ExactActionValue, ParsePlanningObjectiveError, PlannerActionEvaluation, PlannerConfig,
    PlannerError, PlannerPhase, PlannerResult, PlanningObjective, SymbolicLineOutcome, plan_move,
};
