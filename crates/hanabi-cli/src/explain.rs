//! Typed reports built from retained evidence, never a second inference run.
use std::{collections::BTreeMap, process::Command, time::Instant};

use hanabi_core::{Action, PlayerView};
use hanabi_protocol::{HanabiLiveActionCommand, HanabiLiveReplay};
use hanabi_search::{
    CandidateComparison, ConventionAnalysis, IdentitySet, PlannerActionEvaluation, PlannerConfig,
    PositionAnalysis, ProjectedDecision, analyze_position, capture_decisions,
};
use serde::Serialize;
use serde_json::{Value, json};

use crate::{AnalyzeArguments, CliError, action_label};

#[derive(Clone, Copy)]
pub(crate) enum OutputFormat {
    Text,
    Json,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Report {
    schema_version: u8,
    seed: Option<String>,
    completed_actions: u32,
    hanab_live_turn: u32,
    actor: usize,
    players: Vec<String>,
    replay_link: Option<String>,
    convention: String,
    objective: String,
    elapsed_seconds: f64,
    configuration: Value,
    engine: Value,
    board: Value,
    knowledge: Value,
    best_action: Value,
    best_label: String,
    fixture_action: Option<Value>,
    fixture_label: Option<String>,
    line_order: &'static str,
    uncertainty: &'static str,
    candidates: Vec<CandidateReport>,
    planning: Value,
    comparisons: Vec<ComparisonReport>,
    lines: Vec<LineReport>,
    projected_decisions: Vec<DecisionReport>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct CandidateReport {
    selector: String,
    action: Value,
    label: String,
    status: &'static str,
    selected: bool,
    fixture_action: bool,
    requested: bool,
    interpretation: Option<String>,
    priority: Option<i32>,
    score_components: Option<BTreeMap<&'static str, i32>>,
    clue_score: Option<u16>,
    scheduling_adjustment: Option<i32>,
    rejection_reason: Option<String>,
    rejection_evidence: Value,
    semantic_evidence: Value,
    exact: Value,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ComparisonReport {
    left_label: String,
    right_label: String,
    preferred_label: String,
    reason: String,
    endpoint: String,
    in_cycle: bool,
    evidence: Value,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct LineReport {
    label: String,
    action: Value,
    selected: bool,
    fixture_action: bool,
    forced_root: bool,
    projection: Value,
    symbolic_line: Value,
    exact: Value,
    #[serde(skip)]
    evaluation: PlannerActionEvaluation,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct DecisionReport {
    id: usize,
    root: Option<Value>,
    root_label: Option<String>,
    turn: u32,
    actor: usize,
    board: Value,
    knowledge: Value,
    selected: Option<String>,
    forced: bool,
    candidates: Vec<CandidateReport>,
    comparisons: Vec<ComparisonReport>,
    alternatives: Vec<Value>,
    selection: &'static str,
}

const SELECTION: &str = "Endpoint evidence first; incomparable/equivalent endpoints use policy fallback. The planner resolves preference cycles with its evidence graph and stable fallback, not pairwise-win count.";
const UNCERTAINTY: &str = "Projected choices are not forced. Pending unknown discards are possible risks, not inevitable losses. Leaf-policy steps do not run another strategic search. Missing exact principal variations are not reconstructed.";

pub(crate) fn run(
    args: &AnalyzeArguments,
    replay: &HanabiLiveReplay,
    view: &PlayerView,
) -> Result<(), CliError> {
    let legal = view.legal_actions();
    let requested = args.candidates.iter().map(|requested| {
        legal.iter().copied().find(|action| selector(*action, &replay.players).eq_ignore_ascii_case(requested))
            .ok_or_else(|| CliError::Usage(format!("{requested:?} is not a legal candidate here; use e.g. purple:Donald, 3:Alice, play:17, discard:12")))
    }).collect::<Result<Vec<_>, _>>()?;
    let started = Instant::now();
    let (analysis, decisions) = capture_decisions(|| {
        analyze_position(
            view,
            args.convention,
            PlannerConfig {
                objective: args.objective,
                exact_world_limit: args.exact_world_limit,
                exact_node_limit: args.exact_node_limit,
            },
        )
    });
    let analysis = analysis.map_err(CliError::AnalyzePosition)?;
    let report = report(
        args,
        replay,
        view,
        &analysis,
        &decisions,
        &requested,
        started.elapsed().as_secs_f64(),
    );
    match args.format {
        OutputFormat::Json => println!(
            "{}",
            serde_json::to_string_pretty(&report).expect("serializable report")
        ),
        OutputFormat::Text => print_text(&report),
    }
    Ok(())
}

fn wire(action: Action) -> Value {
    json!(HanabiLiveActionCommand::from_engine_action(0, action))
}

fn selector(action: Action, players: &[String]) -> String {
    match action {
        Action::Play(card) => format!("play:{}", card.index()),
        Action::Discard(card) => format!("discard:{}", card.index()),
        Action::Clue { target, clue } => {
            let clue = match clue {
                hanabi_core::Clue::Suit(s) => s.to_string(),
                hanabi_core::Clue::Rank(r) => r.to_string(),
            };
            format!("{clue}:{}", players[target.index()])
        }
    }
}

fn fixture_action(replay: &HanabiLiveReplay, view: &PlayerView) -> Option<Action> {
    let recorded = replay.actions.get(usize::try_from(view.turn).ok()?)?;
    view.legal_actions().into_iter().find(|action| {
        let command = HanabiLiveActionCommand::from_engine_action(0, *action);
        command.action_type == recorded.action_type
            && command.target == recorded.target
            && command.value.is_none_or(|value| value == recorded.value)
    })
}

fn identities(domain: IdentitySet) -> String {
    domain
        .iter()
        .map(|card| {
            format!(
                "{}{}",
                match card.suit {
                    hanabi_core::Suit::Red => "r",
                    hanabi_core::Suit::Yellow => "y",
                    hanabi_core::Suit::Green => "g",
                    hanabi_core::Suit::Blue => "b",
                    hanabi_core::Suit::Purple => "p",
                },
                card.rank.number()
            )
        })
        .collect::<Vec<_>>()
        .join("/")
}

fn board(view: &PlayerView) -> Value {
    json!({"observer": view.observer.index(), "turn": view.turn + 1, "clueTokens": view.clue_tokens,
        "strikes": view.strikes, "deckSize": view.deck_size, "finalTurnsRemaining": view.final_turns_remaining,
        "stacks": view.play_stacks.iter().map(Vec::len).collect::<Vec<_>>(),
        "discards": view.discard_pile.iter().map(|(id, card)| json!({"card":id.index(), "identity": identities(IdentitySet::singleton(*card))})).collect::<Vec<_>>(),
        "hands": view.hands.iter().enumerate().map(|(player, hand)| json!({"player":player,
            "cards":hand.iter().enumerate().map(|(index, card)| json!({"card":card.id.index(), "slot":hand.len()-index,
                "visibleIdentity": card.identity.map(|c| identities(IdentitySet::singleton(c))),
                "clueFacts": format!("{:?}", card.clues)})).collect::<Vec<_>>()})).collect::<Vec<_>>()})
}

fn knowledge(analysis: &ConventionAnalysis) -> Value {
    crate::live_action::convention_inferences_json(analysis.inferences.clone())
}

fn exact(value: &PlannerActionEvaluation) -> Value {
    value.exact.map_or(Value::Null, |e| {
        json!({"worlds":e.worlds, "perfectRate":e.perfect_rate(),
        "expectedScore":e.expected_score(), "strikeoutRate":e.strikeout_rate()})
    })
}

// Documentation URLs come from the enum's authoritative source annotations,
// avoiding a second hand-maintained convention catalog.
fn convention_url(kind: hanabi_search::HGroupMoveKind) -> Option<String> {
    let name = format!("{kind:?},");
    include_str!("../../hanabi-search/src/h_group.rs")
        .lines()
        .collect::<Vec<_>>()
        .windows(2)
        .find_map(|lines| {
            if lines[1].trim() != name {
                return None;
            }
            let (_, rest) = lines[0].split_once("https://hanabi.github.io/")?;
            Some(format!(
                "https://hanabi.github.io/{}",
                rest.split(')').next()?
            ))
        })
}

#[allow(clippy::too_many_arguments)]
fn candidates(
    view: &PlayerView,
    players: &[String],
    convention: &ConventionAnalysis,
    evaluations: &[PlannerActionEvaluation],
    selected: Option<Action>,
    fixture: Option<Action>,
    requested: &[Action],
) -> Vec<CandidateReport> {
    view.legal_actions().into_iter().map(|action| {
        let evaluated = evaluations.iter().find(|c| c.action == action);
        let admitted = convention.actions.iter().find(|c| c.action == action);
        let rejected = convention.rejected_actions.iter().find(|c| c.action == action);
        let clue = convention.clue_explanations.iter().find(|c| c.action == action);
        let status = if evaluated.is_some() { "evaluated" } else if rejected.is_some() { "rejected" }
            else if convention.forced_action.is_some_and(|forced| forced != action) && admitted.is_some() { "excludedByForcedAction" }
            else if admitted.is_some() { "admitted" } else { "notAdmitted" };
        CandidateReport {
            selector: selector(action, players), action: wire(action), label: action_label(view, players, action), status,
            selected: Some(action) == selected, fixture_action: Some(action) == fixture, requested: requested.contains(&action),
            interpretation: clue.and_then(|c| c.kind).map(|kind| format!("{kind:?}"))
                .or_else(|| admitted.map(|c| format!("{:?}", c.reason))),
            priority: evaluated.map(|c| c.preference.within_category()).or_else(|| admitted.map(|c| c.preference.within_category())),
            score_components: clue.map(|c| c.score_components.iter().copied().collect()),
            clue_score: clue.map(|c| c.score), scheduling_adjustment: clue.zip(admitted).map(|(c,a)| a.preference.within_category()-i32::from(c.score)),
            rejection_reason: rejected.map(|r| format!("{:?}",r.reason)),
            rejection_evidence: rejected.map_or(Value::Null,|rejection| {
                let Action::Clue {target,clue}=action else {return Value::Null;};
                let touched=view.hands[target.index()].iter().filter(|card|card.identity.is_some_and(|identity|clue.matches(identity)))
                    .map(|card|json!({"card":card.id.index(),"alreadyHasThisClue":card.clues.has_positive_clue(clue)})).collect::<Vec<_>>();
                json!({"source":"post-admission exclusion classification", "touched":touched,"explanation":match rejection.reason {
                    hanabi_search::ConventionRejectionReason::NoNewInformation => "No touched card gains a new positive clue fact.",
                    hanabi_search::ConventionRejectionReason::NoFocus => "The convention focus rules did not select a focus among these touched cards.",
                    hanabi_search::ConventionRejectionReason::RepeatsKnownIdentity => "The selected focus is already gotten and its exact identity is established.",
                    hanabi_search::ConventionRejectionReason::RedundantOutcome => "The clue repeats an already-scheduled outcome or stomps an unresolved visible connection.",
                    hanabi_search::ConventionRejectionReason::UnsafeConnection => "The proposed connection failed convention safety validation.",
                    hanabi_search::ConventionRejectionReason::NoConventionMeaning => "No semantic generator admitted this clue; the engine did not retain a more specific proof of exclusion.",
                }})
            }),
            semantic_evidence: clue.map_or(Value::Null, |c| json!({"recognition":c.recognition,
                "recipientInterpretation": c.interpretation.as_ref().map(|i|json!({"focus":i.focus.index(),"focusWasChop":i.focus_was_chop,
                    "kind":format!("{:?}",i.kind),"identities":identities(i.focus_identities),"playIdentities":identities(i.play_identities),
                    "saveIdentities":identities(i.save_identities),"touched":i.touched.iter().map(|id|id.index()).collect::<Vec<_>>()})),
                "connectionSteps":c.connection_steps, "actionCoverage":c.action_coverage, "documentation":c.kind.and_then(convention_url),
                "note":"Recipient knowledge and connection promises are retained with projected decisions; unresolved meanings are not reconstructed."})),
            exact: evaluated.map_or(Value::Null, exact),
        }
    }).collect()
}

fn comparisons(
    view: &PlayerView,
    players: &[String],
    comparisons: &[CandidateComparison],
    evaluations: &[PlannerActionEvaluation],
) -> Vec<ComparisonReport> {
    comparisons.iter().map(|comparison| {
        let left = evaluations.iter().find(|c| c.action == comparison.left);
        let right = evaluations.iter().find(|c| c.action == comparison.right);
        let endpoint = |e: Option<&PlannerActionEvaluation>| e.map(|e| json!({"actions":e.symbolic_line.actions,
            "discards":e.symbolic_line.discards, "stop":format!("{:?}",e.symbolic_line.stop_reason),
            "value":e.symbolic_line.position_value.map(crate::live_action::position_value_json)}));
        let shared = left.zip(right).and_then(|(a,b)| a.projection.checkpoints.iter().rev().find_map(|a| {
            b.projection.checkpoints.iter().find(|b| b.actions == a.actions).map(|b| json!({"actions":a.actions,
                "left":crate::live_action::position_value_json(a.value), "right":crate::live_action::position_value_json(b.value),
                "leftDiscards":a.discards, "rightDiscards":b.discards}))
        }));
        ComparisonReport { left_label:action_label(view,players,comparison.left), right_label:action_label(view,players,comparison.right),
            preferred_label:action_label(view,players,comparison.preferred), reason:format!("{:?}",comparison.reason),
            endpoint:format!("{:?}",comparison.endpoint), in_cycle:comparison.in_cycle,
            evidence:json!({"leftEndpoint":endpoint(left),"rightEndpoint":endpoint(right),"latestSharedCheckpoint":shared,
                "actualBasis": comparison.basis.as_ref().map(|basis|json!({"stage":basis.stage,"horizon":basis.horizon,
                    "clueCostBounds": basis.clue_cost_bounds,
                    "scheduledRefunds": basis.scheduled_refunds,
                    "left":basis.left.iter().map(|c|crate::live_action::position_value_json(c.value)).collect::<Vec<_>>(),
                    "right":basis.right.iter().map(|c|crate::live_action::position_value_json(c.value)).collect::<Vec<_>>() })),
                "note":"Shared checkpoint is context, not necessarily the comparator's decisive checkpoint; reason identifies the rule actually used."}) }
    }).collect()
}

fn revision() -> Value {
    let git = |args: &[&str]| {
        Command::new("git")
            .args(args)
            .current_dir(env!("CARGO_MANIFEST_DIR"))
            .output()
            .ok()
            .filter(|o| o.status.success())
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_owned())
    };
    json!({"version":env!("CARGO_PKG_VERSION"), "sourceCheckoutRevision":git(&["rev-parse","HEAD"]),
        "buildRevision":env!("HANABI_BUILD_REVISION"),"buildDirty":env!("HANABI_BUILD_DIRTY"),
        "sourceCheckoutDirty":git(&["status","--porcelain"]).map(|s|!s.is_empty()),
        "revisionNote":"Checkout provenance at report time, not a claim that an older binary was built from it.",
        "rulesetRevision":hanabi_search::H_GROUP_RULESET_REVISION})
}

#[allow(clippy::too_many_arguments, clippy::too_many_lines)]
fn report(
    args: &AnalyzeArguments,
    replay: &HanabiLiveReplay,
    view: &PlayerView,
    analysis: &PositionAnalysis,
    decisions: &[ProjectedDecision],
    requested: &[Action],
    elapsed: f64,
) -> Report {
    let result = &analysis.planner;
    let convention = &analysis.convention_analysis;
    let fixture = fixture_action(replay, view);
    let mut planning = crate::live_action::planner_details_json(0, result.best_action, result);
    let roots = planning["rootActions"].as_array().expect("root array");
    let mut order = (0..result.root_actions.len()).collect::<Vec<_>>();
    order.sort_by_key(|index| {
        let c = &result.root_actions[*index];
        (
            std::cmp::Reverse(c.action == result.best_action),
            std::cmp::Reverse(
                result
                    .comparisons
                    .iter()
                    .filter(|p| p.preferred == c.action)
                    .count(),
            ),
            std::cmp::Reverse(c.preference.within_category()),
            *index,
        )
    });
    let mut shown = order.into_iter().take(args.lines).collect::<Vec<_>>();
    for (index, c) in result.root_actions.iter().enumerate() {
        if (Some(c.action) == fixture || requested.contains(&c.action)) && !shown.contains(&index) {
            shown.push(index);
        }
    }
    let lines = shown
        .iter()
        .map(|index| {
            let c = &result.root_actions[*index];
            LineReport {
                label: action_label(view, &replay.players, c.action),
                action: wire(c.action),
                selected: c.action == result.best_action,
                fixture_action: Some(c.action) == fixture,
                forced_root: convention.forced_action == Some(c.action),
                projection: roots[*index]["projection"].clone(),
                symbolic_line: roots[*index]["symbolicLine"].clone(),
                exact: roots[*index]["exact"].clone(),
                evaluation: c.clone(),
            }
        })
        .collect();
    let projected_decisions = decisions
        .iter()
        .enumerate()
        .filter(|(_, d)| {
            d.root
                .is_some_and(|a| shown.iter().any(|i| result.root_actions[*i].action == a))
        })
        .map(|(id, d)| DecisionReport {
            id,
            root: d.root.map(wire),
            root_label: d.root.map(|a| action_label(view, &replay.players, a)),
            turn: d.view.turn + 1,
            actor: d.view.observer.index(),
            board: board(&d.view),
            knowledge: knowledge(&d.convention),
            selected: d
                .selected
                .map(|a| action_label(&d.view, &replay.players, a)),
            forced: d.convention.forced_action == d.selected && d.selected.is_some(),
            candidates: candidates(
                &d.view,
                &replay.players,
                &d.convention,
                &d.candidates,
                d.selected,
                None,
                &[],
            ),
            comparisons: comparisons(&d.view, &replay.players, &d.comparisons, &d.candidates),
            alternatives: d.candidates.iter().map(|candidate|json!({"action":wire(candidate.action),
                "label":action_label(&d.view,&replay.players,candidate.action),"selected":Some(candidate.action)==d.selected,
                "projection":crate::live_action::projection_evidence_json(0,&candidate.projection),
                "endpoint":candidate.symbolic_line.position_value.map(crate::live_action::position_value_json),
                "stopReason":format!("{:?}",candidate.symbolic_line.stop_reason)})).collect(),
            selection: SELECTION,
        })
        .collect();
    planning.as_object_mut().unwrap().remove("rootActions");
    planning["selection"] = json!(SELECTION);
    Report {
        schema_version: 2,
        seed: replay.seed.clone(),
        completed_actions: view.turn,
        hanab_live_turn: view.turn + 1,
        actor: view.observer.index(),
        players: replay.players.clone(),
        replay_link: hanabi_protocol::replay_link(replay, usize::try_from(view.turn).unwrap() + 1)
            .ok(),
        convention: args.convention.to_string(),
        objective: args.objective.to_string(),
        elapsed_seconds: elapsed,
        configuration: json!({"exactWorldLimit":args.exact_world_limit,"exactNodeLimit":args.exact_node_limit}),
        engine: revision(),
        board: board(view),
        knowledge: knowledge(convention),
        best_action: wire(result.best_action),
        best_label: action_label(view, &replay.players, result.best_action),
        fixture_action: fixture.map(wire),
        fixture_label: fixture.map(|a| action_label(view, &replay.players, a)),
        line_order: "selected first, then pairwise wins, then priority; not a strict ranking; fixture and requested candidates appended",
        uncertainty: UNCERTAINTY,
        candidates: candidates(
            view,
            &replay.players,
            convention,
            &result.root_actions,
            Some(result.best_action),
            fixture,
            requested,
        ),
        comparisons: comparisons(
            view,
            &replay.players,
            &result.comparisons,
            &result.root_actions,
        ),
        planning,
        lines,
        projected_decisions,
    }
}

fn print_candidate(c: &CandidateReport) {
    println!(
        "{}{} | {} | {} | priority {} | {}",
        if c.selected { "* " } else { "  " },
        c.selector,
        c.status,
        c.interpretation.as_deref().unwrap_or("—"),
        c.priority.map_or_else(|| "—".to_owned(), |n| n.to_string()),
        c.rejection_reason.as_deref().unwrap_or("")
    );
    if let Some(terms) = &c.score_components {
        println!(
            "    {} ; scheduling adjustment {}",
            terms
                .iter()
                .filter(|(_, v)| **v != 0)
                .map(|(k, v)| format!("{k}={v:+}"))
                .collect::<Vec<_>>()
                .join(", "),
            c.scheduling_adjustment.unwrap_or(0)
        );
    }
    if !c.semantic_evidence.is_null() {
        println!("    Meaning evidence: {}", c.semantic_evidence);
    }
    if !c.rejection_evidence.is_null() {
        println!(
            "    {} Touched: {}",
            c.rejection_evidence["explanation"].as_str().unwrap_or(""),
            c.rejection_evidence["touched"]
        );
    }
    if !c.exact.is_null() {
        println!("    Exact outcome: {}", c.exact);
    }
}

fn print_comparison(c: &ComparisonReport) {
    println!(
        "{} vs {} => {}: {}; endpoint {}; cycle={}",
        c.left_label, c.right_label, c.preferred_label, c.reason, c.endpoint, c.in_cycle
    );
    let basis = &c.evidence["actualBasis"];
    if !basis.is_null() {
        println!(
            "    Actual comparison: {} after {} actions",
            basis["stage"], basis["horizon"]
        );
        for left in basis["left"].as_array().unwrap() {
            for right in basis["right"].as_array().unwrap() {
                let changes = left
                    .as_object()
                    .unwrap()
                    .iter()
                    .filter(|(key, value)| **value != right[*key])
                    .map(|(key, value)| format!("{key}: {value} vs {}", right[key]))
                    .collect::<Vec<_>>();
                println!(
                    "    Compared values: {}",
                    if changes.is_empty() {
                        "equal".to_owned()
                    } else {
                        changes.join("; ")
                    }
                );
            }
        }
    }
    for (name, value) in [
        ("left", &c.evidence["leftEndpoint"]),
        ("right", &c.evidence["rightEndpoint"]),
    ] {
        println!(
            "    {name}: after {} actions, score {}, tokens {}, discards {}; stop {}",
            value["actions"],
            value["value"]["score"],
            value["value"]["clues"],
            value["discards"],
            value["stop"]
        );
    }
    if let Some(actions) = c.evidence["latestSharedCheckpoint"]["actions"].as_u64() {
        println!(
            "    Shared checkpoint available after {actions} actions (context, not necessarily decisive)."
        );
    }
}

fn print_text(report: &Report) {
    println!(
        "Hanab Live turn {} (after {} completed actions); actor {}",
        report.hanab_live_turn, report.completed_actions, report.players[report.actor]
    );
    println!(
        "{}\nBest: {}\nFixture: {}",
        report
            .replay_link
            .as_deref()
            .unwrap_or("Replay link unavailable"),
        report.best_label,
        report.fixture_label.as_deref().unwrap_or("none")
    );
    println!(
        "Framework: {}; objective: {}; phase: {}; exact status: {}; {:.3}s",
        report.convention,
        report.objective,
        report.planning["phase"],
        report.planning["exactStatus"],
        report.elapsed_seconds
    );
    print_board(&report.board, &report.players);
    println!("Observer knowledge is included in the JSON report; unknown cards remain unknown.");
    println!("\nCandidate actions (scores are not the final comparison):");
    for c in &report.candidates {
        print_candidate(c);
    }
    println!("\nRecorded pairwise decisions:\n{SELECTION}");
    for c in &report.comparisons {
        print_comparison(c);
    }
    println!(
        "\nLine order: {}\n{}",
        report.line_order, report.uncertainty
    );
    for line in &report.lines {
        println!(
            "\n{} (selected={}, fixture={}, forced root={})",
            line.label, line.selected, line.fixture_action, line.forced_root
        );
        if !line.exact.is_null() {
            println!("Exact outcome: {}", line.exact);
        }
        print_projection(&line.evaluation.projection, &report.players, "  ");
        if let Some(value) = line.evaluation.symbolic_line.position_value {
            println!(
                "  Endpoint: score {}, tokens {}, committed plays {}, secured future plays {}, Save pressure {}",
                value.score,
                value.clues,
                value.committed_future_plays,
                value.secured_future_plays,
                value.save_pressure
            );
        }
        for d in report
            .projected_decisions
            .iter()
            .filter(|d| d.root.as_ref() == Some(&line.action))
        {
            println!(
                "  Decision #{}: T{} {} chose {} (forced={})",
                d.id,
                d.turn,
                report.players[d.actor],
                d.selected.as_deref().unwrap_or("none"),
                d.forced
            );
            for c in d
                .candidates
                .iter()
                .filter(|c| matches!(c.status, "admitted" | "evaluated"))
            {
                println!(
                    "    {} | {} | priority {}{}",
                    c.selector,
                    c.interpretation.as_deref().unwrap_or("—"),
                    c.priority.unwrap_or(0),
                    if c.selected { " [selected]" } else { "" }
                );
            }
            for c in d
                .comparisons
                .iter()
                .filter(|c| Some(&c.preferred_label) == d.selected.as_ref())
            {
                print_comparison(c);
            }
        }
    }
}

fn print_board(board: &Value, players: &[String]) {
    println!(
        "Tokens {}; strikes {}; deck {}; stacks [r,y,g,b,p] {}",
        board["clueTokens"], board["strikes"], board["deckSize"], board["stacks"]
    );
    for (player, hand) in board["hands"].as_array().unwrap().iter().enumerate() {
        let cards = hand["cards"]
            .as_array()
            .unwrap()
            .iter()
            .rev()
            .map(|c| {
                format!(
                    "slot {} #{} {}",
                    c["slot"],
                    c["card"],
                    c["visibleIdentity"].as_str().unwrap_or("?")
                )
            })
            .collect::<Vec<_>>()
            .join("; ");
        println!("  {}: {cards}", players[player]);
    }
}

fn print_projection(
    projection: &hanabi_search::ProjectionEvidence,
    players: &[String],
    indent: &str,
) {
    for step in &projection.steps {
        println!(
            "{indent}T{} {}: {} | interpreted {} | tokens -{} +{} | score +{} | strikes {} | BDR {:?} | Save violation {:?}",
            step.turn + 1,
            players[step.projected.actor.index()],
            selector(step.projected.action, players),
            step.interpreted_identities
                .map_or_else(|| "not recorded".to_owned(), identities),
            step.consequences.clues_spent,
            step.consequences.clues_gained,
            step.consequences.score_gain,
            step.consequences.strikes,
            step.consequences.bottom_deck_risk,
            step.consequences.save_principle_violation
        );
    }
    println!(
        "{indent}Stop: {:?}; tokens {}; pending discard: {:?} (not necessarily forced)",
        projection.frontier, projection.resources.tokens, projection.unresolved_discard
    );
    for branch in &projection.clue_branches {
        println!(
            "{indent}Conditional touches {:?} at T{}:",
            branch.touched,
            branch.turn + 1
        );
        print_projection(&branch.continuation, players, &format!("{indent}  "));
    }
    for branch in &projection.discard_branches {
        println!(
            "{indent}Conditional discard #{} reveals {:?} at T{}:",
            branch.card.index(),
            branch.identity,
            branch.turn + 1
        );
        print_projection(&branch.continuation, players, &format!("{indent}  "));
    }
    if !projection.assumptions.is_empty() {
        println!("{indent}Assumptions: {:?}", projection.assumptions);
    }
    if !projection.alternatives.is_empty() {
        println!("{indent}Alternatives: {:?}", projection.alternatives);
    }
    if !projection.dependencies.is_empty() {
        println!("{indent}Dependencies: {:?}", projection.dependencies);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use hanabi_core::{CardId, PlayerId};
    use hanabi_search::{
        ActionPreference, ComparisonReason, EndpointComparison, ExactActionValue,
        ProjectionEvidence, SymbolicLineOutcome,
    };

    fn preference() -> ActionPreference {
        let replay = HanabiLiveReplay::from_json(include_str!(
            "../../hanabi-protocol/tests/fixtures/game-p4v0s1.json"
        ))
        .unwrap();
        let state = replay.state_at_turn(0).unwrap();
        let view = state.view_for(PlayerId::new(0)).unwrap();
        let deductions = hanabi_search::LogicalDeductions::new(view).unwrap();
        hanabi_search::SupportedConvention::None
            .analyze(&deductions)
            .actions[0]
            .preference
    }

    fn evaluation(action: Action) -> PlannerActionEvaluation {
        PlannerActionEvaluation {
            action,
            preference: preference(),
            certainly_playable: false,
            certainly_useless: false,
            newly_touched: 0,
            immediately_playable_touched: 0,
            critical_touched: 0,
            oldest_card_touched: false,
            symbolic_line: SymbolicLineOutcome::default(),
            projection: ProjectionEvidence::default(),
            exact: None,
        }
    }

    #[test]
    fn exact_outcomes_are_separate_from_symbolic_evidence() {
        let mut candidate = evaluation(Action::Play(CardId::new(0)));
        candidate.exact = Some(ExactActionValue {
            worlds: 4,
            perfect_worlds: 3,
            score_sum: 99,
            strikeout_worlds: 0,
            score_ceiling_sum: 100,
        });
        let value = exact(&candidate);
        assert_eq!(value["perfectRate"], 0.75);
        assert_eq!(value["expectedScore"], 24.75);
        assert!(candidate.projection.steps.is_empty());
    }

    #[test]
    fn reports_preserve_cycles_branches_and_forced_exclusions() {
        // Serialization invariants, not invented convention expectations.
        let replay = HanabiLiveReplay::from_json(include_str!(
            "../../hanabi-protocol/tests/fixtures/game-p4v0s1.json"
        ))
        .unwrap();
        let state = replay.state_at_turn(0).unwrap();
        let view = state.view_for(PlayerId::new(0)).unwrap();
        let actions = view.legal_actions();
        let analysis = ConventionAnalysis {
            forced_action: Some(actions[0]),
            actions: actions
                .iter()
                .take(2)
                .map(|action| hanabi_search::ConventionAction {
                    action: *action,
                    preference: preference(),
                    reason: hanabi_search::ConventionActionReason::ConventionFree,
                })
                .collect(),
            ..ConventionAnalysis::default()
        };
        let mut a = evaluation(actions[0]);
        a.projection
            .clue_branches
            .push(hanabi_search::ClueTouchBranch {
                turn: 1,
                touched: vec![CardId::new(4)],
                outcome: SymbolicLineOutcome::default(),
                continuation: ProjectionEvidence::default(),
            });
        let b = evaluation(actions[1]);
        let rows = candidates(
            &view,
            &replay.players,
            &analysis,
            &[a.clone()],
            Some(actions[0]),
            None,
            &[],
        );
        assert_eq!(
            rows.iter()
                .find(|c| c.action == wire(actions[1]))
                .unwrap()
                .status,
            "excludedByForcedAction"
        );
        let comparison = CandidateComparison {
            left: a.action,
            right: b.action,
            preferred: a.action,
            endpoint: EndpointComparison::Incomparable,
            reason: ComparisonReason::StableOrder,
            in_cycle: true,
            basis: None,
        };
        let rows = comparisons(&view, &replay.players, &[comparison], &[a.clone(), b]);
        assert!(rows[0].in_cycle);
        let evidence = crate::live_action::projection_evidence_json(0, &a.projection);
        assert_eq!(evidence["clueBranches"][0]["turn"], 2);
        assert_eq!(evidence["clueBranches"][0]["touched"][0], 4);
        assert!(evidence["steps"].as_array().unwrap().is_empty());
    }
}
