//! Convention interpretation and clue-candidate construction.
//!
//! This module owns observer-relative clue meaning, card inference, and the
//! generation of convention-admissible clue candidates. Public-history
//! reduction and level-specific event recognition live in sibling modules.

#[cfg(test)]
use super::decision::{analysis_clue_candidates, build_h_group_analysis};
use super::{
    Action, BluffTargetKind, Card, CardId, CardSet, Clue, ClueCandidate, ClueFacts, CluePurpose,
    ClueRecognition, ClueSchedule, ClueValue, ConnectionObligation, ConventionFacts,
    ConventionKnowledge, ConventionRejectionReason, FixCondition, HGroupCardInference,
    HGroupClueInterpretation, HGroupClueKind, HGroupConnection, HGroupConnectionKind,
    HGroupInferences, HGroupMoveKind, HGroupPlayObligation, HGroupProfile, HGroupRuleId,
    HGroupState, HistoricalView, IdentityClaims, IdentitySet, KNOWN_TRASH_COLLATERAL_BONUS,
    LogicalDeductions, MAX_CLUE_TOKENS, MaterializedCardFact, ObservedCard, ObservedEvent,
    PlayerId, PlayerSet, PlayerView, Rank, RejectedConventionAction, RequiredFix,
    SemanticallyAdmittedCandidates, StackTimeline, bluff_play_connects,
    bluff_target_order_is_legal, card_is_trash, chop, convention_information_value,
    finesse_position, five_chop_moved_card, five_pulled_card, focus,
    identity_is_queued_before_target, identity_of, identity_set, infer_h_group_from_replay,
    is_convention_trash, is_critical, is_eventually_useful, is_playable_at, is_playable_now,
    is_unique_visible, next_player, ordered_playable_cards, pending_card_allows_identity,
    pending_identity_is_queued, pending_is_active, positional_discard_candidate,
    preferred_due_play_card, projected_h_group_replay, prospective_clue_has_unsafe_connection,
    prospective_clue_marks_focus_saved, prospective_clue_primary_interpretation,
    prospective_clue_primary_kind, prospective_clue_signal_kinds, prospective_clue_view,
    prospective_play_view, prospective_stacked_ejection_card, prospective_team_clue_signal_kinds,
    replay_identity_is_queued, rule_enabled, subjective_convention_cards,
    subjective_playable_cards, was_clued_before, with_prospective_analysis_cache,
};

mod candidate_validation;
mod knowledge;
pub(super) use candidate_validation::{
    h_group_rejected_clues_from_replay, recipient_replay_assessment,
};
pub(super) use knowledge::{
    build_convention_knowledge, convention_card_inferences, convention_playable,
    delayed_focus_identities, find_prompt, identities_at_distance_at,
    snapshot_good_touch_identities, snapshot_play_identities, snapshot_save_identities,
    two_save_allowed,
};

#[cfg(test)]
#[allow(clippy::too_many_lines)]
pub(super) fn h_group_clue_candidates(
    deductions: &LogicalDeductions,
    profile: HGroupProfile,
) -> Vec<ClueCandidate> {
    let analysis = build_h_group_analysis(deductions, profile);
    analysis_clue_candidates(deductions, profile, &analysis).to_vec()
}

#[allow(clippy::too_many_lines)]
pub(super) fn h_group_clue_candidates_from_replay(
    deductions: &LogicalDeductions,
    profile: HGroupProfile,
    replay: &HGroupState,
) -> Vec<ClueCandidate> {
    with_prospective_analysis_cache(deductions.view(), profile, || {
        h_group_clue_candidates_from_replay_inner(deductions, profile, replay)
    })
}

