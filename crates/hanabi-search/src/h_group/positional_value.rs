//! Conditional clue-economy options, never additional promised plays.
//! <https://hanabi.github.io/level-11/#the-pang-of-guilt>
//! <https://hanabi.github.io/level-11/#bluffs-through-already-clued-cards>

use super::{
    Card, ClueProposal, CluePurpose, HGroupProfile, HGroupRuleId, IdentitySet, LogicalDeductions,
    PlayerId, Rank, compiled_baseline_team, finesse_position, is_playable_now, rule_enabled,
};
use hanabi_core::Action;

#[derive(Clone, Copy, Debug, Default)]
pub(super) struct PositionalValue {
    pub(super) foregone_blind_plays: u8,
    pub(super) conditional_prompt_chains: u8,
}

/// Another giver gets a turn before the blind player: they can look for a
/// Finesse/Bluff instead of paying a direct clue. This is an opportunity cost,
/// not proof that a particular clue exists or permission to ignore an emergency.
pub(super) fn evaluate(
    deductions: &LogicalDeductions,
    profile: HGroupProfile,
    candidate: ClueProposal,
) -> PositionalValue {
    let mut value = PositionalValue::default();
    if !rule_enabled(profile, HGroupRuleId::Bluffs) {
        return value;
    }
    let source = deductions.view();
    let Action::Clue { target, clue } = candidate.action else {
        return value;
    };
    let team = compiled_baseline_team(source, profile);
    let anchors = source
        .hands
        .iter()
        .enumerate()
        .filter_map(|(index, hand)| {
            let actor = PlayerId::new(u8::try_from(index).expect("player index"));
            let projection = team.projection(actor)?;
            let card = finesse_position(hand, &projection.inferred.gotten(), 0)?;
            let identity = card.identity?;
            let distance =
                (index + source.hands.len() - source.current_player.index()) % source.hands.len();
            (distance > 1
                && is_playable_now(source, identity)
                && !projection.inferred.playable_now.contains(&card.id))
            .then_some((actor, card.id, identity))
        })
        .collect::<Vec<_>>();
    if candidate.purpose() == CluePurpose::Play && candidate.immediate_play() {
        value.foregone_blind_plays = u8::from(
            anchors
                .iter()
                .any(|(actor, _, identity)| *actor == target && clue.matches(*identity)),
        );
    }
    if !candidate.is_save() {
        return value;
    }
    let Some(owner) = team.projection(source.observer) else {
        return value;
    };
    let mut options = IdentitySet::default();
    for saved in source.hands[target.index()]
        .iter()
        .filter_map(|card| card.identity)
    {
        let rank = usize::from(saved.rank.number());
        if !clue.matches(saved)
            || rank < 2
            || rank >= Rank::ALL.len()
            || source.play_stacks[saved.suit.index()].len() + 2 != rank
        {
            continue;
        }
        let lower = Card::new(saved.suit, Rank::ALL[rank - 2]);
        if !anchors.iter().any(|(_, _, identity)| *identity == lower) {
            continue;
        }
        let upper = Card::new(saved.suit, Rank::ALL[rank]);
        // The other copy may be in our own hand. Check its remaining physical
        // count and our legal domain; never inspect the simulator's hidden face.
        let accounted = source
            .hands
            .iter()
            .flatten()
            .filter(|card| card.identity == Some(upper))
            .count()
            + source
                .discard_pile
                .iter()
                .filter(|(_, card)| *card == upper)
                .count();
        if accounted < usize::from(upper.rank.copies())
            && source.hands[source.observer.index()].iter().any(|card| {
                deductions
                    .possible_identities(card.id)
                    .is_some_and(|ids| ids.contains(upper))
                    && owner
                        .inferred
                        .cards
                        .iter()
                        .find(|note| note.card == card.id)
                        .is_none_or(|note| note.identities.contains(upper))
            })
        {
            options = options.union(IdentitySet::singleton(saved));
        }
    }
    // Alternatives, not independent promised points or a probability estimate.
    value.conditional_prompt_chains = u8::from(!options.is_empty());
    value
}
