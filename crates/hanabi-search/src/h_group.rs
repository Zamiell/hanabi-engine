//! H-Group convention inference.
//!
//! This module deliberately contains interpretations, not game rules or
//! logical clue facts. H-Group profiles are cumulative: a level-N profile
//! enables every interpretation through level N, while `max` also enables the
//! rare moves in the extras chapters of the pinned ruleset.

use std::cell::RefCell;
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
mod action_schedule;
mod bluff;
mod candidate;
mod candidate_pipeline;
mod claims;
mod connection;
mod constraint_graph;
mod constraints;
mod coverage;
mod decision;
mod effects;
mod epistemic;
mod event_reducer;
mod facts;
mod hand;
mod hypothesis;
mod identity;
mod information_value;
mod interpretation;
mod knowledge_effects;
mod ledger;
mod model;
mod outcome;
mod perspective;
mod plan;
mod primary;
mod prospective;
mod rationality;
mod recognition;
mod rule_engine;
mod rules;
mod strategic_value;
mod symbolic_line;
mod transition;
mod turn_context;

use action_analysis::{HGroupActionKind, HGroupActionSet, HGroupAnalyzedAction};
use action_schedule::{ActionSchedule, StackTimeline};
use bluff::{
    BluffTargetKind, bluff_play_connects, bluff_target_kind_at, bluff_target_order_is_legal,
};
use candidate::{ClueCandidate, CluePurpose, ClueRecognition, ClueSchedule, ClueValue};
use candidate_pipeline::SemanticallyAdmittedCandidates;
use claims::{IdentityClaims, claimed_identities_at_clue};
use connection::{ConnectionManager, ConnectionObligation, ConnectionTransitionReason, PromiseId};
use constraint_graph::ConventionConstraintGraph;
use constraints::{ConstraintReason, ConventionConstraints};
pub use coverage::{H_GROUP_DOCUMENTATION_SECTIONS, HGroupDocumentationSection};
pub(crate) use decision::analyze_h_group_convention;
pub use decision::infer_h_group;
use decision::{
    h_group_predictable_action, infer_h_group_from_replay, ordered_playable_cards,
    positional_discard_candidate, positional_discard_is_valid_snapshot, preferred_due_play_card,
};
#[cfg(test)]
use decision::{ordered_h_group_actions, select_h_group_action};
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
    is_eventually_useful, is_playable_at, is_playable_now, is_trash_at, is_unique_visible,
};
use information_value::convention_information_value;
#[cfg(test)]
use interpretation::h_group_clue_candidates;
use interpretation::{
    build_convention_knowledge, convention_card_inferences, creates_false_anxiety,
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
use outcome::{ActionCommitment, CluedCardSuperposition, LineOutcome};
use perspective::{PerspectiveProjector, ProspectiveTransition};
use plan::{ConditionalPlan, PlanFrontier, ProjectedAction, ProjectedConsequences};
use primary::{ClueInterpretationPlan, PrimaryClueInputs};
#[cfg(test)]
use prospective::prospective_clue_hazard;
use prospective::{
    CachedProspectiveProjection, SubjectiveReplayRequest, TeamConventionSnapshot,
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
pub(crate) use symbolic_line::project_h_group_line;
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
    /// [Directness Principle](https://hanabi.github.io/level-10/#directness-principle)
    Directness,
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
        effects: &[HGroupMoveKind::TempoClue, HGroupMoveKind::TempoClueChopMove],
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
            HGroupMoveKind::Directness,
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
}

#[allow(clippy::too_many_lines)]
fn replay_h_group_inner(
    deductions: &LogicalDeductions,
    profile: HGroupProfile,
    perspective_depth: PerspectiveDepth,
    allow_blind_reverse_empathy: bool,
) -> HGroupState {
    debug_assert!(rule_enabled(profile, HGroupRuleId::Basic));
    let view = deductions.view();
    let hand_size = if view.hands.len() <= 3 { 5 } else { 4 };
    let mut hands = (0..view.hands.len())
        .map(|player| {
            let first = player * hand_size;
            (first..first + hand_size).map(CardId::new).collect()
        })
        .collect::<Vec<Vec<CardId>>>();
    let mut explicitly_clued = ProvenancedCardSet::default();
    let mut invisibly_clued = ProvenancedCardSet::default();
    let mut clues = Vec::<HGroupClueInterpretation>::new();
    let mut public_removed = [0_u8; 25];
    let mut facts = vec![ClueFacts::default(); 50];
    let mut stack_heights = [0_u8; 5];
    let mut historical_deck_size = view.deck_size
        + view
            .history
            .iter()
            .filter(|entry| matches!(entry.event, ObservedEvent::Drew { .. }))
            .count();
    let mut pending_connections = ConnectionManager::default();
    let mut already_playing = ProvenancedCardSet::default();
    let mut early_game = true;
    let mut signals = ConventionJournal::default();
    let mut chop_moved = ProvenancedCardSet::default();
    let mut discard_now = Vec::new();
    let mut must_clue = PlayerSet::default();
    let mut forced_playable = ProvenancedCardSet::default();
    let mut invalidated_focuses = CardSet::default();
    let mut declined_direct_plays = CardSet::default();
    let mut declined_direct_play_turns = Vec::<(CardId, u32)>::new();
    let mut implicit_saves = Vec::new();
    let mut required_fixes = FixObligations::default();
    let mut transitions = Vec::new();
    let mut historical_clue_tokens = MAX_CLUE_TOKENS;

    for (entry_index, entry) in view.history.iter().enumerate() {
        let event_connection_transition_start = pending_connections.transitions().len();
        let event_card_snapshot = ConventionCardSetSnapshot::capture(
            &explicitly_clued,
            &invisibly_clued,
            &already_playing,
            &chop_moved,
            &forced_playable,
        );
        let historical = HistoricalView::new(view, entry.turn);
        let mut actor_saw_normal_discard = false;
        let mut actor_known_discard_identity = None;
        let mut declined_with_clue = None;
        let mut required_clue_deferral = false;
        let action_is_settled = view.history[entry_index + 1..]
            .iter()
            .any(|later| later.turn > entry.turn);
        let before = HGroupTurnSnapshot::new(
            &hands,
            &facts,
            stack_heights,
            historical_clue_tokens,
            historical_deck_size,
            early_game,
            already_playing.materialized().clone(),
        );
        let clue_tokens_before = before.clue_tokens;
        match &entry.event {
            ObservedEvent::Clued {
                giver,
                target,
                clue,
                touched,
                untouched,
            } => {
                for card in touched {
                    declined_direct_plays.remove(card);
                    declined_direct_play_turns.retain(|(declined, _)| declined != card);
                }
                let promised_card_fix = pending_connections.iter().any(|connection| {
                    connection.actor == *target
                        && pending_is_active(connection, &pending_connections)
                        && !clue.matches(connection.expected)
                        && connection
                            .cards
                            .first()
                            .is_some_and(|card| touched.contains(card))
                });
                let signaled_card_fix = touched.iter().any(|card| {
                    let has_active_promise = invisibly_clued.contains(card)
                        || pending_connections
                            .iter()
                            .any(|connection| connection.cards.first() == Some(card));
                    has_active_promise
                        && !signals.facts().fixed_cards().contains(card)
                        && signals.facts().identity_claims().iter().rev().any(|claim| {
                            claim.turn < entry.turn
                                && claim.target == Some(*target)
                                && claim.cards.first() == Some(card)
                                && !clue.matches(claim.identity)
                                && matches!(
                                    claim.source,
                                    HGroupMoveKind::Finesse
                                        | HGroupMoveKind::ReverseFinesse
                                        | HGroupMoveKind::SelfFinesse
                                        | HGroupMoveKind::LayeredFinesse
                                        | HGroupMoveKind::ClandestineFinesse
                                        | HGroupMoveKind::QueuedFinesse
                                        | HGroupMoveKind::AmbiguousFinesse
                                )
                        })
                });
                let pre_clue_active_invisible =
                    active_invisibly_clued(&invisibly_clued, &pending_connections);
                let pre_clue_gotten =
                    protected_cards(&explicitly_clued, &pre_clue_active_invisible, &chop_moved);
                let hypothetical_connection_fix = clues.iter().rev().any(|prior| {
                    if !pending_connections.actor_had_pending_before(
                        prior.target,
                        prior.turn,
                        prior.focus,
                    ) {
                        return false;
                    }
                    prior.play_identities.iter().any(|identity| {
                        let first_connector = Card::new(
                            identity.suit,
                            Rank::ALL[usize::from(prior.stack_heights[identity.suit.index()])],
                        );
                        if matches!(prior.clue, Clue::Rank(_))
                            && !pending_connections
                                .identity_was_demonstrated_after(first_connector, prior.turn)
                        {
                            // A rank clue's delayed branch is not established
                            // merely by being structurally possible. A later
                            // clue fixes that branch only after its first
                            // connector publicly demonstrates it. Loaded color
                            // clues retain the immediate lie-component Fix
                            // exception.
                            return false;
                        }
                        loaded_connection_plan(
                            view,
                            Some(&hands),
                            Some(&facts),
                            Some(HistoricalView::new(view, prior.turn)),
                            prior.giver,
                            prior.target,
                            prior.focus,
                            identity,
                            &pre_clue_gotten,
                            &already_playing,
                            &pending_connections,
                            stack_heights,
                        )
                        .flatten()
                        .is_some_and(|required| {
                            required.actor == *giver
                                && required.target == *target
                                && touched.contains(&required.focus)
                                && clue.matches(required.identity)
                        })
                    })
                });
                let is_required_fix = promised_card_fix
                    || signaled_card_fix
                    || hypothetical_connection_fix
                    || required_fixes.iter().any(|obligation| {
                        let required = obligation.required;
                        required.actor == *giver
                            && required.target == *target
                            && touched.contains(&required.focus)
                            && clue.matches(required.identity)
                            && !was_clued_before_with(view, entry.turn, required.focus, *clue)
                    });
                declined_with_clue = Some(*giver);
                required_clue_deferral = is_required_fix;
                if is_required_fix {
                    // https://hanabi.github.io/level-3/#the-fix-clue
                    // Promise repair is decided and applied here, before the
                    // per-level recognizers run. Journal the same transition
                    // at its canonical mutation point so current facts do not
                    // mistake the physical clue for a Play/Tempo promise.
                    push_signal(
                        &mut signals,
                        entry,
                        *giver,
                        Some(*target),
                        HGroupMoveKind::FixClue,
                        touched.clone(),
                        None,
                    );
                    let fixed_cards = touched.iter().copied().collect::<CardSet>();
                    let mut occupied =
                        protected_cards(&explicitly_clued, &invisibly_clued, &chop_moved);
                    occupied.extend(
                        pending_connections
                            .iter()
                            .flat_map(|connection| connection.cards.iter().copied()),
                    );
                    pending_connections.repair_actor(
                        entry.turn,
                        *target,
                        |card| fixed_cards.contains(&card),
                        |_| {
                            let next = hands[target.index()].iter().rev().copied().find(|card| {
                                !fixed_cards.contains(card) && !occupied.contains(card)
                            });
                            if let Some(next) = next {
                                occupied.insert(next);
                                invisibly_clued.insert(next);
                            }
                            next
                        },
                    );
                    for fixed in fixed_cards {
                        invisibly_clued.remove(&fixed);
                    }
                }
                let active_invisible =
                    active_invisibly_clued(&invisibly_clued, &pending_connections);
                let gotten = protected_cards(&explicitly_clued, &active_invisible, &chop_moved);
                let hand = &hands[target.index()];
                let old_chop = chop(hand, &gotten);
                let newly_touched = touched
                    .iter()
                    .copied()
                    .filter(|card| !gotten.contains(card))
                    .collect::<Vec<_>>();
                let previously_promptable = explicitly_clued
                    .union(&active_invisible)
                    .copied()
                    .collect::<CardSet>();
                let displaced_connections = pending_connections
                    .iter()
                    .filter(|connection| connection.actor == *giver)
                    .filter(|connection| {
                        touched
                            .iter()
                            .any(|card| historical.identity(*card) == Some(connection.expected))
                    })
                    .flat_map(|connection| connection.cards.iter().copied())
                    .collect::<CardSet>();
                if !displaced_connections.is_empty() {
                    pending_connections.cancel_where(
                        entry.turn,
                        ConnectionTransitionReason::DisplacedByClue,
                        |connection| {
                            connection.actor == *giver
                                && touched.iter().any(|card| {
                                    historical.identity(*card) == Some(connection.expected)
                                })
                        },
                    );
                    for displaced in displaced_connections {
                        already_playing.remove(&displaced);
                        if !explicitly_clued.contains(&displaced)
                            && !pending_connections
                                .iter()
                                .any(|connection| connection.cards.contains(&displaced))
                        {
                            invisibly_clued.remove(&displaced);
                        }
                    }
                }
                if let Some(focus) = focus(hand, touched, old_chop, &gotten) {
                    let focus_identity = historical.identity(focus);
                    let focus_was_chop = old_chop == Some(focus)
                        || positional_discard_is_valid_snapshot(
                            view,
                            &hands,
                            *target,
                            focus,
                            historical_deck_size,
                            stack_heights,
                        );
                    for card in touched {
                        facts[card.index()].add_positive_clue(*clue);
                    }
                    for card in untouched {
                        facts[card.index()].add_negative_clue(*clue);
                    }
                    explicitly_clued.extend(touched.iter().copied());
                    let raw_focus_identities = focus_identity.map_or_else(
                        || IdentitySet::from_mask(facts[focus.index()].identity_mask()),
                        IdentitySet::singleton,
                    );
                    let mut focus_identities = raw_focus_identities;
                    let mut claimed_identities = IdentitySet::default();
                    if focus_identity.is_none() {
                        // Good Touch lets a recipient eliminate identities
                        // already promised on live cards elsewhere. Apply the
                        // elimination to the whole focus domain, including
                        // Save possibilities: a newly touched 2 beside an
                        // existing saved Red 2 cannot itself be Red 2.
                        claimed_identities = claimed_identities_at_clue(
                            focus,
                            &hands,
                            &historical,
                            &facts,
                            signals.facts(),
                            &clues,
                            &gotten,
                            &pending_connections,
                        );
                        focus_identities = focus_identities.without(claimed_identities);
                    }
                    let active_connector_identity = pending_connections
                        .iter()
                        .find(|connection| {
                            connection.actor == *target
                                && connection.cards.first() == Some(&focus)
                                && clue.matches(connection.expected)
                                && pending_is_active(connection, &pending_connections)
                        })
                        .map(|connection| connection.expected);
                    let clue_promptable = previously_promptable
                        .iter()
                        .copied()
                        .chain(touched.iter().copied().filter(|card| *card != focus))
                        .collect::<CardSet>();
                    let mut play_identities = active_connector_identity.map_or_else(
                        || {
                            snapshot_play_identities(
                                profile,
                                focus_identities,
                                *giver,
                                *target,
                                focus,
                                view,
                                &hands,
                                &facts,
                                &previously_promptable,
                                &already_playing,
                                &pending_connections,
                                signals.facts(),
                                &chop_moved,
                                stack_heights,
                                entry.turn,
                                allow_blind_reverse_empathy,
                            )
                        },
                        IdentitySet::singleton,
                    );
                    if focus_identity.is_none()
                        && *clue == Clue::Rank(Rank::Two)
                        && !claimed_identities.is_empty()
                    {
                        // Good Touch rejects a duplicated direct play, but it
                        // must not erase an independently valid delayed
                        // connection before the recipient can retain that
                        // branch in superposition. For example, a rank-2 clue
                        // can still mean green 2 through a visible green-1
                        // Reverse Finesse even when another touched card is
                        // provisionally claimed as green 2. Candidate
                        // validation decides whether the overall clue is a
                        // useful duplication; rank-2 identity compilation must
                        // first preserve every convention-readable branch.
                        let delayed_claimed = IdentitySet::from_mask(
                            raw_focus_identities
                                .intersection(claimed_identities)
                                .iter()
                                .filter(|identity| {
                                    identity.rank.number()
                                        > stack_heights[identity.suit.index()] + 1
                                })
                                .fold(0, |mask, identity| mask | (1 << identity.index())),
                        );
                        let delayed_claimed_plays = snapshot_play_identities(
                            profile,
                            delayed_claimed,
                            *giver,
                            *target,
                            focus,
                            view,
                            &hands,
                            &facts,
                            &previously_promptable,
                            &already_playing,
                            &pending_connections,
                            signals.facts(),
                            &chop_moved,
                            stack_heights,
                            entry.turn,
                            allow_blind_reverse_empathy,
                        );
                        focus_identities = focus_identities.union(delayed_claimed_plays);
                        play_identities = play_identities.union(delayed_claimed_plays);
                    }
                    let mut intermediate_bluff = false;
                    if rule_enabled(profile, HGroupRuleId::IntermediateBluffs)
                        && *clue == Clue::Rank(Rank::Three)
                        && focus_identities
                            .iter()
                            .all(|identity| !is_playable_at(stack_heights, identity))
                    {
                        let actor = next_player(*giver, hands.len());
                        let bluff_card = finesse_position_id(&hands[actor.index()], &gotten, 0);
                        let bluff_is_credible = bluff_card.is_some_and(|card| {
                            historical
                                .identity(card)
                                .map_or(actor == view.observer, |identity| {
                                    is_playable_at(stack_heights, identity)
                                        && !bluff_play_connects(*clue, identity)
                                })
                        });
                        if bluff_target_order_is_legal(*clue, actor, *target) && bluff_is_credible {
                            let three_bluff_targets = IdentitySet::from_mask(
                                focus_identities
                                    .iter()
                                    .filter(|identity| {
                                        bluff_target_kind_at(stack_heights, *clue, *identity)
                                            == Some(BluffTargetKind::Three)
                                    })
                                    .fold(0, |mask, identity| mask | (1 << identity.index())),
                            );
                            intermediate_bluff = !three_bluff_targets.is_empty();
                            play_identities = play_identities.union(three_bluff_targets);
                        }
                    }
                    let eight_clue_save = rule_enabled(profile, HGroupRuleId::Stalling)
                        && !early_game
                        && clue_tokens_before == MAX_CLUE_TOKENS
                        && hands[target.index()].last() != Some(&focus);
                    let save_identities = snapshot_save_identities(
                        focus_identities,
                        *clue,
                        *giver,
                        focus,
                        focus_was_chop,
                        eight_clue_save,
                        view,
                        &hands,
                        &gotten,
                        play_identities,
                        stack_heights,
                        public_removed,
                    );
                    let score = stack_heights
                        .iter()
                        .map(|height| usize::from(*height))
                        .sum::<usize>();
                    let low_score_number_five = rule_enabled(profile, HGroupRuleId::FiveTech)
                        && *clue == Clue::Rank(Rank::Five)
                        && score < 2 * Suit::ALL.len();
                    let early_five_stall = rule_enabled(profile, HGroupRuleId::BasicMoves)
                        && early_game
                        && *clue == Clue::Rank(Rank::Five)
                        && !focus_was_chop;
                    let eight_clue_five_stall = rule_enabled(profile, HGroupRuleId::Stalling)
                        && !early_game
                        && clue_tokens_before == MAX_CLUE_TOKENS
                        && *clue == Clue::Rank(Rank::Five)
                        && !focus_was_chop
                        && !eight_clue_save;
                    let five_chop_move = rule_enabled(profile, HGroupRuleId::ChopMoves)
                        && *clue == Clue::Rank(Rank::Five)
                        && !early_five_stall
                        && !eight_clue_five_stall
                        && five_chop_moved_card(&hands[target.index()], touched, &gotten).is_some();
                    let no_information_reclue = touched
                        .iter()
                        .all(|card| was_clued_before_with(view, entry.turn, *card, *clue));
                    let interpretation_plan = ClueInterpretationPlan::resolve(PrimaryClueInputs {
                        clue: *clue,
                        play_identities,
                        save_identities,
                        stack_heights,
                        eight_clue_save,
                        suppressions: [
                            is_required_fix.then_some(primary::PrimarySuppression::Fix),
                            five_chop_move.then_some(primary::PrimarySuppression::FiveChopMove),
                            low_score_number_five
                                .then_some(primary::PrimarySuppression::LowScoreFive),
                            early_five_stall.then_some(primary::PrimarySuppression::EarlyFiveStall),
                            eight_clue_five_stall
                                .then_some(primary::PrimarySuppression::EightClueFiveStall),
                            no_information_reclue
                                .then_some(primary::PrimarySuppression::NoInformationReclue),
                        ],
                    });
                    let kind = interpretation_plan.kind;
                    play_identities = interpretation_plan.play_identities;
                    let save_identities = interpretation_plan.save_identities;
                    debug_assert_eq!(
                        interpretation_plan.suppression.is_some(),
                        matches!(kind, HGroupClueKind::Unrecognized)
                            && (is_required_fix
                                || five_chop_move
                                || low_score_number_five
                                || early_five_stall
                                || eight_clue_five_stall
                                || no_information_reclue)
                    );
                    let target_already_loaded = pending_connections.iter().any(|connection| {
                        connection.actor == *target
                            && pending_is_active(connection, &pending_connections)
                    });
                    let direct_play = IdentitySet::from_mask(
                        play_identities
                            .iter()
                            .filter(|identity| {
                                is_playable_at(stack_heights, *identity)
                                    && !pending_identity_is_queued(&pending_connections, *identity)
                            })
                            .fold(0, |mask, identity| mask | (1 << identity.index())),
                    );
                    let connection_context = ConnectionPlanningContext {
                        profile,
                        view,
                        turn: entry.turn,
                        giver: *giver,
                        target: *target,
                        focus,
                        clue: *clue,
                        touches: CurrentClueTouches(touched),
                        hands: &hands,
                        facts: &facts,
                        clues: &clues,
                        promptable_before: PromptableBeforeClue(&clue_promptable),
                        protected_before: &gotten,
                        already_playing: &already_playing,
                        declined_direct_plays: &declined_direct_plays,
                        convention_facts: signals.facts(),
                        chop_moved: &chop_moved,
                        stack_heights,
                        allow_blind_reverse_empathy,
                    };
                    let connection_hypotheses = play_identities
                        .iter()
                        .map(|identity| {
                            connection_context.simulate(
                                identity,
                                &pending_connections,
                                &invisibly_clued,
                            )
                        })
                        .collect::<Vec<_>>();
                    for hypothesis in &connection_hypotheses {
                        let Some(required) = hypothesis.required_fix else {
                            continue;
                        };
                        if focus_identity == Some(hypothesis.focus_identity) {
                            required_fixes.insert_unconditional(required);
                        } else if focus_identity.is_none() {
                            required_fixes.insert_conditional(
                                entry.turn,
                                focus,
                                hypothesis.focus_identity,
                                required,
                            );
                        }
                    }
                    let inferred_connection_identity = focus_identity
                        .or_else(|| {
                            (play_identities.len() == 1)
                                .then(|| play_identities.iter().next())
                                .flatten()
                        })
                        .or_else(|| {
                            // The recipient can initially have both a direct
                            // and a delayed identity in their note. Preserve a
                            // unique delayed interpretation so intervening
                            // blind plays can demonstrate and resolve it.
                            let delayed = IdentitySet::from_mask(
                                play_identities
                                    .iter()
                                    .filter(|identity| {
                                        identity.rank.number()
                                            > stack_heights[identity.suit.index()] + 1
                                    })
                                    .fold(0, |mask, identity| mask | (1 << identity.index())),
                            );
                            (delayed.len() == 1)
                                .then(|| delayed.iter().next())
                                .flatten()
                        })
                        .or_else(|| {
                            // A loaded clue can leave the recipient with
                            // several delayed identities. Prefer a clean line
                            // over one that first requires a Fix, then compare
                            // full executable line lengths, including playable
                            // layers before a connector. Rank alone cannot
                            // distinguish a plain Purple-2 Finesse from a
                            // Red-2 Clandestine Finesse through Purple 1.
                            rule_enabled(profile, HGroupRuleId::Extras)
                                .then(|| {
                                    connection_hypotheses
                                        .iter()
                                        .filter(|hypothesis| hypothesis.loaded)
                                        .max_by_key(|hypothesis| {
                                            let identity = hypothesis.focus_identity;
                                            let base = identity.rank.number().saturating_sub(
                                                stack_heights[identity.suit.index()],
                                            );
                                            let layers = hypothesis
                                                .connection_steps
                                                .iter()
                                                .map(|connection| {
                                                    connection.cards.len().saturating_sub(1)
                                                })
                                                .sum::<usize>();
                                            (
                                                hypothesis.required_fix.is_none(),
                                                usize::from(base) + layers,
                                                core::cmp::Reverse(identity.index()),
                                            )
                                        })
                                        .map(|hypothesis| hypothesis.focus_identity)
                                })
                                .flatten()
                        });
                    let connection_identity = if focus_identity.is_none()
                        && target_already_loaded
                        && !direct_play.is_empty()
                        && !inferred_connection_identity.is_some_and(|identity| {
                            connection_hypotheses.iter().any(|hypothesis| {
                                hypothesis.focus_identity == identity
                                    && hypothesis.loaded
                                    && hypothesis.required_fix.is_some()
                            })
                        }) {
                        // A new direct Play Clue to a loaded player gives them
                        // another explicit play. It does not manufacture a
                        // second speculative finesse from delayed identities
                        // that happen to match the same rank/color clue.
                        None
                    } else {
                        inferred_connection_identity
                    };
                    let focus_identities = if focus_was_chop {
                        play_identities.union(save_identities)
                    } else {
                        play_identities
                    };
                    let new_non_focus = newly_touched
                        .iter()
                        .copied()
                        .filter(|card| *card != focus)
                        .collect::<Vec<_>>();
                    let non_focus_identities = new_non_focus
                        .iter()
                        .copied()
                        .map(|card| {
                            let direct = historical.identity(card).map_or_else(
                                || IdentitySet::from_mask(facts[card.index()].identity_mask()),
                                IdentitySet::singleton,
                            );
                            let good_touch = snapshot_good_touch_identities(
                                card,
                                direct,
                                view,
                                &hands,
                                &previously_promptable,
                                stack_heights,
                                public_removed,
                            );
                            // Good Touch forbids an actual duplicate, but an
                            // ambiguous focus does not claim every identity in
                            // its domain. For a two-card rank-1 clue, the
                            // non-focus card is still safe as the first play;
                            // subtracting the focus's entire mask erased that
                            // promise. Larger duplicate-rank clues retain
                            // focused-play semantics: representing all their
                            // pairwise distinctness requires correlated belief
                            // branches rather than independent card masks.
                            let claimed_focus =
                                if focus_identities.len() == 1 || new_non_focus.len() != 1 {
                                    focus_identities
                                } else {
                                    IdentitySet::default()
                                };
                            let good_touch = good_touch.without(claimed_focus);
                            (card, good_touch)
                        })
                        .collect::<Vec<_>>();
                    let completing_suit = (matches!(kind, HGroupClueKind::Play)
                        && play_identities.len() == 1)
                        .then(|| play_identities.iter().next())
                        .flatten()
                        .filter(|identity| {
                            *clue == Clue::Suit(identity.suit)
                                && identity.rank == Rank::Five
                                && is_playable_at(stack_heights, *identity)
                        });
                    let non_focus_trash_identities = completing_suit.map_or_else(Vec::new, |_| {
                        non_focus_identities
                            .iter()
                            .filter_map(|(card, good_touch)| {
                                let direct = historical.identity(*card).map_or_else(
                                    || IdentitySet::from_mask(facts[card.index()].identity_mask()),
                                    IdentitySet::singleton,
                                );
                                let trash = direct.without(focus_identities).without(*good_touch);
                                (!trash.is_empty()).then_some((*card, trash))
                            })
                            .collect()
                    });
                    clues.push(HGroupClueInterpretation {
                        turn: entry.turn,
                        giver: *giver,
                        target: *target,
                        clue: *clue,
                        touched: touched.clone(),
                        stack_heights,
                        focus,
                        focus_was_chop,
                        kind,
                        focus_identities,
                        play_identities,
                        save_identities,
                        new_non_focus,
                        non_focus_identities,
                        non_focus_trash_identities,
                        // Prompt candidates need actual clue information.
                        // A chop-moved card is protected for chop/layout
                        // purposes, but remains an unknown card and cannot be
                        // Prompted merely because it was moved.
                        previously_gotten: previously_promptable.iter().copied().collect(),
                        hypotheses: connection_hypotheses,
                    });
                    let current_clue = clues
                        .last()
                        .expect("the current clue was just appended")
                        .clone();
                    let declined_alternatives =
                        declined_superior_clue_inferences(&DeclinedAlternativeContext {
                            view,
                            profile,
                            clue: &current_clue,
                            hands: &hands,
                            clue_facts: &facts,
                            historical,
                            gotten: &gotten,
                            promptable_before: &previously_promptable,
                            already_playing: already_playing.materialized(),
                            pending: &pending_connections,
                            convention_facts: signals.facts(),
                            chop_moved: chop_moved.materialized(),
                        });
                    for inference in declined_alternatives {
                        ConventionReducer::apply(
                            EffectBatch::declined_alternative(inference),
                            &mut signals,
                        );
                    }
                    let signal_kind = match kind {
                        HGroupClueKind::Play | HGroupClueKind::PlayOrSave => {
                            Some(HGroupMoveKind::PlayClue)
                        }
                        HGroupClueKind::Save(_) => Some(HGroupMoveKind::SaveClue),
                        HGroupClueKind::Unrecognized => None,
                    };
                    if let Some(signal_kind) = signal_kind {
                        let signal_identity = if signal_kind == HGroupMoveKind::PlayClue {
                            connection_identity.or(focus_identity)
                        } else {
                            focus_identity
                        };
                        push_signal(
                            &mut signals,
                            entry,
                            *giver,
                            Some(*target),
                            signal_kind,
                            vec![focus],
                            signal_identity,
                        );
                    }
                    if matches!(kind, HGroupClueKind::Play)
                        && !low_score_number_five
                        && !intermediate_bluff
                    {
                        let previous_connections = pending_connections.to_vec();
                        let committed_plan = ConnectionPlanningContext {
                            profile,
                            view,
                            turn: entry.turn,
                            giver: *giver,
                            target: *target,
                            focus,
                            clue: *clue,
                            touches: CurrentClueTouches(touched),
                            hands: &hands,
                            facts: &facts,
                            clues: &clues,
                            promptable_before: PromptableBeforeClue(&clue_promptable),
                            protected_before: &gotten,
                            already_playing: &already_playing,
                            declined_direct_plays: &declined_direct_plays,
                            convention_facts: signals.facts(),
                            chop_moved: &chop_moved,
                            stack_heights,
                            allow_blind_reverse_empathy,
                        };
                        let (new_connections, scheduled_fix) = committed_plan.commit(
                            connection_identity,
                            &mut pending_connections,
                            &mut invisibly_clued,
                        );
                        if let (Some(required), Some(identity)) =
                            (scheduled_fix, connection_identity)
                        {
                            if focus_identity.is_some() {
                                required_fixes.insert_unconditional(required);
                            } else {
                                required_fixes
                                    .insert_conditional(entry.turn, focus, identity, required);
                            }
                        }
                        reconcile_connection_fact_lifecycles(
                            &pending_connections,
                            event_connection_transition_start,
                            &mut invisibly_clued,
                            &mut already_playing,
                            &mut forced_playable,
                        );
                        for connection in &new_connections {
                            let elimination_finesse =
                                signals.facts().identity_claims().iter().any(|claim| {
                                    claim.source == HGroupMoveKind::Elimination
                                        && claim.target == Some(connection.actor)
                                        && claim.identity == connection.expected
                                        && connection
                                            .cards
                                            .first()
                                            .is_some_and(|card| claim.cards.contains(card))
                                });
                            push_signal(
                                &mut signals,
                                entry,
                                *giver,
                                Some(connection.actor),
                                if elimination_finesse {
                                    HGroupMoveKind::EliminationFinesse
                                } else if connection.kind == HGroupConnectionKind::Finesse
                                    && connection.cards.len() > 1
                                {
                                    HGroupMoveKind::LayeredFinesse
                                } else {
                                    match connection.kind {
                                        HGroupConnectionKind::Prompt => HGroupMoveKind::Prompt,
                                        HGroupConnectionKind::Finesse => HGroupMoveKind::Finesse,
                                    }
                                },
                                connection.cards.clone(),
                                Some(connection.expected),
                            );

                            let player_count = hands.len();
                            let target_distance =
                                (target.index() + player_count - giver.index()) % player_count;
                            let actor_distance = (connection.actor.index() + player_count
                                - giver.index())
                                % player_count;
                            if connection.kind == HGroupConnectionKind::Finesse
                                && actor_distance > target_distance
                            {
                                // The executable graph is also the source of
                                // the named Level-2 interpretation. Deriving
                                // Reverse Finesse independently from the
                                // focus identity failed whenever that identity
                                // was hidden from the recipient.
                                // Source: https://hanabi.github.io/level-2/#the-reverse-finesse
                                push_signal(
                                    &mut signals,
                                    entry,
                                    *giver,
                                    Some(connection.actor),
                                    HGroupMoveKind::ReverseFinesse,
                                    connection.cards.clone(),
                                    Some(connection.expected),
                                );
                            }

                            // The ordered graph is the executable mechanism,
                            // but preserve the exact Level-5 name as an audit
                            // signal. This keeps Hidden, Clandestine, Queued,
                            // and Ambiguous Finesses distinguishable without
                            // giving each one a second transition system.
                            // Sources:
                            // - https://hanabi.github.io/level-5/#the-hidden-finesse
                            // - https://hanabi.github.io/level-5/#the-clandestine-finesse
                            // - https://hanabi.github.io/level-5/#the-queued-finesse
                            // - https://hanabi.github.io/level-5/#the-ambiguous-finesse
                            if rule_enabled(profile, HGroupRuleId::SpecialFinesses)
                                && connection.kind == HGroupConnectionKind::Finesse
                            {
                                let was_queued = previous_connections
                                    .iter()
                                    .any(|prior| prior.actor == connection.actor);
                                let matching_finesse_positions = hands
                                    .iter()
                                    .enumerate()
                                    .filter(|(player, _)| *player != target.index())
                                    .filter_map(|(player, hand)| {
                                        finesse_position_id(hand, &previously_promptable, 0)
                                            .filter(|card| {
                                                historical.identity(*card)
                                                    == Some(connection.expected)
                                            })
                                            .map(|_| player)
                                    })
                                    .count();
                                let first_actual = connection
                                    .cards
                                    .first()
                                    .and_then(|card| historical.identity(*card));
                                let exact = if was_queued {
                                    Some(HGroupMoveKind::QueuedFinesse)
                                } else if matching_finesse_positions > 1 {
                                    Some(HGroupMoveKind::AmbiguousFinesse)
                                } else if connection.cards.len() > 1
                                    && first_actual.is_some_and(|identity| {
                                        bluff_play_connects(*clue, identity)
                                    })
                                {
                                    Some(HGroupMoveKind::ClandestineFinesse)
                                } else if connection.cards.len() > 1 {
                                    Some(HGroupMoveKind::LayeredFinesse)
                                } else if new_connections.iter().any(|other| {
                                    other.actor == connection.actor
                                        && other.kind == HGroupConnectionKind::Prompt
                                }) {
                                    Some(HGroupMoveKind::HiddenFinesse)
                                } else {
                                    None
                                };
                                if let Some(exact) = exact {
                                    push_signal(
                                        &mut signals,
                                        entry,
                                        *giver,
                                        Some(connection.actor),
                                        exact,
                                        connection.cards.clone(),
                                        Some(connection.expected),
                                    );
                                }
                            }
                        }
                        // A Play interpretation is an executable promise only
                        // when the focus can play now or the shared connection
                        // graph contains the Prompt/Finesse path that makes it
                        // playable later. Merely finding a delayed identity
                        // mask is not enough; treating it as a persistent play
                        // manufactured phantom obligations from otherwise
                        // unresolved clues.
                        if (!play_identities.is_empty() && direct_play == play_identities)
                            || !new_connections.is_empty()
                        {
                            if new_connections.is_empty() {
                                already_playing.insert_from(EffectSource::Event(entry.turn), focus);
                            } else {
                                for connection in &new_connections {
                                    already_playing.insert_from(
                                        EffectSource::Promise(connection.promise),
                                        focus,
                                    );
                                }
                            }
                        }
                    }
                    if is_required_fix {
                        required_fixes.retain(|obligation| {
                            let required = obligation.required;
                            !(required.actor == *giver
                                && required.target == *target
                                && touched.contains(&required.focus)
                                && clue.matches(required.identity))
                        });
                    }
                } else {
                    for card in touched {
                        facts[card.index()].add_positive_clue(*clue);
                    }
                    for card in untouched {
                        facts[card.index()].add_negative_clue(*clue);
                    }
                    explicitly_clued.extend(touched.iter().copied());
                }
                historical_clue_tokens = historical_clue_tokens.saturating_sub(1);
            }
            ObservedEvent::Played {
                player,
                card,
                identity,
                successful,
            } => {
                // A successful off-suit blind play can demonstrate that a
                // visible, later Finesse connector was only one branch of an
                // Ambiguous Layered Finesse. The player immediately after the
                // blind play must then test their own Finesse Position before
                // the visible connector acts. This refinement is necessarily
                // observer-relative: everyone else can see that hidden card
                // and scheduled it from the original clue, while the possible
                // blind player initially had to trust the later visible copy.
                //
                // Sources:
                // - https://hanabi.github.io/level-5/#the-layered-finesse
                // - https://hanabi.github.io/level-5/#the-ambiguous-finesse
                let demonstrated_layer = (*successful
                    && rule_enabled(profile, HGroupRuleId::SpecialFinesses)
                    && next_player(*player, hands.len()) == view.observer
                    && !was_clued_before(view, entry.turn, *card)
                    && !pending_connections.iter().any(|connection| {
                        connection.actor == *player
                            && connection.cards.first() == Some(card)
                            && pending_is_active(connection, &pending_connections)
                    }))
                .then(|| {
                    pending_connections
                        .iter()
                        .find(|connection| {
                            connection.kind == HGroupConnectionKind::Finesse
                                && connection.actor != *player
                                && connection.actor != view.observer
                                && connection.actor == next_player(view.observer, hands.len())
                                && connection.expected != *identity
                                && pending_is_active(connection, &pending_connections)
                        })
                        .cloned()
                })
                .flatten();
                if let Some(prior) = demonstrated_layer {
                    let active_invisible =
                        active_invisibly_clued(&invisibly_clued, &pending_connections);
                    let mut gotten =
                        protected_cards(&explicitly_clued, &active_invisible, &chop_moved);
                    gotten.extend(already_playing.iter().copied());
                    if let Some(next_card) =
                        finesse_position_id(&hands[view.observer.index()], &gotten, 0)
                    {
                        let promise = pending_connections.start(
                            entry.turn,
                            ConnectionObligation {
                                promise: PromiseId::UNASSIGNED,
                                actor: view.observer,
                                cards: vec![next_card],
                                expected: prior.expected,
                                focus_identity: prior.focus_identity,
                                kind: HGroupConnectionKind::Finesse,
                                focus: prior.focus,
                                step: prior.step,
                            },
                        );
                        if promise != PromiseId::UNASSIGNED {
                            invisibly_clued.insert_from(EffectSource::Promise(promise), next_card);
                        }
                    }
                }
                let advance = pending_connections.advance_play(
                    entry.turn,
                    *player,
                    *card,
                    *identity,
                    *successful,
                );
                let failed_connections = advance.failed_focuses;
                let released_candidates = advance.released_candidates;
                for focus in failed_connections {
                    already_playing.remove(&focus);
                    forced_playable.remove(&focus);
                    invalidated_focuses.insert(focus);
                }
                for released in released_candidates {
                    if !explicitly_clued.contains(&released)
                        && !pending_connections
                            .iter()
                            .any(|connection| connection.cards.contains(&released))
                    {
                        invisibly_clued.remove(&released);
                    }
                }
                remove_card(&mut hands[player.index()], *card);
                invisibly_clued.remove(card);
                already_playing.remove(card);
                if *successful {
                    stack_heights[identity.suit.index()] = identity.rank.number();
                    if identity.rank == Rank::Five {
                        historical_clue_tokens = historical_clue_tokens
                            .saturating_add(1)
                            .min(MAX_CLUE_TOKENS);
                    }
                    let satisfied_elsewhere = pending_connections
                        .iter()
                        .filter(|connection| connection.expected == *identity)
                        .flat_map(|connection| connection.cards.iter().copied())
                        .collect::<CardSet>();
                    let disproved_prompts = pending_connections
                        .iter()
                        .filter(|connection| {
                            connection.expected == *identity
                                && connection.kind == HGroupConnectionKind::Prompt
                        })
                        .flat_map(|connection| connection.cards.iter().copied())
                        .collect::<CardSet>();
                    if !disproved_prompts.is_empty() {
                        push_signal(
                            &mut signals,
                            entry,
                            *player,
                            None,
                            HGroupMoveKind::Retraction,
                            disproved_prompts.iter().copied().collect(),
                            Some(*identity),
                        );
                    }
                    pending_connections.cancel_where(
                        entry.turn,
                        ConnectionTransitionReason::IdentitySatisfiedElsewhere,
                        |connection| connection.expected == *identity,
                    );
                    for satisfied in satisfied_elsewhere {
                        already_playing.remove(&satisfied);
                        forced_playable.remove(&satisfied);
                        if !explicitly_clued.contains(&satisfied)
                            && !pending_connections
                                .iter()
                                .any(|connection| connection.cards.contains(&satisfied))
                        {
                            invisibly_clued.remove(&satisfied);
                        }
                    }
                } else {
                    public_removed[identity.index()] += 1;
                }
                must_clue.remove(player);
            }
            ObservedEvent::Discarded {
                player,
                card,
                identity,
            } => {
                let physically_clued = was_clued_before(view, entry.turn, *card);
                let needs_actor_projection = physically_clued
                    || (perspective_depth.models_other_players() && *player != view.observer);
                let subjective = needs_actor_projection
                    .then(|| {
                        subjective_action_context_before(
                            SubjectiveReplayRequest {
                                source: view,
                                profile,
                                observer: *player,
                                history: &view.history[..entry_index],
                                hands: &hands,
                                facts: &facts,
                                deck_size: historical_deck_size,
                            },
                            *card,
                        )
                    })
                    .flatten();
                if physically_clued {
                    actor_known_discard_identity =
                        subjective.and_then(|context| context.known_identity);
                }
                if action_is_settled && !discard_now.contains(card) {
                    record_declined_direct_plays(
                        *player,
                        Some(entry.turn),
                        &hands,
                        &clues,
                        &already_playing,
                        &pending_connections,
                        &mut DirectPlayDeclines {
                            cards: &mut declined_direct_plays,
                            turns: &mut declined_direct_play_turns,
                        },
                    );
                }
                // A discard declines every currently actionable blind-play
                // promise in the actor's hand. Keeping those one-turn
                // obligations alive made later clues appear unsafe because
                // the reducer still expected a Bluff/Finesse card that the
                // player had publicly declined several turns earlier.
                // Delayed connection steps are represented by the connection
                // graph rather than `forced_playable`, so clearing this set
                // does not erase a downstream obligation that is not due yet.
                for declined in &hands[player.index()] {
                    forced_playable.remove(declined);
                }
                let active_invisible =
                    active_invisibly_clued(&invisibly_clued, &pending_connections);
                let gotten = protected_cards(&explicitly_clued, &active_invisible, &chop_moved);
                actor_saw_normal_discard = chop(&hands[player.index()], &gotten) == Some(*card)
                    || (perspective_depth.models_other_players()
                        && *player != view.observer
                        && subjective.and_then(|context| context.chop) == Some(*card));
                if chop(&hands[player.index()], &gotten) == Some(*card) {
                    early_game = false;
                }
                pending_connections.discard(entry.turn, *player, *card);
                remove_card(&mut hands[player.index()], *card);
                invisibly_clued.remove(card);
                already_playing.remove(card);
                public_removed[identity.index()] += 1;
                must_clue.remove(player);
                historical_clue_tokens = historical_clue_tokens
                    .saturating_add(1)
                    .min(MAX_CLUE_TOKENS);
            }
            ObservedEvent::Drew { player, card, .. } => {
                hands[player.index()].push(*card);
                historical_deck_size = historical_deck_size.saturating_sub(1);
            }
        }

        let context = HGroupTurnContext {
            entry,
            historical,
            before,
            after: HGroupTurnView {
                hands: &hands,
                facts: &facts,
                stack_heights,
                clue_tokens: historical_clue_tokens,
                deck_size: historical_deck_size,
                early_game,
            },
            actor_before: ActorBeliefBefore {
                normal_chop_discard: actor_saw_normal_discard,
                discarded_identity: actor_known_discard_identity,
            },
        };
        debug_assert_eq!(context.after.clue_tokens, historical_clue_tokens);
        let mut effects = HGroupRuleEffects {
            explicitly_clued: &explicitly_clued,
            invisibly_clued: &mut invisibly_clued,
            clues: &clues,
            already_playing: &mut already_playing,
            pending: &mut pending_connections,
            chop_moved: &mut chop_moved,
            must_clue: &mut must_clue,
            forced_playable: &mut forced_playable,
            discard_now: &mut discard_now,
            implicit_saves: &mut implicit_saves,
            required_fixes: &mut required_fixes,
            signals: &mut signals,
        };
        let execution = RuleExecutionContext::new(&context, view, profile);
        let mut transition = apply_post_event_rules(&execution, &mut effects);
        if action_is_settled {
            if let Some(actor) = declined_with_clue {
                let recognized_deferral = clue_permits_direct_play_deferral(&signals, entry.turn);
                if !required_clue_deferral && !recognized_deferral {
                    // A direct Play interpretation is falsified when its owner
                    // voluntarily clues instead. Connections are excluded because
                    // their focus can wait for an intervening Prompt/Finesse. A
                    // recognized or required Fix is a convention-mandated
                    // deferral rather than evidence against the prior promise.
                    record_declined_direct_plays(
                        actor,
                        None,
                        &hands,
                        &clues,
                        &already_playing,
                        &pending_connections,
                        &mut DirectPlayDeclines {
                            cards: &mut declined_direct_plays,
                            turns: &mut declined_direct_play_turns,
                        },
                    );
                }
            }
        }
        reconcile_connection_fact_lifecycles(
            &pending_connections,
            event_connection_transition_start,
            &mut invisibly_clued,
            &mut already_playing,
            &mut forced_playable,
        );
        explicitly_clued.reconcile_mask(
            event_card_snapshot.explicitly_clued,
            EffectSource::Event(entry.turn),
        );
        invisibly_clued.reconcile_mask(
            event_card_snapshot.invisibly_clued,
            EffectSource::Event(entry.turn),
        );
        already_playing.reconcile_mask(
            event_card_snapshot.already_playing,
            EffectSource::Event(entry.turn),
        );
        chop_moved.reconcile_mask(
            event_card_snapshot.chop_moved,
            EffectSource::Event(entry.turn),
        );
        forced_playable.reconcile_mask(
            event_card_snapshot.forced_playable,
            EffectSource::Event(entry.turn),
        );
        let event_after = ConventionCardSetSnapshot::capture(
            &explicitly_clued,
            &invisibly_clued,
            &already_playing,
            &chop_moved,
            &forced_playable,
        );
        transition.delta = ConventionTransitionDelta {
            card_changes: event_card_snapshot.changes_to(&event_after),
            knowledge_changes: Vec::new(),
        };
        if !transition.proposals.is_empty() || !transition.delta.is_empty() {
            transitions.push(transition);
        }
    }
    if rule_enabled(profile, HGroupRuleId::SpecialDiscards) {
        for pending in pending_connections.iter().filter(|pending| {
            pending.actor == view.observer
                && pending.kind == HGroupConnectionKind::Finesse
                && pending_is_active(pending, &pending_connections)
        }) {
            let Some(blind) = pending.cards.first() else {
                continue;
            };
            let duplicated_in_own_hand = view.hands[view.observer.index()].iter().any(|card| {
                card.id != *blind
                    && explicitly_clued.contains(&card.id)
                    && card.clues.allows(pending.expected)
            });
            let bluff = signals
                .iter()
                .any(|signal| signal.kind == HGroupMoveKind::Bluff && signal.cards.contains(blind));
            if duplicated_in_own_hand && !bluff && !discard_now.contains(blind) {
                discard_now.push(*blind);
            }
        }
    }
    // A discard instead of a direct play is only a one-round hesitation: if
    // no teammate demonstrates a connection before the owner's next turn,
    // the direct interpretation becomes actionable again. A clue instead of
    // playing is an intentional deferral and therefore has no timestamp here;
    // it remains declined until a later clue retouches the card.
    declined_direct_plays.retain(|card| {
        declined_direct_play_turns
            .iter()
            .find_map(|(declined, turn)| (declined == card).then_some(*turn))
            .is_none_or(|turn| {
                view.turn
                    < turn.saturating_add(
                        u32::try_from(hands.len())
                            .expect("standard Hanabi has at most five players"),
                    )
            })
    });
    let (signals, convention_facts) = signals.into_parts();
    let mut state = HGroupState {
        hands,
        cards: ConventionCardState {
            explicitly_clued,
            invisibly_clued,
            already_playing,
            chop_moved,
            discard_now,
            forced_playable,
            invalidated_focuses,
            declined_direct_plays,
            facts: convention_facts,
        },
        clues,
        pending_connections,
        early_game,
        signals,
        must_clue,
        implicit_saves,
        required_fixes,
        transitions,
        knowledge: ConventionKnowledge::default(),
    };
    state.knowledge = build_convention_knowledge(deductions, &state);
    state
        .knowledge
        .attach_to_transitions(&mut state.transitions);
    debug_assert!(
        state.validate().is_ok(),
        "invalid H-Group replay state: {:?}",
        state.validate()
    );
    state
}

/// Whether a clue carries a recognized obligation that takes precedence over
/// an immediate play. Keeping this precedence in one typed transition avoids
/// each recognizer independently deciding whether an old promise survived.
fn clue_permits_direct_play_deferral(signals: &ConventionJournal, turn: u32) -> bool {
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
        let leads_self = next.is_some_and(|next| {
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
        let loaded = loaded_connection_plan(
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
    let loaded_plan = rule_enabled(profile, HGroupRuleId::Extras).then(|| {
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
        if pending_identity_is_queued(pending, expected) {
            let giver_is_deferring_this_connection = pending.iter().any(|connection| {
                connection.actor == giver
                    && connection.expected == expected
                    && pending_is_active(connection, pending)
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
                        hands[candidate_index]
                            .iter()
                            .rev()
                            .copied()
                            .find(|card| {
                                *card != focus
                                    && !promptable_before_clue.contains(card)
                                    && !invisibly_clued.contains(card)
                                    && !scheduled_cards.contains(card)
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
                    identity_of(view, *card) == Some(expected)
                        || (*candidate_index == view.observer.index()
                            && facts[card.index()].identity_mask() == 1 << expected.index())
                })
                || rule_enabled(profile, HGroupRuleId::SpecialFinesses)
                    && (ordinary_search_len..hands.len()).any(|distance| {
                        let candidate_index = (actor_index + distance) % hands.len();
                        if candidate_index == target.index() || candidate_index == giver.index() {
                            return false;
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
                                !gotten.contains(card)
                                    && !scheduled_cards.contains(card)
                                    && *card != focus
                            })
                            .collect::<Vec<_>>();
                        unclued
                            .iter()
                            .position(|card| {
                                identity_of(view, *card) == Some(expected)
                                    || (candidate_index == view.observer.index()
                                        && facts[card.index()].identity_mask()
                                            == 1 << expected.index())
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
                let cards = if rule_enabled(profile, HGroupRuleId::SpecialFinesses) {
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
                stack_heights,
                focus,
                focus_identity,
            ) {
                if elimination_identity == expected && !scheduled_cards.contains(&card) {
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
                && pending_is_active(connection, pending)
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

fn pending_is_active(candidate: &ConnectionObligation, pending: &[ConnectionObligation]) -> bool {
    !pending.iter().any(|other| {
        other.focus == candidate.focus && other.step < candidate.step && !other.cards.is_empty()
    })
}

fn pending_identity_is_queued(pending: &[ConnectionObligation], identity: Card) -> bool {
    pending
        .iter()
        .any(|connection| connection.expected == identity || connection.focus_identity == identity)
}

fn pending_card_allows_identity(
    pending: &[ConnectionObligation],
    convention_facts: &ConventionFacts,
    card: CardId,
    identity: Card,
    stack_heights: [u8; 5],
) -> bool {
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
    pending: &[ConnectionObligation],
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
    pending_identity_is_queued(&replay.pending_connections, identity)
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
