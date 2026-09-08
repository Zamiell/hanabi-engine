//! H-Group convention inference.
//!
//! This module deliberately contains interpretations, not game rules or
//! logical clue facts. H-Group profiles are cumulative: a level-N profile
//! enables every interpretation through level N, while `max` also enables the
//! rare moves in the extras chapters of the pinned ruleset.

use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::{Arc, OnceLock};

use hanabi_core::{
    Action, Card, CardId, Clue, ClueFacts, GameStatus, MAX_CLUE_TOKENS, ObservedCard,
    ObservedEvent, ObservedHistoryEntry, PlayerId, PlayerView, Rank, Suit,
};

use crate::{
    BeliefConstraints, ConventionActionReason, ConventionRejectionReason, HGroupLevel,
    HGroupProfile, IdentitySet, LogicalDeductions, RejectedConventionAction,
};

mod action_analysis;
mod action_preference;
mod action_schedule;
mod admission;
mod bluff;
mod candidate;
mod candidate_pipeline;
mod claims;
mod compiled_line;
mod connection;
mod constraint_graph;
mod constraints;
#[cfg(test)]
mod coverage;
mod decision;
mod distribution;
mod draw_distribution;
mod effects;
mod epistemic;
mod event_reducer;
mod facts;
mod frontier_value;
mod hand;
mod history_reducer;
mod hypothesis;
mod identity;
mod information_value;
mod interpretation;
mod interpretation_resolution;
mod knowledge_effects;
mod ledger;
mod line_state;
mod model;
mod outcome;
mod perspective;
mod plan;
mod play_order;
mod primary;
mod projection_requirements;
mod prospective;
use history_reducer::replay_h_group_inner_uncached;
mod public_layers;
mod rationality;
mod recognition;
mod rule_engine;
mod rules;
mod strategic_value;
mod symbolic_line;
mod transition;
mod turn_context;

use action_analysis::{CompiledHGroupAction, HGroupActionKind, HGroupActionSet};
pub use action_preference::ActionPreference;
use action_preference::TerminalPlanProgress;
use action_schedule::{ActionSchedule, StackTimeline};
pub use action_schedule::{ActionWindow, TurnCommitment};
use bluff::{
    BluffTargetKind, bluff_play_connects, bluff_target_kind_at, bluff_target_order_is_legal,
};
use candidate::{CluePurpose, ClueRecognition, ClueSchedule, ClueValue, CompiledClueAction};
use candidate_pipeline::SemanticallyAdmittedCandidates;
use claims::{IdentityClaims, claimed_identities_at_clue};
use connection::{
    ConnectionClueMatch, ConnectionManager, ConnectionObligation, ConnectionTransitionReason,
    PromiseId,
};
use constraint_graph::ConventionConstraintGraph;
use constraints::{ConventionConstraints, ConventionRequirementKind};
pub(crate) use decision::analyze_h_group_convention;
pub use decision::infer_h_group;
#[cfg(test)]
use decision::{h_group_predictable_action, ordered_h_group_actions};
use decision::{infer_h_group_from_replay, preferred_due_play_card, select_h_group_action};
use effects::{ConventionJournal, ConventionReducer, EffectBatch, SignalHistory};
use epistemic::{EpistemicState, owner_knowledge_read_model};
use event_reducer::HGroupRuleEffects;
use facts::{ConventionFacts, DeclinedAlternativeInference, IdentityClaimRelation};
use hand::{
    chop, finesse_position, finesse_position_id, five_chop_moved_card, five_pulled_card, focus,
    is_critical, remove_card,
};
use hypothesis::{InterpretationHypotheses, InterpretationSource};
use identity::{
    card_is_trash, identity_of, is_card_identity_accounted_trash, is_convention_trash,
    is_eventually_useful, is_playable_at, is_playable_now, is_trash_at,
};
use information_value::convention_information_value;
#[cfg(test)]
use interpretation::h_group_clue_candidates;
use interpretation::{
    build_convention_knowledge, convention_card_inferences, elimination_finesse_card,
    elimination_finesse_connection, h_group_clue_candidates_from_replay,
    h_group_rejected_clues_from_replay, infer_clue_to_self, loaded_connection_plan,
    recipient_replay_assessment, snapshot_good_touch_identities, snapshot_play_identities,
    snapshot_save_identities,
};
use knowledge_effects::{CardKnowledgeEffect, ConventionKnowledge, KnowledgeSource};
use ledger::{
    ConventionCardSetSnapshot, EffectSource, ProvenancedCardSet,
    reconcile_connection_fact_lifecycles,
};
use model::{
    CardSet, ClueConnectionStep, ClueInterpretationHypothesis, CompactIdHasher,
    ConventionCardState, FixCondition, FixObligations, HGroupState, PerspectiveDepth, PlayerSet,
    RequiredFix, active_invisibly_clued, protected_cards,
};
pub use model::{
    HGroupCardInference, HGroupClueInterpretation, HGroupClueKind, HGroupConnection,
    HGroupConnectionKind, HGroupConnectionPromise, HGroupIdentityStatus, HGroupInferences,
    HGroupPhase, HGroupPlayObligation, HGroupSaveKind, HGroupSignal,
};
use outcome::{
    ActionCommitment, CluedCardSuperposition, LineOutcome, RecipientCardConsequence,
    RecipientCardDisposition,
};
use perspective::{PerspectiveProjector, ProspectiveTransition};
use plan::ConditionalPlan;
pub use plan::{
    ConditionalAlternative, HiddenCardCondition, PerspectiveAssumption, PlanFrontier, PlanStep,
    ProjectedAction, ProjectedConsequences, ProjectionEvidence, ResourceSchedule, TokenTransition,
};
use play_order::ordered_playable_cards;
use primary::{ClueInterpretationPlan, PrimaryClueInputs};
pub use projection_requirements::{
    DependencyAssessment, DependencyStatus, ProjectionRequirement, ProjectionRequirementKind,
};
#[cfg(test)]
use prospective::prospective_clue_hazard;
use prospective::{
    CompiledObserverProjection, CompiledProspectiveClue, SubjectiveReplayRequest,
    TeamConventionSnapshot, compiled_baseline_team, compiled_prospective_clue,
    projected_h_group_replay, prospective_clue_has_unsafe_connection,
    prospective_clue_marks_focus_saved, prospective_clue_primary_interpretation,
    prospective_clue_primary_kind, prospective_clue_signal_kinds, prospective_clue_view,
    prospective_play_has_unsafe_inference, prospective_play_view,
    prospective_stacked_ejection_card, prospective_team_clue_signal_kinds,
    subjective_action_context_before, subjective_convention_cards, subjective_playable_cards,
    with_prospective_analysis_cache,
};
use rationality::{DeclinedAlternativeContext, declined_superior_clue_inferences};
use rule_engine::{RuleExecutionContext, apply_post_event_rules};
use rules::{HGroupRuleId, RulePhase, rule_enabled};
use strategic_value::apply_strategic_clue_values;
pub(crate) use symbolic_line::project_h_group_projection;
#[cfg(test)]
use transition::FactChangeKind;
use transition::{
    ConventionTransitionDelta, ConventionTransitionResult, MaterializedCardFact, MutationDomain,
    MutationSet, RuleProposal,
};
use turn_context::{
    ActorBeliefBefore, HGroupTurnContext, HGroupTurnSnapshot, HGroupTurnView, HistoricalView,
};

const KNOWN_TRASH_COLLATERAL_BONUS: u16 = 80;

