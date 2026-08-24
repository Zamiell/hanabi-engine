use super::{
    Arc, Card, CardId, Clue, ClueFacts, ConnectionObligation, GameStatus, HGroupCardInference,
    HGroupInferences, HGroupProfile, HGroupState, LogicalDeductions, MAX_CLUE_TOKENS, ObservedCard,
    ObservedEvent, ObservedHistoryEntry, PerspectiveProjector, PlayerId, PlayerView,
    ProspectiveTransition, Rank, RefCell, chop, convention_card_inferences, identity_of,
    infer_h_group, infer_h_group_from_replay, is_playable_now, next_player, replay_h_group_inner,
};

pub(super) fn subjective_convention_cards(
    source: &PlayerView,
    profile: HGroupProfile,
    observer: PlayerId,
) -> Option<Vec<HGroupCardInference>> {
    let (deductions, replay) = subjective_h_group_replay(source, profile, observer)?;
    Some(convention_card_inferences(&deductions, &replay))
}

fn subjective_h_group_replay(
    source: &PlayerView,
    profile: HGroupProfile,
    observer: PlayerId,
) -> Option<(LogicalDeductions, HGroupState)> {
    projected_h_group_replay_inner(source, profile, observer, false)
}

pub(super) fn projected_h_group_replay(
    source: &PlayerView,
    profile: HGroupProfile,
    observer: PlayerId,
) -> Option<(LogicalDeductions, HGroupState)> {
    projected_h_group_replay_inner(source, profile, observer, true)
}

#[derive(Clone)]
struct CachedProspectiveProjection {
    replay: HGroupState,
    inferred: HGroupInferences,
}

#[derive(Clone)]
enum ProspectiveCacheSlot<T> {
    Empty,
    Computed(Option<T>),
}

struct ProspectiveAnalysisCache {
    source_address: usize,
    profile: HGroupProfile,
    baselines: Vec<ProspectiveCacheSlot<CachedProspectiveProjection>>,
    source_hand_worlds: ProspectiveCacheSlot<Arc<Vec<PlayerView>>>,
}

thread_local! {
    static PROSPECTIVE_ANALYSIS_CACHE: RefCell<Option<ProspectiveAnalysisCache>> =
        const { RefCell::new(None) };
}

struct ProspectiveAnalysisCacheGuard(Option<ProspectiveAnalysisCache>);

impl Drop for ProspectiveAnalysisCacheGuard {
    fn drop(&mut self) {
        let previous = self.0.take();
        PROSPECTIVE_ANALYSIS_CACHE.with(|cache| {
            cache.replace(previous);
        });
    }
}

pub(super) fn with_prospective_analysis_cache<T>(
    source: &PlayerView,
    profile: HGroupProfile,
    operation: impl FnOnce() -> T,
) -> T {
    let replacement = ProspectiveAnalysisCache {
        source_address: core::ptr::from_ref(source).addr(),
        profile,
        baselines: vec![ProspectiveCacheSlot::Empty; source.hands.len()],
        source_hand_worlds: ProspectiveCacheSlot::Empty,
    };
    let previous = PROSPECTIVE_ANALYSIS_CACHE.with(|cache| cache.replace(Some(replacement)));
    let _guard = ProspectiveAnalysisCacheGuard(previous);
    operation()
}

fn prospective_baseline_projection(
    source: &PlayerView,
    profile: HGroupProfile,
    observer: PlayerId,
) -> Option<CachedProspectiveProjection> {
    let source_address = core::ptr::from_ref(source).addr();
    if let Some(cached) = PROSPECTIVE_ANALYSIS_CACHE.with(|cache| {
        let cache = cache.borrow();
        let cache = cache.as_ref()?;
        if cache.source_address != source_address || cache.profile != profile {
            return None;
        }
        match &cache.baselines[observer.index()] {
            ProspectiveCacheSlot::Empty => None,
            ProspectiveCacheSlot::Computed(projection) => Some(projection.clone()),
        }
    }) {
        return cached;
    }
    let computed =
        projected_h_group_replay(source, profile, observer).map(|(deductions, replay)| {
            let inferred = infer_h_group_from_replay(&deductions, replay.clone(), profile);
            CachedProspectiveProjection { replay, inferred }
        });
    PROSPECTIVE_ANALYSIS_CACHE.with(|cache| {
        let mut cache = cache.borrow_mut();
        if let Some(cache) = cache.as_mut() {
            if cache.source_address == source_address && cache.profile == profile {
                cache.baselines[observer.index()] =
                    ProspectiveCacheSlot::Computed(computed.clone());
            }
        }
    });
    computed
}

