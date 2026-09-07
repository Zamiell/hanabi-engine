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
fn can_play_soon(view: &PlayerView, domain: IdentitySet) -> bool {
    !domain.is_empty()
        && domain.iter().all(|identity| {
            !is_eventually_useful(view, identity)
                || ((view.play_stacks[identity.suit.index()].len() + 1)
                    ..usize::from(identity.rank.number()))
                    .all(|rank| {
                        view.hands.iter().flatten().any(|card| {
                            card.identity == Some(Card::new(identity.suit, Rank::ALL[rank - 1]))
                        })
                    })
        })
}

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
    for player in 0..frontier.hands.len() {
        let actor = PlayerId::new(narrow(player));
        let (d, replay) = PerspectiveProjector::new(frontier, profile)
            .project(actor, PerspectiveDepth::NestedRecipients)?;
        let inferred = infer_h_group_from_replay(&d, replay, profile);
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
            let promised = card
                .promised_identity
                .or_else(|| {
                    (card.identities.len() == 1)
                        .then(|| card.identities.iter().next())
                        .flatten()
                })
                .or_else(|| {
                    // Protecting a useful future card is still progress even
                    // when it cannot play soon. Otherwise a valuable Save
                    // would count only as a hand blockage in this assessment.
                    was_clued_before(frontier, frontier.turn, card.card)
                        .then(|| super::identity_of(frontier, card.card))
                        .flatten()
                });
            if let Some(identity) =
                promised.filter(|identity| is_eventually_useful(frontier, *identity))
            {
                secured = secured.union(IdentitySet::singleton(identity));
            }
            if was_clued_before(frontier, frontier.turn, card.card)
                && !can_play_soon(frontier, card.identities)
            {
                value.blocked_clued_cards = value.blocked_clued_cards.saturating_add(1);
            }
        }
    }
    value.secured_future_plays = narrow(secured.len());
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
    Some(value)
}

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
    use hanabi_core::Clue;

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
                .union(IdentitySet::singleton(Card::new(Suit::Blue, Rank::Two)))
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
    fn contingent_second_save_accounts_for_token_shortfall() {
        assert_eq!(consecutive_save_pressure(2), 1);
        assert_eq!(consecutive_save_pressure(1), 2);
        assert_eq!(consecutive_save_pressure(0), 3);
    }
}
