//! Executable H-Group rule registry.
//!
//! Real history and prospective transitions both enter convention inference
//! through `replay_h_group_inner`, so keeping post-event dispatch here gives
//! both paths the same ordered rule execution rather than parallel `if`
//! ladders.

use hanabi_core::PlayerView;

use super::recognition::{
    apply_bluff_effects, apply_charm_effects, apply_chop_move_effects, apply_context_effects,
    apply_double_bluff_effects, apply_duplication_effects, apply_ejection_discharge_effects,
    apply_elimination_effects, apply_elimination_resolution_effects,
    apply_emergency_discard_effects, apply_extra_effects, apply_five_tech_effects,
    apply_ignition_effects, apply_intermediate_bluff_effects, apply_level_three_effects,
    apply_level_two_effects, apply_max_special_effects, apply_order_chop_move_effects,
    apply_out_of_order_effects, apply_phantom_effects, apply_positional_effects,
    apply_priority_effects, apply_special_finesse_discard_effects, apply_stall_effects,
    apply_tempo_effects, apply_transfer_effects, apply_trash_connection_refinements,
    apply_trash_effects, apply_unnecessary_move_effects,
};
use super::rules::POST_EVENT_RULES;
use super::transition::transition_recording_enabled;
use super::{
    ConventionTransitionResult, HGroupPhase, HGroupProfile, HGroupRuleEffects, HGroupRuleId,
    HGroupTurnContext, MutationDomain, MutationSet, RuleProposal, h_group_phase_at, rule_enabled,
};

