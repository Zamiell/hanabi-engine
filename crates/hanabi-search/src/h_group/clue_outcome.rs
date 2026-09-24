//! Canonical causal outcome compilation, independent of candidate preferences.
use super::line_state::{ProjectedLineState, card_owner, projected_line_state};
use super::{
    Action, ActionCommitment, Card, CardId, ClueProposal, CluedCardSuperposition, HGroupConnection,
    HGroupMoveKind, HGroupProfile, IdentitySet, LineOutcome, PlayerId, PlayerView,
    RecipientCardConsequence, RecipientCardDisposition, card_is_trash, compiled_baseline_team,
    compiled_prospective_clue, identity_of, is_eventually_useful, is_playable_now,
};

/// Compile a scheduling alternative with the same owner-relative outcome
/// calculation used for ordinary clue valuation.
pub(super) fn scheduled_clue_outcome(
    source: &PlayerView,
    profile: HGroupProfile,
    candidate: &ClueProposal,
) -> Option<LineOutcome> {
    let team = compiled_baseline_team(source, profile);
    let baselines = (0..source.hands.len())
        .map(|player| {
            let observer = PlayerId::new(u8::try_from(player).ok()?);
            Some(projected_line_state(
                source,
                team.projection(observer)?.as_ref(),
            ))
        })
        .collect::<Option<Vec<_>>>()?;
    clue_line_value(
        source,
        profile,
        candidate.action,
        &baselines,
        candidate.move_kind(),
    )
}

#[allow(clippy::too_many_lines)]
pub(super) fn clue_line_value(
    source: &PlayerView,
    profile: HGroupProfile,
    action: Action,
    baselines: &[ProjectedLineState],
    canonical_kind: Option<HGroupMoveKind>,
) -> Option<LineOutcome> {
    let Action::Clue { target, clue } = action else {
        return None;
    };
    let touched = source.hands[target.index()]
        .iter()
        .filter(|card| card.identity.is_some_and(|identity| clue.matches(identity)))
        .map(|card| card.id)
        .collect::<Vec<_>>();
    let compiled = compiled_prospective_clue(source, profile, target, clue, &touched)?;
    compiled.outcome(source, profile, baselines, canonical_kind)
}

