use std::collections::HashMap;
use std::sync::OnceLock;

use hanabi_core::{
    Card, CardId, Clue, ClueFacts, MAX_CLUE_TOKENS, ObservedCard, ObservedEvent,
    ObservedHistoryEntry, PlayerId, PlayerView, Rank,
};

use crate::{HGroupProfile, LogicalDeductions, information_set::HandAssignmentVisit};

use super::{
    HGroupState, convention_card_inferences, identity_of, next_player, replay_h_group_inner,
};

/// Central observer projection used by all giver/recipient convention checks.
///
/// A projected visible identity remains unknown when it was hidden from the
/// source observer. Callers must treat that as nested uncertainty rather than
/// filling it from simulator truth.
pub(super) struct PerspectiveProjector<'a> {
    source: &'a PlayerView,
    profile: HGroupProfile,
    source_known_cards: OnceLock<HashMap<CardId, Card>>,
}

impl<'a> PerspectiveProjector<'a> {
    pub(super) const fn new(source: &'a PlayerView, profile: HGroupProfile) -> Self {
        Self {
            source,
            profile,
            source_known_cards: OnceLock::new(),
        }
    }

    pub(super) fn project(
        &self,
        observer: PlayerId,
        model_other_players: bool,
    ) -> Option<(LogicalDeductions, HGroupState)> {
        let source_hand_is_resolved = self.source.hands[self.source.observer.index()]
            .iter()
            .all(|card| card.identity.is_some());
        let source_known_cards = if model_other_players
            && observer != self.source.observer
            && !source_hand_is_resolved
        {
            self.source_known_cards.get_or_init(|| {
                // Another player can see convention-resolved cards in the
                // source observer's hand. Compute that observer-relative map
                // once when this projector is reused for several recipients.
                let mut source_observation = self.source.clone();
                for card in &mut source_observation.hands[self.source.observer.index()] {
                    card.identity = None;
                }
                let Some(source_deductions) = LogicalDeductions::new(source_observation).ok()
                else {
                    return HashMap::new();
                };
                let source_replay = replay_h_group_inner(&source_deductions, self.profile, false);
                convention_card_inferences(&source_deductions, &source_replay)
                    .into_iter()
                    .filter_map(|note| {
                        (note.identities.len() == 1)
                            .then(|| {
                                note.identities
                                    .iter()
                                    .next()
                                    .map(|identity| (note.card, identity))
                            })
                            .flatten()
                    })
                    .collect::<HashMap<_, _>>()
            })
        } else {
            static EMPTY: OnceLock<HashMap<CardId, Card>> = OnceLock::new();
            EMPTY.get_or_init(HashMap::new)
        };
        let mut view = self.source.clone();
        view.observer = observer;
        for (player, hand) in view.hands.iter_mut().enumerate() {
            for card in hand {
                card.identity = (player != observer.index())
                    .then(|| {
                        identity_of(self.source, card.id)
                            .or_else(|| source_known_cards.get(&card.id).copied())
                    })
                    .flatten();
            }
        }
        for entry in &mut view.history {
            if let ObservedEvent::Drew {
                player,
                card,
                identity,
            } = &mut entry.event
            {
                *identity = (*player != observer)
                    .then(|| identity_of(self.source, *card))
                    .flatten();
            }
        }
        let deductions = LogicalDeductions::new(view).ok()?;
        let replay = replay_h_group_inner(&deductions, self.profile, model_other_players);
        Some((deductions, replay))
    }

    /// Projects an owned fully resolved sampled world without cloning it a
    /// second time. Nested hand-world validation has already assigned every
    /// current hand identity consistently.
    pub(super) fn project_resolved_owned(
        mut view: PlayerView,
        profile: HGroupProfile,
        observer: PlayerId,
    ) -> Option<(LogicalDeductions, HGroupState)> {
        debug_assert!(
            view.hands
                .iter()
                .flatten()
                .all(|card| card.identity.is_some())
        );
        let identities: [Option<Card>; 50] =
            core::array::from_fn(|index| identity_of(&view, CardId::new(index)));
        view.observer = observer;
        for (player, hand) in view.hands.iter_mut().enumerate() {
            for card in hand {
                card.identity = (player != observer.index())
                    .then(|| identities[card.id.index()])
                    .flatten();
            }
        }
        for entry in &mut view.history {
            if let ObservedEvent::Drew {
                player,
                card,
                identity,
            } = &mut entry.event
            {
                *identity = (*player != observer)
                    .then(|| identities[card.index()])
                    .flatten();
            }
        }
        let deductions = LogicalDeductions::new(view).ok()?;
        let replay = replay_h_group_inner(&deductions, profile, true);
        Some((deductions, replay))
    }

