use super::{
    Action, ActionCommitment, Card, CardId, ClueCandidate, EpistemicState, HGroupConnection,
    HGroupMoveKind, HGroupProfile, HGroupRuleId, IdentitySet, LineOutcome, LogicalDeductions,
    PlayerId, PlayerView, Rank, card_is_trash, identity_of, infer_h_group_from_replay,
    is_eventually_useful, is_playable_now, projected_h_group_replay, prospective_clue_signal_kinds,
    prospective_clue_view, rule_enabled,
};

const TEAM_ACTION_COVERAGE_PENALTY: u16 = 80;
const TEAM_MULTI_CARD_PROTECTION_BONUS: u16 = 80;
const TEAM_ACTION_DELAY_PENALTY: u16 = 2;
const INDIRECT_CONNECTION_PENALTY: u16 = 24;

/// Compares whole clue outcomes after ordinary legality and convention
/// interpretation have produced the candidate set.
///
/// [Level 10's Directness Principle](https://hanabi.github.io/level-10/#directness-principle)
/// prefers the least complicated route to an
/// identical set of promised cards. Team action coverage separately rewards a
/// clue that establishes useful future actions for more than one teammate;
/// this keeps a token-refunding play from winning when the extra token has no
/// immediate job but another line prepares the rest of the team.
pub(super) fn apply_strategic_clue_values(
    deductions: &LogicalDeductions,
    profile: HGroupProfile,
    candidates: &mut [ClueCandidate],
) {
    if !rule_enabled(profile, HGroupRuleId::SpecialDiscards) || candidates.len() < 2 {
        return;
    }
    let source = deductions.view();
    let baselines = (0..source.hands.len())
        .map(|player| {
            let observer = PlayerId::new(
                u8::try_from(player).expect("standard Hanabi has at most five players"),
            );
            projected_line_state(source, profile, observer)
        })
        .collect::<Option<Vec<_>>>();
    let Some(baselines) = baselines else {
        return;
    };
    let values = candidates
        .iter()
        .map(|candidate| clue_line_value(source, profile, candidate.action, &baselines))
        .collect::<Vec<_>>();
    let best_coverage = values
        .iter()
        .filter_map(|value| value.as_ref().map(LineOutcome::covered_players))
        .max()
        .unwrap_or(0);
    let best_action_distance = values
        .iter()
        .filter_map(|value| {
            value
                .as_ref()
                .map(|value| value.first_action_distance(source.current_player, source.hands.len()))
        })
        .min()
        .unwrap_or(source.hands.len());

    for (index, candidate) in candidates.iter_mut().enumerate() {
        let Some(value) = &values[index] else {
            continue;
        };
        let candidate_is_opening_bluff =
            source.turn == 0 && clue_is_bluff(source, profile, candidate.action);
        // An opening Bluff has no established team action to displace, so do
        // not penalize the concentration that makes the Bluff work. Once the
        // game has started, ordinary Teamwork comparisons still apply.
        let uncovered_players = if candidate_is_opening_bluff {
            0
        } else {
            best_coverage.saturating_sub(value.covered_players())
        };
        candidate.value.penalize_teamwork(
            TEAM_ACTION_COVERAGE_PENALTY
                .saturating_mul(u16::try_from(uncovered_players).unwrap_or(u16::MAX)),
        );
        let consolidates_chop_move = match candidate.action {
            Action::Clue { target, clue } => source.hands[target.index()].iter().any(|card| {
                card.identity.is_some_and(|identity| clue.matches(identity))
                    && baselines[source.observer.index()]
                        .chop_moved
                        .contains(&card.id)
            }),
            Action::Play(_) | Action::Discard(_) => false,
        };
        if consolidates_chop_move {
            let extra_protection = value.protected_card_count().saturating_sub(1);
            candidate.value.reward_teamwork(
                TEAM_MULTI_CARD_PROTECTION_BONUS
                    .saturating_mul(u16::try_from(extra_protection).unwrap_or(u16::MAX)),
            );
        }
        let action_delay = value
            .first_action_distance(source.current_player, source.hands.len())
            .saturating_sub(best_action_distance);
        candidate.value.penalize_delay(
            TEAM_ACTION_DELAY_PENALTY
                .saturating_mul(u16::try_from(action_delay).unwrap_or(u16::MAX)),
        );

        let fewest_equivalent_connections = values
            .iter()
            .filter_map(Option::as_ref)
            .filter(|other| other.directness_key() == value.directness_key())
            .map(|other| other.new_connections)
            .min()
            .unwrap_or(value.new_connections);
        let unnecessary_connections = value
            .new_connections
            .saturating_sub(fewest_equivalent_connections);
        candidate.value.penalize_indirectness(
            INDIRECT_CONNECTION_PENALTY
                .saturating_mul(u16::try_from(unnecessary_connections).unwrap_or(u16::MAX)),
        );
    }
}