#[allow(clippy::too_many_lines)]
pub(super) fn compile_uncached(
    source: &PlayerView,
    profile: HGroupProfile,
    compiled: &super::CompiledProspectiveClue,
    baselines: &[ProjectedLineState],
    canonical_kind: Option<HGroupMoveKind>,
) -> Option<LineOutcome> {
    let Action::Clue { target, clue } = compiled.action() else {
        return None;
    };
    let touched = source.hands[target.index()]
        .iter()
        .filter(|card| card.identity.is_some_and(|id| clue.matches(id)))
        .map(|card| card.id)
        .collect::<Vec<_>>();
    let after_clue = compiled.after();
    let response_effects = demonstrated_response(source, profile, compiled, target, &touched);
    let mut value = LineOutcome {
        demonstrated_trash: response_effects.trash,
        public_actions: response_effects.actions.clone(),
        ..LineOutcome::default()
    };
    let evidence = compiled.line_evidence(source, canonical_kind)?;
    debug_assert_eq!(evidence.observer, source.observer);
    let ignition_cards = &evidence.ignition_cards;
    let charm_focus = evidence.charm_focus;
    let mut giver_public_actions = response_effects.actions;
    let caused_by_clue = |card: CardId, identity: Card| {
        touched.contains(&card)
            || touched
                .iter()
                .chain(ignition_cards)
                .copied()
                .any(|touched_card| {
                    identity_of(source, touched_card).is_some_and(|touched_identity| {
                        touched_identity.suit == identity.suit
                            && touched_identity.rank.number() < identity.rank.number()
                    })
                })
    };
    let connects_to_clue_focus = |identity: Card| {
        touched.iter().copied().any(|touched_card| {
            identity_of(source, touched_card).is_some_and(|focus_identity| {
                focus_identity.suit == identity.suit
                    && identity.rank.number() < focus_identity.rank.number()
            })
        })
    };
    for (player, baseline) in baselines.iter().enumerate() {
        let observer =
            PlayerId::new(u8::try_from(player).expect("standard Hanabi has at most five players"));
        let projection = compiled.projection(observer)?;
        let conflicts_with_giver = evidence.conflicting_observers.contains(&observer);
        let after = projected_line_state(after_clue, &projection);
        for belief in after.epistemic.own_beliefs() {
            if let Some(old) = baseline.epistemic.belief(belief.card) {
                if old.identities != belief.identities {
                    value
                        .knowledge_changes
                        .push(super::outcome::KnowledgeChange {
                            owner: observer,
                            card: belief.card,
                            before: old.identities,
                            after: belief.identities,
                        });
                }
            }
        }
        value.newly_playable.extend(
            after
                .playable_now
                .iter()
                .filter(|card| !baseline.playable_now.contains(card))
                .map(|card| (observer, *card)),
        );
        if let Some(card) = baseline.chop {
            if after.owner_promises.iter().any(|(known, _)| *known == card)
                || after.chop_moved.contains(&card)
            {
                let distance = (observer.index() + source.hands.len()
                    - source.current_player.index())
                    % source.hands.len();
                value.protection.push(super::outcome::ProtectionEffect {
                    owner: observer,
                    card,
                    deadline: source.turn
                        + u32::try_from(if distance == 0 {
                            source.hands.len()
                        } else {
                            distance
                        })
                        .ok()?,
                });
            }
        }

        if baseline.chop != after.chop {
            let safe_discard = super::decision::convention_known_trash_discard(
                projection.deductions.view(),
                &projection.inferred,
            )
            .is_some();
            // Only protection-driven chop changes are an exchange. A clue
            // causing an ordinary play is valued by the ensuing line.
            if baseline
                .chop
                .is_some_and(|card| after.chop_moved.contains(&card))
                && !safe_discard
                && super::decision::fresh_trash_chop_move_focus(
                    projection.deductions.view(),
                    &projection.replay,
                )
                .is_none()
                && super::frontier_value::worsened_chop_exposure(
                    source,
                    after_clue,
                    profile,
                    baseline.chop,
                    after.chop,
                )
                .is_some()
            {
                value.worsened_chop_exposure = true;
            }
        }
        record_clued_superpositions(&mut value, observer, &after);
        let changed_connection_cards = after
            .connection_lines
            .iter()
            .flat_map(|(actor, focus, expected, cards)| {
                let prior = baseline.connection_lines.iter().find(
                    |(old_actor, old_focus, old_expected, _)| {
                        old_actor == actor && old_focus == focus && old_expected == expected
                    },
                );
                cards.iter().copied().filter(move |card| {
                    prior.is_none_or(|(_, _, _, old_cards)| !old_cards.contains(card))
                })
            })
            .collect::<super::CardSet>();
        let commitment_caused = |card: CardId, identity: Card| {
            caused_by_clue(card, identity)
                || after.causal_cards.contains(&card)
                || changed_connection_cards.contains(&card)
        };
        let baseline_public_commitments = baseline.closed_public_commitments(source);
        if observer == target {
            giver_public_actions.extend(
                after
                    .closed_public_commitments(source)
                    .iter()
                    .copied()
                    .filter(|commitment| !baseline_public_commitments.contains(commitment))
                    .filter(|(card, identity)| commitment_caused(*card, *identity))
                    .filter_map(|(card, identity)| {
                        card_owner(source, card)
                            .map(|owner| ActionCommitment::exact(card, owner, identity))
                    }),
            );
            giver_public_actions.extend(changed_connection_cards.iter().copied().filter_map(
                |card| {
                    identity_of(source, card)
                        .filter(|identity| is_eventually_useful(source, *identity))
                        .filter(|identity| {
                            !baseline_public_commitments.contains(&(card, *identity))
                        })
                        .and_then(|identity| {
                            card_owner(source, card)
                                .map(|owner| ActionCommitment::exact(card, owner, identity))
                        })
                },
            ));
            giver_public_actions.extend(
                after
                    .connection_lines
                    .iter()
                    .flat_map(|(_, _, _, cards)| cards.iter().copied())
                    .filter_map(|card| {
                        identity_of(source, card)
                            .filter(|identity| {
                                caused_by_clue(card, *identity)
                                    || connects_to_clue_focus(*identity)
                                    || changed_connection_cards.contains(&card)
                            })
                            .filter(|identity| {
                                !baseline_public_commitments.contains(&(card, *identity))
                            })
                            .and_then(|identity| {
                                card_owner(source, card)
                                    .map(|owner| ActionCommitment::exact(card, owner, identity))
                            })
                    }),
            );
        }
        if !conflicts_with_giver {
            value.public_actions.extend(
                after
                    .closed_public_commitments(source)
                    .iter()
                    .copied()
                    .filter(|commitment| !baseline_public_commitments.contains(commitment))
                    .filter(|(card, identity)| commitment_caused(*card, *identity))
                    .filter_map(|(card, identity)| {
                        card_owner(source, card)
                            .map(|owner| ActionCommitment::exact(card, owner, identity))
                    }),
            );
        }
        let baseline_owner_commitments = baseline.closed_owner_commitments(source);
        let new_actions = after
            .closed_owner_commitments(source)
            .iter()
            .copied()
            .filter(|commitment| !baseline_owner_commitments.contains(commitment))
            .filter(|(card, identity)| commitment_caused(*card, *identity))
            .filter_map(|(card, identity)| {
                card_owner(source, card).map(|owner| ActionCommitment::exact(card, owner, identity))
            })
            .collect::<Vec<_>>();
        value.owner_actions.extend(new_actions);
        value
            .protected_cards
            .extend(
                after
                    .owner_promises
                    .iter()
                    .filter_map(|(card, identities)| {
                        (!baseline.owner_promises.iter().any(|(old, _)| old == card)
                            // A connection can list alternative hidden slots.
                            // The giver must not count every visible face in
                            // that list as secured by the expected identity.
                            && (touched.contains(card)
                                || after.chop_moved.contains(card)
                                || !changed_connection_cards.contains(card)
                                || identity_of(source, *card).is_none_or(|actual| {
                                    after.connection_lines.iter().any(|(_, _, expected, cards)| {
                                        *expected == actual && cards.contains(card)
                                    })
                                }))
                            && identities
                                .iter()
                                .any(|identity| commitment_caused(*card, identity)))
                        .then_some(*card)
                    }),
            );
        // A demonstrated trash clue protects unlabelled cards without
        // making exact identity promises. Do not pool other observers'
        // provisional chop-move alternatives into the chosen meaning.
        if observer == target
            && !touched.is_empty()
            && touched.iter().all(|card| {
                projection.knows_trash(
                    *card,
                    &baseline
                        .giver_visible_promises
                        .iter()
                        .map(|(id, _)| *id)
                        .collect(),
                )
            })
        {
            value.protected_cards.extend(
                after
                    .chop_moved
                    .iter()
                    .copied()
                    .filter(|card| !baseline.chop_moved.contains(card)),
            );
        }
        value
            .known_trash
            .extend(after.epistemic.own_beliefs().filter_map(|belief| {
                if !touched.contains(&belief.card) {
                    return None;
                }
                belief
                    .known_identity()
                    .filter(|identity| card_is_trash(source, *identity))
                    .and_then(|_| {
                        baseline
                            .epistemic
                            .belief(belief.card)
                            .is_none_or(|prior| prior.known_identity().is_none())
                            .then_some(belief.card)
                    })
            }));
        if let Some(connection) = (!conflicts_with_giver)
            .then_some(after.connection)
            .flatten()
            .filter(|connection| {
                baseline
                    .connection
                    .is_none_or(|prior| prior.card != connection.card)
                    && !baseline_public_commitments.iter().any(|(card, identity)| {
                        *card == connection.card && *identity == connection.identity
                    })
            })
        {
            record_new_connection(&mut value, source, connection);
        }
    }
    giver_public_actions.extend(ignition_cards.iter().copied().filter_map(|card| {
        identity_of(source, card)
            .and_then(|identity| card_owner(source, card).map(|owner| (card, owner, identity)))
            .map(|(card, owner, identity)| ActionCommitment::exact(card, owner, identity))
    }));
    if let Some(focus) = charm_focus {
        // A Charm immediately schedules the Fourth-Finesse-Position card.
        // Its untouched 4 remains a valuable long-term promise, but is not a
        // deterministic continuation until the intervening 1, 2, and 3 are
        // secured. Keep it in owner knowledge/protection without inflating
        // immediate team-action coverage.
        giver_public_actions.retain(|commitment| commitment.card != focus);
        value
            .public_actions
            .retain(|commitment| commitment.card != focus);
    }
    // An observer's alternative Finesse reading is not an additional action
    // in the canonical Bluff/Clandestine line. Keep only that line's cards,
    // and never count two different identities on the same visible card.
    // https://hanabi.github.io/level-11/#mistaking-a-layered-finesse-for-a-bluff
    let canonical_cards = evidence
        .named
        .as_ref()
        .and_then(|line| line.playable_cards.as_ref());
    let consistent = |commitment: &ActionCommitment| {
        canonical_cards.is_none_or(|cards| {
            cards.contains(&commitment.card)
                && identity_of(source, commitment.card)
                    .is_none_or(|actual| commitment.identities.contains(actual))
        })
    };
    giver_public_actions.retain(consistent);
    value.public_actions.retain(consistent);
    value.owner_actions.retain(consistent);
    // A canonical blind-play line commits to playing these slots, not to
    // their provisional imagined identities. For example, p2 imagined on a
    // visibly green 1 still produces a green-1 play in a Double Bluff. Count
    // the compiled behavioral consequence instead of losing both actions
    // when the provisional Finesse promises are filtered above.
    if let Some(line) = &evidence.named {
        for card in &line.blind_play_cards {
            if let Some((identity, owner)) = identity_of(source, *card)
                .filter(|identity| is_eventually_useful(source, *identity))
                .zip(card_owner(source, *card))
            {
                let commitment = ActionCommitment::exact(*card, owner, identity);
                giver_public_actions.push(commitment);
                value.public_actions.push(commitment);
            }
        }
    }
    giver_public_actions
        .sort_unstable_by_key(|commitment| (commitment.card.index(), commitment.owner.index()));
    giver_public_actions.dedup();
    value
        .recipient_consequences
        .extend(value.public_actions.iter().map(|commitment| {
            RecipientCardConsequence {
                card: commitment.card,
                owner: commitment.owner,
                identities: commitment.identities,
                disposition: if !commitment.identities.is_empty()
                    && commitment
                        .identities
                        .iter()
                        .all(|identity| is_playable_now(source, identity))
                {
                    RecipientCardDisposition::PlayNow
                } else {
                    RecipientCardDisposition::PlayAfterConnection
                },
            }
        }));
    value
        .recipient_consequences
        .extend(value.known_trash.iter().filter_map(|card| {
            card_owner(source, *card).map(|owner| RecipientCardConsequence {
                card: *card,
                owner,
                identities: IdentitySet::default(),
                disposition: RecipientCardDisposition::KnownTrash,
            })
        }));
    value
        .recipient_consequences
        .extend(value.protected_cards.iter().filter_map(|card| {
            card_owner(source, *card).map(|owner| RecipientCardConsequence {
                card: *card,
                owner,
                identities: IdentitySet::default(),
                disposition: RecipientCardDisposition::Protected,
            })
        }));
    value.action_coverage = giver_public_actions.len();
    // Efficiency counts cards obtained by this clue, not already-clued
    // successors that become playable automatically. Those successors keep
    // their separate tempo/endpoint value. A red-2 Finesse through red 1 is
    // a 2-for-1 even when it also releases an already-clued red 3.
    let mut directly_secured = giver_public_actions
        .iter()
        .filter(|commitment| {
            // Retouching or clarifying an already-clued connector can
            // improve its timing, but does not obtain that card again.
            // Reviewed p4v0s1 turn 19: p3 and g5 are already secured.
            !super::was_clued_before(source, source.turn, commitment.card)
        })
        .map(|commitment| commitment.card)
        .collect::<Vec<_>>();
    // An indirect clue may be unknown to its target until the next player
    // responds. Count that owner's newly established safe response, without
    // pooling every observer's provisional alternative connection.
    let reactor = super::next_player(source.current_player, source.hands.len());
    if let Some(response) = compiled.projection(reactor) {
        if let Some(card) =
            super::preferred_due_play_card(response.deductions.view(), &response.inferred, profile)
                .filter(|card| value.newly_playable.contains(&(reactor, *card)))
                .filter(|card| {
                    identity_of(source, *card).is_some_and(|id| is_playable_now(source, id))
                })
        {
            directly_secured.push(card);
        }
    }
    let already_secured = |card: CardId| {
        super::was_clued_before(source, source.turn, card)
            || baselines.iter().any(|baseline| {
                // A layered connection promises its expected identity, not
                // every physical face in the alternative slots it examines.
                baseline
                    .giver_visible_promises
                    .iter()
                    .any(|(old, promised)| {
                        *old == card
                            && identity_of(source, card).is_none_or(|actual| actual == *promised)
                    })
                    || (baseline.playable_now.contains(&card)
                        && identity_of(source, card).is_some_and(|id| is_playable_now(source, id)))
            })
    };
    directly_secured.extend(touched.iter().copied().filter(|card| {
        !already_secured(*card)
            && identity_of(source, *card)
                .is_some_and(|identity| is_eventually_useful(source, identity))
    }));
    directly_secured.extend(value.protected_cards.iter().copied());
    directly_secured.sort_unstable();
    directly_secured.dedup();
    let previously_secured = source
        .hands
        .iter()
        .flatten()
        .filter(|card| already_secured(card.id))
        .map(|card| (card.id, card.identity))
        .collect::<Vec<_>>();
    value.unresolved_acquisitions = directly_secured
        .iter()
        .copied()
        .filter(|card| {
            identity_of(source, *card).is_none()
                && canonical_cards
                    .is_none_or(|cards| cards.contains(card) || touched.contains(card))
                && !previously_secured.iter().any(|(old, _)| old == card)
        })
        .collect();
    value.clue_efficiency =
        super::admission::minimum_clue_value(source, &previously_secured, directly_secured);
    // A fill-in may improve timing, but does not obtain a previously saved
    // card again. Keep that benefit in actions/tempo, never invent a 1-for-1.
    if let Some(line) = &evidence.named {
        let action_count = line.secured_actions;
        let connection_steps = line.connection_steps;
        value.convention_action_count = Some(if canonical_kind == Some(HGroupMoveKind::PlayClue) {
            // A normal Play line earns only its newly secured cards, not
            // older scheduled predecessors or every alternative blind slot.
            // Cap by the named line's size so unrelated downstream benefits
            // do not become extra steps in that convention line.
            // A compiled cross-suit line has explicit connector evidence;
            // do not reconstruct it with the ordinary same-suit fallback.
            let mut secured = value
                .public_actions
                .iter()
                .map(|action| action.card)
                .chain(value.protected_cards.iter().copied())
                .collect::<Vec<_>>();
            secured.sort_unstable();
            secured.dedup();
            secured.len().min(action_count)
        } else {
            action_count
        });
        value.convention_connection_steps = Some(connection_steps);
    }
    value.normalize();
    Some(value)
}

