use std::collections::HashMap;
use std::sync::OnceLock;

use hanabi_core::{
    Card, CardId, Clue, ClueFacts, MAX_CLUE_TOKENS, ObservedCard, ObservedEvent,
    ObservedHistoryEntry, PlayerId, PlayerView, Rank,
};

use crate::{HGroupProfile, LogicalDeductions, information_set::HandAssignmentVisit};

use super::{
    HGroupState, PerspectiveDepth, convention_card_inferences, identity_of, next_player,
    replay_h_group_inner,
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
        depth: PerspectiveDepth,
    ) -> Option<(LogicalDeductions, HGroupState)> {
        let source_hand_is_resolved = self.source.hands[self.source.observer.index()]
            .iter()
            .all(|card| card.identity.is_some());
        let source_known_cards = if depth.models_other_players()
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
                let source_replay = replay_h_group_inner(
                    &source_deductions,
                    self.profile,
                    PerspectiveDepth::ObserverOnly,
                    false,
                );
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
        let replay = replay_h_group_inner(&deductions, self.profile, depth, false);
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
        if view
            .hands
            .iter()
            .flatten()
            .any(|card| card.identity.is_none())
        {
            // Symbolic continuations deliberately represent hypothetical draws
            // as blank cards. A source-hand assignment resolves only the
            // source observer's hidden cards, so it cannot turn such a partial
            // continuation into the complete sampled world required here.
            return None;
        }
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
        let replay = replay_h_group_inner(
            &deductions,
            profile,
            PerspectiveDepth::NestedRecipients,
            false,
        );
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
        Self::clue_by(source, source.observer, target, clue, touched)
    }

    pub(super) fn clue_by(
        source: &PlayerView,
        giver: PlayerId,
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
                giver,
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
        Self::card_action(source, player, card, identity, true, true)
    }

    pub(super) fn discard(
        source: &PlayerView,
        player: PlayerId,
        card: CardId,
        identity: Card,
    ) -> PlayerView {
        Self::card_action(source, player, card, identity, false, true)
    }

    pub(super) fn play(
        source: &PlayerView,
        player: PlayerId,
        card: CardId,
        identity: Card,
        successful: bool,
    ) -> PlayerView {
        Self::card_action(source, player, card, identity, true, successful)
    }

    fn card_action(
        source: &PlayerView,
        player: PlayerId,
        card: CardId,
        identity: Card,
        play: bool,
        successful: bool,
    ) -> PlayerView {
        let mut after = source.clone();
        let event = if play {
            ObservedEvent::Played {
                player,
                card,
                identity,
                successful,
            }
        } else {
            ObservedEvent::Discarded {
                player,
                card,
                identity,
            }
        };
        after.history.push(ObservedHistoryEntry {
            turn: source.turn,
            event,
        });
        after.hands[player.index()].retain(|candidate| candidate.id != card);
        if play && successful {
            after.play_stacks[identity.suit.index()].push((card, identity));
        } else {
            after.discard_pile.push((card, identity));
        }
        if play && !successful {
            after.strikes = after.strikes.saturating_add(1);
        }
        if !play || successful && identity.rank == Rank::Five {
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
    use hanabi_core::{Action, FullState, PlayerId, Rank, standard_deck};

    use super::*;
    use crate::{HGroupLevel, HGroupProfile};

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

    #[test]
    fn discard_projects_the_public_pile_token_and_blank_draw() {
        let state = FullState::new_standard(3, standard_deck()).unwrap();
        let source = state.view_for(PlayerId::new(0)).unwrap();
        let discarded = source.hands[0][0].id;
        let identity = state.card(discarded).unwrap();

        let after = ProspectiveTransition::discard(&source, PlayerId::new(0), discarded, identity);

        assert!(after.discard_pile.contains(&(discarded, identity)));
        assert_eq!(after.clue_tokens, MAX_CLUE_TOKENS);
        assert_eq!(after.deck_size, source.deck_size - 1);
        assert_eq!(after.hands[0].last().unwrap().identity, None);
    }

    #[test]
    fn resolved_projection_rejects_a_symbolic_blank_draw() {
        let state = FullState::new_standard(3, standard_deck()).unwrap();
        let mut source = state.view_for(PlayerId::new(0)).unwrap();
        for card in source.hands.iter_mut().flatten() {
            card.identity = state.card(card.id);
        }
        let played = source.hands[0][0].id;
        let identity = state.card(played).unwrap();
        let after =
            ProspectiveTransition::successful_play(&source, PlayerId::new(0), played, identity);

        assert!(
            PerspectiveProjector::project_resolved_owned(
                after,
                HGroupProfile::Max,
                PlayerId::new(1),
            )
            .is_none()
        );
    }

    #[test]
    fn hypothetical_clue_matches_the_recipients_actual_projection() {
        let mut state = FullState::new_standard(3, standard_deck()).unwrap();
        let giver = PlayerId::new(0);
        let target = PlayerId::new(1);
        let source = state.view_for(giver).unwrap();
        let rank = source.hands[target.index()]
            .iter()
            .filter_map(|card| card.identity)
            .map(|card| card.rank)
            .find(|rank| *rank == Rank::One)
            .unwrap_or_else(|| source.hands[target.index()][0].identity.unwrap().rank);
        let clue = Clue::Rank(rank);
        let touched = source.hands[target.index()]
            .iter()
            .filter(|card| card.identity.is_some_and(|identity| clue.matches(identity)))
            .map(|card| card.id)
            .collect::<Vec<_>>();
        let profile = HGroupProfile::Level(HGroupLevel::Level1);

        let hypothetical = ProspectiveTransition::clue(&source, target, clue, &touched);
        let (hypothetical_deductions, hypothetical_replay) =
            PerspectiveProjector::new(&hypothetical, profile)
                .project(target, PerspectiveDepth::NestedRecipients)
                .expect("hypothetical recipient projection succeeds");
        let hypothetical_inferences = super::super::infer_h_group_from_replay(
            &hypothetical_deductions,
            hypothetical_replay,
            profile,
        );

        state.apply(Action::Clue { target, clue }).unwrap();
        let actual_deductions = LogicalDeductions::new(state.view_for(target).unwrap()).unwrap();
        let actual_replay = replay_h_group_inner(
            &actual_deductions,
            profile,
            PerspectiveDepth::ObserverOnly,
            false,
        );
        let actual_inferences =
            super::super::infer_h_group_from_replay(&actual_deductions, actual_replay, profile);

        assert_eq!(hypothetical_inferences.clues, actual_inferences.clues);
        assert_eq!(
            hypothetical_inferences.playable_now,
            actual_inferences.playable_now
        );
        assert_eq!(
            hypothetical_inferences.saved_cards().collect::<Vec<_>>(),
            actual_inferences.saved_cards().collect::<Vec<_>>()
        );
        assert_eq!(
            hypothetical_inferences.connection,
            actual_inferences.connection
        );
    }
}