/// Semantic families used by the cumulative H-Group interpreter.
///
/// The documentation gives many combinations their own names. The engine
/// represents combinations as a sequence of these primitive effects instead
/// of duplicating state-transition code for every name. For example, a Trash
/// Push Finesse is represented by `TrashPush` followed by `Finesse`.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
#[non_exhaustive]
pub enum HGroupMoveKind {
    /// [Play Clues](https://hanabi.github.io/beginner/play-clues/)
    PlayClue,
    /// [Save Clues](https://hanabi.github.io/beginner/save-clues/)
    SaveClue,
    /// [5 Stall](https://hanabi.github.io/level-2/#the-5-stall-cluing-off-chop-5s)
    FiveStall,
    /// [Prompt](https://hanabi.github.io/level-1/#the-prompt)
    Prompt,
    /// [Finesse](https://hanabi.github.io/level-1/#the-finesse)
    Finesse,
    /// [Reverse Finesse](https://hanabi.github.io/level-2/#the-reverse-finesse)
    ReverseFinesse,
    /// [Self-Finesse](https://hanabi.github.io/level-2/#the-self-finesse)
    SelfFinesse,
    /// [Layered Finesse](https://hanabi.github.io/level-5/#the-layered-finesse)
    LayeredFinesse,
    /// [Hidden Finesse](https://hanabi.github.io/level-5/#the-hidden-finesse)
    HiddenFinesse,
    /// [Clandestine Finesse](https://hanabi.github.io/level-5/#the-clandestine-finesse)
    ClandestineFinesse,
    /// [Queued Finesse](https://hanabi.github.io/level-5/#the-queued-finesse)
    QueuedFinesse,
    /// [Ambiguous Finesse](https://hanabi.github.io/level-5/#the-ambiguous-finesse)
    AmbiguousFinesse,
    /// [Fix Clue](https://hanabi.github.io/level-3/#the-fix-clue)
    FixClue,
    /// [Sarcastic Discard](https://hanabi.github.io/level-3/#the-sarcastic-discard-sd)
    SarcasticDiscard,
    /// [Chop Moves](https://hanabi.github.io/level-4/#chop-moves)
    ChopMove,
    /// [Order Chop Move](https://hanabi.github.io/level-4/#the-order-chop-move-ocm)
    OrderChopMove,
    /// [Trash Chop Move](https://hanabi.github.io/level-4/#the-trash-chop-move-tcm)
    TrashChopMove,
    /// [5's Chop Move](https://hanabi.github.io/level-4/#the-5s-chop-move-5cm)
    FiveChopMove,
    /// [Tempo Clue](https://hanabi.github.io/level-6/#the-tempo-clue)
    TempoClue,
    /// [Tempo Clue Chop Move](https://hanabi.github.io/level-6/#the-tempo-clue-chop-move-tccm)
    TempoClueChopMove,
    /// [Scream Discard Chop Move](https://hanabi.github.io/level-7/#the-scream-discard-chop-move-sdcm)
    ScreamDiscard,
    /// [Shout Discard Chop Move](https://hanabi.github.io/level-7/#the-shout-discard-chop-move)
    ShoutDiscard,
    /// [Generation Discard](https://hanabi.github.io/level-7/#the-generation-discard)
    GenerationDiscard,
    /// [Positional Discard](https://hanabi.github.io/level-8/#the-positional-discard-indicating-a-play-with-a-discard)
    PositionalDiscard,
    /// [Positional Misplay](https://hanabi.github.io/level-8/#the-positional-misplay-indicating-a-play-with-a-misplay)
    PositionalMisplay,
    /// [Double Positional Misplay](https://hanabi.github.io/level-8/#the-double-positional-misplay-indicating-two-plays-with-a-misplay)
    DoublePositionalMisplay,
    /// [Distribution Clue](https://hanabi.github.io/level-8/#the-distribution-clue)
    DistributionClue,
    /// [Stalling Situations](https://hanabi.github.io/level-9/#stalling-situations)
    Stall,
    /// [Early Game Stall](https://hanabi.github.io/level-9/#the-early-game-severity-1-stalling)
    EarlyGameStall,
    /// [Double Discard Avoidance](https://hanabi.github.io/level-9/#double-discard-situations--double-discard-avoidance-dda-severity-2-stalling)
    DoubleDiscardAvoidance,
    /// [Locked Hand Save](https://hanabi.github.io/level-9/#the-locked-hand-save-lhs)
    LockedHandSave,
    /// [Fill-In Clue](https://hanabi.github.io/level-9/#the-fill-in-clue)
    FillInClue,
    /// [Anxiety Play](https://hanabi.github.io/level-9/#the-anxiety-play-forcing-a-locked-player-to-play)
    AnxietyPlay,
    /// [8 Clue Save](https://hanabi.github.io/level-9/#the-8-clue-save-8cs)
    EightClueSave,
    /// [Burn](https://hanabi.github.io/level-8/#burning-end-game-stalling)
    Burn,
    /// [Gentleman's and Baton Discards](https://hanabi.github.io/level-10/#the-gentlemans-discard-gd)
    TransferDiscard,
    /// [Gentleman's Discard](https://hanabi.github.io/level-10/#the-gentlemans-discard-gd)
    GentlemansDiscard,
    /// [Layered Gentleman's Discard](https://hanabi.github.io/level-10/#the-layered-gentlemans-discard)
    LayeredGentlemansDiscard,
    /// [Baton Discard](https://hanabi.github.io/level-10/#the-baton-discard-bd)
    BatonDiscard,
    /// [Sarcastic Finesse](https://hanabi.github.io/level-10/#the-sarcastic-finesse)
    SarcasticFinesse,
    /// [Certain Finesse](https://hanabi.github.io/level-10/#the-certain-finesse--the-certain-discard)
    CertainFinesse,
    /// [Certain Discard](https://hanabi.github.io/level-10/#the-certain-finesse--the-certain-discard)
    CertainDiscard,
    /// [Composition Finesse](https://hanabi.github.io/level-10/#the-composition-finesse)
    CompositionFinesse,
    /// [Clarity Principle](https://hanabi.github.io/level-6/#clarity-principle-part-1)
    Clarity,
    /// [Bluff](https://hanabi.github.io/level-11/#the-bluff)
    Bluff,
    /// [Self-Bluff](https://hanabi.github.io/level-11/#the-self-bluff)
    SelfBluff,
    /// [3 Bluff](https://hanabi.github.io/level-13/#the-3-bluff)
    ThreeBluff,
    /// [Critical Color Bluff](https://hanabi.github.io/level-13/#the-critical-color-bluff-ccb)
    CriticalColorBluff,
    /// [Hard Bluff](https://hanabi.github.io/level-13/#the-hard-bluff)
    HardBluff,
    /// [Good Touch Bluff](https://hanabi.github.io/level-13/#the-good-touch-bluff)
    GoodTouchBluff,
    /// [Double Bluff](https://hanabi.github.io/level-15/#the-double-bluff)
    DoubleBluff,
    /// [Hard Double Bluff](https://hanabi.github.io/level-15/#the-hard-double-bluff)
    HardDoubleBluff,
    /// [Pestilent Double Bluff](https://hanabi.github.io/level-15/#the-pestilent-double-bluff-pdb)
    PestilentDoubleBluff,
    /// [Selfish Clue](https://hanabi.github.io/level-12/#the-selfish-clue)
    SelfishClue,
    /// [Selfish Finesse](https://hanabi.github.io/level-12/#the-selfish-finesse-a-finesse-through-your-own-hand)
    SelfishFinesse,
    /// [Stale 1's Clue](https://hanabi.github.io/level-12/#the-stale-1s-clue)
    StaleOnesClue,
    /// [Focus Inversion](https://hanabi.github.io/level-12/#focus-inversion)
    FocusInversion,
    /// [Context](https://hanabi.github.io/level-12/#context)
    Context,
    /// [Trash Push](https://hanabi.github.io/level-14/#the-trash-push)
    TrashPush,
    /// [Trash Push Prompt](https://hanabi.github.io/level-14/#the-trash-push-prompt--the-trash-push-finesse)
    TrashPushPrompt,
    /// [Trash Push Finesse](https://hanabi.github.io/level-14/#the-trash-push-prompt--the-trash-push-finesse)
    TrashPushFinesse,
    /// [Trash Finesse](https://hanabi.github.io/level-14/#the-trash-finesse)
    TrashFinesse,
    /// [Reverse Trash Finesse](https://hanabi.github.io/level-14/#the-reverse-trash-finesse)
    ReverseTrashFinesse,
    /// [Forced Gentleman's Discard Chop Move](https://hanabi.github.io/level-14/#the-forced-gentlemans-discard-chop-move)
    ForcedGentlemansDiscardChopMove,
    /// [Trash Bluff](https://hanabi.github.io/level-14/#the-trash-bluff)
    TrashBluff,
    /// [Trash Order Chop Move](https://hanabi.github.io/level-14/#the-trash-order-chop-move-tocm)
    TrashOrderChopMove,
    /// [Ejections](https://hanabi.github.io/level-16/#ejections)
    Ejection,
    /// [Discharges](https://hanabi.github.io/level-16/#discharges)
    Discharge,
    /// [5 Color Ejection](https://hanabi.github.io/level-16/#the-5-color-ejection-5ce)
    FiveColorEjection,
    /// [Unknown Trash Discharge](https://hanabi.github.io/level-16/#the-unknown-trash-discharge-1-for-1-form-utd)
    UnknownTrashDischarge,
    /// [Unknown Dupe Discharge](https://hanabi.github.io/level-16/#the-unknown-dupe-discharge-udd)
    UnknownDupeDischarge,
    /// [Dupe Tech](https://hanabi.github.io/level-17/#the-duplicitous-value-clue)
    Duplication,
    /// [Duplicitous Value Clue](https://hanabi.github.io/level-17/#the-duplicitous-value-clue)
    DuplicitousValue,
    /// [Duplicitous Blind-Play](https://hanabi.github.io/level-17/#the-duplicitous-blind-play)
    DuplicitousBlindPlay,
    /// [Duplicitous Tempo Clue](https://hanabi.github.io/level-17/#the-duplicitous-tempo-clue)
    DuplicitousTempo,
    /// [Assisted Trash Chop Move](https://hanabi.github.io/level-17/#the-assisted-trash-chop-move)
    AssistedTrashChopMove,
    /// [Time Travel Chop Move](https://hanabi.github.io/level-17/#the-time-travel-chop-move-direct-form)
    TimeTravelChopMove,
    /// [Elimination](https://hanabi.github.io/level-18/#elimination--elimination-notes)
    Elimination,
    /// [Elimination Finesse](https://hanabi.github.io/level-18/#the-elimination-finesse)
    EliminationFinesse,
    /// [Elimination Blind-Play](https://hanabi.github.io/level-18/#the-elimination-blind-play)
    EliminationBlindPlay,
    /// [Elimination Play Clue](https://hanabi.github.io/level-18/#the-elimination-play-clue)
    EliminationPlayClue,
    /// [Elimination Riding Deduction](https://hanabi.github.io/level-18/#the-elimination-riding-deduction)
    EliminationRiding,
    /// [Elimination Self-Chop Move](https://hanabi.github.io/level-18/#the-elimination-self-chop-move)
    EliminationSelfChopMove,
    /// [Trash Touch Elimination](https://hanabi.github.io/level-18/#trash-touch-elimination-tte)
    TrashTouchElimination,
    /// [5 Pull](https://hanabi.github.io/level-19/#the-5-pull)
    FivePull,
    /// [5 Number Ejection](https://hanabi.github.io/level-19/#the-5-number-ejection-5ne)
    FiveNumberEjection,
    /// [5 Number Discharge](https://hanabi.github.io/level-19/#the-5-number-discharge-5nd)
    FiveNumberDischarge,
    /// [Occupied Play Clue and Occupied Finesse](https://hanabi.github.io/level-20/#the-occupied-play-clue--the-occupied-finesse-opc)
    OccupiedPlay,
    /// [Out-of-Order Play Clue](https://hanabi.github.io/level-20/#the-out-of-order-play-clue-triple-o--ooo)
    OutOfOrderPlay,
    /// [Out-of-Order Finesse](https://hanabi.github.io/level-20/#the-out-of-order-finesse)
    OutOfOrderFinesse,
    /// [Suboptimal Prompt/Finesse/Bluff](https://hanabi.github.io/level-20/#the-suboptimal-prompt--the-suboptimal-finesse--the-suboptimal-bluff)
    SuboptimalConnection,
    /// [No-Information Finesse](https://hanabi.github.io/level-20/#the-no-information-finesse)
    NoInformationFinesse,
    /// [No-Information Double Bluff](https://hanabi.github.io/level-20/#the-no-information-double-bluff-nidb)
    NoInformationDoubleBluff,
    /// [Ignition](https://hanabi.github.io/level-21/#ignition)
    Ignition,
    /// [Replay Double Ignition](https://hanabi.github.io/level-21/#the-replay-double-ignition-rdi)
    ReplayDoubleIgnition,
    /// [Trash Double Ignition](https://hanabi.github.io/level-21/#the-trash-double-ignition-tdi)
    TrashDoubleIgnition,
    /// [Poke Double Ignition](https://hanabi.github.io/level-21/#the-poke-double-ignition-pdi)
    PokeDoubleIgnition,
    /// [Chop Move Ignition](https://hanabi.github.io/level-21/#the-chop-move-ignition-cmi-with-1-card-chop-moved)
    ChopMoveIgnition,
    /// [Bomb Double Ignition](https://hanabi.github.io/level-21/#bomb-double-ignition)
    BombDoubleIgnition,
    /// [Bomb Triple Ignition](https://hanabi.github.io/level-21/#bomb-triple-ignition)
    BombTripleIgnition,
    /// [Phantom Playable Cards](https://hanabi.github.io/level-22/#phantom-playable-cards)
    PhantomPlayable,
    /// [Sacrifice Discard](https://hanabi.github.io/level-22/#the-sacrifice-discard)
    SacrificeDiscard,
    /// [Echo Scream Discard Chop Move](https://hanabi.github.io/level-22/#the-echo-scream-discard-chop-move-esdcm)
    EchoScreamDiscard,
    /// [Composition Discard](https://hanabi.github.io/level-22/#the-composition-discard)
    CompositionDiscard,
    /// [Rebellious Discard](https://hanabi.github.io/level-22/#the-rebellious-discard)
    RebelliousDiscard,
    /// [Charms](https://hanabi.github.io/level-23/#charms)
    Charm,
    /// [Blaze Discard](https://hanabi.github.io/level-23/#the-blaze-discard)
    BlazeDiscard,
    /// [Hesitation Blind-Play](https://hanabi.github.io/level-23/#the-hesitation-blind-play)
    HesitationBlindPlay,
    /// [Unnecessary Moves](https://hanabi.github.io/level-24/#unnecessary-moves)
    UnnecessaryMove,
    /// [Unnecessary move with known trash](https://hanabi.github.io/level-24/#unnecessary-moves-with-known-trash--ignition)
    UnnecessaryIgnition,
    /// [Unnecessary move with unknown trash off chop](https://hanabi.github.io/level-24/#unnecessary-moves-with-unknown-trash-off-chop--chop-move)
    UnnecessaryChopMove,
    /// [Unnecessary move with unknown trash on chop](https://hanabi.github.io/level-24/#unnecessary-moves-with-unknown-trash-on-chop--trash-push)
    UnnecessaryTrashPush,
    /// [Priority](https://hanabi.github.io/level-25/#the-priority-prompt--the-priority-finesse)
    Priority,
    /// [Max-level extras](https://hanabi.github.io/extras/)
    Extra,
    /// [Transfer Chop Move](https://hanabi.github.io/extras/chop-moves/#the-transfer-chop-move)
    TransferChopMove,
    /// [Misplay Chop Move](https://hanabi.github.io/extras/chop-moves/#the-misplay-chop-move)
    MisplayChopMove,
    /// [Double Order Chop Move](https://hanabi.github.io/extras/chop-moves/#double-order-chop-move-for-3-player-games)
    DoubleOrderChopMove,
    /// [Spillover Chop Move](https://hanabi.github.io/extras/chop-moves/#spillover-chop-move)
    SpilloverChopMove,
    /// [Negative Self-Chop Move](https://hanabi.github.io/extras/chop-moves/#the-negative-self-chop-move)
    NegativeSelfChopMove,
    /// [Out-of-Position Ejection](https://hanabi.github.io/extras/ejection-extensions/#the-out-of-position-ejection)
    OutOfPositionEjection,
    /// [Stacked Ejection](https://hanabi.github.io/extras/ejection-extensions/#the-stacked-ejection)
    StackedEjection,
    /// [Double Ejection](https://hanabi.github.io/extras/ejection-extensions/#the-double-ejection)
    DoubleEjection,
    /// [Promise Clue](https://hanabi.github.io/extras/discards-misplays/#the-promise-clue--the-promise-discard)
    PromiseClue,
    /// [Promise Discard](https://hanabi.github.io/extras/discards-misplays/#the-promise-clue--the-promise-discard)
    PromiseDiscard,
    /// [Trash Push Discharge](https://hanabi.github.io/extras/discharges/#the-trash-push-discharge-tpd)
    TrashPushDischarge,
    /// [Trash Push Ejection](https://hanabi.github.io/extras/ejections/#trash-push-ejection)
    TrashPushEjection,
    /// [Bad Chop Move Ejection](https://hanabi.github.io/extras/ejections/#the-bad-chop-move-ejection-bcme)
    BadChopMoveEjection,
    /// [Rank Choice Ejection](https://hanabi.github.io/extras/ejections/#the-rank-choice-ejection-with-a-number-2-or-a-number-5-rce)
    RankChoiceEjection,
    /// [Trash Ejection](https://hanabi.github.io/extras/ejections/#the-trash-ejection)
    TrashEjection,
    /// [Replay Ejection](https://hanabi.github.io/extras/ejections/#the-replay-ejection)
    ReplayEjection,
    /// [Poke Ejection](https://hanabi.github.io/extras/ejections/#the-poke-ejection)
    PokeEjection,
    /// [Cautious Generation Discard](https://hanabi.github.io/extras/discards-misplays/#the-cautious-generation-discard)
    CautiousGenerationDiscard,
    /// [Unknown Trash Charm](https://hanabi.github.io/extras/charms/#the-unknown-trash-charm-utc)
    UnknownTrashCharm,
    /// [Junk Charm](https://hanabi.github.io/extras/charms/#the-junk-charm-for-1s)
    JunkCharm,
    /// [Out-of-Position Discharge/Charm](https://hanabi.github.io/extras/ejection-extensions/#the-out-of-position-dischargecharm)
    OutOfPositionDischarge,
    /// [Stacked Discharge/Charm](https://hanabi.github.io/extras/ejection-extensions/#the-stacked-dischargecharm)
    StackedDischarge,
    /// [Bad Trash Finesse Ejection](https://hanabi.github.io/extras/ejections/#the-bad-trash-finesse-ejection--the-bad-trash-bluff-ejection)
    BadTrashFinesseEjection,
    /// [Trash Finesse Push Ejection](https://hanabi.github.io/extras/ejections/#the-trash-finesse-push-ejection--the-trash-bluff-push-ejection)
    TrashFinessePushEjection,
    /// [Just-In-Time Fix Clue](https://hanabi.github.io/extras/fix-clues/#the-just-in-time-fix-clue-jit)
    JustInTimeFix,
    /// [Elimination Rewrite](https://hanabi.github.io/extras/miscellaneous/#the-elimination-rewrite-for-1s)
    EliminationRewrite,
    /// [Negative Blind-Play](https://hanabi.github.io/extras/miscellaneous/#the-negative-blind-play)
    NegativeBlindPlay,
    /// [Continuation Clue](https://hanabi.github.io/extras/play-clues/#the-continuation-clue-touching-both-inside-and-outside-a-layer)
    ContinuationClue,
    /// [Trash Pull](https://hanabi.github.io/extras/pushes-pulls/#the-trash-pull)
    TrashPull,
    /// [Fake Save](https://hanabi.github.io/extras/save-clues/#the-fake-save)
    FakeSave,
    /// [Saving Playable Cards when Preceding Cards Are Not Promptable](https://hanabi.github.io/extras/save-clues/#saving-playable-cards-when-the-preceding-cards-are-not-promptable)
    UnpromptablePredecessorSave,
    /// [Self Color Bluff](https://hanabi.github.io/extras/special-bluffs/#self-color-bluffs-1-for-1-form-scb)
    SelfColorBluff,
    /// [Self Color Double Bluff](https://hanabi.github.io/extras/special-bluffs/#self-color-double-bluff-scdb)
    SelfColorDoubleBluff,
    /// [Elimination Bluff](https://hanabi.github.io/extras/special-bluffs/#the-elimination-bluff--the-elimination-layered-finesse)
    EliminationBluff,
    /// [Known Priority Bluff](https://hanabi.github.io/extras/special-bluffs/#the-known-priority-bluff)
    KnownPriorityBluff,
    /// [Pestilent Triple Bluff](https://hanabi.github.io/extras/special-bluffs/#the-pestilent-triple-bluff)
    PestilentTripleBluff,
    /// [Pass Bluff](https://hanabi.github.io/extras/special-bluffs/#the-pass-bluff)
    PassBluff,
    /// [Purge Bluff](https://hanabi.github.io/extras/special-bluffs/#the-purge-bluff-layered-bluff)
    PurgeBluff,
    /// [Ambiguous Finesse Pass-Back](https://hanabi.github.io/extras/special-finesses/#the-ambiguous-finesse-pass-back-afpb)
    AmbiguousFinessePassBack,
    /// [Certain Priority Finesse](https://hanabi.github.io/extras/special-finesses/#potential-priority-duplication--the-certain-priority-finesse-or-priority-certain-finesse)
    CertainPriorityFinesse,
    /// [Patch Finesse](https://hanabi.github.io/extras/special-finesses/#the-patch-finesse)
    PatchFinesse,
    /// [Surreptitious Finesse](https://hanabi.github.io/extras/special-finesses/#the-surreptitious-finesse)
    SurreptitiousFinesse,
    /// [Inverted Priority Finesse](https://hanabi.github.io/extras/special-finesses/#inverted-priority-finesse)
    InvertedPriorityFinesse,
    /// [Finesse with a Lie Component](https://hanabi.github.io/extras/special-finesses/#finesses-with-a-lie-component)
    LieComponentFinesse,
    /// [Declined 5's Finesse](https://hanabi.github.io/extras/special-finesses/#the-declined-5s-finesse)
    DeclinedFiveFinesse,
    /// [Rank Choice Save Finesse/Bluff](https://hanabi.github.io/extras/special-finesses/#the-rank-choice-save-finesse--the-rank-choice-save-bluff)
    RankChoiceSaveFinesse,
    /// An earlier provisional signal was publicly disproved. This is audit
    /// metadata, not a playable convention level.
    Retraction,
}

