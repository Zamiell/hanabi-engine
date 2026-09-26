//! Common, label-independent principle checks over compiled clue effects.
use super::{
    Action, ClueProposal, CluePurpose, HGroupMoveKind, HGroupProfile, HGroupState, LineOutcome,
    LogicalDeductions, MAX_CLUE_TOKENS, Rank, compiled_baseline_team, compiled_prospective_clue,
    identity_of, is_playable_now, next_player,
};
use crate::{CluePrincipleCheck, ConventionRejectionReason, PrincipleVerdict};

#[derive(Clone, Copy, Debug)]
pub(super) struct Validation {
    pub(super) checks: [CluePrincipleCheck; 3],
    pub(super) rejection: Option<ConventionRejectionReason>,
}

fn check(
    principle: &'static str,
    verdict: PrincipleVerdict,
    evidence: &'static str,
) -> CluePrincipleCheck {
    CluePrincipleCheck {
        principle,
        verdict,
        evidence,
        exception: None,
        new_cards: 0,
    }
}

pub(super) fn validate(
    deductions: &LogicalDeductions,
    profile: HGroupProfile,
    replay: &HGroupState,
    proposal: &ClueProposal,
    outcome: Option<&LineOutcome>,
) -> Validation {
    let mcv = minimum_value(deductions, profile, replay, proposal, outcome);
    let touch = good_touch(deductions, profile, replay, proposal, outcome);
    let mut response = response_safety(deductions, profile, proposal, outcome);
    if source_creates_false_anxiety(deductions, profile, replay, proposal) {
        response.verdict = PrincipleVerdict::Fail;
        response.evidence =
            "Leaving the next player locked at zero clues creates a false play obligation.";
    }
    let rejection = if mcv.verdict == PrincipleVerdict::Fail {
        Some(ConventionRejectionReason::MinimumClueValue)
    } else if touch.verdict == PrincipleVerdict::Fail {
        Some(ConventionRejectionReason::BadTouch)
    } else if response.verdict == PrincipleVerdict::Fail {
        Some(ConventionRejectionReason::UnprovenResponse)
    } else {
        None
    };
    Validation {
        checks: [mcv, touch, response],
        rejection,
    }
}

fn source_creates_false_anxiety(
    deductions: &LogicalDeductions,
    profile: HGroupProfile,
    replay: &HGroupState,
    proposal: &ClueProposal,
) -> bool {
    let view = deductions.view();
    super::rule_enabled(profile, super::HGroupRuleId::Stalling)
        && view.clue_tokens == 1
        && (super::interpretation::creates_false_anxiety(
            view,
            profile,
            &replay.gotten_from(&replay.promptable()),
            proposal,
        ) || super::interpretation::creates_false_anxiety_after_forced_play(
            view, profile, proposal,
        ))
}

