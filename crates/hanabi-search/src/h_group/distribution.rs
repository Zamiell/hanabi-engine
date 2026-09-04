//! Scheduling evidence for the intentional duplicate in a Distribution Clue.
//! <https://hanabi.github.io/level-8/#the-distribution-clue>

use super::{Card, PlayerId, Rank};

pub(super) fn recognize(
    context: &super::HGroupTurnContext<'_>,
    view: &super::PlayerView,
    effects: &mut super::HGroupRuleEffects<'_>,
) {
    let super::ObservedEvent::Clued {
        giver,
        target,
        touched,
        ..
    } = &context.entry.event
    else {
        return;
    };
    if context.before.deck_size > context.before.hands.len() {
        return;
    }
    let forced = &*effects.forced_playable;
    let commitments = context
        .before
        .hands
        .iter()
        .enumerate()
        .flat_map(|(player, hand)| {
            hand.iter().filter_map(move |card| {
                let committed = context.before.already_playing.contains(card)
                    || forced.contains(card)
                    || super::was_clued_before(view, context.entry.turn, *card);
                committed
                    .then(|| {
                        context.historical.identity(*card).or_else(|| {
                            let set = super::IdentitySet::from_mask(
                                context.before.facts[card.index()].identity_mask(),
                            );
                            (set.len() == 1).then(|| set.iter().next()).flatten()
                        })
                    })
                    .flatten()
                    .map(|identity| {
                        (
                            PlayerId::new(u8::try_from(player).expect("player count fits")),
                            identity,
                        )
                    })
            })
        })
        .collect::<Vec<_>>();
    let hand = &context.before.hands[target.index()];
    let mut gotten = super::protected_cards(
        effects.explicitly_clued,
        effects.invisibly_clued,
        effects.chop_moved,
    );
    for card in touched {
        if !super::was_clued_before(view, context.entry.turn, *card) {
            gotten.remove(card);
        }
    }
    let Some(focus) = super::focus(hand, touched, super::chop(hand, &gotten), &gotten) else {
        return;
    };
    let identities = context.historical.identity(focus).map_or_else(
        || super::IdentitySet::from_mask(context.after.facts[focus.index()].identity_mask()),
        super::IdentitySet::singleton,
    );
    let feasible = identities
        .iter()
        .filter(|identity| {
            timing(
                context.before.hands.len(),
                *giver,
                *target,
                context.before.stack_heights[identity.suit.index()],
                *identity,
                &commitments,
            )
            .is_some()
        })
        .collect::<Vec<_>>();
    let [identity] = feasible.as_slice() else {
        return;
    };
    effects.already_playing.insert(focus);
    super::push_signal(
        effects.signals,
        context.entry,
        *giver,
        Some(*target),
        super::HGroupMoveKind::DistributionClue,
        vec![focus],
        Some(*identity),
    );
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct DistributionTiming {
    pub(super) before: usize,
    pub(super) after: usize,
}

/// Compare the same known stack suffix before and after adding the duplicate.
/// Turn zero belongs to the giver; each play must follow its predecessor.
/// Unknown higher cards are not assigned owners or invented as continuations.
pub(super) fn timing(
    players: usize,
    giver: PlayerId,
    target: PlayerId,
    height: u8,
    identity: Card,
    commitments: &[(PlayerId, Card)],
) -> Option<DistributionTiming> {
    if identity.rank.number() <= height {
        return None;
    }
    let mut owners: [Vec<PlayerId>; 5] = std::array::from_fn(|_| Vec::new());
    for (owner, card) in commitments {
        if card.suit == identity.suit && card.rank.number() > height {
            let list = &mut owners[usize::from(card.rank.number() - 1)];
            if !list.contains(owner) {
                list.push(*owner);
            }
        }
    }
    let rank_index = usize::from(identity.rank.number() - 1);
    if owners[rank_index].contains(&target)
        || !owners[rank_index]
            .iter()
            .any(|owner| owners.iter().filter(|list| list.contains(owner)).count() >= 2)
    {
        return None;
    }
    let end = (usize::from(height)..Rank::ALL.len())
        .take_while(|rank| !owners[*rank].is_empty())
        .last()?;
    if rank_index > end {
        return None;
    }
    let finish = |owners: &[Vec<PlayerId>; 5]| {
        owners[usize::from(height)..=end]
            .iter()
            .fold(0, |turn, choices| {
                choices
                    .iter()
                    .map(|owner| {
                        let seat = (owner.index() + players - giver.index()) % players;
                        let distance = (seat + players - turn % players) % players;
                        turn + if distance == 0 { players } else { distance }
                    })
                    .min()
                    .expect("known suffix has an owner for every rank")
            })
    };
    let before = finish(&owners);
    owners[rank_index].push(target);
    let after = finish(&owners);
    (after < before).then_some(DistributionTiming { before, after })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::h_group::Suit;

    #[test]
    fn queued_green_stack_finishes_four_turns_earlier() {
        let bob = PlayerId::new(1);
        let cathy = PlayerId::new(2);
        let donald = PlayerId::new(3);
        let green = |rank| Card::new(Suit::Green, rank);
        let cards = [
            (cathy, green(Rank::Three)),
            (cathy, green(Rank::Four)),
            (bob, green(Rank::Five)),
        ];
        assert_eq!(
            timing(4, bob, donald, 2, green(Rank::Four), &cards),
            Some(DistributionTiming {
                before: 8,
                after: 4
            })
        );
        assert_eq!(timing(4, bob, cathy, 2, green(Rank::Four), &cards), None);
        assert_eq!(
            timing(4, bob, donald, 2, green(Rank::Three), &cards),
            None,
            "duplicating the first play does not accelerate this stack"
        );
        assert_eq!(
            timing(4, bob, donald, 1, green(Rank::Four), &cards),
            None,
            "missing predecessors cannot be assumed"
        );
    }
}
