//! Draw allocation is strategic value, not a new card-identity promise.
//!
//! [Team Distribution Principle](https://hanabi.github.io/level-8/#team-distribution-principle)
//! motivates letting an unloaded player draw an unseen connector while the
//! player holding its successor gives an interchangeable clue instead.

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
    let successor = Card::new(
        connector.suit,
        *Rank::ALL.get(usize::from(connector.rank.number()))?,
    );
    let next = next_player(view.observer, view.hands.len());
    if best.target() == next
        || !view.hands[next.index()].iter().any(|card| {
            card.identity == Some(successor)
                && (card.clues.has_positive_clue(Clue::Suit(successor.suit))
                    || card.clues.has_positive_clue(Clue::Rank(successor.rank)))
        })
        || view.hands[view.observer.index()].iter().any(|card| {
            deductions
                .possible_identities(card.id)
                .is_some_and(|set| set.contains(successor))
        })
    {
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
