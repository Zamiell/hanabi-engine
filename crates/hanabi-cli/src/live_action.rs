use std::io::{self, BufRead, Read, Write};

use hanabi_core::{Card, PlayerView};
use hanabi_protocol::{HanabiLiveActionCommand, HanabiLiveSessionState, HanabiLiveSnapshot};
use hanabi_search::{
    ConventionInferences, HGroupInferences, IdentitySet, LogicalDeductions, PlannerConfig,
    PositionAnalysis, analyze_position,
};
use serde::Serialize;
use serde_json::{Value, json};

use crate::{CliError, LiveActionArguments};

pub(super) fn run(arguments: &LiveActionArguments) -> Result<(), CliError> {
    let mut json = String::new();
    io::stdin()
        .read_to_string(&mut json)
        .map_err(CliError::ReadLiveSnapshot)?;
    let snapshot = HanabiLiveSnapshot::from_json(&json).map_err(CliError::LiveSnapshot)?;
    let view = snapshot.player_view().map_err(CliError::LiveSnapshot)?;
    let decision = decide(snapshot.table_id(), &view, arguments)?;
    let output = if arguments.include_planning_details {
        serde_json::to_string(&decision)
    } else {
        serde_json::to_string(&decision.action)
    }
    .map_err(CliError::SerializeReport)?;
    println!("{output}");
    Ok(())
}

pub(super) fn run_session(arguments: &LiveActionArguments) -> Result<(), CliError> {
    let stdin = io::stdin();
    let stdout = io::stdout();
    let mut output = stdout.lock();
    let mut session = HanabiLiveSessionState::new();

    for line in stdin.lock().lines() {
        let line = line.map_err(CliError::ReadLiveSnapshot)?;
        if line.trim().is_empty() {
            continue;
        }
        let response = match session.apply_json(&line).map_err(CliError::LiveSnapshot) {
            Ok((table_id, view)) => decide(table_id, &view, arguments),
            Err(error) => Err(error),
        };
        match response {
            Ok(decision) => {
                if arguments.include_planning_details {
                    serde_json::to_writer(&mut output, &decision)
                        .map_err(CliError::SerializeReport)?;
                } else {
                    serde_json::to_writer(&mut output, &decision.action)
                        .map_err(CliError::SerializeReport)?;
                }
            }
            Err(error) => {
                serde_json::to_writer(
                    &mut output,
                    &LiveSessionErrorResponse {
                        error: error.to_string(),
                    },
                )
                .map_err(CliError::SerializeReport)?;
            }
        }
        writeln!(output).map_err(CliError::WriteLiveSession)?;
        output.flush().map_err(CliError::WriteLiveSession)?;
    }
    Ok(())
}

