use super::line_state::projected_line_state;
use super::{
    Card, CardId, CompiledProspectiveClue, HGroupMoveKind, PlayerId, PlayerView, identity_of,
    is_eventually_useful,
};

/// Interpretation evidence belonging to one clue and source observer. Scoring
/// consumes this result; it cannot select another observer's journal later.
#[derive(Clone)]
pub(super) struct CompiledLineEvidence {
    pub(super) observer: PlayerId,
    pub(super) named: Option<NamedLineEvidence>,
    pub(super) ignition_cards: Vec<CardId>,
    pub(super) charm_focus: Option<CardId>,
    pub(super) conflicting_observers: Vec<PlayerId>,
    pub(super) positional_anchor: Option<CardId>,
}

#[derive(Clone)]
pub(super) struct NamedLineEvidence {
    pub(super) secured_actions: usize,
    pub(super) connection_steps: usize,
    pub(super) playable_cards: Option<Vec<CardId>>,
}

pub(super) fn compile(
    source: &PlayerView,
    team: &CompiledProspectiveClue,
    kind: Option<HGroupMoveKind>,
) -> Option<CompiledLineEvidence> {
    let named = compile_named_line_metrics(source, team, kind).map(
        |(secured_actions, connection_steps, playable_cards)| NamedLineEvidence {
            secured_actions,
            connection_steps,
            playable_cards,
        },
    );
    let giver = team.projection(source.observer)?;
    let ignition_cards = giver
        .replay
        .signals
        .iter()
        .filter(|signal| {
            signal.turn == source.turn
                && matches!(
                    signal.kind,
                    HGroupMoveKind::ReplayDoubleIgnition
                        | HGroupMoveKind::UnnecessaryIgnition
                        | HGroupMoveKind::UnnecessaryMove
                        | HGroupMoveKind::TrashDoubleIgnition
                        | HGroupMoveKind::PokeDoubleIgnition
                        | HGroupMoveKind::BombDoubleIgnition
                        | HGroupMoveKind::BombTripleIgnition
                )
        })
        .flat_map(|signal| signal.cards.iter().copied())
        .collect();
    let charm_focus = giver
        .replay
        .signals
        .has_at_turn(source.turn, HGroupMoveKind::Charm)
        .then(|| {
            giver
                .replay
                .clues
                .iter()
                .rev()
                .find(|clue| clue.turn == source.turn)
                .map(|clue| clue.focus)
        })
        .flatten();
    let mut conflicting_observers = Vec::new();
    if charm_focus.is_none() {
        for player in 0..source.hands.len() {
            let observer = PlayerId::new(u8::try_from(player).ok()?);
            if team
                .projection(observer)?
                .replay
                .signals
                .has_at_turn(source.turn, HGroupMoveKind::Charm)
            {
                conflicting_observers.push(observer);
            }
        }
    }
    let reactor = super::next_player(source.current_player, source.hands.len());
    let positional_anchor = giver.replay.signals.iter().find_map(|signal| {
        (signal.turn == source.turn
            && signal.target == Some(reactor)
            && matches!(
                signal.kind,
                HGroupMoveKind::Bluff
                    | HGroupMoveKind::FiveColorEjection
                    | HGroupMoveKind::StackedEjection
            )
            && signal.cards.len() >= 2)
            .then(|| signal.cards[0])
    });
    Some(CompiledLineEvidence {
        observer: source.observer,
        named,
        ignition_cards,
        charm_focus,
        conflicting_observers,
        positional_anchor,
    })
}

