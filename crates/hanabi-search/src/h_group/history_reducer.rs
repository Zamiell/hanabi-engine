//! Event-specific history reduction. One owned state is mutated in the same
//! order as the public journal; post-event recognizers still receive the
//! restricted `HGroupRuleEffects` capability, not this reducer.
use super::{
    ActorBeliefBefore, BluffTargetKind, Card, CardId, CardSet, Clue, ClueConnectionStep, ClueFacts,
    ClueInterpretationHypothesis, ClueInterpretationPlan, ConnectionManager, ConnectionObligation,
    ConnectionPlanningContext, ConnectionTransitionReason, ConventionCardSetSnapshot,
    ConventionCardState, ConventionJournal, ConventionKnowledge, ConventionTransitionDelta,
    ConventionTransitionResult, CurrentClueTouches, DirectPlayDeclines, EffectSource,
    FixObligations, HGroupClueInterpretation, HGroupClueKind, HGroupConnectionKind, HGroupMoveKind,
    HGroupProfile, HGroupRuleEffects, HGroupRuleId, HGroupState, HGroupTurnContext,
    HGroupTurnSnapshot, HGroupTurnView, HistoricalView, IdentitySet, LogicalDeductions,
    MAX_CLUE_TOKENS, ObservedEvent, ObservedHistoryEntry, PerspectiveDepth, PlayerId, PlayerSet,
    PlayerView, PrimaryClueInputs, PromiseId, PromptableBeforeClue, ProvenancedCardSet, Rank,
    RuleExecutionContext, SubjectiveReplayRequest, Suit, active_invisibly_clued,
    apply_post_event_rules, bluff_play_connects, bluff_target_kind_at, bluff_target_order_is_legal,
    build_convention_knowledge, chop, claimed_identities_at_clue,
    clue_permits_direct_play_deferral, elimination_finesse_card, finesse_position_id,
    five_chop_moved_card, focus, interpretation, is_playable_at, loaded_connection_plan,
    next_player, primary, protected_cards, public_layers, push_signal,
    reconcile_connection_fact_lifecycles, record_declined_direct_plays, remove_card, rule_enabled,
    snapshot_good_touch_identities, snapshot_play_identities, snapshot_save_identities,
    subjective_action_context_before, was_clued_before, was_clued_before_with,
};

struct ReplayReducer {
    hands: Vec<Vec<CardId>>,
    explicitly_clued: ProvenancedCardSet,
    invisibly_clued: ProvenancedCardSet,
    clues: Vec<HGroupClueInterpretation>,
    public_removed: [u8; 25],
    facts: Vec<ClueFacts>,
    stack_heights: [u8; 5],
    historical_deck_size: usize,
    pending_connections: ConnectionManager,
    already_playing: ProvenancedCardSet,
    early_game: bool,
    signals: ConventionJournal,
    chop_moved: ProvenancedCardSet,
    discard_now: Vec<CardId>,
    must_clue: PlayerSet,
    forced_playable: ProvenancedCardSet,
    invalidated_focuses: CardSet,
    declined_direct_plays: CardSet,
    declined_direct_play_turns: Vec<(CardId, u32)>,
    implicit_saves: Vec<(CardId, IdentitySet)>,
    required_fixes: FixObligations,
    transitions: Vec<ConventionTransitionResult>,
    historical_clue_tokens: u8,
}

struct ReplayEvent<'a> {
    event_connection_transition_start: usize,
    view: &'a PlayerView,
    profile: HGroupProfile,
    perspective_depth: PerspectiveDepth,
    allow_blind_reverse_empathy: bool,
    entry_index: usize,
    entry: &'a ObservedHistoryEntry,
    historical: HistoricalView<'a>,
    before: &'a HGroupTurnSnapshot,
    clue_tokens_before: u8,
    action_is_settled: bool,
    outcome: ReplayEventOutcome,
}

struct ReplayEventOutcome {
    actor_saw_normal_discard: bool,
    actor_known_discard_identity: Option<Card>,
    declined_with_clue: Option<PlayerId>,
    required_clue_deferral: bool,
}

pub(super) fn replay_h_group_inner_uncached(
    deductions: &LogicalDeductions,
    profile: HGroupProfile,
    perspective_depth: PerspectiveDepth,
    allow_blind_reverse_empathy: bool,
) -> HGroupState {
    debug_assert!(rule_enabled(profile, HGroupRuleId::Basic));
    let view = deductions.view();
    let hand_size = if view.hands.len() <= 3 { 5 } else { 4 };
    let hands = (0..view.hands.len())
        .map(|player| {
            let first = player * hand_size;
            (first..first + hand_size).map(CardId::new).collect()
        })
        .collect::<Vec<Vec<CardId>>>();
    let explicitly_clued = ProvenancedCardSet::default();
    let invisibly_clued = ProvenancedCardSet::default();
    let clues = Vec::<HGroupClueInterpretation>::new();
    let public_removed = [0_u8; 25];
    let facts = vec![ClueFacts::default(); 50];
    let stack_heights = [0_u8; 5];
    let historical_deck_size = view.deck_size
        + view
            .history
            .iter()
            .filter(|entry| matches!(entry.event, ObservedEvent::Drew { .. }))
            .count();
    let pending_connections = ConnectionManager::default();
    let already_playing = ProvenancedCardSet::default();
    let early_game = true;
    let signals = ConventionJournal::default();
    let chop_moved = ProvenancedCardSet::default();
    let discard_now = Vec::new();
    let must_clue = PlayerSet::default();
    let forced_playable = ProvenancedCardSet::default();
    let invalidated_focuses = CardSet::default();
    let declined_direct_plays = CardSet::default();
    let declined_direct_play_turns = Vec::<(CardId, u32)>::new();
    let implicit_saves = Vec::new();
    let required_fixes = FixObligations::default();
    let transitions = Vec::new();
    let historical_clue_tokens = MAX_CLUE_TOKENS;

    ReplayReducer {
        hands,
        explicitly_clued,
        invisibly_clued,
        clues,
        public_removed,
        facts,
        stack_heights,
        historical_deck_size,
        pending_connections,
        already_playing,
        early_game,
        signals,
        chop_moved,
        discard_now,
        must_clue,
        forced_playable,
        invalidated_focuses,
        declined_direct_plays,
        declined_direct_play_turns,
        implicit_saves,
        required_fixes,
        transitions,
        historical_clue_tokens,
    }
    .run(
        deductions,
        profile,
        perspective_depth,
        allow_blind_reverse_empathy,
    )
}

