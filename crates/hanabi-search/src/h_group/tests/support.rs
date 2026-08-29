struct TestOutcome {
    actions: Vec<Action>,
}

impl TestOutcome {
    fn turns(&self) -> usize {
        self.actions.len()
    }
}

fn continuation_to_terminal(
    mut state: FullState,
    convention: crate::SupportedConvention,
) -> Result<TestOutcome, String> {
    let profile = convention
        .profile()
        .ok_or_else(|| "H-Group test requires an H-Group profile".to_owned())?;
    let mut actions = Vec::new();
    while !state.is_terminal() && actions.len() < MAX_TEST_CONTINUATION_TURNS {
        let deductions = LogicalDeductions::new(
            state
                .view_for(state.current_player())
                .ok_or_else(|| "current player is invalid".to_owned())?,
        )
        .map_err(|error| error.to_string())?;
        let action = select_h_group_action(&deductions, profile)
            .ok_or_else(|| "H-Group selected no continuation".to_owned())?;
        let actor = state.current_player();
        let turn = state.turn();
        state.apply(action).map_err(|error| {
            format!(
                "turn {turn}, actor {actor:?}, action {action:?}: {error}; prior actions: {actions:?}; inference: {:#?}",
                infer_h_group(&deductions, profile)
            )
        })?;
        actions.push(action);
    }
    if !state.is_terminal() {
        return Err("H-Group continuation exceeded its test turn limit".to_owned());
    }
    Ok(TestOutcome { actions })
}

fn state_with_prefix(num_players: u8, prefix: &[Card]) -> FullState {
    let mut deck = standard_deck();
    for (slot, wanted) in prefix.iter().copied().enumerate() {
        let found = deck[slot..]
            .iter()
            .position(|card| *card == wanted)
            .map(|offset| slot + offset)
            .expect("standard deck contains requested prefix");
        deck.swap(slot, found);
    }
    FullState::new_standard(num_players, deck).unwrap()
}

fn paired_sample_five_state() -> FullState {
    state_with_prefix(
        3,
        &[
            Card::new(Suit::Purple, Rank::One),
            Card::new(Suit::Green, Rank::One),
            Card::new(Suit::Red, Rank::Five),
            Card::new(Suit::Yellow, Rank::One),
            Card::new(Suit::Purple, Rank::Four),
            Card::new(Suit::Red, Rank::Two),
            Card::new(Suit::Yellow, Rank::Three),
            Card::new(Suit::Blue, Rank::Three),
            Card::new(Suit::Red, Rank::One),
            Card::new(Suit::Red, Rank::Three),
            Card::new(Suit::Purple, Rank::Three),
            Card::new(Suit::Red, Rank::One),
            Card::new(Suit::Red, Rank::One),
            Card::new(Suit::Blue, Rank::One),
            Card::new(Suit::Yellow, Rank::One),
            Card::new(Suit::Blue, Rank::Two),
            Card::new(Suit::Blue, Rank::Five),
            Card::new(Suit::Purple, Rank::Four),
            Card::new(Suit::Purple, Rank::Three),
            Card::new(Suit::Yellow, Rank::Two),
            Card::new(Suit::Purple, Rank::Two),
            Card::new(Suit::Blue, Rank::Three),
            Card::new(Suit::Purple, Rank::One),
            Card::new(Suit::Green, Rank::One),
            Card::new(Suit::Blue, Rank::Two),
            Card::new(Suit::Red, Rank::Three),
            Card::new(Suit::Red, Rank::Four),
            Card::new(Suit::Green, Rank::Three),
            Card::new(Suit::Yellow, Rank::Two),
            Card::new(Suit::Blue, Rank::One),
            Card::new(Suit::Blue, Rank::One),
            Card::new(Suit::Green, Rank::Four),
            Card::new(Suit::Purple, Rank::One),
            Card::new(Suit::Purple, Rank::Two),
            Card::new(Suit::Yellow, Rank::Four),
            Card::new(Suit::Green, Rank::Three),
            Card::new(Suit::Green, Rank::Four),
            Card::new(Suit::Purple, Rank::Five),
            Card::new(Suit::Blue, Rank::Four),
            Card::new(Suit::Red, Rank::Four),
            Card::new(Suit::Red, Rank::Two),
            Card::new(Suit::Yellow, Rank::Three),
            Card::new(Suit::Yellow, Rank::One),
            Card::new(Suit::Yellow, Rank::Five),
            Card::new(Suit::Green, Rank::One),
            Card::new(Suit::Blue, Rank::Four),
            Card::new(Suit::Green, Rank::Two),
            Card::new(Suit::Green, Rank::Two),
            Card::new(Suit::Yellow, Rank::Four),
            Card::new(Suit::Green, Rank::Five),
        ],
    )
}