fn record_clued_superpositions(
    value: &mut LineOutcome,
    observer: PlayerId,
    state: &ProjectedLineState,
) {
    value
        .clued_superpositions
        .extend(
            state
                .owner_clued_superpositions
                .iter()
                .map(|(card, identities)| CluedCardSuperposition {
                    card: *card,
                    owner: observer,
                    identities: *identities,
                }),
        );
}

fn record_new_connection(
    value: &mut LineOutcome,
    source: &PlayerView,
    connection: HGroupConnection,
) {
    if let Some(owner) = card_owner(source, connection.card) {
        let commitment = ActionCommitment::exact(connection.card, owner, connection.identity);
        value.public_actions.push(commitment);
        value.owner_actions.push(commitment);
    }
    value.new_connections += 1;
}

/// Some trash conventions teach the recipient through an intervening play.
/// Prove that sequence with the recipient's replay instead of waiving GTP
/// because a candidate has an advanced label.
#[derive(Default)]
struct DemonstratedResponse {
    trash: Vec<CardId>,
    actions: Vec<ActionCommitment>,
}

fn demonstrated_response(
    source: &PlayerView,
    profile: HGroupProfile,
    compiled: &super::CompiledProspectiveClue,
    target: PlayerId,
    touched: &[CardId],
) -> DemonstratedResponse {
    let trash = touched
        .iter()
        .copied()
        .filter(|card| {
            identity_of(source, *card).is_some_and(|id| !is_eventually_useful(source, id))
        })
        .collect::<Vec<_>>();
    if trash.is_empty() {
        return DemonstratedResponse::default();
    }
    let actor = super::next_player(source.current_player, source.hands.len());
    if actor == target {
        return DemonstratedResponse::default();
    }
    let Some(response) = compiled.projection(actor) else {
        return DemonstratedResponse::default();
    };
    let Some(card) =
        super::preferred_due_play_card(response.deductions.view(), &response.inferred, profile)
    else {
        return DemonstratedResponse::default();
    };
    let Some(identity) = identity_of(source, card).filter(|id| is_playable_now(source, *id)) else {
        return DemonstratedResponse::default();
    };
    let after =
        super::ProspectiveTransition::successful_play(compiled.after(), actor, card, identity);
    let Some((deductions, replay)) = super::projected_h_group_replay(&after, profile, target)
    else {
        return DemonstratedResponse::default();
    };
    let notes = super::convention_card_inferences(&deductions, &replay);
    let inferred = super::infer_h_group_from_replay(&deductions, replay.clone(), profile);
    let actions = compiled.projection(target).map_or_else(Vec::new, |before| {
        inferred
            .playable_now
            .iter()
            .copied()
            .filter(|card| !before.inferred.playable_now.contains(card))
            .filter_map(|card| {
                identity_of(source, card)
                    .filter(|id| is_playable_now(&after, *id))
                    .map(|identity| ActionCommitment::exact(card, target, identity))
            })
            .collect()
    });
    let trash = trash
        .into_iter()
        .filter(|card| {
            replay.cards.discard_now.contains(card)
                || notes.iter().any(|note| {
                    note.card == *card
                        && !note.identities.is_empty()
                        && note
                            .identities
                            .iter()
                            .all(|id| !is_eventually_useful(&after, id))
                })
        })
        .collect();
    DemonstratedResponse { trash, actions }
}