fn decide(
    table_id: u64,
    view: &PlayerView,
    arguments: &LiveActionArguments,
) -> Result<LiveDecisionResponse, CliError> {
    let analysis = analyze_position(
        view,
        arguments.convention,
        PlannerConfig {
            objective: arguments.objective,
            exact_world_limit: arguments.exact_world_limit,
            exact_node_limit: arguments.exact_node_limit,
        },
    )
    .map_err(CliError::AnalyzePosition)?;
    Ok(LiveDecisionResponse {
        action: HanabiLiveActionCommand::from_engine_action(table_id, analysis.planner.best_action),
        logical_deductions: logical_deductions_json(analysis.information.deductions()),
        convention_inferences: convention_inferences_json(
            analysis.convention_analysis.inferences.clone(),
        ),
        planning: planning_json(table_id, &analysis, arguments.objective),
    })
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct LiveDecisionResponse {
    action: HanabiLiveActionCommand,
    logical_deductions: Value,
    convention_inferences: Value,
    planning: Value,
}

fn logical_deductions_json(deductions: &LogicalDeductions) -> Value {
    let own_cards = deductions
        .unknown_hand_cards()
        .iter()
        .map(|card| {
            json!({
                "card": card.index(),
                "possibleIdentities": deductions
                    .possible_identities(*card)
                    .map_or_else(Vec::new, identity_set_json),
            })
        })
        .collect::<Vec<_>>();
    json!({"ownCards": own_cards})
}

fn convention_inferences_json(inferences: ConventionInferences) -> Value {
    match inferences {
        ConventionInferences::None => json!({"framework": "none"}),
        ConventionInferences::HGroup(inferences) => h_group_inferences_json(&inferences),
        _ => json!({"framework": "unknown"}),
    }
}

fn h_group_inferences_json(inferences: &HGroupInferences) -> Value {
    let connection = inferences.connection.map(|connection| {
        json!({
            "card": connection.card.index(),
            "identity": identity_json(connection.identity),
            "kind": format!("{:?}", connection.kind),
            "focus": connection.focus.index(),
        })
    });
    let cards = inferences
        .cards
        .iter()
        .map(|card| {
            json!({
                "card": card.card.index(),
                "possibleIdentities": identity_set_json(card.identities),
                "focused": card.focused,
                "saved": card.saved,
                "finessed": card.play_obligation.is_some(),
            })
        })
        .collect::<Vec<_>>();
    let promises = inferences
        .connection_promises
        .iter()
        .map(|promise| {
            json!({
                "cards": promise.cards.iter().map(|card| card.index()).collect::<Vec<_>>(),
                "identity": identity_json(promise.identity),
            })
        })
        .collect::<Vec<_>>();
    let signals = inferences
        .signals
        .iter()
        .map(|signal| {
            json!({
                "turn": signal.turn,
                "actor": signal.actor.index(),
                "target": signal.target.map(hanabi_core::PlayerId::index),
                "kind": format!("{:?}", signal.kind),
                "cards": signal.cards.iter().map(|card| card.index()).collect::<Vec<_>>(),
                "identity": signal.identity.map(identity_json),
            })
        })
        .collect::<Vec<_>>();
    let clues = inferences
        .clues
        .iter()
        .map(|clue| {
            json!({
                "turn": clue.turn,
                "giver": clue.giver.index(),
                "target": clue.target.index(),
                "clue": format!("{:?}", clue.clue),
                "focus": clue.focus.index(),
                "focusWasChop": clue.focus_was_chop,
                "kind": format!("{:?}", clue.kind),
                "focusIdentities": identity_set_json(clue.focus_identities),
                "playIdentities": identity_set_json(clue.play_identities),
                "saveIdentities": identity_set_json(clue.save_identities),
            })
        })
        .collect::<Vec<_>>();
    json!({
        "framework": "h-group",
        "phase": format!("{:?}", inferences.phase),
        "earlyGame": inferences.early_game,
        "chops": inferences
            .chops
            .iter()
            .map(|card| card.map(hanabi_core::CardId::index))
            .collect::<Vec<_>>(),
        "playableNow": card_ids_json(&inferences.playable_now),
        "saved": card_ids_json(&inferences.saved),
        "discardNow": card_ids_json(&inferences.discard_now),
        "invisiblyClued": card_ids_json(&inferences.invisibly_clued),
        "chopMoved": card_ids_json(&inferences.chop_moved),
        "mustClue": inferences
            .must_clue
            .iter()
            .map(|player| player.index())
            .collect::<Vec<_>>(),
        "connection": connection,
        "cards": cards,
        "connectionPromises": promises,
        "signals": signals,
        "clues": clues,
    })
}

fn planning_json(
    table_id: u64,
    analysis: &PositionAnalysis,
    objective: hanabi_search::PlanningObjective,
) -> Value {
    let mut planning = json!({
        "convention": analysis.convention.id(),
        "profile": analysis.convention.profile().map(|profile| profile.to_string()),
        "rulesetRevision": analysis.convention.ruleset_revision(),
        "objective": objective.to_string(),
    });
    planning
        .as_object_mut()
        .expect("planning metadata is an object")
        .extend(
            planner_details_json(table_id, analysis.planner.best_action, &analysis.planner)
                .as_object()
                .expect("planning details are an object")
                .clone(),
        );
    planning
}

fn planner_details_json(
    table_id: u64,
    best_action: hanabi_core::Action,
    result: &hanabi_search::PlannerResult,
) -> Value {
    json!({
        "phase": match result.phase {
            hanabi_search::PlannerPhase::Symbolic => "symbolic",
            hanabi_search::PlannerPhase::Exact => "exact",
        },
        "consideredWorlds": result.considered_worlds,
        "worldCountExact": result.world_count_exact,
        "exactNodes": result.exact_nodes,
        "rootActions": result.root_actions.iter().map(|evaluation| json!({
            "action": HanabiLiveActionCommand::from_engine_action(table_id, evaluation.action),
            "selected": evaluation.action == best_action,
            "conventionPriority": evaluation.convention_priority,
            "certainlyPlayable": evaluation.certainly_playable,
            "certainlyUseless": evaluation.certainly_useless,
            "newlyTouched": evaluation.newly_touched,
            "immediatelyPlayableTouched": evaluation.immediately_playable_touched,
            "criticalTouched": evaluation.critical_touched,
            "oldestCardTouched": evaluation.oldest_card_touched,
            "symbolicLine": {
                "actions": evaluation.symbolic_line.actions,
                "scoreGain": evaluation.symbolic_line.score_gain,
                "discards": evaluation.symbolic_line.discards,
                "cluesSpent": evaluation.symbolic_line.clues_spent,
                "cluesGained": evaluation.symbolic_line.clues_gained,
                "strikes": evaluation.symbolic_line.strikes,
                "identityBranch": evaluation.symbolic_line.identity_branch,
                "reachedLimit": evaluation.symbolic_line.reached_limit,
            },
            "exact": evaluation.exact.map(|exact| json!({
                "worlds": exact.worlds,
                "perfectWorlds": exact.perfect_worlds,
                "perfectRate": exact.perfect_rate(),
                "scoreSum": exact.score_sum,
                "expectedScore": exact.expected_score(),
                "strikeoutWorlds": exact.strikeout_worlds,
                "strikeoutRate": exact.strikeout_rate(),
                "scoreCeilingSum": exact.score_ceiling_sum,
                "expectedScoreCeiling": exact.expected_score_ceiling(),
            })),
        })).collect::<Vec<_>>(),
    })
}

fn card_ids_json(cards: &[hanabi_core::CardId]) -> Vec<usize> {
    cards.iter().map(|card| card.index()).collect()
}

fn identity_set_json(identities: IdentitySet) -> Vec<Value> {
    identities.iter().map(identity_json).collect()
}

fn identity_json(card: Card) -> Value {
    json!({"suit": card.suit.to_string(), "rank": card.rank.number()})
}

#[derive(Serialize)]
struct LiveSessionErrorResponse {
    error: String,
}
