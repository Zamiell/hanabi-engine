//! Hidden-information modeling and search policies for Hanabi.

pub mod analysis;
pub mod baseline;
pub mod convention;
pub mod diagnostics;
pub mod evaluation;
pub mod h_group;
pub mod information_set;
pub mod ismcts;
pub mod monte_carlo;
pub mod rollout;

pub use analysis::{BestMove, BestMoveError, SearchConfig, SearchDetails, best_move};
pub use baseline::{
    CardAssessment, ConventionAgnosticPolicy, PolicyError, RolloutPolicy, assess_card,
};
pub use convention::{
    CONVENTION_DESCRIPTORS, ConventionDescriptor, ConventionFramework, ConventionInferences,
    H_GROUP_RULESET_REVISION, HGroupLevel, HGroupProfile, ParseConventionError,
    ParseHGroupProfileError, SupportedConvention,
};
pub use diagnostics::SearchDiagnostics;
pub use evaluation::{ParseSearchObjectiveError, SearchObjective, StrategicMetrics, score_ceiling};
pub use h_group::{
    H_GROUP_LEVELS, HGroupClueInterpretation, HGroupClueKind, HGroupConnection,
    HGroupConnectionKind, HGroupConnectionPromise, HGroupInferences, HGroupLevelDescriptor,
    HGroupMoveKind, HGroupPhase, HGroupSaveKind, HGroupSignal, enabled_h_group_levels,
    infer_h_group,
};
pub use information_set::{
    IdentitySet, InformationSet, InformationSetError, LogicalDeductions, PolicyDeductions,
    SampleError,
};
pub use ismcts::{
    IsmctsConfig, IsmctsError, IsmctsReport, IsmctsResult, TreeActionStatistics, ismcts_search,
    ismcts_search_with_diagnostics,
};
pub use monte_carlo::{
    ActionEvaluation, MonteCarloConfig, MonteCarloReport, SearchError, evaluate_actions,
    evaluate_actions_with_diagnostics, select_best_action,
};
pub use rollout::{
    MAX_ROLLOUT_TURNS, MAX_TERMINAL_UTILITY, OFFICIAL_SCORE_UTILITY_WEIGHT, RolloutDiagnostics,
    RolloutError, RolloutOutcome, RolloutReport, rollout_to_terminal,
    rollout_to_terminal_with_diagnostics, terminal_utility,
};