#[cfg(test)]
mod tests {
    use super::*;
    use hanabi_core::{Clue, Rank, Suit};
    use hanabi_protocol::HanabiLiveReplay;

    #[test]
    fn reviewed_causal_outcomes_are_identical_with_and_without_cache() {
        // Reviewed p4v0s1 turns 19 and 21: two indirect acquisitions per clue.
        // Compare every field, including untouched knowledge and protection.
        let fixture = HanabiLiveReplay::from_json(include_str!(
            "../../../hanabi-protocol/tests/fixtures/game-p4v0s1.json"
        ))
        .unwrap();
        for (turn, target, clue) in [
            (18, PlayerId::new(0), Clue::Rank(Rank::Four)),
            (20, PlayerId::new(3), Clue::Suit(Suit::Purple)),
        ] {
            let state = fixture.state_at_turn(turn).unwrap();
            let source = state.view_for(state.current_player()).unwrap();
            super::super::with_prospective_analysis_cache(&source, HGroupProfile::Max, || {
                let team = compiled_baseline_team(&source, HGroupProfile::Max);
                let baselines = (0..4)
                    .map(|p| {
                        projected_line_state(&source, &team.projection(PlayerId::new(p)).unwrap())
                    })
                    .collect::<Vec<_>>();
                let touched = source.hands[target.index()]
                    .iter()
                    .filter(|card| card.identity.is_some_and(|id| clue.matches(id)))
                    .map(|card| card.id)
                    .collect::<Vec<_>>();
                let compiled =
                    compiled_prospective_clue(&source, HGroupProfile::Max, target, clue, &touched)
                        .unwrap();
                for kind in [
                    Some(HGroupMoveKind::Bluff),
                    Some(HGroupMoveKind::PlayClue),
                    None,
                ] {
                    let direct =
                        compile_uncached(&source, HGroupProfile::Max, &compiled, &baselines, kind);
                    let first = compiled.outcome(&source, HGroupProfile::Max, &baselines, kind);
                    let cached = compiled.outcome(&source, HGroupProfile::Max, &baselines, kind);
                    assert_eq!(direct, first);
                    assert_eq!(first, cached);
                    assert!(direct.is_some());
                }
            });
        }
    }
}
