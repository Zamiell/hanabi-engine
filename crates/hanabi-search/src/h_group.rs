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

use crate::{HGroupLevel, HGroupProfile, IdentitySet, LogicalDeductions};

mod action_analysis;
mod decision;
mod model;
mod perspective;
mod prospective;
mod rules;
mod turn_context;

use action_analysis::{HGroupActionKind, HGroupActionSet, HGroupAnalyzedAction};
pub(crate) use decision::analyze_h_group_convention;
pub use decision::infer_h_group;
#[cfg(test)]
use decision::{
    analysis_clue_candidates, build_h_group_analysis, h_group_predictable_action,
    ordered_h_group_actions, select_h_group_action,
};
use decision::{
    infer_h_group_from_replay, ordered_playable_cards, positional_discard_candidate,
    positional_discard_is_valid_snapshot,
};
use model::{CardSet, ConnectionObligation, HGroupState, PlayerSet, RequiredFix, protected_cards};
pub use model::{
    HGroupCardInference, HGroupClueInterpretation, HGroupClueKind, HGroupConnection,
    HGroupConnectionKind, HGroupConnectionPromise, HGroupInferences, HGroupPhase, HGroupSaveKind,
    HGroupSignal,
};
use perspective::{PerspectiveProjector, ProspectiveTransition};
#[cfg(test)]
use prospective::prospective_clue_hazard;
use prospective::{
    projected_h_group_replay, prospective_clue_has_unsafe_connection,
    prospective_clue_marks_focus_saved, prospective_clue_view,
    prospective_play_has_unsafe_inference, prospective_play_view, subjective_chop_before_action,
    subjective_convention_cards, with_prospective_analysis_cache,
};
use rules::{HGroupRuleId, rule_enabled};
use turn_context::{HGroupTurnContext, HGroupTurnSnapshot, HGroupTurnView};

/// Semantic families used by the cumulative H-Group interpreter.
///
/// The documentation gives many combinations their own names. The engine
/// represents combinations as a sequence of these primitive effects instead
/// of duplicating state-transition code for every name. For example, a Trash
/// Push Finesse is represented by `TrashPush` followed by `Finesse`.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
#[non_exhaustive]
pub enum HGroupMoveKind {
    PlayClue,
    SaveClue,
    FiveStall,
    Prompt,
    Finesse,
    ReverseFinesse,
    SelfFinesse,
    LayeredFinesse,
    FixClue,
    SarcasticDiscard,
    ChopMove,
    TempoClue,
    EmergencyDiscard,
    PositionalDiscard,
    Stall,
    TransferDiscard,
    Bluff,
    DoubleBluff,
    SelfishClue,
    Context,
    TrashPush,
    Ejection,
    Discharge,
    Duplication,
    Elimination,
    FivePull,
    OccupiedPlay,
    Ignition,
    PhantomPlayable,
    Charm,
    UnnecessaryMove,
    Priority,
    Extra,
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
        effects: &[HGroupMoveKind::ChopMove],
    },
    HGroupLevelDescriptor {
        profile: HGroupProfile::Level(HGroupLevel::Level5),
        title: "Special finesses",
        effects: &[HGroupMoveKind::LayeredFinesse],
    },
    HGroupLevelDescriptor {
        profile: HGroupProfile::Level(HGroupLevel::Level6),
        title: "Tempo clues",
        effects: &[HGroupMoveKind::TempoClue],
    },
    HGroupLevelDescriptor {
        profile: HGroupProfile::Level(HGroupLevel::Level7),
        title: "Emergency discards",
        effects: &[HGroupMoveKind::EmergencyDiscard],
    },
    HGroupLevelDescriptor {
        profile: HGroupProfile::Level(HGroupLevel::Level8),
        title: "End-game",
        effects: &[HGroupMoveKind::PositionalDiscard],
    },
    HGroupLevelDescriptor {
        profile: HGroupProfile::Level(HGroupLevel::Level9),
        title: "Stalling",
        effects: &[HGroupMoveKind::Stall],
    },
    HGroupLevelDescriptor {
        profile: HGroupProfile::Level(HGroupLevel::Level10),
        title: "Special discards",
        effects: &[HGroupMoveKind::TransferDiscard],
    },
    HGroupLevelDescriptor {
        profile: HGroupProfile::Level(HGroupLevel::Level11),
        title: "Bluffs",
        effects: &[HGroupMoveKind::Bluff],
    },
    HGroupLevelDescriptor {
        profile: HGroupProfile::Level(HGroupLevel::Level12),
        title: "Context",
        effects: &[HGroupMoveKind::SelfishClue, HGroupMoveKind::Context],
    },
    HGroupLevelDescriptor {
        profile: HGroupProfile::Level(HGroupLevel::Level13),
        title: "Intermediate bluffs",
        effects: &[HGroupMoveKind::Bluff],
    },
    HGroupLevelDescriptor {
        profile: HGroupProfile::Level(HGroupLevel::Level14),
        title: "Trash moves",
        effects: &[HGroupMoveKind::TrashPush],
    },
    HGroupLevelDescriptor {
        profile: HGroupProfile::Level(HGroupLevel::Level15),
        title: "Double bluffs",
        effects: &[HGroupMoveKind::DoubleBluff],
    },
    HGroupLevelDescriptor {
        profile: HGroupProfile::Level(HGroupLevel::Level16),
        title: "Ejections and discharges",
        effects: &[HGroupMoveKind::Ejection, HGroupMoveKind::Discharge],
    },
    HGroupLevelDescriptor {
        profile: HGroupProfile::Level(HGroupLevel::Level17),
        title: "Duplication",
        effects: &[HGroupMoveKind::Duplication],
    },
    HGroupLevelDescriptor {
        profile: HGroupProfile::Level(HGroupLevel::Level18),
        title: "Elimination",
        effects: &[HGroupMoveKind::Elimination],
    },
    HGroupLevelDescriptor {
        profile: HGroupProfile::Level(HGroupLevel::Level19),
        title: "5 tech",
        effects: &[HGroupMoveKind::FivePull],
    },
    HGroupLevelDescriptor {
        profile: HGroupProfile::Level(HGroupLevel::Level20),
        title: "Out-of-order play",
        effects: &[HGroupMoveKind::OccupiedPlay],
    },
    HGroupLevelDescriptor {
        profile: HGroupProfile::Level(HGroupLevel::Level21),
        title: "Ignition",
        effects: &[HGroupMoveKind::Ignition],
    },
    HGroupLevelDescriptor {
        profile: HGroupProfile::Level(HGroupLevel::Level22),
        title: "Phantom playable cards",
        effects: &[HGroupMoveKind::PhantomPlayable],
    },
    HGroupLevelDescriptor {
        profile: HGroupProfile::Level(HGroupLevel::Level23),
        title: "Charms",
        effects: &[HGroupMoveKind::Charm],
    },
    HGroupLevelDescriptor {
        profile: HGroupProfile::Level(HGroupLevel::Level24),
        title: "Unnecessary moves",
        effects: &[HGroupMoveKind::UnnecessaryMove],
    },
    HGroupLevelDescriptor {
        profile: HGroupProfile::Level(HGroupLevel::Level25),
        title: "Priority",
        effects: &[HGroupMoveKind::Priority],
    },
    HGroupLevelDescriptor {
        profile: HGroupProfile::Max,
        title: "Max",
        effects: &[HGroupMoveKind::Extra],
    },
];

/// Selects a conservative, unambiguous Level 1 clue.
///
/// Candidate clues must satisfy focus and Minimum Clue Value. Play clues also
/// satisfy Good Touch and either play now or create exactly one valid Prompt
/// or Finesse connection. Save clues are restricted to the Level 1 5, 2, and
/// critical-card forms.
#[derive(Clone, Copy, Debug)]
struct ClueCandidate {
    action: Action,
    score: u16,
    target: PlayerId,
    save: bool,
    immediate_play: bool,
}

#[cfg(test)]
#[allow(clippy::too_many_lines)]
fn h_group_clue_candidates(
    deductions: &LogicalDeductions,
    profile: HGroupProfile,
) -> Vec<ClueCandidate> {
    let analysis = build_h_group_analysis(deductions, profile);
    analysis_clue_candidates(deductions, profile, &analysis).to_vec()
}

#[allow(clippy::too_many_lines)]
fn h_group_clue_candidates_from_replay(
    deductions: &LogicalDeductions,
    profile: HGroupProfile,
    replay: &HGroupState,
) -> Vec<ClueCandidate> {
    with_prospective_analysis_cache(deductions.view(), profile, || {
        h_group_clue_candidates_from_replay_inner(deductions, profile, replay)
    })
}

#[allow(clippy::too_many_lines)]
fn h_group_clue_candidates_from_replay_inner(
    deductions: &LogicalDeductions,
    profile: HGroupProfile,
    replay: &HGroupState,
) -> Vec<ClueCandidate> {
    let view = deductions.view();
    if view.clue_tokens == 0 {
        return Vec::new();
    }
    if let Some(required) = replay
        .required_fix
        .filter(|required| required.actor == view.observer)
    {
        let target_hand = &view.hands[required.target.index()];
        let required_focus_card = target_hand.iter().find(|card| card.id == required.focus);
        let required_candidates = view
            .legal_actions()
            .into_iter()
            .filter_map(|action| {
                let Action::Clue { target, clue } = action else {
                    return None;
                };
                let touched = target_hand
                    .iter()
                    .filter(|card| card.identity.is_some_and(|identity| clue.matches(identity)))
                    .map(|card| card.id)
                    .collect::<Vec<_>>();
                (target == required.target
                    && clue.matches(required.identity)
                    && touched.contains(&required.focus)
                    && required_focus_card.is_some_and(|card| !card.clues.has_positive_clue(clue))
                    && !prospective_clue_has_unsafe_connection(
                        view,
                        profile,
                        target,
                        required.focus,
                        clue,
                        &touched,
                        false,
                    ))
                .then_some(ClueCandidate {
                    action,
                    score: 600,
                    target,
                    save: false,
                    immediate_play: false,
                })
            })
            .collect::<Vec<_>>();
        if !required_candidates.is_empty() {
            return required_candidates;
        }
    }
    let promptable = replay.promptable();
    let fixed_cards = currently_fixed_cards(&replay.signals);
    let gotten = replay.gotten_from(&promptable);
    let next_player = PlayerId::new(
        u8::try_from((view.current_player.index() + 1) % view.hands.len())
            .expect("standard Hanabi has at most five players"),
    );
    let convention_cards = convention_card_inferences(deductions, replay);
    let mut candidates = Vec::new();

    for action in view.legal_actions() {
        let Action::Clue { target, clue } = action else {
            continue;
        };
        let hand = &view.hands[target.index()];
        let layout = &replay.hands[target.index()];
        let touched = hand
            .iter()
            .filter(|card| card.identity.is_some_and(|identity| clue.matches(identity)))
            .map(|card| card.id)
            .collect::<Vec<_>>();
        let newly_touched = touched
            .iter()
            .copied()
            .filter(|card| !gotten.contains(card))
            .collect::<Vec<_>>();
        let newly_informed = touched
            .iter()
            .copied()
            .filter(|card| !promptable.contains(card))
            .collect::<Vec<_>>();
        // Minimum Clue Value: Level 1 does not spend tempo clues.
        if newly_touched.is_empty() {
            continue;
        }
        let old_chop = chop(layout, &gotten);
        let Some(focus) = focus(layout, &touched, old_chop, &gotten) else {
            continue;
        };
        let focus_identity = hand
            .iter()
            .find(|card| card.id == focus)
            .and_then(|card| card.identity)
            .expect("another player's cards are visible");
        let endangered_discard = positional_discard_candidate(deductions, target, &gotten);
        let save_score = if old_chop == Some(focus) || endangered_discard == Some(focus) {
            save_clue_score(
                view,
                hand,
                focus,
                focus_identity,
                clue,
                target,
                next_player,
                &replay.hands,
                &gotten,
            )
        } else {
            None
        };
        let play_score = play_clue_score(
            view,
            target,
            focus,
            focus_identity,
            clue,
            &newly_informed,
            &promptable,
            &fixed_cards,
            &replay.already_playing,
            &convention_cards,
        )
        .or_else(|| {
            let identities = newly_informed
                .iter()
                .filter_map(|card| current_card_identity(view, *card))
                .collect::<Vec<_>>();
            let distinct = identity_set(identities.iter().copied());
            (clue == Clue::Rank(Rank::One)
                && target == next_player
                && is_playable_now(view, focus_identity)
                && identities.len() == newly_informed.len()
                && identities.iter().all(|identity| identity.rank == Rank::One)
                && distinct.len() >= 2
                && distinct.len() < identities.len())
            .then(|| 410 + u16::try_from(distinct.len()).unwrap_or(0))
        });
        if play_score.is_some()
            && prospective_clue_has_unsafe_connection(
                view, profile, target, focus, clue, &touched, true,
            )
        {
            continue;
        }
        if let Some(mut score) = play_score {
            if old_chop == Some(focus)
                && (focus_identity.rank != Rank::One || target == next_player)
                && is_unique_visible(view, focus, focus_identity)
            {
                score += 120;
            }
            if is_playable_now(view, focus_identity)
                && focus_identity.rank != Rank::Five
                && gotten.iter().copied().any(|card| {
                    card != focus
                        && replay
                            .clues
                            .iter()
                            .rev()
                            .any(|prior| prior.focus == card && !prior.save_identities.is_empty())
                        && current_card_identity(view, card)
                            == Some(Card::new(
                                focus_identity.suit,
                                Rank::ALL[focus_identity.rank.index() + 1],
                            ))
                })
            {
                // Playing a connector that unlocks an already saved card
                // advances two promises. This is the clue analogue of the
                // Level-25 "leads another play" priority rule.
                score += 85;
            }
            candidates.push(ClueCandidate {
                action,
                score,
                target,
                save: false,
                immediate_play: is_playable_now(view, focus_identity),
            });
        } else if let Some(score) = save_score {
            if !prospective_clue_has_unsafe_connection(
                view, profile, target, focus, clue, &touched, false,
            ) && prospective_clue_marks_focus_saved(view, profile, target, focus, clue, &touched)
            {
                candidates.push(ClueCandidate {
                    action,
                    score,
                    target,
                    save: true,
                    immediate_play: false,
                });
            }
        }
    }

    let endangered_targets = candidates
        .iter()
        .filter(|candidate| candidate.save)
        .map(|candidate| candidate.target)
        .collect::<PlayerSet>();
    for candidate in &mut candidates {
        if endangered_targets.contains(&candidate.target) {
            if candidate.target == next_player {
                candidate.score = if candidate.immediate_play { 550 } else { 540 };
            } else if candidate.immediate_play {
                candidate.score = 500;
            }
        }
    }

    if rule_enabled(profile, HGroupRuleId::BasicMoves) {
        for candidate in advanced_clue_candidates(view, replay, &gotten, &convention_cards, profile)
        {
            if !candidates
                .iter()
                .any(|existing| existing.action == candidate.action)
            {
                candidates.push(candidate);
            }
        }
    }
    if rule_enabled(profile, HGroupRuleId::FiveTech)
        && view.play_stacks.iter().map(Vec::len).sum::<usize>() < 5
    {
        candidates.retain(|candidate| {
            !matches!(
                candidate.action,
                Action::Clue {
                    clue: Clue::Rank(Rank::Five),
                    ..
                }
            ) || candidate.save
                || !candidate.immediate_play
        });
    }

    let observer_chop = chop(&replay.hands[view.observer.index()], &gotten);
    if candidates.is_empty()
        && (observer_chop.is_none()
            || view.clue_tokens == MAX_CLUE_TOKENS
            || has_out_of_order_prompt(view, &gotten))
    {
        candidates.extend(tempo_clue_candidates(view, replay, &gotten));
    }
    if rule_enabled(profile, HGroupRuleId::Stalling) && view.clue_tokens == 1 {
        // Every clue source, including the fallback Tempo path above, must
        // respect the promise made by deliberately leaving the next player
        // locked at zero clues.
        candidates.retain(|candidate| {
            !creates_false_anxiety(view, profile, &gotten, candidate)
                && !creates_false_anxiety_after_forced_play(view, profile, candidate)
        });
    }
    candidates
}

fn creates_false_anxiety_after_forced_play(
    view: &PlayerView,
    profile: HGroupProfile,
    candidate: &ClueCandidate,
) -> bool {
    let Action::Clue { target, clue } = candidate.action else {
        return false;
    };
    let next = next_player(view.current_player, view.hands.len());
    let touched = view.hands[target.index()]
        .iter()
        .filter(|card| card.identity.is_some_and(|identity| clue.matches(identity)))
        .map(|card| card.id)
        .collect::<Vec<_>>();
    let after_clue = prospective_clue_view(view, target, clue, &touched);
    let Some((next_deductions, next_replay)) = projected_h_group_replay(&after_clue, profile, next)
    else {
        return true;
    };
    let next_inferred = infer_h_group_from_replay(&next_deductions, next_replay, profile);
    let forced = next_inferred
        .connection
        .map(|connection| connection.card)
        .or_else(|| {
            ordered_playable_cards(next_deductions.view(), &next_inferred, profile)
                .first()
                .copied()
        });
    let Some((forced, forced_identity)) = forced
        .and_then(|card| identity_of(view, card).map(|identity| (card, identity)))
        .filter(|(_, identity)| is_playable_now(view, *identity))
    else {
        return false;
    };
    let after_play = prospective_play_view(&after_clue, next, forced, forced_identity);
    let following = next_player(next, view.hands.len());
    let Some((deductions, replay)) = projected_h_group_replay(&after_play, profile, following)
    else {
        return true;
    };
    let inferred = infer_h_group_from_replay(&deductions, replay, profile);
    let selected = inferred
        .connection
        .map(|connection| connection.card)
        .or_else(|| {
            ordered_playable_cards(deductions.view(), &inferred, profile)
                .first()
                .copied()
        });
    let Some(selected) = selected else {
        return false;
    };
    identity_of(view, selected).is_some_and(|identity| {
        !is_playable_at(
            std::array::from_fn(|suit| {
                u8::try_from(after_play.play_stacks[suit].len())
                    .expect("a Hanabi stack has at most five cards")
            }),
            identity,
        )
    })
}

#[allow(clippy::too_many_lines)]
fn creates_false_anxiety(
    view: &PlayerView,
    profile: HGroupProfile,
    gotten: &CardSet,
    candidate: &ClueCandidate,
) -> bool {
    let Action::Clue { target, clue } = candidate.action else {
        return false;
    };
    let actor = next_player(view.current_player, view.hands.len());
    let mut gotten_after = gotten.clone();
    if target == actor {
        gotten_after.extend(
            view.hands[actor.index()]
                .iter()
                .filter(|card| card.identity.is_some_and(|identity| clue.matches(identity)))
                .map(|card| card.id),
        );
    }
    let hand = &view.hands[actor.index()];
    if hand.is_empty() || hand.iter().any(|card| !gotten_after.contains(&card.id)) {
        return false;
    }
    if !hand.iter().any(|card| {
        card.identity
            .is_some_and(|identity| is_playable_now(view, identity))
    }) {
        // Deliberately leaving a locked player at zero clues promises that an
        // Anxiety Play exists. The giver sees that player's hand and must not
        // make the promise when every card would misplay.
        return true;
    }

    let touched = if target == actor {
        hand.iter()
            .filter(|card| card.identity.is_some_and(|identity| clue.matches(identity)))
            .map(|card| card.id)
            .collect::<Vec<_>>()
    } else {
        Vec::new()
    };
    let after_clue = prospective_clue_view(view, target, clue, &touched);
    let Some((deductions, replay)) = projected_h_group_replay(&after_clue, profile, actor) else {
        return true;
    };
    let inferred = infer_h_group_from_replay(&deductions, replay, profile);
    let selected = inferred
        .connection
        .map(|connection| connection.card)
        .or_else(|| {
            ordered_playable_cards(deductions.view(), &inferred, profile)
                .first()
                .copied()
        });
    selected.is_none_or(|card| {
        identity_of(view, card).is_none_or(|identity| !is_playable_now(view, identity))
    })
}

