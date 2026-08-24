use hanabi_core::{Card, CardId, ObservedEvent, PlayerView};

/// Identity visible in the source view at its current turn. Historical rule
/// interpretation must use `HistoricalView` instead.
pub(super) fn identity_of(view: &PlayerView, card: CardId) -> Option<Card> {
    view.hands
        .iter()
        .flatten()
        .find(|candidate| candidate.id == card)
        .and_then(|candidate| candidate.identity)
        .or_else(|| {
            view.play_stacks
                .iter()
                .flatten()
                .chain(view.discard_pile.iter())
                .find_map(|(candidate, identity)| (*candidate == card).then_some(*identity))
        })
        .or_else(|| {
            view.history.iter().find_map(|entry| match entry.event {
                ObservedEvent::Played {
                    card: candidate,
                    identity,
                    ..
                }
                | ObservedEvent::Discarded {
                    card: candidate,
                    identity,
                    ..
                } if candidate == card => Some(identity),
                ObservedEvent::Drew {
                    card: candidate,
                    identity: Some(identity),
                    ..
                } if candidate == card => Some(identity),
                _ => None,
            })
        })
}
