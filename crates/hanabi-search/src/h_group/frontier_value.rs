//! Compare resources at a projection frontier separately from root clue labels.
//! Unknown draws create opportunities/contingencies, never concrete identities.
//! Strategy: <https://hanabi.github.io/beginner/other-general-strategy#give-play-clues-over-save-clues>

use super::interpretation::h_group_clue_candidates;
use super::{
    HGroupProfile, PerspectiveDepth, PerspectiveProjector, infer_h_group_from_replay,
    is_eventually_useful, is_playable_now, was_clued_before,
};
use crate::{IdentitySet, LogicalDeductions, ProjectedPositionValue};
use hanabi_core::{Action, Card, PlayerId, PlayerView, Rank, Suit};

fn narrow(value: usize) -> u8 {
    u8::try_from(value).unwrap_or(u8::MAX)
}

fn unseen(view: &PlayerView, identity: Card) -> usize {
    usize::from(identity.rank.copies()).saturating_sub(
        view.hands
            .iter()
            .flatten()
            .filter(|card| card.identity == Some(identity))
            .count()
            + view
                .discard_pile
                .iter()
                .filter(|(_, card)| *card == identity)
                .count()
            + usize::from(
                view.play_stacks[identity.suit.index()].len()
                    >= usize::from(identity.rank.number()),
            ),
    )
}

fn critical(view: &PlayerView, identity: Card) -> bool {
    is_eventually_useful(view, identity)
        && view
            .discard_pile
            .iter()
            .filter(|(_, card)| *card == identity)
            .count()
            + 1
            >= usize::from(identity.rank.copies())
}

/// Feasibility, not a promise: all predecessors are played or visible.
/// In particular, an ambiguous saved 2 is not a long-term hand burden when
/// every remaining 1 is visible and can be obtained by the team.
fn can_play_soon(view: &PlayerView, domain: IdentitySet, secured: IdentitySet) -> bool {
    !domain.is_empty()
        && domain.iter().all(|identity| {
            !is_eventually_useful(view, identity)
                || ((view.play_stacks[identity.suit.index()].len() + 1)
                    ..usize::from(identity.rank.number()))
                    .all(|rank| {
                        let predecessor = Card::new(identity.suit, Rank::ALL[rank - 1]);
                        secured.contains(predecessor)
                            || view
                                .hands
                                .iter()
                                .flatten()
                                .any(|card| card.identity == Some(predecessor))
                    })
        })
}

