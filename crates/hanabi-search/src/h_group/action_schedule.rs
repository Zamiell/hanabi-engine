use hanabi_core::{Card, CardId, PlayerId, PlayerView};

use super::{
    CardSet, HGroupState, IdentityClaims, IdentitySet, PromiseId, identity_of, is_playable_at,
    next_player,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum ScheduledAction {
    Play,
    Discard,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum ObligationSource {
    DirectPlay,
    Connection(PromiseId),
    ForcedPlay,
    RequiredDiscard,
}

/// One live convention commitment. `identities` is the domain for which the
/// action succeeds; `promised_identity` is what a connection requires the
/// owner to act as though the card is.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct ActionObligation {
    pub(super) actor: PlayerId,
    pub(super) card: CardId,
    pub(super) action: ScheduledAction,
    pub(super) identities: IdentitySet,
    pub(super) promised_identity: Option<Card>,
    pub(super) source: ObligationSource,
}

/// Unified read model over direct plays, connection responses, forced plays,
/// and required discards. Their lifecycle remains in the canonical owners.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(super) struct ActionSchedule {
    obligations: Vec<ActionObligation>,
    blocked_cards: CardSet,
}

impl ActionSchedule {
    pub(super) fn from_replay(view: &PlayerView, replay: &HGroupState) -> Self {
        let claims = IdentityClaims::new(view, replay);
        let mut obligations = Vec::new();
        let mut scheduled = CardSet::default();

        for connection in replay
            .pending_connections
            .iter()
            .filter(|connection| replay.pending_connections.is_active(connection))
            .filter(|connection| {
                !replay.is_exact_transfer(
                    connection
                        .cards
                        .first()
                        .copied()
                        .unwrap_or(connection.focus),
                    connection.expected,
                )
            })
        {
            let Some(card) = connection.cards.first().copied() else {
                continue;
            };
            if scheduled.insert(card) {
                obligations.push(ActionObligation {
                    actor: connection.actor,
                    card,
                    action: ScheduledAction::Play,
                    identities: IdentitySet::singleton(connection.expected),
                    promised_identity: Some(connection.expected),
                    source: ObligationSource::Connection(connection.promise),
                });
            }
        }
        for (cards, source) in [
            (&replay.cards.forced_playable, ObligationSource::ForcedPlay),
            (&replay.cards.already_playing, ObligationSource::DirectPlay),
        ] {
            for card in cards {
                let Some(actor) = owner_of(replay, *card) else {
                    continue;
                };
                if scheduled.insert(*card) {
                    let identities = claims
                        .exact_identity(*card)
                        .map_or_else(IdentitySet::default, IdentitySet::singleton);
                    obligations.push(ActionObligation {
                        actor,
                        card: *card,
                        action: ScheduledAction::Play,
                        identities,
                        promised_identity: None,
                        source,
                    });
                }
            }
        }
        for card in &replay.cards.discard_now {
            let Some(actor) = owner_of(replay, *card) else {
                continue;
            };
            obligations.push(ActionObligation {
                actor,
                card: *card,
                action: ScheduledAction::Discard,
                identities: IdentitySet::default(),
                promised_identity: None,
                source: ObligationSource::RequiredDiscard,
            });
        }
        let observer = view.observer;
        let blocked_cards = replay
            .pending_connections
            .iter()
            .filter(|pending| {
                pending
                    .cards
                    .first()
                    .is_none_or(|card| !replay.is_exact_transfer(*card, pending.expected))
            })
            .flat_map(|pending| {
                let blocked_candidates = if pending.actor != observer {
                    &[][..]
                } else if replay.pending_connections.is_active(pending) {
                    pending.cards.get(1..).unwrap_or_default()
                } else {
                    pending.cards.as_slice()
                };
                blocked_candidates
                    .iter()
                    .copied()
                    .chain(core::iter::once(pending.focus))
            })
            .collect();
        Self {
            obligations,
            blocked_cards,
        }
    }

    pub(super) fn plays_for(&self, actor: PlayerId) -> impl Iterator<Item = &ActionObligation> {
        self.obligations.iter().filter(move |obligation| {
            obligation.actor == actor && obligation.action == ScheduledAction::Play
        })
    }

    /// Cards that cannot act before the observer's currently due connection
    /// steps resolve. This is the canonical blocking query for inference;
    /// consumers must not rebuild connection suffix semantics themselves.
    pub(super) const fn blocked_cards(&self) -> &CardSet {
        &self.blocked_cards
    }

    pub(super) fn required_discards_for(
        &self,
        actor: PlayerId,
    ) -> impl Iterator<Item = CardId> + '_ {
        self.obligations.iter().filter_map(move |obligation| {
            (obligation.actor == actor && obligation.action == ScheduledAction::Discard)
                .then_some(obligation.card)
        })
    }

    /// Projects only forced, unambiguous plays before `observer` next acts.
    pub(super) fn stack_heights_before(&self, view: &PlayerView, observer: PlayerId) -> [u8; 5] {
        let mut heights = StackTimeline::current(view).heights();
        let mut player = view.current_player;
        while player != observer {
            let playable = self
                .plays_for(player)
                .filter_map(|obligation| {
                    obligation.promised_identity.or_else(|| {
                        (obligation.identities.len() == 1)
                            .then(|| obligation.identities.iter().next())
                            .flatten()
                            .or_else(|| identity_of(view, obligation.card))
                    })
                })
                .filter(|identity| is_playable_at(heights, *identity))
                .collect::<Vec<_>>();
            if let [identity] = playable.as_slice() {
                heights[identity.suit.index()] += 1;
            }
            player = next_player(player, view.hands.len());
        }
        heights
    }
}

fn owner_of(replay: &HGroupState, card: CardId) -> Option<PlayerId> {
    replay
        .hands
        .iter()
        .position(|hand| hand.contains(&card))
        .map(|owner| {
            PlayerId::new(u8::try_from(owner).expect("standard Hanabi has at most five players"))
        })
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum StackHorizon {
    AtClue(u32),
    Current(u32),
    BeforePlayerTurn(PlayerId),
}

/// Stack heights paired with the time at which they are valid.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct StackTimeline {
    heights: [u8; 5],
    horizon: StackHorizon,
}

impl StackTimeline {
    pub(super) const fn at_clue(turn: u32, heights: [u8; 5]) -> Self {
        Self {
            heights,
            horizon: StackHorizon::AtClue(turn),
        }
    }

    pub(super) fn current(view: &PlayerView) -> Self {
        Self {
            heights: std::array::from_fn(|index| {
                u8::try_from(view.play_stacks[index].len())
                    .expect("a standard stack has at most five cards")
            }),
            horizon: StackHorizon::Current(view.turn),
        }
    }

    pub(super) fn before_player_turn(
        view: &PlayerView,
        replay: &HGroupState,
        player: PlayerId,
    ) -> Self {
        let schedule = ActionSchedule::from_replay(view, replay);
        Self {
            heights: schedule.stack_heights_before(view, player),
            horizon: StackHorizon::BeforePlayerTurn(player),
        }
    }

    pub(super) const fn heights(self) -> [u8; 5] {
        self.heights
    }
}