fn prospective_source_hand_worlds(
    source: &PlayerView,
    profile: HGroupProfile,
) -> Option<Arc<Vec<PlayerView>>> {
    let source_address = core::ptr::from_ref(source).addr();
    if let Some(cached) = PROSPECTIVE_ANALYSIS_CACHE.with(|cache| {
        let cache = cache.borrow();
        let cache = cache.as_ref()?;
        if cache.source_address != source_address || cache.profile != profile {
            return None;
        }
        match &cache.source_hand_worlds {
            ProspectiveCacheSlot::Empty => None,
            ProspectiveCacheSlot::Computed(worlds) => Some(worlds.clone()),
        }
    }) {
        return cached;
    }
    let mut worlds = Vec::new();
    PerspectiveProjector::new(source, profile).visit_source_hand_worlds(256, |world| {
        worlds.push(world.clone());
        false
    })?;
    let computed = Arc::new(worlds);
    PROSPECTIVE_ANALYSIS_CACHE.with(|cache| {
        let mut cache = cache.borrow_mut();
        if let Some(cache) = cache.as_mut() {
            if cache.source_address == source_address && cache.profile == profile {
                cache.source_hand_worlds =
                    ProspectiveCacheSlot::Computed(Some(Arc::clone(&computed)));
            }
        }
    });
    Some(computed)
}

fn projected_h_group_replay_inner(
    source: &PlayerView,
    profile: HGroupProfile,
    observer: PlayerId,
    model_other_players: bool,
) -> Option<(LogicalDeductions, HGroupState)> {
    PerspectiveProjector::new(source, profile).project(observer, model_other_players)
}

pub(super) fn prospective_play_has_unsafe_inference(
    deductions: &LogicalDeductions,
    profile: HGroupProfile,
    card: CardId,
) -> bool {
    let source = deductions.view();
    let inferred = infer_h_group(deductions, profile);
    let identity = deductions
        .possible_identities(card)
        .filter(|identities| identities.len() == 1)
        .and_then(|identities| identities.iter().next())
        .or_else(|| {
            inferred
                .cards
                .iter()
                .find(|note| note.card == card && note.identities.len() == 1)
                .and_then(|note| note.identities.iter().next())
        });
    let Some(identity) = identity else {
        return false;
    };
    let following = next_player(source.observer, source.hands.len());
    let Some((baseline_deductions, baseline_replay)) =
        projected_h_group_replay(source, profile, following)
    else {
        return true;
    };
    let baseline = infer_h_group_from_replay(&baseline_deductions, baseline_replay, profile);
    let after_play = prospective_play_view(source, source.observer, card, identity);
    let Some((after_deductions, after_replay)) =
        projected_h_group_replay(&after_play, profile, following)
    else {
        return true;
    };
    let after = infer_h_group_from_replay(&after_deductions, after_replay, profile);
    let newly_promised = after
        .playable_now
        .iter()
        .copied()
        .filter(|candidate| !baseline.playable_now.contains(candidate))
        .chain(
            after
                .connection
                .map(|connection| connection.card)
                .filter(|candidate| {
                    baseline
                        .connection
                        .is_none_or(|prior| prior.card != *candidate)
                }),
        );

    newly_promised.into_iter().any(|candidate| {
        identity_of(source, candidate).is_some_and(|actual| !is_playable_now(&after_play, actual))
    })
}

pub(super) fn prospective_clue_marks_focus_saved(
    source: &PlayerView,
    profile: HGroupProfile,
    target: PlayerId,
    focus: CardId,
    clue: Clue,
    touched: &[CardId],
) -> bool {
    let focus_is_saved = |world: &PlayerView| {
        let after_clue = prospective_clue_view(world, target, clue, touched);
        projected_h_group_replay(&after_clue, profile, target)
            .map(|(deductions, replay)| infer_h_group_from_replay(&deductions, replay, profile))
            .is_some_and(|inferred| inferred.saved.contains(&focus))
    };
    if !focus_is_saved(source) {
        return false;
    }

    // The recipient sees the giver's complete hand even though the giver does
    // not. Visit joint, card-count-consistent giver hands instead of mutating
    // one card at a time into combinations that may not form a legal world.
    let Some(worlds) = prospective_source_hand_worlds(source, profile) else {
        return false;
    };
    let unsafe_world = worlds.iter().any(|world| {
        let after_clue = prospective_clue_view(world, target, clue, touched);
        PerspectiveProjector::project_resolved_owned(after_clue, profile, target).is_none_or(
            |(deductions, replay)| {
                replay.chop_moved.contains(&focus)
                    || !infer_h_group_from_replay(&deductions, replay, profile)
                        .saved
                        .contains(&focus)
            },
        )
    });
    !unsafe_world
}