    /// Visits complete source-hand worlds for nested recipient reasoning.
    /// Assignments respect direct clues, visible copies, and joint copy counts.
    pub(super) fn visit_source_hand_worlds(
        &self,
        limit: usize,
        mut visitor: impl FnMut(&PlayerView) -> bool,
    ) -> Option<HandAssignmentVisit> {
        let deductions = LogicalDeductions::new(self.source.clone()).ok()?;
        Some(deductions.visit_hand_assignments(limit, |assignment| {
            let mut world = self.source.clone();
            for (card, identity) in assignment {
                if let Some(observed) = world.hands[self.source.observer.index()]
                    .iter_mut()
                    .find(|candidate| candidate.id == *card)
                {
                    observed.identity = Some(*identity);
                }
            }
            visitor(&world)
        }))
    }
}

/// Applies public hypothetical transitions without consulting simulator truth.
pub(super) struct ProspectiveTransition;

impl ProspectiveTransition {
    pub(super) fn clue(
        source: &PlayerView,
        target: PlayerId,
        clue: Clue,
        touched: &[CardId],
    ) -> PlayerView {
        let untouched = source.hands[target.index()]
            .iter()
            .map(|card| card.id)
            .filter(|card| !touched.contains(card))
            .collect::<Vec<_>>();
        let mut after = source.clone();
        for card in &mut after.hands[target.index()] {
            if touched.contains(&card.id) {
                card.clues.add_positive_clue(clue);
            } else {
                card.clues.add_negative_clue(clue);
            }
        }
        after.history.push(ObservedHistoryEntry {
            turn: source.turn,
            event: ObservedEvent::Clued {
                giver: source.observer,
                target,
                clue,
                touched: touched.to_vec(),
                untouched,
            },
        });
        after.turn = after.turn.saturating_add(1);
        after.current_player = next_player(source.current_player, source.hands.len());
        after.clue_tokens = after.clue_tokens.saturating_sub(1);
        after
    }

    /// Applies a successful play and the public shape of its subsequent draw.
    /// The new identity remains unknown because it was in the source observer's
    /// deck, but its stable card id and hand position are public after drawing.
    pub(super) fn successful_play(
        source: &PlayerView,
        player: PlayerId,
        card: CardId,
        identity: Card,
    ) -> PlayerView {
        let mut after = source.clone();
        after.history.push(ObservedHistoryEntry {
            turn: source.turn,
            event: ObservedEvent::Played {
                player,
                card,
                identity,
                successful: true,
            },
        });
        after.hands[player.index()].retain(|candidate| candidate.id != card);
        after.play_stacks[identity.suit.index()].push((card, identity));
        if identity.rank == Rank::Five {
            after.clue_tokens = after.clue_tokens.saturating_add(1).min(MAX_CLUE_TOKENS);
        }
        if source.deck_size > 0 {
            let drawn = CardId::new(50 - source.deck_size);
            after.hands[player.index()].push(ObservedCard {
                id: drawn,
                identity: None,
                clues: ClueFacts::default(),
            });
            after.history.push(ObservedHistoryEntry {
                turn: source.turn,
                event: ObservedEvent::Drew {
                    player,
                    card: drawn,
                    identity: None,
                },
            });
            after.deck_size -= 1;
            if after.deck_size == 0 {
                after.final_turns_remaining = Some(
                    u8::try_from(after.hands.len())
                        .expect("standard Hanabi has at most five players"),
                );
            }
        }
        after.turn = after.turn.saturating_add(1);
        after.current_player = next_player(player, source.hands.len());
        after
    }
}

#[cfg(test)]
mod tests {
    use hanabi_core::{FullState, PlayerId, standard_deck};

    use super::*;

    #[test]
    fn successful_play_projects_the_public_draw_shape() {
        let state = FullState::new_standard(3, standard_deck()).unwrap();
        let source = state.view_for(PlayerId::new(0)).unwrap();
        let played = source.hands[0][0].id;
        let identity = state.card(played).unwrap();

        let after =
            ProspectiveTransition::successful_play(&source, PlayerId::new(0), played, identity);

        assert_eq!(after.deck_size, source.deck_size - 1);
        assert_eq!(after.hands[0].len(), source.hands[0].len());
        assert_eq!(after.hands[0].last().unwrap().id, CardId::new(15));
        assert_eq!(after.hands[0].last().unwrap().identity, None);
        assert!(matches!(
            after.history.last().map(|entry| &entry.event),
            Some(ObservedEvent::Drew {
                player,
                card,
                identity: None,
            }) if *player == PlayerId::new(0) && *card == CardId::new(15)
        ));
    }
}
