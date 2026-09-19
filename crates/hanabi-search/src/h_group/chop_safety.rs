//! Conditional discard safety from a declined mandatory protection opportunity.
//!
//! This is strategy knowledge, not literal card information or a trash note.
//! [Save Principle](https://hanabi.github.io/beginner/save-principle/) forbids
//! knowingly letting the next player discard a unique playable or last copy.
//! Unlike general inverse planning, the witness is a mandatory protection duty,
//! not a comparison between heuristic scores.

use hanabi_core::{CardId, ClueFacts, ObservedCard, ObservedEvent, PlayerView};

use super::{
    HGroupInferences, HGroupProfile, HGroupRuleId, LogicalDeductions, PerspectiveDepth,
    PerspectiveProjector, Rank, infer_h_group, is_critical_save_identity, is_eventually_useful,
    is_playable_now, next_player, rule_enabled,
};
use crate::IdentitySet;

/// Identities still relevant when choosing to discard. Never publishes an exact
/// identity or changes the literal/conventional card notes. No witness means
/// retaining the entire logical domain.
pub(super) fn discard_domain(
    deductions: &LogicalDeductions,
    inferred: &HGroupInferences,
    profile: HGroupProfile,
    card: CardId,
) -> Option<IdentitySet> {
    let domain = deductions.possible_identities(card)?;
    let view = deductions.view();
    if !rule_enabled(profile, HGroupRuleId::BasicStrategy)
        || view.observer != view.current_player
        || inferred.chops[view.observer.index()] != Some(card)
        || !inferred.playable_now.is_empty()
        || inferred.connection.is_some()
        || inferred.must_clue.contains(&view.observer)
        || view.hands[view.observer.index()]
            .iter()
            .any(|held| positively_clued(held.clues))
    {
        return Some(domain);
    }
    let Some(before) = before_ordinary_play(view) else {
        return Some(domain);
    };
    let player = before.current_player;
    let Ok(prior_deductions) = LogicalDeductions::new(before.clone()) else {
        return Some(domain);
    };
    let prior = infer_h_group(&prior_deductions, profile);
    if prior.chops[view.observer.index()] != Some(card)
        || !prior.playable_now.is_empty()
        || prior.connection.is_some()
        || prior.must_clue.contains(&view.observer)
    {
        return Some(domain);
    }
    let Some((giver_deductions, giver_replay)) =
        PerspectiveProjector::new(&before, profile).project(player, PerspectiveDepth::ObserverOnly)
    else {
        return Some(domain);
    };
    let giver = super::infer_h_group_from_replay(&giver_deductions, giver_replay, profile);
    if giver.connection.is_some() || giver.must_clue.contains(&player) {
        return Some(domain);
    }

    let mut allowed = IdentitySet::from_mask(0);
    for candidate in domain.iter() {
        let replacement = before
            .hands
            .iter()
            .flatten()
            .any(|held| held.id != card && held.identity == Some(candidate));
        let required = is_eventually_useful(&before, candidate)
            && !replacement
            && (candidate.rank == Rank::Five
                || is_critical_save_identity(&before, candidate)
                || is_playable_now(&before, candidate)
                || candidate.rank == Rank::Two);
        if !required {
            allowed = allowed.union(IdentitySet::singleton(candidate));
        }
    }
    // Inconsistent observations must not make every discard vacuously safe.
    Some(if allowed.is_empty() { domain } else { allowed })
}

fn before_ordinary_play(view: &PlayerView) -> Option<PlayerView> {
    let last = view
        .history
        .iter()
        .rev()
        .find(|entry| !matches!(entry.event, ObservedEvent::Drew { .. }))?;
    let ObservedEvent::Played {
        player,
        card: played,
        identity,
        successful: true,
    } = last.event
    else {
        return None;
    };
    if last.turn + 1 != view.turn || next_player(player, view.hands.len()) != view.observer {
        return None;
    }
    // Rewind only this successful play and its draw. In particular, do not
    // exclude a card that only became playable because of the observed play.
    let mut before = view.clone();
    before.turn = last.turn;
    before.current_player = player;
    before.play_stacks[identity.suit.index()].retain(|(id, _)| *id != played);
    if identity.rank == Rank::Five {
        // A play at the token cap does not reveal whether there was a token
        // available beforehand; the conservative lower bound is sufficient.
        before.clue_tokens = before.clue_tokens.saturating_sub(1);
    }
    if before.clue_tokens == 0 {
        return None;
    }
    for event in view.history.iter().filter(|entry| entry.turn == last.turn) {
        if let ObservedEvent::Drew { player, card, .. } = event.event {
            before.hands[player.index()].retain(|held| held.id != card);
            before.deck_size += 1;
        }
    }
    before.history.retain(|entry| entry.turn < last.turn);
    let mut clues = ClueFacts::default();
    for entry in &before.history {
        if let ObservedEvent::Clued {
            clue,
            touched,
            untouched,
            ..
        } = &entry.event
        {
            if touched.contains(&played) {
                clues.add_positive_clue(*clue);
            }
            if untouched.contains(&played) {
                clues.add_negative_clue(*clue);
            }
        }
    }
    // A blind response is not an ordinary declined protection opportunity.
    if !positively_clued(clues) {
        return None;
    }
    before.hands[player.index()].push(ObservedCard {
        id: played,
        identity: Some(identity),
        clues,
    });
    before.hands[player.index()].sort_by_key(|held| held.id);
    Some(before)
}

fn positively_clued(clues: ClueFacts) -> bool {
    super::Suit::ALL
        .into_iter()
        .any(|suit| clues.has_positive_clue(super::Clue::Suit(suit)))
        || Rank::ALL
            .into_iter()
            .any(|rank| clues.has_positive_clue(super::Clue::Rank(rank)))
}
