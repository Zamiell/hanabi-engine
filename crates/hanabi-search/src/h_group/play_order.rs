use hanabi_core::{Card, CardId, Clue, ObservedCard, PlayerView, Rank};

use super::{
    CardSet, HGroupInferences, HGroupPhase, HGroupProfile, HGroupRuleId, IdentitySet, rule_enabled,
};

/// Orders concurrently playable cards from the canonical action schedule and
/// Level 3 Priority rules. Interpretation produces obligations; this module is
/// the sole owner of their within-turn ordering.
pub(super) fn ordered_playable_cards(
    view: &PlayerView,
    inferred: &HGroupInferences,
    profile: HGroupProfile,
) -> Vec<CardId> {
    let mut cards = inferred.playable_now.clone();
    if !rule_enabled(profile, HGroupRuleId::BasicStrategy) || cards.len() < 2 {
        return cards;
    }
    let own_hand = &view.hands[view.observer.index()];
    let initial_hand_size = if view.hands.len() <= 3 { 5 } else { 4 };
    let initial_cards = initial_hand_size * view.hands.len();
    let gotten = inferred.gotten();
    let ordering = PlayableOrderContext {
        view,
        inferred,
        profile,
        own_hand,
        initial_cards,
        gotten: &gotten,
    };
    cards.sort_by_key(|card| playable_order_key(&ordering, *card));
    cards
}

type PlayableOrderKey = (bool, bool, bool, bool, bool, usize, u8, u8);

struct PlayableOrderContext<'a> {
    view: &'a PlayerView,
    inferred: &'a HGroupInferences,
    profile: HGroupProfile,
    own_hand: &'a [ObservedCard],
    initial_cards: usize,
    gotten: &'a CardSet,
}

#[derive(Clone, Copy)]
struct PlayableOrderTraits {
    singleton: Option<Card>,
    blind: bool,
    accounted_rank_focus: bool,
    saved_five_focus: bool,
    rank: u8,
    position: usize,
}

fn playable_order_key(context: &PlayableOrderContext<'_>, card: CardId) -> PlayableOrderKey {
    let position = context
        .own_hand
        .iter()
        .position(|candidate| candidate.id == card)
        .unwrap_or(0);
    let note = context.inferred.cards.iter().find(|note| note.card == card);
    let convention_singleton = note
        .filter(|note| note.identities.len() == 1)
        .and_then(|note| note.identities.iter().next());
    let logical_singleton = context
        .own_hand
        .iter()
        .find(|candidate| candidate.id == card)
        .map(|candidate| IdentitySet::from_mask(candidate.clues.identity_mask()))
        .filter(|identities| identities.len() == 1)
        .and_then(|identities| identities.iter().next());
    let singleton = convention_singleton.or(logical_singleton);
    let rank = singleton.map_or(6, |identity| identity.rank.number());
    let fresh_one = rank == 1 && card.index() >= context.initial_cards;
    let starting_one = rank == 1 && !fresh_one;
    let chop_focused = context
        .inferred
        .clues
        .iter()
        .any(|clue| clue.focus == card && clue.focus_was_chop);
    let saved_five_focus = context.inferred.clues.iter().any(|clue| {
        clue.focus == card && clue.focus_was_chop && clue.clue == Clue::Rank(Rank::Five)
    });
    let blind = note.is_some_and(|note| note.play_obligation.is_some());
    let accounted_rank_focus = context.inferred.priority_plays.contains(&card);
    if !rule_enabled(context.profile, HGroupRuleId::Priority)
        || context.inferred.phase == HGroupPhase::EndGame
    {
        let age = if fresh_one {
            usize::MAX - position
        } else if starting_one {
            0
        } else {
            position + 1
        };
        return (
            !blind,
            !accounted_rank_focus,
            !chop_focused,
            !fresh_one,
            false,
            age,
            0,
            0,
        );
    }
    priority_playable_order_key(
        context,
        card,
        PlayableOrderTraits {
            singleton,
            blind,
            accounted_rank_focus,
            saved_five_focus,
            rank,
            position,
        },
    )
}

fn priority_playable_order_key(
    context: &PlayableOrderContext<'_>,
    card: CardId,
    traits: PlayableOrderTraits,
) -> PlayableOrderKey {
    let PlayableOrderTraits {
        singleton,
        blind,
        accounted_rank_focus,
        saved_five_focus,
        rank,
        position,
    } = traits;
    let leads_other = singleton.is_some_and(|identity| {
        let next = Card::new(
            identity.suit,
            Rank::ALL
                .get(identity.rank.index() + 1)
                .copied()
                .unwrap_or(Rank::Five),
        );
        identity.rank != Rank::Five
            && context
                .view
                .hands
                .iter()
                .enumerate()
                .filter(|(player, _)| *player != context.view.observer.index())
                .any(|(_, hand)| {
                    hand.iter()
                        .any(|candidate| candidate.identity == Some(next))
                })
    });
    let leads_self = singleton.is_some_and(|identity| {
        if identity.rank == Rank::Five {
            return false;
        }
        let next = Card::new(identity.suit, Rank::ALL[identity.rank.index() + 1]);
        context.inferred.cards.iter().any(|candidate| {
            candidate.card != card
                && context.gotten.contains(&candidate.card)
                && candidate.identities.contains(next)
        })
    });
    (
        !blind,
        !accounted_rank_focus,
        !saved_five_focus,
        !leads_other,
        !leads_self,
        usize::from(rank != 5),
        rank,
        u8::try_from(context.own_hand.len().saturating_sub(position)).unwrap_or(u8::MAX),
    )
}