fn paired_sample_four_state() -> FullState {
    state_with_prefix(
        3,
        &[
            Card::new(Suit::Purple, Rank::One),
            Card::new(Suit::Yellow, Rank::Four),
            Card::new(Suit::Green, Rank::Four),
            Card::new(Suit::Green, Rank::Four),
            Card::new(Suit::Blue, Rank::One),
            Card::new(Suit::Red, Rank::Two),
            Card::new(Suit::Yellow, Rank::Three),
            Card::new(Suit::Blue, Rank::Three),
            Card::new(Suit::Red, Rank::One),
            Card::new(Suit::Red, Rank::Three),
            Card::new(Suit::Purple, Rank::Three),
            Card::new(Suit::Red, Rank::One),
            Card::new(Suit::Red, Rank::One),
            Card::new(Suit::Blue, Rank::One),
            Card::new(Suit::Yellow, Rank::One),
            Card::new(Suit::Red, Rank::Three),
            Card::new(Suit::Purple, Rank::Two),
            Card::new(Suit::Red, Rank::Four),
            Card::new(Suit::Green, Rank::Three),
            Card::new(Suit::Yellow, Rank::One),
            Card::new(Suit::Red, Rank::Five),
            Card::new(Suit::Green, Rank::Two),
            Card::new(Suit::Blue, Rank::Four),
            Card::new(Suit::Yellow, Rank::Two),
            Card::new(Suit::Blue, Rank::Five),
            Card::new(Suit::Purple, Rank::Four),
            Card::new(Suit::Blue, Rank::One),
            Card::new(Suit::Green, Rank::One),
            Card::new(Suit::Green, Rank::Two),
            Card::new(Suit::Purple, Rank::Two),
            Card::new(Suit::Blue, Rank::Four),
            Card::new(Suit::Purple, Rank::Four),
            Card::new(Suit::Blue, Rank::Two),
            Card::new(Suit::Blue, Rank::Three),
            Card::new(Suit::Green, Rank::One),
            Card::new(Suit::Yellow, Rank::Three),
            Card::new(Suit::Green, Rank::One),
            Card::new(Suit::Purple, Rank::One),
            Card::new(Suit::Green, Rank::Five),
            Card::new(Suit::Yellow, Rank::One),
            Card::new(Suit::Yellow, Rank::Four),
            Card::new(Suit::Blue, Rank::Two),
            Card::new(Suit::Green, Rank::Three),
            Card::new(Suit::Red, Rank::Two),
            Card::new(Suit::Yellow, Rank::Five),
            Card::new(Suit::Purple, Rank::One),
            Card::new(Suit::Purple, Rank::Three),
            Card::new(Suit::Yellow, Rank::Two),
            Card::new(Suit::Purple, Rank::Five),
            Card::new(Suit::Red, Rank::Four),
        ],
    )
}