/// Machine-readable coverage metadata for one cumulative learning-path level.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct HGroupLevelDescriptor {
    pub profile: HGroupProfile,
    pub title: &'static str,
    pub effects: &'static [HGroupMoveKind],
}

/// The cumulative learning path, with `max` represented as effective level 26.
pub const H_GROUP_LEVELS: [HGroupLevelDescriptor; 26] = [
    HGroupLevelDescriptor {
        profile: HGroupProfile::Level(HGroupLevel::Level1),
        title: "Basic conventions",
        effects: &[
            HGroupMoveKind::PlayClue,
            HGroupMoveKind::SaveClue,
            HGroupMoveKind::Prompt,
            HGroupMoveKind::Finesse,
        ],
    },
    HGroupLevelDescriptor {
        profile: HGroupProfile::Level(HGroupLevel::Level2),
        title: "Basic moves",
        effects: &[
            HGroupMoveKind::FiveStall,
            HGroupMoveKind::ReverseFinesse,
            HGroupMoveKind::SelfFinesse,
        ],
    },
    HGroupLevelDescriptor {
        profile: HGroupProfile::Level(HGroupLevel::Level3),
        title: "Basic strategy",
        effects: &[HGroupMoveKind::FixClue, HGroupMoveKind::SarcasticDiscard],
    },
    HGroupLevelDescriptor {
        profile: HGroupProfile::Level(HGroupLevel::Level4),
        title: "Chop moves",
        effects: &[
            HGroupMoveKind::ChopMove,
            HGroupMoveKind::TrashChopMove,
            HGroupMoveKind::FiveChopMove,
            HGroupMoveKind::OrderChopMove,
        ],
    },
    HGroupLevelDescriptor {
        profile: HGroupProfile::Level(HGroupLevel::Level5),
        title: "Special finesses",
        effects: &[
            HGroupMoveKind::HiddenFinesse,
            HGroupMoveKind::LayeredFinesse,
            HGroupMoveKind::ClandestineFinesse,
            HGroupMoveKind::QueuedFinesse,
            HGroupMoveKind::AmbiguousFinesse,
        ],
    },
    HGroupLevelDescriptor {
        profile: HGroupProfile::Level(HGroupLevel::Level6),
        title: "Tempo clues",
        effects: &[
            HGroupMoveKind::TempoClue,
            HGroupMoveKind::TempoClueChopMove,
            HGroupMoveKind::Clarity,
        ],
    },
    HGroupLevelDescriptor {
        profile: HGroupProfile::Level(HGroupLevel::Level7),
        title: "Emergency discards",
        effects: &[
            HGroupMoveKind::ScreamDiscard,
            HGroupMoveKind::ShoutDiscard,
            HGroupMoveKind::GenerationDiscard,
        ],
    },
    HGroupLevelDescriptor {
        profile: HGroupProfile::Level(HGroupLevel::Level8),
        title: "End-game",
        effects: &[
            HGroupMoveKind::PositionalDiscard,
            HGroupMoveKind::PositionalMisplay,
            HGroupMoveKind::DoublePositionalMisplay,
            HGroupMoveKind::DistributionClue,
        ],
    },
    HGroupLevelDescriptor {
        profile: HGroupProfile::Level(HGroupLevel::Level9),
        title: "Stalling",
        effects: &[
            HGroupMoveKind::Stall,
            HGroupMoveKind::EarlyGameStall,
            HGroupMoveKind::DoubleDiscardAvoidance,
            HGroupMoveKind::LockedHandSave,
            HGroupMoveKind::FillInClue,
            HGroupMoveKind::AnxietyPlay,
            HGroupMoveKind::EightClueSave,
            HGroupMoveKind::Burn,
        ],
    },
    HGroupLevelDescriptor {
        profile: HGroupProfile::Level(HGroupLevel::Level10),
        title: "Special discards",
        effects: &[
            HGroupMoveKind::TransferDiscard,
            HGroupMoveKind::GentlemansDiscard,
            HGroupMoveKind::LayeredGentlemansDiscard,
            HGroupMoveKind::BatonDiscard,
            HGroupMoveKind::SarcasticFinesse,
            HGroupMoveKind::CertainFinesse,
            HGroupMoveKind::CertainDiscard,
            HGroupMoveKind::CompositionFinesse,
        ],
    },
    HGroupLevelDescriptor {
        profile: HGroupProfile::Level(HGroupLevel::Level11),
        title: "Bluffs",
        effects: &[HGroupMoveKind::Bluff, HGroupMoveKind::SelfBluff],
    },
    HGroupLevelDescriptor {
        profile: HGroupProfile::Level(HGroupLevel::Level12),
        title: "Context",
        effects: &[
            HGroupMoveKind::SelfishClue,
            HGroupMoveKind::SelfishFinesse,
            HGroupMoveKind::StaleOnesClue,
            HGroupMoveKind::FocusInversion,
            HGroupMoveKind::Context,
        ],
    },
    HGroupLevelDescriptor {
        profile: HGroupProfile::Level(HGroupLevel::Level13),
        title: "Intermediate bluffs",
        effects: &[
            HGroupMoveKind::ThreeBluff,
            HGroupMoveKind::CriticalColorBluff,
            HGroupMoveKind::HardBluff,
            HGroupMoveKind::GoodTouchBluff,
        ],
    },
    HGroupLevelDescriptor {
        profile: HGroupProfile::Level(HGroupLevel::Level14),
        title: "Trash moves",
        effects: &[
            HGroupMoveKind::TrashPush,
            HGroupMoveKind::TrashPushPrompt,
            HGroupMoveKind::TrashPushFinesse,
            HGroupMoveKind::TrashFinesse,
            HGroupMoveKind::ReverseTrashFinesse,
            HGroupMoveKind::ForcedGentlemansDiscardChopMove,
            HGroupMoveKind::TrashBluff,
            HGroupMoveKind::TrashOrderChopMove,
        ],
    },
    HGroupLevelDescriptor {
        profile: HGroupProfile::Level(HGroupLevel::Level15),
        title: "Double bluffs",
        effects: &[
            HGroupMoveKind::DoubleBluff,
            HGroupMoveKind::HardDoubleBluff,
            HGroupMoveKind::PestilentDoubleBluff,
        ],
    },
    HGroupLevelDescriptor {
        profile: HGroupProfile::Level(HGroupLevel::Level16),
        title: "Ejections and discharges",
        effects: &[
            HGroupMoveKind::FiveColorEjection,
            HGroupMoveKind::UnknownTrashDischarge,
            HGroupMoveKind::UnknownDupeDischarge,
        ],
    },
    HGroupLevelDescriptor {
        profile: HGroupProfile::Level(HGroupLevel::Level17),
        title: "Duplication",
        effects: &[
            HGroupMoveKind::DuplicitousValue,
            HGroupMoveKind::DuplicitousBlindPlay,
            HGroupMoveKind::DuplicitousTempo,
            HGroupMoveKind::AssistedTrashChopMove,
            HGroupMoveKind::TimeTravelChopMove,
        ],
    },
    HGroupLevelDescriptor {
        profile: HGroupProfile::Level(HGroupLevel::Level18),
        title: "Elimination",
        effects: &[
            HGroupMoveKind::Elimination,
            HGroupMoveKind::EliminationFinesse,
            HGroupMoveKind::EliminationBlindPlay,
            HGroupMoveKind::EliminationPlayClue,
            HGroupMoveKind::EliminationRiding,
            HGroupMoveKind::EliminationSelfChopMove,
            HGroupMoveKind::TrashTouchElimination,
        ],
    },
    HGroupLevelDescriptor {
        profile: HGroupProfile::Level(HGroupLevel::Level19),
        title: "5 tech",
        effects: &[
            HGroupMoveKind::FivePull,
            HGroupMoveKind::FiveNumberEjection,
            HGroupMoveKind::FiveNumberDischarge,
        ],
    },
    HGroupLevelDescriptor {
        profile: HGroupProfile::Level(HGroupLevel::Level20),
        title: "Out-of-order play",
        effects: &[
            HGroupMoveKind::OccupiedPlay,
            HGroupMoveKind::OutOfOrderPlay,
            HGroupMoveKind::OutOfOrderFinesse,
            HGroupMoveKind::SuboptimalConnection,
            HGroupMoveKind::NoInformationFinesse,
            HGroupMoveKind::NoInformationDoubleBluff,
        ],
    },
    HGroupLevelDescriptor {
        profile: HGroupProfile::Level(HGroupLevel::Level21),
        title: "Ignition",
        effects: &[
            HGroupMoveKind::ReplayDoubleIgnition,
            HGroupMoveKind::TrashDoubleIgnition,
            HGroupMoveKind::PokeDoubleIgnition,
            HGroupMoveKind::ChopMoveIgnition,
            HGroupMoveKind::BombDoubleIgnition,
            HGroupMoveKind::BombTripleIgnition,
        ],
    },
    HGroupLevelDescriptor {
        profile: HGroupProfile::Level(HGroupLevel::Level22),
        title: "Phantom playable cards",
        effects: &[
            HGroupMoveKind::PhantomPlayable,
            HGroupMoveKind::SacrificeDiscard,
            HGroupMoveKind::EchoScreamDiscard,
            HGroupMoveKind::CompositionDiscard,
            HGroupMoveKind::RebelliousDiscard,
        ],
    },
    HGroupLevelDescriptor {
        profile: HGroupProfile::Level(HGroupLevel::Level23),
        title: "Charms",
        effects: &[
            HGroupMoveKind::Charm,
            HGroupMoveKind::BlazeDiscard,
            HGroupMoveKind::HesitationBlindPlay,
        ],
    },
    HGroupLevelDescriptor {
        profile: HGroupProfile::Level(HGroupLevel::Level24),
        title: "Unnecessary moves",
        effects: &[
            HGroupMoveKind::UnnecessaryMove,
            HGroupMoveKind::UnnecessaryIgnition,
            HGroupMoveKind::UnnecessaryChopMove,
            HGroupMoveKind::UnnecessaryTrashPush,
        ],
    },
    HGroupLevelDescriptor {
        profile: HGroupProfile::Level(HGroupLevel::Level25),
        title: "Priority",
        effects: &[HGroupMoveKind::Priority],
    },
    HGroupLevelDescriptor {
        profile: HGroupProfile::Max,
        title: "Max",
        effects: &[
            HGroupMoveKind::Extra,
            HGroupMoveKind::TransferChopMove,
            HGroupMoveKind::MisplayChopMove,
            HGroupMoveKind::DoubleOrderChopMove,
            HGroupMoveKind::SpilloverChopMove,
            HGroupMoveKind::NegativeSelfChopMove,
            HGroupMoveKind::OutOfPositionEjection,
            HGroupMoveKind::StackedEjection,
            HGroupMoveKind::DoubleEjection,
            HGroupMoveKind::PromiseClue,
            HGroupMoveKind::PromiseDiscard,
            HGroupMoveKind::TrashPushDischarge,
            HGroupMoveKind::TrashPushEjection,
            HGroupMoveKind::BadChopMoveEjection,
            HGroupMoveKind::RankChoiceEjection,
            HGroupMoveKind::TrashEjection,
            HGroupMoveKind::ReplayEjection,
            HGroupMoveKind::PokeEjection,
            HGroupMoveKind::CautiousGenerationDiscard,
            HGroupMoveKind::UnknownTrashCharm,
            HGroupMoveKind::JunkCharm,
            HGroupMoveKind::OutOfPositionDischarge,
            HGroupMoveKind::StackedDischarge,
            HGroupMoveKind::BadTrashFinesseEjection,
            HGroupMoveKind::TrashFinessePushEjection,
            HGroupMoveKind::JustInTimeFix,
            HGroupMoveKind::EliminationRewrite,
            HGroupMoveKind::NegativeBlindPlay,
            HGroupMoveKind::ContinuationClue,
            HGroupMoveKind::TrashPull,
            HGroupMoveKind::FakeSave,
            HGroupMoveKind::UnpromptablePredecessorSave,
            HGroupMoveKind::SelfColorBluff,
            HGroupMoveKind::SelfColorDoubleBluff,
            HGroupMoveKind::EliminationBluff,
            HGroupMoveKind::KnownPriorityBluff,
            HGroupMoveKind::PestilentTripleBluff,
            HGroupMoveKind::PassBluff,
            HGroupMoveKind::PurgeBluff,
            HGroupMoveKind::AmbiguousFinessePassBack,
            HGroupMoveKind::CertainPriorityFinesse,
            HGroupMoveKind::PatchFinesse,
            HGroupMoveKind::SurreptitiousFinesse,
            HGroupMoveKind::InvertedPriorityFinesse,
            HGroupMoveKind::LieComponentFinesse,
            HGroupMoveKind::DeclinedFiveFinesse,
            HGroupMoveKind::RankChoiceSaveFinesse,
        ],
    },
];