fn clue_is_bluff(source: &PlayerView, profile: HGroupProfile, action: Action) -> bool {
    let Action::Clue { target, clue } = action else {
        return false;
    };
    let touched = source.hands[target.index()]
        .iter()
        .filter(|card| card.identity.is_some_and(|identity| clue.matches(identity)))
        .map(|card| card.id)
        .collect::<Vec<_>>();
    prospective_clue_signal_kinds(source, profile, target, clue, &touched)
        .into_iter()
        .any(|kind| {
            matches!(
                kind,
                HGroupMoveKind::Bluff | HGroupMoveKind::SelfBluff | HGroupMoveKind::DoubleBluff
            )
        })
}

#[derive(Clone)]
struct ProjectedLineState {
    giver_visible_commitments: Vec<(CardId, Card)>,
    giver_visible_promises: Vec<(CardId, Card)>,
    epistemic: EpistemicState,
    owner_promises: Vec<(CardId, IdentitySet)>,
    connection: Option<HGroupConnection>,
    chop_moved: super::CardSet,
    causal_cards: super::CardSet,
}

impl ProjectedLineState {
    /// Team coverage is evaluated by the clue giver, who may legally use the
    /// visible identities in teammates' hands. This projection is kept
    /// separate from owner knowledge so it can never establish Directness.
    fn closed_public_commitments(&self, source: &PlayerView) -> Vec<(CardId, Card)> {
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
    /// identical outcomes for the Directness Principle. Team coverage retains
    /// the established public projection above; equivalence is stricter and
    /// must not use identities visible only to another player.
    fn closed_owner_commitments(&self, source: &PlayerView) -> Vec<(CardId, Card)> {
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

fn projected_line_state(
    source: &PlayerView,
    profile: HGroupProfile,
    observer: PlayerId,
) -> Option<ProjectedLineState> {
    let (deductions, replay) = projected_h_group_replay(source, profile, observer)?;
    let promised = replay
        .cards
        .explicitly_clued
        .union(&replay.cards.invisibly_clued)
        .copied()
        .collect::<Vec<_>>();
    let chop_moved = replay.cards.chop_moved.clone();
    let causal_turn = source.history.last().map(|entry| entry.turn);
    let causal_cards = replay
        .signals
        .iter()
        .filter(|signal| Some(signal.turn) == causal_turn)
        .flat_map(|signal| signal.cards.iter().copied())
        .collect();
    let inferred = infer_h_group_from_replay(&deductions, replay, profile);
    let epistemic = EpistemicState::from_analysis(&deductions, &inferred);
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
                .map(|identity| (card, identity))
        })
        .collect::<Vec<_>>();
    giver_visible_promises
        .sort_unstable_by_key(|(card, identity)| (card.index(), identity.index()));
    giver_visible_promises.dedup();
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
    Some(ProjectedLineState {
        giver_visible_commitments,
        giver_visible_promises,
        epistemic,
        owner_promises,
        connection: inferred.connection,
        chop_moved,
        causal_cards,
    })
}

fn clue_line_value(
    source: &PlayerView,
    profile: HGroupProfile,
    action: Action,
    baselines: &[ProjectedLineState],
) -> Option<LineOutcome> {
    let Action::Clue { target, clue } = action else {
        return None;
    };
    let touched = source.hands[target.index()]
        .iter()
        .filter(|card| card.identity.is_some_and(|identity| clue.matches(identity)))
        .map(|card| card.id)
        .collect::<Vec<_>>();
    let after_clue = prospective_clue_view(source, target, clue, &touched);
    let mut value = LineOutcome::default();
    let caused_by_clue = |card: CardId, identity: Card| {
        touched.contains(&card)
            || touched.iter().copied().any(|touched_card| {
                identity_of(source, touched_card).is_some_and(|touched_identity| {
                    touched_identity.suit == identity.suit
                        && touched_identity.rank.number() < identity.rank.number()
                })
            })
    };

    for (player, baseline) in baselines.iter().enumerate() {
        let observer =
            PlayerId::new(u8::try_from(player).expect("standard Hanabi has at most five players"));
        let after = projected_line_state(&after_clue, profile, observer)?;
        let commitment_caused = |card: CardId, identity: Card| {
            caused_by_clue(card, identity) || after.causal_cards.contains(&card)
        };
        let baseline_public_commitments = baseline.closed_public_commitments(source);
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
                            && identities
                                .iter()
                                .any(|identity| commitment_caused(*card, identity)))
                        .then_some(*card)
                    }),
            );
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
        if let Some(connection) = after.connection.filter(|connection| {
            baseline
                .connection
                .is_none_or(|prior| prior.card != connection.card)
                && !baseline_public_commitments.iter().any(|(card, identity)| {
                    *card == connection.card && *identity == connection.identity
                })
        }) {
            record_new_connection(&mut value, source, connection);
        }
    }
    value.normalize();
    Some(value)
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

fn card_owner(source: &PlayerView, card: CardId) -> Option<PlayerId> {
    source
        .hands
        .iter()
        .position(|hand| hand.iter().any(|candidate| candidate.id == card))
        .and_then(|index| u8::try_from(index).ok())
        .map(PlayerId::new)
}