fn minimum_value(
    deductions: &LogicalDeductions,
    profile: HGroupProfile,
    replay: &HGroupState,
    proposal: &ClueProposal,
    outcome: Option<&LineOutcome>,
) -> CluePrincipleCheck {
    let source = deductions.view();
    let Action::Clue { target, clue } = proposal.action else {
        unreachable!("clue proposal")
    };
    let touched = source.hands[target.index()]
        .iter()
        .filter(|card| card.identity.is_some_and(|id| clue.matches(id)))
        .map(|card| card.id)
        .collect::<Vec<_>>();
    let compiled = compiled_prospective_clue(source, profile, target, clue, &touched);
    let gotten = replay.gotten_from(&replay.promptable());
    let allowed_stall = permits_stall(deductions, replay, proposal);
    let mut mcv = match outcome {
        Some(value) if value.clue_efficiency > 0 => {
            let mut result = check(
                "minimumClueValue",
                PrincipleVerdict::Pass,
                "New useful identities are secured by the compiled effects.",
            );
            result.new_cards = value.clue_efficiency;
            result
        }
        Some(value) if !value.unresolved_acquisitions.is_empty() => check(
            "minimumClueValue",
            PrincipleVerdict::Unresolved,
            "A potentially new acquisition has an unresolved identity; zero value is not established.",
        ),
        Some(_) => check(
            "minimumClueValue",
            PrincipleVerdict::Fail,
            "The clue obtains no new useful play or save.",
        ),
        None => check(
            "minimumClueValue",
            PrincipleVerdict::Unresolved,
            "The causal outcome could not be compiled; value is not established.",
        ),
    };
    if mcv.verdict == PrincipleVerdict::Fail {
        let before_team = compiled_baseline_team(source, profile);
        let repair = matches!(proposal.purpose(), CluePurpose::Fix)
            && (replay.required_fixes.iter().any(|obligation| {
                obligation.required.actor == source.current_player
                    && obligation.required.target == target
                    && touched.contains(&obligation.required.focus)
            }) || outcome.is_some_and(|value| !value.known_trash.is_empty())
                || before_team
                    .projection(target)
                    .zip(compiled.as_ref().and_then(|c| c.projection(target)))
                    .is_some_and(|(before, after)| {
                        before.inferred.connection.is_some_and(|connection| {
                            identity_of(source, connection.card)
                                .is_some_and(|actual| actual != connection.identity)
                                && after.inferred.connection != before.inferred.connection
                        })
                    }));
        let valuable_tempo = outcome.is_some_and(|value| {
            let tempo_plays = value
                .newly_playable
                .iter()
                .filter(|(_, card)| {
                    super::was_clued_before(source, source.turn, *card)
                        && identity_of(source, *card).is_some_and(|id| is_playable_now(source, id))
                })
                .collect::<Vec<_>>();
            !tempo_plays.is_empty()
                && (tempo_plays.len() >= 2
                    || source.hands[target.index()]
                        .iter()
                        .all(|card| gotten.contains(&card.id))
                    || tempo_plays.iter().any(|(owner, card)| {
                        *owner == target
                            && identity_of(source, *card).is_some_and(|id| id.rank != Rank::Five)
                            && source.hands[target.index()]
                                .iter()
                                .rev()
                                .take_while(|c| c.id != *card)
                                .any(|c| {
                                    replay.promptable().contains(&c.id)
                                        && c.identity.is_some_and(|id| !is_playable_now(source, id))
                                })
                    }))
        });
        let exception = if repair {
            Some("https://hanabi.github.io/level-3/#the-fix-clue")
        } else if valuable_tempo {
            Some("https://hanabi.github.io/level-6/#the-valuable-tempo-clue")
        } else if allowed_stall {
            Some("https://hanabi.github.io/level-9/#allowable-stall-clues-stall-table")
        } else {
            None
        };
        if let Some(url) = exception {
            mcv.verdict = PrincipleVerdict::Exception;
            mcv.exception = Some(url);
            mcv.evidence =
                "A documented repair, valuable tempo, or stall has its preconditions satisfied.";
        }
    }

    mcv
}