/// Selects a conservative, unambiguous Level 1 clue.
///
/// Candidate clues must satisfy focus and Minimum Clue Value. Play clues also
/// satisfy Good Touch and either play now or create exactly one valid Prompt
/// or Finesse connection. Save clues are restricted to the Level 1 5, 2, and
/// critical-card forms.
#[allow(clippy::too_many_lines)]
fn replay_h_group(deductions: &LogicalDeductions, profile: HGroupProfile) -> HGroupState {
    with_replay_memo(|| {
        let ordinary = replay_h_group_inner(
            deductions,
            profile,
            PerspectiveDepth::NestedRecipients,
            false,
        );
        let current = deductions.view().current_player;
        let mut hypotheses = InterpretationHypotheses::ordinary(ordinary);
        if hypotheses.ordinary_gives_actor_a_live_connection(current) {
            return hypotheses.resolve_for_actor(current);
        }
        let empathetic = replay_h_group_inner(
            deductions,
            profile,
            PerspectiveDepth::NestedRecipients,
            true,
        );
        hypotheses.add(InterpretationSource::BlindReverseEmpathy, empathetic);
        hypotheses.resolve_for_actor(current)
    })
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
struct ReplayMemoKey {
    view: PlayerView,
    profile: HGroupProfile,
    perspective_depth: PerspectiveDepth,
    allow_blind_reverse_empathy: bool,
}

thread_local! {
    static H_GROUP_REPLAY_MEMO: RefCell<Option<HashMap<ReplayMemoKey, HGroupState>>> =
        const { RefCell::new(None) };
}

struct ReplayMemoGuard;

impl Drop for ReplayMemoGuard {
    fn drop(&mut self) {
        H_GROUP_REPLAY_MEMO.with(|memo| {
            memo.replace(None);
        });
    }
}

/// Shares immutable prefix reductions across one recursive replay. Historical
/// actor-perspective queries repeatedly ask for overlapping prefixes; without
/// this scope, a length-N replay recursively rebuilds the same length-0..N
/// histories for every later discard.
fn with_replay_memo<T>(operation: impl FnOnce() -> T) -> T {
    let already_active = H_GROUP_REPLAY_MEMO.with(|memo| memo.borrow().is_some());
    if already_active {
        return operation();
    }
    H_GROUP_REPLAY_MEMO.with(|memo| {
        memo.replace(Some(HashMap::new()));
    });
    let _guard = ReplayMemoGuard;
    operation()
}

#[allow(clippy::too_many_lines)]
fn replay_h_group_inner(
    deductions: &LogicalDeductions,
    profile: HGroupProfile,
    perspective_depth: PerspectiveDepth,
    allow_blind_reverse_empathy: bool,
) -> HGroupState {
    with_replay_memo(|| {
        let key = ReplayMemoKey {
            view: deductions.view().clone(),
            profile,
            perspective_depth,
            allow_blind_reverse_empathy,
        };
        if let Some(replay) = H_GROUP_REPLAY_MEMO.with(|memo| {
            memo.borrow()
                .as_ref()
                .and_then(|memo| memo.get(&key).cloned())
        }) {
            return replay;
        }
        let replay = replay_h_group_inner_uncached(
            deductions,
            profile,
            perspective_depth,
            allow_blind_reverse_empathy,
        );
        H_GROUP_REPLAY_MEMO.with(|memo| {
            memo.borrow_mut()
                .as_mut()
                .expect("replay memo scope is active")
                .insert(key, replay.clone());
        });
        replay
    })
}

/// Whether a clue carries a recognized obligation that takes precedence over
/// an immediate play. Keeping this precedence in one typed transition avoids
/// each recognizer independently deciding whether an old promise survived.
fn clue_permits_direct_play_deferral(signals: &ConventionJournal, turn: u32) -> bool {
    // A recognized clue action does not falsify an older playable card merely
    // because its owner spent this turn giving the clue. In particular, a
    // Finesse with a Lie Component is a team obligation, so performing it can
    // legitimately defer an otherwise-due direct Play promise.
    // Source: https://hanabi.github.io/extras/special-finesses/#finesses-with-a-lie-component
    signals.iter().any(|signal| {
        signal.turn == turn
            && matches!(
                signal.kind,
                HGroupMoveKind::PlayClue
                    | HGroupMoveKind::FixClue
                    | HGroupMoveKind::SaveClue
                    | HGroupMoveKind::FiveStall
                    | HGroupMoveKind::Stall
                    | HGroupMoveKind::Context
                    | HGroupMoveKind::DoubleDiscardAvoidance
                    | HGroupMoveKind::LockedHandSave
                    | HGroupMoveKind::EightClueSave
                    | HGroupMoveKind::LieComponentFinesse
            )
    })
}

/// Records direct play promises that their owner has publicly declined.
///
/// This does not rewrite clue facts or feed back into recognizers. It is an
/// action-selection fact: the card should not be played from that superseded
/// interpretation, while later rules may still reason from the objective clue
/// information. Ordered connections own their separate lifecycle in
/// `ConnectionManager` and may span turns.
struct DirectPlayDeclines<'a> {
    cards: &'a mut CardSet,
    turns: &'a mut Vec<(CardId, u32)>,
}

