#[test]
fn level_one_policy_can_roll_a_game_to_completion() {
    let convention =
        crate::SupportedConvention::HGroup(HGroupProfile::Level(crate::HGroupLevel::Level1));
    for players in 2..=5 {
        let mut deck = standard_deck();
        deck.rotate_left(usize::from(players) * 3);
        let state = FullState::new_standard(players, deck).unwrap();
        let outcome = continuation_to_terminal(state, convention).unwrap();
        assert!(outcome.turns() > 0);
        assert!(outcome.turns() < MAX_TEST_CONTINUATION_TURNS);
    }
}

fn profile_rolls_to_completion(profile: HGroupProfile) {
    let mut deck = standard_deck();
    deck.rotate_left(11);
    let state = FullState::new_standard(3, deck).unwrap();
    let convention = crate::SupportedConvention::HGroup(profile);
    let outcome = continuation_to_terminal(state, convention)
        .unwrap_or_else(|error| panic!("{profile} continuation failed: {error}"));
    assert!(outcome.turns() > 0, "{profile}");
    assert!(outcome.turns() < MAX_TEST_CONTINUATION_TURNS, "{profile}");
}

macro_rules! profile_rollout_test {
    ($name:ident, $profile:expr) => {
        #[test]
        #[ignore = "exhaustive H-Group profile matrix; run scripts/check-exhaustive.sh"]
        fn $name() {
            profile_rolls_to_completion($profile);
        }
    };
}

macro_rules! representative_profile_rollout_test {
    ($name:ident, $profile:expr) => {
        #[test]
        fn $name() {
            profile_rolls_to_completion($profile);
        }
    };
}

representative_profile_rollout_test!(
    representative_profile_rollout_level_01,
    HGroupProfile::Level(HGroupLevel::Level1)
);
representative_profile_rollout_test!(
    representative_profile_rollout_level_10,
    HGroupProfile::Level(HGroupLevel::Level10)
);
representative_profile_rollout_test!(representative_profile_rollout_max, HGroupProfile::Max);

profile_rollout_test!(profile_rollout_level_01, HGroupProfile::Level(HGroupLevel::Level1));
profile_rollout_test!(profile_rollout_level_02, HGroupProfile::Level(HGroupLevel::Level2));
profile_rollout_test!(profile_rollout_level_03, HGroupProfile::Level(HGroupLevel::Level3));
profile_rollout_test!(profile_rollout_level_04, HGroupProfile::Level(HGroupLevel::Level4));
profile_rollout_test!(profile_rollout_level_05, HGroupProfile::Level(HGroupLevel::Level5));
profile_rollout_test!(profile_rollout_level_06, HGroupProfile::Level(HGroupLevel::Level6));
profile_rollout_test!(profile_rollout_level_07, HGroupProfile::Level(HGroupLevel::Level7));
profile_rollout_test!(profile_rollout_level_08, HGroupProfile::Level(HGroupLevel::Level8));
profile_rollout_test!(profile_rollout_level_09, HGroupProfile::Level(HGroupLevel::Level9));
profile_rollout_test!(profile_rollout_level_10, HGroupProfile::Level(HGroupLevel::Level10));
profile_rollout_test!(profile_rollout_level_11, HGroupProfile::Level(HGroupLevel::Level11));
profile_rollout_test!(profile_rollout_level_12, HGroupProfile::Level(HGroupLevel::Level12));
profile_rollout_test!(profile_rollout_level_13, HGroupProfile::Level(HGroupLevel::Level13));
profile_rollout_test!(profile_rollout_level_14, HGroupProfile::Level(HGroupLevel::Level14));
profile_rollout_test!(profile_rollout_level_15, HGroupProfile::Level(HGroupLevel::Level15));
profile_rollout_test!(profile_rollout_level_16, HGroupProfile::Level(HGroupLevel::Level16));
profile_rollout_test!(profile_rollout_level_17, HGroupProfile::Level(HGroupLevel::Level17));
profile_rollout_test!(profile_rollout_level_18, HGroupProfile::Level(HGroupLevel::Level18));
profile_rollout_test!(profile_rollout_level_19, HGroupProfile::Level(HGroupLevel::Level19));
profile_rollout_test!(profile_rollout_level_20, HGroupProfile::Level(HGroupLevel::Level20));
profile_rollout_test!(profile_rollout_level_21, HGroupProfile::Level(HGroupLevel::Level21));
profile_rollout_test!(profile_rollout_level_22, HGroupProfile::Level(HGroupLevel::Level22));
profile_rollout_test!(profile_rollout_level_23, HGroupProfile::Level(HGroupLevel::Level23));
profile_rollout_test!(profile_rollout_level_24, HGroupProfile::Level(HGroupLevel::Level24));
profile_rollout_test!(profile_rollout_level_25, HGroupProfile::Level(HGroupLevel::Level25));
profile_rollout_test!(profile_rollout_max, HGroupProfile::Max);
