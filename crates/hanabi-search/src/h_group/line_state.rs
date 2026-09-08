//! Pure observer-relative read model shared by line compilation and scoring.
use super::{
    Card, CardId, CompiledObserverProjection, EpistemicState, HGroupConnection, IdentitySet,
    PlayerId, PlayerView, Rank, identity_of, is_eventually_useful, is_playable_now,
};

pub(super) fn card_owner(source: &PlayerView, card: CardId) -> Option<PlayerId> {
    source
        .hands
        .iter()
        .position(|hand| hand.iter().any(|candidate| candidate.id == card))
        .and_then(|index| u8::try_from(index).ok())
        .map(PlayerId::new)
}

#[derive(Clone)]
pub(super) struct ProjectedLineState {
    pub(super) giver_visible_commitments: Vec<(CardId, Card)>,
    pub(super) giver_visible_promises: Vec<(CardId, Card)>,
    pub(super) epistemic: EpistemicState,
    pub(super) owner_promises: Vec<(CardId, IdentitySet)>,
    pub(super) owner_clued_superpositions: Vec<(CardId, IdentitySet)>,
    pub(super) connection: Option<HGroupConnection>,
    pub(super) connection_lines: Vec<(PlayerId, CardId, Card, Vec<CardId>)>,
    pub(super) playable_now: Vec<CardId>,
    pub(super) chop: Option<CardId>,
    pub(super) chop_moved: super::CardSet,
    pub(super) causal_cards: super::CardSet,
}

impl ProjectedLineState {
    /// Team coverage is evaluated by the clue giver, who may legally use the
    /// visible identities in teammates' hands. This projection is kept
    /// separate from owner knowledge so it can never establish Clarity equivalence.
    pub(super) fn closed_public_commitments(&self, source: &PlayerView) -> Vec<(CardId, Card)> {
        let mut closed = self.giver_visible_commitments.clone();
        loop {
            let mut changed = false;
            for (card, identity) in &self.giver_visible_promises {
                if closed.iter().any(|(known, _)| known == card) {
                    continue;
                }
                let stack_height = source.play_stacks[identity.suit.index()].len();
                let lower_promises_are_secured = Rank::ALL.iter().copied().all(|rank| {
                    let number = usize::from(rank.number());
                    number <= stack_height
                        || number >= usize::from(identity.rank.number())
                        || closed.iter().any(|(_, secured)| {
                            secured.suit == identity.suit && secured.rank == rank
                        })
                });
                if lower_promises_are_secured {
                    closed.push((*card, *identity));
                    changed = true;
                }
            }
            if !changed {
                break;
            }
        }
        closed.sort_unstable_by_key(|(card, identity)| (card.index(), identity.index()));
        closed.dedup();
        closed
    }

    /// Owner-relative counterpart used only to decide whether two clues have
    /// identical outcomes for the Clarity Principle. Team coverage retains
    /// the established public projection above; equivalence is stricter and
    /// must not use identities visible only to another player.
    pub(super) fn closed_owner_commitments(&self, source: &PlayerView) -> Vec<(CardId, Card)> {
        let mut closed = self
            .epistemic
            .own_beliefs()
            .filter_map(|belief| {
                belief
                    .known_identity()
                    .filter(|identity| is_eventually_useful(source, *identity))
                    .map(|identity| (belief.card, identity))
            })
            .collect::<Vec<_>>();
        loop {
            let mut changed = false;
            for (card, identities) in &self.owner_promises {
                if closed.iter().any(|(known, _)| known == card) {
                    continue;
                }
                // Good Touch excludes identities already committed to other
                // useful cards, but it does not reveal which of several
                // remaining future identities this card is. In particular, a
                // purple card that could be purple 4 or purple 5 does not
                // become a promised purple 4 merely because purple 2 and 3
                // are scheduled to play.
                let claimed = closed
                    .iter()
                    .fold(IdentitySet::default(), |set, (_, identity)| {
                        set.union(IdentitySet::singleton(*identity))
                    });
                let remaining = identities.without(claimed);
                let Some(identity) = (remaining.len() == 1)
                    .then(|| remaining.iter().next())
                    .flatten()
                else {
                    continue;
                };
                let stack_height = source.play_stacks[identity.suit.index()].len();
                let lower_promises_are_secured = Rank::ALL.iter().copied().all(|rank| {
                    let number = usize::from(rank.number());
                    number <= stack_height
                        || number >= usize::from(identity.rank.number())
                        || closed.iter().any(|(_, secured)| {
                            secured.suit == identity.suit && secured.rank == rank
                        })
                });
                if lower_promises_are_secured {
                    closed.push((*card, identity));
                    changed = true;
                }
            }
            if !changed {
                break;
            }
        }
        closed.sort_unstable_by_key(|(card, identity)| (card.index(), identity.index()));
        closed.dedup();
        closed
    }
}

