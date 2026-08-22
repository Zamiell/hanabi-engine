use std::{path::PathBuf, process::Command};

#[test]
fn help_describes_turn_semantics_and_search_modes() {
    let output = Command::new(env!("CARGO_BIN_EXE_hanabi-engine"))
        .arg("--help")
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("turn 0 is the initial deal"));
    assert!(stdout.contains("--mode <ismcts|flat>"));
    assert!(stdout.contains("--convention <none|h-group>"));
    assert!(stdout.contains("--h-group-level <1-25|max>"));
    assert!(stdout.contains("hanabi-engine benchmark"));
    assert!(stdout.contains("versioned JSON report"));
}

#[test]
fn rejects_an_unregistered_convention() {
    let output = Command::new(env!("CARGO_BIN_EXE_hanabi-engine"))
        .args([
            "analyze",
            "unused.json",
            "--turn",
            "0",
            "--convention",
            "rainbow",
        ])
        .output()
        .unwrap();

    assert!(!output.status.success());
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(stderr.contains("unknown convention \"rainbow\"; expected none or h-group"));
}

#[test]
fn validates_h_group_profile_options_before_reading_a_replay() {
    let missing = Command::new(env!("CARGO_BIN_EXE_hanabi-engine"))
        .args([
            "analyze",
            "unused.json",
            "--turn",
            "0",
            "--convention",
            "h-group",
        ])
        .output()
        .unwrap();
    assert!(!missing.status.success());
    assert!(
        String::from_utf8(missing.stderr)
            .unwrap()
            .contains("--h-group-level is required when --convention h-group")
    );

    let irrelevant = Command::new(env!("CARGO_BIN_EXE_hanabi-engine"))
        .args([
            "analyze",
            "unused.json",
            "--turn",
            "0",
            "--h-group-level",
            "5",
        ])
        .output()
        .unwrap();
    assert!(!irrelevant.status.success());
    assert!(
        String::from_utf8(irrelevant.stderr)
            .unwrap()
            .contains("--h-group-level requires --convention h-group")
    );

    let out_of_range = Command::new(env!("CARGO_BIN_EXE_hanabi-engine"))
        .args([
            "analyze",
            "unused.json",
            "--turn",
            "0",
            "--convention",
            "h-group",
            "--h-group-level",
            "26",
        ])
        .output()
        .unwrap();
    assert!(!out_of_range.status.success());
    assert!(
        String::from_utf8(out_of_range.stderr)
            .unwrap()
            .contains("expected 1 through 25, or max")
    );
}

#[test]
fn analyzes_a_real_hanabi_live_prefix() {
    let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../../hanabi-live/packages/client/test_data/no_variant.json");
    if !fixture.exists() {
        return;
    }

    let output = Command::new(env!("CARGO_BIN_EXE_hanabi-engine"))
        .args([
            "analyze",
            fixture.to_str().unwrap(),
            "--turn",
            "17",
            "--iterations",
            "8",
            "--seed",
            "42",
        ])
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("Turn: 17  Actor: Cathy (P2)"));
    assert!(stdout.contains("Convention: none"));
    assert!(stdout.contains("Search: ISMCTS, 8 iterations, seed 42"));
    assert!(stdout.contains("Visits"));
    assert!(stdout.contains("Official"));
    assert!(stdout.contains("Raw"));
    assert!(stdout.contains("Utility"));
    assert!(stdout.contains("slot 1 is newest"));

    let flat = Command::new(env!("CARGO_BIN_EXE_hanabi-engine"))
        .args([
            "analyze",
            fixture.to_str().unwrap(),
            "--turn",
            "17",
            "--mode",
            "flat",
            "--samples",
            "2",
            "--convention",
            "none",
            "--seed",
            "42",
        ])
        .output()
        .unwrap();
    assert!(
        flat.status.success(),
        "{}",
        String::from_utf8_lossy(&flat.stderr)
    );
    let stdout = String::from_utf8(flat.stdout).unwrap();
    assert!(stdout.contains("Search: flat Monte Carlo, 2 samples/action"));
    assert!(stdout.contains("Official"));
    assert!(stdout.contains("Raw"));
    assert!(stdout.contains("Utility"));
    assert!(stdout.contains("Variance"));

    let h_group = Command::new(env!("CARGO_BIN_EXE_hanabi-engine"))
        .args([
            "analyze",
            fixture.to_str().unwrap(),
            "--turn",
            "17",
            "--iterations",
            "1",
            "--convention",
            "h-group",
            "--h-group-level",
            "5",
        ])
        .output()
        .unwrap();
    assert!(
        h_group.status.success(),
        "{}",
        String::from_utf8_lossy(&h_group.stderr)
    );
    let stdout = String::from_utf8(h_group.stdout).unwrap();
    assert!(stdout.contains("Convention: h-group (level 5)"));
    assert!(
        stdout.contains("Convention ruleset revision: 1ef83242d71c62f2db6422f09e83abddba9611dd")
    );
}