impl ReplayReducer {
    #[allow(clippy::too_many_lines)]
    fn run(
        mut self,
        deductions: &LogicalDeductions,
        profile: HGroupProfile,
        perspective_depth: PerspectiveDepth,
        allow_blind_reverse_empathy: bool,
    ) -> HGroupState {
        let view = deductions.view();
        for (entry_index, entry) in view.history.iter().enumerate() {
            let event_connection_transition_start = self.pending_connections.transitions().len();
            let event_card_snapshot = ConventionCardSetSnapshot::capture(
                &self.explicitly_clued,
                &self.invisibly_clued,
                &self.already_playing,
                &self.chop_moved,
                &self.forced_playable,
            );
            let historical = HistoricalView::new(view, entry.turn);
            let mut actor_saw_normal_discard = false;
            let mut actor_known_discard_identity = None;
            let mut declined_with_clue = None;
            let mut required_clue_deferral = false;
            let action_is_settled = view.history[entry_index + 1..]
                .iter()
                .any(|later| later.turn > entry.turn);
            let preceding_turn = entry.turn.saturating_sub(1);
            let source_predates_preceding_clue = |source: &EffectSource| match source {
                EffectSource::Event(turn) | EffectSource::Rule { turn, .. } => {
                    *turn < preceding_turn
                }
                EffectSource::Promise(promise) => self
                    .pending_connections
                    .provenance(*promise)
                    .is_some_and(|origin| origin.created_turn < preceding_turn),
            };
            let older_play_obligations = self
                .already_playing
                .iter()
                .chain(self.forced_playable.iter())
                .filter(|card| {
                    self.already_playing
                        .sources(**card)
                        .iter()
                        .chain(self.forced_playable.sources(**card))
                        .any(source_predates_preceding_clue)
                })
                .chain(
                    self.pending_connections
                        .iter()
                        .filter(|connection| {
                            self.pending_connections.is_active(connection)
                                && self
                                    .pending_connections
                                    .provenance(connection.promise)
                                    .is_some_and(|origin| origin.created_turn < preceding_turn)
                        })
                        .flat_map(|connection| connection.cards.iter()),
                )
                .copied()
                .collect();
            let before = HGroupTurnSnapshot {
                hands: self.hands.clone(),
                facts: self.facts.clone(),
                stack_heights: self.stack_heights,
                clue_tokens: self.historical_clue_tokens,
                deck_size: self.historical_deck_size,
                early_game: self.early_game,
                already_playing: self.already_playing.materialized().clone(),
                forced_playable: self.forced_playable.materialized().clone(),
                older_play_obligations,
            };
            let clue_tokens_before = before.clue_tokens;
            let mut frame = ReplayEvent {
                event_connection_transition_start,
                view,
                profile,
                perspective_depth,
                allow_blind_reverse_empathy,
                entry_index,
                entry,
                historical,
                before: &before,
                clue_tokens_before,
                action_is_settled,
                outcome: ReplayEventOutcome {
                    actor_saw_normal_discard,
                    actor_known_discard_identity,
                    declined_with_clue,
                    required_clue_deferral,
                },
            };
            match &entry.event {
                ObservedEvent::Clued { .. } => self.reduce_clue(&mut frame),
                ObservedEvent::Played { .. } => self.reduce_play(&mut frame),
                ObservedEvent::Discarded { .. } => self.reduce_discard(&mut frame),
                ObservedEvent::Drew { .. } => self.reduce_draw(&mut frame),
            }
            actor_saw_normal_discard = frame.outcome.actor_saw_normal_discard;
            actor_known_discard_identity = frame.outcome.actor_known_discard_identity;
            declined_with_clue = frame.outcome.declined_with_clue;
            required_clue_deferral = frame.outcome.required_clue_deferral;

            let context = HGroupTurnContext {
                entry,
                historical,
                before,
                after: HGroupTurnView {
                    hands: &self.hands,
                    facts: &self.facts,
                    stack_heights: self.stack_heights,
                    clue_tokens: self.historical_clue_tokens,
                    deck_size: self.historical_deck_size,
                    early_game: self.early_game,
                },
                actor_before: ActorBeliefBefore {
                    normal_chop_discard: actor_saw_normal_discard,
                    discarded_identity: actor_known_discard_identity,
                },
            };
            debug_assert_eq!(context.after.clue_tokens, self.historical_clue_tokens);
            let mut effects = HGroupRuleEffects {
                explicitly_clued: &self.explicitly_clued,
                invisibly_clued: &mut self.invisibly_clued,
                clues: &self.clues,
                already_playing: &mut self.already_playing,
                pending: &mut self.pending_connections,
                chop_moved: &mut self.chop_moved,
                must_clue: &mut self.must_clue,
                forced_playable: &mut self.forced_playable,
                discard_now: &mut self.discard_now,
                implicit_saves: &mut self.implicit_saves,
                required_fixes: &mut self.required_fixes,
                signals: &mut self.signals,
            };
            let execution = RuleExecutionContext::new(&context, view, profile);
            let mut transition = apply_post_event_rules(&execution, &mut effects);
            if action_is_settled {
                if let Some(actor) = declined_with_clue {
                    let recognized_deferral =
                        clue_permits_direct_play_deferral(&self.signals, entry.turn);
                    if !required_clue_deferral && !recognized_deferral {
                        // A direct Play interpretation is falsified when its owner
                        // voluntarily clues instead. Connections are excluded because
                        // their focus can wait for an intervening Prompt/Finesse. A
                        // recognized or required Fix is a convention-mandated
                        // deferral rather than evidence against the prior promise.
                        record_declined_direct_plays(
                            actor,
                            None,
                            &self.hands,
                            &self.clues,
                            &self.already_playing,
                            &self.pending_connections,
                            &mut DirectPlayDeclines {
                                cards: &mut self.declined_direct_plays,
                                turns: &mut self.declined_direct_play_turns,
                            },
                        );
                    }
                }
            }
            reconcile_connection_fact_lifecycles(
                &self.pending_connections,
                event_connection_transition_start,
                &mut self.invisibly_clued,
                &mut self.already_playing,
                &mut self.forced_playable,
            );
            self.explicitly_clued.reconcile_mask(
                event_card_snapshot.explicitly_clued,
                EffectSource::Event(entry.turn),
            );
            self.invisibly_clued.reconcile_mask(
                event_card_snapshot.invisibly_clued,
                EffectSource::Event(entry.turn),
            );
            self.already_playing.reconcile_mask(
                event_card_snapshot.already_playing,
                EffectSource::Event(entry.turn),
            );
            self.chop_moved.reconcile_mask(
                event_card_snapshot.chop_moved,
                EffectSource::Event(entry.turn),
            );
            self.forced_playable.reconcile_mask(
                event_card_snapshot.forced_playable,
                EffectSource::Event(entry.turn),
            );
            let event_after = ConventionCardSetSnapshot::capture(
                &self.explicitly_clued,
                &self.invisibly_clued,
                &self.already_playing,
                &self.chop_moved,
                &self.forced_playable,
            );
            transition.delta = ConventionTransitionDelta {
                card_changes: event_card_snapshot.changes_to(&event_after),
                knowledge_changes: Vec::new(),
            };
            if !transition.proposals.is_empty() || !transition.delta.is_empty() {
                self.transitions.push(transition);
            }
        }
        if rule_enabled(profile, HGroupRuleId::SpecialDiscards) {
            for pending in self.pending_connections.iter().filter(|pending| {
                pending.actor == view.observer
                    && pending.kind == HGroupConnectionKind::Finesse
                    && self.pending_connections.is_active(pending)
            }) {
                let Some(blind) = pending.cards.first() else {
                    continue;
                };
                let duplicated_in_own_hand = view.hands[view.observer.index()].iter().any(|card| {
                    card.id != *blind
                        && self.explicitly_clued.contains(&card.id)
                        && card.clues.allows(pending.expected)
                });
                let bluff = self.signals.iter().any(|signal| {
                    signal.kind == HGroupMoveKind::Bluff && signal.cards.contains(blind)
                });
                if duplicated_in_own_hand && !bluff && !self.discard_now.contains(blind) {
                    self.discard_now.push(*blind);
                }
            }
        }
        // A discard instead of a direct play is only a one-round hesitation: if
        // no teammate demonstrates a connection before the owner's next turn,
        // the direct interpretation becomes actionable again. A clue instead of
        // playing is an intentional deferral and therefore has no timestamp here;
        // it remains declined until a later clue retouches the card.
        self.declined_direct_plays.retain(|card| {
            self.declined_direct_play_turns
                .iter()
                .find_map(|(declined, turn)| (declined == card).then_some(*turn))
                .is_none_or(|turn| {
                    view.turn
                        < turn.saturating_add(
                            u32::try_from(self.hands.len())
                                .expect("standard Hanabi has at most five players"),
                        )
                })
        });

        self.finish(deductions)
    }

    fn finish(self, deductions: &LogicalDeductions) -> HGroupState {
        let Self {
            hands,
            explicitly_clued,
            invisibly_clued,
            clues,
            pending_connections,
            already_playing,
            early_game,
            signals,
            chop_moved,
            discard_now,
            must_clue,
            forced_playable,
            invalidated_focuses,
            declined_direct_plays,
            implicit_saves,
            required_fixes,
            transitions,
            ..
        } = self;
        let (signals, convention_facts) = signals.into_parts();
        let mut state = HGroupState {
            hands,
            cards: ConventionCardState {
                explicitly_clued,
                invisibly_clued,
                already_playing,
                chop_moved,
                discard_now,
                forced_playable,
                invalidated_focuses,
                declined_direct_plays,
                facts: convention_facts,
            },
            clues,
            pending_connections,
            early_game,
            signals,
            must_clue,
            implicit_saves,
            required_fixes,
            transitions,
            knowledge: ConventionKnowledge::default(),
        };
        state.knowledge = build_convention_knowledge(deductions, &state);
        state
            .knowledge
            .attach_to_transitions(&mut state.transitions);
        debug_assert!(
            state.validate().is_ok(),
            "invalid H-Group replay state: {:?}",
            state.validate()
        );
        state
    }