fn record_declined_direct_plays(
    actor: PlayerId,
    temporary_from_turn: Option<u32>,
    hands: &[Vec<CardId>],
    clues: &[HGroupClueInterpretation],
    already_playing: &CardSet,
    pending_connections: &ConnectionManager,
    declines: &mut DirectPlayDeclines<'_>,
) {
    let connection_cards = pending_connections
        .iter()
        .flat_map(|connection| {
            connection
                .cards
                .iter()
                .copied()
                .chain(core::iter::once(connection.focus))
        })
        .collect::<CardSet>();
    let newly_declined = hands[actor.index()]
        .iter()
        .copied()
        .filter(|card| {
            let has_direct_play_clue = clues.iter().rev().any(|clue| {
                clue.target == actor
                    && clue.focus == *card
                    && clue.kind == HGroupClueKind::Play
                    && !clue.play_identities.is_empty()
                    && clue
                        .play_identities
                        .iter()
                        .all(|identity| is_playable_at(clue.stack_heights, identity))
            });
            (already_playing.contains(card) || has_direct_play_clue)
                && !connection_cards.contains(card)
        })
        .collect::<Vec<_>>();
    for card in newly_declined {
        declines.cards.insert(card);
        if let Some(turn) = temporary_from_turn {
            if let Some((_, declined_turn)) = declines
                .turns
                .iter_mut()
                .find(|(declined, _)| *declined == card)
            {
                *declined_turn = turn;
            } else {
                declines.turns.push((card, turn));
            }
        } else {
            declines.turns.retain(|(declined, _)| *declined != card);
        }
    }
}

fn h_group_phase(view: &PlayerView, early_game: bool) -> HGroupPhase {
    let stack_heights = std::array::from_fn(|suit| {
        u8::try_from(view.play_stacks[suit].len()).expect("a Hanabi stack has at most five cards")
    });
    h_group_phase_at(view.hands.len(), early_game, view.deck_size, stack_heights)
}

fn h_group_phase_at(
    player_count: usize,
    early_game: bool,
    deck_size: usize,
    stack_heights: [u8; 5],
) -> HGroupPhase {
    let score = stack_heights
        .iter()
        .map(|height| usize::from(*height))
        .sum::<usize>();
    let remaining_plays = 25_usize.saturating_sub(score);
    let remaining_turns = deck_size.saturating_add(player_count);
    let pace = isize::try_from(remaining_turns).unwrap_or(isize::MAX)
        - isize::try_from(remaining_plays).unwrap_or(isize::MAX);
    if pace < isize::try_from(player_count).unwrap_or(isize::MAX) {
        HGroupPhase::EndGame
    } else if early_game {
        HGroupPhase::EarlyGame
    } else if score < 5 {
        HGroupPhase::LowScore
    } else {
        HGroupPhase::Normal
    }
}

fn push_signal(
    signals: &mut ConventionJournal,
    entry: &ObservedHistoryEntry,
    actor: PlayerId,
    target: Option<PlayerId>,
    kind: HGroupMoveKind,
    cards: Vec<CardId>,
    identity: Option<Card>,
) {
    ConventionReducer::apply(
        EffectBatch::recognized(HGroupSignal {
            turn: entry.turn,
            actor,
            target,
            kind,
            cards,
            identity,
        }),
        signals,
    );
}

fn next_player(player: PlayerId, player_count: usize) -> PlayerId {
    PlayerId::new(
        u8::try_from((player.index() + 1) % player_count)
            .expect("standard Hanabi has at most five players"),
    )
}

fn was_clued_before(view: &PlayerView, turn: u32, card: CardId) -> bool {
    view.history.iter().take_while(|entry| entry.turn < turn).any(
        |entry| matches!(&entry.event, ObservedEvent::Clued { touched, .. } if touched.contains(&card)),
    )
}

fn was_clued_before_with(view: &PlayerView, turn: u32, card: CardId, clue: Clue) -> bool {
    view.history.iter().take_while(|entry| entry.turn < turn).any(
        |entry| matches!(&entry.event, ObservedEvent::Clued { clue: prior, touched, .. } if *prior == clue && touched.contains(&card)),
    )
}

#[allow(clippy::too_many_arguments)]
fn has_higher_basic_priority(
    view: &PlayerView,
    hands: &[Vec<CardId>],
    facts: &[ClueFacts],
    forced_playable: &CardSet,
    actor: PlayerId,
    hand: &[CardId],
    candidate: CardId,
    candidate_identity: Card,
    played: CardId,
    played_identity: Card,
) -> bool {
    let priority_features = |card: CardId, identity: Card| {
        let exact = facts[card.index()].identity_mask() == 1 << identity.index();
        let blind = forced_playable.contains(&card) && !exact;
        let next = (identity.rank != Rank::Five)
            .then(|| Card::new(identity.suit, Rank::ALL[identity.rank.index() + 1]));
        let leads_other = next.is_some_and(|next| {
            hands
                .iter()
                .enumerate()
                .filter(|(player, _)| *player != actor.index())
                .flat_map(|(_, other_hand)| other_hand)
                .any(|other| identity_of(view, *other) == Some(next))
        });
        let terminal_chain = (candidate_identity.rank == Rank::Five
            || played_identity.rank == Rank::Five)
            && play_order::completes_own_terminal_chain(identity, |successor| {
                hand.iter()
                    .any(|other| facts[other.index()].identity_mask() == 1 << successor.index())
            });
        let leads_self = !terminal_chain
            && next.is_some_and(|next| {
                hand.iter().copied().any(|other| {
                    other != card && facts[other.index()].identity_mask() == 1 << next.index()
                })
            });
        let position = hand
            .iter()
            .position(|in_hand| *in_hand == card)
            .unwrap_or(0);
        (blind, leads_other, leads_self, position)
    };

    let (candidate_blind, candidate_leads_other, candidate_leads_self, candidate_position) =
        priority_features(candidate, candidate_identity);
    let (played_blind, played_leads_other, played_leads_self, played_position) =
        priority_features(played, played_identity);
    if candidate_blind != played_blind {
        return candidate_blind;
    }
    if candidate_blind {
        // The ordering between multiple unresolved blind plays depends on the
        // order of their originating Finesses. Do not invent a Priority signal
        // when replay does not have enough evidence to distinguish them.
        return false;
    }
    if candidate_leads_other != played_leads_other {
        return candidate_leads_other;
    }
    if candidate_leads_other {
        // The Level 25 flowchart explicitly gives equal Priority when both
        // cards lead into Finessed or known clued cards in other hands.
        return false;
    }
    if candidate_leads_self != played_leads_self {
        return candidate_leads_self;
    }
    match (
        candidate_identity.rank == Rank::Five,
        played_identity.rank == Rank::Five,
    ) {
        (true, false) => return true,
        (false, true) => return false,
        _ => {}
    }
    match candidate_identity
        .rank
        .number()
        .cmp(&played_identity.rank.number())
    {
        std::cmp::Ordering::Less => true,
        std::cmp::Ordering::Greater => false,
        std::cmp::Ordering::Equal => candidate_position > played_position,
    }
}

/// Builds the shared ordered connection graph used by the named Prompt and
/// Finesse forms. A named form is represented by the graph shape (actor order,
/// Prompt versus blind connection, ambiguity, and layering), rather than by a
/// second bespoke transition system.
///
/// Sources:
/// - <https://hanabi.github.io/level-1/#the-prompt>
/// - <https://hanabi.github.io/level-1/#the-finesse>
/// - <https://hanabi.github.io/level-2/#the-double-prompt--triple-prompt--quadruple-prompt>
/// - <https://hanabi.github.io/level-2/#the-double-finesse--triple-finesse--quadruple-finesse>
/// - <https://hanabi.github.io/level-2/#the-prompt--finesse>
/// - <https://hanabi.github.io/level-2/#the-reverse-finesse>
/// - <https://hanabi.github.io/level-2/#the-self-finesse>
/// - <https://hanabi.github.io/level-5/#the-hidden-finesse>
/// - <https://hanabi.github.io/level-5/#the-layered-finesse>
/// - <https://hanabi.github.io/level-5/#the-clandestine-finesse>
/// - <https://hanabi.github.io/level-5/#the-queued-finesse>
/// - <https://hanabi.github.io/level-5/#the-ambiguous-finesse>
#[derive(Clone, Copy)]
struct PromptableBeforeClue<'a>(&'a CardSet);

#[derive(Clone, Copy)]
struct CurrentClueTouches<'a>(&'a [CardId]);

/// Immutable inputs shared by speculative connection planning and the single
/// canonical commit. Keeping pre-clue promptability and current-clue touches
/// in distinct types prevents a newly touched card from becoming its own
/// historical Prompt.
struct ConnectionPlanningContext<'a> {
    profile: HGroupProfile,
    view: &'a PlayerView,
    turn: u32,
    giver: PlayerId,
    target: PlayerId,
    focus: CardId,
    clue: Clue,
    touches: CurrentClueTouches<'a>,
    hands: &'a [Vec<CardId>],
    facts: &'a [ClueFacts],
    clues: &'a [HGroupClueInterpretation],
    promptable_before: PromptableBeforeClue<'a>,
    protected_before: &'a CardSet,
    already_playing: &'a CardSet,
    declined_direct_plays: &'a CardSet,
    convention_facts: &'a ConventionFacts,
    chop_moved: &'a CardSet,
    stack_heights: [u8; 5],
    allow_blind_reverse_empathy: bool,
}

