use super::{
    Card, CardId, CardSet, Clue, HGroupCardInference, HGroupIdentityStatus, HGroupProfile,
    HGroupState, IdentitySet, PlayerId, PlayerView, Rank, is_critical, prospective_clue_view,
    subjective_convention_cards,
};

/// Convention-aware value of the negative information supplied by a clue.
///
/// Field order is semantic: resolving the action class of a promised card is
/// more useful than saving a later clue, which is more useful than a weighted
/// reduction of otherwise equivalent identities.
#[derive(Clone, Copy, Debug, Default, Eq, Ord, PartialEq, PartialOrd)]
pub(super) struct ConventionInformationValue {
    pub(super) promised_action_certainty: u16,
    pub(super) future_clue_savings: u16,
    pub(super) weighted_eliminations: u16,
    pub(super) eliminated_identities: u16,
}

pub(super) fn convention_information_value(
    source: &PlayerView,
    profile: HGroupProfile,
    replay: &HGroupState,
    target: PlayerId,
    clue: Clue,
    touched: &[CardId],
) -> ConventionInformationValue {
    let after_view = prospective_clue_view(source, target, clue, touched);
    let (before, after) = match (
        subjective_convention_cards(source, profile, target),
        subjective_convention_cards(&after_view, profile, target),
    ) {
        (Some(before), Some(after)) => (before, after),
        _ => (
            direct_card_inferences(source, target),
            direct_card_inferences(&after_view, target),
        ),
    };
    let promised = promised_cards(replay, target);

    source.hands[target.index()]
        .iter()
        .filter(|card| !touched.contains(&card.id))
        .filter_map(|card| {
            let before = before.iter().find(|note| note.card == card.id)?;
            let after = after.iter().find(|note| note.card == card.id)?;
            information_for_card(source, before, after, promised.contains(&card.id))
        })
        .fold(ConventionInformationValue::default(), |total, card| {
            ConventionInformationValue {
                promised_action_certainty: total
                    .promised_action_certainty
                    .saturating_add(card.promised_action_certainty),
                future_clue_savings: total
                    .future_clue_savings
                    .saturating_add(card.future_clue_savings),
                weighted_eliminations: total
                    .weighted_eliminations
                    .saturating_add(card.weighted_eliminations),
                eliminated_identities: total
                    .eliminated_identities
                    .saturating_add(card.eliminated_identities),
            }
        })
}

fn direct_card_inferences(source: &PlayerView, target: PlayerId) -> Vec<HGroupCardInference> {
    source.hands[target.index()]
        .iter()
        .map(|card| HGroupCardInference {
            card: card.id,
            identities: IdentitySet::from_mask(card.clues.identity_mask()),
            promised_identity: None,
            identity_status: HGroupIdentityStatus::Settled,
            focused: false,
            saved: false,
            finessed: false,
            play_obligation: None,
        })
        .collect()
}

fn promised_cards(replay: &HGroupState, target: PlayerId) -> CardSet {
    replay
        .pending_connections
        .iter()
        .filter(|connection| {
            connection.actor == target && replay.pending_connections.is_active(connection)
        })
        .flat_map(|connection| connection.cards.iter().copied())
        .chain(replay.cards.forced_playable.iter().copied())
        .chain(replay.cards.already_playing.iter().copied())
        .collect()
}