#[allow(clippy::too_many_lines)]
fn advanced_clue_candidates(
    view: &PlayerView,
    replay: &HGroupState,
    gotten: &CardSet,
    convention_cards: &[HGroupCardInference],
    profile: HGroupProfile,
) -> Vec<ClueCandidate> {
    if view.clue_tokens == 0 {
        return Vec::new();
    }
    let actor_locked = replay.hands[view.observer.index()]
        .iter()
        .all(|card| gotten.contains(card) || replay.chop_moved.contains(card));
    let stalling = replay.early_game || actor_locked || view.clue_tokens == MAX_CLUE_TOKENS;
    let promptable = replay.promptable();
    let previously_fixed = currently_fixed_cards(&replay.signals);
    let mut candidates = Vec::new();
    for action in view.legal_actions() {
        let Action::Clue { target, clue } = action else {
            continue;
        };
        let hand = &view.hands[target.index()];
        let layout = &replay.hands[target.index()];
        let touched = hand
            .iter()
            .filter(|card| card.identity.is_some_and(|identity| clue.matches(identity)))
            .map(|card| card.id)
            .collect::<Vec<_>>();
        if touched.is_empty() {
            continue;
        }
        let newly_touched = touched
            .iter()
            .copied()
            .filter(|card| !gotten.contains(card) && !replay.chop_moved.contains(card))
            .collect::<Vec<_>>();
        let newly_informed = touched
            .iter()
            .copied()
            .filter(|card| !promptable.contains(card))
            .collect::<Vec<_>>();
        let identities = touched
            .iter()
            .filter_map(|card| current_card_identity(view, *card))
            .collect::<Vec<_>>();
        let all_trash = touched.iter().copied().all(|card| {
            hand.iter()
                .find(|candidate| candidate.id == card)
                .is_some_and(|card| {
                    let mut facts = card.clues;
                    facts.add_positive_clue(clue);
                    IdentitySet::from_mask(facts.identity_mask())
                        .iter()
                        .all(|identity| card_is_trash(view, identity))
                })
        });
        let playable = touched
            .iter()
            .filter(|card| !previously_fixed.contains(card))
            .filter(|card| {
                current_card_identity(view, **card)
                    .is_some_and(|identity| is_playable_now(view, identity))
            })
            .count();
        let off_chop_five = clue == Clue::Rank(Rank::Five)
            && identities
                .iter()
                .any(|identity| identity.rank == Rank::Five)
            && chop(layout, gotten).is_none_or(|card| !touched.contains(&card));
        let five_pulled = off_chop_five
            .then(|| five_pulled_card(layout, &touched, gotten))
            .flatten();
        let five_tech_kind = five_pulled
            .and_then(|card| current_card_identity(view, card))
            .and_then(|identity| {
                let height = view.play_stacks[identity.suit.index()].len();
                let rank = usize::from(identity.rank.number());
                let actor = next_player(view.current_player, view.hands.len());
                if rank <= height {
                    finesse_position(&view.hands[actor.index()], gotten, 2)
                        .and_then(|card| card.identity)
                        .is_some_and(|candidate| is_playable_now(view, candidate))
                        .then_some(HGroupMoveKind::Discharge)
                } else if rank == height + 1 {
                    Some(HGroupMoveKind::FivePull)
                } else if rank == height + 2 {
                    let connector = Card::new(identity.suit, Rank::ALL[height]);
                    (actor != target
                        && finesse_position(&view.hands[actor.index()], gotten, 0)
                            .and_then(|card| card.identity)
                            == Some(connector))
                    .then_some(HGroupMoveKind::FivePull)
                } else {
                    finesse_position(&view.hands[actor.index()], gotten, 1)
                        .and_then(|card| card.identity)
                        .is_some_and(|candidate| is_playable_now(view, candidate))
                        .then_some(HGroupMoveKind::Ejection)
                }
            });
        let tempo = newly_touched.is_empty()
            && touched.iter().any(|card| {
                !previously_fixed.contains(card)
                    && !replay.already_playing.contains(card)
                    && !replay.forced_playable.contains(card)
                    && !replay.pending_connections.iter().any(|connection| {
                        connection.actor == target && connection.cards.contains(card)
                    })
                    && current_card_identity(view, *card)
                        .is_some_and(|identity| is_playable_now(view, identity))
            });
        let fills_in = touched.iter().copied().any(|card| {
            hand.iter()
                .find(|candidate| candidate.id == card)
                .is_some_and(|card| !card.clues.has_positive_clue(clue))
        });
        let false_two_save_on_five = clue == Clue::Rank(Rank::Five)
            && touched
                .iter()
                .all(|card| replay.explicitly_clued.contains(card))
            && chop(layout, gotten).is_some_and(|target_chop| {
                let saved_twos = replay.clues.iter().rev().find_map(|interpretation| {
                    let twos = IdentitySet::from_mask(
                        interpretation
                            .save_identities
                            .iter()
                            .filter(|identity| identity.rank == Rank::Two)
                            .fold(0, |mask, identity| mask | (1 << identity.index())),
                    );
                    (!twos.is_empty() && !layout.contains(&interpretation.focus)).then_some(twos)
                });
                saved_twos.is_some_and(|twos| {
                    current_card_identity(view, target_chop)
                        .is_none_or(|actual| !twos.contains(actual))
                })
            });
        if false_two_save_on_five {
            // A repeated 5 transfers the identity of a previously lost saved
            // 2 onto the recipient's chop. The giver sees that chop and must
            // not use the clue as a generic stall when the promise is false.
            continue;
        }
        let stops_existing_play = touched.iter().any(|card| {
            replay.already_playing.contains(card)
                || replay.forced_playable.contains(card)
                || replay
                    .pending_connections
                    .iter()
                    .any(|connection| connection.actor == target && connection.cards.contains(card))
        });
        let stops_bad_existing_play = stops_existing_play
            && touched.iter().copied().any(|card| {
                current_card_identity(view, card).is_some_and(|identity| {
                    !is_playable_now(view, identity)
                        && !convention_playable(view, gotten, card, identity)
                        && !replay.pending_connections.iter().any(|connection| {
                            connection.focus == card
                                && pending_is_active(connection, &replay.pending_connections)
                        })
                })
            });
        let duplicate_touch = identities.len() != identity_set(identities.iter().copied()).len();
        let ejection_actor = next_player(view.current_player, view.hands.len());
        let clue_focus = focus(layout, &touched, chop(layout, gotten), gotten);
        let unresolved_same_clue_connector = target == ejection_actor
            && clue_focus
                .and_then(|focus| {
                    current_card_identity(view, focus).map(|identity| (focus, identity))
                })
                .is_some_and(|(focus, identity)| {
                    let height = view.play_stacks[identity.suit.index()].len();
                    usize::from(identity.rank.number()) > height + 1
                        && touched.iter().copied().any(|card| {
                            card != focus
                                && !promptable.contains(&card)
                                && current_card_identity(view, card).is_some_and(|connector| {
                                    connector.suit == identity.suit
                                        && usize::from(connector.rank.number()) > height
                                        && connector.rank.number() < identity.rank.number()
                                })
                        })
                });
        if unresolved_same_clue_connector {
            // A connector introduced by this clue is not Promptable yet. If
            // the recipient acts next, nobody can give the Out-of-Order Fix
            // needed to distinguish it from the delayed focus.
            continue;
        }
        let no_information_one_fix = clue == Clue::Rank(Rank::One)
            && newly_touched.is_empty()
            && !fills_in
            && touched
                .iter()
                .all(|card| replay.explicitly_clued.contains(card));
        if no_information_one_fix
            && clue_focus
                .and_then(|focus| current_card_identity(view, focus))
                .is_some_and(|identity| !card_is_trash(view, identity))
        {
            // Re-cluing remaining 1s tells the recipient to skip the next
            // one as a no-information Fix. The giver can see that card and
            // must not send this signal when it is still useful.
            continue;
        }
        let fix = newly_touched.is_empty()
            && touched
                .iter()
                .all(|card| replay.explicitly_clued.contains(card))
            && ((fills_in && (duplicate_touch || stops_bad_existing_play))
                || no_information_one_fix);
        let five_ejection = matches!(clue, Clue::Suit(_))
            && clue_focus
                .and_then(|focus| {
                    current_card_identity(view, focus).map(|identity| (focus, identity))
                })
                .is_some_and(|(focus, identity)| {
                    if identity.rank != Rank::Five || replay.explicitly_clued.contains(&focus) {
                        return false;
                    }
                    let height = view.play_stacks[identity.suit.index()].len();
                    let blind_plays = ((height + 1)..usize::from(identity.rank.number()))
                        .filter(|needed_rank| {
                            let needed = Card::new(identity.suit, Rank::ALL[*needed_rank - 1]);
                            !view.hands.iter().flatten().any(|card| {
                                gotten.contains(&card.id)
                                    && (card.identity == Some(needed)
                                        || convention_cards.iter().any(|note| {
                                            note.card == card.id
                                                && note.identities == IdentitySet::singleton(needed)
                                        }))
                            })
                        })
                        .count();
                    blind_plays >= 2
                });
        let ejection_playable = finesse_position(
            &view.hands[ejection_actor.index()],
            &replay.explicitly_clued,
            1,
        )
        .and_then(|card| card.identity)
        .is_some_and(|identity| is_playable_now(view, identity));
        if rule_enabled(profile, HGroupRuleId::EjectionsAndDischarges)
            && five_ejection
            && !ejection_playable
        {
            // The clue still means Ejection to its recipient even when the
            // intended second-position blind play would strike. It cannot be
            // rescued by classifying the same clue as an 8 Clue Save or Stall.
            continue;
        }
        let unknown_discharge = touched.len() >= 2
            && clue_focus.is_some_and(|focus| {
                hand.iter()
                    .find(|card| card.id == focus)
                    .is_some_and(|card| {
                        let mut facts = card.clues;
                        facts.add_positive_clue(clue);
                        let possibilities = IdentitySet::from_mask(facts.identity_mask());
                        !possibilities.is_empty()
                            && possibilities
                                .iter()
                                .all(|identity| card_is_trash(view, identity))
                    })
            });
        let discharge_playable = finesse_position(
            &view.hands[ejection_actor.index()],
            &replay.explicitly_clued,
            2,
        )
        .and_then(|card| card.identity)
        .is_some_and(|identity| is_playable_now(view, identity));
        if rule_enabled(profile, HGroupRuleId::EjectionsAndDischarges)
            && unknown_discharge
            && !discharge_playable
        {
            // A Discharge is a promise that the next player's Third Finesse
            // Position will play. The clue giver can see that card and must
            // not create a forced misplay.
            continue;
        }
        let has_elimination_notes = replay.signals.iter().any(|signal| {
            signal.kind == HGroupMoveKind::Elimination
                && signal.target == Some(target)
                && signal.identity.is_some()
        });
        let safe_generic_play = clue_focus
            .and_then(|focus| current_card_identity(view, focus).map(|identity| (focus, identity)))
            .is_none_or(|(focus, identity)| {
                let height = view.play_stacks[identity.suit.index()].len();
                usize::from(identity.rank.number()) == height + 1
                    || delayed_connection_score(
                        view,
                        target,
                        focus,
                        identity,
                        &replay.explicitly_clued,
                        &replay.already_playing,
                    )
                    .is_some()
                        && !prospective_clue_has_unsafe_connection(
                            view, profile, target, focus, clue, &touched, false,
                        )
            });
        let elimination = has_elimination_notes
            && touched.len() == 1
            && replay.explicitly_clued.contains(&touched[0])
            && fills_in
            && safe_generic_play;
        let delayed = identities.iter().find(|identity| {
            usize::from(identity.rank.number()) > view.play_stacks[identity.suit.index()].len() + 1
        });
        let respects_good_touch = good_touch(
            view,
            &newly_informed,
            &promptable,
            &previously_fixed,
            convention_cards,
        );
        let out_of_order = clue_focus
            .and_then(|focus| current_card_identity(view, focus).map(|identity| (focus, identity)))
            .is_some_and(|(focus, identity)| {
                let height = view.play_stacks[identity.suit.index()].len();
                let fix_is_available =
                    hand.iter()
                        .find(|card| card.id == focus)
                        .is_some_and(|card| {
                            let mut prospective = card.clues;
                            prospective.add_positive_clue(clue);
                            !prospective.has_positive_clue(Clue::Suit(identity.suit))
                                || !prospective.has_positive_clue(Clue::Rank(identity.rank))
                        });
                safe_generic_play
                    && respects_good_touch
                    && fix_is_available
                    && target != next_player(view.current_player, view.hands.len())
                    && usize::from(identity.rank.number()) > height + 1
                    && touched.iter().copied().any(|card| {
                        card != focus
                            && current_card_identity(view, card).is_some_and(|candidate| {
                                candidate.suit == identity.suit
                                    && candidate.rank.number() > u8::try_from(height).unwrap_or(0)
                                    && candidate.rank.number() < identity.rank.number()
                            })
                    })
                    && out_of_order_connections_accounted(
                        view,
                        target,
                        focus,
                        identity,
                        &touched,
                        gotten,
                        &replay.already_playing,
                    )
            });
        let bluff = delayed.is_some_and(|focus| {
            let actor = next_player(view.current_player, view.hands.len());
            if actor == target {
                return false;
            }
            let actor_is_loaded = replay.pending_connections.iter().any(|connection| {
                connection.actor == actor
                    && pending_is_active(connection, &replay.pending_connections)
            }) || replay.hands[actor.index()].iter().any(|card| {
                (gotten.contains(card) || replay.forced_playable.contains(card))
                    && current_card_identity(view, *card)
                        .is_some_and(|identity| is_playable_now(view, identity))
            });
            if actor_is_loaded {
                return false;
            }
            if !bluff_focus_is_one_away(view, *focus, gotten) {
                return false;
            }
            view.hands[actor.index()]
                .iter()
                .rev()
                .find(|candidate| !gotten.contains(&candidate.id))
                .and_then(|candidate| candidate.identity)
                .is_some_and(|actual| {
                    let height = view.play_stacks[focus.suit.index()].len();
                    is_playable_now(view, actual)
                        && height < Rank::ALL.len()
                        && actual != Card::new(focus.suit, Rank::ALL[height])
                })
        });
        let distinct_touched_identities = identity_set(identities.iter().copied());
        let every_touched_card_is_playable = identities.len() == touched.len()
            && playable == identities.len()
            && distinct_touched_identities.len() == identities.len();
        let charm = rule_enabled(profile, HGroupRuleId::Charms)
            && clue == Clue::Rank(Rank::Four)
            && target != next_player(view.current_player, view.hands.len())
            && clue_focus.is_some_and(|focus| {
                newly_touched.contains(&focus)
                    && current_card_identity(view, focus).is_some_and(|identity| {
                        let height = view.play_stacks[identity.suit.index()].len();
                        usize::from(identity.rank.number()) == height + 4
                    })
                    && finesse_position(
                        &view.hands[next_player(view.current_player, view.hands.len()).index()],
                        gotten,
                        3,
                    )
                    .and_then(|card| card.identity)
                    .is_some_and(|identity| is_playable_now(view, identity))
            });
        let eight_clue_save = rule_enabled(profile, HGroupRuleId::Stalling)
            && !replay.early_game
            && view.clue_tokens == MAX_CLUE_TOKENS
            && clue_focus.is_some_and(|focus| layout.last() != Some(&focus));

        let classification = if rule_enabled(profile, HGroupRuleId::Ignition)
            && playable >= 2
            && every_touched_card_is_playable
            && respects_good_touch
        {
            Some((HGroupMoveKind::Ignition, 360))
        } else if rule_enabled(profile, HGroupRuleId::BasicStrategy) && fix {
            Some((HGroupMoveKind::FixClue, 500))
        } else if rule_enabled(profile, HGroupRuleId::EjectionsAndDischarges) && five_ejection {
            Some((HGroupMoveKind::Ejection, 290))
        } else if rule_enabled(profile, HGroupRuleId::EjectionsAndDischarges) && unknown_discharge {
            Some((HGroupMoveKind::Discharge, 285))
        } else if rule_enabled(profile, HGroupRuleId::Bluffs) && bluff {
            Some((HGroupMoveKind::Bluff, 280))
        } else if rule_enabled(profile, HGroupRuleId::Elimination) && elimination {
            Some((HGroupMoveKind::Elimination, 230))
        } else if rule_enabled(profile, HGroupRuleId::OutOfOrderPlay) && out_of_order {
            Some((HGroupMoveKind::OccupiedPlay, 220))
        } else if rule_enabled(profile, HGroupRuleId::ChopMoves) && all_trash {
            Some((HGroupMoveKind::ChopMove, 210))
        } else if rule_enabled(profile, HGroupRuleId::TempoClues) && tempo {
            let valuable = playable >= 2 || actor_locked;
            if valuable || stalling {
                Some((HGroupMoveKind::TempoClue, if valuable { 205 } else { 90 }))
            } else {
                Some((HGroupMoveKind::ChopMove, 180))
            }
        } else if rule_enabled(profile, HGroupRuleId::FiveTech) && five_tech_kind.is_some() {
            Some((five_tech_kind.expect("checked above"), 150))
        } else if rule_enabled(profile, HGroupRuleId::Extras)
            && respects_good_touch
            && !newly_touched.is_empty()
            && touched.len() > newly_touched.len()
            && delayed.is_none()
        {
            Some((HGroupMoveKind::Extra, 145))
        } else if off_chop_five && stalling {
            Some((HGroupMoveKind::FiveStall, 80))
        } else if charm {
            Some((HGroupMoveKind::Charm, 70))
        } else if eight_clue_save {
            Some((HGroupMoveKind::SaveClue, 50))
        } else if rule_enabled(profile, HGroupRuleId::Stalling)
            && newly_touched.is_empty()
            && fills_in
            && (actor_locked || view.clue_tokens == MAX_CLUE_TOKENS)
        {
            Some((HGroupMoveKind::Stall, 40))
        } else {
            None
        };
        let Some((kind, score)) = classification else {
            continue;
        };
        let score_is_low = view.play_stacks.iter().map(Vec::len).sum::<usize>() < 10;
        let unsafe_unsuppressed_play =
            matches!(
                kind,
                HGroupMoveKind::FixClue
                    | HGroupMoveKind::Elimination
                    | HGroupMoveKind::OccupiedPlay
                    | HGroupMoveKind::ChopMove
                    | HGroupMoveKind::TempoClue
                    | HGroupMoveKind::Extra
                    | HGroupMoveKind::Stall
            ) || (kind == HGroupMoveKind::FiveStall && !replay.early_game && !score_is_low);
        if !safe_generic_play && unsafe_unsuppressed_play {
            // These moves do not replace a delayed Play interpretation. If
            // the focused card can be read as a Play clue, its ordinary
            // Prompt/Finesse chain must also be valid. Otherwise an advanced
            // stall or chop move can manufacture a false layered finesse.
            continue;
        }
        if clue_focus.is_some_and(|focus| {
            prospective_clue_has_unsafe_connection(
                view, profile, target, focus, clue, &touched, false,
            )
        }) {
            // Advanced classifications can create indirect effects (for
            // example, a same-clue chop move followed by a 2 Save on 5) that
            // are not represented by the focused card's generic safety test.
            // Validate the recipient's complete post-clue inference as well.
            continue;
        }
        let efficiency = if kind == HGroupMoveKind::Ignition {
            2 * u16::try_from(newly_touched.len()).unwrap_or(0)
        } else {
            0
        };
        candidates.push(ClueCandidate {
            action,
            score: score + efficiency + u16::from(matches!(clue, Clue::Suit(_))),
            target,
            save: kind == HGroupMoveKind::SaveClue,
            immediate_play: playable > 0,
        });
    }
    candidates
}