#[allow(clippy::too_many_lines)]
pub(super) fn evaluate(
    source: &PlayerView,
    frontier: &PlayerView,
    profile: HGroupProfile,
    root: Action,
) -> Option<ProjectedPositionValue> {
    let mut value = ProjectedPositionValue {
        score: narrow(frontier.play_stacks.iter().map(Vec::len).sum()),
        clues: frontier.clue_tokens,
        ..ProjectedPositionValue::default()
    };
    let mut secured = IdentitySet::default();
    let mut committed = IdentitySet::default();
    let mut positional = IdentitySet::default();
    let mut clued_domains = Vec::new();
    let mut mandatory_clues = 0_u8;
    let mut exposed_chops = IdentitySet::default();
    for player in 0..frontier.hands.len() {
        let actor = PlayerId::new(narrow(player));
        let (d, replay) = PerspectiveProjector::new(frontier, profile)
            .project(actor, PerspectiveDepth::NestedRecipients)?;
        let fresh_trash = super::decision::fresh_trash_chop_move_focus(d.view(), &replay).is_some();
        let inferred = infer_h_group_from_replay(&d, replay, profile);
        // Charge the worsening introduced by protection, not every existing
        // chop. Ordinary draws/plays change chop too, without constituting a
        // protection exchange. A queued play delays this liability; it does
        // not erase it. Unknown identities are never filled from the deck.
        let has_moved_card = frontier.hands[player]
            .iter()
            .any(|card| inferred.chop_moved.contains(&card.id));
        let safe_discard =
            super::decision::convention_known_trash_discard(d.view(), &inferred).is_some();
        if let Some((before_d, before_r)) = (has_moved_card && !safe_discard && !fresh_trash)
            .then(|| {
                PerspectiveProjector::new(source, profile)
                    .project(actor, PerspectiveDepth::NestedRecipients)
            })
            .flatten()
        {
            let before = infer_h_group_from_replay(&before_d, before_r, profile);
            let old = before.chops.get(player).copied().flatten();
            let new = inferred.chops.get(player).copied().flatten();
            if old != new && old.is_some_and(|card| inferred.chop_moved.contains(&card)) {
                if let Some(identity) = worsened_chop_exposure(source, frontier, profile, old, new)
                {
                    exposed_chops = exposed_chops.union(IdentitySet::singleton(identity));
                }
            }
        }
        if let Some(identity) =
            super::finesse_position(&frontier.hands[player], &inferred.gotten(), 0)
                .and_then(|card| card.identity)
                .filter(|identity| is_playable_now(frontier, *identity))
        {
            positional = positional.union(IdentitySet::singleton(identity));
        }
        if inferred.must_clue.contains(&actor) {
            mandatory_clues = mandatory_clues.saturating_add(1);
        }
        if inferred.playable_now.is_empty() {
            if let Some(chop) = inferred.chops.get(player).copied().flatten() {
                if frontier.hands[player]
                    .iter()
                    .find(|card| card.id == chop)
                    .and_then(|card| card.identity)
                    .is_some_and(|identity| critical(frontier, identity))
                {
                    value.exposed_critical_chops = value.exposed_critical_chops.saturating_add(1);
                }
            }
        }
        for card in &inferred.cards {
            let established = card.promised_identity.or_else(|| {
                (card.identities.len() == 1)
                    .then(|| card.identities.iter().next())
                    .flatten()
            });
            // A held, clued face is protected, but is not necessarily an
            // executable promise. Keep those concepts separate. Reviewed
            // p4v0s9 turn 10: 4s establishes p3 -> p4, whereas 2s merely
            // retains the previously touched ambiguous purple card.
            // https://hanabi.github.io/level-1/#minimum-clue-value-principle
            if let Some(identity) = established
                .or_else(|| {
                    inferred
                        .playable_now
                        .contains(&card.card)
                        .then(|| super::identity_of(frontier, card.card))
                        .flatten()
                })
                .filter(|identity| is_eventually_useful(frontier, *identity))
                .filter(|identity| {
                    super::identity_of(frontier, card.card).is_none_or(|actual| actual == *identity)
                })
            {
                committed = committed.union(IdentitySet::singleton(identity));
            }
            let promised = established.or_else(|| {
                // Protecting a useful future card is still progress even
                // when it cannot play soon. Otherwise a valuable Save
                // would count only as a hand blockage in this assessment.
                was_clued_before(frontier, frontier.turn, card.card)
                    .then(|| super::identity_of(frontier, card.card))
                    .flatten()
            });
            if let Some(identity) = promised
                .filter(|identity| is_eventually_useful(frontier, *identity))
                // A recipient's mistaken promise is not a secured physical
                // card. Use only faces visible in the forecasting observer's
                // view, never the simulator's hidden hand or future deck.
                .filter(|identity| {
                    super::identity_of(frontier, card.card).is_none_or(|actual| actual == *identity)
                })
            {
                secured = secured.union(IdentitySet::singleton(identity));
            }
            if was_clued_before(frontier, frontier.turn, card.card) {
                clued_domains.push(card.identities);
            }
        }
    }
    // A known connector in the root observer's own hand has no physical face
    // in this view, but its established promise is still available. Gather
    // every secured identity before judging any hand, avoiding seat-order
    // dependence and a false blockage for delayed plays through that card.
    value.blocked_clued_cards = narrow(
        clued_domains
            .into_iter()
            .filter(|domain| !can_play_soon(frontier, *domain, secured))
            .count(),
    );
    value.secured_future_plays = narrow(secured.len());
    // All prerequisites must also be established, not just visible, saved,
    // or guessed on a future draw. Count identities once across the team.
    value.committed_future_plays = narrow(
        committed
            .iter()
            .filter(|identity| {
                ((frontier.play_stacks[identity.suit.index()].len() + 1)
                    ..usize::from(identity.rank.number()))
                    .all(|rank| committed.contains(Card::new(identity.suit, Rank::ALL[rank - 1])))
            })
            .count(),
    );
    value.playable_finesse_opportunities = narrow(
        positional
            .iter()
            .filter(|identity| !secured.contains(*identity))
            .count(),
    );
    value.secured_card_quality = secured_card_quality(frontier, secured);
    value.exposed_chop_quality = secured_card_quality(frontier, exposed_chops);
    let mut protected = secured;
    for suit in Suit::ALL {
        for rank in source.play_stacks[suit.index()].len()..frontier.play_stacks[suit.index()].len()
        {
            protected = protected.union(IdentitySet::singleton(Card::new(suit, Rank::ALL[rank])));
        }
    }
    value.protected_bottom_deck_risks = narrow(
        protected
            .iter()
            .filter(|identity| {
                source
                    .hands
                    .iter()
                    .flatten()
                    .filter(|card| card.identity == Some(*identity))
                    .count()
                    == 1
            })
            .count(),
    );
    // Apply visible continuation value to every line, including root plays
    // and delayed clues, rather than granting a bonus only to direct clues.
    for suit in Suit::ALL {
        let height = frontier.play_stacks[suit.index()].len();
        if height > source.play_stacks[suit.index()].len() && height < Rank::ALL.len() {
            let next = Card::new(suit, Rank::ALL[height]);
            if frontier
                .hands
                .iter()
                .flatten()
                .any(|card| card.identity == Some(next))
            {
                value.visible_successors = value.visible_successors.saturating_add(1);
            }
        }
    }
    add_root_opportunities(source, profile, root, &mut value)?;
    // Reserve a productive clue only for an evidenced opportunity, not an
    // invented identity on a future draw. Saves/repairs are reserved separately.
    let productive_clue = value.conditional_successors > 0
        || frontier.hands.iter().flatten().any(|card| {
            card.identity.is_some_and(|identity| {
                is_eventually_useful(frontier, identity)
                    && !committed.contains(identity)
                    && can_play_soon(frontier, IdentitySet::singleton(identity), secured)
            })
        });
    value.clue_demand = super::ResourceSchedule::reserve(
        value.exposed_critical_chops,
        mandatory_clues,
        value.save_pressure > 0,
        productive_clue,
    );
    Some(value)
}