impl ConnectionPlanningContext<'_> {
    fn simulate(
        &self,
        identity: Card,
        pending: &ConnectionManager,
        invisibly_clued: &ProvenancedCardSet,
    ) -> ClueInterpretationHypothesis {
        let loaded = !self.convention_facts.fixed_cards().contains(&self.focus)
            && loaded_connection_plan(
                self.view,
                Some(self.hands),
                Some(self.facts),
                Some(HistoricalView::new(self.view, self.turn)),
                self.giver,
                self.target,
                self.focus,
                identity,
                self.protected_before,
                self.already_playing,
                pending,
                self.stack_heights,
            )
            .is_some();
        let mut simulated_pending = pending.clone();
        let mut simulated_invisible = invisibly_clued.clone();
        let mut required_fix = None;
        let connections = schedule_connection(
            self.profile,
            self.view,
            self.turn,
            self.giver,
            self.target,
            self.focus,
            self.clue,
            self.touches.0,
            Some(identity),
            self.hands,
            self.facts,
            self.clues,
            self.promptable_before.0,
            self.already_playing,
            self.declined_direct_plays,
            self.convention_facts,
            self.chop_moved,
            &mut simulated_invisible,
            self.stack_heights,
            &mut simulated_pending,
            &mut required_fix,
            self.allow_blind_reverse_empathy,
        );
        ClueInterpretationHypothesis {
            focus_identity: identity,
            connection_steps: connections
                .into_iter()
                .map(|connection| ClueConnectionStep {
                    actor: connection.actor,
                    cards: connection.cards,
                    expected: connection.expected,
                    kind: connection.kind,
                })
                .collect(),
            required_fix,
            loaded,
        }
    }

    fn commit(
        &self,
        identity: Option<Card>,
        pending: &mut ConnectionManager,
        invisibly_clued: &mut ProvenancedCardSet,
    ) -> (Vec<ConnectionObligation>, Option<RequiredFix>) {
        let mut required_fix = None;
        let connections = schedule_connection(
            self.profile,
            self.view,
            self.turn,
            self.giver,
            self.target,
            self.focus,
            self.clue,
            self.touches.0,
            identity,
            self.hands,
            self.facts,
            self.clues,
            self.promptable_before.0,
            self.already_playing,
            self.declined_direct_plays,
            self.convention_facts,
            self.chop_moved,
            invisibly_clued,
            self.stack_heights,
            pending,
            &mut required_fix,
            self.allow_blind_reverse_empathy,
        );
        (connections, required_fix)
    }
}