pub(super) fn prospective_clue_has_unsafe_connection(
    source: &PlayerView,
    profile: HGroupProfile,
    target: PlayerId,
    focus: CardId,
    clue: Clue,
    touched: &[CardId],
    expect_immediate_focus: bool,
) -> bool {
    prospective_clue_hazard(
        source,
        profile,
        target,
        focus,
        clue,
        touched,
        expect_immediate_focus,
    )
    .is_some()
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum ProspectiveClueHazard {
    ProjectionFailed,
    RecipientMissingFocusPlay,
    RecipientWrongSave,
    RecipientWrongPlay,
    RecipientWrongConnection,
    OtherPlayerWrongPromise,
    FalseConnectionIdentity,
}

pub(super) fn prospective_clue_hazard(
    source: &PlayerView,
    profile: HGroupProfile,
    target: PlayerId,
    focus: CardId,
    clue: Clue,
    touched: &[CardId],
    expect_immediate_focus: bool,
) -> Option<ProspectiveClueHazard> {
    let after_clue = prospective_clue_view(source, target, clue, touched);

    let after_projector = PerspectiveProjector::new(&after_clue, profile);
    let Some((deductions, replay)) = after_projector.project(target, true) else {
        return Some(ProspectiveClueHazard::ProjectionFailed);
    };
    let inferred = infer_h_group_from_replay(&deductions, replay.clone(), profile);
    let Some(baseline) = prospective_baseline_projection(source, profile, target) else {
        return Some(ProspectiveClueHazard::ProjectionFailed);
    };
    if let Some(hazard) = recipient_projection_hazard(
        source,
        focus,
        expect_immediate_focus,
        &replay,
        &inferred,
        &baseline.replay,
    ) {
        return Some(hazard);
    }
    if other_player_projection_is_unsafe(source, &after_clue, profile, target, &after_projector) {
        return Some(ProspectiveClueHazard::OtherPlayerWrongPromise);
    }
    if touched.len() <= 1 {
        return None;
    }
    replay
        .pending_connections
        .iter()
        .filter(|connection| connection.focus == focus)
        .filter(|connection| is_new_connection(connection, &baseline.replay))
        .any(|connection| {
            connection.cards.iter().copied().any(|card| {
                identity_of(source, card).is_some_and(|actual| {
                    actual != connection.expected && !is_playable_now(source, actual)
                })
            })
        })
        .then_some(ProspectiveClueHazard::FalseConnectionIdentity)
}

fn recipient_projection_hazard(
    source: &PlayerView,
    focus: CardId,
    expect_immediate_focus: bool,
    replay: &HGroupState,
    inferred: &HGroupInferences,
    baseline: &HGroupState,
) -> Option<ProspectiveClueHazard> {
    let missing_focus = expect_immediate_focus
        && identity_of(source, focus).is_some_and(|actual| is_playable_now(source, actual))
        && !inferred.playable_now.contains(&focus);
    let wrong_save = replay
        .implicit_saves
        .iter()
        .filter(|(card, identities)| {
            !baseline
                .implicit_saves
                .iter()
                .any(|(prior_card, prior_identities)| {
                    prior_card == card && prior_identities == identities
                })
        })
        .any(|(card, identities)| {
            identity_of(source, *card).is_some_and(|actual| !identities.contains(actual))
        });
    let wrong_play = inferred.playable_now.iter().copied().any(|card| {
        identity_of(source, card).is_some_and(|actual| !is_playable_now(source, actual))
    });
    let wrong_connection = inferred.connection.is_some_and(|connection| {
        identity_of(source, connection.card)
            .is_some_and(|actual| actual != connection.identity && !is_playable_now(source, actual))
    });
    if missing_focus {
        Some(ProspectiveClueHazard::RecipientMissingFocusPlay)
    } else if wrong_save {
        Some(ProspectiveClueHazard::RecipientWrongSave)
    } else if wrong_play {
        Some(ProspectiveClueHazard::RecipientWrongPlay)
    } else if wrong_connection {
        Some(ProspectiveClueHazard::RecipientWrongConnection)
    } else {
        None
    }
}

fn other_player_projection_is_unsafe(
    source: &PlayerView,
    after_clue: &PlayerView,
    profile: HGroupProfile,
    target: PlayerId,
    after_projector: &PerspectiveProjector<'_>,
) -> bool {
    for player in 0..source.hands.len() {
        let observer =
            PlayerId::new(u8::try_from(player).expect("standard Hanabi has at most five players"));
        if observer == target || observer == source.observer {
            continue;
        }
        let Some(other_baseline) = prospective_baseline_projection(source, profile, observer)
        else {
            return true;
        };
        let Some((other_after_deductions, other_after_replay)) =
            after_projector.project(observer, true)
        else {
            return true;
        };
        let other_after =
            infer_h_group_from_replay(&other_after_deductions, other_after_replay, profile);
        let newly_promised = other_after
            .playable_now
            .iter()
            .copied()
            .filter(|card| !other_baseline.inferred.playable_now.contains(card))
            .chain(
                other_after
                    .connection
                    .map(|connection| connection.card)
                    .filter(|card| {
                        other_baseline
                            .inferred
                            .connection
                            .is_none_or(|prior| prior.card != *card)
                    }),
            );
        if newly_promised.into_iter().any(|card| {
            identity_of(source, card).is_some_and(|actual| !is_playable_now(after_clue, actual))
        }) {
            return true;
        }
    }
    false
}

fn is_new_connection(connection: &ConnectionObligation, baseline: &HGroupState) -> bool {
    !replay_contains_connection(baseline, connection)
}

fn replay_contains_connection(replay: &HGroupState, connection: &ConnectionObligation) -> bool {
    replay.pending_connections.iter().any(|prior| {
        prior.actor == connection.actor
            && prior.cards == connection.cards
            && prior.expected == connection.expected
            && prior.kind == connection.kind
            && prior.focus == connection.focus
            && prior.step == connection.step
    })
}

pub(super) fn prospective_clue_view(
    source: &PlayerView,
    target: PlayerId,
    clue: Clue,
    touched: &[CardId],
) -> PlayerView {
    ProspectiveTransition::clue(source, target, clue, touched)
}

pub(super) fn prospective_play_view(
    source: &PlayerView,
    player: PlayerId,
    card: CardId,
    identity: Card,
) -> PlayerView {
    ProspectiveTransition::successful_play(source, player, card, identity)
}

pub(super) fn subjective_chop_before_action(
    source: &PlayerView,
    profile: HGroupProfile,
    observer: PlayerId,
    history: &[ObservedHistoryEntry],
    hands: &[Vec<CardId>],
    facts: &[ClueFacts],
    deck_size: usize,
) -> Option<CardId> {
    let observed_hands = hands
        .iter()
        .enumerate()
        .map(|(player, hand)| {
            hand.iter()
                .map(|card| ObservedCard {
                    id: *card,
                    identity: (player != observer.index())
                        .then(|| identity_of(source, *card))
                        .flatten(),
                    clues: facts[card.index()],
                })
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>();
    let projected_history = history
        .iter()
        .cloned()
        .map(|mut entry| {
            if let ObservedEvent::Drew {
                player,
                card,
                identity,
            } = &mut entry.event
            {
                *identity = (*player != observer)
                    .then(|| identity_of(source, *card))
                    .flatten();
            }
            entry
        })
        .collect::<Vec<_>>();
    let mut play_stacks: [Vec<(CardId, Card)>; 5] = std::array::from_fn(|_| Vec::new());
    let mut discard_pile = Vec::new();
    let mut clue_tokens = MAX_CLUE_TOKENS;
    let mut strikes = 0;
    for entry in history {
        match entry.event {
            ObservedEvent::Clued { .. } => clue_tokens = clue_tokens.saturating_sub(1),
            ObservedEvent::Played {
                card,
                identity,
                successful: true,
                ..
            } => {
                play_stacks[identity.suit.index()].push((card, identity));
                if identity.rank == Rank::Five {
                    clue_tokens = clue_tokens.saturating_add(1).min(MAX_CLUE_TOKENS);
                }
            }
            ObservedEvent::Played {
                card,
                identity,
                successful: false,
                ..
            } => {
                discard_pile.push((card, identity));
                strikes += 1;
            }
            ObservedEvent::Discarded { card, identity, .. } => {
                discard_pile.push((card, identity));
                clue_tokens = clue_tokens.saturating_add(1).min(MAX_CLUE_TOKENS);
            }
            ObservedEvent::Drew { .. } => {}
        }
    }
    let view = PlayerView {
        observer,
        current_player: observer,
        turn: history
            .last()
            .map_or(0, |entry| entry.turn.saturating_add(1)),
        hands: observed_hands,
        deck_size,
        play_stacks,
        discard_pile,
        clue_tokens,
        strikes,
        final_turns_remaining: None,
        status: GameStatus::InProgress,
        history: projected_history,
    };
    let deductions = LogicalDeductions::new(view).ok()?;
    let replay = replay_h_group_inner(&deductions, profile, false);
    let promptable = replay.promptable();
    let gotten = replay.gotten_from(&promptable);
    chop(&replay.hands[observer.index()], &gotten)
}
