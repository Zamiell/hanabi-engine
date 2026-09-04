//! Draw allocation is strategic value, not a new card-identity promise.
//!
//! [Team Distribution Principle](https://hanabi.github.io/level-8/#team-distribution-principle)
//! motivates comparing completion times, not counting cards in each hand.

use super::{
    Card, CardId, Clue, CluePurpose, CompiledClueAction, HGroupInferences, HGroupProfile,
    HGroupRuleId, LogicalDeductions, PerspectiveDepth, PerspectiveProjector, ProspectiveTransition,
    Rank, Suit, h_group_clue_candidates_from_replay, is_critical, is_eventually_useful,
    next_player, rule_enabled,
};

/// Conservative preference over a non-urgent direct clue, never a mandatory
/// play or repair. Unknown draws stay blank. Every possible discarded identity
/// must preserve the next player's ability to give the same clue.
pub(super) fn discard_priority(
    deductions: &LogicalDeductions,
    inferred: &HGroupInferences,
    profile: HGroupProfile,
    candidates: &[CompiledClueAction],
    discard: CardId,
) -> Option<i32> {
    let view = deductions.view();
    let remaining_plays = 25_usize.saturating_sub(view.play_stacks.iter().map(Vec::len).sum());
    if !rule_enabled(profile, HGroupRuleId::EndGame)
        || view.deck_size < view.hands.len()
        || view.deck_size + view.hands.len() <= remaining_plays + 1
        || view.clue_tokens == 0
        || view.clue_tokens >= super::MAX_CLUE_TOKENS
        || !inferred.playable_now.is_empty()
        || inferred.connection.is_some()
        || inferred.must_clue.contains(&view.observer)
        || candidates
            .iter()
            .any(|candidate| candidate.is_urgent_save() || candidate.purpose() == CluePurpose::Fix)
    {
        return None;
    }
    let best = candidates
        .iter()
        .max_by_key(|candidate| candidate.score())?;
    if best.purpose() != CluePurpose::Play
        || !best.immediate_play()
        || best.connection_steps() != 0
        || best.score() >= 425
    {
        return None;
    }
    let missing = Suit::ALL
        .into_iter()
        .filter_map(|suit| {
            let rank = Rank::ALL.get(view.play_stacks[suit.index()].len())?;
            let identity = Card::new(suit, *rank);
            let visible = view
                .hands
                .iter()
                .flatten()
                .any(|card| card.identity == Some(identity));
            let known_own = inferred
                .cards
                .iter()
                .any(|card| card.identities.len() == 1 && card.identities.contains(identity));
            (!visible && !known_own && is_eventually_useful(view, identity)).then_some(identity)
        })
        .collect::<Vec<_>>();
    let [connector] = missing.as_slice() else {
        return None;
    };
    let next = next_player(view.observer, view.hands.len());
    if best.target() == next {
        return None;
    }
    let schedules = completion_times(deductions, inferred, profile, *connector)?;
    if schedules.0 >= schedules.1 {
        return None;
    }
    let identities = deductions.possible_identities(discard)?;
    if identities.is_empty()
        || identities.contains(*connector)
        || identities.iter().any(|identity| {
            is_eventually_useful(view, identity)
                && (identity.rank == Rank::Five || is_critical(view, identity))
        })
    {
        return None;
    }
    for identity in identities.iter() {
        let after = ProspectiveTransition::discard(view, view.observer, discard, identity);
        let (next_deductions, next_replay) = PerspectiveProjector::new(&after, profile)
            .project(next, PerspectiveDepth::NestedRecipients)?;
        let next_inferred = super::decision::infer_h_group_from_replay(
            &next_deductions,
            next_replay.clone(),
            profile,
        );
        if next_inferred.connection.is_some()
            || !next_inferred.playable_now.is_empty()
            || !next_inferred.discard_now.is_empty()
            || !h_group_clue_candidates_from_replay(&next_deductions, profile, &next_replay)
                .iter()
                .any(|candidate| candidate.action == best.action && candidate.immediate_play())
        {
            return None;
        }
    }
    // Stay below an ordinary guaranteed play (525). This only chooses who
    // performs a deferrable clue, not whether playing promises is optional.
    Some(101 + i32::from(best.score()))
}

