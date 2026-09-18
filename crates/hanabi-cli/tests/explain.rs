use std::{path::PathBuf, process::Command};

fn command() -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_hanabi-engine"));
    command
        .arg("analyze")
        .arg(
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../hanabi-protocol/tests/fixtures/game-p4v0s1.json"),
        )
        .args(["--turn", "0", "--exact-world-limit", "1"]);
    command
}

#[test]
fn explanation_json_is_clean_and_includes_the_fixture_outside_line_limit() {
    let output = command()
        .args(["--format", "json", "--lines", "1"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["schemaVersion"], 2);
    assert_eq!(report["completedActions"], 0);
    assert_eq!(report["hanabLiveTurn"], 1);
    let lines = report["lines"].as_array().unwrap();
    assert!(lines[0]["selected"].as_bool().unwrap());
    assert!(lines.iter().any(|line| line["fixtureAction"] == true));
    assert!(lines.len() <= 2);
    assert!(
        report["candidates"]
            .as_array()
            .unwrap()
            .iter()
            .any(|c| c["fixtureAction"] == true)
    );
    assert!(report["replayLink"].as_str().unwrap().ends_with("#1"));
    assert!(report["planning"]["comparisons"].is_array());
}

#[test]
fn text_explanation_and_option_errors_are_explicit() {
    let output = command().arg("--explain").output().unwrap();
    assert!(output.status.success());
    let text = String::from_utf8(output.stdout).unwrap();
    for expected in [
        "Hanab Live turn 1 (after 0 completed actions)",
        "Candidate actions",
        "Recorded pairwise",
        "Stop:",
        "not inevitable losses",
    ] {
        assert!(text.contains(expected), "missing {expected}: {text}");
    }
    for args in [["--lines", "0"], ["--lines", "-1"], ["--format", "yaml"]] {
        assert!(!command().args(args).output().unwrap().status.success());
    }
}

#[test]
fn h_group_explanation_retains_scores_rejections_and_actor_domains() {
    let output = command()
        .args([
            "--turn",
            "2",
            "--convention",
            "h-group",
            "--h-group-level",
            "max",
            "--format",
            "json",
            "--lines",
            "3",
        ])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let candidates = report["candidates"].as_array().unwrap();
    assert!(
        candidates
            .iter()
            .any(|c| c["status"] == "rejected" && c["rejectionReason"].is_string())
    );
    for c in candidates
        .iter()
        .filter(|c| c["clueScore"].is_number() && c["priority"].is_number())
    {
        assert_eq!(
            c["priority"].as_i64().unwrap(),
            c["clueScore"].as_i64().unwrap() + c["schedulingAdjustment"].as_i64().unwrap()
        );
        assert!(c["scoreComponents"]["base"].is_number());
    }
    assert!(candidates.iter().any(|c| c["scoreComponents"].is_object()));
    assert!(
        candidates
            .iter()
            .any(|c| c["semanticEvidence"]["documentation"].is_string())
    );
    assert!(
        report["projectedDecisions"]
            .as_array()
            .is_some_and(|d| !d.is_empty())
    );
    assert!(
        report["board"]["hands"][usize::try_from(report["actor"].as_u64().unwrap()).unwrap()]["cards"]
            .as_array()
            .unwrap()
            .iter()
            .all(|c| c["visibleIdentity"].is_null())
    );
    for d in report["projectedDecisions"].as_array().unwrap() {
        assert!(d["selected"].is_string());
        assert!(d["candidates"].as_array().is_some_and(|c| !c.is_empty()));
        assert!(d["knowledge"].is_object());
        assert!(d["alternatives"].is_array());
        for alternative in d["alternatives"].as_array().unwrap() {
            assert!(alternative["projection"]["steps"].is_array());
        }
    }
    let lines = report["lines"].as_array().unwrap();
    assert!(lines.len() >= 3);
    assert!(
        lines
            .iter()
            .flat_map(|l| l["projection"]["steps"].as_array().unwrap())
            .any(|s| s["interpretedIdentities"].is_array())
    );
    assert!(
        lines
            .iter()
            .all(|l| l["projection"]["frontier"].is_string())
    );
}

#[test]
fn explicitly_requested_candidate_and_live_turn_are_supported() {
    let output = command()
        .args([
            "--live-turn",
            "1",
            "--format",
            "json",
            "--lines",
            "1",
            "--candidate",
            "blue:Bob",
        ])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["hanabLiveTurn"], 1);
    assert!(
        report["candidates"]
            .as_array()
            .unwrap()
            .iter()
            .any(|c| c["selector"] == "blue:Bob" && c["requested"] == true)
    );
    assert!(
        report["lines"]
            .as_array()
            .unwrap()
            .iter()
            .any(|c| c["action"]["type"] == 2
                && c["action"]["target"] == 1
                && c["action"]["value"] == 3)
    );
    let output = command()
        .args(["--format", "json", "--lines", "all"])
        .output()
        .unwrap();
    let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(
        report["lines"].as_array().unwrap().len(),
        report["candidates"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|c| c["status"] == "evaluated")
            .count()
    );
    for args in [["--candidate", "purple:Nobody"], ["--live-turn", "0"]] {
        assert!(!command().args(args).output().unwrap().status.success());
    }
}