#[test]
fn benchmarks_both_search_modes_with_reproducible_trials() {
    let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../../hanabi-live/packages/client/test_data/no_variant.json");
    if !fixture.exists() {
        return;
    }

    let output = Command::new(env!("CARGO_BIN_EXE_hanabi-engine"))
        .args([
            "benchmark",
            fixture.to_str().unwrap(),
            "--turn",
            "17",
            "--trials",
            "2",
            "--convention",
            "none",
            "--iterations",
            "8",
            "--samples",
            "2",
            "--seed",
            "42",
        ])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );

    let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["schema_version"], 4);
    assert_eq!(report["policy"], "convention_agnostic");
    assert_eq!(report["convention"], "none");
    assert!(report["convention_profile"].is_null());
    assert!(report["convention_ruleset_revision"].is_null());
    assert_eq!(report["base_seed"], 42);
    let position = &report["positions"][0];
    assert_eq!(position["turn"], 17);
    assert_eq!(position["actor"], "Cathy");
    assert_eq!(position["starting_score"], 7);

    let searches = position["searches"].as_array().unwrap();
    assert_eq!(searches.len(), 2);
    for (search, mode, work_units, raw_scores, expected_diagnostics) in [
        (&searches[0], "ismcts", 8, [10.0, 10.0], [8, 0, 8, 8, 8, 1]),
        (&searches[1], "flat", 16, [10.5, 8.5], [2, 16, 0, 16, 16, 1]),
    ] {
        assert_eq!(search["mode"], mode);
        assert_eq!(search["trial_count"], 2);
        assert_eq!(search["action_stability"], 0.5);
        assert!(
            search["aggregate_throughput_per_second"]
                .as_f64()
                .is_some_and(|value| value > 0.0)
        );
        let trials = search["trials"].as_array().unwrap();
        assert_eq!(trials[0]["seed"], 42);
        assert_eq!(trials[1]["seed"], 43);
        assert_eq!(trials[0]["selected_action"]["key"], "play:22");
        assert_eq!(trials[1]["selected_action"]["key"], "play:20");
        for (trial, raw_score) in trials.iter().zip(raw_scores) {
            assert_eq!(trial["mean_official_score"], 0.0);
            assert_eq!(trial["mean_raw_score"], raw_score);
            assert_eq!(trial["mean_utility"], raw_score);
            assert_eq!(trial["strikeout_rate"], 1.0);
            assert_eq!(trial["work_units"], work_units);
            let diagnostics = &trial["diagnostics"];
            for (field, expected) in [
                ("worlds_sampled", expected_diagnostics[0]),
                ("candidate_state_clones", expected_diagnostics[1]),
                ("tree_nodes_expanded", expected_diagnostics[2]),
                ("search_actions_applied", expected_diagnostics[3]),
                ("rollouts", expected_diagnostics[4]),
                ("max_tree_depth", expected_diagnostics[5]),
            ] {
                assert_eq!(diagnostics[field], expected);
            }
            assert!(
                diagnostics["rollout_turns"].as_u64().unwrap()
                    > diagnostics["rollouts"].as_u64().unwrap()
            );
            let timing = &diagnostics["timing_seconds"];
            let accounted = timing["sampling"].as_f64().unwrap()
                + timing["tree"].as_f64().unwrap()
                + timing["rollout"].as_f64().unwrap();
            assert!((timing["total"].as_f64().unwrap() - accounted).abs() < 1e-9);
            let rollout_accounted = timing["rollout_observation"].as_f64().unwrap()
                + timing["rollout_deduction"].as_f64().unwrap()
                + timing["rollout_policy"].as_f64().unwrap()
                + timing["rollout_apply"].as_f64().unwrap()
                + timing["rollout_other"].as_f64().unwrap();
            assert!((timing["rollout"].as_f64().unwrap() - rollout_accounted).abs() < 1e-9);
        }
    }
}

#[test]
fn benchmark_reports_structured_h_group_profile_and_revision() {
    let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../../hanabi-live/packages/client/test_data/no_variant.json");
    if !fixture.exists() {
        return;
    }

    let output = Command::new(env!("CARGO_BIN_EXE_hanabi-engine"))
        .args([
            "benchmark",
            fixture.to_str().unwrap(),
            "--turn",
            "17",
            "--trials",
            "1",
            "--iterations",
            "1",
            "--samples",
            "1",
            "--convention",
            "h-group",
            "--h-group-level",
            "max",
        ])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );

    let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["policy"], "h_group");
    assert_eq!(report["convention"], "h-group");
    assert_eq!(report["schema_version"], 4);
    assert_eq!(report["convention_profile"]["level"], 26);
    assert_eq!(
        report["convention_ruleset_revision"],
        "1ef83242d71c62f2db6422f09e83abddba9611dd"
    );
}