/// Conditional earliest completion offsets if the connector is drawn now or
/// by the next seat. This is a scheduling heuristic, not a prediction of the
/// draw or a convention inference. Both alternatives reserve turns 0 and 1
/// for the discard and the interchangeable clue. Unclued visible successors
/// are eligible, but cost an additional clue turn before they can play.
pub(super) fn completion_times(
    deductions: &LogicalDeductions,
    inferred: &HGroupInferences,
    profile: HGroupProfile,
    connector: Card,
) -> Option<(usize, usize)> {
    let view = deductions.view();
    let players = view.hands.len();
    let projector = PerspectiveProjector::new(view, profile);
    let mut busy = vec![0, 1];
    for offset in 2..players {
        let actor =
            super::PlayerId::new(u8::try_from((view.observer.index() + offset) % players).ok()?);
        let (d, replay) = projector.project(actor, PerspectiveDepth::NestedRecipients)?;
        let notes = super::decision::infer_h_group_from_replay(&d, replay, profile);
        if notes.connection.is_some()
            || !notes.playable_now.is_empty()
            || !notes.discard_now.is_empty()
            || notes.must_clue.contains(&actor)
        {
            busy.push(offset);
        }
    }
    let mut suffix = Vec::new();
    for rank in Rank::ALL.iter().skip(usize::from(connector.rank.number())) {
        let identity = Card::new(connector.suit, *rank);
        let mut owners = Vec::new();
        for (player, hand) in view.hands.iter().enumerate() {
            for card in hand {
                let known_own = player == view.observer.index()
                    && inferred.cards.iter().any(|note| {
                        note.card == card.id
                            && note.identities.len() == 1
                            && note.identities.contains(identity)
                    });
                if card.identity == Some(identity) || known_own {
                    let exact_clue = card.clues.has_positive_clue(Clue::Suit(identity.suit))
                        && card.clues.has_positive_clue(Clue::Rank(identity.rank));
                    owners.push(ScheduledCard {
                        seat: (player + players - view.observer.index()) % players,
                        needs_clue: !known_own && !exact_clue,
                    });
                }
            }
        }
        if owners.is_empty() {
            break; // Do not invent the location of an unseen higher card.
        }
        suffix.push(owners);
    }
    if suffix.is_empty() {
        return None;
    }
    let finish = |drawer| {
        let mut chain = vec![vec![ScheduledCard {
            seat: drawer,
            needs_clue: true,
        }]];
        chain.extend(suffix.iter().cloned());
        schedule_chain(
            &chain,
            players,
            drawer,
            drawer,
            &busy,
            usize::from(view.clue_tokens),
        )
    };
    Some((finish(0)?, finish(1)?))
}

#[derive(Clone, Copy)]
struct ScheduledCard {
    seat: usize,
    needs_clue: bool,
}

fn schedule_chain(
    chain: &[Vec<ScheduledCard>],
    players: usize,
    after: usize,
    clue_after: usize,
    busy: &[usize],
    clues: usize,
) -> Option<usize> {
    let Some((cards, rest)) = chain.split_first() else {
        return Some(after);
    };
    cards
        .iter()
        .filter_map(|card| {
            let mut occupied = busy.to_vec();
            let mut ready = after;
            let mut remaining_clues = clues;
            let mut last_clue = clue_after;
            if card.needs_clue {
                remaining_clues = remaining_clues.checked_sub(1)?;
                // Once the predecessor has been announced, a later clue can
                // prepare its successor before the predecessor physically plays.
                let clue_turn = (clue_after + 1..64)
                    .find(|turn| turn % players != card.seat && !occupied.contains(turn))?;
                occupied.push(clue_turn);
                ready = ready.max(clue_turn);
                last_clue = clue_turn;
            }
            let play_turn = (ready + 1..64)
                .find(|turn| turn % players == card.seat && !occupied.contains(turn))?;
            occupied.push(play_turn);
            schedule_chain(
                rest,
                players,
                play_turn,
                last_clue,
                &occupied,
                remaining_clues,
            )
        })
        .min()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scheduler_requires_a_clue_and_respects_reserved_actions() {
        let chain = vec![vec![ScheduledCard {
            seat: 0,
            needs_clue: true,
        }]];
        assert_eq!(schedule_chain(&chain, 4, 0, 0, &[0, 1], 0), None);
        assert_eq!(schedule_chain(&chain, 4, 0, 0, &[0, 1], 1), Some(4));
        assert_eq!(schedule_chain(&chain, 4, 0, 0, &[0, 1, 4], 1), Some(8));
    }

    #[test]
    fn scheduler_chooses_the_faster_visible_successor_copy() {
        let chain = vec![
            vec![ScheduledCard {
                seat: 0,
                needs_clue: true,
            }],
            vec![
                ScheduledCard {
                    seat: 0,
                    needs_clue: false,
                },
                ScheduledCard {
                    seat: 1,
                    needs_clue: true,
                },
            ],
        ];
        assert_eq!(schedule_chain(&chain, 4, 0, 0, &[0, 1], 2), Some(5));
        assert_eq!(schedule_chain(&chain, 4, 0, 0, &[0, 1], 1), Some(8));
    }
}