    #[allow(clippy::too_many_lines)]
    fn reduce_clue(&mut self, frame: &mut ReplayEvent<'_>) {
        let event_connection_transition_start = frame.event_connection_transition_start;
        let view = frame.view;
        let profile = frame.profile;
        let allow_blind_reverse_empathy = frame.allow_blind_reverse_empathy;
        let entry = frame.entry;
        let historical = frame.historical;
        let before = frame.before;
        let clue_tokens_before = frame.clue_tokens_before;
        let ObservedEvent::Clued {
            giver,
            target,
            clue,
            touched,
            untouched,
        } = &frame.entry.event
        else {
            unreachable!("event dispatcher selects its handler")
        };

        for card in touched {
            self.declined_direct_plays.remove(card);
            self.declined_direct_play_turns
                .retain(|(declined, _)| declined != card);
        }
        let promised_card_fix = self.pending_connections.iter().any(|connection| {
            connection.actor == *target
                && self.pending_connections.is_active(connection)
                && !clue.matches(connection.expected)
                && connection
                    .cards
                    .first()
                    .is_some_and(|card| touched.contains(card))
        });
        let signaled_card_fix = touched.iter().any(|card| {
            let has_active_promise = self.invisibly_clued.contains(card)
                || self
                    .pending_connections
                    .iter()
                    .any(|connection| connection.cards.first() == Some(card));
            has_active_promise
                && !self.signals.facts().fixed_cards().contains(card)
                && self
                    .signals
                    .facts()
                    .identity_claims()
                    .iter()
                    .rev()
                    .any(|claim| {
                        claim.turn < entry.turn
                            && claim.target == Some(*target)
                            && claim.cards.first() == Some(card)
                            && !clue.matches(claim.identity)
                            && matches!(
                                claim.source,
                                HGroupMoveKind::Finesse
                                    | HGroupMoveKind::ReverseFinesse
                                    | HGroupMoveKind::SelfFinesse
                                    | HGroupMoveKind::LayeredFinesse
                                    | HGroupMoveKind::ClandestineFinesse
                                    | HGroupMoveKind::QueuedFinesse
                                    | HGroupMoveKind::AmbiguousFinesse
                            )
                    })
        });
        let pre_clue_active_invisible =
            active_invisibly_clued(&self.invisibly_clued, &self.pending_connections);
        let pre_clue_gotten = protected_cards(
            &self.explicitly_clued,
            &pre_clue_active_invisible,
            &self.chop_moved,
        );
        let hypothetical_connection_fix = self.clues.iter().rev().any(|prior| {
            if !self.pending_connections.actor_had_pending_before(
                prior.target,
                prior.turn,
                prior.focus,
            ) {
                return false;
            }
            prior.play_identities.iter().any(|identity| {
                let first_connector = Card::new(
                    identity.suit,
                    Rank::ALL[usize::from(prior.stack_heights[identity.suit.index()])],
                );
                if matches!(prior.clue, Clue::Rank(_))
                    && !self
                        .pending_connections
                        .identity_was_demonstrated_after(first_connector, prior.turn)
                {
                    // A rank clue's delayed branch is not established
                    // merely by being structurally possible. A later
                    // clue fixes that branch only after its first
                    // connector publicly demonstrates it. Loaded color
                    // clues retain the immediate lie-component Fix
                    // exception.
                    return false;
                }
                loaded_connection_plan(
                    view,
                    Some(&self.hands),
                    Some(&self.facts),
                    Some(HistoricalView::new(view, prior.turn)),
                    prior.giver,
                    prior.target,
                    prior.focus,
                    identity,
                    &pre_clue_gotten,
                    &self.already_playing,
                    &self.pending_connections,
                    self.stack_heights,
                )
                .flatten()
                .is_some_and(|required| {
                    required.actor == *giver
                        && required.target == *target
                        && touched.contains(&required.focus)
                        && clue.matches(required.identity)
                })
            })
        });
        let is_required_fix = promised_card_fix
            || signaled_card_fix
            || hypothetical_connection_fix
            || self.required_fixes.iter().any(|obligation| {
                let required = obligation.required;
                required.actor == *giver
                    && required.target == *target
                    && touched.contains(&required.focus)
                    && clue.matches(required.identity)
                    && !was_clued_before_with(view, entry.turn, required.focus, *clue)
            });
        frame.outcome.declined_with_clue = Some(*giver);
        frame.outcome.required_clue_deferral = is_required_fix;
        if is_required_fix {
            // https://hanabi.github.io/level-3/#the-fix-clue
            // Promise repair is decided and applied here, before the
            // per-level recognizers run. Journal the same transition
            // at its canonical mutation point so current facts do not
            // mistake the physical clue for a Play/Tempo promise.
            push_signal(
                &mut self.signals,
                entry,
                *giver,
                Some(*target),
                HGroupMoveKind::FixClue,
                touched.clone(),
                None,
            );
            let fixed_cards = touched.iter().copied().collect::<CardSet>();
            let mut occupied = protected_cards(
                &self.explicitly_clued,
                &self.invisibly_clued,
                &self.chop_moved,
            );
            occupied.extend(
                self.pending_connections
                    .iter()
                    .flat_map(|connection| connection.cards.iter().copied()),
            );
            self.pending_connections.repair_actor(
                entry.turn,
                *target,
                |card| fixed_cards.contains(&card),
                |_| {
                    let next = self.hands[target.index()]
                        .iter()
                        .rev()
                        .copied()
                        .find(|card| !fixed_cards.contains(card) && !occupied.contains(card));
                    if let Some(next) = next {
                        occupied.insert(next);
                        self.invisibly_clued.insert(next);
                    }
                    next
                },
            );
            for fixed in fixed_cards {
                self.invisibly_clued.remove(&fixed);
            }
        }
        let active_invisible =
            active_invisibly_clued(&self.invisibly_clued, &self.pending_connections);
        let gotten = protected_cards(&self.explicitly_clued, &active_invisible, &self.chop_moved);
        let hand = &self.hands[target.index()];
        let old_chop = chop(hand, &gotten);
        let newly_touched = touched
            .iter()
            .copied()
            .filter(|card| !gotten.contains(card))
            .collect::<Vec<_>>();
        let previously_promptable = self
            .explicitly_clued
            .union(&active_invisible)
            .copied()
            .collect::<CardSet>();
        let displaced_connections = self
            .pending_connections
            .iter()
            .filter(|connection| connection.actor == *giver)
            .filter(|connection| {
                touched
                    .iter()
                    .any(|card| historical.identity(*card) == Some(connection.expected))
            })
            .flat_map(|connection| connection.cards.iter().copied())
            .collect::<CardSet>();
        if !displaced_connections.is_empty() {
            self.pending_connections.cancel_where(
                entry.turn,
                ConnectionTransitionReason::DisplacedByClue,
                |connection| {
                    connection.actor == *giver
                        && touched
                            .iter()
                            .any(|card| historical.identity(*card) == Some(connection.expected))
                },
            );
            for displaced in displaced_connections {
                self.already_playing.remove(&displaced);
                if !self.explicitly_clued.contains(&displaced)
                    && !self
                        .pending_connections
                        .iter()
                        .any(|connection| connection.cards.contains(&displaced))
                {
                    self.invisibly_clued.remove(&displaced);
                }
            }
        }
        if let Some(focus) = focus(hand, touched, old_chop, &gotten) {
            let focus_identity = historical.identity(focus);
            // Save meaning depends on the actual pre-clue chop, not
            // another card that could be a Positional Discard.
            // https://hanabi.github.io/level-1/#the-5-save
            let focus_was_chop = old_chop == Some(focus);
            for card in touched {
                self.facts[card.index()].add_positive_clue(*clue);
            }
            for card in untouched {
                self.facts[card.index()].add_negative_clue(*clue);
            }
            self.explicitly_clued.extend(touched.iter().copied());
            let raw_focus_identities = focus_identity.map_or_else(
                || IdentitySet::from_mask(self.facts[focus.index()].identity_mask()),
                IdentitySet::singleton,
            );
            let mut focus_identities = raw_focus_identities;
            let mut claimed_identities = IdentitySet::default();
            if focus_identity.is_none() {
                // Good Touch lets a recipient eliminate identities
                // already promised on live cards elsewhere. Apply the
                // elimination to the whole focus domain, including
                // Save possibilities: a newly touched 2 beside an
                // existing saved Red 2 cannot itself be Red 2.
                claimed_identities = claimed_identities_at_clue(
                    focus,
                    &self.hands,
                    &historical,
                    &self.facts,
                    self.signals.facts(),
                    &self.clues,
                    &gotten,
                    &self.pending_connections,
                );
                focus_identities = focus_identities.without(claimed_identities);
            }
            let active_connector_identity = self
                .pending_connections
                .iter()
                .find(|connection| {
                    connection.actor == *target
                        && connection.cards.first() == Some(&focus)
                        && clue.matches(connection.expected)
                        && self.pending_connections.is_active(connection)
                })
                .map(|connection| connection.expected);
            let mut play_identities = active_connector_identity.map_or_else(
                || {
                    snapshot_play_identities(
                        profile,
                        focus_identities,
                        *giver,
                        *target,
                        focus,
                        view,
                        &self.hands,
                        &self.facts,
                        &previously_promptable,
                        &self.already_playing,
                        &self.pending_connections,
                        self.signals.facts(),
                        &self.chop_moved,
                        self.stack_heights,
                        entry.turn,
                        allow_blind_reverse_empathy,
                    )
                },
                IdentitySet::singleton,
            );
            if focus_identity.is_none()
                && *clue == Clue::Rank(Rank::Two)
                && !claimed_identities.is_empty()
            {
                // Good Touch rejects a duplicated direct play, but it
                // must not erase an independently valid delayed
                // connection before the recipient can retain that
                // branch in superposition. For example, a rank-2 clue
                // can still mean green 2 through a visible green-1
                // Reverse Finesse even when another touched card is
                // provisionally claimed as green 2. Candidate
                // validation decides whether the overall clue is a
                // useful duplication; rank-2 identity compilation must
                // first preserve every convention-readable branch.
                let delayed_claimed = IdentitySet::from_mask(
                    raw_focus_identities
                        .intersection(claimed_identities)
                        .iter()
                        .filter(|identity| {
                            identity.rank.number() > self.stack_heights[identity.suit.index()] + 1
                        })
                        .fold(0, |mask, identity| mask | (1 << identity.index())),
                );
                let delayed_claimed_plays = snapshot_play_identities(
                    profile,
                    delayed_claimed,
                    *giver,
                    *target,
                    focus,
                    view,
                    &self.hands,
                    &self.facts,
                    &previously_promptable,
                    &self.already_playing,
                    &self.pending_connections,
                    self.signals.facts(),
                    &self.chop_moved,
                    self.stack_heights,
                    entry.turn,
                    allow_blind_reverse_empathy,
                );
                focus_identities = focus_identities.union(delayed_claimed_plays);
                play_identities = play_identities.union(delayed_claimed_plays);
            }
            // Speculation and commit must both read the same pre-clue
            // convention snapshot. Pushing the new Play signal may
            // reactivate a previously fixed card; allowing commit to
            // observe that mutation made it manufacture a loaded
            // connection and Fix that simulation never proposed.
            let convention_facts_before_clue = self.signals.facts().clone();
            let connection_context = ConnectionPlanningContext {
                profile,
                view,
                turn: entry.turn,
                giver: *giver,
                target: *target,
                focus,
                clue: *clue,
                touches: CurrentClueTouches(touched),
                hands: &self.hands,
                facts: &self.facts,
                clues: &self.clues,
                promptable_before: PromptableBeforeClue(&previously_promptable),
                protected_before: &gotten,
                already_playing: &self.already_playing,
                declined_direct_plays: &self.declined_direct_plays,
                convention_facts: &convention_facts_before_clue,
                chop_moved: &self.chop_moved,
                stack_heights: self.stack_heights,
                allow_blind_reverse_empathy,
            };
            // The snapshot shortcut handles one missing connector.
            // An Elimination Finesse can also start a longer sequence:
            // prove that sequence with the same planner that will
            // materialize its obligations, not a special clue score.
            // https://hanabi.github.io/level-18/#the-elimination-finesse
            for identity in focus_identities.without(play_identities).iter() {
                let height = self.stack_heights[identity.suit.index()];
                if identity.rank.number() <= height + 2
                    || !rule_enabled(profile, HGroupRuleId::Elimination)
                    || !convention_facts_before_clue
                        .identity_claims()
                        .iter()
                        .any(|claim| {
                            claim.source == HGroupMoveKind::Elimination
                                && claim.identity.suit == identity.suit
                                && claim.identity.rank.number() > height
                                && claim.identity.rank.number() < identity.rank.number()
                        })
                {
                    continue;
                }
                let hypothesis = connection_context.simulate(
                    identity,
                    &self.pending_connections,
                    &self.invisibly_clued,
                );
                let complete = ((height + 1)..identity.rank.number()).all(|rank| {
                    let expected = Card::new(identity.suit, Rank::ALL[usize::from(rank - 1)]);
                    self.pending_connections.identity_is_queued(expected)
                        || interpretation::snapshot_accounted(
                            expected,
                            focus,
                            view,
                            &self.hands,
                            &self.facts,
                            &previously_promptable,
                        )
                        || hypothesis
                            .connection_steps
                            .iter()
                            .any(|step| step.expected == expected)
                });
                // Visible connectors must match. The blind reactor,
                // however, learns their own connection from the clue:
                // requiring prior identity knowledge would prevent an
                // Elimination Finesse from ever reaching its owner.
                // Only accept the ordered Finesse slots selected by
                // the shared planner, with literal/count feasibility.
                let evidenced = hypothesis.connection_steps.iter().all(|step| {
                    step.cards.last().is_some_and(|card| {
                        historical.identity(*card) == Some(step.expected)
                            || self.facts[card.index()].identity_mask()
                                == 1 << step.expected.index()
                            || convention_facts_before_clue.known_identity(*card)
                                == Some(step.expected)
                            || (step.actor == view.observer
                                && *giver != view.observer
                                && step.kind == HGroupConnectionKind::Finesse
                                && step.cards.iter().all(|candidate| {
                                    self.facts[candidate.index()].allows(step.expected)
                                })
                                && historical.has_unseen_copy(step.expected, &self.hands))
                    })
                });
                let uses_elimination = hypothesis.connection_steps.iter().any(|step| {
                    rule_enabled(profile, HGroupRuleId::Elimination)
                        && elimination_finesse_card(
                            step.actor,
                            &self.hands[step.actor.index()],
                            focus,
                            step.expected,
                            &convention_facts_before_clue,
                            &self.chop_moved,
                            |card| self.facts[card.index()].allows(step.expected),
                        )
                        .is_some_and(|card| step.cards == [card])
                });
                if complete && evidenced && uses_elimination {
                    play_identities = play_identities.union(IdentitySet::singleton(identity));
                }
            }
            let mut intermediate_bluff = false;
            if rule_enabled(profile, HGroupRuleId::IntermediateBluffs)
                && *clue == Clue::Rank(Rank::Three)
                && focus_identities
                    .iter()
                    .all(|identity| !is_playable_at(self.stack_heights, identity))
            {
                let actor = next_player(*giver, self.hands.len());
                let bluff_card = finesse_position_id(&self.hands[actor.index()], &gotten, 0);
                let bluff_is_credible = bluff_card.is_some_and(|card| {
                    historical
                        .identity(card)
                        .map_or(actor == view.observer, |identity| {
                            is_playable_at(self.stack_heights, identity)
                                && !bluff_play_connects(*clue, identity)
                        })
                });
                if bluff_target_order_is_legal(*clue, actor, *target) && bluff_is_credible {
                    let three_bluff_targets = IdentitySet::from_mask(
                        focus_identities
                            .iter()
                            .filter(|identity| {
                                bluff_target_kind_at(self.stack_heights, *clue, *identity)
                                    == Some(BluffTargetKind::Three)
                            })
                            .fold(0, |mask, identity| mask | (1 << identity.index())),
                    );
                    intermediate_bluff = !three_bluff_targets.is_empty();
                    play_identities = play_identities.union(three_bluff_targets);
                }
            }
            let eight_clue_save_position = rule_enabled(profile, HGroupRuleId::Stalling)
                && !self.early_game
                && clue_tokens_before == MAX_CLUE_TOKENS
                && !gotten.contains(&focus)
                && self.hands[target.index()].last() != Some(&focus);
            // Level 9 changes where a Save may be given, not what
            // needs saving. Resolve secured identities from this
            // event's information, never from later draws/reveals.
            // https://hanabi.github.io/level-9/#the-8-clue-save-8cs
            let eight_save_identities = if eight_clue_save_position {
                IdentitySet::from_mask(
                    focus_identities
                        .iter()
                        .filter(|identity| {
                            identity.rank.number() > self.stack_heights[identity.suit.index()]
                                && !gotten.iter().copied().any(|other| {
                                    if other == focus {
                                        return false;
                                    }
                                    historical.identity(other) == Some(*identity)
                                        || primary::protected_identity_from_clues(
                                            other,
                                            before.facts[other.index()],
                                            self.stack_heights,
                                            self.clues.iter(),
                                        ) == Some(*identity)
                                })
                        })
                        .fold(0, |mask, identity| mask | (1 << identity.index())),
                )
            } else {
                IdentitySet::default()
            };
            let eight_clue_save = !eight_save_identities.is_empty();
            let save_identities = snapshot_save_identities(
                if eight_clue_save_position {
                    eight_save_identities
                } else {
                    focus_identities
                },
                *clue,
                *giver,
                focus,
                focus_was_chop,
                eight_clue_save,
                view,
                &self.hands,
                &gotten,
                play_identities,
                self.stack_heights,
                self.public_removed,
            );
            let score = self
                .stack_heights
                .iter()
                .map(|height| usize::from(*height))
                .sum::<usize>();
            let low_score_number_five = rule_enabled(profile, HGroupRuleId::FiveTech)
                        && *clue == Clue::Rank(Rank::Five)
                        // Level 19 turns off Play Clues, not ordinary 5 Saves.
                        // https://hanabi.github.io/level-19/#no-play-clues-with-a-number-5-clue-in-the-low-score-phase
                        && save_identities.is_empty()
                        && score < 2 * Suit::ALL.len();
            let early_five_stall = rule_enabled(profile, HGroupRuleId::BasicMoves)
                && self.early_game
                && *clue == Clue::Rank(Rank::Five)
                && !focus_was_chop;
            let eight_clue_five_stall = rule_enabled(profile, HGroupRuleId::Stalling)
                        && !self.early_game
                        && clue_tokens_before == MAX_CLUE_TOKENS
                        && *clue == Clue::Rank(Rank::Five)
                        && !focus_was_chop
                        && !eight_clue_save
                        // Eight tokens allow a last-resort Stall; they do
                        // not erase a valid Play Clue on an off-chop five.
                        // https://hanabi.github.io/level-9/#5-stalls-are-a-last-resort
                        && play_identities.is_empty();
            let five_chop_move = rule_enabled(profile, HGroupRuleId::ChopMoves)
                && *clue == Clue::Rank(Rank::Five)
                && !early_five_stall
                && !eight_clue_five_stall
                && five_chop_moved_card(&self.hands[target.index()], touched, &gotten).is_some();
            let no_information_reclue = touched
                .iter()
                .all(|card| was_clued_before_with(view, entry.turn, *card, *clue));
            // Reconfirming an existing play is a Burn/Fill-In, not a
            // fresh Play or Tempo promise. Preserve the older promise.
            // https://hanabi.github.io/level-6/#the-tempo-clue
            let already_playing_reclue = touched.iter().all(|card| {
                previously_promptable.contains(card)
                    && (self.already_playing.contains(card)
                        || primary::prior_play_is_ready(
                            *card,
                            before.facts[card.index()],
                            self.stack_heights,
                            self.clues.iter(),
                        )
                        || convention_facts_before_clue
                            .known_identity(*card)
                            .is_some_and(|identity| is_playable_at(self.stack_heights, identity)))
            });
            let interpretation_plan = ClueInterpretationPlan::resolve(PrimaryClueInputs {
                clue: *clue,
                play_identities,
                save_identities,
                stack_heights: self.stack_heights,
                eight_clue_save,
                suppressions: [
                    is_required_fix.then_some(primary::PrimarySuppression::Fix),
                    five_chop_move.then_some(primary::PrimarySuppression::FiveChopMove),
                    low_score_number_five.then_some(primary::PrimarySuppression::LowScoreFive),
                    early_five_stall.then_some(primary::PrimarySuppression::EarlyFiveStall),
                    eight_clue_five_stall
                        .then_some(primary::PrimarySuppression::EightClueFiveStall),
                    no_information_reclue
                        .then_some(primary::PrimarySuppression::NoInformationReclue),
                    already_playing_reclue
                        .then_some(primary::PrimarySuppression::AlreadyPlayingReclue),
                ],
            });
            let kind = interpretation_plan.kind;
            // An observer can recognize a visible Finesse prefix
            // without knowing the hidden continuation. Keep that
            // evidence separate from committed card identities.
            // https://hanabi.github.io/level-5/#the-ambiguous-finesse
            let unresolved_visible_prefix = if kind == HGroupClueKind::Unrecognized
                && interpretation_plan.suppression.is_none()
                && matches!(clue, Clue::Suit(_))
                && rule_enabled(profile, HGroupRuleId::BasicMoves)
            {
                historical
                    .identity(focus)
                    .and_then(|identity| {
                        let height = self.stack_heights[identity.suit.index()];
                        if identity.rank.number() <= height + 2 {
                            return None;
                        }
                        let context = ConnectionPlanningContext {
                            allow_blind_reverse_empathy: true,
                            ..connection_context
                        };
                        let plan = context.simulate(
                            identity,
                            &self.pending_connections,
                            &self.invisibly_clued,
                        );
                        let complete = ((height + 1)..identity.rank.number()).all(|rank| {
                            let expected =
                                Card::new(identity.suit, Rank::ALL[usize::from(rank - 1)]);
                            self.pending_connections.identity_is_queued(expected)
                                || interpretation::snapshot_accounted(
                                    expected,
                                    focus,
                                    view,
                                    &self.hands,
                                    &self.facts,
                                    &previously_promptable,
                                )
                                || plan
                                    .connection_steps
                                    .iter()
                                    .any(|step| step.expected == expected)
                        });
                        let feasible = plan.connection_steps.iter().all(|step| {
                            step.cards.last().is_some_and(|card| {
                                historical.identity(*card) == Some(step.expected)
                                    || (step.actor == view.observer
                                        && step.cards.iter().all(|candidate| {
                                            self.facts[candidate.index()].allows(step.expected)
                                        })
                                        && historical.has_unseen_copy(step.expected, &self.hands))
                            })
                        });
                        (complete && feasible && plan.required_fix.is_none()).then(|| {
                            plan.connection_steps
                                .into_iter()
                                .take_while(|step| {
                                    step.kind == HGroupConnectionKind::Finesse
                                        && step.cards.last().is_some_and(|card| {
                                            historical.identity(*card) == Some(step.expected)
                                        })
                                })
                                .collect::<Vec<_>>()
                        })
                    })
                    .unwrap_or_default()
            } else {
                Vec::new()
            };
            play_identities = interpretation_plan.play_identities;
            let save_identities = interpretation_plan.save_identities;
            debug_assert_eq!(
                interpretation_plan.suppression.is_some(),
                matches!(kind, HGroupClueKind::Unrecognized)
                    && (is_required_fix
                        || five_chop_move
                        || low_score_number_five
                        || early_five_stall
                        || eight_clue_five_stall
                        || no_information_reclue
                        || already_playing_reclue)
            );
            let target_already_loaded = self.pending_connections.iter().any(|connection| {
                connection.actor == *target && self.pending_connections.is_active(connection)
            });
            let direct_play = IdentitySet::from_mask(
                play_identities
                    .iter()
                    .filter(|identity| {
                        is_playable_at(self.stack_heights, *identity)
                            && !self.pending_connections.identity_is_queued(*identity)
                    })
                    .fold(0, |mask, identity| mask | (1 << identity.index())),
            );
            let connection_hypotheses = play_identities
                .iter()
                .map(|identity| {
                    connection_context.simulate(
                        identity,
                        &self.pending_connections,
                        &self.invisibly_clued,
                    )
                })
                .collect::<Vec<_>>();
            // A rank-3 clue may superficially look like a Self 3
            // Bluff, but a complete two-card chain to the focus is a
            // Double Finesse instead. Bluff recognition is the
            // fallback only when the recipient cannot see that full
            // connecting line.
            // Sources:
            // - https://hanabi.github.io/level-2/#the-reverse-finesse
            // - https://hanabi.github.io/level-13/#the-3-bluff
            if intermediate_bluff
                && connection_hypotheses
                    .iter()
                    .any(|hypothesis| hypothesis.connection_steps.len() > 1)
            {
                intermediate_bluff = false;
            }
            for hypothesis in &connection_hypotheses {
                let Some(required) = hypothesis.required_fix else {
                    continue;
                };
                if focus_identity == Some(hypothesis.focus_identity) {
                    self.required_fixes.insert_unconditional(required);
                } else if focus_identity.is_none() {
                    self.required_fixes.insert_conditional(
                        entry.turn,
                        focus,
                        hypothesis.focus_identity,
                        required,
                    );
                }
            }
            let inferred_connection_identity = focus_identity
                .or_else(|| {
                    (play_identities.len() == 1)
                        .then(|| play_identities.iter().next())
                        .flatten()
                })
                .or_else(|| {
                    // The recipient can initially have both a direct
                    // and a delayed identity in their note. Preserve a
                    // unique delayed interpretation so intervening
                    // blind plays can demonstrate and resolve it.
                    let delayed = IdentitySet::from_mask(
                        play_identities
                            .iter()
                            .filter(|identity| {
                                identity.rank.number()
                                    > self.stack_heights[identity.suit.index()] + 1
                            })
                            .fold(0, |mask, identity| mask | (1 << identity.index())),
                    );
                    (delayed.len() == 1)
                        .then(|| delayed.iter().next())
                        .flatten()
                })
                .or_else(|| {
                    // A loaded clue can leave the recipient with
                    // several delayed identities. Prefer a clean line
                    // over one that first requires a Fix, then compare
                    // full executable line lengths, including playable
                    // layers before a connector. Rank alone cannot
                    // distinguish a plain Purple-2 Finesse from a
                    // Red-2 Clandestine Finesse through Purple 1.
                    rule_enabled(profile, HGroupRuleId::Extras)
                        .then(|| {
                            connection_hypotheses
                                .iter()
                                .filter(|hypothesis| hypothesis.loaded)
                                .max_by_key(|hypothesis| {
                                    let identity = hypothesis.focus_identity;
                                    let base = identity
                                        .rank
                                        .number()
                                        .saturating_sub(self.stack_heights[identity.suit.index()]);
                                    let layers = hypothesis
                                        .connection_steps
                                        .iter()
                                        .map(|connection| connection.cards.len().saturating_sub(1))
                                        .sum::<usize>();
                                    (
                                        hypothesis.required_fix.is_none(),
                                        usize::from(base) + layers,
                                        core::cmp::Reverse(identity.index()),
                                    )
                                })
                                .map(|hypothesis| hypothesis.focus_identity)
                        })
                        .flatten()
                });
            let connection_identity = if focus_identity.is_none()
                && target_already_loaded
                && !direct_play.is_empty()
                && !inferred_connection_identity.is_some_and(|identity| {
                    connection_hypotheses.iter().any(|hypothesis| {
                        hypothesis.focus_identity == identity
                            && hypothesis.loaded
                            && hypothesis.required_fix.is_some()
                    })
                }) {
                // A new direct Play Clue to a loaded player gives them
                // another explicit play. It does not manufacture a
                // second speculative finesse from delayed identities
                // that happen to match the same rank/color clue.
                None
            } else {
                inferred_connection_identity
            };
            let focus_identities = if focus_was_chop {
                play_identities.union(save_identities)
            } else {
                play_identities
            };
            let new_non_focus = newly_touched
                .iter()
                .copied()
                .filter(|card| *card != focus)
                .collect::<Vec<_>>();
            let non_focus_identities = new_non_focus
                .iter()
                .copied()
                .map(|card| {
                    let direct = historical.identity(card).map_or_else(
                        || IdentitySet::from_mask(self.facts[card.index()].identity_mask()),
                        IdentitySet::singleton,
                    );
                    let good_touch = snapshot_good_touch_identities(
                        card,
                        direct,
                        view,
                        &self.hands,
                        &previously_promptable,
                        self.stack_heights,
                        self.public_removed,
                    );
                    // Good Touch forbids an actual duplicate, but an
                    // ambiguous focus does not claim every identity in
                    // its domain. For a two-card rank-1 clue, the
                    // non-focus card is still safe as the first play;
                    // subtracting the focus's entire mask erased that
                    // promise. Larger duplicate-rank clues retain
                    // focused-play semantics: representing all their
                    // pairwise distinctness requires correlated belief
                    // branches rather than independent card masks.
                    let claimed_focus = if focus_identities.len() == 1 || new_non_focus.len() != 1 {
                        focus_identities
                    } else {
                        IdentitySet::default()
                    };
                    let good_touch = good_touch.without(claimed_focus);
                    (card, good_touch)
                })
                .collect::<Vec<_>>();
            let completing_suit = (matches!(kind, HGroupClueKind::Play)
                && play_identities.len() == 1)
                .then(|| play_identities.iter().next())
                .flatten()
                .filter(|identity| {
                    *clue == Clue::Suit(identity.suit)
                        && identity.rank == Rank::Five
                        && is_playable_at(self.stack_heights, *identity)
                });
            let non_focus_trash_identities = completing_suit.map_or_else(Vec::new, |_| {
                non_focus_identities
                    .iter()
                    .filter_map(|(card, good_touch)| {
                        let direct = historical.identity(*card).map_or_else(
                            || IdentitySet::from_mask(self.facts[card.index()].identity_mask()),
                            IdentitySet::singleton,
                        );
                        let trash = direct.without(focus_identities).without(*good_touch);
                        (!trash.is_empty()).then_some((*card, trash))
                    })
                    .collect()
            });
            self.clues.push(HGroupClueInterpretation {
                turn: entry.turn,
                giver: *giver,
                target: *target,
                clue: *clue,
                touched: touched.clone(),
                stack_heights: self.stack_heights,
                focus,
                focus_was_chop,
                kind,
                focus_identities,
                play_identities,
                save_identities,
                new_non_focus,
                non_focus_identities,
                non_focus_trash_identities,
                // Prompt candidates need actual clue information.
                // A chop-moved card is protected for chop/layout
                // purposes, but remains an unknown card and cannot be
                // Prompted merely because it was moved.
                previously_gotten: previously_promptable.iter().copied().collect(),
                hypotheses: connection_hypotheses,
                unresolved_visible_prefix,
            });
            let current_clue = self
                .clues
                .last()
                .expect("the current clue was just appended")
                .clone();
            let signal_kind = match kind {
                HGroupClueKind::Play | HGroupClueKind::PlayOrSave => Some(HGroupMoveKind::PlayClue),
                HGroupClueKind::Save(_) => Some(HGroupMoveKind::SaveClue),
                HGroupClueKind::Unrecognized => None,
            };
            if let Some(signal_kind) = signal_kind {
                let signal_identity = if signal_kind == HGroupMoveKind::PlayClue {
                    connection_identity.or(focus_identity)
                } else {
                    focus_identity
                };
                push_signal(
                    &mut self.signals,
                    entry,
                    *giver,
                    Some(*target),
                    signal_kind,
                    vec![focus],
                    signal_identity,
                );
            }
            if matches!(kind, HGroupClueKind::Play) && !low_score_number_five && !intermediate_bluff
            {
                let previous_connections =
                    self.pending_connections.iter().cloned().collect::<Vec<_>>();
                let committed_plan = ConnectionPlanningContext {
                    profile,
                    view,
                    turn: entry.turn,
                    giver: *giver,
                    target: *target,
                    focus,
                    clue: *clue,
                    touches: CurrentClueTouches(touched),
                    hands: &self.hands,
                    facts: &self.facts,
                    clues: &self.clues,
                    promptable_before: PromptableBeforeClue(&previously_promptable),
                    protected_before: &gotten,
                    already_playing: &self.already_playing,
                    declined_direct_plays: &self.declined_direct_plays,
                    convention_facts: self.signals.facts(),
                    chop_moved: &self.chop_moved,
                    stack_heights: self.stack_heights,
                    allow_blind_reverse_empathy,
                };
                let (new_connections, _recomputed_fix) = committed_plan.commit(
                    connection_identity,
                    &mut self.pending_connections,
                    &mut self.invisibly_clued,
                );
                // The hypothesis is the canonical speculative result.
                // Commit materializes its connection graph, but must
                // not independently invent a repair obligation after
                // clue-state mutations have occurred.
                let scheduled_fix = connection_identity.and_then(|identity| {
                    current_clue
                        .hypotheses
                        .iter()
                        .find(|hypothesis| hypothesis.focus_identity == identity)
                        .and_then(|hypothesis| hypothesis.required_fix)
                });
                if let (Some(required), Some(identity)) = (scheduled_fix, connection_identity) {
                    if focus_identity.is_some() {
                        self.required_fixes.insert_unconditional(required);
                    } else {
                        self.required_fixes
                            .insert_conditional(entry.turn, focus, identity, required);
                    }
                }
                reconcile_connection_fact_lifecycles(
                    &self.pending_connections,
                    event_connection_transition_start,
                    &mut self.invisibly_clued,
                    &mut self.already_playing,
                    &mut self.forced_playable,
                );
                for connection in &new_connections {
                    let elimination_finesse =
                        self.signals.facts().identity_claims().iter().any(|claim| {
                            claim.source == HGroupMoveKind::Elimination
                                && claim.target == Some(connection.actor)
                                && claim.identity == connection.expected
                                && connection
                                    .cards
                                    .first()
                                    .is_some_and(|card| claim.cards.contains(card))
                        });
                    push_signal(
                        &mut self.signals,
                        entry,
                        *giver,
                        Some(connection.actor),
                        if elimination_finesse {
                            HGroupMoveKind::EliminationFinesse
                        } else if connection.kind == HGroupConnectionKind::Finesse
                            && connection.cards.len() > 1
                        {
                            HGroupMoveKind::LayeredFinesse
                        } else {
                            match connection.kind {
                                HGroupConnectionKind::Prompt => HGroupMoveKind::Prompt,
                                HGroupConnectionKind::Finesse => HGroupMoveKind::Finesse,
                            }
                        },
                        connection.cards.clone(),
                        Some(connection.expected),
                    );

                    let player_count = self.hands.len();
                    let target_distance =
                        (target.index() + player_count - giver.index()) % player_count;
                    let actor_distance =
                        (connection.actor.index() + player_count - giver.index()) % player_count;
                    if connection.kind == HGroupConnectionKind::Finesse
                        && actor_distance > target_distance
                    {
                        // The executable graph is also the source of
                        // the named Level-2 interpretation. Deriving
                        // Reverse Finesse independently from the
                        // focus identity failed whenever that identity
                        // was hidden from the recipient.
                        // Source: https://hanabi.github.io/level-2/#the-reverse-finesse
                        push_signal(
                            &mut self.signals,
                            entry,
                            *giver,
                            Some(connection.actor),
                            HGroupMoveKind::ReverseFinesse,
                            connection.cards.clone(),
                            Some(connection.expected),
                        );
                    }

                    // The ordered graph is the executable mechanism,
                    // but preserve the exact Level-5 name as an audit
                    // signal. This keeps Hidden, Clandestine, Queued,
                    // and Ambiguous Finesses distinguishable without
                    // giving each one a second transition system.
                    // Sources:
                    // - https://hanabi.github.io/level-5/#the-hidden-finesse
                    // - https://hanabi.github.io/level-5/#the-clandestine-finesse
                    // - https://hanabi.github.io/level-5/#the-queued-finesse
                    // - https://hanabi.github.io/level-5/#the-ambiguous-finesse
                    if rule_enabled(profile, HGroupRuleId::SpecialFinesses)
                        && connection.kind == HGroupConnectionKind::Finesse
                    {
                        let was_queued = previous_connections
                            .iter()
                            .any(|prior| prior.actor == connection.actor);
                        let matching_finesse_positions = self
                            .hands
                            .iter()
                            .enumerate()
                            .filter(|(player, _)| *player != target.index())
                            .filter_map(|(player, hand)| {
                                finesse_position_id(hand, &previously_promptable, 0)
                                    .filter(|card| {
                                        historical.identity(*card) == Some(connection.expected)
                                    })
                                    .map(|_| player)
                            })
                            .count();
                        let first_actual = connection
                            .cards
                            .first()
                            .and_then(|card| historical.identity(*card));
                        let exact = if was_queued {
                            Some(HGroupMoveKind::QueuedFinesse)
                        } else if matching_finesse_positions > 1 {
                            Some(HGroupMoveKind::AmbiguousFinesse)
                        } else if connection.cards.len() > 1
                            && first_actual
                                .is_some_and(|identity| bluff_play_connects(*clue, identity))
                        {
                            Some(HGroupMoveKind::ClandestineFinesse)
                        } else if connection.cards.len() > 1 {
                            Some(HGroupMoveKind::LayeredFinesse)
                        } else if new_connections.iter().any(|other| {
                            other.actor == connection.actor
                                && other.kind == HGroupConnectionKind::Prompt
                        }) {
                            Some(HGroupMoveKind::HiddenFinesse)
                        } else {
                            None
                        };
                        if let Some(exact) = exact {
                            push_signal(
                                &mut self.signals,
                                entry,
                                *giver,
                                Some(connection.actor),
                                exact,
                                connection.cards.clone(),
                                Some(connection.expected),
                            );
                        }
                    }
                }
                // A Play interpretation is an executable promise only
                // when the focus can play now or the shared connection
                // graph contains the Prompt/Finesse path that makes it
                // playable later. Merely finding a delayed identity
                // mask is not enough; treating it as a persistent play
                // manufactured phantom obligations from otherwise
                // unresolved clues.
                if (!play_identities.is_empty() && direct_play == play_identities)
                    || !new_connections.is_empty()
                {
                    if new_connections.is_empty() {
                        self.already_playing
                            .insert_from(EffectSource::Event(entry.turn), focus);
                    } else {
                        for connection in &new_connections {
                            self.already_playing
                                .insert_from(EffectSource::Promise(connection.promise), focus);
                        }
                    }
                }
            }
            if is_required_fix {
                self.required_fixes.retain(|obligation| {
                    let required = obligation.required;
                    !(required.actor == *giver
                        && required.target == *target
                        && touched.contains(&required.focus)
                        && clue.matches(required.identity))
                });
            }
        } else {
            for card in touched {
                self.facts[card.index()].add_positive_clue(*clue);
            }
            for card in untouched {
                self.facts[card.index()].add_negative_clue(*clue);
            }
            self.explicitly_clued.extend(touched.iter().copied());
        }
        self.historical_clue_tokens = self.historical_clue_tokens.saturating_sub(1);
    }