/// Missing prerequisites are neither already played, secured, nor visible.
/// This measures remaining access, not the number of turns until a play.
/// User-reviewed p4v0s1 opening comparison, September 15, 2026.
fn secured_card_quality(frontier: &PlayerView, secured: IdentitySet) -> crate::SecuredCardQuality {
    crate::SecuredCardQuality::from_cards(secured.iter().map(|identity| {
        let missing_predecessors = ((frontier.play_stacks[identity.suit.index()].len() + 1)
            ..usize::from(identity.rank.number()))
            .filter(|rank| {
                let predecessor = Card::new(identity.suit, Rank::ALL[*rank - 1]);
                !secured.contains(predecessor)
                    && !frontier
                        .hands
                        .iter()
                        .flatten()
                        .any(|card| card.identity == Some(predecessor))
            })
            .count();
        let visible_successor = identity.rank != Rank::Five
            && frontier.hands.iter().flatten().any(|card| {
                card.identity
                    == Some(Card::new(
                        identity.suit,
                        Rank::ALL[identity.rank.index() + 1],
                    ))
            });
        crate::future_card_quality::FutureCardQuality {
            rank: identity.rank,
            missing_predecessors: narrow(missing_predecessors),
            visible_successor,
        }
    }))
}

/// Shared net-risk assessment for clue scoring and unfinished endpoints.
/// The caller establishes that protection, rather than an ordinary draw or
/// play, moved chop. Only observer-known replacements remove BDR.
pub(super) fn worsened_chop_exposure(
    before: &PlayerView,
    after: &PlayerView,
    profile: HGroupProfile,
    old: Option<hanabi_core::CardId>,
    new: Option<hanabi_core::CardId>,
) -> Option<Card> {
    let old_loss = old.map(|card| super::symbolic_line::important_discard(before, profile, card));
    // Critical loss is worse than non-critical BDR. These occupy different
    // fields; absence of BDR must not be mistaken for zero old value.
    if old_loss.is_some_and(|(loss, _)| loss == Some(super::SavePrincipleViolation::CriticalCard)) {
        return None;
    }
    let old_risk = old_loss.and_then(|(_, risk)| risk);
    let new_risk =
        new.and_then(|card| super::symbolic_line::important_discard(after, profile, card).1)?;
    let old_quality = secured_card_quality(
        after,
        old_risk.map_or_else(IdentitySet::default, IdentitySet::singleton),
    );
    let new_quality = secured_card_quality(after, IdentitySet::singleton(new_risk));
    (!old_quality.no_worse_than(new_quality)).then_some(new_risk)
}

