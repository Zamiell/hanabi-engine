use hanabi_core::CardId;

use super::{
    BeliefConstraints, HGroupInferences, HGroupMoveKind, HGroupState, IdentityClaimRelation,
    IdentitySet, LogicalDeductions, identity_of,
};

/// Canonical symbolic convention constraints passed to exact world building.
///
/// Per-card domains are common constraints. Relational claims and ordered
/// connection alternatives are mutually exclusive branches; they are kept
/// symbolic here rather than flattened into unrelated marginal domains.
pub(super) struct ConventionConstraintGraph {
    common: Vec<(CardId, IdentitySet)>,
    factors: Vec<Vec<Vec<(CardId, IdentitySet)>>>,
}

impl ConventionConstraintGraph {
    pub(super) fn from_replay(
        deductions: &LogicalDeductions,
        replay: &HGroupState,
        inferred: &HGroupInferences,
    ) -> Self {
        let mut common = inferred
            .cards
            .iter()
            .map(|card| (card.card, card.identities))
            .collect::<Vec<_>>();
        let own_cards = deductions.view().hands[deductions.view().observer.index()]
            .iter()
            .map(|card| card.id)
            .collect::<Vec<_>>();
        let mut factors = Vec::new();

        for claim in replay.cards.facts.identity_claims().iter().filter(|claim| {
            claim.relation == IdentityClaimRelation::OneOf && !is_connection_claim(claim.source)
        }) {
            let claimed = IdentitySet::singleton(claim.identity);
            let claim_was_demonstrated = claim
                .cards
                .iter()
                .copied()
                .any(|card| identity_of(deductions.view(), card) == Some(claim.identity));
            if claim_was_demonstrated {
                for card in claim
                    .cards
                    .iter()
                    .copied()
                    .filter(|card| own_cards.contains(card))
                {
                    common.push((card, IdentitySet::all().without(claimed)));
                }
                continue;
            }
            let candidates = claim
                .cards
                .iter()
                .copied()
                .filter(|card| own_cards.contains(card))
                .filter(|card| identity_of(deductions.view(), *card).is_none())
                .collect::<Vec<_>>();
            if candidates.is_empty() {
                continue;
            }
            let claim_alternatives = candidates
                .iter()
                .map(|selected| {
                    candidates
                        .iter()
                        .map(|card| {
                            (
                                *card,
                                if card == selected {
                                    claimed
                                } else {
                                    IdentitySet::all().without(claimed)
                                },
                            )
                        })
                        .collect::<Vec<_>>()
                })
                .collect::<Vec<_>>();
            factors.push(claim_alternatives);
        }

        let view = deductions.view();
        let immediately_playable = IdentitySet::from_mask(
            IdentitySet::all()
                .iter()
                .filter(|identity| {
                    identity.rank.number()
                        == u8::try_from(view.play_stacks[identity.suit.index()].len())
                            .expect("a standard stack has at most five cards")
                            + 1
                })
                .fold(0, |mask, identity| mask | (1 << identity.index())),
        );
        for promise in &inferred.connection_promises {
            let expected = IdentitySet::singleton(promise.identity);
            let wrong_success = immediately_playable.without(expected);
            let promise_alternatives = promise
                .cards
                .iter()
                .enumerate()
                .map(|(correct, card)| {
                    promise.cards[..correct]
                        .iter()
                        .copied()
                        .map(|prior| (prior, wrong_success))
                        .chain(core::iter::once((*card, expected)))
                        .collect::<Vec<_>>()
                })
                .collect::<Vec<_>>();
            factors.push(promise_alternatives);
        }

        Self { common, factors }
    }

    pub(super) fn into_belief_constraints(self) -> BeliefConstraints {
        BeliefConstraints {
            constraints: self.common,
            factors: self.factors,
        }
    }
}

fn is_connection_claim(kind: HGroupMoveKind) -> bool {
    matches!(
        kind,
        HGroupMoveKind::Prompt
            | HGroupMoveKind::Finesse
            | HGroupMoveKind::ReverseFinesse
            | HGroupMoveKind::SelfFinesse
            | HGroupMoveKind::LayeredFinesse
            | HGroupMoveKind::HiddenFinesse
            | HGroupMoveKind::ClandestineFinesse
            | HGroupMoveKind::QueuedFinesse
            | HGroupMoveKind::AmbiguousFinesse
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn constraint_graph_retains_factors_without_expanding_them() {
        let first = vec![vec![(CardId::new(1), IdentitySet::all())]];
        let second = vec![
            vec![(CardId::new(2), IdentitySet::all())],
            vec![(CardId::new(3), IdentitySet::all())],
        ];
        let graph = ConventionConstraintGraph {
            common: Vec::new(),
            factors: vec![first.clone(), second.clone()],
        };
        assert_eq!(graph.into_belief_constraints().factors, vec![first, second]);
    }
}