fn paired_sample_two_state() -> FullState {
    state_with_prefix(
        3,
        &[
            Card::new(Suit::Purple, Rank::One),
            Card::new(Suit::Green, Rank::Three),
            Card::new(Suit::Yellow, Rank::Three),
            Card::new(Suit::Yellow, Rank::Four),
            Card::new(Suit::Purple, Rank::Two),
            Card::new(Suit::Red, Rank::Two),
            Card::new(Suit::Yellow, Rank::Three),
            Card::new(Suit::Blue, Rank::Three),
            Card::new(Suit::Red, Rank::One),
            Card::new(Suit::Red, Rank::Three),
            Card::new(Suit::Purple, Rank::Three),
            Card::new(Suit::Red, Rank::One),
            Card::new(Suit::Red, Rank::One),
            Card::new(Suit::Blue, Rank::One),
            Card::new(Suit::Yellow, Rank::One),
            Card::new(Suit::Red, Rank::Five),
            Card::new(Suit::Blue, Rank::Four),
            Card::new(Suit::Yellow, Rank::One),
            Card::new(Suit::Green, Rank::Four),
            Card::new(Suit::Blue, Rank::Two),
            Card::new(Suit::Purple, Rank::One),
            Card::new(Suit::Yellow, Rank::Two),
            Card::new(Suit::Purple, Rank::Five),
            Card::new(Suit::Yellow, Rank::One),
            Card::new(Suit::Red, Rank::Four),
            Card::new(Suit::Blue, Rank::One),
            Card::new(Suit::Green, Rank::One),
            Card::new(Suit::Yellow, Rank::Four),
            Card::new(Suit::Green, Rank::Two),
            Card::new(Suit::Purple, Rank::Three),
            Card::new(Suit::Red, Rank::Two),
            Card::new(Suit::Red, Rank::Three),
            Card::new(Suit::Green, Rank::Five),
            Card::new(Suit::Purple, Rank::Four),
            Card::new(Suit::Yellow, Rank::Five),
        ],
    )
}

fn paired_sample_three_state() -> FullState {
    state_with_prefix(
        3,
        &[
            Card::new(Suit::Green, Rank::Two),
            Card::new(Suit::Red, Rank::Three),
            Card::new(Suit::Yellow, Rank::Two),
            Card::new(Suit::Blue, Rank::Two),
            Card::new(Suit::Green, Rank::Three),
            Card::new(Suit::Red, Rank::Two),
            Card::new(Suit::Yellow, Rank::Three),
            Card::new(Suit::Blue, Rank::Three),
            Card::new(Suit::Red, Rank::One),
            Card::new(Suit::Red, Rank::Three),
            Card::new(Suit::Purple, Rank::Three),
            Card::new(Suit::Red, Rank::One),
            Card::new(Suit::Red, Rank::One),
            Card::new(Suit::Blue, Rank::One),
            Card::new(Suit::Yellow, Rank::One),
            Card::new(Suit::Blue, Rank::Three),
            Card::new(Suit::Green, Rank::Four),
            Card::new(Suit::Blue, Rank::Two),
            Card::new(Suit::Purple, Rank::Four),
            Card::new(Suit::Yellow, Rank::Five),
            Card::new(Suit::Green, Rank::One),
            Card::new(Suit::Green, Rank::Five),
            Card::new(Suit::Purple, Rank::Two),
            Card::new(Suit::Blue, Rank::Four),
            Card::new(Suit::Yellow, Rank::One),
            Card::new(Suit::Red, Rank::Two),
            Card::new(Suit::Yellow, Rank::Four),
            Card::new(Suit::Purple, Rank::Three),
            Card::new(Suit::Red, Rank::Five),
            Card::new(Suit::Purple, Rank::Five),
            Card::new(Suit::Yellow, Rank::Four),
            Card::new(Suit::Yellow, Rank::Two),
            Card::new(Suit::Red, Rank::Four),
            Card::new(Suit::Purple, Rank::Four),
            Card::new(Suit::Green, Rank::Four),
            Card::new(Suit::Purple, Rank::One),
            Card::new(Suit::Blue, Rank::One),
            Card::new(Suit::Purple, Rank::Two),
            Card::new(Suit::Blue, Rank::Four),
            Card::new(Suit::Yellow, Rank::One),
            Card::new(Suit::Green, Rank::Three),
            Card::new(Suit::Green, Rank::One),
            Card::new(Suit::Red, Rank::Four),
            Card::new(Suit::Yellow, Rank::Three),
            Card::new(Suit::Purple, Rank::One),
            Card::new(Suit::Purple, Rank::One),
            Card::new(Suit::Blue, Rank::Five),
            Card::new(Suit::Green, Rank::Two),
            Card::new(Suit::Blue, Rank::One),
            Card::new(Suit::Green, Rank::One),
        ],
    )
}