fn bluff_focus_is_one_away(view: &PlayerView, focus: Card, gotten: &CardSet) -> bool {
    let height = view.play_stacks[focus.suit.index()].len();
    let rank = usize::from(focus.rank.number());
    rank > height + 1
        && ((height + 2)..rank).all(|needed_rank| {
            let needed = Card::new(focus.suit, Rank::ALL[needed_rank - 1]);
            view.hands
                .iter()
                .flatten()
                .any(|card| gotten.contains(&card.id) && card.identity == Some(needed))
        })
}

fn out_of_order_connections_accounted(
    view: &PlayerView,
    target: PlayerId,
    focus: CardId,
    identity: Card,
    touched: &[CardId],
    gotten: &CardSet,
    already_playing: &CardSet,
) -> bool {
    let height = view.play_stacks[identity.suit.index()].len();
    ((height + 1)..usize::from(identity.rank.number())).all(|rank| {
        let needed = Card::new(identity.suit, Rank::ALL[rank - 1]);
        touched
            .iter()
            .copied()
            .any(|card| card != focus && current_card_identity(view, card) == Some(needed))
            || view.hands.iter().flatten().any(|card| {
                card.id != focus
                    && (gotten.contains(&card.id) || already_playing.contains(&card.id))
                    && card.identity == Some(needed)
            })
            || view
                .hands
                .iter()
                .enumerate()
                .filter(|(player, _)| *player != view.observer.index() && *player != target.index())
                .any(|(_, hand)| {
                    hand.iter()
                        .rev()
                        .filter(|card| !gotten.contains(&card.id))
                        .take_while(|card| {
                            card.identity
                                .is_some_and(|candidate| is_playable_now(view, candidate))
                        })
                        .any(|card| card.identity == Some(needed))
                })
    })
}

#[allow(clippy::too_many_arguments)]
fn save_clue_score(
    view: &PlayerView,
    target_hand: &[hanabi_core::ObservedCard],
    focus: CardId,
    identity: Card,
    clue: Clue,
    target: PlayerId,
    next_player: PlayerId,
    layouts: &[Vec<CardId>],
    gotten: &CardSet,
) -> Option<u16> {
    if is_playable_now(view, identity)
        && !matches!((clue, identity.rank), (Clue::Rank(Rank::Two), Rank::Two))
    {
        // Play Clue interpretation takes precedence for a playable focus. If
        // that interpretation is unsafe (for example, because it creates a
        // false Prompt), the same clue cannot be rescued by calling it a Save.
        return None;
    }
    let chops = layouts
        .iter()
        .map(|hand| chop(hand, gotten))
        .collect::<Vec<_>>();
    let valid = match (clue, identity.rank) {
        (Clue::Rank(Rank::Five), Rank::Five) => true,
        (Clue::Rank(Rank::Two), Rank::Two) => {
            !card_is_trash(view, identity)
                && !has_false_two_save_prompt(target_hand, focus, identity, gotten)
                && two_save_allowed(view, focus, identity, &chops)
        }
        (_, Rank::Five) => false,
        _ => is_critical(view, identity),
    };
    if !valid || !target_hand.iter().any(|card| card.id == focus) {
        return None;
    }
    // Save Principle, with urgency as a deterministic tie-break.
    Some(if target == next_player { 450 } else { 400 })
}

fn has_false_two_save_prompt(
    target_hand: &[hanabi_core::ObservedCard],
    focus: CardId,
    identity: Card,
    gotten: &CardSet,
) -> bool {
    let connector = Card::new(identity.suit, Rank::One);
    target_hand.iter().any(|card| {
        card.id != focus
            && gotten.contains(&card.id)
            && card.clues.allows(connector)
            && card.identity != Some(connector)
    })
}

#[allow(clippy::too_many_arguments)]
fn play_clue_score(
    view: &PlayerView,
    target: PlayerId,
    focus: CardId,
    focus_identity: Card,
    clue: Clue,
    newly_touched: &[CardId],
    explicitly_clued: &CardSet,
    fixed_cards: &CardSet,
    already_playing: &CardSet,
    convention_cards: &[HGroupCardInference],
) -> Option<u16> {
    if !good_touch(
        view,
        newly_touched,
        explicitly_clued,
        fixed_cards,
        convention_cards,
    ) {
        return None;
    }
    let height = view.play_stacks[focus_identity.suit.index()].len();
    let rank = usize::from(focus_identity.rank.number());
    if rank <= height {
        return None;
    }
    if target == next_player(view.current_player, view.hands.len())
        && rank > height + 1
        && newly_touched.iter().copied().any(|card| {
            current_card_identity(view, card).is_some_and(|connector| {
                connector.suit == focus_identity.suit
                    && usize::from(connector.rank.number()) > height
                    && connector.rank.number() < focus_identity.rank.number()
            })
        })
    {
        // The next player cannot distinguish a newly introduced connector
        // from the delayed focus without an intervening Out-of-Order Fix.
        return None;
    }
    let base = if rank == height + 1 {
        330
    } else {
        delayed_connection_score(
            view,
            target,
            focus,
            focus_identity,
            explicitly_clued,
            already_playing,
        )?
    };
    Some(
        base + 2 * u16::try_from(newly_touched.len()).unwrap_or(0)
            + u16::from(matches!(clue, Clue::Suit(_))),
    )
}

fn good_touch(
    view: &PlayerView,
    newly_touched: &[CardId],
    explicitly_clued: &CardSet,
    fixed_cards: &CardSet,
    convention_cards: &[HGroupCardInference],
) -> bool {
    let known_identity = |card: CardId| {
        current_card_identity(view, card).or_else(|| {
            convention_cards
                .iter()
                .find(|note| note.card == card && note.identities.len() == 1)
                .and_then(|note| note.identities.iter().next())
        })
    };
    let mut identities = IdentitySet::default();
    for card in newly_touched {
        let Some(identity) = known_identity(*card) else {
            return false;
        };
        if !is_eventually_useful(view, identity) || identities.contains(identity) {
            return false;
        }
        identities = identities.union(IdentitySet::singleton(identity));
        if view.hands.iter().flatten().any(|candidate| {
            candidate.id != *card
                && explicitly_clued.contains(&candidate.id)
                && !fixed_cards.contains(&candidate.id)
                && (known_identity(candidate.id) == Some(identity)
                    || (identity.rank == Rank::One
                        && candidate.identity.is_none()
                        && convention_cards.iter().any(|note| {
                            note.card == candidate.id && note.identities.contains(identity)
                        })))
        }) {
            return false;
        }
    }
    true
}

fn is_eventually_useful(view: &PlayerView, identity: Card) -> bool {
    let stack_height = view.play_stacks[identity.suit.index()].len();
    if usize::from(identity.rank.number()) <= stack_height {
        return false;
    }
    Rank::ALL
        .iter()
        .copied()
        .filter(|rank| {
            usize::from(rank.number()) > stack_height && rank.number() < identity.rank.number()
        })
        .all(|rank| {
            let lower = Card::new(identity.suit, rank);
            view.discard_pile
                .iter()
                .filter(|(_, card)| *card == lower)
                .count()
                < usize::from(rank.copies())
        })
}

fn is_convention_trash(
    view: &PlayerView,
    identity: Card,
    gotten: &CardSet,
    own_notes: &[HGroupCardInference],
) -> bool {
    if !is_eventually_useful(view, identity) {
        return true;
    }
    view.hands
        .iter()
        .flatten()
        .filter(|card| gotten.contains(&card.id))
        .filter(|card| {
            card.identity == Some(identity)
                || (card.identity.is_none()
                    && own_notes.iter().any(|note| {
                        note.card == card.id
                            && note.identities.len() == 1
                            && note.identities.contains(identity)
                    }))
        })
        .take(2)
        .count()
        >= 2
}

fn is_unique_visible(view: &PlayerView, excluded: CardId, identity: Card) -> bool {
    !view
        .hands
        .iter()
        .flatten()
        .any(|card| card.id != excluded && card.identity == Some(identity))
}

fn delayed_connection_score(
    view: &PlayerView,
    target: PlayerId,
    focus: CardId,
    focus_identity: Card,
    explicitly_clued: &CardSet,
    already_playing: &CardSet,
) -> Option<u16> {
    let stack_height = view.play_stacks[focus_identity.suit.index()].len();
    if usize::from(focus_identity.rank.number()) <= stack_height + 1
        || stack_height >= Rank::ALL.len()
    {
        return None;
    }
    let connector = Card::new(focus_identity.suit, Rank::ALL[stack_height]);
    let next = (view.current_player.index() + 1) % view.hands.len();
    let connection_hand = &view.hands[next];
    let prompt_candidates = connection_hand
        .iter()
        .rev()
        .filter(|card| {
            card.id != focus
                && explicitly_clued.contains(&card.id)
                && !already_playing.contains(&card.id)
                && card.clues.allows(connector)
        })
        .collect::<Vec<_>>();
    let first_connection_valid = if prompt_candidates.is_empty() {
        if target.index() == next {
            false
        } else {
            connection_hand
                .iter()
                .rev()
                .find(|card| !explicitly_clued.contains(&card.id))
                .is_some_and(|card| card.identity == Some(connector))
        }
    } else {
        prompt_candidates
            .iter()
            .position(|card| card.identity == Some(connector))
            .is_some_and(|correct| {
                prompt_candidates[..correct].iter().all(|card| {
                    card.identity
                        .is_some_and(|identity| is_playable_now(view, identity))
                })
            })
    };
    if !first_connection_valid {
        return None;
    }

    for needed_rank in (stack_height + 2)..usize::from(focus_identity.rank.number()) {
        let needed = Card::new(focus_identity.suit, Rank::ALL[needed_rank - 1]);
        let false_prompt = view.hands.iter().flatten().any(|card| {
            card.id != focus
                && explicitly_clued.contains(&card.id)
                && !already_playing.contains(&card.id)
                && card.clues.allows(needed)
                && card
                    .identity
                    .is_some_and(|actual| actual != needed && !is_playable_now(view, actual))
        });
        if false_prompt {
            // Prompts take precedence over later Prompts and Finesses. A
            // clued card that can be mistaken for this connector and would
            // misplay therefore invalidates the whole delayed clue, even if
            // the correct connector is also visible elsewhere.
            return None;
        }
        if !view
            .hands
            .iter()
            .flatten()
            .any(|card| explicitly_clued.contains(&card.id) && card.identity == Some(needed))
        {
            return None;
        }
    }
    Some(if prompt_candidates.is_empty() {
        370
    } else {
        380
    })
}

fn tempo_clue_candidates(
    view: &PlayerView,
    replay: &HGroupState,
    gotten: &CardSet,
) -> Vec<ClueCandidate> {
    let mut candidates = Vec::new();
    for action in view.legal_actions() {
        let Action::Clue { target, clue } = action else {
            continue;
        };
        let hand = &view.hands[target.index()];
        let touched = hand
            .iter()
            .filter(|card| card.identity.is_some_and(|identity| clue.matches(identity)))
            .map(|card| card.id)
            .collect::<Vec<_>>();
        if touched.iter().all(|card| {
            hand.iter()
                .find(|candidate| candidate.id == *card)
                .is_some_and(|card| card.clues.has_positive_clue(clue))
        }) {
            continue;
        }
        if touched.iter().any(|card| !gotten.contains(card)) {
            continue;
        }
        let target_chop = chop(&replay.hands[target.index()], gotten);
        let Some(focus) = focus(&replay.hands[target.index()], &touched, target_chop, gotten)
        else {
            continue;
        };
        let Some(identity) = hand
            .iter()
            .find(|card| card.id == focus)
            .and_then(|card| card.identity)
        else {
            continue;
        };
        if is_playable_now(view, identity) {
            candidates.push(ClueCandidate {
                action,
                score: 100 + u16::from(matches!(clue, Clue::Suit(_))),
                target,
                save: false,
                immediate_play: true,
            });
        }
    }
    candidates
}

fn has_out_of_order_prompt(view: &PlayerView, gotten: &CardSet) -> bool {
    for action in view.legal_actions() {
        let Action::Clue { target, clue } = action else {
            continue;
        };
        let hand = &view.hands[target.index()];
        let touched = hand
            .iter()
            .filter(|card| card.identity.is_some_and(|identity| clue.matches(identity)))
            .map(|card| card.id)
            .collect::<Vec<_>>();
        let Some(focus) = hand
            .iter()
            .rev()
            .find(|card| touched.contains(&card.id))
            .map(|card| card.id)
        else {
            continue;
        };
        let Some(focus_identity) = identity_of(view, focus) else {
            continue;
        };
        let height = view.play_stacks[focus_identity.suit.index()].len();
        if usize::from(focus_identity.rank.number()) != height + 2 {
            continue;
        }
        let expected = Card::new(focus_identity.suit, Rank::ALL[height]);
        let next = (view.current_player.index() + 1) % view.hands.len();
        let prompt_candidates = view.hands[next]
            .iter()
            .rev()
            .filter(|card| gotten.contains(&card.id) && card.clues.allows(expected))
            .collect::<Vec<_>>();
        if let Some(correct) = prompt_candidates
            .iter()
            .position(|card| card.identity == Some(expected))
        {
            if prompt_candidates[..correct].iter().any(|card| {
                card.identity
                    .is_some_and(|identity| !is_playable_now(view, identity))
            }) {
                return true;
            }
        }
    }
    false
}

#[allow(clippy::too_many_lines)]
fn infer_clue_to_self(
    deductions: &LogicalDeductions,
    clue: &HGroupClueInterpretation,
    explicitly_clued: &CardSet,
    inferred: &mut HGroupInferences,
) {
    if inferred.signals.iter().any(|signal| {
        signal.turn == clue.turn
            && matches!(
                signal.kind,
                HGroupMoveKind::Ejection | HGroupMoveKind::Discharge
            )
    }) {
        // These moves use the clue only as an instruction to another reacting
        // player. They do not also promise that the clue focus should play.
        return;
    }
    if inferred.signals.iter().any(|signal| {
        signal.kind == HGroupMoveKind::Bluff
            && signal.cards.last() == Some(&clue.focus)
            && signal.turn >= clue.turn
    }) {
        // Once the immediately following blind play disconnects from the
        // clue, the target knows the focus is merely one-away-from-playable.
        // It must be held rather than played as the imagined connector.
        return;
    }
    let view = deductions.view();
    let demonstrated_ejection = matches!(clue.clue, Clue::Suit(_))
        && !clue.new_non_focus.is_empty()
        && deductions
            .possible_identities(clue.focus)
            .is_some_and(|identities| {
                identities
                    .iter()
                    .any(|identity| identity.rank == Rank::Five)
            })
        && view.history.iter().any(|entry| {
            entry.turn == clue.turn + 1
                && matches!(
                    entry.event,
                    ObservedEvent::Played {
                        player,
                        card,
                        successful: true,
                        ..
                    } if player == next_player(clue.giver, view.hands.len())
                        && !was_clued_before(view, entry.turn, card)
                )
        });
    if demonstrated_ejection {
        // The target cannot see that its own focus is a 5. The intervening
        // player's immediate blind play is the public proof that the clue was
        // a 5 Color Ejection, not a direct Play clue on the focus.
        return;
    }
    let allow_direct_play = match clue.kind {
        HGroupClueKind::Save(_) => return,
        HGroupClueKind::PlayOrSave if inferred.saved.contains(&clue.focus) => return,
        HGroupClueKind::Play | HGroupClueKind::PlayOrSave => true,
        HGroupClueKind::Unrecognized => false,
    };

    let Some(logical_possibilities) = deductions.possible_identities(clue.focus) else {
        return;
    };
    let convention_possibilities = inferred
        .cards
        .iter()
        .find(|card| card.card == clue.focus)
        .map_or(clue.focus_identities, |card| card.identities)
        .intersection(clue.focus_identities);
    let focus_possibilities = logical_possibilities.intersection(convention_possibilities);
    if focus_possibilities.is_empty() {
        return;
    }
    let direct = identities_at_distance_at(focus_possibilities, clue.stack_heights, 0);
    let delayed = delayed_focus_identities(
        focus_possibilities,
        clue.stack_heights,
        view,
        explicitly_clued,
        clue.focus,
    );
    let live_direct = IdentitySet::from_mask(
        direct
            .iter()
            .filter(|identity| is_playable_now(view, *identity))
            .fold(0, |mask, identity| mask | (1 << identity.index())),
    );
    let prompt_identities = IdentitySet::from_mask(delayed.iter().fold(0, |mask, identity| {
        let connector = Card::new(
            identity.suit,
            Rank::ALL[usize::from(clue.stack_heights[identity.suit.index()])],
        );
        if is_playable_now(view, connector) {
            mask | (1 << connector.index())
        } else {
            mask
        }
    }));
    // Once a connecting play made after this clue has brought one of the
    // focus possibilities onto the stack, the promised focus is due. Do not
    // reinterpret an ancillary card touched by the same clue as a new
    // Self-Prompt after the original connection has already been demonstrated.
    let completed_connection = direct.iter().any(|identity| {
        let Some(previous_rank) = identity.rank.index().checked_sub(1) else {
            return false;
        };
        let connector = Card::new(identity.suit, Rank::ALL[previous_rank]);
        view.history.iter().any(|entry| {
            entry.turn > clue.turn
                && matches!(
                    entry.event,
                    ObservedEvent::Played {
                        identity: played,
                        successful: true,
                        ..
                    } if played == connector
                )
        })
    });
    if allow_direct_play
        && completed_connection
        && !live_direct.is_empty()
        && !inferred.playable_now.contains(&clue.focus)
    {
        inferred.playable_now.push(clue.focus);
        return;
    }

    let direct_identities_claimed = !live_direct.is_empty()
        && live_direct.iter().all(|identity| {
            view.hands.iter().flatten().any(|card| {
                card.id != clue.focus
                    && explicitly_clued.contains(&card.id)
                    && (card.identity == Some(identity)
                        || inferred
                            .cards
                            .iter()
                            .any(|note| note.card == card.id && note.identities.contains(identity)))
            })
        });
    // A Self-Prompt only exists when no unclaimed identity allowed for the
    // focus can be played immediately. Good Touch may eliminate the direct
    // identities when matching cards are already promised elsewhere.
    if allow_direct_play
        && !live_direct.is_empty()
        && focus_possibilities.without(live_direct).is_empty()
        && !direct_identities_claimed
        && !inferred.playable_now.contains(&clue.focus)
    {
        inferred.playable_now.push(clue.focus);
    } else if allow_direct_play {
        if let Some(connection) = find_prompt(
            deductions,
            explicitly_clued,
            &inferred.cards,
            true,
            clue.focus,
            prompt_identities,
            clue.focus,
        ) {
            inferred.connection = Some(connection);
        }
    }
}

fn delayed_focus_identities(
    identities: IdentitySet,
    stack_heights: [u8; 5],
    view: &PlayerView,
    gotten: &CardSet,
    excluded: CardId,
) -> IdentitySet {
    let mask = identities
        .iter()
        .filter(|identity| {
            let height = usize::from(stack_heights[identity.suit.index()]);
            let rank = usize::from(identity.rank.number());
            rank > height + 1
                && ((height + 2)..rank).all(|needed_rank| {
                    let needed = Card::new(identity.suit, Rank::ALL[needed_rank - 1]);
                    view.hands.iter().flatten().any(|card| {
                        card.id != excluded
                            && gotten.contains(&card.id)
                            && card.identity.map_or_else(
                                || card.clues.allows(needed),
                                |actual| actual == needed,
                            )
                    })
                })
        })
        .fold(0, |mask, identity| mask | (1 << identity.index()));
    IdentitySet::from_mask(mask)
}