/// Check an explicit conditional branch immediately after a stack advances.
/// Only cards already hidden in the planning player's hand are considered;
/// blank draws are not silently assumed to contain the desired successor.
/// The next teammate must be free and have an admitted Play Clue in that
/// branch. Neither this branch nor its assumed identity enters public belief.
pub(super) fn conditional_successor(
    source: &PlayerView,
    after: &PlayerView,
    profile: HGroupProfile,
    played: Card,
) -> Option<super::ConditionalAlternative> {
    if played.rank == Rank::Five
        || after.clue_tokens == 0
        || after.current_player == source.observer
    {
        return None;
    }
    let successor = Card::new(played.suit, Rank::ALL[usize::from(played.rank.number())]);
    if unseen(after, successor) == 0
        || after
            .hands
            .iter()
            .flatten()
            .any(|card| card.identity == Some(successor))
    {
        return None;
    }
    let d = LogicalDeductions::new(after.clone()).ok()?;
    let notes = super::infer_h_group(&d, profile);
    let giver = after.current_player;
    let (giver_d, giver_replay) = PerspectiveProjector::new(after, profile)
        .project(giver, PerspectiveDepth::NestedRecipients)?;
    let giver_notes = infer_h_group_from_replay(&giver_d, giver_replay, profile);
    if !super::ActionWindow::from_inferences(giver_d.view(), &giver_notes).is_free() {
        return None;
    }
    for card in &after.hands[source.observer.index()] {
        if card.identity.is_some()
            || !source.hands[source.observer.index()]
                .iter()
                .any(|old| old.id == card.id)
            || !d
                .possible_identities(card.id)
                .is_some_and(|domain| domain.contains(successor))
            || !notes
                .cards
                .iter()
                .any(|note| note.card == card.id && note.identities.contains(successor))
        {
            continue;
        }
        let mut branch = after.clone();
        branch.hands[source.observer.index()]
            .iter_mut()
            .find(|slot| slot.id == card.id)?
            .identity = Some(successor);
        let Some((branch_d, replay)) = PerspectiveProjector::new(&branch, profile)
            .project(giver, PerspectiveDepth::NestedRecipients)
        else {
            continue;
        };
        let branch_notes = infer_h_group_from_replay(&branch_d, replay.clone(), profile);
        if !super::ActionWindow::from_inferences(branch_d.view(), &branch_notes).is_free() {
            continue;
        }
        let candidates = super::h_group_clue_candidates_from_replay(&branch_d, profile, &replay);
        if candidates.iter().any(|candidate| {
            candidate.is_urgent_save() || candidate.purpose() == super::CluePurpose::Fix
        }) {
            continue;
        }
        if let Some(candidate) = candidates.iter().find(|candidate| {
            candidate.target() == source.observer
                && candidate.purpose() == super::CluePurpose::Play
                && candidate.immediate_play()
                && matches!(candidate.action, Action::Clue { clue, .. } if clue.matches(successor))
        }) {
            let mut resources = super::ResourceSchedule::new(after.clue_tokens);
            if !resources.apply(after.turn, 1, 0) {
                continue;
            }
            return Some(super::ConditionalAlternative {
                after_step: 0,
                condition: super::HiddenCardCondition {
                    observer: source.observer,
                    owner: source.observer,
                    card: card.id,
                    identity: successor,
                },
                follow_up: super::PlanStep {
                    interpreted_identities: None,
                    turn: after.turn,
                    depends_on: None,
                    projected: super::ProjectedAction {
                        actor: giver,
                        action: candidate.action,
                    },
                    consequences: super::ProjectedConsequences {
                        clues_spent: 1,
                        ..super::ProjectedConsequences::default()
                    },
                },
                latest_turn: after.turn,
                resources,
            });
        }
    }
    None
}