#[allow(clippy::too_many_lines)]
pub(super) fn projected_line_state(
    source: &PlayerView,
    projection: &CompiledObserverProjection,
) -> ProjectedLineState {
    let observer = projection.deductions.view().observer;
    let replay = &projection.replay;
    let connection_lines = replay
        .pending_connections
        .iter()
        .map(|connection| {
            (
                connection.actor,
                connection.focus,
                connection.expected,
                connection.cards.clone(),
            )
        })
        .collect();
    let promised = replay
        .cards
        .explicitly_clued
        .union(&replay.cards.invisibly_clued)
        .copied()
        .collect::<Vec<_>>();
    let chop_moved = replay.cards.chop_moved.materialized().clone();
    let causal_cards = replay
        .transitions
        .iter()
        .rev()
        .find(|transition| Some(transition.turn) == source.history.last().map(|entry| entry.turn))
        .into_iter()
        .flat_map(|transition| transition.delta.added_cards())
        .collect();
    let inferred = &projection.inferred;
    let chop = inferred.chops[observer.index()];
    let playable_now = inferred.playable_now.clone();
    let epistemic = EpistemicState::from_analysis(&projection.deductions, inferred);
    let mut giver_visible_commitments = inferred
        .cards
        .iter()
        .filter_map(|note| {
            note.identities
                .iter()
                .next()
                .filter(|_| note.identities.len() == 1)
                .filter(|identity| {
                    identity_of(source, note.card).is_none_or(|actual| actual == *identity)
                })
                .filter(|identity| is_eventually_useful(source, *identity))
                .map(|identity| (note.card, identity))
        })
        .collect::<Vec<_>>();
    giver_visible_commitments.extend(inferred.playable_now.iter().filter_map(|card| {
        identity_of(source, *card)
            .filter(|identity| is_playable_now(source, *identity))
            .map(|identity| (*card, identity))
    }));
    giver_visible_commitments
        .sort_unstable_by_key(|(card, identity)| (card.index(), identity.index()));
    giver_visible_commitments.dedup();
    let mut giver_visible_promises = promised
        .iter()
        .copied()
        .filter_map(|card| {
            identity_of(source, card)
                .or_else(|| {
                    inferred
                        .cards
                        .iter()
                        .find(|note| note.card == card && note.identities.len() == 1)
                        .and_then(|note| note.identities.iter().next())
                })
                .filter(|identity| is_eventually_useful(source, *identity))
                // Invisible alternative slots promise the connection's
                // identity, not every visible face in the candidate list.
                // Otherwise a red-1 layer containing a red 4 manufactures
                // an unrelated red-4 play once the lower stack is secured.
                .filter(|identity| {
                    replay.cards.explicitly_clued.contains(&card)
                        || !replay.pending_connections.iter().any(|connection| {
                            replay.pending_connections.is_active(connection)
                                && connection.cards.contains(&card)
                        })
                        || replay.pending_connections.iter().any(|connection| {
                            replay.pending_connections.is_active(connection)
                                && connection.cards.contains(&card)
                                && connection.expected == *identity
                        })
                })
                .map(|identity| (card, identity))
        })
        .collect::<Vec<_>>();
    giver_visible_promises
        .sort_unstable_by_key(|(card, identity)| (card.index(), identity.index()));
    giver_visible_promises.dedup();
    let owner_clued_superpositions =
        collect_owner_clued_superpositions(source, observer, &epistemic, &promised);
    let mut owner_promises = promised
        .into_iter()
        .filter_map(|card| {
            if card_owner(source, card) != Some(observer) {
                return None;
            }
            epistemic
                .belief(card)
                .map(|belief| belief.identities)
                .map(|identities| {
                    IdentitySet::from_mask(
                        identities
                            .iter()
                            .filter(|identity| is_eventually_useful(source, *identity))
                            .fold(0, |mask, identity| mask | (1 << identity.index())),
                    )
                })
                .filter(|identities| !identities.is_empty())
                .map(|identities| (card, identities))
        })
        .collect::<Vec<_>>();
    owner_promises.sort_unstable_by_key(|(card, _)| card.index());
    owner_promises.dedup();
    ProjectedLineState {
        giver_visible_commitments,
        giver_visible_promises,
        epistemic,
        owner_promises,
        owner_clued_superpositions,
        connection: inferred.connection,
        connection_lines,
        playable_now,
        chop,
        chop_moved,
        causal_cards,
    }
}

fn collect_owner_clued_superpositions(
    source: &PlayerView,
    observer: PlayerId,
    epistemic: &EpistemicState,
    clued_cards: &[CardId],
) -> Vec<(CardId, IdentitySet)> {
    let mut superpositions = clued_cards
        .iter()
        .copied()
        .filter(|card| card_owner(source, *card) == Some(observer))
        .filter_map(|card| {
            epistemic
                .belief(card)
                .map(|belief| (card, belief.identities))
        })
        .collect::<Vec<_>>();
    superpositions.sort_unstable_by_key(|(card, _)| card.index());
    superpositions.dedup();
    superpositions
}