fn find_prompt(
    deductions: &LogicalDeductions,
    explicitly_clued: &CardSet,
    convention_cards: &[HGroupCardInference],
    prefer_convention_identities: bool,
    excluded: CardId,
    connection_identities: IdentitySet,
    focus: CardId,
) -> Option<HGroupConnection> {
    let hand = &deductions.view().hands[deductions.view().observer.index()];
    for card in hand
        .iter()
        .rev()
        .filter(|card| card.id != excluded && explicitly_clued.contains(&card.id))
    {
        let possibilities = if prefer_convention_identities {
            convention_cards
                .iter()
                .find(|note| note.card == card.id)
                .map(|note| note.identities)
                .or_else(|| deductions.possible_identities(card.id))?
        } else {
            deductions.possible_identities(card.id)?
        };
        let matching = possibilities.intersection(connection_identities);
        if matching.is_empty() {
            continue;
        }
        let identity = matching.iter().next()?;
        return Some(HGroupConnection {
            card: card.id,
            identity,
            kind: HGroupConnectionKind::Prompt,
            focus,
        });
    }
    None
}

fn identities_at_distance(identities: IdentitySet, view: &PlayerView, distance: u8) -> IdentitySet {
    let stack_heights = std::array::from_fn(|index| {
        u8::try_from(view.play_stacks[index].len())
            .expect("a standard stack has at most five cards")
    });
    identities_at_distance_at(identities, stack_heights, distance)
}

fn identities_at_distance_at(
    identities: IdentitySet,
    stack_heights: [u8; 5],
    distance: u8,
) -> IdentitySet {
    let mask = identities
        .iter()
        .filter(|identity| {
            let height = stack_heights[identity.suit.index()];
            identity.rank.number() == height + distance + 1
        })
        .fold(0, |mask, identity| mask | (1 << identity.index()));
    IdentitySet::from_mask(mask)
}

#[allow(clippy::too_many_lines)]
fn convention_card_inferences(
    deductions: &LogicalDeductions,
    replay: &HGroupState,
) -> Vec<HGroupCardInference> {
    let view = deductions.view();
    let mut cards = view.hands[view.observer.index()]
        .iter()
        .filter_map(|card| {
            deductions
                .possible_identities(card.id)
                .map(|identities| HGroupCardInference {
                    card: card.id,
                    identities,
                    focused: false,
                    saved: false,
                    // Invisible touch also covers passive transfer-discard
                    // knowledge. Only an active pending connection (handled
                    // below) or a forced-play effect creates a play promise.
                    finessed: false,
                })
        })
        .collect::<Vec<_>>();

    for clue in &replay.clues {
        if !replay.invalidated_focuses.contains(&clue.focus) {
            if let Some(card) = cards.iter_mut().find(|card| card.card == clue.focus) {
                let resolved_bluff = replay.signals.iter().any(|signal| {
                    signal.kind == HGroupMoveKind::Bluff
                        && signal.turn >= clue.turn
                        && signal.cards.last() == Some(&clue.focus)
                });
                if resolved_bluff {
                    let one_away =
                        identities_at_distance_at(card.identities, clue.stack_heights, 1);
                    if !one_away.is_empty() {
                        card.identities = one_away;
                    }
                    card.focused = true;
                    card.saved = false;
                } else {
                    let clue_time = clue.play_identities.union(clue.save_identities);
                    // A Play promise is fixed at clue time. When a matching copy
                    // reaches the stack later, the old focus becomes known trash;
                    // it does not silently migrate to the next still-live rank.
                    // Only an explicit Fix may reinterpret that promise.
                    let mut narrowed = card.identities.intersection(clue_time);
                    if clue.play_identities.len() > 1 {
                        // An ambiguous delayed Play clue is conditional on its
                        // connector. Once the lower candidate has actually
                        // reached the stack, the still-live alternative is the
                        // focus identity. Treating the per-card clue note as an
                        // independent fact forgot that implication as soon as
                        // the connection obligation resolved.
                        let live = IdentitySet::from_mask(
                            narrowed
                                .iter()
                                .filter(|identity| {
                                    identity.rank.number()
                                        > u8::try_from(
                                            view.play_stacks[identity.suit.index()].len(),
                                        )
                                        .expect("a standard stack has at most five cards")
                                })
                                .fold(0, |mask, identity| mask | (1 << identity.index())),
                        );
                        if !live.is_empty() {
                            narrowed = live;
                        }
                    }
                    if !narrowed.is_empty() {
                        card.identities = narrowed;
                    }
                    card.focused = true;
                    card.saved |= !card
                        .identities
                        .intersection(clue.save_identities)
                        .is_empty();
                }
            }
        }
        let intentionally_duplicates = replay.signals.iter().any(|signal| {
            signal.turn == clue.turn
                && matches!(
                    signal.kind,
                    HGroupMoveKind::FixClue | HGroupMoveKind::Duplication
                )
        });
        if !intentionally_duplicates && clue.focus_identities.len() == 1 {
            for previous in &clue.previously_gotten {
                let Some(card) = cards.iter_mut().find(|card| card.card == *previous) else {
                    continue;
                };
                if clue.giver == view.observer && card.identities.len() > 1 {
                    // A clue giver cannot use the hidden identity of their
                    // own ambiguous card to retroactively apply Good Touch.
                    // Only an exact note makes a duplicate intentional from
                    // the giver's perspective.
                    continue;
                }
                let narrowed = card.identities.without(clue.focus_identities);
                if !narrowed.is_empty() {
                    card.identities = narrowed;
                }
            }
        }
        for (non_focus, good_touch) in &clue.non_focus_identities {
            let convention_dupes = cards
                .iter()
                .filter(|other| other.card != *non_focus && other.identities.len() == 1)
                .fold(IdentitySet::default(), |duplicates, other| {
                    duplicates.union(other.identities)
                });
            if let Some(card) = cards.iter_mut().find(|card| card.card == *non_focus) {
                let narrowed = card
                    .identities
                    .intersection(good_touch.without(convention_dupes));
                if !narrowed.is_empty() {
                    card.identities = narrowed;
                }
            }
        }
    }

    for pending in replay.pending_connections.iter().filter(|pending| {
        pending.actor == view.observer && pending_is_active(pending, &replay.pending_connections)
    }) {
        for pending_card in &pending.cards {
            let Some(card) = cards.iter_mut().find(|card| card.card == *pending_card) else {
                continue;
            };
            let expected = IdentitySet::singleton(pending.expected);
            if pending.kind == HGroupConnectionKind::Finesse {
                let allowed = if pending.cards.len() == 1 {
                    expected
                } else {
                    expected.union(identities_at_distance(card.identities, view, 0))
                };
                let narrowed = card.identities.intersection(allowed);
                if !narrowed.is_empty() {
                    card.identities = narrowed;
                }
                card.finessed = true;
            } else {
                let successful_alternatives = identities_at_distance(card.identities, view, 0);
                let narrowed = card
                    .identities
                    .intersection(expected.union(successful_alternatives));
                if !narrowed.is_empty() {
                    card.identities = narrowed;
                }
                // Later candidates are conditionally constrained only after
                // every earlier candidate is a wrong successful play.
                break;
            }
        }
    }
    for forced in &replay.forced_playable {
        let Some(card) = cards.iter_mut().find(|card| card.card == *forced) else {
            continue;
        };
        let playable = identities_at_distance(card.identities, view, 0);
        if !playable.is_empty() {
            card.identities = playable;
        }
        card.finessed = true;
    }
    for (saved, identities) in &replay.implicit_saves {
        let Some(card) = cards.iter_mut().find(|card| card.card == *saved) else {
            continue;
        };
        let narrowed = card.identities.intersection(*identities);
        if !narrowed.is_empty() {
            card.identities = narrowed;
        }
        card.saved = true;
    }
    cards
}

fn convention_playable(
    view: &PlayerView,
    gotten: &CardSet,
    excluded: CardId,
    identity: Card,
) -> bool {
    let stack_height = view.play_stacks[identity.suit.index()].len();
    let rank = usize::from(identity.rank.number());
    if rank <= stack_height {
        return false;
    }
    ((stack_height + 1)..rank).all(|needed_rank| {
        let needed = Card::new(identity.suit, Rank::ALL[needed_rank - 1]);
        view.hands.iter().flatten().any(|card| {
            card.id != excluded
                && gotten.contains(&card.id)
                && card
                    .identity
                    .map_or_else(|| card.clues.allows(needed), |actual| actual == needed)
        })
    })
}

fn two_save_allowed(
    view: &PlayerView,
    focus: CardId,
    identity: Card,
    chops: &[Option<CardId>],
) -> bool {
    let visible_copies = view
        .hands
        .iter()
        .flatten()
        .filter(|card| card.id != focus && card.identity == Some(identity))
        .collect::<Vec<_>>();
    visible_copies.is_empty()
        || visible_copies
            .iter()
            .all(|card| chops.contains(&Some(card.id)))
}

fn clue_kind_from_masks(clue: Clue, play: IdentitySet, save: IdentitySet) -> HGroupClueKind {
    match (play.is_empty(), save.is_empty()) {
        (false, false) => HGroupClueKind::PlayOrSave,
        (false, true) => HGroupClueKind::Play,
        (true, false) => match clue {
            Clue::Rank(Rank::Five) => HGroupClueKind::Save(HGroupSaveKind::Five),
            Clue::Rank(Rank::Two) => HGroupClueKind::Save(HGroupSaveKind::Two),
            _ => HGroupClueKind::Save(HGroupSaveKind::Critical),
        },
        (true, true) => HGroupClueKind::Unrecognized,
    }
}

fn is_playable_now(view: &PlayerView, identity: Card) -> bool {
    identity.rank.number()
        == u8::try_from(view.play_stacks[identity.suit.index()].len())
            .expect("a standard stack has at most five cards")
            + 1
}

#[allow(clippy::too_many_arguments)]
fn snapshot_play_identities(
    identities: IdentitySet,
    giver: PlayerId,
    target: PlayerId,
    focus: CardId,
    view: &PlayerView,
    hands: &[Vec<CardId>],
    facts: &[ClueFacts],
    gotten: &CardSet,
    already_playing: &CardSet,
    stack_heights: [u8; 5],
) -> IdentitySet {
    let mask = identities
        .iter()
        .filter(|identity| {
            snapshot_playable(
                *identity,
                giver,
                target,
                focus,
                view,
                hands,
                facts,
                gotten,
                already_playing,
                stack_heights,
            )
        })
        .fold(0, |mask, identity| mask | (1 << identity.index()));
    IdentitySet::from_mask(mask)
}

#[allow(clippy::too_many_arguments)]
fn snapshot_playable(
    identity: Card,
    giver: PlayerId,
    target: PlayerId,
    focus: CardId,
    view: &PlayerView,
    hands: &[Vec<CardId>],
    facts: &[ClueFacts],
    gotten: &CardSet,
    already_playing: &CardSet,
    stack_heights: [u8; 5],
) -> bool {
    let height = usize::from(stack_heights[identity.suit.index()]);
    let rank = usize::from(identity.rank.number());
    if rank <= height {
        return false;
    }
    let accounted_after_first = ((height + 2)..rank).all(|needed_rank| {
        let needed = Card::new(identity.suit, Rank::ALL[needed_rank - 1]);
        snapshot_accounted(needed, focus, view, hands, facts, gotten)
    });
    if !accounted_after_first {
        return false;
    }
    if rank == height + 1 {
        return true;
    }
    let first = Card::new(identity.suit, Rank::ALL[height]);
    if snapshot_accounted(first, focus, view, hands, facts, gotten) {
        return true;
    }

    let actor_index = (giver.index() + 1) % hands.len();
    let prompt_candidates = hands[actor_index]
        .iter()
        .rev()
        .copied()
        .filter(|card| {
            *card != focus
                && gotten.contains(card)
                && !already_playing.contains(card)
                && facts[card.index()].allows(first)
        })
        .collect::<Vec<_>>();
    if !prompt_candidates.is_empty() {
        if actor_index == view.observer.index() {
            if giver != view.observer {
                return true;
            }
            return prompt_candidates
                .iter()
                .position(|card| {
                    IdentitySet::from_mask(facts[card.index()].identity_mask())
                        == IdentitySet::singleton(first)
                })
                .is_some_and(|correct| {
                    prompt_candidates[..correct].iter().all(|card| {
                        let possibilities =
                            IdentitySet::from_mask(facts[card.index()].identity_mask());
                        !possibilities.is_empty()
                            && possibilities
                                .iter()
                                .all(|identity| is_playable_at(stack_heights, identity))
                    })
                });
        }
        return prompt_candidates
            .iter()
            .position(|card| identity_of(view, *card) == Some(first))
            .is_some_and(|correct| {
                prompt_candidates[..correct].iter().all(|card| {
                    identity_of(view, *card).is_some_and(|candidate| {
                        candidate.rank.number() == stack_heights[candidate.suit.index()] + 1
                    })
                })
            });
    }
    if target.index() == actor_index {
        return false;
    }
    let finesse = hands[actor_index]
        .iter()
        .rev()
        .copied()
        .find(|card| !gotten.contains(card));
    finesse.is_some_and(|card| {
        if actor_index != view.observer.index() {
            identity_of(view, card) == Some(first)
        } else if giver != view.observer {
            true
        } else {
            IdentitySet::from_mask(facts[card.index()].identity_mask())
                == IdentitySet::singleton(first)
        }
    })
}

fn snapshot_accounted(
    identity: Card,
    excluded: CardId,
    view: &PlayerView,
    hands: &[Vec<CardId>],
    facts: &[ClueFacts],
    gotten: &CardSet,
) -> bool {
    hands.iter().flatten().copied().any(|card| {
        card != excluded
            && gotten.contains(&card)
            && if hands[view.observer.index()].contains(&card) {
                facts[card.index()].allows(identity)
            } else {
                identity_of(view, card) == Some(identity)
            }
    })
}

#[allow(clippy::too_many_arguments)]
fn snapshot_save_identities(
    identities: IdentitySet,
    clue: Clue,
    giver: PlayerId,
    focus: CardId,
    focus_was_chop: bool,
    eight_clue_save: bool,
    view: &PlayerView,
    hands: &[Vec<CardId>],
    gotten: &CardSet,
    _play_identities: IdentitySet,
    stack_heights: [u8; 5],
    discarded: [u8; 25],
) -> IdentitySet {
    if !focus_was_chop && !eight_clue_save {
        return IdentitySet::default();
    }
    let chops = hands
        .iter()
        .map(|hand| chop(hand, gotten))
        .collect::<Vec<_>>();
    let mask = identities
        .iter()
        .filter(|identity| {
            if eight_clue_save {
                return true;
            }
            match clue {
                Clue::Rank(Rank::Five) => identity.rank == Rank::Five,
                Clue::Rank(Rank::Two) if identity.rank == Rank::Two => {
                    identity.rank.number() > stack_heights[identity.suit.index()]
                        && snapshot_two_save_allowed(view, hands, giver, focus, *identity, &chops)
                }
                _ => {
                    identity.rank != Rank::Five
                    // A critical card on chop is a Save even when a delayed
                    // finesse line could eventually play it. Only an
                    // immediately playable focus takes Play precedence.
                    && !is_playable_at(stack_heights, *identity)
                    && discarded[identity.index()] + 1 == identity.rank.copies()
                    && !hands.iter().flatten().copied().any(|card| {
                        card != focus && identity_of(view, card) == Some(*identity)
                    })
                }
            }
        })
        .fold(0, |mask, identity| mask | (1 << identity.index()));
    IdentitySet::from_mask(mask)
}

fn snapshot_two_save_allowed(
    view: &PlayerView,
    hands: &[Vec<CardId>],
    giver: PlayerId,
    focus: CardId,
    identity: Card,
    chops: &[Option<CardId>],
) -> bool {
    let visible = hands
        .iter()
        .enumerate()
        .filter(|(player, _)| *player != giver.index())
        .flat_map(|(_, hand)| hand)
        .copied()
        .filter(|card| *card != focus && identity_of(view, *card) == Some(identity))
        .collect::<Vec<_>>();
    visible.is_empty() || visible.iter().all(|card| chops.contains(&Some(*card)))
}

#[allow(clippy::too_many_arguments)]
fn snapshot_good_touch_identities(
    card: CardId,
    identities: IdentitySet,
    view: &PlayerView,
    hands: &[Vec<CardId>],
    gotten: &CardSet,
    stack_heights: [u8; 5],
    discarded: [u8; 25],
) -> IdentitySet {
    let mask = identities
        .iter()
        .filter(|identity| {
            let rank = identity.rank.number();
            rank > stack_heights[identity.suit.index()]
                && Rank::ALL
                    .iter()
                    .copied()
                    .filter(|lower| {
                        lower.number() > stack_heights[identity.suit.index()]
                            && lower.number() < rank
                    })
                    .all(|lower| {
                        discarded[Card::new(identity.suit, lower).index()] < lower.copies()
                    })
                && !hands.iter().flatten().copied().any(|candidate| {
                    candidate != card
                        && gotten.contains(&candidate)
                        && identity_of(view, candidate) == Some(*identity)
                })
        })
        .fold(0, |mask, identity| mask | (1 << identity.index()));
    IdentitySet::from_mask(mask)
}

/// Mutable convention effects shared by the level rules for one event.
///
/// Keeping this façade separate from the public before/after turn context
/// makes rule inputs uniform and prevents giver-side and recipient-side rules
/// from rebuilding subtly different slices of replay state.
struct HGroupRuleEffects<'a> {
    explicitly_clued: &'a CardSet,
    invisibly_clued: &'a mut CardSet,
    clues: &'a [HGroupClueInterpretation],
    already_playing: &'a mut CardSet,
    pending: &'a mut Vec<ConnectionObligation>,
    chop_moved: &'a mut CardSet,
    must_clue: &'a mut PlayerSet,
    forced_playable: &'a mut CardSet,
    implicit_saves: &'a mut Vec<(CardId, IdentitySet)>,
    required_fix: &'a mut Option<RequiredFix>,
    signals: &'a mut Vec<HGroupSignal>,
}

#[allow(clippy::too_many_lines)]
fn replay_h_group(deductions: &LogicalDeductions, profile: HGroupProfile) -> HGroupState {
    replay_h_group_inner(deductions, profile, true)
}