    #[allow(clippy::too_many_lines)]
    fn reduce_play(&mut self, frame: &mut ReplayEvent<'_>) {
        let view = frame.view;
        let profile = frame.profile;
        let entry = frame.entry;
        let ObservedEvent::Played {
            player,
            card,
            identity,
            successful,
        } = &frame.entry.event
        else {
            unreachable!("event dispatcher selects its handler")
        };

        if rule_enabled(profile, HGroupRuleId::SpecialFinesses) {
            if let Some(proof) = public_layers::demonstrated_hidden_layer(
                view,
                entry,
                &self.hands,
                &self.clues,
                &self.facts,
                &self.explicitly_clued,
                &self.forced_playable,
                &self.pending_connections,
                self.stack_heights,
            ) {
                let clue = &mut self.clues[proof.clue_index];
                self.pending_connections.cancel_where(
                    entry.turn,
                    ConnectionTransitionReason::Superseded,
                    |connection| connection.focus == clue.focus,
                );
                for old in clue
                    .focus_identities
                    .iter()
                    .filter(|identity| *identity != proof.focus_identity)
                {
                    push_signal(
                        &mut self.signals,
                        entry,
                        clue.giver,
                        Some(clue.target),
                        HGroupMoveKind::Retraction,
                        vec![clue.focus],
                        Some(old),
                    );
                }
                clue.kind = HGroupClueKind::Play;
                clue.focus_identities = IdentitySet::singleton(proof.focus_identity);
                clue.play_identities = clue.focus_identities;
                clue.hypotheses = vec![ClueInterpretationHypothesis {
                    focus_identity: proof.focus_identity,
                    connection_steps: vec![ClueConnectionStep {
                        actor: proof.actor,
                        cards: proof.cards.clone(),
                        expected: proof.expected,
                        kind: HGroupConnectionKind::Finesse,
                    }],
                    required_fix: None,
                    loaded: false,
                }];
                let promise = self.pending_connections.start(
                    clue.turn,
                    ConnectionObligation {
                        promise: PromiseId::UNASSIGNED,
                        actor: proof.actor,
                        cards: proof.cards.clone(),
                        expected: proof.expected,
                        focus_identity: proof.focus_identity,
                        kind: HGroupConnectionKind::Finesse,
                        focus: clue.focus,
                        step: 0,
                    },
                );
                self.invisibly_clued.extend_from(
                    EffectSource::Promise(promise),
                    proof
                        .cards
                        .iter()
                        .copied()
                        .filter(|candidate| candidate != card),
                );
                push_signal(
                    &mut self.signals,
                    entry,
                    clue.giver,
                    Some(clue.target),
                    HGroupMoveKind::Context,
                    vec![clue.focus],
                    Some(proof.focus_identity),
                );
                push_signal(
                    &mut self.signals,
                    entry,
                    clue.giver,
                    Some(proof.actor),
                    HGroupMoveKind::LayeredFinesse,
                    proof.cards,
                    Some(proof.expected),
                );
            }
        }
        // A successful off-suit blind play can demonstrate that a
        // visible, later Finesse connector was only one branch of an
        // Ambiguous Layered Finesse. The player immediately after the
        // blind play must then test their own Finesse Position before
        // the visible connector acts. This refinement is necessarily
        // observer-relative: everyone else can see that hidden card
        // and scheduled it from the original clue, while the possible
        // blind player initially had to trust the later visible copy.
        //
        // Sources:
        // - https://hanabi.github.io/level-5/#the-layered-finesse
        // - https://hanabi.github.io/level-5/#the-ambiguous-finesse
        let demonstrated_layer = (*successful
            && rule_enabled(profile, HGroupRuleId::SpecialFinesses)
            && next_player(*player, self.hands.len()) == view.observer
            && !was_clued_before(view, entry.turn, *card)
            && !self.pending_connections.iter().any(|connection| {
                connection.actor == *player
                    && connection.cards.first() == Some(card)
                    && self.pending_connections.is_active(connection)
            }))
        .then(|| {
            self.pending_connections
                .iter()
                .find(|connection| {
                    connection.kind == HGroupConnectionKind::Finesse
                        && connection.actor != *player
                        && connection.actor != view.observer
                        && connection.actor == next_player(view.observer, self.hands.len())
                        && connection.expected != *identity
                        && self.pending_connections.is_active(connection)
                })
                .cloned()
        })
        .flatten();
        if let Some(prior) = demonstrated_layer {
            let active_invisible =
                active_invisibly_clued(&self.invisibly_clued, &self.pending_connections);
            let mut gotten =
                protected_cards(&self.explicitly_clued, &active_invisible, &self.chop_moved);
            gotten.extend(self.already_playing.iter().copied());
            if let Some(next_card) =
                finesse_position_id(&self.hands[view.observer.index()], &gotten, 0)
            {
                let promise = self.pending_connections.start(
                    entry.turn,
                    ConnectionObligation {
                        promise: PromiseId::UNASSIGNED,
                        actor: view.observer,
                        cards: vec![next_card],
                        expected: prior.expected,
                        focus_identity: prior.focus_identity,
                        kind: HGroupConnectionKind::Finesse,
                        focus: prior.focus,
                        step: prior.step,
                    },
                );
                if promise != PromiseId::UNASSIGNED {
                    self.invisibly_clued
                        .insert_from(EffectSource::Promise(promise), next_card);
                }
            }
        }
        let advance = self.pending_connections.advance_play(
            entry.turn,
            *player,
            *card,
            *identity,
            *successful,
        );
        self.pending_connections
            .prioritize_active_prompts(entry.turn, |connection, candidate| {
                self.explicitly_clued.contains(&candidate)
                    && self.facts[candidate.index()].allows(connection.expected)
            });
        let failed_connections = advance.failed_focuses;
        let released_candidates = advance.released_candidates;
        for focus in failed_connections {
            self.already_playing.remove(&focus);
            self.forced_playable.remove(&focus);
            self.invalidated_focuses.insert(focus);
        }
        for released in released_candidates {
            if !self.explicitly_clued.contains(&released)
                && !self
                    .pending_connections
                    .iter()
                    .any(|connection| connection.cards.contains(&released))
            {
                self.invisibly_clued.remove(&released);
            }
        }
        remove_card(&mut self.hands[player.index()], *card);
        self.invisibly_clued.remove(card);
        self.already_playing.remove(card);
        if *successful {
            self.stack_heights[identity.suit.index()] = identity.rank.number();
            if identity.rank == Rank::Five {
                self.historical_clue_tokens = self
                    .historical_clue_tokens
                    .saturating_add(1)
                    .min(MAX_CLUE_TOKENS);
            }
            let satisfied_elsewhere = self
                .pending_connections
                .iter()
                .filter(|connection| connection.expected == *identity)
                .flat_map(|connection| connection.cards.iter().copied())
                .collect::<CardSet>();
            let disproved_prompts = self
                .pending_connections
                .iter()
                .filter(|connection| {
                    connection.expected == *identity
                        && connection.kind == HGroupConnectionKind::Prompt
                })
                .flat_map(|connection| connection.cards.iter().copied())
                .collect::<CardSet>();
            if !disproved_prompts.is_empty() {
                push_signal(
                    &mut self.signals,
                    entry,
                    *player,
                    None,
                    HGroupMoveKind::Retraction,
                    disproved_prompts.iter().copied().collect(),
                    Some(*identity),
                );
            }
            self.pending_connections.cancel_where(
                entry.turn,
                ConnectionTransitionReason::IdentitySatisfiedElsewhere,
                |connection| connection.expected == *identity,
            );
            for satisfied in satisfied_elsewhere {
                self.already_playing.remove(&satisfied);
                self.forced_playable.remove(&satisfied);
                if !self.explicitly_clued.contains(&satisfied)
                    && !self
                        .pending_connections
                        .iter()
                        .any(|connection| connection.cards.contains(&satisfied))
                {
                    self.invisibly_clued.remove(&satisfied);
                }
            }
        } else {
            self.public_removed[identity.index()] += 1;
        }
        self.must_clue.remove(player);
    }

