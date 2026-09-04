use hanabi_core::{Card, CardId, Clue, PlayerView, Rank};

use super::{
    CardSet, HGroupCardInference, IdentitySet, RecipientCardConsequence, RecipientCardDisposition,
    identity_of, is_eventually_useful,
};

/// Immutable inputs for Good Touch admission. Keeping the complete recipient
/// consequence context together prevents callers from accidentally validating
/// only the giver-visible focus while ignoring other newly promised cards.
#[derive(Clone, Copy)]
pub(super) struct GoodTouchContext<'a> {
    pub(super) view: &'a PlayerView,
    pub(super) newly_touched: &'a [CardId],
    pub(super) clue: Option<(Clue, &'a [CardId])>,
    pub(super) explicitly_clued: &'a CardSet,
    pub(super) fixed_cards: &'a CardSet,
    pub(super) convention_cards: &'a [HGroupCardInference],
}

/// Compiles the behavioral consequences relevant to Good Touch and admits the
/// clue only when no recipient would acquire an impossible duplicate play
/// obligation.
pub(super) fn good_touch(context: GoodTouchContext<'_>) -> bool {
    let known_identity = |card: CardId| {
        identity_of(context.view, card).or_else(|| {
            context
                .convention_cards
                .iter()
                .find(|note| note.card == card && note.identities.len() == 1)
                .and_then(|note| note.identities.iter().next())
        })
    };
    let consequences = context
        .newly_touched
        .iter()
        .copied()
        .map(|card| {
            let identity = known_identity(card)?;
            is_eventually_useful(context.view, identity).then_some(RecipientCardConsequence {
                card,
                owner: context.view.current_player,
                identities: IdentitySet::singleton(identity),
                disposition: RecipientCardDisposition::PlayAfterConnection,
            })
        })
        .collect::<Option<Vec<_>>>();
    let Some(consequences) = consequences else {
        return false;
    };

    let mut newly_promised = IdentitySet::default();
    for consequence in consequences {
        let identity = consequence
            .identities
            .iter()
            .next()
            .expect("Good Touch consequences are exact");
        let accounted_existing_copy = context.clue.is_some_and(|(clue, touched)| {
            clue_accounts_for_every_copy(context.view, clue, touched, identity)
                && context
                    .newly_touched
                    .iter()
                    .filter(|candidate| known_identity(**candidate) == Some(identity))
                    .count()
                    == 1
        });
        if newly_promised.contains(identity) && !accounted_existing_copy {
            return false;
        }
        newly_promised = newly_promised.union(IdentitySet::singleton(identity));
        if !accounted_existing_copy
            && context.view.hands.iter().flatten().any(|candidate| {
                candidate.id != consequence.card
                    && context.explicitly_clued.contains(&candidate.id)
                    && !context.fixed_cards.contains(&candidate.id)
                    && (known_identity(candidate.id) == Some(identity)
                        || (identity.rank == Rank::One
                            && candidate.identity.is_none()
                            && context.convention_cards.iter().any(|note| {
                                note.card == candidate.id && note.identities.contains(identity)
                            })))
            })
        {
            return false;
        }
    }
    true
}

/// A duplicate touch does not violate Good Touch when this clue gets only one
/// new copy, establishes its rank, and also touches every physical copy of that
/// identity. Once the newly focused copy plays, card-count elimination makes
/// every previously gotten copy known trash instead of a future play promise.
/// Two newly touched copies remain a violation: the recipient could reasonably
/// treat both as future plays. Merely touching every copy with a suit clue is
/// likewise insufficient when their ranks remain unknown.
/// Source: <https://hanabi.github.io/level-1/#good-touch-principle>
pub(super) fn clue_accounts_for_every_copy(
    view: &PlayerView,
    clue: Clue,
    touched: &[CardId],
    identity: Card,
) -> bool {
    let matching = touched
        .iter()
        .copied()
        .filter(|card| identity_of(view, *card) == Some(identity))
        .collect::<Vec<_>>();
    matching.len() == usize::from(identity.rank.copies())
        && (clue == Clue::Rank(identity.rank)
            || matching.iter().all(|card| {
                view.hands
                    .iter()
                    .flatten()
                    .find(|candidate| candidate.id == *card)
                    .is_some_and(|candidate| {
                        candidate.clues.has_positive_clue(Clue::Rank(identity.rank))
                    })
            }))
}