fn paired_sample_seven_state() -> FullState {
    state_with_prefix(
        3,
        &[
            Card::new(Suit::Yellow, Rank::Four),
            Card::new(Suit::Purple, Rank::Four),
            Card::new(Suit::Purple, Rank::Five),
            Card::new(Suit::Green, Rank::Three),
            Card::new(Suit::Red, Rank::Four),
            Card::new(Suit::Red, Rank::Two),
            Card::new(Suit::Yellow, Rank::Three),
            Card::new(Suit::Blue, Rank::Three),
            Card::new(Suit::Red, Rank::One),
            Card::new(Suit::Red, Rank::Three),
            Card::new(Suit::Purple, Rank::Three),
            Card::new(Suit::Red, Rank::One),
            Card::new(Suit::Red, Rank::One),
            Card::new(Suit::Blue, Rank::One),
            Card::new(Suit::Yellow, Rank::One),
            Card::new(Suit::Purple, Rank::Four),
            Card::new(Suit::Green, Rank::One),
        ],
    )
}

fn paired_sample_eight_state() -> FullState {
    state_with_prefix(
        3,
        &[
            Card::new(Suit::Green, Rank::Two),
            Card::new(Suit::Purple, Rank::One),
            Card::new(Suit::Purple, Rank::One),
            Card::new(Suit::Green, Rank::Three),
            Card::new(Suit::Red, Rank::Two),
            Card::new(Suit::Red, Rank::Two),
            Card::new(Suit::Yellow, Rank::Three),
            Card::new(Suit::Blue, Rank::Three),
            Card::new(Suit::Red, Rank::One),
            Card::new(Suit::Red, Rank::Three),
            Card::new(Suit::Purple, Rank::Three),
            Card::new(Suit::Red, Rank::One),
            Card::new(Suit::Red, Rank::One),
            Card::new(Suit::Blue, Rank::One),
            Card::new(Suit::Yellow, Rank::One),
            Card::new(Suit::Blue, Rank::Four),
            Card::new(Suit::Green, Rank::Five),
            Card::new(Suit::Red, Rank::Four),
            Card::new(Suit::Purple, Rank::Four),
            Card::new(Suit::Blue, Rank::Two),
            Card::new(Suit::Purple, Rank::Two),
            Card::new(Suit::Green, Rank::Two),
            Card::new(Suit::Blue, Rank::Three),
            Card::new(Suit::Green, Rank::One),
            Card::new(Suit::Blue, Rank::Five),
            Card::new(Suit::Purple, Rank::Three),
            Card::new(Suit::Blue, Rank::One),
            Card::new(Suit::Red, Rank::Five),
            Card::new(Suit::Purple, Rank::One),
            Card::new(Suit::Yellow, Rank::One),
            Card::new(Suit::Yellow, Rank::Two),
            Card::new(Suit::Yellow, Rank::One),
            Card::new(Suit::Green, Rank::One),
            Card::new(Suit::Red, Rank::Four),
            Card::new(Suit::Yellow, Rank::Four),
            Card::new(Suit::Green, Rank::One),
            Card::new(Suit::Purple, Rank::Five),
            Card::new(Suit::Red, Rank::Three),
            Card::new(Suit::Blue, Rank::One),
            Card::new(Suit::Purple, Rank::Two),
            Card::new(Suit::Blue, Rank::Four),
            Card::new(Suit::Yellow, Rank::Five),
            Card::new(Suit::Yellow, Rank::Two),
            Card::new(Suit::Blue, Rank::Two),
            Card::new(Suit::Green, Rank::Three),
            Card::new(Suit::Green, Rank::Four),
            Card::new(Suit::Green, Rank::Four),
            Card::new(Suit::Yellow, Rank::Three),
            Card::new(Suit::Yellow, Rank::Four),
            Card::new(Suit::Purple, Rank::Four),
        ],
    )
}