/// Returns the action count and blind-play depth of the canonical named line.
///
/// Different observers can retain provisional alternatives for the same
/// clue. In Bluff Seat, a recognized Bluff takes precedence over an apparent
/// Layered Finesse. A Clandestine Finesse, meanwhile, includes every layered
/// blind play plus the clued focus. Keeping this precedence here prevents the
/// outcome comparison from adding mutually exclusive observer projections.
#[allow(clippy::too_many_lines)]
pub(super) fn compile_named_line_metrics(
    source: &PlayerView,
    team: &CompiledProspectiveClue,
    canonical_kind: Option<HGroupMoveKind>,
) -> Option<(usize, usize, Option<Vec<CardId>>)> {
    let mut bluff = None;
    let mut clandestine = None;
    let mut bluff_cards = Vec::new();
    let mut clandestine_cards = Vec::new();
    let mut layered = None;
    let mut ejection = None;
    let mut ignition = None;
    for player in 0..source.hands.len() {
        let observer =
            PlayerId::new(u8::try_from(player).expect("standard Hanabi has at most five players"));
        let projection = team.projection(observer)?;
        for signal in projection
            .replay
            .signals
            .iter()
            .filter(|signal| signal.turn == source.turn)
        {
            match signal.kind {
                HGroupMoveKind::Bluff => {
                    if canonical_kind != Some(HGroupMoveKind::Bluff) {
                        continue;
                    }
                    let blind_plays = signal.cards.len().saturating_sub(1);
                    // Efficiency includes the protected focus even for a 3
                    // Bluff. It is not a claim that the 3 can play yet.
                    let mut secured_cards = signal.cards.clone();
                    if let Some(clue) = projection
                        .replay
                        .clues
                        .iter()
                        .find(|clue| clue.turn == source.turn)
                    {
                        secured_cards.extend(clue.new_non_focus.iter().copied().filter(|card| {
                            identity_of(source, *card)
                                .is_some_and(|identity| is_eventually_useful(source, identity))
                        }));
                    }
                    secured_cards.sort_unstable();
                    secured_cards.dedup();
                    bluff = Some((secured_cards.len(), blind_plays));
                    bluff_cards.clone_from(&signal.cards);
                    if bluff_cards.last().is_some_and(|card| {
                        identity_of(source, *card).is_none_or(|identity| {
                            view_distance_from_playable(source, identity) > 1
                        })
                    }) {
                        bluff_cards.pop();
                    }
                }
                HGroupMoveKind::ClandestineFinesse => {
                    // An alternative observer's Clandestine reading cannot
                    // replace the admitted ordinary line's connector cards.
                    // Ordinary candidate classification can include a
                    // Clandestine line, but it must be the giver's reading.
                    if observer != source.observer {
                        continue;
                    }
                    clandestine = Some((signal.cards.len() + 1, signal.cards.len()));
                    clandestine_cards.clone_from(&signal.cards);
                    if let Some(clue) = projection
                        .replay
                        .clues
                        .iter()
                        .find(|clue| clue.turn == source.turn)
                    {
                        clandestine_cards.push(clue.focus);
                    }
                }
                HGroupMoveKind::LayeredFinesse
                | HGroupMoveKind::HiddenFinesse
                | HGroupMoveKind::QueuedFinesse
                | HGroupMoveKind::AmbiguousFinesse => {
                    layered = Some((signal.cards.len() + 1, signal.cards.len()));
                }
                HGroupMoveKind::FiveColorEjection
                | HGroupMoveKind::Ejection
                | HGroupMoveKind::OutOfPositionEjection
                | HGroupMoveKind::StackedEjection => {
                    // Focus-only identity annotations are not action signals.
                    // The full signal contains one ejected card followed by
                    // touched cards; those touches are not additional plays.
                    // https://hanabi.github.io/level-16/#ejections
                    if signal.cards.len() < 2 {
                        continue;
                    }
                    let commitments =
                        projected_line_state(source, team.projection(source.observer)?.as_ref())
                            .closed_public_commitments(source);
                    let focus_is_secured = projection
                        .replay
                        .clues
                        .iter()
                        .find(|clue| clue.turn == source.turn)
                        .and_then(|clue| identity_of(source, clue.focus))
                        .is_some_and(|focus| {
                            is_eventually_useful(source, focus)
                                && (1..focus.rank.number()).all(|rank| {
                                    usize::from(rank)
                                        <= source.play_stacks[focus.suit.index()].len()
                                        || commitments.iter().any(|(_, promised)| {
                                            promised.suit == focus.suit
                                                && promised.rank.number() == rank
                                        })
                                })
                        });
                    ejection = Some((1 + usize::from(focus_is_secured), 1));
                }
                HGroupMoveKind::Charm => {
                    if observer != source.observer {
                        // The blind player necessarily treats their hidden
                        // Fourth Finesse Position as possibly playable. Only
                        // the clue giver can verify that the Charm is safe;
                        // another observer's provisional reading must not
                        // inflate the deterministic team line.
                        continue;
                    }
                    // The signal contains the immediate Fourth-Finesse-
                    // Position blind play and the long-term focused 4. Only
                    // the former is a deterministic continuation now; the 4
                    // still depends on its ordinary intervening stack cards.
                    // Source: https://hanabi.github.io/level-23/#the-4-charm
                    ejection = Some((1, 1));
                }
                HGroupMoveKind::UnnecessaryIgnition => {
                    let pushed = projection
                        .replay
                        .signals
                        .iter()
                        .filter(|other| {
                            other.turn == source.turn
                                && other.kind == HGroupMoveKind::UnnecessaryMove
                        })
                        .map(|other| other.cards.len())
                        .sum::<usize>();
                    ignition = Some((signal.cards.len() + pushed, signal.cards.len() + pushed));
                }
                HGroupMoveKind::ReplayDoubleIgnition
                | HGroupMoveKind::TrashDoubleIgnition
                | HGroupMoveKind::PokeDoubleIgnition
                | HGroupMoveKind::BombDoubleIgnition
                | HGroupMoveKind::BombTripleIgnition => {
                    // Every card named by an Ignition signal is an immediate
                    // blind-play obligation. These remain real line actions
                    // even when the clue giver sees that the physical cards
                    // happen to be playable.
                    ignition = Some((signal.cards.len(), signal.cards.len()));
                }
                _ => {}
            }
        }
    }
    ignition
        .or(ejection)
        .map(|(count, depth)| (count, depth, None))
        .or_else(|| bluff.map(|(count, depth)| (count, depth, Some(bluff_cards))))
        .or_else(|| clandestine.map(|(count, depth)| (count, depth, Some(clandestine_cards))))
        .or_else(|| layered.map(|(count, depth)| (count, depth, None)))
}

fn view_distance_from_playable(source: &PlayerView, identity: Card) -> usize {
    usize::from(identity.rank.number())
        .saturating_sub(source.play_stacks[identity.suit.index()].len() + 1)
}