#[allow(clippy::too_many_arguments, clippy::too_many_lines)]
fn schedule_connection(
    profile: HGroupProfile,
    view: &PlayerView,
    turn: u32,
    giver: PlayerId,
    target: PlayerId,
    focus: CardId,
    clue: Clue,
    same_clue_touched: &[CardId],
    focus_identity: Option<Card>,
    hands: &[Vec<CardId>],
    facts: &[ClueFacts],
    clues: &[HGroupClueInterpretation],
    promptable_before_clue: &CardSet,
    already_playing: &CardSet,
    declined_direct_plays: &CardSet,
    convention_facts: &ConventionFacts,
    chop_moved: &CardSet,
    invisibly_clued: &mut ProvenancedCardSet,
    stack_heights: [u8; 5],
    pending: &mut ConnectionManager,
    required_fix: &mut Option<RequiredFix>,
    allow_blind_reverse_empathy: bool,
) -> Vec<ConnectionObligation> {
    let mut scheduled_connections = Vec::new();
    let Some(focus_identity) = focus_identity else {
        return scheduled_connections;
    };
    let height = stack_heights[focus_identity.suit.index()];
    if focus_identity.rank.number() <= height + 1 {
        return scheduled_connections;
    }
    // A previously fixed focus has no live Play obligation for a new clue to
    // load. Its physical clues remain, but the Fix retracted their convention
    // meaning; the new clue establishes a fresh interpretation instead.
    let loaded_plan = (rule_enabled(profile, HGroupRuleId::Extras)
        && !convention_facts.fixed_cards().contains(&focus))
    .then(|| {
        loaded_connection_plan(
            view,
            Some(hands),
            Some(facts),
            Some(HistoricalView::new(view, turn)),
            giver,
            target,
            focus,
            focus_identity,
            promptable_before_clue,
            already_playing,
            pending,
            stack_heights,
        )
    });
    if let Some(Some(Some(fix))) = loaded_plan {
        *required_fix = Some(fix);
    }
    let target_loaded = loaded_plan.is_some_and(|plan| plan.is_some())
        || (promptable_before_clue.contains(&focus)
            && IdentitySet::from_mask(facts[focus.index()].identity_mask()).len() == 1);
    let connection_count = if rule_enabled(profile, HGroupRuleId::BasicMoves) {
        focus_identity.rank.number().saturating_sub(height + 1)
    } else {
        1
    };
    let mut actor_index = (giver.index() + 1) % hands.len();
    let mut scheduled_cards = CardSet::default();
    let mut reverse_cycle_started = false;
    for offset in 0..connection_count {
        let expected_rank = usize::from(height + offset);
        let expected = Card::new(focus_identity.suit, Rank::ALL[expected_rank]);
        let matches_expected = |card: CardId| {
            identity_of(view, card) == Some(expected)
                || clues.iter().rev().any(|clue| {
                    clue.focus == card && clue.focus_identities == IdentitySet::singleton(expected)
                })
        };
        let gotten_match_is_still_waiting = promptable_before_clue
            .iter()
            .any(|card| !already_playing.contains(card) && matches_expected(*card));
        let expected_is_already_playing = !gotten_match_is_still_waiting
            && already_playing
                .iter()
                .any(|card| !declined_direct_plays.contains(card) && matches_expected(*card));
        if pending.identity_is_queued(expected) {
            let giver_is_deferring_this_connection = pending.iter().any(|connection| {
                connection.actor == giver
                    && connection.expected == expected
                    && pending.is_active(connection)
            });
            if giver_is_deferring_this_connection {
                // By giving the connecting clue instead of taking their own
                // queued play, the giver demonstrates that the old route is
                // being replaced by the new clue's connection graph.
                // <https://hanabi.github.io/level-5/#the-layered-finesse>
                pending.cancel_where(
                    turn,
                    ConnectionTransitionReason::DisplacedByClue,
                    |connection| connection.actor == giver && connection.expected == expected,
                );
            } else {
                if rule_enabled(profile, HGroupRuleId::SpecialFinesses) && !clue.matches(expected) {
                    if let Some(connection) = extend_queued_finesse_with_playable_layers(
                        view,
                        giver,
                        target,
                        expected,
                        hands,
                        promptable_before_clue,
                        invisibly_clued,
                        stack_heights,
                        pending,
                        turn,
                    ) {
                        scheduled_connections.push(connection);
                    }
                }
                continue;
            }
        }
        if expected_is_already_playing {
            continue;
        }
        let elimination_card_for = |actor: PlayerId| {
            rule_enabled(profile, HGroupRuleId::Elimination)
                .then(|| {
                    elimination_finesse_card(
                        actor,
                        &hands[actor.index()],
                        focus,
                        expected,
                        convention_facts,
                        chop_moved,
                        |card| facts[card.index()].allows(expected),
                    )
                })
                .flatten()
        };
        let mut found = None;
        let ordinary_search_len = if rule_enabled(profile, HGroupRuleId::BasicMoves) {
            (target.index() + hands.len() - actor_index) % hands.len() + 1
        } else {
            1
        };
        let reverse_finesse_positions = (ordinary_search_len..hands.len())
            .filter_map(|distance| {
                let candidate_index = (actor_index + distance) % hands.len();
                (candidate_index != target.index() && candidate_index != giver.index())
                    .then(|| {
                        elimination_card_for(PlayerId::new(
                            u8::try_from(candidate_index).expect("player index"),
                        ))
                        .or_else(|| {
                            hands[candidate_index].iter().rev().copied().find(|card| {
                                *card != focus
                                    && !promptable_before_clue.contains(card)
                                    && !invisibly_clued.contains(card)
                                    && !scheduled_cards.contains(card)
                            })
                        })
                        .map(|card| (candidate_index, card))
                    })
                    .flatten()
            })
            .collect::<Vec<_>>();
        let visible_reverse_finesse =
            reverse_finesse_positions
                .iter()
                .any(|(candidate_index, card)| {
                    !visible_reverse_candidate_was_just_declined(
                        view,
                        turn,
                        PlayerId::new(
                            u8::try_from(*candidate_index)
                                .expect("standard Hanabi has at most five players"),
                        ),
                        *card,
                        expected,
                        allow_blind_reverse_empathy,
                    ) && (identity_of(view, *card) == Some(expected)
                        || (*candidate_index == view.observer.index()
                            && (facts[card.index()].identity_mask() == 1 << expected.index()
                                || elimination_card_for(view.observer) == Some(*card))))
                })
                || rule_enabled(profile, HGroupRuleId::SpecialFinesses)
                    && (ordinary_search_len..hands.len()).any(|distance| {
                        let candidate_index = (actor_index + distance) % hands.len();
                        if candidate_index == target.index() || candidate_index == giver.index() {
                            return false;
                        }
                        let actor = PlayerId::new(
                            u8::try_from(candidate_index)
                                .expect("standard Hanabi has at most five players"),
                        );
                        let gotten = promptable_before_clue
                            .union(invisibly_clued)
                            .copied()
                            .collect::<CardSet>();
                        let unclued = hands[candidate_index]
                            .iter()
                            .rev()
                            .copied()
                            .filter(|card| {
                                !gotten.contains(card)
                                    && !scheduled_cards.contains(card)
                                    && *card != focus
                            })
                            .collect::<Vec<_>>();
                        unclued
                            .iter()
                            .position(|card| {
                                !visible_reverse_candidate_was_just_declined(
                                    view,
                                    turn,
                                    actor,
                                    *card,
                                    expected,
                                    allow_blind_reverse_empathy,
                                ) && (identity_of(view, *card) == Some(expected)
                                    || (candidate_index == view.observer.index()
                                        && facts[card.index()].identity_mask()
                                            == 1 << expected.index()))
                            })
                            .is_some_and(|position| {
                                position > 0
                                    && unclued[..position].iter().all(|card| {
                                        identity_of(view, *card).is_some_and(|identity| {
                                            is_playable_at(stack_heights, identity)
                                        })
                                    })
                            })
                    });
        let blind_reverse_finesse = !visible_reverse_finesse
            && blind_reverse_finesse_is_eligible(view, giver, allow_blind_reverse_empathy)
            && reverse_finesse_positions
                .iter()
                .any(|(candidate_index, card)| {
                    *candidate_index == view.observer.index()
                        && facts[card.index()].allows(expected)
                });
        let visible_reverse_prompt = (ordinary_search_len..hands.len()).any(|distance| {
            let candidate_index = (actor_index + distance) % hands.len();
            if candidate_index == target.index() || candidate_index == giver.index() {
                return false;
            }
            hands[candidate_index].iter().rev().copied().any(|card| {
                card != focus
                    && promptable_before_clue.contains(&card)
                    && !chop_moved.contains(&card)
                    && !already_playing.contains(&card)
                    && !scheduled_cards.contains(&card)
                    && pending_card_allows_identity(
                        pending,
                        convention_facts,
                        card,
                        expected,
                        stack_heights,
                    )
                    && identity_of(view, card) == Some(expected)
            })
        });
        let direct_reverse_connection = rule_enabled(profile, HGroupRuleId::BasicMoves)
            && !target_loaded
            && (visible_reverse_prompt || visible_reverse_finesse || blind_reverse_finesse);
        if direct_reverse_connection {
            reverse_cycle_started = true;
        }
        let search_len = if target_loaded || reverse_cycle_started {
            hands.len()
        } else {
            ordinary_search_len
        };
        let directly_clued = hands[target.index()]
            .iter()
            .rev()
            .copied()
            .filter(|card| {
                *card != focus
                    && same_clue_touched.contains(card)
                    && promptable_before_clue.contains(card)
                    && !chop_moved.contains(card)
                    && !scheduled_cards.contains(card)
                    && pending_card_allows_identity(
                        pending,
                        convention_facts,
                        *card,
                        expected,
                        stack_heights,
                    )
                    && identity_of(view, *card) == Some(expected)
            })
            .collect::<Vec<_>>();
        if !directly_clued.is_empty() {
            found = Some((target, directly_clued, HGroupConnectionKind::Prompt));
            actor_index = target.index();
        }
        // A visible Prompt anywhere in turn order takes precedence over making
        // an earlier player blind-play a Finesse. Searching both connection
        // kinds in a single player loop incorrectly stopped at that earlier
        // Finesse and never reached an existing clued connector.
        for distance in 0..search_len {
            if found.is_some() {
                break;
            }
            let candidate_index = (actor_index + distance) % hands.len();
            let actor = PlayerId::new(
                u8::try_from(candidate_index).expect("standard Hanabi has at most five players"),
            );
            if actor == giver {
                continue;
            }
            let prompt_cards = hands[candidate_index]
                .iter()
                .rev()
                .copied()
                .filter(|card| {
                    *card != focus
                        && promptable_before_clue.contains(card)
                        && !chop_moved.contains(card)
                        && !already_playing.contains(card)
                        && !scheduled_cards.contains(card)
                        && pending_card_allows_identity(
                            pending,
                            convention_facts,
                            *card,
                            expected,
                            stack_heights,
                        )
                        && identity_of(view, *card).map_or_else(
                            || {
                                if actor == view.observer && giver == view.observer {
                                    // A clue giver cannot knowingly build a Prompt through
                                    // one of their own merely-compatible hidden cards. From
                                    // their perspective, that card must already be established
                                    // as the connector. Other observers must still respect an
                                    // ambiguous Prompt in their own hand.
                                    facts[card.index()].identity_mask() == 1 << expected.index()
                                        || convention_focus_is_live_identity(
                                            *card,
                                            expected,
                                            view,
                                            clues,
                                            already_playing,
                                            stack_heights,
                                            false,
                                        )
                                } else {
                                    facts[card.index()].allows(expected)
                                        && HistoricalView::new(view, turn)
                                            .has_unseen_copy(expected, hands)
                                }
                            },
                            |actual| actual == expected,
                        )
                })
                .collect::<Vec<_>>();
            if !prompt_cards.is_empty() {
                found = Some((actor, prompt_cards, HGroupConnectionKind::Prompt));
                actor_index = candidate_index;
                break;
            }
        }
        let mut unknown_observer_fallback = None;
        if found.is_none() {
            for distance in 0..search_len {
                let candidate_index = (actor_index + distance) % hands.len();
                let actor = PlayerId::new(
                    u8::try_from(candidate_index)
                        .expect("standard Hanabi has at most five players"),
                );
                if actor == giver {
                    continue;
                }
                let queued = hands[candidate_index]
                    .iter()
                    .rev()
                    .copied()
                    .filter(|card| {
                        *card != focus
                            && already_playing.contains(card)
                            && !declined_direct_plays.contains(card)
                            && !scheduled_cards.contains(card)
                            && pending_card_allows_identity(
                                pending,
                                convention_facts,
                                *card,
                                expected,
                                stack_heights,
                            )
                            && (identity_of(view, *card) == Some(expected)
                                || facts[card.index()].identity_mask() == 1 << expected.index()
                                || (actor == view.observer
                                    && facts[card.index()].allows(expected)
                                    && is_playable_at(stack_heights, expected))
                                || convention_focus_is_live_identity(
                                    *card,
                                    expected,
                                    view,
                                    clues,
                                    already_playing,
                                    stack_heights,
                                    actor == view.observer,
                                ))
                    })
                    // Good Touch gives an existing physically clued play
                    // precedence over an otherwise-compatible invisible
                    // candidate. Creating a second connector would duplicate
                    // the established promise and make the player abandon the
                    // card the team already expects to play.
                    .min_by_key(|card| !was_clued_before(view, turn, *card));
                if let Some(card) = queued {
                    found = Some((actor, vec![card], HGroupConnectionKind::Prompt));
                    actor_index = candidate_index;
                    break;
                }
            }
        }
        if found.is_none() {
            // If several visible players have the same connector on Finesse
            // Position, the earlier player trusts that the clue is directed
            // at the later visible copy. The clue giver and recipient must
            // therefore schedule that later copy too; otherwise their shared
            // connection graph disagrees with the blind players' Ambiguous
            // Finesse interpretation.
            // Source: https://hanabi.github.io/level-5/#the-ambiguous-finesse
            let visible_finesse_actors = (0..search_len)
                .filter_map(|distance| {
                    let candidate_index = (actor_index + distance) % hands.len();
                    if candidate_index == target.index() || candidate_index == giver.index() {
                        return None;
                    }
                    let card = hands[candidate_index].iter().rev().copied().find(|card| {
                        !promptable_before_clue.contains(card)
                            && !invisibly_clued.contains(card)
                            && !scheduled_cards.contains(card)
                            && *card != focus
                            && !visible_reverse_candidate_was_just_declined(
                                view,
                                turn,
                                PlayerId::new(
                                    u8::try_from(candidate_index)
                                        .expect("standard Hanabi has at most five players"),
                                ),
                                *card,
                                expected,
                                allow_blind_reverse_empathy,
                            )
                    })?;
                    (identity_of(view, card) == Some(expected)).then_some((candidate_index, card))
                })
                .collect::<Vec<_>>();
            let ambiguous_visible_actor = (visible_finesse_actors.len() > 1)
                .then(|| visible_finesse_actors.last().map(|(actor, _)| *actor))
                .flatten();
            for distance in 0..search_len {
                let candidate_index = (actor_index + distance) % hands.len();
                let actor = PlayerId::new(
                    u8::try_from(candidate_index)
                        .expect("standard Hanabi has at most five players"),
                );
                if target == actor || giver == actor {
                    continue;
                }
                let gotten = promptable_before_clue
                    .union(invisibly_clued)
                    .copied()
                    .collect::<CardSet>();
                let unclued = hands[candidate_index]
                    .iter()
                    .rev()
                    .copied()
                    .filter(|card| {
                        !gotten.contains(card) && !scheduled_cards.contains(card) && *card != focus
                    })
                    .collect::<Vec<_>>();
                if unclued.first().is_some_and(|card| {
                    visible_reverse_candidate_was_just_declined(
                        view,
                        turn,
                        actor,
                        *card,
                        expected,
                        allow_blind_reverse_empathy,
                    )
                }) {
                    continue;
                }
                let cards = if let Some(card) = elimination_card_for(actor) {
                    // Elimination changes this actor's Finesse position, not
                    // the validity of the whole clue. Select before checking
                    // visibility so a wrong selected card cannot be skipped.
                    (!scheduled_cards.contains(&card)
                        && (actor == view.observer || identity_of(view, card) == Some(expected)))
                    .then_some(card)
                    .into_iter()
                    .collect()
                } else if rule_enabled(profile, HGroupRuleId::SpecialFinesses) {
                    if actor == view.observer {
                        if giver == view.observer {
                            unclued
                                .iter()
                                .position(|card| {
                                    IdentitySet::from_mask(facts[card.index()].identity_mask())
                                        == IdentitySet::singleton(expected)
                                })
                                .map_or_else(Vec::new, |position| unclued[..=position].to_vec())
                        } else {
                            // In a multi-rank sequence, each rank begins at
                            // one finesse position. If that position is a lie,
                            // the intervening Fix advances this obligation to
                            // the next position. The final rank may still be a
                            // normal layered finesse with several candidates.
                            let current_len = if offset.saturating_add(1) < connection_count {
                                1
                            } else {
                                unclued.len()
                            };
                            unclued[..current_len.min(unclued.len())].to_vec()
                        }
                    } else {
                        unclued
                            .iter()
                            .position(|card| identity_of(view, *card) == Some(expected))
                            .filter(|position| {
                                // Visibility alone does not make a connector
                                // eligible. It must be on Finesse Position, or
                                // reachable through successful layered plays.
                                // An unplayable intervening card cannot make
                                // the observer defer their own blind play.
                                // A separately planned Fix may remove a lie
                                // component; mere visibility cannot invent it.
                                // https://hanabi.github.io/beginner/finesse/#finesse-position
                                // https://hanabi.github.io/level-5/#the-layered-finesse
                                let mut heights = stack_heights;
                                unclued[..*position].iter().all(|card| {
                                    identity_of(view, *card).is_some_and(|identity| {
                                        if !is_playable_at(heights, identity) {
                                            return required_fix.is_some_and(|fix| {
                                                fix.target == actor && fix.focus == *card
                                            });
                                        }
                                        heights[identity.suit.index()] = identity.rank.number();
                                        true
                                    })
                                })
                            })
                            .map_or_else(Vec::new, |position| unclued[..=position].to_vec())
                    }
                } else if actor == view.observer && giver == view.observer {
                    unclued
                        .first()
                        .copied()
                        .filter(|card| {
                            IdentitySet::from_mask(facts[card.index()].identity_mask())
                                == IdentitySet::singleton(expected)
                        })
                        .into_iter()
                        .collect()
                } else {
                    unclued.first().copied().into_iter().collect()
                };
                if !cards.is_empty() {
                    if ambiguous_visible_actor.is_some_and(|chosen| chosen != candidate_index)
                        && cards.len() == 1
                        && cards
                            .first()
                            .is_some_and(|card| identity_of(view, *card) == Some(expected))
                    {
                        continue;
                    }
                    if actor == view.observer && giver != view.observer {
                        // From the possible blind player's perspective their
                        // own identities are unknown. Prefer a later, visible
                        // connector when one exists: under Ambiguous Finesse,
                        // the earlier player trusts that the clue is directed
                        // at the teammate whose connection they can see.
                        unknown_observer_fallback.get_or_insert((
                            actor,
                            cards,
                            HGroupConnectionKind::Finesse,
                        ));
                        continue;
                    }
                    found = Some((actor, cards, HGroupConnectionKind::Finesse));
                    actor_index = candidate_index;
                    break;
                }
            }
            if found.is_none() {
                if let Some((actor, cards, kind)) = unknown_observer_fallback {
                    actor_index = actor.index();
                    found = Some((actor, cards, kind));
                }
            }
        }
        if found.is_none() && rule_enabled(profile, HGroupRuleId::Elimination) {
            if let Some((actor, card, elimination_identity)) = elimination_finesse_connection(
                view,
                hands,
                Some(facts),
                Some(HistoricalView::new(view, turn)),
                convention_facts,
                chop_moved,
                focus,
                expected,
            ) {
                if actor == target
                    && elimination_identity == expected
                    && !scheduled_cards.contains(&card)
                {
                    actor_index = actor.index();
                    found = Some((actor, vec![card], HGroupConnectionKind::Finesse));
                }
            }
        }
        let Some((actor, cards, kind)) = found else {
            break;
        };
        scheduled_cards.extend(cards.iter().copied());
        let connection_cards = cards.clone();
        let promise = pending.start(
            turn,
            ConnectionObligation {
                promise: PromiseId::UNASSIGNED,
                actor,
                cards,
                expected,
                focus_identity,
                kind,
                focus,
                step: offset,
            },
        );
        if kind == HGroupConnectionKind::Finesse && promise != PromiseId::UNASSIGNED {
            invisibly_clued.extend_from(EffectSource::Promise(promise), connection_cards);
        }
        if promise != PromiseId::UNASSIGNED {
            if let Some(connection) = pending
                .iter()
                .find(|connection| connection.promise == promise)
                .cloned()
            {
                scheduled_connections.push(connection);
            }
        }
        actor_index = (actor_index + 1) % hands.len();
    }
    scheduled_connections
}