fn paired_sample_eleven_state() -> FullState {
    state_with_prefix(
        3,
        &[
            Card::new(Suit::Blue, Rank::Five),
            Card::new(Suit::Blue, Rank::Three),
            Card::new(Suit::Yellow, Rank::One),
            Card::new(Suit::Purple, Rank::Four),
            Card::new(Suit::Red, Rank::Two),
            Card::new(Suit::Red, Rank::Two),
            Card::new(Suit::Yellow, Rank::Three),
            Card::new(Suit::Blue, Rank::Three),
            Card::new(Suit::Red, Rank::One),
            Card::new(Suit::Red, Rank::Three),
            Card::new(Suit::Purple, Rank::Three),
            Card::new(Suit::Red, Rank::One),
            Card::new(Suit::Red, Rank::One),
            Card::new(Suit::Blue, Rank::One),
            Card::new(Suit::Yellow, Rank::One),
            Card::new(Suit::Red, Rank::Five),
            Card::new(Suit::Yellow, Rank::Five),
            Card::new(Suit::Blue, Rank::Four),
            Card::new(Suit::Green, Rank::Three),
            Card::new(Suit::Purple, Rank::One),
            Card::new(Suit::Purple, Rank::Two),
            Card::new(Suit::Green, Rank::Two),
            Card::new(Suit::Red, Rank::Four),
            Card::new(Suit::Purple, Rank::Three),
            Card::new(Suit::Green, Rank::Three),
            Card::new(Suit::Green, Rank::Two),
            Card::new(Suit::Yellow, Rank::One),
            Card::new(Suit::Blue, Rank::One),
            Card::new(Suit::Yellow, Rank::Four),
            Card::new(Suit::Purple, Rank::Four),
            Card::new(Suit::Green, Rank::One),
            Card::new(Suit::Green, Rank::One),
        ],
    )
}

fn paired_sample_twelve_state() -> FullState {
    state_with_prefix(
        3,
        &[
            Card::new(Suit::Purple, Rank::One),
            Card::new(Suit::Blue, Rank::Four),
            Card::new(Suit::Yellow, Rank::Two),
            Card::new(Suit::Green, Rank::Four),
            Card::new(Suit::Yellow, Rank::One),
            Card::new(Suit::Red, Rank::Two),
            Card::new(Suit::Yellow, Rank::Three),
            Card::new(Suit::Blue, Rank::Three),
            Card::new(Suit::Red, Rank::One),
            Card::new(Suit::Red, Rank::Three),
            Card::new(Suit::Purple, Rank::Three),
            Card::new(Suit::Red, Rank::One),
            Card::new(Suit::Red, Rank::One),
            Card::new(Suit::Blue, Rank::One),
            Card::new(Suit::Yellow, Rank::One),
            Card::new(Suit::Purple, Rank::One),
        ],
    )
}