#[allow(clippy::too_many_lines)]
fn replay_h_group_inner(
    deductions: &LogicalDeductions,
    profile: HGroupProfile,
    model_other_players: bool,
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
    let mut explicitly_clued = CardSet::default();
    let mut invisibly_clued = CardSet::default();
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
    let mut pending_connections = Vec::<ConnectionObligation>::new();
    let mut already_playing = CardSet::default();
    let mut early_game = true;
    let mut signals = Vec::new();
    let mut chop_moved = CardSet::default();
    let mut discard_now = Vec::new();
    let mut must_clue = PlayerSet::default();
    let mut forced_playable = CardSet::default();
    let mut invalidated_focuses = CardSet::default();
    let mut implicit_saves = Vec::new();
    let mut required_fix = None;
    let mut historical_clue_tokens = MAX_CLUE_TOKENS;

    for (entry_index, entry) in view.history.iter().enumerate() {
        let mut actor_saw_normal_discard = false;
        let before = HGroupTurnSnapshot::new(
            &hands,
            &facts,
            stack_heights,
            historical_clue_tokens,
            historical_deck_size,
            early_game,
        );
        let clue_tokens_before = before.clue_tokens;
        if rule_enabled(profile, HGroupRuleId::Bluffs) {
            let mut effects = HGroupRuleEffects {
                explicitly_clued: &explicitly_clued,
                invisibly_clued: &mut invisibly_clued,
                clues: &clues,
                already_playing: &mut already_playing,
                pending: &mut pending_connections,
                chop_moved: &mut chop_moved,
                must_clue: &mut must_clue,
                forced_playable: &mut forced_playable,
                implicit_saves: &mut implicit_saves,
                required_fix: &mut required_fix,
                signals: &mut signals,
            };
            apply_resolved_bluff_effects(entry, view, &before, &mut effects);
        }
        match &entry.event {
            ObservedEvent::Clued {
                giver,
                target,
                clue,
                touched,
                untouched,
            } => {
                let is_required_fix = required_fix.is_some_and(|required: RequiredFix| {
                    required.actor == *giver
                        && required.target == *target
                        && touched.contains(&required.focus)
                        && clue.matches(required.identity)
                        && !was_clued_before_with(view, entry.turn, required.focus, *clue)
                });
                let gotten = protected_cards(&explicitly_clued, &invisibly_clued, &chop_moved);
                let hand = &hands[target.index()];
                let old_chop = chop(hand, &gotten);
                let newly_touched = touched
                    .iter()
                    .copied()
                    .filter(|card| !gotten.contains(card))
                    .collect::<Vec<_>>();
                let previously_promptable = explicitly_clued
                    .union(&invisibly_clued)
                    .copied()
                    .collect::<CardSet>();
                let displaced_connections = pending_connections
                    .iter()
                    .filter(|connection| connection.actor == *giver)
                    .filter(|connection| {
                        touched
                            .iter()
                            .any(|card| identity_of(view, *card) == Some(connection.expected))
                    })
                    .flat_map(|connection| connection.cards.iter().copied())
                    .collect::<CardSet>();
                if !displaced_connections.is_empty() {
                    pending_connections.retain(|connection| {
                        connection.actor != *giver
                            || !touched
                                .iter()
                                .any(|card| identity_of(view, *card) == Some(connection.expected))
                    });
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
                    let focus_identity = identity_of(view, focus);
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
                    let mut focus_identities = focus_identity.map_or_else(
                        || IdentitySet::from_mask(facts[focus.index()].identity_mask()),
                        IdentitySet::singleton,
                    );
                    if focus_identity.is_none() {
                        // Good Touch lets a recipient eliminate identities
                        // already promised on live cards elsewhere. Apply the
                        // elimination to the whole focus domain, including
                        // Save possibilities: a newly touched 2 beside an
                        // existing saved Red 2 cannot itself be Red 2.
                        let live_cards = hands.iter().flatten().copied().collect::<CardSet>();
                        let fixed_cards = currently_fixed_cards(&signals);
                        let claimed = gotten
                            .iter()
                            .copied()
                            .filter(|card| {
                                *card != focus
                                    && live_cards.contains(card)
                                    && !fixed_cards.contains(card)
                            })
                            .fold(IdentitySet::default(), |claimed, card| {
                                let giver_holds_card = hands[giver.index()].contains(&card);
                                let identity = (!giver_holds_card)
                                    .then(|| identity_of(view, card))
                                    .flatten()
                                    .or_else(|| {
                                        let logical = IdentitySet::from_mask(
                                            facts[card.index()].identity_mask(),
                                        );
                                        (logical.len() == 1)
                                            .then(|| logical.iter().next())
                                            .flatten()
                                    })
                                    .or_else(|| {
                                        let prior =
                                            clues.iter().rev().find(|prior| prior.focus == card)?;
                                        (prior.play_identities.len() == 1)
                                            .then(|| prior.play_identities.iter().next())
                                            .flatten()
                                    });
                                identity.map_or(claimed, |identity| {
                                    claimed.union(IdentitySet::singleton(identity))
                                })
                            });
                        focus_identities = focus_identities.without(claimed);
                    }
                    let mut play_identities = snapshot_play_identities(
                        focus_identities,
                        *giver,
                        *target,
                        focus,
                        view,
                        &hands,
                        &facts,
                        &gotten,
                        &already_playing,
                        stack_heights,
                    );
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
                    // Number 2 and number 5 clues to an unclued chop 2/5 are
                    // Saves by definition, even when that identity happens to
                    // be playable. Critical Saves only override a delayed Play
                    // interpretation; an immediately playable critical card
                    // remains a Play Clue.
                    let save_precedence = IdentitySet::from_mask(
                        save_identities
                            .iter()
                            .filter(|identity| {
                                eight_clue_save
                                    || matches!(
                                        (*clue, identity.rank),
                                        (Clue::Rank(Rank::Two), Rank::Two)
                                            | (Clue::Rank(Rank::Five), Rank::Five)
                                    )
                                    || !is_playable_at(stack_heights, *identity)
                            })
                            .fold(0, |mask, identity| mask | (1 << identity.index())),
                    );
                    play_identities = play_identities.without(save_precedence);
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
                    let no_information_reclue = touched
                        .iter()
                        .all(|card| was_clued_before_with(view, entry.turn, *card, *clue));
                    let kind = if is_required_fix
                        || low_score_number_five
                        || early_five_stall
                        || eight_clue_five_stall
                        || no_information_reclue
                    {
                        HGroupClueKind::Unrecognized
                    } else if eight_clue_save && !save_identities.is_empty() {
                        HGroupClueKind::Save(HGroupSaveKind::EightClue)
                    } else {
                        clue_kind_from_masks(*clue, play_identities, save_identities)
                    };
                    if kind == HGroupClueKind::Unrecognized {
                        // A Fix or Stall still contributes its objective clue
                        // facts, but it makes no Play promise. Retaining the
                        // hypothetical play mask here caused an off-chop 5
                        // Stall to become an exact prompted 5 much later.
                        play_identities = IdentitySet::default();
                    }
                    let connection_identity = focus_identity.or_else(|| {
                        (play_identities.len() == 1)
                            .then(|| play_identities.iter().next())
                            .flatten()
                    });
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
                            let direct = identity_of(view, card).map_or_else(
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
                            )
                            .without(focus_identities);
                            (card, good_touch)
                        })
                        .collect();
                    clues.push(HGroupClueInterpretation {
                        turn: entry.turn,
                        giver: *giver,
                        target: *target,
                        clue: *clue,
                        stack_heights,
                        focus,
                        focus_was_chop,
                        kind,
                        focus_identities,
                        play_identities,
                        save_identities,
                        new_non_focus,
                        non_focus_identities,
                        // Prompt candidates need actual clue information.
                        // A chop-moved card is protected for chop/layout
                        // purposes, but remains an unknown card and cannot be
                        // Prompted merely because it was moved.
                        previously_gotten: previously_promptable.iter().copied().collect(),
                    });
                    let signal_kind = match kind {
                        HGroupClueKind::Play | HGroupClueKind::PlayOrSave => {
                            Some(HGroupMoveKind::PlayClue)
                        }
                        HGroupClueKind::Save(_) => Some(HGroupMoveKind::SaveClue),
                        HGroupClueKind::Unrecognized => None,
                    };
                    if let Some(signal_kind) = signal_kind {
                        signals.push(HGroupSignal {
                            turn: entry.turn,
                            actor: *giver,
                            target: Some(*target),
                            kind: signal_kind,
                            cards: vec![focus],
                            identity: focus_identity,
                        });
                    }
                    if matches!(kind, HGroupClueKind::Play) && !low_score_number_five {
                        let previous_pending = pending_connections.len();
                        schedule_connection(
                            profile,
                            view,
                            *giver,
                            *target,
                            focus,
                            touched,
                            connection_identity,
                            &hands,
                            &facts,
                            &clues,
                            &previously_promptable,
                            &already_playing,
                            &mut invisibly_clued,
                            stack_heights,
                            &mut pending_connections,
                        );
                        for connection in &pending_connections[previous_pending..] {
                            push_signal(
                                &mut signals,
                                entry,
                                *giver,
                                Some(connection.actor),
                                if connection.kind == HGroupConnectionKind::Finesse
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
                        }
                        already_playing.insert(focus);
                    }
                    if is_required_fix {
                        required_fix = None;
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
                let failed_connections = advance_pending_connections(
                    &mut pending_connections,
                    *player,
                    *card,
                    *identity,
                    *successful,
                );
                for focus in failed_connections {
                    already_playing.remove(&focus);
                    forced_playable.remove(&focus);
                    invalidated_focuses.insert(focus);
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
                    pending_connections.retain(|connection| connection.expected != *identity);
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
                let gotten = protected_cards(&explicitly_clued, &invisibly_clued, &chop_moved);
                actor_saw_normal_discard = chop(&hands[player.index()], &gotten) == Some(*card)
                    || (model_other_players
                        && *player != view.observer
                        && subjective_chop_before_action(
                            view,
                            profile,
                            *player,
                            &view.history[..entry_index],
                            &hands,
                            &facts,
                            historical_deck_size,
                        ) == Some(*card));
                if chop(&hands[player.index()], &gotten) == Some(*card) {
                    early_game = false;
                }
                for pending in pending_connections
                    .iter_mut()
                    .filter(|pending| pending.actor == *player)
                {
                    if pending.cards.first() == Some(card) {
                        pending.cards.clear();
                    } else {
                        pending.cards.retain(|candidate| candidate != card);
                    }
                }
                pending_connections
                    .retain(|pending| !pending.cards.is_empty() && pending.focus != *card);
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
            before,
            after: HGroupTurnView {
                hands: &hands,
                facts: &facts,
                stack_heights,
                clue_tokens: historical_clue_tokens,
                deck_size: historical_deck_size,
                early_game,
            },
            actor_saw_normal_discard,
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
            implicit_saves: &mut implicit_saves,
            required_fix: &mut required_fix,
            signals: &mut signals,
        };
        if rule_enabled(profile, HGroupRuleId::Priority)
            && h_group_phase_at(
                view.hands.len(),
                context.before.early_game,
                context.before.deck_size,
                context.before.stack_heights,
            ) != HGroupPhase::EndGame
        {
            apply_priority_effects(
                &context,
                view,
                effects.explicitly_clued,
                effects.forced_playable,
                effects.signals,
            );
        }

        if rule_enabled(profile, HGroupRuleId::BasicMoves) {
            apply_level_two_effects(
                context.entry,
                view,
                context.after.hands,
                effects.explicitly_clued,
                effects.signals,
            );
        }
        if rule_enabled(profile, HGroupRuleId::BasicStrategy) {
            apply_level_three_effects(&context, view, &mut effects);
        }
        if rule_enabled(profile, HGroupRuleId::Elimination) {
            apply_elimination_effects(
                context.entry,
                view,
                context.after.hands,
                context.after.stack_heights,
                effects.signals,
            );
        }
        if rule_enabled(profile, HGroupRuleId::ChopMoves)
            && (!rule_enabled(profile, HGroupRuleId::EndGame)
                || h_group_phase_at(
                    view.hands.len(),
                    context.after.early_game,
                    context.after.deck_size,
                    context.after.stack_heights,
                ) != HGroupPhase::EndGame)
        {
            apply_chop_move_effects(
                context.entry,
                view,
                context.after.hands,
                context.after.stack_heights,
                effects.explicitly_clued,
                effects.chop_moved,
                effects.signals,
            );
        }
        if rule_enabled(profile, HGroupRuleId::TempoClues) {
            apply_tempo_effects(
                context.entry,
                view,
                context.after.hands,
                effects.explicitly_clued,
                effects.chop_moved,
                effects.signals,
            );
        }
        if rule_enabled(profile, HGroupRuleId::EmergencyDiscards) {
            apply_emergency_discard_effects(&context, view, &mut effects);
        }
        if rule_enabled(profile, HGroupRuleId::EndGame) {
            apply_positional_effects(&context, view, &mut effects);
        }
        if rule_enabled(profile, HGroupRuleId::Stalling) {
            apply_stall_effects(context.entry, view, effects.signals);
        }
        if rule_enabled(profile, HGroupRuleId::SpecialDiscards) {
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
        if rule_enabled(profile, HGroupRuleId::Bluffs) {
            apply_bluff_effects(
                context.entry,
                view,
                context.after.hands,
                context.after.stack_heights,
                effects.explicitly_clued,
                effects.pending,
                effects.signals,
            );
        }
        if rule_enabled(profile, HGroupRuleId::Context) {
            apply_context_effects(
                context.entry,
                view,
                context.after.hands,
                effects.explicitly_clued,
                effects.signals,
            );
        }
        if rule_enabled(profile, HGroupRuleId::IntermediateBluffs) {
            apply_intermediate_bluff_effects(
                context.entry,
                view,
                context.after.hands,
                effects.signals,
            );
        }
        if rule_enabled(profile, HGroupRuleId::TrashMoves) {
            apply_trash_effects(
                context.entry,
                view,
                context.after.hands,
                context.after.stack_heights,
                effects.chop_moved,
                effects.pending,
                effects.signals,
            );
        }
        if rule_enabled(profile, HGroupRuleId::DoubleBluffs) {
            apply_double_bluff_effects(context.entry, view, context.after.hands, effects.signals);
        }
        if rule_enabled(profile, HGroupRuleId::EjectionsAndDischarges) {
            apply_ejection_discharge_effects(
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
                effects.signals,
            );
        }
        if rule_enabled(profile, HGroupRuleId::Duplication) {
            apply_duplication_effects(
                context.entry,
                view,
                effects.explicitly_clued,
                effects.signals,
            );
        }
        if rule_enabled(profile, HGroupRuleId::FiveTech) {
            apply_five_tech_effects(
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
            );
        }
        if rule_enabled(profile, HGroupRuleId::OutOfOrderPlay) {
            apply_out_of_order_effects(
                context.entry,
                view,
                context.after.hands,
                effects.clues,
                context.after.stack_heights,
                effects.pending,
                effects.forced_playable,
                effects.required_fix,
                effects.signals,
            );
        }
        if rule_enabled(profile, HGroupRuleId::Ignition) {
            apply_ignition_effects(
                context.entry,
                view,
                context.after.stack_heights,
                effects.forced_playable,
                effects.signals,
            );
        }
        if rule_enabled(profile, HGroupRuleId::PhantomPlayable) {
            apply_phantom_effects(
                context.entry,
                view,
                context.after.hands,
                effects.forced_playable,
                effects.signals,
            );
        }
        if rule_enabled(profile, HGroupRuleId::Charms) {
            apply_charm_effects(
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
            );
        }
        if rule_enabled(profile, HGroupRuleId::UnnecessaryMoves) {
            apply_unnecessary_move_effects(&context, view, effects.signals);
        }
        if rule_enabled(profile, HGroupRuleId::Extras) {
            apply_extra_effects(
                context.entry,
                view,
                context.after.hands,
                effects.explicitly_clued,
                effects.pending,
                effects.chop_moved,
                effects.must_clue,
                effects.forced_playable,
                effects.signals,
            );
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
            if duplicated_in_own_hand && !bluff {
                discard_now.push(*blind);
            }
        }
    }
    HGroupState {
        hands,
        explicitly_clued,
        invisibly_clued,
        clues,
        pending_connections,
        already_playing,
        early_game,
        signals,
        chop_moved,
        discard_now,
        must_clue,
        forced_playable,
        invalidated_focuses,
        implicit_saves,
        required_fix,
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
    signals: &mut Vec<HGroupSignal>,
    entry: &ObservedHistoryEntry,
    actor: PlayerId,
    target: Option<PlayerId>,
    kind: HGroupMoveKind,
    cards: Vec<CardId>,
    identity: Option<Card>,
) {
    if signals.iter().any(|signal| {
        signal.turn == entry.turn
            && signal.actor == actor
            && signal.kind == kind
            && signal.cards == cards
    }) {
        return;
    }
    signals.push(HGroupSignal {
        turn: entry.turn,
        actor,
        target,
        kind,
        cards,
        identity,
    });
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

fn current_card_identity(view: &PlayerView, card: CardId) -> Option<Card> {
    identity_of(view, card)
}

fn card_is_trash(view: &PlayerView, identity: Card) -> bool {
    usize::from(identity.rank.number()) <= view.play_stacks[identity.suit.index()].len()
}

fn visible_playable_in_hand(
    view: &PlayerView,
    player: PlayerId,
    excluded: Option<CardId>,
) -> Option<(CardId, Card)> {
    view.hands[player.index()].iter().rev().find_map(|card| {
        (Some(card.id) != excluded)
            .then_some(card.identity)
            .flatten()
            .filter(|identity| is_playable_now(view, *identity))
            .map(|identity| (card.id, identity))
    })
}

fn is_playable_at(stack_heights: [u8; 5], identity: Card) -> bool {
    identity.rank.number() == stack_heights[identity.suit.index()] + 1
}

fn is_trash_at(stack_heights: [u8; 5], identity: Card) -> bool {
    identity.rank.number() <= stack_heights[identity.suit.index()]
}

#[allow(clippy::too_many_arguments)]
fn has_higher_basic_priority(
    view: &PlayerView,
    hands: &[Vec<CardId>],
    facts: &[ClueFacts],
    explicitly_clued: &CardSet,
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
                .any(|other| {
                    explicitly_clued.contains(other)
                        && current_card_identity(view, *other) == Some(next)
                })
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

fn apply_level_two_effects(
    entry: &ObservedHistoryEntry,
    view: &PlayerView,
    hands: &[Vec<CardId>],
    explicitly_clued: &CardSet,
    signals: &mut Vec<HGroupSignal>,
) {
    let ObservedEvent::Clued {
        giver,
        target,
        clue,
        touched,
        ..
    } = &entry.event
    else {
        return;
    };
    if *clue == Clue::Rank(Rank::Five)
        && touched.iter().any(|card| {
            current_card_identity(view, *card).is_some_and(|identity| identity.rank == Rank::Five)
        })
        && hands[target.index()]
            .first()
            .is_none_or(|chop| !touched.contains(chop))
    {
        push_signal(
            signals,
            entry,
            *giver,
            Some(*target),
            HGroupMoveKind::FiveStall,
            touched.clone(),
            None,
        );
    }

    // A clue to one's own next connecting card is the Self-Finesse form. A
    // connection that has to pass the target before resolving is the Reverse
    // form. Multi-card Prompts/Finesses are represented by repeated primitive
    // connection signals in resolution order.
    let delayed_focus = touched.last().copied().and_then(|focus| {
        current_card_identity(view, focus)
            .filter(|identity| !is_playable_now(view, *identity))
            .map(|identity| (focus, identity))
    });
    if let Some((focus, identity)) = delayed_focus {
        let actor = next_player(*giver, hands.len());
        let kind = if actor == *target {
            HGroupMoveKind::SelfFinesse
        } else if target.index() < actor.index() {
            HGroupMoveKind::ReverseFinesse
        } else if explicitly_clued.contains(&focus) {
            HGroupMoveKind::Prompt
        } else {
            HGroupMoveKind::Finesse
        };
        push_signal(
            signals,
            entry,
            *giver,
            Some(*target),
            kind,
            vec![focus],
            Some(identity),
        );
    }
}

fn apply_level_three_effects(
    context: &HGroupTurnContext<'_>,
    view: &PlayerView,
    effects: &mut HGroupRuleEffects<'_>,
) {
    if apply_repeated_one_fix(context, view, effects) {
        return;
    }
    if !apply_fill_in_fix(context, view, effects) {
        apply_sarcastic_discard(context.entry, view, effects.signals);
    }
}

fn apply_repeated_one_fix(
    context: &HGroupTurnContext<'_>,
    view: &PlayerView,
    effects: &mut HGroupRuleEffects<'_>,
) -> bool {
    let entry = context.entry;
    let hands = context.after.hands;
    let explicitly_clued = effects.explicitly_clued;
    let ObservedEvent::Clued {
        giver,
        target,
        clue: Clue::Rank(Rank::One),
        touched,
        ..
    } = &entry.event
    else {
        return false;
    };
    if touched.is_empty()
        || !touched.iter().all(|card| {
            view.history
                .iter()
                .take_while(|prior| prior.turn < entry.turn)
                .any(|prior| {
                    matches!(
                        &prior.event,
                        ObservedEvent::Clued {
                            clue: Clue::Rank(Rank::One),
                            touched,
                            ..
                        } if touched.contains(card)
                    )
                })
        })
    {
        return false;
    }
    let Some(fixed) = focus(
        &hands[target.index()],
        touched,
        chop(&hands[target.index()], explicitly_clued),
        explicitly_clued,
    ) else {
        return false;
    };
    let canceled_cards = effects
        .pending
        .iter()
        .filter(|connection| connection.focus == fixed)
        .flat_map(|connection| connection.cards.iter().copied())
        .collect::<CardSet>();
    effects.already_playing.remove(&fixed);
    effects
        .pending
        .retain(|connection| connection.focus != fixed);
    for card in canceled_cards {
        if !explicitly_clued.contains(&card)
            && !effects
                .pending
                .iter()
                .any(|connection| connection.cards.contains(&card))
        {
            effects.invisibly_clued.remove(&card);
        }
    }
    effects.forced_playable.remove(&fixed);
    push_signal(
        effects.signals,
        entry,
        *giver,
        Some(*target),
        HGroupMoveKind::FixClue,
        vec![fixed],
        None,
    );
    true
}

fn apply_fill_in_fix(
    context: &HGroupTurnContext<'_>,
    view: &PlayerView,
    effects: &mut HGroupRuleEffects<'_>,
) -> bool {
    let entry = context.entry;
    let ObservedEvent::Clued {
        giver,
        target,
        clue,
        touched,
        ..
    } = &entry.event
    else {
        return false;
    };
    if touched.is_empty()
        || !touched
            .iter()
            .all(|card| was_clued_before(view, entry.turn, *card))
    {
        return false;
    }
    let fills_in = touched.iter().any(|card| {
        !view
            .history
            .iter()
            .take_while(|prior| prior.turn < entry.turn)
            .any(
                        |prior| matches!(&prior.event, ObservedEvent::Clued { clue: prior_clue, touched, .. } if prior_clue == clue && touched.contains(card)),
            )
    });
    let facts = context.after.facts;
    let identities = touched
        .iter()
        .filter_map(|card| {
            let identities = IdentitySet::from_mask(facts[card.index()].identity_mask());
            (identities.len() == 1)
                .then(|| identities.iter().next())
                .flatten()
        })
        .collect::<Vec<_>>();
    let duplicate = identities.len() == touched.len()
        && identity_set(identities.iter().copied()).len() < identities.len();
    let stops_existing = touched.iter().any(|card| {
        effects.already_playing.contains(card) && {
            let identities = IdentitySet::from_mask(facts[card.index()].identity_mask());
            !identities.is_empty()
                && identities
                    .iter()
                    .all(|identity| !is_playable_at(context.after.stack_heights, identity))
                && !effects.pending.iter().any(|connection| {
                    connection.focus == *card && pending_is_active(connection, effects.pending)
                })
        }
    });
    if !fills_in || (!duplicate && !stops_existing) {
        return false;
    }
    let canceled_cards = effects
        .pending
        .iter()
        .filter(|connection| touched.contains(&connection.focus))
        .filter(|connection| connection.kind == HGroupConnectionKind::Finesse)
        .flat_map(|connection| connection.cards.iter().copied())
        .collect::<CardSet>();
    effects
        .already_playing
        .retain(|card| !touched.contains(card));
    effects
        .pending
        .retain(|connection| !touched.contains(&connection.focus));
    for card in canceled_cards {
        if !effects.explicitly_clued.contains(&card)
            && !effects
                .pending
                .iter()
                .any(|connection| connection.cards.contains(&card))
        {
            effects.invisibly_clued.remove(&card);
        }
    }
    effects
        .forced_playable
        .retain(|card| !touched.contains(card));
    push_signal(
        effects.signals,
        entry,
        *giver,
        Some(*target),
        HGroupMoveKind::FixClue,
        touched.clone(),
        None,
    );
    true
}

fn apply_sarcastic_discard(
    entry: &ObservedHistoryEntry,
    view: &PlayerView,
    signals: &mut Vec<HGroupSignal>,
) {
    let ObservedEvent::Discarded {
        player,
        card,
        identity,
    } = entry.event
    else {
        return;
    };
    if was_clued_before(view, entry.turn, card)
        && view
            .hands
            .iter()
            .flatten()
            .any(|candidate| candidate.id != card && candidate.identity == Some(identity))
    {
        push_signal(
            signals,
            entry,
            player,
            None,
            HGroupMoveKind::SarcasticDiscard,
            vec![card],
            Some(identity),
        );
    }
}

fn apply_chop_move_effects(
    entry: &ObservedHistoryEntry,
    view: &PlayerView,
    hands: &[Vec<CardId>],
    stack_heights: [u8; 5],
    explicitly_clued: &CardSet,
    chop_moved: &mut CardSet,
    signals: &mut Vec<HGroupSignal>,
) {
    if signals
        .iter()
        .any(|signal| signal.turn == entry.turn && signal.kind == HGroupMoveKind::FiveStall)
    {
        return;
    }
    let ObservedEvent::Clued {
        giver,
        target,
        clue,
        touched,
        ..
    } = &entry.event
    else {
        return;
    };
    let hand = &hands[target.index()];
    let all_trash = !touched.is_empty()
        && touched.iter().all(|card| {
            current_card_identity(view, *card).is_some_and(|identity| {
                identity.rank.number() <= stack_heights[identity.suit.index()]
            })
        });
    let five_chop_move = *clue == Clue::Rank(Rank::Five)
        && touched.iter().any(|card| {
            current_card_identity(view, *card).is_some_and(|identity| identity.rank == Rank::Five)
        });
    if !all_trash && !five_chop_move {
        return;
    }
    let boundary = touched
        .iter()
        .filter_map(|card| hand.iter().position(|candidate| candidate == card))
        .min();
    let Some(boundary) = boundary else {
        return;
    };
    let count = if five_chop_move { 1 } else { boundary };
    let moved = hand[..boundary]
        .iter()
        .rev()
        .filter(|card| !explicitly_clued.contains(card) && !chop_moved.contains(card))
        .take(count.max(1))
        .copied()
        .collect::<Vec<_>>();
    if moved.is_empty() {
        return;
    }
    chop_moved.extend(moved.iter().copied());
    push_signal(
        signals,
        entry,
        *giver,
        Some(*target),
        HGroupMoveKind::ChopMove,
        moved,
        None,
    );
}

fn apply_tempo_effects(
    entry: &ObservedHistoryEntry,
    view: &PlayerView,
    hands: &[Vec<CardId>],
    explicitly_clued: &CardSet,
    chop_moved: &mut CardSet,
    signals: &mut Vec<HGroupSignal>,
) {
    if signals.iter().any(|signal| {
        signal.turn == entry.turn
            && matches!(
                signal.kind,
                HGroupMoveKind::FiveStall | HGroupMoveKind::FixClue | HGroupMoveKind::Elimination
            )
    }) {
        return;
    }
    let ObservedEvent::Clued {
        giver,
        target,
        touched,
        ..
    } = &entry.event
    else {
        return;
    };
    if touched.is_empty()
        || !touched
            .iter()
            .all(|card| was_clued_before(view, entry.turn, *card) || chop_moved.contains(card))
    {
        return;
    }
    push_signal(
        signals,
        entry,
        *giver,
        Some(*target),
        HGroupMoveKind::TempoClue,
        touched.clone(),
        None,
    );
    let playable_count = touched
        .iter()
        .filter(|card| {
            current_card_identity(view, **card)
                .is_some_and(|identity| is_playable_now(view, identity))
        })
        .count();
    if playable_count < 2 {
        if let Some(card) = chop(&hands[target.index()], explicitly_clued) {
            chop_moved.insert(card);
            push_signal(
                signals,
                entry,
                *giver,
                Some(*target),
                HGroupMoveKind::ChopMove,
                vec![card],
                None,
            );
        }
    }
}

fn apply_emergency_discard_effects(
    context: &HGroupTurnContext<'_>,
    view: &PlayerView,
    effects: &mut HGroupRuleEffects<'_>,
) {
    let entry = context.entry;
    let ObservedEvent::Discarded { player, card, .. } = &entry.event else {
        return;
    };
    if context.actor_saw_normal_discard {
        return;
    }
    let hands = context.after.hands;
    let facts = context.after.facts;
    let stack_heights = context.after.stack_heights;
    let explicitly_clued = effects.explicitly_clued;
    let pending_connections = &*effects.pending;
    // Emergency-discard interpretation must be reproducible from public
    // knowledge available to the discarder. Looking at convention identities
    // recovered from visible simulator truth made other players see a Scream
    // Discard that the discarder themselves could not know they performed.
    let known_playable = hands[player.index()].iter().any(|card| {
        pending_connections.iter().any(|connection| {
            connection.actor == *player
                && connection.cards.contains(card)
                && pending_is_active(connection, pending_connections)
        }) || {
            let identities = IdentitySet::from_mask(facts[card.index()].identity_mask());
            let live_identities = identities
                .iter()
                .filter(|identity| !is_trash_at(stack_heights, *identity))
                .collect::<Vec<_>>();
            let has_useful_touch = explicitly_clued.contains(card)
                && !effects.signals.iter().any(|signal| {
                    signal.turn < entry.turn
                        && matches!(
                            signal.kind,
                            HGroupMoveKind::TrashPush | HGroupMoveKind::Discharge
                        )
                        && signal.cards.contains(card)
                });
            (identities.len() == 1
                && identities
                    .iter()
                    .next()
                    .is_some_and(|identity| is_playable_at(stack_heights, identity)))
                || (has_useful_touch
                    && !live_identities.is_empty()
                    && live_identities
                        .iter()
                        .all(|identity| is_playable_at(stack_heights, *identity)))
        }
    });
    let discarded_possibilities = IdentitySet::from_mask(facts[card.index()].identity_mask());
    let known_trash = was_clued_before(view, entry.turn, *card)
        && !discarded_possibilities.is_empty()
        && discarded_possibilities
            .iter()
            .all(|identity| identity.rank.number() <= stack_heights[identity.suit.index()]);
    if !known_playable && !known_trash {
        return;
    }
    let target = next_player(*player, hands.len());
    if let Some(target_chop) = chop(&hands[target.index()], explicitly_clued) {
        effects.chop_moved.insert(target_chop);
        effects.must_clue.insert(target);
        push_signal(
            effects.signals,
            entry,
            *player,
            Some(target),
            HGroupMoveKind::EmergencyDiscard,
            vec![target_chop],
            current_card_identity(view, target_chop),
        );
    }
}

#[allow(clippy::too_many_arguments)]
fn apply_positional_effects(
    context: &HGroupTurnContext<'_>,
    view: &PlayerView,
    effects: &mut HGroupRuleEffects<'_>,
) {
    let entry = context.entry;
    let hands = context.after.hands;
    let historical_deck_size = context.after.deck_size;
    let stack_heights = context.after.stack_heights;
    let explicitly_clued = effects.explicitly_clued;
    let invisibly_clued = &*effects.invisibly_clued;
    let chop_moved = &*effects.chop_moved;
    let actor_saw_normal_discard = context.actor_saw_normal_discard;
    let pending = &mut *effects.pending;
    let forced_playable = &mut *effects.forced_playable;
    let signals = &mut *effects.signals;
    let (player, card, is_misplay) = match &entry.event {
        ObservedEvent::Discarded { player, card, .. } => (*player, *card, false),
        ObservedEvent::Played {
            player,
            card,
            successful: false,
            ..
        } => (*player, *card, true),
        _ => return,
    };
    if historical_deck_size > view.hands.len()
        || was_clued_before(view, entry.turn, card)
        || invisibly_clued.contains(&card)
        || chop_moved.contains(&card)
        || actor_saw_normal_discard
    {
        // A Positional Discard has to be an otherwise-unexplained discard
        // from an ordinary unknown slot. Discarding a directly clued,
        // conventionally clued, or formerly chop-moved card already has a
        // natural interpretation (most commonly, disposing of known trash).
        // It therefore cannot also promise a matching-slot blind play.
        return;
    }
    if !is_misplay {
        let mut pre_hand = hands[player.index()].clone();
        let position = pre_hand
            .iter()
            .filter(|candidate| candidate.index() < card.index())
            .count();
        pre_hand.insert(position, card);
        let gotten = protected_cards(explicitly_clued, invisibly_clued, chop_moved);
        if chop(&pre_hand, &gotten) == Some(card) {
            // An expected chop discard is ordinary; it cannot simultaneously
            // communicate a Positional Discard to the matching slot.
            return;
        }
    }
    let indicated_slot = hands[player.index()]
        .iter()
        .filter(|candidate| candidate.index() < card.index())
        .count();
    let visible_target = (1..hands.len())
        .filter_map(|distance| {
            let index = (player.index() + distance) % hands.len();
            let target = PlayerId::new(u8::try_from(index).ok()?);
            let card = hands[index].get(indicated_slot).copied()?;
            let playable = current_card_identity(view, card)
                .is_some_and(|identity| is_playable_at(stack_heights, identity));
            playable.then_some((target, card))
        })
        .next_back();
    // If another player's matching card is visibly playable, that public
    // recipient resolves the positional message. An observer cannot promote
    // their own hidden matching slot past that known target merely because it
    // might also be playable. Only infer the hidden observer as the target
    // when no visible matching play exists.
    let hidden_observer = hands[view.observer.index()]
        .get(indicated_slot)
        .copied()
        .map(|card| (view.observer, card));
    let target_and_card = visible_target.or(hidden_observer);
    let Some((target, indicated)) = target_and_card else {
        return;
    };
    forced_playable.insert(indicated);
    if let Some(identity) = current_card_identity(view, indicated) {
        pending.push(ConnectionObligation {
            actor: target,
            cards: vec![indicated],
            expected: identity,
            kind: HGroupConnectionKind::Finesse,
            focus: indicated,
            step: 0,
        });
    }
    push_signal(
        signals,
        entry,
        player,
        Some(target),
        HGroupMoveKind::PositionalDiscard,
        vec![indicated],
        current_card_identity(view, indicated),
    );
    if is_misplay {
        // A positional misplay carries the same play indication; strike count
        // is already represented by the authoritative game state.
    }
}

fn apply_stall_effects(
    entry: &ObservedHistoryEntry,
    view: &PlayerView,
    signals: &mut Vec<HGroupSignal>,
) {
    let ObservedEvent::Clued {
        giver,
        target,
        touched,
        ..
    } = &entry.event
    else {
        return;
    };
    if touched.is_empty()
        || !touched
            .iter()
            .all(|card| was_clued_before(view, entry.turn, *card))
    {
        return;
    }
    push_signal(
        signals,
        entry,
        *giver,
        Some(*target),
        HGroupMoveKind::Stall,
        touched.clone(),
        None,
    );
}

fn apply_context_effects(
    entry: &ObservedHistoryEntry,
    view: &PlayerView,
    hands: &[Vec<CardId>],
    explicitly_clued: &CardSet,
    signals: &mut Vec<HGroupSignal>,
) {
    let ObservedEvent::Clued {
        giver,
        target,
        touched,
        ..
    } = &entry.event
    else {
        return;
    };
    let Some(focus) = touched.last().copied() else {
        return;
    };
    let Some(identity) = current_card_identity(view, focus) else {
        return;
    };
    let height = view.play_stacks[identity.suit.index()].len();
    if usize::from(identity.rank.number()) <= height + 1 {
        return;
    }
    let connector = Card::new(identity.suit, Rank::ALL[height]);
    let selfish = hands[giver.index()].iter().any(|card| {
        explicitly_clued.contains(card) && current_card_identity(view, *card) == Some(connector)
    });
    push_signal(
        signals,
        entry,
        *giver,
        Some(*target),
        if selfish {
            HGroupMoveKind::SelfishClue
        } else {
            HGroupMoveKind::Context
        },
        vec![focus],
        Some(identity),
    );
}

fn apply_intermediate_bluff_effects(
    entry: &ObservedHistoryEntry,
    view: &PlayerView,
    hands: &[Vec<CardId>],
    signals: &mut Vec<HGroupSignal>,
) {
    let ObservedEvent::Clued {
        giver,
        target,
        clue,
        touched,
        ..
    } = &entry.event
    else {
        return;
    };
    let actor = next_player(*giver, hands.len());
    // A Bluff asks the player immediately after the giver to blind-play for a
    // clue given to somebody later in turn order. A clue given directly to
    // that next player is an ordinary Play Clue, never a Bluff.
    if actor == *target {
        return;
    }
    let specialized = *clue == Clue::Rank(Rank::Three)
        || touched.iter().any(|card| {
            current_card_identity(view, *card).is_some_and(|identity| identity.rank.number() >= 3)
        });
    if specialized && visible_playable_in_hand(view, actor, None).is_some() {
        push_signal(
            signals,
            entry,
            *giver,
            Some(*target),
            HGroupMoveKind::Bluff,
            touched.clone(),
            None,
        );
    }
}

fn apply_double_bluff_effects(
    entry: &ObservedHistoryEntry,
    view: &PlayerView,
    hands: &[Vec<CardId>],
    signals: &mut Vec<HGroupSignal>,
) {
    let ObservedEvent::Clued {
        giver,
        target,
        touched,
        ..
    } = &entry.event
    else {
        return;
    };
    let Some(identity) = touched
        .last()
        .and_then(|card| current_card_identity(view, *card))
    else {
        return;
    };
    let distance = usize::from(identity.rank.number())
        .saturating_sub(view.play_stacks[identity.suit.index()].len() + 1);
    if distance < 2 {
        return;
    }
    let first = next_player(*giver, hands.len());
    let second = next_player(first, hands.len());
    if visible_playable_in_hand(view, first, None).is_some()
        && visible_playable_in_hand(view, second, None).is_some()
    {
        push_signal(
            signals,
            entry,
            *giver,
            Some(*target),
            HGroupMoveKind::DoubleBluff,
            touched.clone(),
            Some(identity),
        );
    }
}

fn apply_duplication_effects(
    entry: &ObservedHistoryEntry,
    view: &PlayerView,
    explicitly_clued: &CardSet,
    signals: &mut Vec<HGroupSignal>,
) {
    let ObservedEvent::Clued {
        giver,
        target,
        touched,
        ..
    } = &entry.event
    else {
        return;
    };
    let duplicated = touched.iter().find_map(|card| {
        let identity = current_card_identity(view, *card)?;
        view.hands
            .iter()
            .flatten()
            .any(|other| {
                other.id != *card
                    && explicitly_clued.contains(&other.id)
                    && other.identity == Some(identity)
            })
            .then_some((*card, identity))
    });
    if let Some((card, identity)) = duplicated {
        push_signal(
            signals,
            entry,
            *giver,
            Some(*target),
            HGroupMoveKind::Duplication,
            vec![card],
            Some(identity),
        );
    }
}

#[allow(clippy::too_many_arguments)]
fn apply_transfer_effects(
    entry: &ObservedHistoryEntry,
    view: &PlayerView,
    hands: &[Vec<CardId>],
    explicitly_clued: &CardSet,
    invisibly_clued: &mut CardSet,
    already_playing: &mut CardSet,
    pending: &mut Vec<ConnectionObligation>,
    signals: &mut Vec<HGroupSignal>,
) {
    let ObservedEvent::Discarded {
        player,
        card,
        identity,
    } = &entry.event
    else {
        return;
    };
    if !was_clued_before(view, entry.turn, *card) {
        return;
    }
    let mut transfer = None;
    let mut kind = HGroupMoveKind::TransferDiscard;
    for distance in 1..hands.len() {
        let index = (player.index() + distance) % hands.len();
        if let Some(target_card) = hands[index].iter().rev().copied().find(|candidate| {
            explicitly_clued.contains(candidate)
                && current_card_identity(view, *candidate) == Some(*identity)
        }) {
            transfer = Some((PlayerId::new(u8::try_from(index).unwrap_or(0)), target_card));
            kind = HGroupMoveKind::SarcasticDiscard;
            break;
        }
    }
    if transfer.is_none() {
        for distance in 1..hands.len() {
            let index = (player.index() + distance) % hands.len();
            let finesse_position = hands[index].iter().rev().copied().find(|candidate| {
                !explicitly_clued.contains(candidate) && !invisibly_clued.contains(candidate)
            });
            if let Some(target_card) = finesse_position
                .filter(|candidate| current_card_identity(view, *candidate) == Some(*identity))
            {
                transfer = Some((PlayerId::new(u8::try_from(index).unwrap_or(0)), target_card));
                break;
            }
        }
    }
    let Some((target, target_card)) = transfer else {
        return;
    };
    invisibly_clued.insert(target_card);
    if is_playable_now(view, *identity) {
        already_playing.insert(target_card);
        pending.push(ConnectionObligation {
            actor: target,
            cards: vec![target_card],
            expected: *identity,
            kind: HGroupConnectionKind::Finesse,
            focus: target_card,
            step: 0,
        });
    }
    push_signal(
        signals,
        entry,
        *player,
        Some(target),
        kind,
        vec![target_card],
        Some(*identity),
    );
}

fn apply_bluff_effects(
    entry: &ObservedHistoryEntry,
    view: &PlayerView,
    hands: &[Vec<CardId>],
    stack_heights: [u8; 5],
    explicitly_clued: &CardSet,
    pending: &mut Vec<ConnectionObligation>,
    signals: &mut Vec<HGroupSignal>,
) {
    let ObservedEvent::Clued {
        giver,
        target,
        touched,
        ..
    } = &entry.event
    else {
        return;
    };
    let Some(focus) = touched.last().copied() else {
        return;
    };
    let Some(focus_identity) = current_card_identity(view, focus) else {
        return;
    };
    let height = stack_heights[focus_identity.suit.index()];
    let focus_is_one_away = focus_identity.rank.number() == height + 2;
    if is_playable_at(stack_heights, focus_identity) || !focus_is_one_away {
        return;
    }
    let actor = next_player(*giver, hands.len());
    if actor == *target {
        return;
    }
    let actor_is_loaded = pending.iter().any(|connection| {
        connection.actor == actor
            && connection.focus != focus
            && pending_is_active(connection, pending)
    }) || hands[actor.index()].iter().any(|card| {
        explicitly_clued.contains(card)
            && current_card_identity(view, *card)
                .is_some_and(|identity| is_playable_at(stack_heights, identity))
    });
    if actor_is_loaded {
        return;
    }
    let Some((bluff_card, bluff_identity)) = hands[actor.index()]
        .iter()
        .rev()
        .copied()
        .filter(|card| Some(*card) != Some(focus))
        .find_map(|card| {
            current_card_identity(view, card)
                .filter(|identity| is_playable_at(stack_heights, *identity))
                .map(|identity| (card, identity))
        })
    else {
        return;
    };
    let stack_height = usize::from(height);
    if stack_height == Rank::ALL.len() {
        return;
    }
    let expected_connector = Card::new(focus_identity.suit, Rank::ALL[stack_height]);
    if bluff_identity == expected_connector {
        return;
    }
    pending.push(ConnectionObligation {
        actor,
        cards: vec![bluff_card],
        expected: bluff_identity,
        kind: HGroupConnectionKind::Finesse,
        focus,
        step: 0,
    });
    push_signal(
        signals,
        entry,
        *giver,
        Some(actor),
        HGroupMoveKind::Bluff,
        vec![bluff_card, focus],
        Some(bluff_identity),
    );
}

fn apply_resolved_bluff_effects(
    entry: &ObservedHistoryEntry,
    view: &PlayerView,
    before: &HGroupTurnSnapshot,
    effects: &mut HGroupRuleEffects<'_>,
) {
    let clues = effects.clues;
    let facts = &before.facts;
    let stack_heights = before.stack_heights;
    let already_playing = &mut *effects.already_playing;
    let pending = &mut *effects.pending;
    let signals = &mut *effects.signals;
    let ObservedEvent::Played {
        player,
        card,
        identity,
        successful: true,
    } = entry.event
    else {
        return;
    };
    if was_clued_before(view, entry.turn, card) {
        return;
    }
    let Some(clue) = clues.iter().rev().find(|clue| {
        clue.turn + 1 == entry.turn
            && player == next_player(clue.giver, view.hands.len())
            && matches!(clue.kind, HGroupClueKind::Play | HGroupClueKind::PlayOrSave)
    }) else {
        return;
    };
    let connects = match clue.clue {
        Clue::Suit(suit) => identity.suit == suit,
        Clue::Rank(rank) => identity.rank.number().saturating_add(1) == rank.number(),
    };
    let legal_bluff_target = IdentitySet::all().iter().any(|candidate| {
        clue.clue.matches(candidate)
            && facts[clue.focus.index()].allows(candidate)
            && candidate.rank.number() == stack_heights[candidate.suit.index()].saturating_add(2)
    });
    if connects || !legal_bluff_target {
        return;
    }

    pending.retain(|connection| connection.focus != clue.focus);
    already_playing.remove(&clue.focus);
    push_signal(
        signals,
        entry,
        clue.giver,
        Some(player),
        HGroupMoveKind::Bluff,
        vec![card, clue.focus],
        Some(identity),
    );
}

fn apply_trash_effects(
    entry: &ObservedHistoryEntry,
    view: &PlayerView,
    hands: &[Vec<CardId>],
    stack_heights: [u8; 5],
    chop_moved: &mut CardSet,
    pending: &mut Vec<ConnectionObligation>,
    signals: &mut Vec<HGroupSignal>,
) {
    match &entry.event {
        ObservedEvent::Clued {
            giver,
            target,
            touched,
            ..
        } if !touched.is_empty()
            && touched.iter().all(|card| {
                current_card_identity(view, *card)
                    .is_some_and(|identity| is_trash_at(stack_heights, identity))
            }) =>
        {
            let hand = &hands[target.index()];
            let focus = touched
                .iter()
                .filter_map(|card| {
                    hand.iter()
                        .position(|candidate| candidate == card)
                        .map(|p| (p, *card))
                })
                .max_by_key(|(position, _)| *position)
                .map(|(_, card)| card);
            if let Some(focus) = focus {
                push_signal(
                    signals,
                    entry,
                    *giver,
                    Some(*target),
                    HGroupMoveKind::TrashPush,
                    vec![focus],
                    current_card_identity(view, focus),
                );
                if let Some(position) = hand.iter().position(|card| *card == focus) {
                    if let Some(pushed) = hand.get(position + 1).copied() {
                        chop_moved.insert(pushed);
                    }
                }
            }
        }
        ObservedEvent::Discarded {
            player,
            card,
            identity,
        } if is_trash_at(stack_heights, *identity)
            && !was_clued_before(view, entry.turn, *card) =>
        {
            let target = next_player(*player, hands.len());
            let playable_finesse = hands[target.index()].last().copied().and_then(|finesse| {
                current_card_identity(view, finesse)
                    .filter(|expected| is_playable_now(view, *expected))
                    .map(|expected| (finesse, expected))
            });
            if let Some((finesse, expected)) = playable_finesse {
                pending.push(ConnectionObligation {
                    actor: target,
                    cards: vec![finesse],
                    expected,
                    kind: HGroupConnectionKind::Finesse,
                    focus: finesse,
                    step: 0,
                });
                push_signal(
                    signals,
                    entry,
                    *player,
                    Some(target),
                    HGroupMoveKind::TrashPush,
                    vec![finesse],
                    Some(expected),
                );
            }
        }
        _ => {}
    }
}

#[allow(clippy::too_many_arguments)]
fn apply_ejection_discharge_effects(
    entry: &ObservedHistoryEntry,
    view: &PlayerView,
    hands: &[Vec<CardId>],
    facts: &[ClueFacts],
    clues: &[HGroupClueInterpretation],
    stack_heights: [u8; 5],
    explicitly_clued: &CardSet,
    _invisibly_clued: &CardSet,
    _chop_moved: &CardSet,
    pending: &mut Vec<ConnectionObligation>,
    forced_playable: &mut CardSet,
    signals: &mut Vec<HGroupSignal>,
) {
    let ObservedEvent::Clued {
        giver,
        target: _,
        clue,
        touched,
        ..
    } = &entry.event
    else {
        return;
    };
    let interpretation = clues.iter().rev().find(|clue| clue.turn == entry.turn);
    let focus_identity =
        interpretation.and_then(|interpretation| current_card_identity(view, interpretation.focus));
    let ejection_actor = next_player(*giver, hands.len());
    let blind_plays = interpretation.map_or(0, |interpretation| {
        let Some(identity) = focus_identity else {
            return 0;
        };
        let previously_gotten = interpretation
            .previously_gotten
            .iter()
            .copied()
            .collect::<CardSet>();
        ((stack_heights[identity.suit.index()] + 1)..identity.rank.number())
            .filter(|rank| {
                let needed = Card::new(identity.suit, Rank::ALL[usize::from(*rank - 1)]);
                !hands.iter().flatten().copied().any(|card| {
                    previously_gotten.contains(&card) && identity_of(view, card) == Some(needed)
                })
            })
            .count()
    });
    let five_ejection = matches!(clue, Clue::Suit(_))
        && interpretation.is_some_and(|interpretation| {
            !was_clued_before(view, entry.turn, interpretation.focus)
        })
        && focus_identity.is_some_and(|identity| {
            identity.rank == Rank::Five
                && 5_u8.saturating_sub(stack_heights[identity.suit.index()]) >= 2
        })
        && blind_plays >= 2;
    // An Unknown Trash Discharge communicates that the focused card is trash.
    // Merely touching an already-played duplicate as a useful non-focus card is
    // an ordinary multi-card clue and must not eject the next player's slot 3.
    let unknown_discharge = touched.len() >= 2
        && interpretation.is_none_or(|interpretation| interpretation.save_identities.is_empty())
        && interpretation.is_some_and(|interpretation| {
            let possibilities =
                IdentitySet::from_mask(facts[interpretation.focus.index()].identity_mask());
            !possibilities.is_empty()
                && possibilities
                    .iter()
                    .all(|identity| is_trash_at(stack_heights, identity))
        });
    let (kind, position) = if five_ejection {
        (Some(HGroupMoveKind::Ejection), 1)
    } else if unknown_discharge {
        (Some(HGroupMoveKind::Discharge), 2)
    } else {
        (None, 0)
    };
    if let Some(kind) = kind {
        let actor = ejection_actor;
        let Some(card) = finesse_position_id(&hands[actor.index()], explicitly_clued, position)
        else {
            // An Ejection or Discharge cannot supersede an existing connection
            // when the requested ungotten position does not exist.
            return;
        };
        pending.retain(|connection| connection.actor != actor);
        forced_playable.insert(card);
        push_signal(
            signals,
            entry,
            *giver,
            Some(actor),
            kind,
            touched.clone(),
            None,
        );
    }
}

fn apply_elimination_effects(
    entry: &ObservedHistoryEntry,
    view: &PlayerView,
    hands: &[Vec<CardId>],
    stack_heights: [u8; 5],
    signals: &mut Vec<HGroupSignal>,
) {
    match &entry.event {
        ObservedEvent::Discarded {
            player,
            card,
            identity,
        } if identity.rank == Rank::Two || is_playable_at(stack_heights, *identity) => {
            push_signal(
                signals,
                entry,
                *player,
                Some(*player),
                HGroupMoveKind::Elimination,
                hands[player.index()].clone(),
                Some(*identity),
            );
            let _ = card;
        }
        ObservedEvent::Clued {
            giver,
            target,
            touched,
            untouched,
            ..
        } => {
            let has_notes = signals.iter().any(|signal| {
                signal.kind == HGroupMoveKind::Elimination
                    && signal.target == Some(*target)
                    && signal.identity.is_some()
            });
            let singled_out = touched.len() == 1 || untouched.len() == 1;
            if has_notes
                && singled_out
                && touched
                    .iter()
                    .all(|card| was_clued_before(view, entry.turn, *card))
            {
                let cards = if touched.len() == 1 {
                    touched.clone()
                } else {
                    untouched.clone()
                };
                push_signal(
                    signals,
                    entry,
                    *giver,
                    Some(*target),
                    HGroupMoveKind::Elimination,
                    cards,
                    None,
                );
            }
        }
        ObservedEvent::Played { .. }
        | ObservedEvent::Drew { .. }
        | ObservedEvent::Discarded { .. } => {}
    }
}

#[allow(clippy::too_many_arguments, clippy::too_many_lines)]
fn apply_five_tech_effects(
    entry: &ObservedHistoryEntry,
    view: &PlayerView,
    hands: &[Vec<CardId>],
    clues: &[HGroupClueInterpretation],
    stack_heights: [u8; 5],
    explicitly_clued: &CardSet,
    invisibly_clued: &CardSet,
    chop_moved: &mut CardSet,
    pending: &mut Vec<ConnectionObligation>,
    forced_playable: &mut CardSet,
    implicit_saves: &mut Vec<(CardId, IdentitySet)>,
    signals: &mut Vec<HGroupSignal>,
) {
    let ObservedEvent::Clued {
        giver,
        target,
        clue: Clue::Rank(Rank::Five),
        touched,
        ..
    } = &entry.event
    else {
        return;
    };
    if touched.is_empty() {
        return;
    }
    let repeated_five = touched
        .iter()
        .all(|card| was_clued_before(view, entry.turn, *card));
    let gotten = protected_cards(explicitly_clued, invisibly_clued, chop_moved);
    let target_chop = chop(&hands[target.index()], &gotten);
    let visible_saved_two = clues
        .iter()
        .rev()
        .filter(|interpretation| interpretation.turn < entry.turn)
        .filter(|interpretation| !interpretation.save_identities.is_empty())
        .find_map(|interpretation| {
            let twos = IdentitySet::from_mask(
                interpretation
                    .save_identities
                    .iter()
                    .filter(|identity| identity.rank == Rank::Two)
                    .fold(0, |mask, identity| mask | (1 << identity.index())),
            );
            (!twos.is_empty()
                && hands[target.index()]
                    .iter()
                    .all(|card| *card != interpretation.focus))
            .then_some(twos)
        });
    if repeated_five {
        if let (Some(saved), Some(identities)) = (target_chop, visible_saved_two) {
            if !touched.contains(&saved) {
                let possible = view.hands[target.index()]
                    .iter()
                    .find(|card| card.id == saved)
                    .is_some_and(|card| {
                        identities
                            .iter()
                            .any(|identity| card.clues.allows(identity))
                    });
                if possible {
                    implicit_saves.push((saved, identities));
                    push_signal(
                        signals,
                        entry,
                        *giver,
                        Some(*target),
                        HGroupMoveKind::FiveStall,
                        touched.clone(),
                        None,
                    );
                    return;
                }
            }
        }
    }
    if signals
        .iter()
        .any(|signal| signal.turn == entry.turn && signal.kind == HGroupMoveKind::FiveStall)
    {
        return;
    }
    // Save clues take precedence over 5 tech. A rank-5 clue that can be a
    // 5 Save cannot simultaneously pull the adjacent card.
    if clues.last().is_some_and(|interpretation| {
        interpretation.turn == entry.turn && !interpretation.save_identities.is_empty()
    }) {
        return;
    }
    let Some(pulled) = five_pulled_card(&hands[target.index()], touched, &gotten) else {
        return;
    };
    let Some(identity) = current_card_identity(view, pulled) else {
        return;
    };
    let height = stack_heights[identity.suit.index()];
    let actor = next_player(*giver, hands.len());
    let (kind, forced) = if identity.rank.number() <= height {
        let Some(card) = finesse_position_id(&hands[actor.index()], &gotten, 2).filter(|card| {
            current_card_identity(view, *card)
                .is_some_and(|candidate| is_playable_at(stack_heights, candidate))
        }) else {
            return;
        };
        chop_moved.insert(pulled);
        (HGroupMoveKind::Discharge, Some(card))
    } else if identity.rank.number() == height + 1 {
        (HGroupMoveKind::FivePull, Some(pulled))
    } else if identity.rank.number() == height + 2 {
        if actor == *target {
            return;
        }
        let connector = Card::new(identity.suit, Rank::ALL[usize::from(height)]);
        let Some(card) = finesse_position_id(&hands[actor.index()], &gotten, 0)
            .filter(|card| current_card_identity(view, *card) == Some(connector))
        else {
            return;
        };
        pending.retain(|connection| connection.actor != actor && connection.actor != *target);
        pending.push(ConnectionObligation {
            actor,
            cards: vec![card],
            expected: connector,
            kind: HGroupConnectionKind::Finesse,
            focus: pulled,
            step: 0,
        });
        pending.push(ConnectionObligation {
            actor: *target,
            cards: vec![pulled],
            expected: identity,
            kind: HGroupConnectionKind::Finesse,
            focus: pulled,
            step: 1,
        });
        (HGroupMoveKind::FivePull, None)
    } else {
        let Some(card) = finesse_position_id(&hands[actor.index()], &gotten, 1).filter(|card| {
            current_card_identity(view, *card)
                .is_some_and(|candidate| is_playable_at(stack_heights, candidate))
        }) else {
            return;
        };
        chop_moved.insert(pulled);
        (HGroupMoveKind::Ejection, Some(card))
    };
    if let Some(forced) = forced {
        pending.retain(|connection| connection.actor != actor);
        forced_playable.insert(forced);
    }
    push_signal(
        signals,
        entry,
        *giver,
        Some(*target),
        kind,
        touched.clone(),
        touched
            .iter()
            .find_map(|card| current_card_identity(view, *card)),
    );
}

#[allow(clippy::too_many_arguments)]
fn apply_out_of_order_effects(
    entry: &ObservedHistoryEntry,
    view: &PlayerView,
    _hands: &[Vec<CardId>],
    clues: &[HGroupClueInterpretation],
    stack_heights: [u8; 5],
    _pending: &mut Vec<ConnectionObligation>,
    _forced_playable: &mut CardSet,
    required_fix: &mut Option<RequiredFix>,
    signals: &mut Vec<HGroupSignal>,
) {
    let ObservedEvent::Clued {
        giver,
        target,
        touched,
        ..
    } = &entry.event
    else {
        return;
    };
    let clue_focus = clues
        .iter()
        .rev()
        .find(|clue| clue.turn == entry.turn)
        .filter(|clue| {
            matches!(clue.kind, HGroupClueKind::Play)
                || (clue.kind == HGroupClueKind::Unrecognized && clue.save_identities.is_empty())
        })
        .map(|clue| clue.focus);
    if let Some(card) = clue_focus.filter(|card| {
        current_card_identity(view, *card).is_some_and(|identity| {
            identity.rank.number() > stack_heights[identity.suit.index()] + 1
                && touched.iter().any(|candidate| {
                    candidate != card
                        && current_card_identity(view, *candidate).is_some_and(|lower| {
                            lower.suit == identity.suit
                                && lower.rank.number() < identity.rank.number()
                                && lower.rank.number() > stack_heights[identity.suit.index()]
                        })
                })
        })
    }) {
        let focus = card;
        let focus_identity = current_card_identity(view, focus);
        if let Some(identity) = focus_identity {
            *required_fix = Some(RequiredFix {
                actor: next_player(*giver, view.hands.len()),
                target: *target,
                focus,
                identity,
            });
        }
        push_signal(
            signals,
            entry,
            *giver,
            Some(*target),
            HGroupMoveKind::OccupiedPlay,
            vec![focus],
            focus_identity,
        );
    }
}

fn apply_ignition_effects(
    entry: &ObservedHistoryEntry,
    view: &PlayerView,
    stack_heights: [u8; 5],
    forced_playable: &mut CardSet,
    signals: &mut Vec<HGroupSignal>,
) {
    let ObservedEvent::Clued {
        giver,
        target,
        touched,
        ..
    } = &entry.event
    else {
        return;
    };
    let touched_identities = touched
        .iter()
        .filter_map(|card| current_card_identity(view, *card))
        .collect::<Vec<_>>();
    let every_touched_card_is_playable = touched.len() >= 2
        && touched_identities.len() == touched.len()
        && touched_identities
            .iter()
            .all(|identity| is_playable_at(stack_heights, *identity))
        && identity_set(touched_identities.iter().copied()).len() == touched_identities.len();
    if every_touched_card_is_playable {
        forced_playable.extend(touched.iter().copied());
        push_signal(
            signals,
            entry,
            *giver,
            Some(*target),
            HGroupMoveKind::Ignition,
            touched.clone(),
            None,
        );
    }
}

fn apply_phantom_effects(
    entry: &ObservedHistoryEntry,
    view: &PlayerView,
    hands: &[Vec<CardId>],
    _forced_playable: &mut CardSet,
    signals: &mut Vec<HGroupSignal>,
) {
    let ObservedEvent::Discarded { player, card, .. } = &entry.event else {
        return;
    };
    let target = next_player(*player, hands.len());
    if visible_playable_in_hand(view, target, None).is_none() {
        return;
    }
    // "Phantom playable" describes the endangered card that motivated an
    // emergency discard. It does not tell the next player to blind-play their
    // newest card; the Scream/Generation discard rules determine that reply.
    push_signal(
        signals,
        entry,
        *player,
        Some(target),
        HGroupMoveKind::PhantomPlayable,
        vec![*card],
        None,
    );
}

#[allow(clippy::too_many_arguments)]
fn apply_charm_effects(
    entry: &ObservedHistoryEntry,
    view: &PlayerView,
    hands: &[Vec<CardId>],
    clues: &[HGroupClueInterpretation],
    stack_heights: [u8; 5],
    explicitly_clued: &CardSet,
    invisibly_clued: &CardSet,
    chop_moved: &CardSet,
    pending: &mut Vec<ConnectionObligation>,
    forced_playable: &mut CardSet,
    signals: &mut Vec<HGroupSignal>,
) {
    match &entry.event {
        ObservedEvent::Clued {
            giver,
            target,
            clue: Clue::Rank(Rank::Four),
            touched,
            ..
        } => {
            let interpretation = clues.iter().rev().find(|clue| clue.turn == entry.turn);
            let charm_focus = interpretation.and_then(|clue| {
                current_card_identity(view, clue.focus).filter(|identity| {
                    usize::from(identity.rank.number())
                        == usize::from(stack_heights[identity.suit.index()]) + 4
                })
            });
            if let (Some(interpretation), Some(_)) = (interpretation, charm_focus) {
                // Once a rank-4 clue would require three ordinary blind plays,
                // it is either a valid 4 Charm or an invalid clue. Do not leave
                // the generic layered-Finesse interpretation active.
                pending.retain(|connection| connection.focus != interpretation.focus);
                let actor = next_player(*giver, hands.len());
                let gotten = explicitly_clued
                    .union(invisibly_clued)
                    .copied()
                    .chain(chop_moved.iter().copied())
                    .collect::<CardSet>();
                let charmed = (actor != *target)
                    .then(|| finesse_position_id(&hands[actor.index()], &gotten, 3))
                    .flatten()
                    .filter(|card| {
                        current_card_identity(view, *card)
                            .is_some_and(|identity| is_playable_at(stack_heights, identity))
                    });
                if let Some(charmed) = charmed {
                    pending.retain(|connection| connection.actor != actor);
                    forced_playable.insert(charmed);
                    push_signal(
                        signals,
                        entry,
                        *giver,
                        Some(actor),
                        HGroupMoveKind::Charm,
                        vec![charmed],
                        current_card_identity(view, charmed),
                    );
                }
            }
            let _ = touched;
        }
        ObservedEvent::Discarded {
            player,
            card,
            identity,
        } if was_clued_before(view, entry.turn, *card) && !card_is_trash(view, *identity) => {
            push_signal(
                signals,
                entry,
                *player,
                None,
                HGroupMoveKind::Charm,
                vec![*card],
                Some(*identity),
            );
        }
        _ => {}
    }
}

fn apply_unnecessary_move_effects(
    context: &HGroupTurnContext<'_>,
    view: &PlayerView,
    signals: &mut Vec<HGroupSignal>,
) {
    let entry = context.entry;
    let stack_heights = context.before.stack_heights;
    let (actor, cards, unnecessary) = match &entry.event {
        ObservedEvent::Clued { giver, touched, .. } => (
            *giver,
            touched.clone(),
            !touched.is_empty()
                && touched.iter().all(|card| {
                    current_card_identity(view, *card)
                        .is_some_and(|identity| is_trash_at(stack_heights, identity))
                }),
        ),
        ObservedEvent::Discarded {
            player,
            card,
            identity,
        } => (*player, vec![*card], !is_trash_at(stack_heights, *identity)),
        _ => return,
    };
    if unnecessary {
        push_signal(
            signals,
            entry,
            actor,
            None,
            HGroupMoveKind::UnnecessaryMove,
            cards,
            None,
        );
    }
}

#[allow(clippy::too_many_arguments)]
fn apply_priority_effects(
    context: &HGroupTurnContext<'_>,
    view: &PlayerView,
    explicitly_clued: &CardSet,
    forced_playable: &mut CardSet,
    signals: &mut Vec<HGroupSignal>,
) {
    let entry = context.entry;
    let hands = &context.before.hands;
    let facts = &context.before.facts;
    let stack_heights = context.before.stack_heights;
    let ObservedEvent::Played {
        player,
        card,
        identity,
        successful: true,
        ..
    } = &entry.event
    else {
        return;
    };

    // Ordinary Priority is a deliberate choice between globally-known playable
    // cards. Trust Finesses from unknown cards need substantially stronger
    // evidence and must not be inferred merely from hidden simulator truth.
    let played_possibilities = IdentitySet::from_mask(facts[card.index()].identity_mask());
    if played_possibilities != IdentitySet::singleton(*identity) {
        return;
    }

    let actor_hand = &hands[player.index()];
    let fixed_cards = currently_fixed_cards(signals);
    let declined_priority = actor_hand.iter().copied().find(|candidate| {
        if *candidate == *card || fixed_cards.contains(candidate) {
            return false;
        }
        let possibilities = IdentitySet::from_mask(facts[candidate.index()].identity_mask());
        !possibilities.is_empty()
            && possibilities.iter().all(|candidate_identity| {
                is_playable_at(stack_heights, candidate_identity)
                    && has_higher_basic_priority(
                        view,
                        hands,
                        facts,
                        explicitly_clued,
                        forced_playable,
                        *player,
                        actor_hand,
                        *candidate,
                        candidate_identity,
                        *card,
                        *identity,
                    )
            })
    });

    if declined_priority.is_some() {
        let target = next_player(*player, hands.len());
        if identity.rank != Rank::Five {
            let connector = Card::new(identity.suit, Rank::ALL[identity.rank.index() + 1]);
            let prompt = hands[target.index()]
                .iter()
                .rev()
                .copied()
                .find(|candidate| {
                    explicitly_clued.contains(candidate)
                        && current_card_identity(view, *candidate) == Some(connector)
                });
            let finesse = hands[target.index()]
                .iter()
                .rev()
                .copied()
                .find(|candidate| {
                    !explicitly_clued.contains(candidate)
                        && (target == view.observer
                            || current_card_identity(view, *candidate) == Some(connector))
                });
            if let Some(connection) = prompt.or(finesse) {
                forced_playable.insert(connection);
            }
        }
        push_signal(
            signals,
            entry,
            *player,
            None,
            HGroupMoveKind::Priority,
            vec![*card],
            current_card_identity(view, *card),
        );
    }
}

#[allow(clippy::too_many_arguments)]
fn apply_extra_effects(
    entry: &ObservedHistoryEntry,
    view: &PlayerView,
    hands: &[Vec<CardId>],
    explicitly_clued: &CardSet,
    pending: &mut Vec<ConnectionObligation>,
    chop_moved: &mut CardSet,
    must_clue: &mut PlayerSet,
    forced_playable: &mut CardSet,
    signals: &mut Vec<HGroupSignal>,
) {
    match &entry.event {
        ObservedEvent::Clued {
            target, touched, ..
        } => {
            let pending_cards = pending
                .iter()
                .filter(|connection| connection.actor == *target)
                .flat_map(|connection| connection.cards.iter().copied())
                .collect::<CardSet>();
            let continuation = touched.iter().any(|card| pending_cards.contains(card))
                && touched
                    .iter()
                    .any(|card| !was_clued_before(view, entry.turn, *card));
            if !continuation {
                let invalidated = pending
                    .iter()
                    .filter(|connection| connection.actor == *target)
                    .filter(|connection| {
                        connection.cards.iter().any(|pending_card| {
                            view.hands[target.index()]
                                .iter()
                                .find(|card| card.id == *pending_card)
                                .is_some_and(|card| !card.clues.allows(connection.expected))
                        })
                    })
                    .map(|connection| connection.focus)
                    .collect::<CardSet>();
                pending.retain(|connection| !invalidated.contains(&connection.focus));
            }
        }
        ObservedEvent::Played {
            player,
            card,
            successful,
            ..
        } => {
            if *successful && !was_clued_before(view, entry.turn, *card) {
                forced_playable.remove(card);
            } else if !successful {
                let target = next_player(*player, hands.len());
                let gotten = explicitly_clued
                    .iter()
                    .chain(chop_moved.iter())
                    .copied()
                    .collect::<CardSet>();
                if let Some(card) = chop(&hands[target.index()], &gotten) {
                    chop_moved.insert(card);
                    must_clue.insert(target);
                }
            }
        }
        ObservedEvent::Discarded { .. } | ObservedEvent::Drew { .. } => {}
    }
    // Extras refine or compose the numbered primitives. Preserve an explicit
    // marker when a single turn already matched two or more primitive effects;
    // consumers can inspect the preceding same-turn signals to recover the
    // exact composition without a parallel hierarchy of bespoke state types.
    let same_turn = signals
        .iter()
        .filter(|signal| signal.turn == entry.turn && signal.kind != HGroupMoveKind::Extra)
        .count();
    if same_turn >= 2 {
        let actor = match &entry.event {
            ObservedEvent::Clued { giver, .. } => *giver,
            ObservedEvent::Played { player, .. }
            | ObservedEvent::Discarded { player, .. }
            | ObservedEvent::Drew { player, .. } => *player,
        };
        let cards = match &entry.event {
            ObservedEvent::Clued { touched, .. } => touched.clone(),
            ObservedEvent::Played { card, .. }
            | ObservedEvent::Discarded { card, .. }
            | ObservedEvent::Drew { card, .. } => vec![*card],
        };
        push_signal(
            signals,
            entry,
            actor,
            None,
            HGroupMoveKind::Extra,
            cards,
            None,
        );
    }
}

#[allow(clippy::too_many_arguments, clippy::too_many_lines)]
fn schedule_connection(
    profile: HGroupProfile,
    view: &PlayerView,
    giver: PlayerId,
    target: PlayerId,
    focus: CardId,
    same_clue_touched: &[CardId],
    focus_identity: Option<Card>,
    hands: &[Vec<CardId>],
    facts: &[ClueFacts],
    clues: &[HGroupClueInterpretation],
    promptable_before_clue: &CardSet,
    already_playing: &CardSet,
    invisibly_clued: &mut CardSet,
    stack_heights: [u8; 5],
    pending: &mut Vec<ConnectionObligation>,
) {
    let Some(focus_identity) = focus_identity else {
        return;
    };
    let height = stack_heights[focus_identity.suit.index()];
    if focus_identity.rank.number() <= height + 1 {
        return;
    }
    let connection_count = if rule_enabled(profile, HGroupRuleId::BasicMoves) {
        focus_identity.rank.number().saturating_sub(height + 1)
    } else {
        1
    };
    let mut actor_index = (giver.index() + 1) % hands.len();
    let mut scheduled_cards = CardSet::default();
    for offset in 0..connection_count {
        let expected_rank = usize::from(height + offset);
        let expected = Card::new(focus_identity.suit, Rank::ALL[expected_rank]);
        let mut found = None;
        let search_len = if rule_enabled(profile, HGroupRuleId::BasicMoves) {
            (target.index() + hands.len() - actor_index) % hands.len() + 1
        } else {
            1
        };
        let directly_clued = hands[target.index()]
            .iter()
            .rev()
            .copied()
            .filter(|card| {
                *card != focus
                    && same_clue_touched.contains(card)
                    && promptable_before_clue.contains(card)
                    && !scheduled_cards.contains(card)
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
            let prompt_cards = hands[candidate_index]
                .iter()
                .rev()
                .copied()
                .filter(|card| {
                    *card != focus
                        && promptable_before_clue.contains(card)
                        && !already_playing.contains(card)
                        && !scheduled_cards.contains(card)
                        && identity_of(view, *card).map_or_else(
                            || facts[card.index()].allows(expected),
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
        if found.is_none() {
            for distance in 0..search_len {
                let candidate_index = (actor_index + distance) % hands.len();
                let actor = PlayerId::new(
                    u8::try_from(candidate_index)
                        .expect("standard Hanabi has at most five players"),
                );
                let queued = hands[candidate_index].iter().rev().copied().find(|card| {
                    *card != focus
                        && already_playing.contains(card)
                        && !scheduled_cards.contains(card)
                        && (identity_of(view, *card) == Some(expected)
                            || facts[card.index()].identity_mask() == 1 << expected.index()
                            || convention_focus_is_live_identity(
                                *card,
                                expected,
                                view,
                                clues,
                                already_playing,
                                stack_heights,
                            ))
                });
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
                if target == actor {
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
                            unclued
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
                    found = Some((actor, cards, HGroupConnectionKind::Finesse));
                    actor_index = candidate_index;
                    break;
                }
            }
        }
        let Some((actor, cards, kind)) = found else {
            break;
        };
        if kind == HGroupConnectionKind::Finesse {
            invisibly_clued.extend(cards.iter().copied());
        }
        scheduled_cards.extend(cards.iter().copied());
        pending.push(ConnectionObligation {
            actor,
            cards,
            expected,
            kind,
            focus,
            step: offset,
        });
        actor_index = (actor_index + 1) % hands.len();
    }
}

fn convention_focus_is_live_identity(
    card: CardId,
    expected: Card,
    view: &PlayerView,
    clues: &[HGroupClueInterpretation],
    already_playing: &CardSet,
    stack_heights: [u8; 5],
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
}

fn pending_is_active(candidate: &ConnectionObligation, pending: &[ConnectionObligation]) -> bool {
    !pending.iter().any(|other| {
        other.focus == candidate.focus && other.step < candidate.step && !other.cards.is_empty()
    })
}

fn advance_pending_connections(
    pending: &mut Vec<ConnectionObligation>,
    player: PlayerId,
    card: CardId,
    identity: Card,
    successful: bool,
) -> Vec<CardId> {
    let mut failed_focuses = Vec::new();
    let active = pending
        .iter()
        .enumerate()
        .filter(|(_, item)| item.actor == player && pending_is_active(item, pending))
        .map(|(index, _)| index)
        .collect::<Vec<_>>();
    for index in active {
        let connection = &mut pending[index];
        if connection.cards.first() != Some(&card) {
            connection.cards.retain(|candidate| *candidate != card);
            continue;
        }
        connection.cards.remove(0);
        if identity == connection.expected || !successful {
            connection.cards.clear();
        } else if connection.cards.is_empty() {
            failed_focuses.push(connection.focus);
        }
    }
    pending.retain(|connection| {
        !connection.cards.is_empty()
            && connection.focus != card
            && !failed_focuses.contains(&connection.focus)
    });
    failed_focuses
}

fn identity_set(identities: impl IntoIterator<Item = Card>) -> IdentitySet {
    identities
        .into_iter()
        .fold(IdentitySet::default(), |set, identity| {
            set.union(IdentitySet::singleton(identity))
        })
}

fn chop(hand: &[CardId], explicitly_clued: &CardSet) -> Option<CardId> {
    hand.iter()
        .copied()
        .find(|card| !explicitly_clued.contains(card))
}

fn currently_fixed_cards(signals: &[HGroupSignal]) -> CardSet {
    let mut fixed = CardSet::default();
    for signal in signals {
        match signal.kind {
            HGroupMoveKind::FixClue => fixed.extend(signal.cards.iter().copied()),
            HGroupMoveKind::PlayClue => {
                for card in &signal.cards {
                    fixed.remove(card);
                }
            }
            _ => {}
        }
    }
    fixed
}

fn finesse_position<'a>(
    hand: &'a [ObservedCard],
    gotten: &CardSet,
    position: usize,
) -> Option<&'a ObservedCard> {
    hand.iter()
        .rev()
        .filter(|card| !gotten.contains(&card.id))
        .nth(position)
}

fn finesse_position_id(hand: &[CardId], gotten: &CardSet, position: usize) -> Option<CardId> {
    hand.iter()
        .rev()
        .filter(|card| !gotten.contains(card))
        .nth(position)
        .copied()
}

fn five_pulled_card(hand: &[CardId], touched: &[CardId], gotten: &CardSet) -> Option<CardId> {
    let five_position = touched
        .iter()
        .copied()
        .filter(|card| !gotten.contains(card))
        .filter_map(|card| {
            hand.iter()
                .position(|candidate| *candidate == card)
                .map(|position| (position, card))
        })
        .max_by_key(|(position, _)| *position)
        .map(|(position, _)| position)?;
    hand[..five_position]
        .iter()
        .rev()
        .copied()
        .find(|card| !gotten.contains(card))
}

fn focus(
    hand: &[CardId],
    touched: &[CardId],
    chop: Option<CardId>,
    explicitly_clued: &CardSet,
) -> Option<CardId> {
    let newly_touched = touched
        .iter()
        .copied()
        .filter(|card| !explicitly_clued.contains(card))
        .collect::<Vec<_>>();
    match newly_touched.as_slice() {
        [] => hand
            .iter()
            .rev()
            .copied()
            .find(|card| touched.contains(card)),
        [only] => Some(*only),
        _ if chop.is_some_and(|card| touched.contains(&card)) => chop,
        _ => hand
            .iter()
            .rev()
            .copied()
            .find(|card| newly_touched.contains(card)),
    }
}

fn identity_of(view: &PlayerView, card: CardId) -> Option<Card> {
    view.hands
        .iter()
        .flatten()
        .find(|candidate| candidate.id == card)
        .and_then(|candidate| candidate.identity)
        .or_else(|| {
            view.play_stacks
                .iter()
                .flatten()
                .chain(view.discard_pile.iter())
                .find_map(|(candidate, identity)| (*candidate == card).then_some(*identity))
        })
        .or_else(|| {
            view.history.iter().find_map(|entry| match entry.event {
                ObservedEvent::Played {
                    card: candidate,
                    identity,
                    ..
                }
                | ObservedEvent::Discarded {
                    card: candidate,
                    identity,
                    ..
                } if candidate == card => Some(identity),
                ObservedEvent::Drew {
                    card: candidate,
                    identity: Some(identity),
                    ..
                } if candidate == card => Some(identity),
                _ => None,
            })
        })
}

fn is_critical(view: &PlayerView, identity: Card) -> bool {
    identity.rank != Rank::Five
        && view
            .discard_pile
            .iter()
            .filter(|(_, discarded)| *discarded == identity)
            .count()
            + 1
            == usize::from(identity.rank.copies())
}

fn remove_card(hand: &mut Vec<CardId>, card: CardId) {
    if let Some(position) = hand.iter().position(|candidate| *candidate == card) {
        hand.remove(position);
    }
}

#[cfg(test)]
mod tests;