#[allow(clippy::too_many_lines)]
pub(super) fn h_group_clue_candidates_from_replay_inner(
    deductions: &LogicalDeductions,
    profile: HGroupProfile,
    replay: &HGroupState,
) -> Vec<ClueCandidate> {
    let view = deductions.view();
    if view.clue_tokens == 0 {
        return Vec::new();
    }
    if let Some(required) = replay
        .required_fixes
        .iter()
        .find(|obligation| {
            obligation.required.actor == view.observer
                && fix_condition_is_live(view, obligation.condition)
        })
        .map(|obligation| obligation.required)
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
                let unambiguously_stops_play = required_focus_card.is_some_and(|card| {
                    let mut facts = card.clues;
                    facts.add_positive_clue(clue);
                    let possibilities = IdentitySet::from_mask(facts.identity_mask());
                    !possibilities.is_empty()
                        && possibilities
                            .iter()
                            .all(|identity| !is_playable_now(view, identity))
                });
                let contradicts_promised_play =
                    replay.pending_connections.iter().any(|connection| {
                        connection.actor == required.target
                            && connection.cards.contains(&required.focus)
                            && pending_is_active(connection, &replay.pending_connections)
                            && !clue.matches(connection.expected)
                    });
                (target == required.target
                    && clue.matches(required.identity)
                    && touched.contains(&required.focus)
                    // A Fix is interpreted before ordinary direct-clue
                    // playability. Contradicting the identity promised for
                    // this exact Finesse position proves that the blind play
                    // was a lie, even if the new positive clue alone would
                    // still allow some other playable identity.
                    && (contradicts_promised_play || unambiguously_stops_play)
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
                .then(|| {
                    let information =
                        convention_information_value(view, profile, replay, target, clue, &touched);
                    (
                        ClueCandidate {
                            action,
                            value: ClueValue::new(600),
                            purpose: CluePurpose::Fix,
                            target,
                            save: false,
                            schedule: ClueSchedule::new(false, false),
                            connection_steps: 0,
                            action_coverage: 0,
                            convention_action_count: None,
                            convention_connection_steps: None,
                            recognition: ClueRecognition::GeneratorProof,
                        },
                        touched,
                        information,
                    )
                })
            })
            .collect::<Vec<_>>();
        if !required_candidates.is_empty() {
            let comparison_keys = required_candidates
                .iter()
                .map(|(candidate, touched, information)| {
                    let color_tie_break = matches!(
                        candidate.action,
                        Action::Clue {
                            clue: Clue::Suit(_),
                            ..
                        }
                    ) && required_candidates.iter().any(
                        |(alternative, other_touched, other_information)| {
                            matches!(
                                alternative.action,
                                Action::Clue {
                                    target,
                                    clue: Clue::Rank(_),
                                } if target == candidate.target
                            ) && other_touched == touched
                                && other_information == information
                        },
                    );
                    (*information, color_tie_break)
                })
                .collect::<Vec<_>>();
            let information_ranks = comparison_keys
                .iter()
                .map(|key| {
                    comparison_keys
                        .iter()
                        .filter(|alternative| *alternative < key)
                        .count()
                })
                .collect::<Vec<_>>();
            return required_candidates
                .into_iter()
                .zip(information_ranks)
                .map(|((mut candidate, _, _), information_rank)| {
                    // Valid Fixes are compared by recipient-visible negative
                    // information. The Level 1 color preference is retained
                    // only as the final tie-break for identical touch sets.
                    candidate.value.add_information(
                        u16::try_from(information_rank).unwrap_or(u16::MAX - candidate.score()),
                    );
                    candidate
                })
                .collect();
        }
    }
    let promptable = replay.promptable();
    let fixed_cards = replay.cards.facts.fixed_cards();
    let gotten = replay.gotten_from(&promptable);
    let next_player = PlayerId::new(
        u8::try_from((view.current_player.index() + 1) % view.hands.len())
            .expect("standard Hanabi has at most five players"),
    );
    let convention_cards = convention_card_inferences(deductions, replay);
    let mut baseline_playing = replay.cards.already_playing.clone();
    let active_connection_cards = replay
        .pending_connections
        .iter()
        .filter(|connection| pending_is_active(connection, &replay.pending_connections))
        .flat_map(|connection| connection.cards.iter().copied())
        .collect::<CardSet>();
    let due_connection_cards = replay
        .pending_connections
        .iter()
        .filter(|connection| pending_is_active(connection, &replay.pending_connections))
        .filter_map(|connection| connection.cards.first().copied())
        .collect::<CardSet>();
    let conditional_connection_cards = replay
        .pending_connections
        .iter()
        .flat_map(|connection| connection.cards.iter().skip(1).copied())
        .collect::<CardSet>();
    baseline_playing.extend(active_connection_cards.iter().copied());
    let mut giver_has_playable_now = false;
    for player in 0..view.hands.len() {
        let observer =
            PlayerId::new(u8::try_from(player).expect("standard Hanabi has at most five players"));
        if let Some(cards) = subjective_playable_cards(view, profile, observer) {
            if observer == view.current_player && !cards.is_empty() {
                giver_has_playable_now = true;
            }
            baseline_playing.extend(cards);
        }
    }
    let next_player_has_multi_one = view.hands[next_player.index()]
        .iter()
        .filter(|card| {
            !promptable.contains(&card.id)
                && card.identity.is_some_and(|identity| {
                    identity.rank == Rank::One && is_playable_now(view, identity)
                })
        })
        .take(2)
        .count()
        >= 2;
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
        let newly_informed = touched
            .iter()
            .copied()
            .filter(|card| !promptable.contains(card))
            .collect::<Vec<_>>();
        // Minimum Clue Value rejects a literal reclue, not a clue that fills
        // in a new suit/rank on an already gotten card. Fill-in/Fix clues are
        // essential for repairing ambiguous connection notes.
        let adds_objective_information = hand
            .iter()
            .any(|card| touched.contains(&card.id) && !card.clues.has_positive_clue(clue));
        if !adds_objective_information {
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
        let five_chop_move = rule_enabled(profile, HGroupRuleId::ChopMoves)
            && clue == Clue::Rank(Rank::Five)
            && five_chop_moved_card(layout, &touched, &gotten).is_some();
        let repairs_required_fix = replay.required_fixes.iter().any(|obligation| {
            let required = obligation.required;
            required.actor == view.current_player
                && fix_condition_is_live(view, obligation.condition)
                && required.target == target
                && touched.contains(&required.focus)
                && clue.matches(required.identity)
        });
        let repairs_focus_inversion =
            replay
                .signals
                .of_kind(HGroupMoveKind::FocusInversion)
                .any(|signal| {
                    signal.target == Some(target)
                // Focus Inversion records [old focus, new focus]. The old
                // card's promise was cancelled; only the newly established
                // out-of-order focus requires a Fix before it acts.
                && signal.cards.last() == Some(&focus)
                });
        if !repairs_required_fix
            && !repairs_focus_inversion
            && baseline_playing.contains(&focus)
            && gotten.contains(&focus)
            && (replay.clues.iter().rev().any(|prior| {
                prior.focus == focus
                    && prior.focus_identities == IdentitySet::singleton(focus_identity)
            }) || replay
                .pending_connections
                .iter()
                .any(|pending| pending.focus == focus && pending.focus_identity == focus_identity)
                || subjective_convention_cards(view, profile, target).is_some_and(|cards| {
                    cards.iter().any(|card| {
                        card.card == focus
                            && card.identities == IdentitySet::singleton(focus_identity)
                    })
                }))
        {
            // Adding a direct clue fact that merely repeats an identity the
            // recipient already knows by convention is not Minimum Clue
            // Value. A genuine Fix remains allowed when their note differs,
            // as does a fill-in that turns a merely saved exact card into a
            // newly scheduled Play connection.
            continue;
        }
        if repairs_required_fix || repairs_focus_inversion {
            // A Fix repairs an existing false or ambiguous promise. Once the
            // repair has been identified, it must not flow through filters
            // for creating a brand-new Play Clue: successor fill-in,
            // self-prompt, and anxiety checks answer different questions and
            // can incorrectly erase the mandatory repair.
            candidates.push(ClueCandidate {
                action,
                value: ClueValue::new(650),
                purpose: CluePurpose::Fix,
                target,
                save: false,
                schedule: ClueSchedule::new(false, is_playable_now(view, focus_identity)),
                connection_steps: 0,
                action_coverage: 0,
                convention_action_count: None,
                convention_connection_steps: None,
                recognition: ClueRecognition::GeneratorProof,
            });
            continue;
        }
        let redundant_delayed_successor_fill_in = gotten.contains(&focus)
            && !is_playable_now(view, focus_identity)
            && focus_identity.rank != Rank::One
            && replay.clues.iter().rev().any(|prior| {
                let predecessor = Card::new(
                    focus_identity.suit,
                    Rank::ALL[focus_identity.rank.index() - 1],
                );
                matches!(
                    prior.kind,
                    HGroupClueKind::Play | HGroupClueKind::PlayOrSave
                ) && prior.target == target
                    && prior.touched.contains(&focus)
                    && prior.focus_identities == IdentitySet::singleton(predecessor)
            });
        if redundant_delayed_successor_fill_in {
            // Level 6 does not make generic identity fill-ins legal. A Tempo
            // Clue must get an already-clued card to play now; merely naming a
            // delayed successor behind an existing Good Touch line creates no
            // new action. The currently-playable case is deliberately kept so
            // it can be classified as valuable Tempo, Tempo Stall, or TCCM.
            // Source: https://hanabi.github.io/level-6/#the-tempo-clue
            continue;
        }
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
        let duplicates_existing_good_touch = is_eventually_useful(view, focus_identity)
            && (convention_cards.iter().any(|card| {
                card.card != focus
                    // Later cards in an ordered Finesse are conditional
                    // alternatives, not independent Good Touch promises—unless
                    // the card already carried physical clue information before
                    // entering that later connection. In that case its older
                    // Good Touch promise remains live and still forbids a
                    // duplicate identity elsewhere.
                    && (!conditional_connection_cards.contains(&card.card)
                        || promptable.contains(&card.card))
                    && promptable.contains(&card.card)
                    && view
                        .hands
                        .iter()
                        .flatten()
                        .any(|held| held.id == card.card && held.identity.is_none())
                    && card.promised_identity.map_or_else(
                        || card.identities.contains(focus_identity),
                        |promised| promised == focus_identity,
                    )
            }) || view.hands.iter().flatten().any(|card| {
                card.id != focus
                    && promptable.contains(&card.id)
                    && card.identity == Some(focus_identity)
            }));
        let adds_valuable_non_focus = newly_informed.iter().copied().any(|card| {
            card != focus
                && identity_of(view, card)
                    .is_some_and(|identity| is_eventually_useful(view, identity))
        });
        let adds_non_focus_tempo = hand.iter().any(|card| {
            card.id != focus
                && touched.contains(&card.id)
                && promptable.contains(&card.id)
                && !card.clues.has_positive_clue(clue)
                && card
                    .identity
                    .is_some_and(|identity| is_playable_now(view, identity))
        });
        if duplicates_existing_good_touch
            && !adds_valuable_non_focus
            && !adds_non_focus_tempo
            && save_score.is_none()
        {
            // A Play Clue cannot spend a clue merely to duplicate an identity
            // already promised by Good Touch. Retouching old cards does not
            // create Duplicitous Value: the clue must get an additional card,
            // not just add a redundant suit/rank fact to cards already gotten.
            // Save clues have their separate Duplication Responsibility rules.
            // Sources:
            // - https://hanabi.github.io/level-1/#good-touch-principle
            // - https://hanabi.github.io/level-17/#the-duplicitous-value-clue
            continue;
        }
        let mut play_score = play_clue_score(
            view,
            profile,
            target,
            focus,
            focus_identity,
            clue,
            &newly_informed,
            touched
                .iter()
                .filter(|card| {
                    !replay
                        .signals
                        .of_kind(HGroupMoveKind::OrderChopMove)
                        .any(|signal| signal.cards.contains(card))
                })
                .count(),
            &promptable,
            fixed_cards,
            &replay.cards.invalidated_focuses,
            &baseline_playing,
            &replay.pending_connections,
            &convention_cards,
            &replay.cards.facts,
        )
        .or_else(|| {
            let identities = newly_informed
                .iter()
                .filter_map(|card| identity_of(view, *card))
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
        })
        .or_else(|| {
            rule_enabled(profile, HGroupRuleId::Elimination)
                .then(|| {
                    elimination_finesse_connection(
                        view,
                        &replay.hands,
                        None,
                        None,
                        &replay.cards.facts,
                        &replay.cards.chop_moved,
                        std::array::from_fn(|suit| {
                            u8::try_from(view.play_stacks[suit].len())
                                .expect("a standard stack has at most five cards")
                        }),
                        focus,
                        focus_identity,
                    )
                })
                .flatten()
                // The clue both secures the immediately playable elimination
                // card and promises its delayed focus. Treat it as an urgent
                // play line; strategic coverage can then distinguish it from
                // a direct clue that concentrates both plays in one hand.
                .map(|_| 500)
        });
        let fallback_signals =
            prospective_team_clue_signal_kinds(view, profile, target, clue, &touched);
        let recipient_focus_inversion = fallback_signals.contains(&HGroupMoveKind::FocusInversion);
        let mixed_touch_continuation = matches!(clue, Clue::Suit(_))
            && touched.len() > newly_informed.len()
            && !gotten.contains(&focus);
        if play_score.is_none()
            && save_score.is_none()
            && (recipient_focus_inversion || mixed_touch_continuation)
            && prospective_clue_primary_interpretation(view, profile, target, clue, &touched)
                .is_some_and(|interpretation| {
                    let height = view.play_stacks[focus_identity.suit.index()].len();
                    let predecessors_are_queued =
                        ((height + 1)..usize::from(focus_identity.rank.number())).all(|rank| {
                            let expected = Card::new(focus_identity.suit, Rank::ALL[rank - 1]);
                            baseline_playing
                                .iter()
                                .any(|card| identity_of(view, *card) == Some(expected))
                                || replay
                                    .pending_connections
                                    .iter()
                                    .any(|connection| connection.expected == expected)
                        });
                    interpretation.focus == focus
                        && matches!(
                            interpretation.kind,
                            HGroupClueKind::Play | HGroupClueKind::PlayOrSave
                        )
                        && interpretation.play_identities.contains(focus_identity)
                        && (is_playable_now(view, focus_identity)
                            || predecessors_are_queued
                            || (recipient_focus_inversion
                                && interpretation
                                    .hypotheses
                                    .iter()
                                    .any(|hypothesis| !hypothesis.connection_steps.is_empty())))
                })
        {
            // Recipient replay is the canonical semantic compiler. It can
            // recognize a Focus Inversion or Continuation line whose intermediate
            // promises depend on observer-relative knowledge that the
            // giver-side structural search cannot reconstruct. Admit that
            // same Play interpretation when the recipient retains this focus
            // and identity; hazard validation below still rejects unsafe team
            // projections.
            play_score = Some(
                390 + 2 * u16::try_from(newly_informed.len()).unwrap_or(0)
                    + u16::from(matches!(clue, Clue::Suit(_))),
            );
        }
        if five_chop_move {
            // A number-5 clue whose rightmost 5 is exactly one unclued card
            // from chop is a 5CM, not a Play Clue. In particular, it cannot
            // construct a layered line through the 5's visible identity.
            // Source: https://hanabi.github.io/level-4/#the-5s-chop-move-5cm
            play_score = None;
        }
        if save_score.is_some()
            && matches!(
                (clue, focus_identity.rank),
                (Clue::Rank(Rank::Two), Rank::Two) | (Clue::Rank(Rank::Five), Rank::Five)
            )
        {
            // A number-2 or number-5 clue to an unclued chop card is a Save
            // by definition. Candidate generation must use the same Save
            // precedence as replay interpretation; otherwise a visible
            // delayed connection can incorrectly turn the Save into a Play
            // clue and receive play-line bonuses it does not earn.
            // Sources:
            // - https://hanabi.github.io/level-1/#the-save-principle
            // - https://hanabi.github.io/level-1/#the-2-save
            play_score = None;
        }
        if play_score.is_some()
            && baseline_playing.contains(&focus)
            && !fixed_cards.contains(&focus)
            && !replay.cards.invalidated_focuses.contains(&focus)
            && newly_informed
                .iter()
                .all(|card| *card == focus || baseline_playing.contains(card))
            && (newly_informed.is_empty() || due_connection_cards.contains(&focus))
        {
            // A direct clue on a card already promised to play creates no new
            // action, regardless of whether that promise came from positive
            // information or an invisible connection. Touching only that
            // focus and other already-playing cards is merely extra identity
            // information and fails Minimum Clue Value. A newly touched focus
            // is rejected here only when it is already an explicit active
            // connection card; broader subjective playability must not erase
            // a new Elimination Finesse or other legitimate clue. Required
            // Fixes are handled before ordinary candidate generation above.
            play_score = None;
        }
        let repairable_lie = !is_playable_now(view, focus_identity)
            && rule_enabled(profile, HGroupRuleId::Extras)
            && loaded_connection_plan(
                view,
                None,
                None,
                None,
                view.current_player,
                target,
                focus,
                focus_identity,
                &promptable,
                &baseline_playing,
                &replay.pending_connections,
                std::array::from_fn(|suit| {
                    u8::try_from(view.play_stacks[suit].len())
                        .expect("a standard stack has at most five cards")
                }),
            )
            .is_some_and(|fix| fix.is_some());
        let retouches_older_delayed_focus = is_playable_now(view, focus_identity)
            && touched.iter().copied().any(|card| {
                card != focus
                    && promptable.contains(&card)
                    && identity_of(view, card).is_some_and(|identity| {
                        identity.suit == focus_identity.suit
                            && identity.rank.number() > focus_identity.rank.number() + 1
                    })
            });
        let creates_false_non_focus_self_prompt = is_playable_now(view, focus_identity)
            && touched.iter().copied().any(|card| {
                if card == focus {
                    return false;
                }
                identity_of(view, card).is_some_and(|identity| {
                    let height = view.play_stacks[identity.suit.index()].len();
                    let rank = usize::from(identity.rank.number());
                    if rank <= height + 1 || height >= Rank::ALL.len() {
                        return false;
                    }
                    let expected = Card::new(identity.suit, Rank::ALL[height]);
                    hand.iter().rev().any(|candidate| {
                        candidate.id != card
                            && promptable.contains(&candidate.id)
                            && candidate.clues.allows(expected)
                            && candidate.identity.is_some_and(|actual| {
                                actual != expected && !is_playable_now(view, actual)
                            })
                    })
                })
            });
        if retouches_older_delayed_focus || creates_false_non_focus_self_prompt {
            // Playing the direct focus would leave the older card as a fresh
            // delayed focus, or a newly touched non-focus card would expose a
            // false Self-Prompt. The recipient can then Prompt it even though
            // the giver intended only the direct play.
            continue;
        }
        let is_continuation_clue = rule_enabled(profile, HGroupRuleId::Extras)
            && prospective_clue_signal_kinds(view, profile, target, clue, &touched)
                .contains(&HGroupMoveKind::ContinuationClue);
        if play_score.is_some()
            && !repairable_lie
            && prospective_clue_has_unsafe_connection(
                view,
                profile,
                target,
                focus,
                clue,
                &touched,
                is_playable_now(view, focus_identity) && !is_continuation_clue,
            )
        {
            continue;
        }
        if let Some(mut score) = play_score {
            let target_has_known_trash = target != next_player
                && newly_informed.len() == 1
                && layout.iter().copied().any(|card| {
                    card != focus
                        && gotten.contains(&card)
                        && identity_of(view, card)
                            .is_some_and(|identity| card_is_trash(view, identity))
                });
            if target_has_known_trash {
                // Do not spend a clue on an off-turn one-for-one merely to
                // occupy a player who already has a publicly demonstrated
                // trash discard. Recovering a token first preserves both
                // tempo and the later play clue.
                score = score.saturating_sub(50);
            }
            if old_chop == Some(focus)
                && is_unique_visible(view, focus, focus_identity)
                && (target == next_player
                    || focus_identity.rank != Rank::One
                    || !next_player_has_multi_one)
            {
                // Occupying a player with their playable chop is valuable at
                // every distance, including an off-turn 1. Otherwise an
                // intervening teammate is forced to spend the next clue on
                // that chop, losing the opportunity to find a stronger line.
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
                        && identity_of(view, card)
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
                value: ClueValue::new(score),
                purpose: CluePurpose::Play,
                target,
                save: false,
                // A Continuation Clue can focus an already-playable card while
                // Information Lock still requires the recipient to finish the
                // pre-existing layer first. The clue is a valid Play Clue, but
                // its focus is not the recipient's immediate action.
                // https://hanabi.github.io/extras/play-clues/#the-continuation-clue-touching-both-inside-and-outside-a-layer
                schedule: ClueSchedule::new(
                    false,
                    is_playable_now(view, focus_identity) && !is_continuation_clue,
                ),
                connection_steps: u8::try_from(
                    usize::from(focus_identity.rank.number())
                        .saturating_sub(view.play_stacks[focus_identity.suit.index()].len() + 1),
                )
                .expect("a standard connection has at most four steps"),
                action_coverage: 0,
                convention_action_count: None,
                convention_connection_steps: None,
                recognition: ClueRecognition::GeneratorProof,
            });
        } else if let Some(score) = save_score {
            if !prospective_clue_has_unsafe_connection(
                view, profile, target, focus, clue, &touched, false,
            ) && prospective_clue_marks_focus_saved(view, profile, target, focus, clue, &touched)
            {
                candidates.push(ClueCandidate {
                    action,
                    value: ClueValue::new(score),
                    purpose: CluePurpose::Save,
                    target,
                    save: true,
                    schedule: ClueSchedule::new(
                        focus_identity.rank == Rank::Five || is_critical(view, focus_identity),
                        false,
                    ),
                    connection_steps: 0,
                    action_coverage: 0,
                    convention_action_count: None,
                    convention_connection_steps: None,
                    recognition: ClueRecognition::GeneratorProof,
                });
            }
        }
    }

    let immediately_endangered_targets = candidates
        .iter()
        .filter(|candidate| candidate.save && candidate.target == next_player)
        .map(|candidate| candidate.target)
        .collect::<PlayerSet>();
    for candidate in &mut candidates {
        if immediately_endangered_targets.contains(&candidate.target) {
            // Only the next player's chop is time-sensitive on this turn.
            // This raises the clue's value when the actor is free, but
            // `urgent_save` separately controls whether it may preempt an
            // already-promised play.
            let color_tie_break = u16::from(matches!(
                candidate.action,
                Action::Clue {
                    clue: Clue::Suit(_),
                    ..
                }
            ));
            candidate
                .value
                .set_base(if candidate.immediate_play() { 550 } else { 540 } + color_tie_break);
        }
    }

    if rule_enabled(profile, HGroupRuleId::BasicMoves) {
        for candidate in advanced_clue_candidates(
            view,
            replay,
            &gotten,
            giver_has_playable_now,
            &convention_cards,
            profile,
        ) {
            if let Some(existing) = candidates
                .iter()
                .position(|existing| existing.action == candidate.action)
            {
                // One physical clue can have several convention roles. Basic
                // Play/Save interpretation normally keeps precedence over
                // optional advanced labels. A Fix replaces it because it
                // repairs an active false promise. A Bluff also replaces a
                // delayed Play interpretation: from Bluff Seat, H-Group gives
                // the Bluff precedence over a Layered Finesse, so retaining
                // the latter would score connection steps the clue does not
                // promise.
                // Sources:
                // - https://hanabi.github.io/level-3/#the-fix-clue
                // - https://hanabi.github.io/level-11/#mistaking-a-layered-finesse-for-a-bluff
                let bluff_has_precedence = if candidate.purpose == CluePurpose::Advanced {
                    let Action::Clue { target, clue } = candidate.action else {
                        unreachable!("clue candidates always contain clues");
                    };
                    let touched = view.hands[target.index()]
                        .iter()
                        .filter(|card| card.identity.is_some_and(|identity| clue.matches(identity)))
                        .map(|card| card.id)
                        .collect::<Vec<_>>();
                    prospective_team_clue_signal_kinds(view, profile, target, clue, &touched)
                        .contains(&HGroupMoveKind::Bluff)
                } else {
                    false
                };
                if candidate.purpose == CluePurpose::Fix || bluff_has_precedence {
                    candidates[existing] = candidate;
                }
            } else {
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
                || !candidate.immediate_play()
        });
    }

    let observer_chop = chop(&replay.hands[view.observer.index()], &gotten);
    if candidates.is_empty()
        && (observer_chop.is_none()
            || view.clue_tokens == MAX_CLUE_TOKENS
            || has_out_of_order_prompt(view, &gotten))
    {
        candidates.extend(tempo_clue_candidates(view, replay, &gotten, profile));
    }
    if rule_enabled(profile, HGroupRuleId::Stalling) && view.clue_tokens == 1 {
        // Every clue source, including the fallback Tempo path above, must
        // respect the promise made by deliberately leaving the next player
        // locked at zero clues.
        candidates.retain(|candidate| {
            candidate.purpose == CluePurpose::Fix
                || (!creates_false_anxiety(view, profile, &gotten, candidate)
                    && !creates_false_anxiety_after_forced_play(view, profile, candidate))
        });
    }
    SemanticallyAdmittedCandidates::new(candidates).finalize(deductions, profile)
}

