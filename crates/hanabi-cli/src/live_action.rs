use std::io::{self, BufRead, Read, Write};

use hanabi_core::{Card, PlayerView};
use hanabi_protocol::{HanabiLiveActionCommand, HanabiLiveSessionState, HanabiLiveSnapshot};
use hanabi_search::{
    BestMove, BestMoveError, ConventionFramework, ConventionInferences, HGroupInferences,
    IdentitySet, LogicalDeductions, SearchConfig, SearchDetails, best_move,
};
use serde::Serialize;
use serde_json::{Value, json};

use crate::{CliError, LiveActionArguments, SearchMode};

pub(super) fn run(arguments: &LiveActionArguments) -> Result<(), CliError> {
    let mut json = String::new();
    io::stdin()
        .read_to_string(&mut json)
        .map_err(CliError::ReadLiveSnapshot)?;
    let snapshot = HanabiLiveSnapshot::from_json(&json).map_err(CliError::LiveSnapshot)?;
    let view = snapshot.player_view().map_err(CliError::LiveSnapshot)?;
    let decision = decide(snapshot.table_id(), view, arguments)?;
    let output = if arguments.include_search_details {
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
        let response = session
            .apply_json(&line)
            .map_err(CliError::LiveSnapshot)
            .and_then(|(table_id, view)| decide(table_id, view, arguments));
        match response {
            Ok(decision) => {
                if arguments.include_search_details {
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
    view: PlayerView,
    arguments: &LiveActionArguments,
) -> Result<LiveDecisionResponse, CliError> {
    let deductions = LogicalDeductions::new(view.clone())
        .map_err(|error| CliError::BestMove(BestMoveError::InformationSet(error)))?;
    let convention_inferences = arguments.convention.infer(&deductions);
    let config = match arguments.mode {
        SearchMode::Ismcts => SearchConfig::Ismcts(hanabi_search::IsmctsConfig {
            iterations: arguments.iterations,
            exploration: arguments.exploration,
            seed: arguments.seed,
            objective: arguments.objective,
        }),
        SearchMode::Flat => SearchConfig::Flat(hanabi_search::MonteCarloConfig {
            samples_per_action: arguments.samples,
            seed: arguments.seed,
            objective: arguments.objective,
        }),
    };
    let best = best_move(view, arguments.convention, config).map_err(CliError::BestMove)?;
    Ok(LiveDecisionResponse {
        action: HanabiLiveActionCommand::from_engine_action(table_id, best.action),
        logical_deductions: logical_deductions_json(&deductions),
        convention_inferences: convention_inferences_json(convention_inferences),
        search: search_json(table_id, &best),
    })
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct LiveDecisionResponse {
    action: HanabiLiveActionCommand,
    logical_deductions: Value,
    convention_inferences: Value,
    search: Value,
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
                "finessed": card.finessed,
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

fn search_json(table_id: u64, best: &BestMove) -> Value {
    let common = json!({
        "convention": best.convention.id(),
        "profile": best.convention.profile().map(|profile| profile.to_string()),
        "rulesetRevision": best.convention.ruleset_revision(),
        "objective": best.objective.to_string(),
    });
    let details = match &best.details {
        SearchDetails::Ismcts(result) => json!({
            "mode": "ismcts",
            "iterations": result.iterations,
            "rootActions": result.root_actions.iter().map(|statistics| json!({
                "action": HanabiLiveActionCommand::from_engine_action(table_id, statistics.action),
                "selected": statistics.action == best.action,
                "visits": statistics.visits,
                "availability": statistics.availability,
                "meanScore": statistics.mean_score,
                "meanRawScore": statistics.mean_raw_score,
                "meanUtility": statistics.mean_utility,
                "perfectRate": statistics.perfect_rate,
                "meanScoreCeiling": statistics.mean_score_ceiling,
                "meanClueActions": statistics.mean_clue_actions,
                "meanClueEfficiency": statistics.mean_clue_efficiency,
                "meanTempoClues": statistics.mean_tempo_clues,
                "meanCriticalDiscards": statistics.mean_critical_discards,
                "meanBottomDeckRisk": statistics.mean_bottom_deck_risk,
                "meanClueDebt": statistics.mean_clue_debt,
                "meanPredictableTurns": statistics.mean_predictable_turns,
                "prior": statistics.prior,
                "principalVariation": statistics.principal_variation.iter().map(|action| {
                    HanabiLiveActionCommand::from_engine_action(table_id, *action)
                }).collect::<Vec<_>>(),
                "strikeoutRate": statistics.strikeout_rate,
                "minScore": statistics.min_score,
                "maxScore": statistics.max_score,
            })).collect::<Vec<_>>(),
        }),
        SearchDetails::Flat(evaluations) => json!({
            "mode": "flat",
            "rootActions": evaluations.iter().map(|evaluation| json!({
                "action": HanabiLiveActionCommand::from_engine_action(table_id, evaluation.action),
                "selected": evaluation.action == best.action,
                "samples": evaluation.samples,
                "meanScore": evaluation.mean_score,
                "meanRawScore": evaluation.mean_raw_score,
                "meanUtility": evaluation.mean_utility,
                "perfectRate": evaluation.perfect_rate,
                "meanScoreCeiling": evaluation.mean_score_ceiling,
                "meanClueActions": evaluation.mean_clue_actions,
                "meanClueEfficiency": evaluation.mean_clue_efficiency,
                "meanTempoClues": evaluation.mean_tempo_clues,
                "meanCriticalDiscards": evaluation.mean_critical_discards,
                "meanBottomDeckRisk": evaluation.mean_bottom_deck_risk,
                "meanClueDebt": evaluation.mean_clue_debt,
                "meanPredictableTurns": evaluation.mean_predictable_turns,
                "principalVariation": evaluation.principal_variation.iter().map(|action| {
                    HanabiLiveActionCommand::from_engine_action(table_id, *action)
                }).collect::<Vec<_>>(),
                "scoreVariance": evaluation.score_variance,
                "strikeoutRate": evaluation.strikeout_rate,
                "minScore": evaluation.min_score,
                "maxScore": evaluation.max_score,
            })).collect::<Vec<_>>(),
        }),
    };
    let mut combined = common;
    combined
        .as_object_mut()
        .expect("search metadata is an object")
        .extend(
            details
                .as_object()
                .expect("search details are an object")
                .clone(),
        );
    combined
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