/// Extends an already queued Finesse when a later clue demonstrates playable
/// cards in front of its current Finesse Position.
///
/// [Layered Finesse](https://hanabi.github.io/level-5/#the-layered-finesse)
/// semantics are cumulative: the old connector remains due, but each visible
/// playable card in front of it must now be played first. Treating the queued
/// identity as an unconditional reason to skip connection scheduling loses
/// precisely this kind of higher-efficiency follow-up.
#[allow(clippy::too_many_arguments)]
fn extend_queued_finesse_with_playable_layers(
    view: &PlayerView,
    giver: PlayerId,
    target: PlayerId,
    expected: Card,
    hands: &[Vec<CardId>],
    promptable_before_clue: &CardSet,
    invisibly_clued: &mut ProvenancedCardSet,
    stack_heights: [u8; 5],
    pending: &mut ConnectionManager,
    turn: u32,
) -> Option<ConnectionObligation> {
    let connection = pending
        .iter()
        .find(|connection| {
            connection.expected == expected
                && connection.kind == HGroupConnectionKind::Finesse
                && pending.is_active(connection)
        })?
        .clone();
    let current_position = *connection.cards.first()?;
    let gotten = promptable_before_clue
        .union(invisibly_clued)
        .copied()
        .collect::<CardSet>();
    let player_count = hands.len();
    let queued_distance = (connection.actor.index() + player_count - giver.index()) % player_count;
    let nearer_visible_connector = (1..queued_distance).any(|distance| {
        let player = (giver.index() + distance) % player_count;
        player != target.index()
            && finesse_position_id(&hands[player], &gotten, 0)
                .is_some_and(|card| identity_of(view, card) == Some(expected))
    });
    if nearer_visible_connector {
        return None;
    }
    let mut layers = Vec::new();
    let mut reached_current_position = false;
    for card in hands[connection.actor.index()].iter().rev().copied() {
        if card == current_position {
            reached_current_position = true;
            break;
        }
        if gotten.contains(&card) {
            continue;
        }
        let identity = identity_of(view, card)?;
        if !is_playable_at(stack_heights, identity) {
            return None;
        }
        layers.push(card);
    }
    if !reached_current_position || layers.is_empty() {
        return None;
    }
    invisibly_clued.extend_from(
        EffectSource::Promise(connection.promise),
        layers.iter().copied(),
    );
    pending.prepend_layers(turn, connection.promise, &layers)
}

/// Whether the observer may infer that their own unknown Finesse Position is
/// the otherwise-unaccounted connector in a Reverse Finesse.
///
/// This empathy inference is actionable only for the player whose turn it is.
/// A direct clue just received by that player takes precedence; speculative
/// projections must not reinterpret an older clue as a competing blind play.
fn blind_reverse_finesse_is_eligible(
    view: &PlayerView,
    giver: PlayerId,
    allow_blind_reverse_empathy: bool,
) -> bool {
    allow_blind_reverse_empathy
        && giver != view.observer
        && view.observer == view.current_player
        && !matches!(
            view.history.last().map(|entry| &entry.event),
            Some(ObservedEvent::Clued { target, .. }) if *target == view.observer
        )
}

/// In an Ambiguous Reverse Finesse, a visible player who clues instead of
/// taking their apparent blind play demonstrates that the connector is in a
/// later player's hidden hand. The acting recipient may therefore use the
/// blind-reverse empathy branch rather than continuing to trust the visible
/// duplicate. This is intentionally limited to the immediately preceding
/// clue: older actions need their own connection-lifecycle evidence.
/// <https://hanabi.github.io/level-5/#the-ambiguous-finesse>
fn visible_reverse_candidate_was_just_declined(
    view: &PlayerView,
    connection_turn: u32,
    actor: PlayerId,
    card: CardId,
    expected: Card,
    allow_blind_reverse_empathy: bool,
) -> bool {
    allow_blind_reverse_empathy
        && actor != view.observer
        && view.observer == view.current_player
        && view.hands[actor.index()]
            .iter()
            .any(|candidate| candidate.id == card)
        && view.play_stacks[expected.suit.index()].len() < usize::from(expected.rank.number())
        && view.history.last().is_some_and(|entry| {
            entry.turn > connection_turn
                && matches!(entry.event, ObservedEvent::Clued { giver, .. } if giver == actor)
        })
}

fn convention_focus_is_live_identity(
    card: CardId,
    expected: Card,
    view: &PlayerView,
    clues: &[HGroupClueInterpretation],
    already_playing: &CardSet,
    stack_heights: [u8; 5],
    allow_ambiguous_owned_identity: bool,
) -> bool {
    let Some(clue) = clues.iter().rev().find(|clue| clue.focus == card) else {
        return false;
    };
    let mut live = IdentitySet::from_mask(
        clue.play_identities
            .iter()
            .filter(|identity| identity.rank.number() > stack_heights[identity.suit.index()])
            .fold(0, |mask, identity| mask | (1 << identity.index())),
    );
    for other in already_playing
        .iter()
        .copied()
        .filter(|other| *other != card)
    {
        let claimed = identity_of(view, other).or_else(|| {
            let clue = clues.iter().rev().find(|clue| clue.focus == other)?;
            let identities = IdentitySet::from_mask(
                clue.play_identities
                    .iter()
                    .filter(|identity| {
                        identity.rank.number() > stack_heights[identity.suit.index()]
                    })
                    .fold(0, |mask, identity| mask | (1 << identity.index())),
            );
            (identities.len() == 1)
                .then(|| identities.iter().next())
                .flatten()
        });
        if let Some(claimed) = claimed {
            live = live.without(IdentitySet::singleton(claimed));
        }
    }
    live == IdentitySet::singleton(expected)
        || (allow_ambiguous_owned_identity && live.contains(expected))
}

fn pending_card_allows_identity(
    pending: &ConnectionManager,
    convention_facts: &ConventionFacts,
    card: CardId,
    identity: Card,
    stack_heights: [u8; 5],
) -> bool {
    // A Fix retracts the card's former Play meaning. Until a later Play clue
    // explicitly reactivates that card, it cannot become a Prompt merely
    // because its physical clues still allow the connecting identity. This is
    // essential for Reverse Finesses: the recipient must ignore an older
    // fixed card before looking for the visible connector in a later hand.
    // Source: https://hanabi.github.io/level-2/#the-reverse-finesse
    if convention_facts.fixed_cards().contains(&card) {
        return false;
    }
    let is_conditional_connection = |source| {
        matches!(
            source,
            HGroupMoveKind::Prompt
                | HGroupMoveKind::Finesse
                | HGroupMoveKind::ReverseFinesse
                | HGroupMoveKind::SelfFinesse
                | HGroupMoveKind::LayeredFinesse
                | HGroupMoveKind::HiddenFinesse
                | HGroupMoveKind::ClandestineFinesse
                | HGroupMoveKind::QueuedFinesse
                | HGroupMoveKind::AmbiguousFinesse
                | HGroupMoveKind::Bluff
                | HGroupMoveKind::SelfBluff
                | HGroupMoveKind::ThreeBluff
                | HGroupMoveKind::CriticalColorBluff
                | HGroupMoveKind::HardBluff
                | HGroupMoveKind::GoodTouchBluff
                | HGroupMoveKind::DoubleBluff
                | HGroupMoveKind::HardDoubleBluff
                | HGroupMoveKind::PestilentDoubleBluff
                | HGroupMoveKind::TrashBluff
                | HGroupMoveKind::NoInformationDoubleBluff
                | HGroupMoveKind::SelfColorBluff
                | HGroupMoveKind::SelfColorDoubleBluff
                | HGroupMoveKind::EliminationBluff
                | HGroupMoveKind::KnownPriorityBluff
                | HGroupMoveKind::PestilentTripleBluff
                | HGroupMoveKind::PassBluff
                | HGroupMoveKind::PurgeBluff
        )
    };
    let mut hard_claims = convention_facts.identity_claims().iter().filter(|claim| {
        claim.relation == IdentityClaimRelation::Each
            && claim.cards.contains(&card)
            && !is_conditional_connection(claim.source)
    });
    if hard_claims.clone().any(|claim| claim.identity == identity) {
        // A later conditional connection cannot erase an identity already
        // established by a direct clue or another hard convention fact.
        return true;
    }
    let conflicts_with_hard_claim = hard_claims.any(|claim| claim.identity != identity);
    !conflicts_with_hard_claim
        && !convention_facts
            .excluded_identities(card)
            .contains(identity)
        && !pending.iter().any(|connection| {
            connection.cards.len() == 1
                && connection.cards.contains(&card)
                && connection.expected != identity
                && !is_playable_at(stack_heights, identity)
        })
}

fn identity_is_queued_before_target(
    view: &PlayerView,
    giver: PlayerId,
    target: PlayerId,
    already_playing: &CardSet,
    pending: &ConnectionManager,
    identity: Card,
) -> bool {
    let player_count = view.hands.len();
    let target_distance = (target.index() + player_count - giver.index()) % player_count;
    let acts_before_target = |player: PlayerId| {
        let distance = (player.index() + player_count - giver.index()) % player_count;
        distance != 0 && distance <= target_distance
    };
    let owner = |card: CardId| {
        view.hands
            .iter()
            .position(|hand| hand.iter().any(|candidate| candidate.id == card))
            .map(|player| {
                PlayerId::new(
                    u8::try_from(player).expect("standard Hanabi has at most five players"),
                )
            })
    };
    pending.iter().any(|connection| {
        (connection.expected == identity && acts_before_target(connection.actor))
            || (connection.focus_identity == identity
                && owner(connection.focus).is_some_and(acts_before_target))
    }) || already_playing.iter().any(|card| {
        identity_of(view, *card) == Some(identity) && owner(*card).is_some_and(acts_before_target)
    })
}

fn replay_identity_is_queued(view: &PlayerView, replay: &HGroupState, identity: Card) -> bool {
    let identity_is_still_useful =
        usize::from(identity.rank.number()) > view.play_stacks[identity.suit.index()].len();
    let card_is_still_held = |card: CardId| {
        view.hands
            .iter()
            .any(|hand| hand.iter().any(|candidate| candidate.id == card))
    };
    replay.pending_connections.identity_is_queued(identity)
        || replay.cards.already_playing.iter().any(|card| {
            identity_of(view, *card) == Some(identity)
                || replay.clues.iter().rev().any(|clue| {
                    clue.focus == *card && clue.focus_identities == IdentitySet::singleton(identity)
                })
        })
        || (identity_is_still_useful
            && replay.clues.iter().any(|clue| {
                clue.non_focus_identities.iter().any(|(card, identities)| {
                    card_is_still_held(*card) && *identities == IdentitySet::singleton(identity)
                })
            }))
}

fn identity_set(identities: impl IntoIterator<Item = Card>) -> IdentitySet {
    identities
        .into_iter()
        .fold(IdentitySet::default(), |set, identity| {
            set.union(IdentitySet::singleton(identity))
        })
}

#[cfg(test)]
mod tests;