#[allow(clippy::too_many_lines)]
fn add_root_opportunities(
    source: &PlayerView,
    profile: HGroupProfile,
    root: Action,
    value: &mut ProjectedPositionValue,
) -> Option<()> {
    let Action::Clue { target, clue } = root else {
        return Some(());
    };
    let d = LogicalDeductions::new(source.clone()).ok()?;
    let candidates = h_group_clue_candidates(&d, profile);
    let candidate = candidates
        .iter()
        .find(|candidate| candidate.action == root)?;
    let positional = super::positional_value::evaluate(&d, profile, *candidate);
    value.foregone_blind_plays = positional.foregone_blind_plays;
    value.conditional_prompt_chains = positional.conditional_prompt_chains;
    if candidate.immediate_play() {
        for touched in source.hands[target.index()]
            .iter()
            .filter_map(|card| card.identity)
            .filter(|identity| clue.matches(*identity) && is_playable_now(source, *identity))
        {
            if touched.rank == Rank::Five {
                continue;
            }
            let next = Card::new(touched.suit, Rank::ALL[usize::from(touched.rank.number())]);
            for (owner, hand) in source.hands.iter().enumerate() {
                if hand.iter().any(|card| card.identity == Some(next)) {
                    // Conditional option: if this owner clues instead of
                    // drawing, the next needed card remains on finesse position.
                    // No particular Finesse/Bluff/Ignition is asserted to exist.
                    if owner != target.index()
                        && hand
                            .iter()
                            .rev()
                            .find(|card| !was_clued_before(source, source.turn, card.id))
                            .is_some_and(|card| card.identity == Some(next))
                    {
                        value.finesse_opportunities = value.finesse_opportunities.saturating_add(1);
                    }
                }
            }
        }
    }
    if candidate.is_save() && source.deck_size > 0 {
        let own = super::infer_h_group(&d, profile);
        let (owner_d, owner_replay) = PerspectiveProjector::new(source, profile)
            .project(target, PerspectiveDepth::NestedRecipients)?;
        let owner = infer_h_group_from_replay(&owner_d, owner_replay, profile);
        let can_draw_before_discarding = !owner.playable_now.is_empty();
        let touched = source.hands[target.index()]
            .iter()
            .filter(|card| card.identity.is_some_and(|identity| clue.matches(identity)))
            .map(|card| card.id)
            .collect::<Vec<_>>();
        let after = super::compiled_prospective_clue(source, profile, target, clue, &touched)?
            .projection(target)?;
        if touched.iter().any(|card| {
            after.inferred.playable_now.contains(card) && !owner.playable_now.contains(card)
        }) {
            // A Save-shaped clue can also obtain an immediate play. It is
            // not a passive Early Save merely because Save has interpretation
            // precedence. Waiting sacrifices real progress in this case.
            return Some(());
        }
        let next_chop = source.hands[target.index()]
            .iter()
            .find(|card| !touched.contains(&card.id) && !owner.gotten().contains(&card.id));
        let critical_possible = match next_chop {
            Some(card) => card.identity.map_or_else(
                || {
                    IdentitySet::from_mask(card.clues.identity_mask())
                        .iter()
                        .any(|identity| unseen(source, identity) > 0 && critical(source, identity))
                },
                |identity| critical(source, identity),
            ),
            None => Suit::ALL.iter().any(|suit| {
                Rank::ALL.iter().any(|rank| {
                    let identity = Card::new(*suit, *rank);
                    unseen(source, identity) > 0 && critical(source, identity)
                })
            }),
        };
        if critical_possible {
            // Two Saves may be needed before the hand can safely discard.
            // The contingency is worse if the current token supply cannot
            // fund both; it is not a prediction of the unknown draw.
            value.save_pressure = consecutive_save_pressure(source.clue_tokens);
        }
        if can_draw_before_discarding {
            value.foregone_touch_opportunities = narrow(
                Suit::ALL
                    .iter()
                    .flat_map(|suit| Rank::ALL.iter().map(move |rank| Card::new(*suit, *rank)))
                    .filter(|identity| {
                        clue.matches(*identity)
                            && is_eventually_useful(source, *identity)
                            && unseen(source, *identity) > 0
                            && !source.hands.iter().flatten().any(|card| {
                                card.identity == Some(*identity)
                                    && was_clued_before(source, source.turn, card.id)
                            })
                            && !own.cards.iter().any(|card| {
                                card.promised_identity == Some(*identity)
                                    || card.identities == IdentitySet::singleton(*identity)
                            })
                            && !source.hands[target.index()].iter().any(|card| {
                                card.identity == Some(*identity) && touched.contains(&card.id)
                            })
                    })
                    .count(),
            );
        }
    }
    Some(())
}

