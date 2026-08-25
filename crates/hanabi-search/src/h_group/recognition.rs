//! Level-gated H-Group rule recognizers.
//!
//! Recognizers inspect one public-history event and emit semantic effects into
//! the shared reducer state. Replay orchestration and shared card semantics
//! remain in the parent module.

use super::{
    BluffTargetKind, Card, CardId, CardSet, Clue, ClueFacts, ConnectionManager,
    ConnectionObligation, ConnectionTransitionReason, ConventionJournal, HGroupClueInterpretation,
    HGroupClueKind, HGroupConnectionKind, HGroupMoveKind, HGroupRuleEffects, HGroupSaveKind,
    HGroupTurnContext, HGroupTurnSnapshot, IdentitySet, MAX_CLUE_TOKENS, ObservedEvent,
    ObservedHistoryEntry, PlayerId, PlayerView, PromiseId, Rank, RequiredFix, bluff_play_connects,
    bluff_target_kind_at, bluff_target_order_is_legal, card_is_trash, chop, finesse_position_id,
    five_pulled_card, focus, has_higher_basic_priority, identity_of, identity_set, is_critical,
    is_playable_at, is_playable_now, is_trash_at, next_player, pending_is_active, protected_cards,
    push_signal, was_clued_before, was_clued_before_with,
};

mod advanced;
mod advanced_bluffs;
mod basic;
mod bluffs;
mod chop_moves;
mod extras;
mod late_game;
mod order_chop;
mod special_discards;
mod tempo;
mod trash;
pub(super) use advanced::{
    apply_elimination_effects, apply_elimination_resolution_effects, apply_five_tech_effects,
    apply_out_of_order_effects,
};
pub(super) use advanced_bluffs::{
    apply_context_effects, apply_double_bluff_effects, apply_duplication_effects,
    apply_intermediate_bluff_effects,
};
pub(super) use basic::{apply_level_three_effects, apply_level_two_effects};
pub(super) use bluffs::{apply_bluff_effects, apply_resolved_bluff_effects};
pub(super) use chop_moves::apply_chop_move_effects;
pub(super) use extras::{apply_extra_effects, apply_max_special_effects};
pub(super) use late_game::{
    apply_charm_effects, apply_ignition_effects, apply_phantom_effects, apply_priority_effects,
    apply_unnecessary_move_effects,
};
pub(super) use order_chop::apply_order_chop_move_effects;
pub(super) use special_discards::{apply_special_finesse_discard_effects, apply_transfer_effects};
pub(super) use tempo::{
    apply_emergency_discard_effects, apply_positional_effects, apply_stall_effects,
    apply_tempo_effects,
};
pub(super) use trash::{
    apply_ejection_discharge_effects, apply_trash_connection_refinements, apply_trash_effects,
};

fn same_turn_signal(signals: &ConventionJournal, turn: u32, kind: HGroupMoveKind) -> bool {
    signals.has_at_turn(turn, kind)
}