    #[allow(clippy::too_many_lines)]
    fn reduce_discard(&mut self, frame: &mut ReplayEvent<'_>) {
        let view = frame.view;
        let profile = frame.profile;
        let perspective_depth = frame.perspective_depth;
        let entry_index = frame.entry_index;
        let entry = frame.entry;
        let action_is_settled = frame.action_is_settled;
        let ObservedEvent::Discarded {
            player,
            card,
            identity,
        } = &frame.entry.event
        else {
            unreachable!("event dispatcher selects its handler")
        };

        let physically_clued = was_clued_before(view, entry.turn, *card);
        let needs_actor_projection = physically_clued
            || (perspective_depth.models_other_players() && *player != view.observer);
        let subjective = needs_actor_projection
            .then(|| {
                subjective_action_context_before(
                    SubjectiveReplayRequest {
                        source: view,
                        profile,
                        observer: *player,
                        history: &view.history[..entry_index],
                        hands: &self.hands,
                        facts: &self.facts,
                        deck_size: self.historical_deck_size,
                    },
                    *card,
                )
            })
            .flatten();
        if physically_clued {
            frame.outcome.actor_known_discard_identity =
                subjective.and_then(|context| context.known_identity);
        }
        if action_is_settled && !self.discard_now.contains(card) {
            record_declined_direct_plays(
                *player,
                Some(entry.turn),
                &self.hands,
                &self.clues,
                &self.already_playing,
                &self.pending_connections,
                &mut DirectPlayDeclines {
                    cards: &mut self.declined_direct_plays,
                    turns: &mut self.declined_direct_play_turns,
                },
            );
        }
        // A discard declines every currently actionable blind-play
        // promise in the actor's hand. Keeping those one-turn
        // obligations alive made later clues appear unsafe because
        // the reducer still expected a Bluff/Finesse card that the
        // player had publicly declined several turns earlier.
        // Delayed connection steps are represented by the connection
        // graph rather than `forced_playable`, so clearing this set
        // does not erase a downstream obligation that is not due yet.
        for declined in &self.hands[player.index()] {
            self.forced_playable.remove(declined);
        }
        let active_invisible =
            active_invisibly_clued(&self.invisibly_clued, &self.pending_connections);
        let gotten = protected_cards(&self.explicitly_clued, &active_invisible, &self.chop_moved);
        frame.outcome.actor_saw_normal_discard = chop(&self.hands[player.index()], &gotten)
            == Some(*card)
            || (perspective_depth.models_other_players()
                && *player != view.observer
                && subjective.and_then(|context| context.chop) == Some(*card));
        if chop(&self.hands[player.index()], &gotten) == Some(*card) {
            self.early_game = false;
        }
        self.pending_connections.discard(entry.turn, *player, *card);
        remove_card(&mut self.hands[player.index()], *card);
        self.invisibly_clued.remove(card);
        self.already_playing.remove(card);
        self.public_removed[identity.index()] += 1;
        self.must_clue.remove(player);
        self.historical_clue_tokens = self
            .historical_clue_tokens
            .saturating_add(1)
            .min(MAX_CLUE_TOKENS);
    }

    #[allow(clippy::too_many_lines)]
    fn reduce_draw(&mut self, frame: &mut ReplayEvent<'_>) {
        let ObservedEvent::Drew { player, card, .. } = &frame.entry.event else {
            unreachable!("event dispatcher selects its handler")
        };

        self.hands[player.index()].push(*card);
        self.historical_deck_size = self.historical_deck_size.saturating_sub(1);
    }
}