fn consecutive_save_pressure(tokens: u8) -> u8 {
    1 + 2_u8.saturating_sub(tokens)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reviewed_opening_distinguishes_the_saved_threes_and_fours() {
        let replay = hanabi_protocol::HanabiLiveReplay::from_json(include_str!(
            "tests/fixtures/game-p4v0s1-before-turn18-revision.json"
        ))
        .unwrap();
        let root = replay.state_at_turn(0).unwrap();
        let bluff = replay.state_at_turn(2).unwrap();
        let mut charm = root.clone();
        charm
            .apply(Action::Clue {
                target: PlayerId::new(2),
                clue: Clue::Rank(Rank::Four),
            })
            .unwrap();
        charm
            .apply(Action::Play(hanabi_core::CardId::new(4)))
            .unwrap();
        let identities = |a, b| IdentitySet::singleton(a).union(IdentitySet::singleton(b));
        let threes = secured_card_quality(
            &bluff.view_for(PlayerId::new(0)).unwrap(),
            identities(
                Card::new(Suit::Purple, Rank::Three),
                Card::new(Suit::Red, Rank::Three),
            ),
        );
        let fours = secured_card_quality(
            &charm.view_for(PlayerId::new(0)).unwrap(),
            identities(
                Card::new(Suit::Blue, Rank::Four),
                Card::new(Suit::Green, Rank::Four),
            ),
        );
        let quality = |rank, missing_predecessors, visible_successor| {
            crate::future_card_quality::FutureCardQuality {
                rank,
                missing_predecessors,
                visible_successor,
            }
        };
        assert_eq!(
            threes,
            crate::SecuredCardQuality::from_cards([
                quality(Rank::Three, 1, true),
                quality(Rank::Three, 2, false),
            ])
        );
        assert_eq!(
            fours,
            crate::SecuredCardQuality::from_cards([
                quality(Rank::Four, 1, false),
                quality(Rank::Four, 3, false),
            ])
        );
        assert!(threes.no_worse_than(fours));
        assert!(!fours.no_worse_than(threes));
    }
    use hanabi_core::Clue;

    #[test]
    fn reviewed_blue_two_opportunity_beats_a_surplus_five_token() {
        // User-reviewed p4v0s3 turn 36: Cathy can clue b3 if Donald has it;
        // otherwise she discards. His actual b3 is never supplied to planning.
        let replay = hanabi_protocol::HanabiLiveReplay::from_json(include_str!(
            "../../../hanabi-protocol/tests/fixtures/game-p4v0s3.json"
        ))
        .unwrap();
        let state = replay.state_at_turn(35).unwrap();
        let view = state.view_for(state.current_player()).unwrap();
        assert!(view.hands[3].iter().all(|card| card.identity.is_none()));
        let result = crate::analyze_position(
            &view,
            crate::SupportedConvention::HGroup(HGroupProfile::Max),
            crate::PlannerConfig::default(),
        )
        .unwrap();
        let blue = Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Suit(Suit::Blue),
        };
        let purple = Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Suit(Suit::Purple),
        };
        assert_eq!(result.planner.best_action, blue);
        let value = |action| {
            result
                .planner
                .root_actions
                .iter()
                .find(|c| c.action == action)
                .unwrap()
                .symbolic_line
                .position_value
                .unwrap()
        };
        assert!(value(blue).conditional_successors > value(purple).conditional_successors);
        assert!(value(blue).clues < value(purple).clues);
        assert_eq!(value(blue).score, value(purple).score);

        let b2 = Card::new(Suit::Blue, Rank::Two);
        // Build the reviewed prefix with blank draws, exactly as Donald
        // projects it. Later actual draws can change Cathy's available turn.
        let after = super::super::ProspectiveTransition::clue(
            &view,
            PlayerId::new(1),
            Clue::Suit(Suit::Blue),
            &[hanabi_core::CardId::new(30)],
        );
        let after = super::super::ProspectiveTransition::play(
            &after,
            PlayerId::new(0),
            hanabi_core::CardId::new(1),
            Card::new(Suit::Red, Rank::Three),
            true,
        );
        let mut after = super::super::ProspectiveTransition::play(
            &after,
            PlayerId::new(1),
            hanabi_core::CardId::new(30),
            b2,
            true,
        );
        let original = after.clone();
        let branch = conditional_successor(&view, &after, HGroupProfile::Max, b2).unwrap();
        assert_eq!(
            branch.condition.identity,
            Card::new(Suit::Blue, Rank::Three)
        );
        assert_eq!(
            after, original,
            "a conditional assumption must not mutate the ordinary line"
        );
        assert_eq!(branch.condition.observer, view.observer);
        assert_eq!(branch.condition.owner, view.observer);
        assert_eq!(branch.follow_up.projected.actor, after.current_player);
        assert_eq!(branch.latest_turn, after.turn);
        assert_eq!(branch.resources.tokens, after.clue_tokens - 1);
        assert!(
            matches!(branch.follow_up.projected.action, Action::Clue { target, .. } if target == view.observer)
        );
        for card in &mut after.hands[3] {
            card.clues.add_negative_clue(Clue::Rank(Rank::Three));
        }
        assert_eq!(
            conditional_successor(&view, &after, HGroupProfile::Max, b2),
            None,
            "the opportunity requires an owner-compatible hidden identity"
        );
        after = original;
        after.clue_tokens = 0;
        assert_eq!(
            conditional_successor(&view, &after, HGroupProfile::Max, b2),
            None,
            "a teammate cannot give the conditional clue without a token"
        );
        assert_eq!(
            conditional_successor(
                &view,
                &after,
                HGroupProfile::Max,
                Card::new(Suit::Purple, Rank::Five)
            ),
            None
        );
    }

    #[test]
    fn reviewed_turn_eight_compares_productivity_and_waiting_options() {
        // All strategic expectations below were supplied by the user for
        // p4v0s2 turn 8; draw opportunities are not deterministic draws.
        let replay = hanabi_protocol::HanabiLiveReplay::from_json(include_str!(
            "../../../hanabi-protocol/tests/fixtures/game-p4v0s2.json"
        ))
        .unwrap();
        let state = replay.state_at_turn(7).unwrap();
        let view = state.view_for(state.current_player()).unwrap();
        let result = crate::analyze_position(
            &view,
            crate::SupportedConvention::HGroup(HGroupProfile::Max),
            crate::PlannerConfig::default(),
        )
        .unwrap();
        let value = |target, clue| {
            result
                .planner
                .root_actions
                .iter()
                .find(|candidate| {
                    candidate.action
                        == Action::Clue {
                            target: PlayerId::new(target),
                            clue,
                        }
                })
                .unwrap()
                .symbolic_line
                .position_value
                .unwrap()
        };
        let save = value(2, Clue::Rank(Rank::Two));
        let blue = value(0, Clue::Suit(Suit::Blue));
        let purple = value(0, Clue::Suit(Suit::Purple));
        let _yellow = value(1, Clue::Suit(Suit::Yellow));
        assert_eq!(
            result.planner.best_action,
            Action::Clue {
                target: PlayerId::new(0),
                clue: Clue::Suit(Suit::Purple)
            }
        );
        assert!(purple.score > save.score && purple.clues >= save.clues);
        assert_eq!(save.exposed_critical_chops, 0);
        assert_eq!(save.save_pressure, 0);
        assert_eq!(
            save.foregone_touch_opportunities, 1,
            "y2 can add a useful touch after Cathy's draw"
        );
        assert_eq!(blue.visible_successors, 1);
        assert_eq!(purple.visible_successors, 1);
        assert_eq!(blue.finesse_opportunities, 0);
        assert_eq!(purple.finesse_opportunities, 1);
        assert!(can_play_soon(
            &view,
            IdentitySet::singleton(Card::new(Suit::Yellow, Rank::Two))
                .union(IdentitySet::singleton(Card::new(Suit::Blue, Rank::Two))),
            IdentitySet::default()
        ));
        // Algorithmic boundary: there is no extra-touch draw opportunity
        // once the deck is empty, even with the same visible cards.
        let mut no_draw = view.clone();
        no_draw.deck_size = 0;
        let mut opportunities = ProjectedPositionValue::default();
        add_root_opportunities(
            &no_draw,
            HGroupProfile::Max,
            Action::Clue {
                target: PlayerId::new(2),
                clue: Clue::Rank(Rank::Two),
            },
            &mut opportunities,
        )
        .unwrap();
        assert_eq!(opportunities.foregone_touch_opportunities, 0);
    }

    #[test]
    fn promised_connector_in_own_hand_is_not_a_missing_predecessor() {
        let mut replay = hanabi_protocol::HanabiLiveReplay::from_json(include_str!(
            "../../../hanabi-protocol/tests/fixtures/game-p4v0s2.json"
        ))
        .unwrap();
        // Put the other y3 back in the deck to isolate the known, hidden
        // connector. This changes no clue touch or action in this prefix.
        replay.deck.swap(25, 39);
        let view = replay
            .state_at_turn(33)
            .unwrap()
            .view_for(PlayerId::new(2))
            .unwrap();
        let y4 = IdentitySet::singleton(Card::new(Suit::Yellow, Rank::Four));
        let y3 = IdentitySet::singleton(Card::new(Suit::Yellow, Rank::Three));
        assert!(!can_play_soon(&view, y4, IdentitySet::default()));
        assert!(can_play_soon(&view, y4, y3));
        assert!(view.hands[2].iter().all(|card| card.identity.is_none()));
    }

    #[test]
    fn contingent_second_save_accounts_for_token_shortfall() {
        assert_eq!(consecutive_save_pressure(2), 1);
        assert_eq!(consecutive_save_pressure(1), 2);
        assert_eq!(consecutive_save_pressure(0), 3);
    }
}