fn paired_sample_six_state() -> FullState {
    state_with_prefix(
        3,
        &[
            Card::new(Suit::Green, Rank::Two),
            Card::new(Suit::Yellow, Rank::One),
            Card::new(Suit::Blue, Rank::Two),
            Card::new(Suit::Green, Rank::Four),
            Card::new(Suit::Blue, Rank::Two),
            Card::new(Suit::Red, Rank::Two),
            Card::new(Suit::Yellow, Rank::Three),
            Card::new(Suit::Blue, Rank::Three),
            Card::new(Suit::Red, Rank::One),
            Card::new(Suit::Red, Rank::Three),
            Card::new(Suit::Purple, Rank::Three),
            Card::new(Suit::Red, Rank::One),
            Card::new(Suit::Red, Rank::One),
            Card::new(Suit::Blue, Rank::One),
            Card::new(Suit::Yellow, Rank::One),
            Card::new(Suit::Purple, Rank::One),
            Card::new(Suit::Red, Rank::Four),
            Card::new(Suit::Purple, Rank::Five),
            Card::new(Suit::Purple, Rank::Two),
            Card::new(Suit::Yellow, Rank::Five),
            Card::new(Suit::Blue, Rank::Three),
            Card::new(Suit::Green, Rank::Three),
            Card::new(Suit::Green, Rank::Four),
            Card::new(Suit::Purple, Rank::One),
            Card::new(Suit::Red, Rank::Four),
            Card::new(Suit::Green, Rank::One),
            Card::new(Suit::Green, Rank::Two),
            Card::new(Suit::Blue, Rank::Four),
            Card::new(Suit::Yellow, Rank::Two),
            Card::new(Suit::Blue, Rank::Four),
            Card::new(Suit::Purple, Rank::Two),
            Card::new(Suit::Purple, Rank::Four),
            Card::new(Suit::Yellow, Rank::Four),
            Card::new(Suit::Red, Rank::Two),
            Card::new(Suit::Green, Rank::Five),
            Card::new(Suit::Yellow, Rank::Four),
            Card::new(Suit::Red, Rank::Five),
            Card::new(Suit::Blue, Rank::Five),
            Card::new(Suit::Blue, Rank::One),
            Card::new(Suit::Purple, Rank::Four),
            Card::new(Suit::Yellow, Rank::Three),
            Card::new(Suit::Purple, Rank::One),
            Card::new(Suit::Blue, Rank::One),
            Card::new(Suit::Yellow, Rank::One),
            Card::new(Suit::Green, Rank::One),
            Card::new(Suit::Green, Rank::One),
            Card::new(Suit::Green, Rank::Three),
            Card::new(Suit::Purple, Rank::Three),
            Card::new(Suit::Yellow, Rank::Two),
            Card::new(Suit::Red, Rank::Three),
        ],
    )
}

fn paired_sample_one_state() -> FullState {
    state_with_prefix(
        3,
        &[
            Card::new(Suit::Green, Rank::Two),
            Card::new(Suit::Yellow, Rank::Two),
            Card::new(Suit::Green, Rank::Two),
            Card::new(Suit::Green, Rank::Five),
            Card::new(Suit::Green, Rank::One),
            Card::new(Suit::Red, Rank::Two),
            Card::new(Suit::Yellow, Rank::Three),
            Card::new(Suit::Blue, Rank::Three),
            Card::new(Suit::Red, Rank::One),
            Card::new(Suit::Red, Rank::Three),
            Card::new(Suit::Purple, Rank::Three),
            Card::new(Suit::Red, Rank::One),
            Card::new(Suit::Red, Rank::One),
            Card::new(Suit::Blue, Rank::One),
            Card::new(Suit::Yellow, Rank::One),
            Card::new(Suit::Purple, Rank::Three),
            Card::new(Suit::Purple, Rank::Two),
            Card::new(Suit::Yellow, Rank::Four),
            Card::new(Suit::Red, Rank::Three),
            Card::new(Suit::Green, Rank::Four),
            Card::new(Suit::Red, Rank::Four),
            Card::new(Suit::Red, Rank::Two),
            Card::new(Suit::Purple, Rank::One),
            Card::new(Suit::Red, Rank::Four),
            Card::new(Suit::Blue, Rank::Five),
            Card::new(Suit::Purple, Rank::Four),
            Card::new(Suit::Green, Rank::Three),
            Card::new(Suit::Yellow, Rank::Three),
            Card::new(Suit::Yellow, Rank::One),
            Card::new(Suit::Yellow, Rank::Two),
            Card::new(Suit::Green, Rank::Three),
            Card::new(Suit::Green, Rank::One),
            Card::new(Suit::Blue, Rank::Two),
            Card::new(Suit::Yellow, Rank::Four),
            Card::new(Suit::Blue, Rank::Four),
            Card::new(Suit::Yellow, Rank::Five),
            Card::new(Suit::Red, Rank::Five),
            Card::new(Suit::Green, Rank::One),
            Card::new(Suit::Purple, Rank::One),
            Card::new(Suit::Blue, Rank::One),
            Card::new(Suit::Green, Rank::Four),
            Card::new(Suit::Purple, Rank::Two),
            Card::new(Suit::Blue, Rank::One),
            Card::new(Suit::Blue, Rank::Two),
            Card::new(Suit::Purple, Rank::Four),
            Card::new(Suit::Yellow, Rank::One),
            Card::new(Suit::Purple, Rank::Five),
            Card::new(Suit::Blue, Rank::Four),
            Card::new(Suit::Purple, Rank::One),
            Card::new(Suit::Blue, Rank::Three),
        ],
    )
}

