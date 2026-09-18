use hanabi_protocol::{HanabiLiveReplay, replay_link};

#[test]
fn replay_link_preserves_seed_hyphens_and_empty_seed_field() {
    let mut replay = HanabiLiveReplay::from_json(r#"{"seed":"p4v0s3","actions":[]}"#).unwrap();
    assert_eq!(replay.seed.as_deref(), Some("p4v0s3"));
    // Codec-only metadata checks; a named custom seed is not regenerated here.
    for seed in [
        "legacy-1-p4v0s3",
        "a-long-seed-with-more-than-twenty-characters",
        "",
    ] {
        replay.seed = Some(seed.to_owned());
        assert!(
            replay_link(&replay, 1)
                .unwrap()
                .ends_with(&format!(",{seed}#1"))
        );
    }
    replay.seed = None;
    assert!(replay_link(&replay, 1).unwrap().ends_with(",0,#1"));
}

#[test]
fn replay_link_rejects_seed_url_delimiters() {
    let mut replay = HanabiLiveReplay::from_json(r#"{"seed":"p4v0s3","actions":[]}"#).unwrap();
    for seed in ["bad,seed", "bad#seed", "bad?seed", "bad/seed", "bad seed"] {
        replay.seed = Some(seed.to_owned());
        assert!(replay_link(&replay, 1).is_err());
    }
}