/// Semantic execution order. It is intentionally not numerical level order:
/// refinements such as Elimination and Phantom Playable must run before the
/// lower-level rule whose provisional meaning they replace.
pub(super) fn apply_post_event_rules(
    context: &HGroupTurnContext<'_>,
    view: &PlayerView,
    profile: HGroupProfile,
    effects: &mut HGroupRuleEffects<'_>,
) -> ConventionTransitionResult {
    debug_assert!(registry_is_valid());
    if !transition_recording_enabled() {
        for spec in POST_EVENT_RULES {
            if rule_enabled(profile, spec.id) {
                apply_rule(spec.id, context, view, profile, effects);
            }
        }
        return ConventionTransitionResult {
            turn: context.entry.turn,
            proposals: Vec::new(),
        };
    }
    let mut proposals = Vec::new();
    for spec in POST_EVENT_RULES {
        if rule_enabled(profile, spec.id) {
            let before = RuleStateFingerprint::capture(effects);
            let signal_start = effects.signals.len();
            let promise_start = effects.pending.transitions().len();
            apply_rule(spec.id, context, view, profile, effects);
            let after = RuleStateFingerprint::capture(effects);
            let signal_end = effects.signals.len();
            let promise_end = effects.pending.transitions().len();
            let mut mutations = before.changed_domains(&after);
            if signal_start != signal_end {
                mutations.insert(MutationDomain::CurrentFacts);
            }
            let proposal = RuleProposal {
                rule: spec.id,
                phase: spec.phase,
                signal_range: signal_start..signal_end,
                promise_transition_range: promise_start..promise_end,
                mutations,
            };
            if !proposal.is_empty() {
                proposals.push(proposal);
            }
        }
    }
    ConventionTransitionResult {
        turn: context.entry.turn,
        proposals,
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct RuleStateFingerprint {
    invisible_clues: usize,
    playing_promises: usize,
    connections: (usize, usize),
    chop_movement: usize,
    must_clue: usize,
    forced_plays: usize,
    required_discards: usize,
    implicit_saves: usize,
    required_fix: Option<(usize, usize, usize, usize)>,
}

impl RuleStateFingerprint {
    fn capture(effects: &HGroupRuleEffects<'_>) -> Self {
        Self {
            invisible_clues: effects.invisibly_clued.len(),
            playing_promises: effects.already_playing.len(),
            connections: (effects.pending.len(), effects.pending.transitions().len()),
            chop_movement: effects.chop_moved.len(),
            must_clue: effects.must_clue.len(),
            forced_plays: effects.forced_playable.len(),
            required_discards: effects.discard_now.len(),
            implicit_saves: effects.implicit_saves.len(),
            required_fix: effects.required_fix.map(|fix| {
                (
                    fix.actor.index(),
                    fix.target.index(),
                    fix.focus.index(),
                    fix.identity.index(),
                )
            }),
        }
    }

    fn changed_domains(self, after: &Self) -> MutationSet {
        let checks = [
            (
                self.invisible_clues != after.invisible_clues,
                MutationDomain::InvisibleClues,
            ),
            (
                self.playing_promises != after.playing_promises,
                MutationDomain::PlayingPromises,
            ),
            (
                self.connections != after.connections,
                MutationDomain::Connections,
            ),
            (
                self.chop_movement != after.chop_movement,
                MutationDomain::ChopMovement,
            ),
            (self.must_clue != after.must_clue, MutationDomain::MustClue),
            (
                self.forced_plays != after.forced_plays,
                MutationDomain::ForcedPlays,
            ),
            (
                self.required_discards != after.required_discards,
                MutationDomain::RequiredDiscards,
            ),
            (
                self.implicit_saves != after.implicit_saves,
                MutationDomain::ImplicitSaves,
            ),
            (
                self.required_fix != after.required_fix,
                MutationDomain::RequiredFix,
            ),
        ];
        let mut mutations = MutationSet::default();
        for (changed, domain) in checks {
            if changed {
                mutations.insert(domain);
            }
        }
        mutations
    }
}

fn registry_is_valid() -> bool {
    POST_EVENT_RULES.iter().enumerate().all(|(index, spec)| {
        index
            .checked_sub(1)
            .is_none_or(|prior| POST_EVENT_RULES[prior].phase <= spec.phase)
            && spec.depends_on.iter().all(|dependency| {
                POST_EVENT_RULES[..index]
                    .iter()
                    .any(|candidate| candidate.id == *dependency)
            })
    })
}

#[allow(clippy::too_many_lines)]
fn apply_rule(
    rule: HGroupRuleId,
    context: &HGroupTurnContext<'_>,
    view: &PlayerView,
    profile: HGroupProfile,
    effects: &mut HGroupRuleEffects<'_>,
) {
    match rule {
        HGroupRuleId::Priority => {
            if h_group_phase_at(
                view.hands.len(),
                context.before.early_game,
                context.before.deck_size,
                context.before.stack_heights,
            ) != HGroupPhase::EndGame
            {
                apply_priority_effects(
                    context,
                    view,
                    effects.explicitly_clued,
                    effects.forced_playable,
                    effects.signals,
                );
            }
        }
        HGroupRuleId::BasicMoves => apply_level_two_effects(
            context.entry,
            view,
            context.after.hands,
            effects.explicitly_clued,
            effects.signals,
        ),
        HGroupRuleId::BasicStrategy => apply_level_three_effects(context, view, effects),
        HGroupRuleId::Elimination => {
            apply_elimination_effects(
                context.entry,
                view,
                context.after.hands,
                context.after.stack_heights,
                effects.signals,
            );
            apply_elimination_resolution_effects(context, effects);
        }
        HGroupRuleId::ChopMoves => {
            if !rule_enabled(profile, HGroupRuleId::EndGame)
                || h_group_phase_at(
                    view.hands.len(),
                    context.after.early_game,
                    context.after.deck_size,
                    context.after.stack_heights,
                ) != HGroupPhase::EndGame
            {
                apply_order_chop_move_effects(
                    context,
                    view,
                    effects,
                    rule_enabled(profile, HGroupRuleId::Extras),
                );
                apply_chop_move_effects(context, view, effects);
            }
        }
        HGroupRuleId::TempoClues => apply_tempo_effects(
            context.entry,
            view,
            context.after.hands,
            effects.explicitly_clued,
            effects.chop_moved,
            effects.signals,
        ),
        HGroupRuleId::EmergencyDiscards => apply_emergency_discard_effects(
            context,
            view,
            effects,
            rule_enabled(profile, HGroupRuleId::TrashMoves),
        ),
        HGroupRuleId::PhantomPlayable => apply_phantom_effects(context, view, effects),
        HGroupRuleId::EndGame => apply_positional_effects(context, view, effects),
        HGroupRuleId::Stalling => apply_stall_effects(context, view, effects),
        HGroupRuleId::SpecialDiscards => {
            apply_special_finesse_discard_effects(context, view, effects);
            apply_transfer_effects(
                context.entry,
                view,
                context.after.hands,
                effects.explicitly_clued,
                effects.invisibly_clued,
                effects.already_playing,
                effects.pending,
                effects.signals,
            );
        }
        HGroupRuleId::Bluffs => apply_bluff_effects(
            context.entry,
            view,
            context.after.hands,
            context.after.stack_heights,
            effects.explicitly_clued,
            effects.pending,
            effects.forced_playable,
            effects.signals,
        ),
        HGroupRuleId::Context => apply_context_effects(context, view, effects),
        HGroupRuleId::IntermediateBluffs => {
            apply_intermediate_bluff_effects(context, view, effects);
        }
        HGroupRuleId::TrashMoves => {
            apply_trash_effects(
                context.entry,
                view,
                context.after.hands,
                context.after.stack_heights,
                effects.chop_moved,
                effects.pending,
                effects.signals,
            );
            apply_trash_connection_refinements(context, effects);
        }
        HGroupRuleId::DoubleBluffs => apply_double_bluff_effects(context, view, effects),
        HGroupRuleId::EjectionsAndDischarges => apply_ejection_discharge_effects(
            context.entry,
            view,
            context.after.hands,
            context.after.facts,
            effects.clues,
            context.after.stack_heights,
            effects.explicitly_clued,
            effects.invisibly_clued,
            effects.chop_moved,
            effects.pending,
            effects.forced_playable,
            effects.discard_now,
            effects.signals,
            rule_enabled(profile, HGroupRuleId::Extras),
        ),
        HGroupRuleId::Duplication => apply_duplication_effects(context, view, effects),
        HGroupRuleId::FiveTech => apply_five_tech_effects(
            context.entry,
            view,
            context.after.hands,
            effects.clues,
            context.after.stack_heights,
            effects.explicitly_clued,
            effects.invisibly_clued,
            effects.chop_moved,
            effects.pending,
            effects.forced_playable,
            effects.implicit_saves,
            effects.signals,
        ),
        HGroupRuleId::OutOfOrderPlay => apply_out_of_order_effects(
            context.entry,
            view,
            context.after.hands,
            effects.clues,
            context.after.stack_heights,
            effects.pending,
            effects.forced_playable,
            effects.required_fix,
            effects.signals,
        ),
        HGroupRuleId::Ignition => apply_ignition_effects(context, view, effects),
        HGroupRuleId::Charms => apply_charm_effects(
            context.entry,
            view,
            context.after.hands,
            effects.clues,
            context.after.stack_heights,
            effects.explicitly_clued,
            effects.invisibly_clued,
            effects.chop_moved,
            effects.pending,
            effects.forced_playable,
            effects.signals,
        ),
        HGroupRuleId::UnnecessaryMoves => apply_unnecessary_move_effects(context, view, effects),
        HGroupRuleId::Extras => {
            apply_extra_effects(context, view, effects);
            apply_max_special_effects(context, view, effects);
        }
        HGroupRuleId::Basic | HGroupRuleId::SpecialFinesses => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_post_event_rule_is_dispatched_once() {
        let mut unique = POST_EVENT_RULES
            .iter()
            .map(|spec| spec.id)
            .collect::<Vec<_>>();
        unique.sort_unstable_by_key(|rule| *rule as u8);
        unique.dedup();
        assert_eq!(unique.len(), POST_EVENT_RULES.len());
        assert!(!unique.contains(&HGroupRuleId::Basic));
        assert!(!unique.contains(&HGroupRuleId::SpecialFinesses));
    }
}