fn paired_sample_zero_state() -> FullState {
    state_with_prefix(
        3,
        &[
            Card::new(Suit::Blue, Rank::Two),
            Card::new(Suit::Green, Rank::Four),
            Card::new(Suit::Blue, Rank::Four),
            Card::new(Suit::Purple, Rank::Four),
            Card::new(Suit::Blue, Rank::Five),
            Card::new(Suit::Red, Rank::Two),
            Card::new(Suit::Yellow, Rank::Three),
            Card::new(Suit::Blue, Rank::Three),
            Card::new(Suit::Red, Rank::One),
            Card::new(Suit::Red, Rank::Three),
            Card::new(Suit::Purple, Rank::Three),
            Card::new(Suit::Red, Rank::One),
            Card::new(Suit::Red, Rank::One),
            Card::new(Suit::Blue, Rank::One),
            Card::new(Suit::Yellow, Rank::One),
            Card::new(Suit::Purple, Rank::Three),
            Card::new(Suit::Red, Rank::Four),
            Card::new(Suit::Yellow, Rank::Four),
            Card::new(Suit::Blue, Rank::Four),
            Card::new(Suit::Red, Rank::Five),
            Card::new(Suit::Yellow, Rank::Five),
            Card::new(Suit::Yellow, Rank::Two),
            Card::new(Suit::Yellow, Rank::Two),
            Card::new(Suit::Blue, Rank::One),
            Card::new(Suit::Purple, Rank::Two),
            Card::new(Suit::Red, Rank::Two),
            Card::new(Suit::Yellow, Rank::One),
            Card::new(Suit::Green, Rank::One),
            Card::new(Suit::Yellow, Rank::Four),
            Card::new(Suit::Green, Rank::Four),
            Card::new(Suit::Blue, Rank::Two),
            Card::new(Suit::Yellow, Rank::Three),
            Card::new(Suit::Yellow, Rank::One),
            Card::new(Suit::Purple, Rank::Two),
            Card::new(Suit::Red, Rank::Four),
        ],
    )
}

fn paired_sample_five_after_second_five_save() -> FullState {
    let mut state = paired_sample_five_state();
    for action in [
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Rank(Rank::Two),
        },
        Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Rank(Rank::One),
        },
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Rank(Rank::One),
        },
        Action::Play(CardId::new(3)),
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Rank(Rank::Five),
        },
        Action::Play(CardId::new(14)),
        Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Suit(Suit::Red),
        },
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Suit(Suit::Blue),
        },
        Action::Play(CardId::new(13)),
        Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Suit(Suit::Red),
        },
        Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Suit(Suit::Red),
        },
        Action::Play(CardId::new(12)),
        Action::Play(CardId::new(15)),
        Action::Discard(CardId::new(6)),
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Rank(Rank::Five),
        },
    ] {
        state.apply(action).unwrap();
    }
    state
}

fn observed(id: usize, identity: Option<Card>, clues: &[Clue]) -> ObservedCard {
    let mut facts = ClueFacts::default();
    for clue in clues {
        facts.add_positive_clue(*clue);
    }
    ObservedCard {
        id: CardId::new(id),
        identity,
        clues: facts,
    }
}
