//! Conditional discard safety from a declined mandatory protection opportunity.
//!
//! This is strategy knowledge, not literal card information or a trash note.
//! [Save Principle](https://hanabi.github.io/beginner/save-principle/) forbids
//! knowingly letting a teammate discard a unique playable or last copy.
//! Unlike general inverse planning, the witness is a mandatory protection duty,
//! not a comparison between heuristic scores.

use hanabi_core::{CardId, ClueFacts, ObservedCard, ObservedEvent, PlayerView};

use super::{
    HGroupInferences, HGroupProfile, HGroupRuleId, LogicalDeductions, PerspectiveDepth,
    PerspectiveProjector, Rank, infer_h_group, is_critical_save_identity, is_eventually_useful,
    is_playable_now, rule_enabled,
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
    {
        return Some(domain);
    }
    let mut allowed = domain;
    for entry in view.history.iter().rev() {
        // This query evaluates discard safety, not exact identity. Once all
        // remaining useful possibilities have visible replacements, older
        // reductions cannot improve its answer.
        if !allowed.iter().any(|identity| {
            is_eventually_useful(view, identity)
                && !view
                    .hands
                    .iter()
                    .flatten()
                    .any(|held| held.id != card && held.identity == Some(identity))
        }) {
            break;
        }
        let actor = match entry.event {
            ObservedEvent::Played {
                player,
                successful: true,
                ..
            }
            | ObservedEvent::Discarded { player, .. } => player,
            ObservedEvent::Clued { giver, .. } => giver,
            _ => continue,
        };
        // Protection is a team obligation, not evidence confined to the
        // immediately preceding seat. A prior teammate can spend the last
        // clue elsewhere, leaving the predecessor unable to protect this
        // chop. Retain that earlier opportunity (reviewed p4v0s1 turn 26).
        if actor == view.observer {
            continue;
        }
        let snapshot = before_historical_turn(view, entry.turn + 1);
        if !snapshot.hands[view.observer.index()]
            .iter()
            .any(|held| held.id == card)
        {
            break;
        }
        let before = match entry.event {
            // Giving a different clue is also a declined protection
            // opportunity. Saving somebody else, or leaving this player a
            // clue to give, does not waive Save Principle for their exposed
            // chop. The same historical obligation guards apply below.
            // https://hanabi.github.io/beginner/save-principle/
            ObservedEvent::Clued { .. } => {
                let mut before = before_historical_turn(view, entry.turn);
                before.current_player = actor;
                Some(before)
            }
            ObservedEvent::Discarded { identity, .. } => {
                let mut before = before_historical_turn(view, entry.turn);
                before.current_player = actor;
                // A voluntary trash discard with a token available also
                // declines protection. Do not treat a zero-token discard
                // or a useful-card transfer as the same evidence.
                (before.clue_tokens > 0 && !is_eventually_useful(&before, identity))
                    .then_some(before)
            }
            _ => before_ordinary_play(&snapshot),
        };
        if let Some(before) = before {
            // A Save can expose a different chop. The protection duty is
            // about the recipient's actual response position, not only the
            // card that was chop before the clue. Use pre-play state for
            // plays so a newly advanced stack cannot invent prior evidence.
            let recipient = if matches!(
                entry.event,
                ObservedEvent::Clued { .. } | ObservedEvent::Discarded { .. }
            ) {
                &snapshot
            } else {
                &before
            };
            allowed = declined_protection_domain(&before, recipient, profile, card, allowed);
        }
    }
    // Inconsistent observations must not make every discard vacuously safe.
    Some(if allowed.is_empty() { domain } else { allowed })
}

fn declined_protection_domain(
    before: &PlayerView,
    recipient_position: &PlayerView,
    profile: HGroupProfile,
    card: CardId,
    domain: IdentitySet,
) -> IdentitySet {
    let player = before.current_player;
    let Ok(prior_deductions) = LogicalDeductions::new(recipient_position.clone()) else {
        return domain;
    };
    let prior = infer_h_group(&prior_deductions, profile);
    if prior.chops[before.observer.index()] != Some(card)
        || !prior.playable_now.is_empty()
        || prior.connection.is_some()
        || prior.must_clue.contains(&before.observer)
    {
        return domain;
    }
    let Some((giver_deductions, giver_replay)) =
        PerspectiveProjector::new(before, profile).project(player, PerspectiveDepth::ObserverOnly)
    else {
        return domain;
    };
    let giver = super::infer_h_group_from_replay(&giver_deductions, giver_replay, profile);
    if giver.connection.is_some() || giver.must_clue.contains(&player) {
        return domain;
    }

    let mut allowed = IdentitySet::from_mask(0);
    for candidate in domain.iter() {
        let replacement = before
            .hands
            .iter()
            .flatten()
            .any(|held| held.id != card && held.identity == Some(candidate));
        let required = is_eventually_useful(before, candidate)
            && !replacement
            && (candidate.rank == Rank::Five
                || is_critical_save_identity(before, candidate)
                || is_playable_now(before, candidate)
                || candidate.rank == Rank::Two);
        if !required {
            allowed = allowed.union(IdentitySet::singleton(candidate));
        }
    }
    allowed
}

/// Reuse the historical observation constructor; never import future clues,
/// draws, stack heights, or the observer's later-revealed card identities.
pub(super) fn before_historical_turn(source: &PlayerView, turn: u32) -> PlayerView {
    let mut hands = source
        .hands
        .iter()
        .map(|hand| hand.iter().map(|held| held.id).collect::<Vec<_>>())
        .collect::<Vec<_>>();
    let mut deck_size = source.deck_size;
    for entry in source
        .history
        .iter()
        .rev()
        .take_while(|entry| entry.turn >= turn)
    {
        match entry.event {
            ObservedEvent::Drew { player, card, .. } => {
                hands[player.index()].retain(|held| *held != card);
                deck_size += 1;
            }
            ObservedEvent::Played { player, card, .. }
            | ObservedEvent::Discarded { player, card, .. } => hands[player.index()].push(card),
            ObservedEvent::Clued { .. } => {}
        }
    }
    for hand in &mut hands {
        hand.sort_unstable();
    }
    let end = source.history.partition_point(|entry| entry.turn < turn);
    let history = &source.history[..end];
    let mut facts = vec![
        ClueFacts::default();
        hands
            .iter()
            .flatten()
            .map(|card| card.index() + 1)
            .max()
            .unwrap_or(0)
    ];
    for entry in history {
        if let ObservedEvent::Clued {
            clue,
            touched,
            untouched,
            ..
        } = &entry.event
        {
            for card in touched {
                if let Some(fact) = facts.get_mut(card.index()) {
                    fact.add_positive_clue(*clue);
                }
            }
            for card in untouched {
                if let Some(fact) = facts.get_mut(card.index()) {
                    fact.add_negative_clue(*clue);
                }
            }
        }
    }
    super::prospective::subjective_view_before_action(
        source,
        source.observer,
        history,
        &hands,
        &facts,
        deck_size,
    )
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
    if last.turn + 1 != view.turn || player == view.observer {
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
