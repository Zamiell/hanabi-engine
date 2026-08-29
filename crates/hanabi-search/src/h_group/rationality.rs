use hanabi_core::{Action, CardId, Clue, ClueFacts, PlayerId, PlayerView, Rank};

use super::{
    CardSet, ConnectionManager, ConventionFacts, DeclinedAlternativeInference,
    HGroupClueInterpretation, HGroupClueKind, HGroupProfile, HGroupRuleId, HistoricalView,
    IdentitySet, chop, focus, is_playable_at, rule_enabled, snapshot_play_identities,
};

/// Immutable inputs for inverse planning at one observed clue.
pub(super) struct DeclinedAlternativeContext<'a> {
    pub(super) view: &'a PlayerView,
    pub(super) profile: HGroupProfile,
    pub(super) clue: &'a HGroupClueInterpretation,
    pub(super) hands: &'a [Vec<CardId>],
    pub(super) clue_facts: &'a [ClueFacts],
    pub(super) historical: HistoricalView<'a>,
    pub(super) gotten: &'a CardSet,
    pub(super) promptable_before: &'a CardSet,
    pub(super) already_playing: &'a CardSet,
    pub(super) pending: &'a ConnectionManager,
    pub(super) convention_facts: &'a ConventionFacts,
    pub(super) chop_moved: &'a CardSet,
}

/// Infers identities from a clue giver declining a strictly more efficient
/// clue that would otherwise satisfy Good Touch.
///
/// This is intentionally structural rather than a general assumption that
/// every human always selects the engine's top-scored move. It applies only
/// when:
///
/// - the observed clue is a direct one-for-one Play Clue;
/// - a single-touch rank clue would establish a strictly longer deterministic
///   play chain;
/// - exactly one previously touched card in the observer's hand could
///   duplicate that alternative focus; and
/// - the alternative line is convention-valid when that duplication is
///   absent.
///
/// Under those conditions, Good Touch explains why the objectively stronger
/// clue was unavailable: the ambiguous touched card must duplicate its focus.
/// This is the counterfactual used in `game-p4v0s2` after Cathy's direct
/// yellow-2 clue.
///
/// Sources:
/// - <https://hanabi.github.io/level-1/#good-touch-principle>
/// - <https://hanabi.github.io/level-3/#efficiency>
#[allow(clippy::too_many_lines)]
pub(super) fn declined_superior_clue_inferences(
    context: &DeclinedAlternativeContext<'_>,
) -> Vec<DeclinedAlternativeInference> {
    let clue = context.clue;
    if !rule_enabled(context.profile, HGroupRuleId::BasicStrategy)
        || clue.target != context.view.observer
        || clue.kind != HGroupClueKind::Play
        || clue.touched.len() != 1
        || clue.play_identities.len() != 1
    {
        return Vec::new();
    }
    let Some(chosen_identity) = clue.play_identities.iter().next() else {
        return Vec::new();
    };
    let chosen_height = clue.stack_heights[chosen_identity.suit.index()];
    if !is_playable_at(clue.stack_heights, chosen_identity) {
        return Vec::new();
    }
    let chosen_action = Action::Clue {
        target: clue.target,
        clue: clue.clue,
    };
    let observer_hand = &context.hands[context.view.observer.index()];
    let mut proposed = Vec::<(u8, DeclinedAlternativeInference)>::new();

    for (target_index, hand) in context.hands.iter().enumerate() {
        let target = PlayerId::new(
            u8::try_from(target_index).expect("standard Hanabi has at most five players"),
        );
        if target == clue.giver || target == clue.target {
            continue;
        }
        let old_chop = chop(hand, context.gotten);
        for rank in Rank::ALL {
            let alternative_clue = Clue::Rank(rank);
            let touched = hand
                .iter()
                .copied()
                .filter(|card| {
                    context
                        .historical
                        .identity(*card)
                        .is_some_and(|identity| alternative_clue.matches(identity))
                })
                .collect::<Vec<_>>();
            if touched.len() != 1 {
                continue;
            }
            let Some(alternative_focus) = focus(hand, &touched, old_chop, context.gotten) else {
                continue;
            };
            if context.gotten.contains(&alternative_focus) {
                continue;
            }
            let Some(alternative_identity) = context.historical.identity(alternative_focus) else {
                continue;
            };
            let alternative_height = clue.stack_heights[alternative_identity.suit.index()];
            let action_count = alternative_identity
                .rank
                .number()
                .saturating_sub(alternative_height);
            let chosen_action_count = chosen_identity.rank.number().saturating_sub(chosen_height);
            if action_count <= chosen_action_count {
                continue;
            }

            let clue_gotten = context
                .gotten
                .iter()
                .copied()
                .chain(touched.iter().copied())
                .collect::<CardSet>();
            let playable = snapshot_play_identities(
                context.profile,
                IdentitySet::singleton(alternative_identity),
                clue.giver,
                target,
                alternative_focus,
                context.view,
                context.hands,
                context.clue_facts,
                &clue_gotten,
                context.already_playing,
                context.pending,
                context.convention_facts,
                context.chop_moved,
                clue.stack_heights,
                clue.turn,
                true,
            );
            if !playable.contains(alternative_identity) {
                continue;
            }

            let duplicates = observer_hand
                .iter()
                .copied()
                .filter(|card| {
                    *card != clue.focus
                        && context.promptable_before.contains(card)
                        && context.clue_facts[card.index()].has_positive_clue(alternative_clue)
                        && context.clue_facts[card.index()].allows(alternative_identity)
                        && context.convention_facts.known_identity(*card).is_none()
                })
                .collect::<Vec<_>>();
            let [duplicate] = duplicates.as_slice() else {
                continue;
            };
            proposed.push((
                action_count,
                DeclinedAlternativeInference {
                    turn: clue.turn,
                    actor: clue.giver,
                    card: *duplicate,
                    identity: alternative_identity,
                    chosen: chosen_action,
                    superior: Action::Clue {
                        target,
                        clue: alternative_clue,
                    },
                },
            ));
        }
    }

    // A conflict means the absence of one clue cannot uniquely identify the
    // card. Otherwise keep only the strongest declined line for each card.
    proposed.sort_by_key(|(actions, inference)| {
        (
            core::cmp::Reverse(*actions),
            inference.card.index(),
            inference.identity.index(),
        )
    });
    let mut inferred = Vec::<DeclinedAlternativeInference>::new();
    let mut conflicts = CardSet::default();
    for (_, inference) in proposed {
        if conflicts.contains(&inference.card) {
            continue;
        }
        if let Some(existing) = inferred
            .iter()
            .find(|existing| existing.card == inference.card)
        {
            if existing.identity != inference.identity {
                inferred.retain(|existing| existing.card != inference.card);
                conflicts.insert(inference.card);
            }
            continue;
        }
        inferred.push(inference);
    }
    inferred
}