fn fix_condition_is_live(view: &PlayerView, condition: FixCondition) -> bool {
    match condition {
        FixCondition::Unconditional => true,
        FixCondition::FocusIdentity {
            focus, identity, ..
        } => identity_of(view, focus) == Some(identity),
    }
}

pub(super) fn creates_false_anxiety_after_forced_play(
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
    let forced = preferred_due_play_card(next_deductions.view(), &next_inferred, profile);
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
    let selected = preferred_due_play_card(deductions.view(), &inferred, profile);
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
pub(super) fn creates_false_anxiety(
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
pub(super) fn advanced_clue_candidates(
    view: &PlayerView,
    replay: &HGroupState,
    gotten: &CardSet,
    giver_has_playable_now: bool,
    convention_cards: &[HGroupCardInference],
    profile: HGroupProfile,
) -> Vec<ClueCandidate> {
    if view.clue_tokens == 0 {
        return Vec::new();
    }
    let actor_locked = replay.hands[view.observer.index()]
        .iter()
        .all(|card| gotten.contains(card) || replay.cards.chop_moved.contains(card));
    let stalling = replay.early_game || actor_locked || view.clue_tokens == MAX_CLUE_TOKENS;
    let promptable = replay.promptable();
    let previously_fixed = replay.cards.facts.fixed_cards();
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
            .filter(|card| !gotten.contains(card) && !replay.cards.chop_moved.contains(card))
            .collect::<Vec<_>>();
        let newly_informed = touched
            .iter()
            .copied()
            .filter(|card| !promptable.contains(card))
            .collect::<Vec<_>>();
        let identities = touched
            .iter()
            .filter_map(|card| identity_of(view, *card))
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
                identity_of(view, **card).is_some_and(|identity| is_playable_now(view, identity))
            })
            .count();
        let off_chop_five = clue == Clue::Rank(Rank::Five)
            && identities
                .iter()
                .any(|identity| identity.rank == Rank::Five)
            && chop(layout, gotten).is_none_or(|card| !touched.contains(&card));
        let five_chop_moved = (rule_enabled(profile, HGroupRuleId::ChopMoves)
            && clue == Clue::Rank(Rank::Five))
        .then(|| five_chop_moved_card(layout, &touched, gotten))
        .flatten();
        let five_chop_move = five_chop_moved.is_some() && !stalling;
        let five_chop_move_has_value = five_chop_moved
            .and_then(|moved| identity_of(view, moved).map(|identity| (moved, identity)))
            .is_some_and(|(moved, identity)| {
                is_eventually_useful(view, identity)
                    && !view.hands.iter().flatten().any(|card| {
                        card.id != moved
                            && promptable.contains(&card.id)
                            && (card.identity == Some(identity)
                                || convention_cards.iter().any(|note| {
                                    note.card == card.id && note.identities.contains(identity)
                                }))
                    })
            });
        let five_pulled = off_chop_five
            .then(|| five_pulled_card(layout, &touched, gotten))
            .flatten();
        let five_tech_kind = five_pulled
            .and_then(|card| identity_of(view, card))
            .and_then(|identity| {
                let height = view.play_stacks[identity.suit.index()].len();
                let rank = usize::from(identity.rank.number());
                let actor = next_player(view.current_player, view.hands.len());
                if rank <= height {
                    finesse_position(&view.hands[actor.index()], gotten, 2)
                        .and_then(|card| card.identity)
                        .is_some_and(|candidate| is_playable_now(view, candidate))
                        .then_some(HGroupMoveKind::FiveNumberDischarge)
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
                        .then_some(HGroupMoveKind::FiveNumberEjection)
                }
            });
        let clue_focus = focus(layout, &touched, chop(layout, gotten), gotten);
        let recipient_playing = subjective_playable_cards(view, profile, target).map_or_else(
            || replay.cards.already_playing.materialized().clone(),
            |cards| cards.into_iter().collect::<CardSet>(),
        );
        let tempo = newly_touched.is_empty()
            && touched.iter().any(|card| {
                !previously_fixed.contains(card)
                    && !recipient_playing.contains(card)
                    && !replay.cards.forced_playable.contains(card)
                    && !replay.pending_connections.iter().any(|connection| {
                        connection.actor == target && connection.cards.contains(card)
                    })
                    && identity_of(view, *card)
                        .is_some_and(|identity| is_playable_now(view, identity))
            });
        let tempo_focus_is_blocked_from_prompt = clue_focus
            .and_then(|focus| {
                identity_of(view, focus).map(|identity| {
                    (
                        focus,
                        identity,
                        layout.iter().rev().position(|card| *card == focus),
                    )
                })
            })
            .is_some_and(|(_focus, identity, position)| {
                identity.rank != Rank::Five
                    && position.is_some_and(|position| {
                        layout.iter().rev().take(position).any(|card| {
                            promptable.contains(card)
                                && identity_of(view, *card)
                                    .is_some_and(|identity| !is_playable_now(view, identity))
                        })
                    })
            });
        let target_locked = layout.iter().all(|card| gotten.contains(card));
        let tempo_stall_allowed = actor_locked
            || replay.must_clue.contains(&view.observer)
            || (view.clue_tokens == MAX_CLUE_TOKENS && view.turn > 0)
            || view.deck_size <= view.hands.len();
        let fills_in = touched.iter().copied().any(|card| {
            hand.iter()
                .find(|candidate| candidate.id == card)
                .is_some_and(|card| !card.clues.has_positive_clue(clue))
        });
        let false_two_save_on_five = clue == Clue::Rank(Rank::Five)
            && touched
                .iter()
                .all(|card| replay.cards.explicitly_clued.contains(card))
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
                    identity_of(view, target_chop).is_none_or(|actual| !twos.contains(actual))
                })
            });
        if false_two_save_on_five {
            // A repeated 5 transfers the identity of a previously lost saved
            // 2 onto the recipient's chop. The giver sees that chop and must
            // not use the clue as a generic stall when the promise is false.
            continue;
        }
        let stops_bad_existing_play = touched.iter().copied().any(|card| {
            let has_existing_play = replay.cards.already_playing.contains(&card)
                || replay.cards.forced_playable.contains(&card)
                || replay.pending_connections.iter().any(|connection| {
                    connection.actor == target && connection.cards.contains(&card)
                });
            let confirms_existing_promise = convention_cards
                .iter()
                .find(|knowledge| knowledge.card == card)
                .is_some_and(|knowledge| {
                    knowledge
                        .identities
                        .iter()
                        .any(|identity| clue.matches(identity))
                })
                || replay.pending_connections.iter().any(|connection| {
                    connection.actor == target
                        && connection.cards.contains(&card)
                        && clue.matches(connection.expected)
                        && pending_is_active(connection, &replay.pending_connections)
                });
            has_existing_play
                && !confirms_existing_promise
                && identity_of(view, card).is_some_and(|identity| {
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
        let unresolved_same_clue_connector = target == ejection_actor
            && clue_focus
                .and_then(|focus| identity_of(view, focus).map(|identity| (focus, identity)))
                .is_some_and(|(focus, identity)| {
                    let height = view.play_stacks[identity.suit.index()].len();
                    usize::from(identity.rank.number()) > height + 1
                        && touched.iter().copied().any(|card| {
                            card != focus
                                && !promptable.contains(&card)
                                && identity_of(view, card).is_some_and(|connector| {
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
                .all(|card| replay.cards.explicitly_clued.contains(card));
        if no_information_one_fix
            && clue_focus
                .and_then(|focus| identity_of(view, focus))
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
                .all(|card| replay.cards.explicitly_clued.contains(card))
            && ((fills_in && (duplicate_touch || stops_bad_existing_play))
                || no_information_one_fix);
        let five_ejection = matches!(clue, Clue::Suit(_))
            && clue_focus
                .and_then(|focus| identity_of(view, focus).map(|identity| (focus, identity)))
                .is_some_and(|(focus, identity)| {
                    if identity.rank != Rank::Five || replay.cards.explicitly_clued.contains(&focus)
                    {
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
            &replay.cards.explicitly_clued,
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
            &replay.cards.explicitly_clued,
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
        let has_elimination_notes = replay.cards.facts.identity_claims().iter().any(|claim| {
            claim.source == HGroupMoveKind::Elimination && claim.target == Some(target)
        });
        let safe_generic_play = clue_focus
            .and_then(|focus| identity_of(view, focus).map(|identity| (focus, identity)))
            .is_none_or(|(focus, identity)| {
                let height = view.play_stacks[identity.suit.index()].len();
                usize::from(identity.rank.number()) == height + 1
                    || delayed_connection_score(
                        view,
                        profile,
                        target,
                        focus,
                        identity,
                        previously_fixed.contains(&focus)
                            || replay.cards.invalidated_focuses.contains(&focus),
                        touched.len() == 1,
                        &replay.cards.explicitly_clued,
                        &replay.cards.already_playing,
                        &replay.pending_connections,
                        &replay.cards.facts,
                    )
                    .is_some()
                        && !prospective_clue_has_unsafe_connection(
                            view, profile, target, focus, clue, &touched, false,
                        )
            });
        let elimination = has_elimination_notes
            && touched.len() == 1
            && replay.cards.explicitly_clued.contains(&touched[0])
            && fills_in
            && !replay
                .pending_connections
                .iter()
                .any(|connection| connection.focus == touched[0])
            && safe_generic_play;
        let delayed = identities.iter().find(|identity| {
            usize::from(identity.rank.number()) > view.play_stacks[identity.suit.index()].len() + 1
        });
        let respects_good_touch = good_touch(
            view,
            &newly_informed,
            &promptable,
            previously_fixed,
            convention_cards,
        );
        let out_of_order = clue_focus
            .and_then(|focus| identity_of(view, focus).map(|identity| (focus, identity)))
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
                            && identity_of(view, card).is_some_and(|candidate| {
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
                        &replay.cards.already_playing,
                    )
            });
        let bluff_kind = clue_focus
            .and_then(|focus| identity_of(view, focus))
            .and_then(|focus| {
                let actor = next_player(view.current_player, view.hands.len());
                if !bluff_target_order_is_legal(clue, actor, target) {
                    return None;
                }
                let actor_is_loaded = replay.pending_connections.iter().any(|connection| {
                    connection.actor == actor
                        && pending_is_active(connection, &replay.pending_connections)
                }) || replay.hands[actor.index()].iter().any(|card| {
                    (gotten.contains(card) || replay.cards.forced_playable.contains(card))
                        && identity_of(view, *card)
                            .is_some_and(|identity| is_playable_now(view, identity))
                });
                if actor_is_loaded {
                    return None;
                }
                let kind = if bluff_focus_is_one_away(view, focus, gotten) {
                    BluffTargetKind::Ordinary
                } else if clue == Clue::Rank(Rank::Three)
                    && focus.rank == Rank::Three
                    && is_eventually_useful(view, focus)
                    && !is_playable_now(view, focus)
                {
                    BluffTargetKind::Three
                } else {
                    return None;
                };
                view.hands[actor.index()]
                    .iter()
                    .rev()
                    .find(|candidate| !gotten.contains(&candidate.id))
                    .and_then(|candidate| candidate.identity)
                    .filter(|actual| is_playable_now(view, *actual))
                    .filter(|actual| !bluff_play_connects(clue, *actual))
                    .map(|_| kind)
            });
        let distinct_touched_identities = identity_set(identities.iter().copied());
        let every_touched_card_is_playable = identities.len() == touched.len()
            && playable == identities.len()
            && distinct_touched_identities.len() == identities.len();
        let charm_actor = next_player(view.current_player, view.hands.len());
        let charm_double_bluff_actor = next_player(charm_actor, view.hands.len());
        let charm_double_bluff_available =
            finesse_position(&view.hands[charm_double_bluff_actor.index()], gotten, 0)
                .and_then(|card| card.identity)
                .is_some_and(|identity| is_playable_now(view, identity));
        let charm = rule_enabled(profile, HGroupRuleId::Charms)
            && !safe_generic_play
            && !charm_double_bluff_available
            && target != next_player(view.current_player, view.hands.len())
            && clue_focus.is_some_and(|focus| {
                newly_touched.contains(&focus)
                    && !gotten.contains(&focus)
                    && !was_clued_before(view, view.turn, focus)
                    && identity_of(view, focus).is_some_and(|identity| {
                        identity.rank == Rank::Four
                            && view.play_stacks[identity.suit.index()].is_empty()
                    })
                    && finesse_position(&view.hands[charm_actor.index()], gotten, 3)
                        .and_then(|card| card.identity)
                        .is_some_and(|identity| is_playable_now(view, identity))
            });
        let eight_clue_save = rule_enabled(profile, HGroupRuleId::Stalling)
            && !replay.early_game
            && view.clue_tokens == MAX_CLUE_TOKENS
            && clue_focus.is_some_and(|focus| layout.last() != Some(&focus));
        let trash_chop_move = rule_enabled(profile, HGroupRuleId::ChopMoves)
            && prospective_clue_signal_kinds(view, profile, target, clue, &touched)
                .contains(&HGroupMoveKind::TrashChopMove);
        let max_signal = rule_enabled(profile, HGroupRuleId::Extras)
            .then(|| {
                let recipient_signals =
                    prospective_clue_signal_kinds(view, profile, target, clue, &touched);
                prospective_team_clue_signal_kinds(view, profile, target, clue, &touched)
                    .into_iter()
                    .find(|kind| {
                        matches!(
                            kind,
                            HGroupMoveKind::JustInTimeFix
                                | HGroupMoveKind::FakeSave
                                | HGroupMoveKind::SelfColorBluff
                                | HGroupMoveKind::SelfColorDoubleBluff
                                | HGroupMoveKind::UnknownTrashCharm
                                | HGroupMoveKind::JunkCharm
                                | HGroupMoveKind::TrashPull
                                | HGroupMoveKind::OutOfPositionEjection
                                | HGroupMoveKind::OutOfPositionDischarge
                                | HGroupMoveKind::StackedEjection
                                | HGroupMoveKind::StackedDischarge
                                | HGroupMoveKind::TrashPushDischarge
                                | HGroupMoveKind::TrashPushEjection
                                | HGroupMoveKind::BadChopMoveEjection
                                | HGroupMoveKind::BadTrashFinesseEjection
                                | HGroupMoveKind::TrashFinessePushEjection
                                | HGroupMoveKind::RankChoiceEjection
                                | HGroupMoveKind::LieComponentFinesse
                                | HGroupMoveKind::TrashEjection
                                | HGroupMoveKind::ReplayEjection
                                | HGroupMoveKind::PokeEjection
                        ) && (!matches!(
                            kind,
                            HGroupMoveKind::JustInTimeFix | HGroupMoveKind::LieComponentFinesse
                        ) || recipient_signals.contains(kind))
                            && !(recipient_signals.contains(&HGroupMoveKind::PlayClue)
                                && matches!(
                                    kind,
                                    HGroupMoveKind::UnknownTrashDischarge
                                        | HGroupMoveKind::UnknownDupeDischarge
                                        | HGroupMoveKind::OutOfPositionDischarge
                                        | HGroupMoveKind::StackedDischarge
                                ))
                    })
            })
            .flatten();

        let all_previously_touched = newly_touched.is_empty();
        let all_touched_trash = identities.len() == touched.len()
            && identities
                .iter()
                .all(|identity| card_is_trash(view, *identity));
        let replay_ignition = all_previously_touched && every_touched_card_is_playable && !stalling;
        let poke_ignition = all_previously_touched && all_touched_trash && !stalling;
        let trash_ignition = !all_previously_touched
            && all_touched_trash
            && view.deck_size <= view.hands.len()
            && layout
                .iter()
                .copied()
                .find(|card| !gotten.contains(card) && !touched.contains(card))
                .is_none_or(|card| {
                    identity_of(view, card).is_some_and(|identity| {
                        card_is_trash(view, identity) || is_playable_now(view, identity)
                    })
                });
        let double_ignition_available = (1..view.hands.len())
            .filter_map(|distance| {
                let player = (view.current_player.index() + distance) % view.hands.len();
                finesse_position(&view.hands[player], gotten, 0)
                    .and_then(|card| card.identity)
                    .filter(|identity| is_playable_now(view, *identity))
            })
            .count()
            >= 2;
        let distribution = rule_enabled(profile, HGroupRuleId::EndGame)
            && view.deck_size <= view.hands.len()
            && touched.iter().copied().any(|card| {
                let Some(identity) = identity_of(view, card) else {
                    return false;
                };
                is_playable_now(view, identity)
                    && replay.hands.iter().enumerate().any(|(player, other_hand)| {
                        player != target.index()
                            && other_hand.iter().copied().any(|other| {
                                replay.cards.already_playing.contains(&other)
                                    && identity_of(view, other) == Some(identity)
                            })
                            && other_hand
                                .iter()
                                .filter(|other| {
                                    replay.cards.already_playing.contains(other)
                                        && identity_of(view, **other)
                                            .is_some_and(|known| is_playable_now(view, known))
                                })
                                .count()
                                >= 2
                    })
            });
        let classification = if rule_enabled(profile, HGroupRuleId::Ignition)
            && (replay_ignition || poke_ignition || trash_ignition)
            && double_ignition_available
        {
            Some((
                if replay_ignition {
                    HGroupMoveKind::ReplayDoubleIgnition
                } else if poke_ignition {
                    HGroupMoveKind::PokeDoubleIgnition
                } else {
                    HGroupMoveKind::TrashDoubleIgnition
                },
                360,
            ))
        } else if distribution {
            Some((HGroupMoveKind::DistributionClue, 350))
        } else if max_signal == Some(HGroupMoveKind::JustInTimeFix) {
            Some((HGroupMoveKind::JustInTimeFix, 500))
        } else if max_signal == Some(HGroupMoveKind::FakeSave) {
            Some((HGroupMoveKind::FakeSave, 450))
        } else if max_signal == Some(HGroupMoveKind::LieComponentFinesse) {
            // This line is intentionally below every equally efficient
            // truthful Finesse. Whole-line action coverage can still make it
            // beat an unrelated one-for-one play.
            // Source: https://hanabi.github.io/extras/special-finesses/#finesses-with-a-lie-component
            Some((HGroupMoveKind::LieComponentFinesse, 300))
        } else if rule_enabled(profile, HGroupRuleId::BasicStrategy) && fix {
            Some((HGroupMoveKind::FixClue, 500))
        } else if rule_enabled(profile, HGroupRuleId::EjectionsAndDischarges) && five_ejection {
            Some((HGroupMoveKind::Ejection, 290))
        } else if rule_enabled(profile, HGroupRuleId::EjectionsAndDischarges) && unknown_discharge {
            Some((HGroupMoveKind::Discharge, 285))
        } else if bluff_kind.is_some_and(|kind| {
            rule_enabled(profile, HGroupRuleId::Bluffs)
                && (kind == BluffTargetKind::Ordinary
                    || rule_enabled(profile, HGroupRuleId::IntermediateBluffs))
        }) {
            let useful_cards = newly_touched.len().saturating_add(1);
            Some((
                HGroupMoveKind::Bluff,
                330 + 2 * u16::try_from(useful_cards).unwrap_or(u16::MAX),
            ))
        } else if rule_enabled(profile, HGroupRuleId::Elimination) && elimination {
            Some((HGroupMoveKind::Elimination, 230))
        } else if rule_enabled(profile, HGroupRuleId::OutOfOrderPlay) && out_of_order {
            Some((HGroupMoveKind::OccupiedPlay, 220))
        } else if let Some(kind) = max_signal {
            let score = if matches!(
                kind,
                HGroupMoveKind::SelfColorBluff | HGroupMoveKind::SelfColorDoubleBluff
            ) {
                330
            } else if matches!(
                kind,
                HGroupMoveKind::OutOfPositionEjection
                    | HGroupMoveKind::OutOfPositionDischarge
                    | HGroupMoveKind::StackedEjection
                    | HGroupMoveKind::StackedDischarge
                    | HGroupMoveKind::UnknownTrashCharm
                    | HGroupMoveKind::JunkCharm
                    | HGroupMoveKind::TrashPushDischarge
                    | HGroupMoveKind::TrashPushEjection
                    | HGroupMoveKind::BadChopMoveEjection
                    | HGroupMoveKind::BadTrashFinesseEjection
                    | HGroupMoveKind::TrashFinessePushEjection
                    | HGroupMoveKind::RankChoiceEjection
                    | HGroupMoveKind::TrashEjection
                    | HGroupMoveKind::ReplayEjection
                    | HGroupMoveKind::PokeEjection
            ) {
                290
            } else {
                145
            };
            Some((kind, score))
        } else if rule_enabled(profile, HGroupRuleId::TempoClues) && tempo {
            // https://hanabi.github.io/level-6/#the-valuable-tempo-clue
            // https://hanabi.github.io/level-6/#the-tempo-clue-chop-move-tccm
            // Tempo is valuable only for the three enumerated reasons. Merely
            // adding focus information does not satisfy Minimum Clue Value.
            // This branch intentionally precedes Trash Chop Move: Level 6
            // explicitly gives a clue with both appearances the Tempo meaning.
            let valuable = playable >= 2 || tempo_focus_is_blocked_from_prompt || target_locked;
            if valuable {
                Some((HGroupMoveKind::TempoClue, 205))
            } else if tempo_stall_allowed {
                // https://hanabi.github.io/level-6/#the-tempo-clue-stall-a-non-valuable-tempo-clue
                Some((HGroupMoveKind::TempoClue, 90))
            } else {
                Some((HGroupMoveKind::TempoClueChopMove, 180))
            }
        } else if trash_chop_move {
            // https://hanabi.github.io/level-4/#the-trash-chop-move-tcm
            // A TCM both protects the cards to the right of its trash focus
            // and gives the recipient a demonstrated safe discard. Spending
            // the token is urgent when it moves a still-useful chop, but not
            // when the nominal chop has already become trash.
            let protects_useful_chop = chop(layout, gotten)
                .and_then(|card| identity_of(view, card))
                .is_some_and(|identity| is_eventually_useful(view, identity));
            let actor_has_known_trash = replay.hands[view.observer.index()].iter().any(|card| {
                convention_cards
                    .iter()
                    .find(|note| note.card == *card)
                    .is_some_and(|note| {
                        !note.identities.is_empty()
                            && note.identities.iter().all(|identity| {
                                is_convention_trash(view, identity, gotten, convention_cards)
                            })
                    })
            });
            let target_distance = (target.index() + view.hands.len() - view.current_player.index())
                % view.hands.len();
            Some((
                HGroupMoveKind::TrashChopMove,
                if protects_useful_chop && !actor_has_known_trash && target_distance <= 2 {
                    310
                } else {
                    210
                },
            ))
        } else if five_chop_move {
            // A Chop Move on an identity already secured elsewhere protects
            // only convention trash and has no Minimum Clue Value. Do not let
            // the same physical clue fall through to 5 Pull or Play meaning.
            five_chop_move_has_value.then_some((HGroupMoveKind::FiveChopMove, 210))
        } else if rule_enabled(profile, HGroupRuleId::ChopMoves) && all_trash {
            Some((HGroupMoveKind::ChopMove, 210))
        } else if let Some(kind) =
            five_tech_kind.filter(|_| rule_enabled(profile, HGroupRuleId::FiveTech))
        {
            Some((kind, 150))
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
            // A 4 Charm is still a Play Clue on the focused 4, while also
            // producing the next player's Fourth-Finesse-Position play.
            // Value it like an ordinary Play Clue; its named two-action line
            // supplies the efficiency comparison.
            // Source: https://hanabi.github.io/level-23/#the-4-charm
            Some((HGroupMoveKind::Charm, 400))
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
        let Some((kind, mut score)) = classification else {
            continue;
        };
        let replaces_ordinary_play = advanced_kind_replaces_ordinary_play(kind);
        let recognized_stacked_ejection = matches!(
            max_signal,
            Some(HGroupMoveKind::StackedEjection | HGroupMoveKind::StackedDischarge)
        );
        let stacked_ejection_card = recognized_stacked_ejection
            .then(|| prospective_stacked_ejection_card(view, profile, target, clue, &touched))
            .flatten();
        let protects_critical_chop = clue_focus == chop(layout, gotten)
            && clue_focus
                .and_then(|focus| identity_of(view, focus))
                .is_some_and(|identity| identity.rank == Rank::Five || is_critical(view, identity));
        let target_already_has_a_play = replay.hands[target.index()].iter().any(|card| {
            (replay.cards.already_playing.contains(card)
                || replay.cards.forced_playable.contains(card)
                || replay.pending_connections.iter().any(|connection| {
                    connection.actor == target
                        && connection.cards.contains(card)
                        && pending_is_active(connection, &replay.pending_connections)
                }))
                && identity_of(view, *card).is_some_and(|identity| is_playable_now(view, identity))
        });
        let ejection_actor_already_has_a_play =
            replay.hands[ejection_actor.index()].iter().any(|card| {
                (replay.cards.already_playing.contains(card)
                    || replay.cards.forced_playable.contains(card)
                    || replay.pending_connections.iter().any(|connection| {
                        connection.actor == ejection_actor
                            && connection.cards.contains(card)
                            && pending_is_active(connection, &replay.pending_connections)
                    }))
                    && identity_of(view, *card)
                        .is_some_and(|identity| is_playable_now(view, identity))
            });
        let urgently_protects_critical_chop = protects_critical_chop && !target_already_has_a_play;
        let stacked_ejection_adds_an_action = stacked_ejection_card.is_none_or(|ejected| {
            replay
                .pending_connections
                .iter()
                .find_map(|connection| {
                    let position = connection.cards.iter().position(|card| *card == ejected)?;
                    if position == 0 {
                        return Some(false);
                    }
                    Some(
                        connection
                            .cards
                            .first()
                            .and_then(|card| identity_of(view, *card))
                            == Some(connection.expected),
                    )
                })
                .unwrap_or(true)
        });
        let ejection_adds_an_action = if recognized_stacked_ejection {
            // If the loaded connection's first candidate is the promised
            // identity, that candidate would resolve the connection and the
            // second card would never be played. Ejecting the second card
            // therefore adds a real play. If the first candidate is merely a
            // different playable card, ordinary connection continuation was
            // already going to reach the second card, so the Ejection only
            // reorders an existing line.
            stacked_ejection_adds_an_action
        } else {
            !ejection_actor_already_has_a_play
        };
        if replaces_ordinary_play
            && urgently_protects_critical_chop
            && ejection_adds_an_action
            && giver_has_playable_now
        {
            // Ejections and Discharges replace the clue's apparent Play
            // connection. When that move also protects a critical chop from
            // the recipient's next action, it has the urgency of a Save while
            // retaining its advanced meaning. A generic Ejection does not add
            // an action when its blind player was already scheduled to make
            // that same play; in that case Directness prefers the ordinary
            // Save. A Stacked Ejection is the exception because it explicitly
            // changes which loaded card is due first. This special boost is
            // needed only when the clue must beat the giver's otherwise-due
            // play; without that conflict, Directness prefers an available
            // ordinary Save over the more complicated Ejection line.
            score = score.max(450);
        }
        let score_is_low = view.play_stacks.iter().map(Vec::len).sum::<usize>() < 10;
        let unsafe_unsuppressed_play = (kind == HGroupMoveKind::ChopMove && !all_trash)
            || matches!(
                kind,
                HGroupMoveKind::FixClue
                    | HGroupMoveKind::Elimination
                    | HGroupMoveKind::OccupiedPlay
                    | HGroupMoveKind::TempoClue
                    | HGroupMoveKind::TempoClueChopMove
                    | HGroupMoveKind::Extra
                    | HGroupMoveKind::Stall
            )
            || (kind == HGroupMoveKind::FiveStall && !replay.early_game && !score_is_low);
        if !safe_generic_play && unsafe_unsuppressed_play {
            // These moves do not replace a delayed Play interpretation. If
            // the focused card can be read as a Play clue, its ordinary
            // Prompt/Finesse chain must also be valid. Otherwise an advanced
            // stall or chop move can manufacture a false layered finesse.
            continue;
        }
        if !replaces_ordinary_play
            && clue_focus.is_some_and(|focus| {
                prospective_clue_has_unsafe_connection(
                    view, profile, target, focus, clue, &touched, false,
                )
            })
        {
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
            value: ClueValue::new(score + efficiency + u16::from(matches!(clue, Clue::Suit(_)))),
            purpose: if matches!(kind, HGroupMoveKind::SaveClue | HGroupMoveKind::FakeSave) {
                CluePurpose::Save
            } else if kind == HGroupMoveKind::LieComponentFinesse {
                CluePurpose::Play
            } else {
                CluePurpose::Advanced
            },
            target,
            save: matches!(kind, HGroupMoveKind::SaveClue | HGroupMoveKind::FakeSave),
            schedule: ClueSchedule::new(
                kind == HGroupMoveKind::FakeSave || urgently_protects_critical_chop,
                playable > 0,
            ),
            connection_steps: if kind == HGroupMoveKind::LieComponentFinesse {
                clue_focus
                    .and_then(|focus| identity_of(view, focus))
                    .map_or(0, |identity| {
                        u8::try_from(
                            usize::from(identity.rank.number())
                                .saturating_sub(view.play_stacks[identity.suit.index()].len() + 1),
                        )
                        .expect("a standard connection has at most four steps")
                    })
            } else {
                0
            },
            action_coverage: 0,
            convention_action_count: None,
            convention_connection_steps: None,
            recognition: ClueRecognition::GeneratorProof,
        });
    }
    candidates
}

fn advanced_kind_replaces_ordinary_play(kind: HGroupMoveKind) -> bool {
    matches!(
        kind,
        HGroupMoveKind::Ejection
            | HGroupMoveKind::Discharge
            | HGroupMoveKind::FiveColorEjection
            | HGroupMoveKind::UnknownTrashDischarge
            | HGroupMoveKind::UnknownDupeDischarge
            | HGroupMoveKind::OutOfPositionEjection
            | HGroupMoveKind::OutOfPositionDischarge
            | HGroupMoveKind::StackedEjection
            | HGroupMoveKind::StackedDischarge
            | HGroupMoveKind::TrashPushDischarge
            | HGroupMoveKind::TrashPushEjection
            | HGroupMoveKind::BadChopMoveEjection
            | HGroupMoveKind::BadTrashFinesseEjection
            | HGroupMoveKind::TrashFinessePushEjection
            | HGroupMoveKind::RankChoiceEjection
            | HGroupMoveKind::TrashEjection
            | HGroupMoveKind::ReplayEjection
            | HGroupMoveKind::PokeEjection
            | HGroupMoveKind::LieComponentFinesse
            | HGroupMoveKind::Charm
    )
}

pub(super) fn bluff_focus_is_one_away(view: &PlayerView, focus: Card, gotten: &CardSet) -> bool {
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

pub(super) fn out_of_order_connections_accounted(
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
            .any(|card| card != focus && identity_of(view, card) == Some(needed))
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
pub(super) fn save_clue_score(
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
    // Save Principle, with next-player timing as a deterministic tie-break.
    // Whether the Save may preempt an existing play obligation is represented
    // separately on `ClueCandidate`.
    Some(if target == next_player { 450 } else { 400 })
}

pub(super) fn has_false_two_save_prompt(
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
pub(super) fn play_clue_score(
    view: &PlayerView,
    profile: HGroupProfile,
    target: PlayerId,
    focus: CardId,
    focus_identity: Card,
    clue: Clue,
    newly_touched: &[CardId],
    clue_touch_count: usize,
    explicitly_clued: &CardSet,
    fixed_cards: &CardSet,
    invalidated_focuses: &CardSet,
    already_playing: &CardSet,
    pending_connections: &[ConnectionObligation],
    convention_cards: &[HGroupCardInference],
    convention_facts: &ConventionFacts,
) -> Option<u16> {
    let trash_collateral = known_trash_collateral(view, focus, focus_identity, clue, newly_touched);
    let ordinary_touches = newly_touched
        .iter()
        .copied()
        .filter(|card| !trash_collateral.contains(card))
        .collect::<Vec<_>>();
    if !good_touch(
        view,
        &ordinary_touches,
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
            identity_of(view, card).is_some_and(|connector| {
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
            profile,
            target,
            focus,
            focus_identity,
            fixed_cards.contains(&focus) || invalidated_focuses.contains(&focus),
            clue_touch_count == 1,
            explicitly_clued,
            already_playing,
            pending_connections,
            convention_facts,
        )?
    };
    Some(
        base + 2 * u16::try_from(newly_touched.len()).unwrap_or(0)
            + u16::from(matches!(clue, Clue::Suit(_)))
            + KNOWN_TRASH_COLLATERAL_BONUS
                .saturating_mul(u16::try_from(trash_collateral.len()).unwrap_or(u16::MAX)),
    )
}

pub(super) fn known_trash_collateral(
    view: &PlayerView,
    focus: CardId,
    focus_identity: Card,
    clue: Clue,
    newly_touched: &[CardId],
) -> Vec<CardId> {
    if clue != Clue::Suit(focus_identity.suit)
        || focus_identity.rank != Rank::Five
        || !is_playable_now(view, focus_identity)
    {
        return Vec::new();
    }
    newly_touched
        .iter()
        .copied()
        .filter(|card| *card != focus)
        .filter(|card| {
            identity_of(view, *card).is_some_and(|identity| !is_eventually_useful(view, identity))
        })
        .collect()
}

pub(super) fn good_touch(
    view: &PlayerView,
    newly_touched: &[CardId],
    explicitly_clued: &CardSet,
    fixed_cards: &CardSet,
    convention_cards: &[HGroupCardInference],
) -> bool {
    let known_identity = |card: CardId| {
        identity_of(view, card).or_else(|| {
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

/// Finds the card promised by an Elimination Finesse for `focus_identity`.
///
/// Elimination notes are disjunctive: the noted identity is somewhere among
/// the cards recorded by the original signal. A Finesse on the next card in
/// that suit promises the oldest still-possible noted card, skipping
/// Chop-Moved cards unless every remaining candidate is Chop-Moved.
#[allow(clippy::too_many_arguments)]
pub(super) fn elimination_finesse_connection(
    view: &PlayerView,
    hands: &[Vec<CardId>],
    clue_facts: Option<&[ClueFacts]>,
    historical: Option<HistoricalView<'_>>,
    convention_facts: &ConventionFacts,
    chop_moved: &CardSet,
    stack_heights: [u8; 5],
    focus: CardId,
    focus_identity: Card,
) -> Option<(PlayerId, CardId, Card)> {
    let stack_height = stack_heights[focus_identity.suit.index()];
    if focus_identity.rank.number() != stack_height + 2 {
        return None;
    }
    let expected = Card::new(focus_identity.suit, Rank::ALL[usize::from(stack_height)]);
    let direct_facts = |card: CardId| {
        clue_facts.map_or_else(
            || {
                view.hands
                    .iter()
                    .flatten()
                    .find(|candidate| candidate.id == card)
                    .map_or_else(ClueFacts::default, |candidate| candidate.clues)
            },
            |facts| facts[card.index()],
        )
    };
    let visible_identity = |card: CardId| {
        historical.map_or_else(|| identity_of(view, card), |history| history.identity(card))
    };

    convention_facts
        .identity_claims()
        .iter()
        .rev()
        .find_map(|claim| {
            if claim.source != HGroupMoveKind::Elimination || claim.identity != expected {
                return None;
            }
            let player = claim.target?;
            let hand = hands.get(player.index())?;
            let candidates = hand
                .iter()
                .copied()
                .filter(|card| {
                    *card != focus
                        && claim.cards.contains(card)
                        && direct_facts(*card).allows(expected)
                })
                .collect::<Vec<_>>();
            let promised = candidates
                .iter()
                .copied()
                .find(|card| !chop_moved.contains(card))
                .or_else(|| candidates.first().copied())?;
            (player == view.observer || visible_identity(promised) == Some(expected))
                .then_some((player, promised, expected))
        })
}

#[allow(clippy::too_many_arguments, clippy::too_many_lines)]
pub(super) fn delayed_connection_score(
    view: &PlayerView,
    profile: HGroupProfile,
    target: PlayerId,
    focus: CardId,
    focus_identity: Card,
    focus_was_fixed: bool,
    allow_queued_prefix: bool,
    explicitly_clued: &CardSet,
    already_playing: &CardSet,
    pending_connections: &[ConnectionObligation],
    convention_facts: &ConventionFacts,
) -> Option<u16> {
    let stack_height = view.play_stacks[focus_identity.suit.index()].len();
    if usize::from(focus_identity.rank.number()) <= stack_height + 1
        || stack_height >= Rank::ALL.len()
    {
        return None;
    }
    let pending_wholly_queued = ((stack_height + 1)..usize::from(focus_identity.rank.number()))
        .all(|needed_rank| {
            pending_identity_is_queued(
                pending_connections,
                Card::new(focus_identity.suit, Rank::ALL[needed_rank - 1]),
            )
        });
    if pending_wholly_queued {
        if explicitly_clued.contains(&focus) && !focus_was_fixed {
            // Every predecessor was already promised, so Good Touch would
            // make this already-gotten focus play without the fill-in. The
            // clue creates no action and fails Minimum Clue Value.
            // Source: https://hanabi.github.io/level-1/#minimum-clue-value-principle
            return None;
        }
        return Some(390);
    }
    let first_unqueued_rank = if allow_queued_prefix {
        ((stack_height + 1)..usize::from(focus_identity.rank.number())).find(|needed_rank| {
            let needed = Card::new(focus_identity.suit, Rank::ALL[*needed_rank - 1]);
            !identity_is_queued_before_target(
                view,
                view.current_player,
                target,
                already_playing,
                pending_connections,
                needed,
            )
        })
    } else {
        Some(stack_height + 1)
    };
    let Some(first_unqueued_rank) = first_unqueued_rank else {
        if explicitly_clued.contains(&focus) && !focus_was_fixed {
            // `already_playing` can account for every predecessor without a
            // pending connection record. As above, identifying an
            // already-gotten successor does not create a Prompt or any other
            // new play commitment.
            return None;
        }
        // A newly touched successor still creates a future play commitment
        // when its predecessor was already scheduled. Only an already-gotten
        // focus is a no-value fill-in, and that case returned above. This is
        // the distinction between a legal rank-2 clue that gets a fresh Red 2
        // behind a known Red 1 and an illegal reclue of an existing Red 3.
        return Some(390);
    };
    // Already-promised plays advance the delayed line. Search for the first
    // genuinely missing connector instead of trying to create a duplicate
    // Prompt/Finesse for an identity the team is already committed to play.
    let connector = Card::new(focus_identity.suit, Rank::ALL[first_unqueued_rank - 1]);
    if rule_enabled(profile, HGroupRuleId::Extras)
        && loaded_connection_plan(
            view,
            None,
            None,
            None,
            view.current_player,
            target,
            focus,
            focus_identity,
            explicitly_clued,
            already_playing,
            pending_connections,
            std::array::from_fn(|suit| {
                u8::try_from(view.play_stacks[suit].len())
                    .expect("a standard stack has at most five cards")
            }),
        )
        .is_some()
    {
        return Some(365);
    }
    let first = (view.current_player.index() + 1) % view.hands.len();
    let ordinary_search_len = if rule_enabled(profile, HGroupRuleId::BasicMoves) {
        (target.index() + view.hands.len() - first) % view.hands.len() + 1
    } else {
        1
    };
    let prompt_allows = |card: &ObservedCard, identity: Card| {
        convention_facts
            .known_identity(card.id)
            .map_or_else(|| card.clues.allows(identity), |known| known == identity)
    };
    let direct_reverse_connection = rule_enabled(profile, HGroupRuleId::BasicMoves)
        && (ordinary_search_len..view.hands.len()).any(|distance| {
            let player = (first + distance) % view.hands.len();
            if player == target.index() {
                return false;
            }
            let prompt = view.hands[player].iter().rev().any(|card| {
                card.id != focus
                    && explicitly_clued.contains(&card.id)
                    && !already_playing.contains(&card.id)
                    && prompt_allows(card, connector)
                    && card.identity == Some(connector)
            });
            let finesse = view.hands[player]
                .iter()
                .rev()
                .find(|card| {
                    card.id != focus
                        && !explicitly_clued.contains(&card.id)
                        && !already_playing.contains(&card.id)
                })
                .and_then(|card| card.identity)
                == Some(connector);
            prompt || finesse
        });
    let search_len = if explicitly_clued.contains(&focus) || direct_reverse_connection {
        // A fill-in clue on an already gotten delayed card cannot make its
        // recipient play immediately. Its connection may therefore wrap
        // beyond that recipient, as in a Green-4 reclue that finesses the
        // following player's Green 2 and Green 3. A newly clued card also
        // wraps when the recipient can see the exact connector either on a
        // later player's immediate Finesse Position (a Level-2 Reverse
        // Finesse) or as an already-clued Prompt. This prevents deeper hidden
        // layers from manufacturing a reverse line while preserving direct,
        // recipient-visible reverse connections.
        view.hands.len()
    } else {
        ordinary_search_len
    };
    let player_order = (0..search_len)
        .map(|distance| (first + distance) % view.hands.len())
        .collect::<Vec<_>>();
    let has_prompt = player_order.iter().copied().any(|player| {
        let candidates = view.hands[player]
            .iter()
            .rev()
            .filter(|card| {
                card.id != focus
                    && explicitly_clued.contains(&card.id)
                    && !already_playing.contains(&card.id)
                    && prompt_allows(card, connector)
            })
            .collect::<Vec<_>>();
        candidates
            .iter()
            .position(|card| card.identity == Some(connector))
            .is_some_and(|correct| {
                candidates[..correct].iter().all(|card| {
                    card.identity
                        .is_some_and(|identity| is_playable_now(view, identity))
                })
            })
    });
    let has_finesse = !has_prompt
        && player_order.iter().copied().any(|player| {
            player != target.index()
                && visible_finesse_connects(
                    view,
                    &view.hands[player],
                    connector,
                    focus,
                    explicitly_clued,
                    already_playing,
                    rule_enabled(profile, HGroupRuleId::SpecialFinesses),
                )
        });
    if !has_prompt && !has_finesse {
        return None;
    }

    for needed_rank in (first_unqueued_rank + 1)..usize::from(focus_identity.rank.number()) {
        let needed = Card::new(focus_identity.suit, Rank::ALL[needed_rank - 1]);
        let already_queued =
            pending_identity_is_queued(pending_connections, needed)
                || view.hands.iter().flatten().any(|card| {
                    already_playing.contains(&card.id) && card.identity == Some(needed)
                });
        if already_queued {
            // A later connector can already be convention-bound even when an
            // earlier connector is the first missing step. Do not demand a
            // second Prompt/Finesse for an identity the team will already
            // play before the delayed focus.
            continue;
        }
        let false_prompt = view.hands.iter().flatten().any(|card| {
            card.id != focus
                && explicitly_clued.contains(&card.id)
                && !already_playing.contains(&card.id)
                && prompt_allows(card, needed)
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
        let explicitly_available = view
            .hands
            .iter()
            .flatten()
            .any(|card| explicitly_clued.contains(&card.id) && card.identity == Some(needed));
        let layered_finesse_available = player_order.iter().copied().any(|player| {
            player != target.index()
                && visible_finesse_connects(
                    view,
                    &view.hands[player],
                    needed,
                    focus,
                    explicitly_clued,
                    already_playing,
                    rule_enabled(profile, HGroupRuleId::SpecialFinesses),
                )
        });
        if !explicitly_available && !layered_finesse_available {
            return None;
        }
    }
    Some(if has_prompt { 380 } else { 370 })
}

/// Validates a delayed line through a player who is already occupied. Max
/// permits one repairable lie inside the finesse layer when the next player
/// can fill in that bad card before its owner acts.
#[allow(
    clippy::option_option,
    clippy::too_many_arguments,
    clippy::too_many_lines
)]
pub(super) fn loaded_connection_plan(
    view: &PlayerView,
    historical_hands: Option<&[Vec<CardId>]>,
    historical_facts: Option<&[ClueFacts]>,
    historical_view: Option<HistoricalView<'_>>,
    giver: PlayerId,
    target: PlayerId,
    focus: CardId,
    focus_identity: Card,
    gotten: &CardSet,
    already_playing: &CardSet,
    pending: &[ConnectionObligation],
    mut stack_heights: [u8; 5],
) -> Option<Option<RequiredFix>> {
    let visible_identity = |card| {
        historical_view.map_or_else(|| identity_of(view, card), |history| history.identity(card))
    };
    let current_hands;
    let hands = if let Some(hands) = historical_hands {
        hands
    } else {
        current_hands = view
            .hands
            .iter()
            .map(|hand| hand.iter().map(|card| card.id).collect::<Vec<_>>())
            .collect::<Vec<_>>();
        &current_hands
    };
    let target_loaded = pending
        .iter()
        .any(|connection| connection.actor == target && pending_is_active(connection, pending))
        || (gotten.contains(&focus)
            && historical_facts.is_some_and(|facts| {
                IdentitySet::from_mask(facts[focus.index()].identity_mask())
                    == IdentitySet::singleton(focus_identity)
            }));
    if !target_loaded {
        return None;
    }

    let fixer = next_player(giver, hands.len());
    let mut scheduled = CardSet::default();
    let mut required_fix = None;
    let height = stack_heights[focus_identity.suit.index()];
    for rank in (height + 1)..focus_identity.rank.number() {
        let expected = Card::new(focus_identity.suit, Rank::ALL[usize::from(rank - 1)]);
        if pending_identity_is_queued(pending, expected)
            || already_playing
                .iter()
                .any(|card| visible_identity(*card) == Some(expected))
        {
            stack_heights[expected.suit.index()] = expected.rank.number();
            continue;
        }

        let prompted = hands.iter().flatten().any(|card| {
            *card != focus
                && gotten.contains(card)
                && !already_playing.contains(card)
                && visible_identity(*card) == Some(expected)
        });
        if prompted {
            stack_heights[expected.suit.index()] = expected.rank.number();
            continue;
        }

        let mut found = false;
        let mut observer_fallback = None;
        for (actor_index, hand) in hands.iter().enumerate() {
            if actor_index == target.index() {
                continue;
            }
            let actor = PlayerId::new(
                u8::try_from(actor_index).expect("standard Hanabi has at most five players"),
            );
            let candidates = hand
                .iter()
                .rev()
                .filter(|card| {
                    **card != focus
                        && !gotten.contains(*card)
                        && !already_playing.contains(*card)
                        && !scheduled.contains(*card)
                })
                .collect::<Vec<_>>();
            if actor == view.observer && giver != view.observer {
                observer_fallback = observer_fallback.or_else(|| candidates.first().copied());
                continue;
            }
            let Some(position) = candidates
                .iter()
                .position(|card| visible_identity(**card) == Some(expected))
            else {
                continue;
            };
            let mut simulated = stack_heights;
            let mut candidate_fix = required_fix;
            let mut valid = true;
            for card in &candidates[..position] {
                let Some(identity) = visible_identity(**card) else {
                    valid = false;
                    break;
                };
                if is_playable_at(simulated, identity) {
                    simulated[identity.suit.index()] = identity.rank.number();
                    continue;
                }
                let actor_distance = (actor.index() + hands.len() - giver.index()) % hands.len();
                let clues = historical_facts.map_or_else(
                    || {
                        view.hands
                            .iter()
                            .flatten()
                            .find(|candidate| candidate.id == **card)
                            .map_or_else(ClueFacts::default, |candidate| candidate.clues)
                    },
                    |facts| facts[card.index()],
                );
                let repair_clue_available = [Clue::Suit(identity.suit), Clue::Rank(identity.rank)]
                    .into_iter()
                    .any(|clue| !clues.has_positive_clue(clue));
                if candidate_fix.is_some()
                    || fixer == actor
                    || actor_distance <= 1
                    || !repair_clue_available
                {
                    valid = false;
                    break;
                }
                candidate_fix = Some(RequiredFix {
                    actor: fixer,
                    target: actor,
                    focus: **card,
                    identity,
                });
            }
            if valid {
                scheduled.extend(candidates[..=position].iter().map(|card| **card));
                required_fix = candidate_fix;
                stack_heights = simulated;
                stack_heights[expected.suit.index()] = expected.rank.number();
                found = true;
                break;
            }
        }
        if !found {
            if let Some(card) = observer_fallback {
                scheduled.insert(*card);
                stack_heights[expected.suit.index()] = expected.rank.number();
                found = true;
            }
        }
        if !found {
            return None;
        }
    }
    Some(required_fix)
}

/// Whether one visible hand supplies a valid Finesse or Layered Finesse up to
/// `expected`. Cards before the connector must themselves play successfully in
/// finesse order; otherwise the clue would create a known misplay.
#[allow(clippy::too_many_arguments)]
pub(super) fn visible_finesse_connects(
    view: &PlayerView,
    hand: &[ObservedCard],
    expected: Card,
    focus: CardId,
    gotten: &CardSet,
    already_playing: &CardSet,
    special_finesses: bool,
) -> bool {
    let mut stack_heights = std::array::from_fn(|suit| {
        u8::try_from(view.play_stacks[suit].len()).expect("a Hanabi stack has at most five cards")
    });
    for (position, card) in hand
        .iter()
        .rev()
        .filter(|card| {
            card.id != focus && !gotten.contains(&card.id) && !already_playing.contains(&card.id)
        })
        .enumerate()
    {
        let Some(identity) = card.identity else {
            return false;
        };
        if identity == expected {
            return position == 0 || special_finesses;
        }
        if position > 0 && !special_finesses {
            return false;
        }
        if !is_playable_at(stack_heights, identity) {
            return false;
        }
        stack_heights[identity.suit.index()] = identity.rank.number();
    }
    false
}

pub(super) fn tempo_clue_candidates(
    view: &PlayerView,
    replay: &HGroupState,
    gotten: &CardSet,
    profile: HGroupProfile,
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
        let recipient_already_knows_playable = subjective_playable_cards(view, profile, target)
            .is_none_or(|playing| playing.contains(&focus));
        if is_playable_now(view, identity) && !recipient_already_knows_playable {
            // https://hanabi.github.io/level-6/#the-tempo-clue
            // A Tempo Clue accelerates an already-clued card that was not
            // already known to be playable. Re-cluing a card already bound to
            // play is a Burn Clue and is not admitted by this fallback.
            candidates.push(ClueCandidate {
                action,
                value: ClueValue::new(100 + u16::from(matches!(clue, Clue::Suit(_)))),
                purpose: CluePurpose::Tempo,
                target,
                save: false,
                schedule: ClueSchedule::new(false, true),
                connection_steps: 0,
                action_coverage: 0,
                convention_action_count: None,
                convention_connection_steps: None,
                recognition: ClueRecognition::GeneratorProof,
            });
        }
    }
    candidates
}

pub(super) fn has_out_of_order_prompt(view: &PlayerView, gotten: &CardSet) -> bool {
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
pub(super) fn infer_clue_to_self(
    deductions: &LogicalDeductions,
    clue: &HGroupClueInterpretation,
    explicitly_clued: &CardSet,
    inferred: &mut HGroupInferences,
) {
    if inferred.signals.iter().any(|signal| {
        signal.turn == clue.turn
            && matches!(
                signal.kind,
                HGroupMoveKind::Ejection
                    | HGroupMoveKind::Discharge
                    | HGroupMoveKind::FiveColorEjection
                    | HGroupMoveKind::UnknownTrashDischarge
                    | HGroupMoveKind::UnknownDupeDischarge
            )
    }) {
        // These moves use the clue only as an instruction to another reacting
        // player. They do not also promise that the clue focus should play.
        return;
    }
    if let Some(bluff_card) = inferred.signals.iter().find_map(|signal| {
        (signal.kind == HGroupMoveKind::Bluff
            && signal.cards.len() >= 2
            && signal.cards.last() == Some(&clue.focus)
            && signal.turn >= clue.turn)
            .then(|| signal.cards.first().copied())
            .flatten()
    }) {
        if deductions.view().hands[deductions.view().observer.index()]
            .iter()
            .any(|card| card.id == bluff_card)
            && !inferred.playable_now.contains(&bluff_card)
        {
            inferred.playable_now.push(bluff_card);
        }
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
        HGroupClueKind::PlayOrSave if inferred.is_saved(clue.focus) => return,
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
    // Once connecting plays made after this clue have brought any promised
    // focus identity into the playable position, the focus is due. This must
    // use the current stack rather than only identities that were immediately
    // playable when the clue was given: an existing multi-card line can move
    // a delayed focus several ranks while the clue waits.
    let demonstrated_focus = IdentitySet::from_mask(
        focus_possibilities
            .iter()
            .filter(|identity| is_playable_now(view, *identity))
            .fold(0, |mask, identity| mask | (1 << identity.index())),
    );
    let all_possibilities_completed = demonstrated_focus == focus_possibilities
        && demonstrated_focus.iter().all(|identity| {
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
    let completed_good_touch_chain = matches!(clue.clue, Clue::Suit(_))
        && focus_possibilities.iter().next().is_some_and(|first| {
            focus_possibilities
                .iter()
                .all(|identity| identity.suit == first.suit)
        })
        && clue.touched.iter().copied().any(|card| {
            card != clue.focus
                && explicitly_clued.contains(&card)
                && inferred
                    .cards
                    .iter()
                    .any(|note| note.card == card && note.identities == focus_possibilities)
        })
        && demonstrated_focus.iter().any(|identity| {
            identity.rank.index()
                == focus_possibilities
                    .iter()
                    .map(|candidate| candidate.rank.index())
                    .min()
                    .expect("focus possibilities are non-empty")
                && identity
                    .rank
                    .index()
                    .checked_sub(1)
                    .is_some_and(|previous| {
                        let connector = Card::new(identity.suit, Rank::ALL[previous]);
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
                    })
        });
    // https://hanabi.github.io/level-1/#good-touch-principle
    // A same-suit clue can correlate two previously gotten cards as the next
    // two ranks. Once the predecessor of the lower identity plays, the focus
    // is that lower identity and the matching non-focus card is its successor.
    // This exception does not apply to a rank clue whose alternatives span
    // suits; one demonstrated connector cannot resolve those alternatives.
    let completed_connection = all_possibilities_completed || completed_good_touch_chain;
    if allow_direct_play
        && completed_connection
        && !demonstrated_focus.is_empty()
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
    } else if allow_direct_play && live_direct.is_empty() {
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