fn good_touch(
    deductions: &LogicalDeductions,
    profile: HGroupProfile,
    replay: &HGroupState,
    proposal: &ClueProposal,
    outcome: Option<&LineOutcome>,
) -> CluePrincipleCheck {
    let source = deductions.view();
    let Action::Clue { target, clue } = proposal.action else {
        unreachable!("clue proposal")
    };
    let touched = source.hands[target.index()]
        .iter()
        .filter(|card| card.identity.is_some_and(|id| clue.matches(id)))
        .map(|card| card.id)
        .collect::<Vec<_>>();
    let compiled = compiled_prospective_clue(source, profile, target, clue, &touched);
    let allowed_stall = permits_stall(deductions, replay, proposal);
    let promptable = replay.promptable();
    let newly_touched = touched
        .iter()
        .copied()
        .filter(|id| !promptable.contains(id))
        .collect::<Vec<_>>();
    let notes = super::convention_card_inferences(deductions, replay);
    let fixed = replay.cards.facts.fixed_cards();
    let context = super::admission::GoodTouchContext {
        view: source,
        newly_touched: &newly_touched,
        clue: Some((clue, &touched)),
        explicitly_clued: &promptable,
        fixed_cards: fixed,
        convention_cards: &notes,
    };
    let mut touch = check(
        "goodTouch",
        PrincipleVerdict::Pass,
        "New touch promises are useful and nonduplicating.",
    );
    if !super::admission::good_touch(context) {
        // Trash conventions modify the touched cards' promise, not the
        // validity of every consequence. Require the actual owner to know
        // those cards are trash and a productive effect or permitted stall.
        let owner_trash = compiled
            .as_ref()
            .and_then(|c| c.projection(target))
            .map(|after| {
                newly_touched
                    .iter()
                    .copied()
                    .filter(|card| {
                        // A bad clue can make the recipient incorrectly infer trash.
                        // That inference cannot certify its own Good Touch exception.
                        let actually_trash = identity_of(source, *card).is_some_and(|id| {
                            !super::is_eventually_useful(source, id)
                                || source.hands.iter().flatten().any(|other| {
                                    other.id != *card
                                        && promptable.contains(&other.id)
                                        && other.identity == Some(id)
                                })
                        });
                        actually_trash
                            && (after.knows_trash(*card, &promptable)
                                || outcome
                                    .is_some_and(|value| value.demonstrated_trash.contains(card)))
                    })
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        let remaining = newly_touched
            .iter()
            .copied()
            .filter(|card| !owner_trash.contains(card))
            .collect::<Vec<_>>();
        let productive_touches = super::admission::good_touch(super::admission::GoodTouchContext {
            newly_touched: &remaining,
            ..context
        });
        if !owner_trash.is_empty()
            && productive_touches
            && (allowed_stall || outcome.is_some_and(|value| value.clue_efficiency > 0))
        {
            touch.verdict = PrincipleVerdict::Exception;
            touch.exception = Some(
                if outcome.is_some_and(|value| !value.demonstrated_trash.is_empty()) {
                    "https://hanabi.github.io/level-16/#the-unknown-trash-discharge-1-for-1-form-utd"
                } else {
                    "https://hanabi.github.io/level-4/#the-trash-chop-move-tcm"
                },
            );
            touch.evidence = "The owner recognizes touched trash immediately or after the safe required response; productivity or a permitted stall is independently established.";
        } else if critical_protection_requires_bad_touch(context, replay, target, outcome) {
            touch.verdict = PrincipleVerdict::Exception;
            touch.exception = Some(
                "https://hanabi.github.io/beginner/save-principle/#violating-good-touch-principle",
            );
            touch.evidence = "The clue protects a critical chop, and every direct clue on that card violates Good Touch.";
        } else {
            touch.verdict = PrincipleVerdict::Fail;
            touch.evidence = "New touched cards promise trash or a secured duplicate without a demonstrated exception.";
        }
    }

    touch
}

fn critical_protection_requires_bad_touch(
    context: super::admission::GoodTouchContext<'_>,
    replay: &HGroupState,
    target: super::PlayerId,
    outcome: Option<&LineOutcome>,
) -> bool {
    let source = context.view;
    outcome.is_some_and(|value| {
        value.protection.iter().any(|effect| {
            effect.owner == target
                && identity_of(source, effect.card).is_some_and(|id| {
                    (id.rank == Rank::Five || super::is_critical_save_identity(source, id))
                        && source
                            .legal_actions()
                            .into_iter()
                            .filter_map(|action| {
                                let Action::Clue {
                                    target: other,
                                    clue: alternate,
                                } = action
                                else {
                                    return None;
                                };
                                (other == target && alternate.matches(id)).then_some(alternate)
                            })
                            .all(|alternate| {
                                let touches = source.hands[target.index()]
                                    .iter()
                                    .filter(|card| {
                                        card.identity.is_some_and(|face| alternate.matches(face))
                                    })
                                    .map(|card| card.id)
                                    .collect::<Vec<_>>();
                                let newly = touches
                                    .iter()
                                    .copied()
                                    .filter(|card| !replay.promptable().contains(card))
                                    .collect::<Vec<_>>();
                                !super::admission::good_touch(super::admission::GoodTouchContext {
                                    newly_touched: &newly,
                                    clue: Some((alternate, &touches)),
                                    ..context
                                })
                            })
                })
        })
    })
}

fn response_safety(
    deductions: &LogicalDeductions,
    profile: HGroupProfile,
    proposal: &ClueProposal,
    outcome: Option<&LineOutcome>,
) -> CluePrincipleCheck {
    let source = deductions.view();
    let Action::Clue { target, clue } = proposal.action else {
        unreachable!("clue proposal")
    };
    let touched = source.hands[target.index()]
        .iter()
        .filter(|card| card.identity.is_some_and(|id| clue.matches(id)))
        .map(|card| card.id)
        .collect::<Vec<_>>();
    let compiled = compiled_prospective_clue(source, profile, target, clue, &touched);
    let reactor = next_player(source.current_player, source.hands.len());
    let mut response = check(
        "responseSafety",
        PrincipleVerdict::Pass,
        "No newly due visible response is an unsupported misplay.",
    );
    if let Some(value) = outcome {
        for (owner, card) in &value.newly_playable {
            if *owner == reactor {
                match identity_of(source, *card) {
                    Some(id) if !is_playable_now(source, id) => {
                        response.verdict = PrincipleVerdict::Fail;
                        response.evidence = "The immediate reactor acquires a visible misplay.";
                    }
                    None if response.verdict != PrincipleVerdict::Fail => {
                        response.verdict = PrincipleVerdict::Unresolved;
                        response.evidence = "The immediate response identity remains conditional.";
                    }
                    _ => {}
                }
            }
        }
    } else {
        response.verdict = PrincipleVerdict::Unresolved;
        response.evidence = "Response consequences could not be compiled.";
    }
    if let Some((actor, card)) = proposal.required_response {
        match compiled.as_ref().and_then(|c| c.projection(actor)) {
            Some(owner)
                if super::preferred_due_play_card(
                    owner.deductions.view(),
                    &owner.inferred,
                    profile,
                ) != Some(card) =>
            {
                response.verdict = PrincipleVerdict::Fail;
                response.evidence =
                    "The declared response is not the responding player's own due play.";
            }
            Some(owner)
                if owner.inferred.connection.is_some_and(|connection| {
                    connection.card == card
                        && super::decision::response_requires_pass_back(
                            owner.deductions.view(),
                            profile,
                            connection,
                        )
                }) =>
            {
                response.verdict = PrincipleVerdict::Fail;
                response.evidence =
                    "The responder must pass back this unsafe blind-play obligation.";
            }
            None => {
                response.verdict = PrincipleVerdict::Unresolved;
                response.evidence =
                    "The declared response could not be verified in its owner's view.";
            }
            _ => {}
        }
    }
    if let Some(line) = compiled
        .as_ref()
        .and_then(|c| c.line_evidence(source, proposal.move_kind()))
    {
        if let Some(blind) = line
            .named
            .as_ref()
            .and_then(|line| line.blind_play_cards.first())
        {
            match compiled.as_ref().and_then(|c| c.projection(reactor)) {
                Some(owner)
                    if !owner.inferred.playable_now.contains(blind)
                        && owner
                            .inferred
                            .connection
                            .is_none_or(|connection| connection.card != *blind) =>
                {
                    response.verdict = PrincipleVerdict::Fail;
                    response.evidence = "The proposed blind response is not an obligation in the reactor's own view.";
                }
                None => {
                    response.verdict = PrincipleVerdict::Unresolved;
                }
                _ => {}
            }
        }
    }
    response
}

fn permits_stall(
    deductions: &LogicalDeductions,
    replay: &HGroupState,
    proposal: &ClueProposal,
) -> bool {
    let source = deductions.view();
    let gotten = replay.gotten_from(&replay.promptable());
    let stall_context = source.clue_tokens == MAX_CLUE_TOKENS
        || replay.must_clue.contains(&source.observer)
        || source.hands[source.observer.index()]
            .iter()
            .all(|card| gotten.contains(&card.id))
        || source.deck_size <= source.hands.len();
    matches!(
        proposal.move_kind(),
        Some(
            HGroupMoveKind::Stall
                | HGroupMoveKind::Burn
                | HGroupMoveKind::FillInClue
                | HGroupMoveKind::TempoClue
                | HGroupMoveKind::FiveStall
        )
    ) && (stall_context
        || (proposal.move_kind() == Some(HGroupMoveKind::FiveStall) && replay.early_game))
}

#[cfg(test)]
mod tests {
    use super::*;
    use hanabi_core::{Clue, PlayerId, Suit};
    use hanabi_protocol::HanabiLiveReplay;

    #[test]
    fn reviewed_critical_save_exception_requires_protection_evidence() {
        // Human-reviewed historical p4v0s1 turn 34: 4s saves Alice's last
        // y4 despite b4 collateral; yellow would touch the trash y2 instead.
        let fixture = HanabiLiveReplay::from_json(include_str!(
            "tests/fixtures/game-p4v0s1-before-turn18-revision.json"
        ))
        .unwrap();
        let state = fixture.state_at_turn(33).unwrap();
        let d = LogicalDeductions::new(state.view_for(state.current_player()).unwrap()).unwrap();
        let replay = super::super::replay_h_group(&d, HGroupProfile::Max);
        let proposal = ClueProposal::new(
            Action::Clue {
                target: PlayerId::new(0),
                clue: Clue::Rank(Rank::Four),
            },
            Some(HGroupMoveKind::SaveClue),
            super::super::ClueValue::new(1),
            CluePurpose::Save,
            super::super::ClueSchedule::new(false, false),
            0,
        );
        let mut outcome = super::super::clue_outcome::scheduled_clue_outcome(
            d.view(),
            HGroupProfile::Max,
            &proposal,
        )
        .unwrap();
        let accepted = validate(&d, HGroupProfile::Max, &replay, &proposal, Some(&outcome));
        assert_eq!(accepted.checks[1].verdict, PrincipleVerdict::Exception);
        assert_eq!(accepted.rejection, None);
        outcome.protection.clear();
        let without_protection =
            validate(&d, HGroupProfile::Max, &replay, &proposal, Some(&outcome));
        assert_eq!(
            without_protection.rejection,
            Some(ConventionRejectionReason::BadTouch)
        );
    }

    #[test]
    fn principle_failures_and_unknowns_cannot_be_rescued_by_labels_or_scores() {
        // Prior reviewed p4v0s1 turn 33; substitute only the outcome evidence
        // to exercise the validator's contract, not to assert a new strategy.
        let fixture = HanabiLiveReplay::from_json(include_str!(
            "tests/fixtures/game-p4v0s1-before-turn27-revision.json"
        ))
        .unwrap();
        let state = fixture.state_at_turn(32).unwrap();
        let deductions =
            LogicalDeductions::new(state.view_for(state.current_player()).unwrap()).unwrap();
        let replay = super::super::replay_h_group(&deductions, HGroupProfile::Max);
        super::super::with_prospective_analysis_cache(
            deductions.view(),
            HGroupProfile::Max,
            || {
                for kind in super::super::H_GROUP_LEVELS
                    .iter()
                    .flat_map(|level| level.effects)
                    .copied()
                {
                    let purpose = match kind {
                        HGroupMoveKind::FixClue => CluePurpose::Fix,
                        HGroupMoveKind::TempoClue => CluePurpose::Tempo,
                        HGroupMoveKind::PlayClue => CluePurpose::Play,
                        HGroupMoveKind::SaveClue => CluePurpose::Save,
                        _ => CluePurpose::Advanced,
                    };
                    let proposal = ClueProposal::new(
                        Action::Clue {
                            target: PlayerId::new(2),
                            clue: Clue::Suit(Suit::Green),
                        },
                        Some(kind),
                        super::super::ClueValue::new(u16::MAX),
                        purpose,
                        super::super::ClueSchedule::new(false, false),
                        0,
                    );
                    let failed = validate(
                        &deductions,
                        HGroupProfile::Max,
                        &replay,
                        &proposal,
                        Some(&LineOutcome::default()),
                    );
                    assert_eq!(failed.checks[0].verdict, PrincipleVerdict::Fail, "{kind:?}");
                    assert_eq!(
                        failed.rejection,
                        Some(ConventionRejectionReason::MinimumClueValue)
                    );
                    let unknown =
                        validate(&deductions, HGroupProfile::Max, &replay, &proposal, None);
                    assert_eq!(unknown.checks[0].verdict, PrincipleVerdict::Unresolved);
                    assert_eq!(unknown.checks[2].verdict, PrincipleVerdict::Unresolved);
                    let partial = LineOutcome {
                        unresolved_acquisitions: vec![super::super::CardId::new(30)],
                        ..LineOutcome::default()
                    };
                    let uncertain_card = validate(
                        &deductions,
                        HGroupProfile::Max,
                        &replay,
                        &proposal,
                        Some(&partial),
                    );
                    assert_eq!(
                        uncertain_card.checks[0].verdict,
                        PrincipleVerdict::Unresolved
                    );
                }
            },
        );
    }
}
