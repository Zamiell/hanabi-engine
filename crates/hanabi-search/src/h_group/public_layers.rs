//! Public demonstration of a layer must survive nested hidden hands.
//! <https://hanabi.github.io/level-5/#the-layered-finesse>

use super::{
    Card, CardId, CardSet, Clue, ClueFacts, ConnectionManager, HGroupClueInterpretation,
    ObservedEvent, ObservedHistoryEntry, PlayerId, PlayerView, Rank, finesse_position_id,
    identity_of, next_player,
};

pub(super) struct DemonstratedLayer {
    pub(super) clue_index: usize,
    pub(super) actor: PlayerId,
    pub(super) expected: Card,
    pub(super) focus_identity: Card,
    pub(super) cards: Vec<CardId>,
}

/// Recover a uniquely attributable delayed colour clue when a nested view
/// cannot see the blind player's remaining cards. The public off-colour play
/// supplies the evidence; no hidden card is assigned the promised identity.
#[allow(clippy::too_many_arguments)]
pub(super) fn demonstrated_hidden_layer(
    view: &PlayerView,
    entry: &ObservedHistoryEntry,
    hands: &[Vec<CardId>],
    clues: &[HGroupClueInterpretation],
    facts: &[ClueFacts],
    explicitly_clued: &CardSet,
    forced_playable: &CardSet,
    pending: &ConnectionManager,
    heights: [u8; 5],
) -> Option<DemonstratedLayer> {
    let ObservedEvent::Played {
        player,
        card,
        identity,
        successful: true,
    } = entry.event
    else {
        return None;
    };
    if explicitly_clued.contains(&card)
        || forced_playable.contains(&card)
        || pending.iter().any(|connection| connection.actor == player)
        || finesse_position_id(&hands[player.index()], explicitly_clued, 0) != Some(card)
        || !hands[player.index()]
            .iter()
            .any(|card| identity_of(view, *card).is_none())
    {
        return None;
    }
    let mut candidates = clues.iter().enumerate().filter_map(|(clue_index, clue)| {
        let Clue::Suit(suit) = clue.clue else {
            return None;
        };
        let height = clue.stack_heights[suit.index()];
        if clue.target != next_player(player, hands.len())
            || clue.giver == player
            || next_player(clue.giver, hands.len()) == player
            || clue.turn + 1 >= entry.turn
            || suit == identity.suit
            || !hands[clue.target.index()].contains(&clue.focus)
            || !clue.save_identities.is_empty()
            || heights[suit.index()] != height
            || usize::from(height) + 1 >= Rank::ALL.len()
        {
            return None;
        }
        let expected = Card::new(suit, Rank::ALL[usize::from(height)]);
        let focus_identity = Card::new(suit, Rank::ALL[usize::from(height) + 1]);
        if !facts[clue.focus.index()].allows(focus_identity)
            || identity_of(view, clue.focus).is_some_and(|known| known != focus_identity)
        {
            return None;
        }
        Some(DemonstratedLayer {
            clue_index,
            actor: player,
            expected,
            focus_identity,
            cards: hands[player.index()]
                .iter()
                .rev()
                .copied()
                .filter(|card| !explicitly_clued.contains(card))
                .collect(),
        })
    });
    let candidate = candidates.next()?;
    // An ambiguous attribution is not shared proof of either clue's meaning.
    candidates.next().is_none().then_some(candidate)
}