fn information_for_card(
    source: &PlayerView,
    before: &HGroupCardInference,
    after: &HGroupCardInference,
    explicitly_promised: bool,
) -> Option<ConventionInformationValue> {
    if after.identities.is_empty() {
        return None;
    }
    let eliminated = before.identities.without(after.identities);
    if eliminated.is_empty() {
        return None;
    }

    let convention_weight = 1
        + u16::from(explicitly_promised || before.play_obligation.is_some()) * 2
        + u16::from(before.focused)
        + u16::from(before.saved);
    let promised =
        explicitly_promised || before.play_obligation.is_some() || before.focused || before.saved;
    let became_action_certain = action_classes(source, before.identities).count_ones() > 1
        && action_classes(source, after.identities).is_power_of_two();
    let future_clue_savings = description_certainty(after.identities)
        .saturating_sub(description_certainty(before.identities));

    Some(ConventionInformationValue {
        promised_action_certainty: u16::from(promised && became_action_certain),
        future_clue_savings,
        weighted_eliminations: eliminated
            .iter()
            .map(|identity| identity_relevance(source, identity) * convention_weight)
            .sum(),
        eliminated_identities: u16::try_from(eliminated.len()).unwrap_or(u16::MAX),
    })
}

/// Bit 0: already trash; bit 1: playable now; bit 2: a future card.
fn action_classes(source: &PlayerView, identities: IdentitySet) -> u8 {
    identities.iter().fold(0, |classes, identity| {
        let stack = source.play_stacks[identity.suit.index()].len();
        let rank = usize::from(identity.rank.number());
        classes
            | if rank <= stack {
                1
            } else if rank == stack + 1 {
                2
            } else {
                4
            }
    })
}

/// Counts identity dimensions that no longer need a future positive clue.
fn description_certainty(identities: IdentitySet) -> u16 {
    let mut suits = 0_u8;
    let mut ranks = 0_u8;
    for identity in identities.iter() {
        suits |= 1 << identity.suit.index();
        ranks |= 1 << identity.rank.index();
    }
    u16::from(suits.is_power_of_two())
        + u16::from(ranks.is_power_of_two())
        + u16::from(identities.len() == 1)
}

fn identity_relevance(source: &PlayerView, identity: Card) -> u16 {
    let stack = u8::try_from(source.play_stacks[identity.suit.index()].len())
        .expect("a standard Hanabi stack has at most five cards");
    let timing = if identity.rank.number() <= stack {
        1
    } else {
        match identity.rank.number() - stack - 1 {
            0 => 8,
            1 => 6,
            2 => 4,
            3 => 3,
            _ => 2,
        }
    };
    let criticality = if is_critical(source, identity) {
        5
    } else if identity.rank == Rank::Five {
        3
    } else {
        0
    };
    timing + criticality
}

#[cfg(test)]
mod tests {
    use super::*;
    use hanabi_core::{GameStatus, Suit};

    #[test]
    fn rank_information_can_beat_redundant_color_information() {
        let source = PlayerView {
            observer: PlayerId::new(0),
            current_player: PlayerId::new(0),
            turn: 0,
            hands: vec![Vec::new(), Vec::new()],
            deck_size: 40,
            play_stacks: std::array::from_fn(|_| Vec::new()),
            discard_pile: Vec::new(),
            clue_tokens: 8,
            strikes: 0,
            final_turns_remaining: None,
            status: GameStatus::InProgress,
            history: Vec::new(),
        };
        let identities = [Suit::Red, Suit::Yellow]
            .into_iter()
            .flat_map(|suit| Rank::ALL.map(|rank| Card::new(suit, rank)))
            .fold(IdentitySet::default(), |set, identity| {
                set.union(IdentitySet::singleton(identity))
            });
        let before = HGroupCardInference {
            card: CardId::new(0),
            identities,
            promised_identity: None,
            identity_status: HGroupIdentityStatus::Settled,
            focused: false,
            saved: false,
            finessed: false,
            play_obligation: None,
        };
        let after_rank = HGroupCardInference {
            identities: identities.without(
                IdentitySet::singleton(Card::new(Suit::Red, Rank::Four))
                    .union(IdentitySet::singleton(Card::new(Suit::Yellow, Rank::Four))),
            ),
            ..before
        };

        let rank_information = information_for_card(&source, &before, &after_rank, false)
            .expect("a rank negative eliminates two live identities");
        let redundant_color_information =
            information_for_card(&source, &before, &before, false).unwrap_or_default();

        assert!(rank_information > redundant_color_information);
    }
}
