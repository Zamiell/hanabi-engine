use super::*;
use hanabi_core::{
    Action, FullState, GameStatus, ObservedCard, ObservedHistoryEntry, PlayerView, standard_deck,
};

const MAX_TEST_CONTINUATION_TURNS: usize = 512;

struct TestOutcome {
    actions: Vec<Action>,
}

impl TestOutcome {
    fn actions(&self) -> &[Action] {
        &self.actions
    }

    fn turns(&self) -> usize {
        self.actions.len()
    }
}

struct TestContinuationReport {
    outcome: TestOutcome,
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
        state.apply(action).map_err(|error| error.to_string())?;
        actions.push(action);
    }
    if !state.is_terminal() {
        return Err("H-Group continuation exceeded its test turn limit".to_owned());
    }
    Ok(TestOutcome { actions })
}

fn continuation_for_search(
    state: FullState,
    convention: crate::SupportedConvention,
) -> Result<TestContinuationReport, String> {
    continuation_to_terminal(state, convention).map(|outcome| TestContinuationReport { outcome })
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

#[test]
fn learning_path_metadata_covers_every_cumulative_level() {
    assert_eq!(H_GROUP_LEVELS.len(), 26);
    for (index, descriptor) in H_GROUP_LEVELS.iter().enumerate() {
        assert_eq!(usize::from(descriptor.profile.effective_level()), index + 1);
        assert!(!descriptor.title.is_empty());
        assert!(!descriptor.effects.is_empty());
    }
    assert_eq!(HGroupProfile::Max.effective_level(), 26);
}

#[test]
fn level_two_enables_an_off_chop_five_stall() {
    let state = state_with_prefix(
        2,
        &[
            Card::new(Suit::Blue, Rank::Three),
            Card::new(Suit::Green, Rank::Three),
            Card::new(Suit::Yellow, Rank::Four),
            Card::new(Suit::Purple, Rank::Four),
            Card::new(Suit::Red, Rank::Three),
            Card::new(Suit::Blue, Rank::One),
            Card::new(Suit::Red, Rank::Five),
            Card::new(Suit::Green, Rank::Four),
            Card::new(Suit::Yellow, Rank::Three),
            Card::new(Suit::Purple, Rank::Four),
        ],
    );
    let deductions = LogicalDeductions::new(state.view_for(PlayerId::new(0)).unwrap()).unwrap();
    let five_stall = Action::Clue {
        target: PlayerId::new(1),
        clue: Clue::Rank(Rank::Five),
    };
    let level_one = h_group_clue_candidates(&deductions, HGroupProfile::Level(HGroupLevel::Level1));
    let level_two = h_group_clue_candidates(&deductions, HGroupProfile::Level(HGroupLevel::Level2));
    let level_one_score = level_one
        .iter()
        .find(|candidate| candidate.action == five_stall)
        .map(|candidate| candidate.score);
    let level_two_score = level_two
        .iter()
        .find(|candidate| candidate.action == five_stall)
        .map(|candidate| candidate.score);
    assert!(level_two_score > level_one_score);
}

#[test]
fn level_five_keeps_a_layered_finesse_as_an_exact_disjunction() {
    let mut state = state_with_prefix(
        3,
        &[
            Card::new(Suit::Blue, Rank::Four),
            Card::new(Suit::Green, Rank::Four),
            Card::new(Suit::Yellow, Rank::Three),
            Card::new(Suit::Purple, Rank::Four),
            Card::new(Suit::Blue, Rank::Five),
            Card::new(Suit::Yellow, Rank::Four),
            Card::new(Suit::Purple, Rank::Three),
            Card::new(Suit::Blue, Rank::Four),
            Card::new(Suit::Red, Rank::One),
            Card::new(Suit::Green, Rank::One),
            Card::new(Suit::Red, Rank::Two),
            Card::new(Suit::Blue, Rank::Three),
            Card::new(Suit::Green, Rank::Four),
            Card::new(Suit::Yellow, Rank::Five),
            Card::new(Suit::Purple, Rank::Four),
        ],
    );
    state
        .apply(Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Suit(Suit::Red),
        })
        .unwrap();
    let deductions = LogicalDeductions::new(state.view_for(PlayerId::new(1)).unwrap()).unwrap();
    let level_four = infer_h_group(&deductions, HGroupProfile::Level(HGroupLevel::Level4));
    let level_five = infer_h_group(&deductions, HGroupProfile::Level(HGroupLevel::Level5));
    assert_eq!(
        level_four.connection_promises[0].cards,
        vec![CardId::new(9)]
    );
    assert_eq!(
        level_five.connection_promises[0].cards[..2],
        [CardId::new(9), CardId::new(8)]
    );
    assert!(
        level_five
            .signals
            .iter()
            .any(|signal| signal.kind == HGroupMoveKind::LayeredFinesse)
    );
}

#[test]
fn level_sixteen_ejects_the_second_finesse_position() {
    let mut state = state_with_prefix(
        3,
        &[
            Card::new(Suit::Blue, Rank::Three),
            Card::new(Suit::Green, Rank::Four),
            Card::new(Suit::Yellow, Rank::Three),
            Card::new(Suit::Purple, Rank::Four),
            Card::new(Suit::Blue, Rank::Five),
            Card::new(Suit::Yellow, Rank::Four),
            Card::new(Suit::Purple, Rank::Three),
            Card::new(Suit::Blue, Rank::Four),
            Card::new(Suit::Green, Rank::One),
            Card::new(Suit::Yellow, Rank::One),
            Card::new(Suit::Red, Rank::Five),
            Card::new(Suit::Blue, Rank::Three),
            Card::new(Suit::Green, Rank::Four),
            Card::new(Suit::Yellow, Rank::Five),
            Card::new(Suit::Purple, Rank::Four),
        ],
    );
    state
        .apply(Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Suit(Suit::Red),
        })
        .unwrap();
    let deductions = LogicalDeductions::new(state.view_for(PlayerId::new(1)).unwrap()).unwrap();
    let level_fifteen = infer_h_group(&deductions, HGroupProfile::Level(HGroupLevel::Level15));
    let level_sixteen = infer_h_group(&deductions, HGroupProfile::Level(HGroupLevel::Level16));
    assert!(!level_fifteen.playable_now.contains(&CardId::new(8)));
    assert!(level_sixteen.connection.is_none());
    assert!(
        level_sixteen.playable_now.contains(&CardId::new(8)),
        "inference: {level_sixteen:#?}"
    );
    assert!(
        level_sixteen
            .signals
            .iter()
            .any(|signal| signal.kind == HGroupMoveKind::Ejection)
    );
}

#[test]
fn focus_prefers_chop_when_a_clue_newly_touches_multiple_cards() {
    let red_one = Card::new(Suit::Red, Rank::One);
    let mut state = state_with_prefix(
        2,
        &[
            red_one,
            Card::new(Suit::Blue, Rank::Three),
            Card::new(Suit::Green, Rank::Four),
            Card::new(Suit::Yellow, Rank::Three),
            Card::new(Suit::Purple, Rank::Four),
            red_one,
            Card::new(Suit::Blue, Rank::One),
            Card::new(Suit::Green, Rank::Four),
            Card::new(Suit::Yellow, Rank::One),
            red_one,
        ],
    );
    state
        .apply(Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Rank(Rank::One),
        })
        .unwrap();
    let deductions = LogicalDeductions::new(state.view_for(PlayerId::new(1)).unwrap()).unwrap();
    let inferred = infer_h_group(
        &deductions,
        HGroupProfile::Level(crate::HGroupLevel::Level1),
    );
    assert_eq!(inferred.clues[0].focus, CardId::new(5));
    assert!(inferred.clues[0].focus_was_chop);
}

#[test]
fn focus_rules_cover_retouched_single_and_leftmost_new_cards() {
    let hand = (0..5).map(CardId::new).collect::<Vec<_>>();
    let gotten = [CardId::new(0), CardId::new(1)]
        .into_iter()
        .collect::<CardSet>();
    let current_chop = chop(&hand, &gotten);
    assert_eq!(current_chop, Some(CardId::new(2)));
    assert_eq!(
        focus(
            &hand,
            &[CardId::new(0), CardId::new(1)],
            current_chop,
            &gotten,
        ),
        Some(CardId::new(1))
    );
    assert_eq!(
        focus(
            &hand,
            &[CardId::new(0), CardId::new(3)],
            current_chop,
            &gotten,
        ),
        Some(CardId::new(3))
    );
    assert_eq!(
        focus(
            &hand,
            &[CardId::new(3), CardId::new(4)],
            current_chop,
            &gotten,
        ),
        Some(CardId::new(4))
    );
    assert_eq!(
        focus(
            &hand,
            &[CardId::new(2), CardId::new(3)],
            current_chop,
            &gotten,
        ),
        Some(CardId::new(2))
    );
}

#[test]
fn two_save_is_followed_by_the_more_efficient_rank_one_play_clue() {
    let mut state = paired_sample_five_state();
    state
        .apply(Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Rank(Rank::Two),
        })
        .unwrap();
    let deductions = LogicalDeductions::new(state.view_for(PlayerId::new(1)).unwrap()).unwrap();

    assert_eq!(
        select_h_group_action(&deductions, HGroupProfile::Max),
        Some(Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Rank(Rank::One),
        }),
        "candidates: {:#?}",
        h_group_clue_candidates(&deductions, HGroupProfile::Max),
    );
}

#[test]
fn out_of_order_clue_is_not_given_to_the_immediate_next_player() {
    let mut state = paired_sample_five_state();
    for action in [
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Rank(Rank::Two),
        },
        Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Suit(Suit::Red),
        },
        Action::Play(CardId::new(12)),
        Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Suit(Suit::Yellow),
        },
    ] {
        state.apply(action).unwrap();
    }
    let deductions = LogicalDeductions::new(state.view_for(PlayerId::new(1)).unwrap()).unwrap();
    let candidates = h_group_clue_candidates(&deductions, HGroupProfile::Max);

    assert!(
        !candidates.iter().any(|candidate| {
            candidate.action
                == Action::Clue {
                    target: PlayerId::new(2),
                    clue: Clue::Suit(Suit::Blue),
                }
        }),
        "the recipient would act before anyone could give the required Fix Clue: {candidates:#?}"
    );
}

#[test]
fn multi_card_purple_clue_prompts_purple_one_before_purple_two() {
    let mut state = paired_sample_two_state();
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
            clue: Clue::Suit(Suit::Purple),
        },
    ] {
        state.apply(action).unwrap();
    }
    let deductions = LogicalDeductions::new(state.view_for(PlayerId::new(0)).unwrap()).unwrap();
    let inferred = infer_h_group(&deductions, HGroupProfile::Max);
    assert_eq!(inferred.connection, None);
    assert_eq!(
        select_h_group_action(&deductions, HGroupProfile::Max),
        Some(Action::Play(CardId::new(0))),
        "inference: {inferred:#?}; clues: {:#?}",
        h_group_clue_candidates(&deductions, HGroupProfile::Max)
    );
}

#[test]
fn repeated_two_clue_does_not_play_red_two_before_its_prompt() {
    let mut state = paired_sample_two_state();
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
            clue: Clue::Suit(Suit::Purple),
        },
        Action::Play(CardId::new(0)),
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Suit(Suit::Purple),
        },
        Action::Play(CardId::new(14)),
        Action::Play(CardId::new(4)),
        Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Suit(Suit::Purple),
        },
        Action::Play(CardId::new(10)),
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Rank(Rank::Two),
        },
    ] {
        state.apply(action).unwrap();
    }
    let deductions = LogicalDeductions::new(state.view_for(PlayerId::new(1)).unwrap()).unwrap();
    let inferred = infer_h_group(&deductions, HGroupProfile::Max);
    assert_ne!(
        select_h_group_action(&deductions, HGroupProfile::Max),
        Some(Action::Play(CardId::new(5))),
        "inference: {inferred:#?}"
    );
}

#[test]
fn delayed_green_three_without_connectors_is_not_a_fix_clue() {
    let mut state = paired_sample_two_state();
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
            clue: Clue::Suit(Suit::Purple),
        },
        Action::Play(CardId::new(0)),
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Suit(Suit::Purple),
        },
        Action::Play(CardId::new(14)),
        Action::Play(CardId::new(4)),
        Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Suit(Suit::Purple),
        },
        Action::Play(CardId::new(10)),
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Rank(Rank::Two),
        },
    ] {
        state.apply(action).unwrap();
    }
    let deductions = LogicalDeductions::new(state.view_for(PlayerId::new(1)).unwrap()).unwrap();
    let candidates = h_group_clue_candidates(&deductions, HGroupProfile::Max);

    assert!(
        !candidates.iter().any(|candidate| {
            candidate.action
                == Action::Clue {
                    target: PlayerId::new(0),
                    clue: Clue::Suit(Suit::Green),
                }
        }),
        "candidates: {candidates:#?}"
    );
    assert!(
        !candidates.iter().any(|candidate| {
            candidate.action
                == Action::Clue {
                    target: PlayerId::new(2),
                    clue: Clue::Suit(Suit::Blue),
                }
        }),
        "a blue-4 out-of-order clue has no accounted blue 2: {candidates:#?}"
    );
}

#[test]
fn self_prompt_survives_an_intervening_fix_clue() {
    let mut state = paired_sample_three_state();
    for action in [
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Rank(Rank::Two),
        },
        Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Rank(Rank::One),
        },
        Action::Play(CardId::new(14)),
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Rank(Rank::Two),
        },
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Suit(Suit::Blue),
        },
        Action::Play(CardId::new(13)),
        Action::Play(CardId::new(3)),
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Suit(Suit::Yellow),
        },
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Rank(Rank::Three),
        },
        Action::Play(CardId::new(2)),
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Rank(Rank::Two),
        },
        Action::Play(CardId::new(12)),
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Suit(Suit::Yellow),
        },
    ] {
        state.apply(action).unwrap();
    }
    let deductions = LogicalDeductions::new(state.view_for(PlayerId::new(1)).unwrap()).unwrap();
    let inferred = infer_h_group(&deductions, HGroupProfile::Max);

    assert_ne!(
        h_group_predictable_action(&deductions, HGroupProfile::Max),
        Some(Action::Play(CardId::new(26))),
        "yellow 4 cannot be a predictable play before yellow 3: {inferred:#?}"
    );
    assert_ne!(
        select_h_group_action(&deductions, HGroupProfile::Max),
        Some(Action::Play(CardId::new(9))),
        "inference: {inferred:#?}"
    );
}

#[test]
fn immediate_rank_three_clue_is_rejected_when_it_creates_a_false_self_prompt() {
    let mut state = paired_sample_one_state();
    for action in [
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Rank(Rank::Two),
        },
        Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Rank(Rank::One),
        },
        Action::Play(CardId::new(14)),
        Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Suit(Suit::Blue),
        },
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Suit(Suit::Yellow),
        },
        Action::Play(CardId::new(13)),
        Action::Play(CardId::new(1)),
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Rank(Rank::One),
        },
    ] {
        state.apply(action).unwrap();
    }
    let action = Action::Clue {
        target: PlayerId::new(1),
        clue: Clue::Rank(Rank::Three),
    };
    let giver = LogicalDeductions::new(state.view_for(PlayerId::new(2)).unwrap()).unwrap();
    assert!(
        !h_group_clue_candidates(&giver, HGroupProfile::Max)
            .iter()
            .any(|candidate| candidate.action == action),
        "candidates: {:#?}",
        h_group_clue_candidates(&giver, HGroupProfile::Max),
    );
}

#[test]
fn five_color_ejection_is_not_given_when_second_finesse_position_would_misplay() {
    let mut state = paired_sample_seven_state();
    for action in [
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Rank(Rank::Two),
        },
        Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Rank(Rank::One),
        },
        Action::Play(CardId::new(14)),
        Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Suit(Suit::Red),
        },
        Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Suit(Suit::Red),
        },
        Action::Play(CardId::new(12)),
        Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Suit(Suit::Green),
        },
    ] {
        state.apply(action).unwrap();
    }
    let deductions = LogicalDeductions::new(state.view_for(PlayerId::new(1)).unwrap()).unwrap();
    let candidates = ordered_h_group_actions(&deductions, HGroupProfile::Max);

    assert!(!candidates.contains(&Action::Clue {
        target: PlayerId::new(0),
        clue: Clue::Suit(Suit::Purple),
    }));
}

#[test]
fn duplicate_rank_ones_use_a_focused_play_clue_instead_of_ignition() {
    let mut state = paired_sample_one_state();
    for action in [
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Rank(Rank::Two),
        },
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Suit(Suit::Yellow),
        },
        Action::Play(CardId::new(14)),
        Action::Play(CardId::new(1)),
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Rank(Rank::One),
        },
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Rank(Rank::Five),
        },
        Action::Play(CardId::new(4)),
    ] {
        state.apply(action).unwrap();
    }
    let deductions = LogicalDeductions::new(state.view_for(PlayerId::new(1)).unwrap()).unwrap();

    let rank_one = Action::Clue {
        target: PlayerId::new(2),
        clue: Clue::Rank(Rank::One),
    };
    assert!(ordered_h_group_actions(&deductions, HGroupProfile::Max).contains(&rank_one));

    state.apply(rank_one).unwrap();
    let receiver = LogicalDeductions::new(state.view_for(PlayerId::new(2)).unwrap()).unwrap();
    let inferred = infer_h_group(&receiver, HGroupProfile::Max);
    assert_eq!(
        select_h_group_action(&receiver, HGroupProfile::Max),
        Some(Action::Play(CardId::new(13)))
    );
    assert!(!inferred.playable_now.contains(&CardId::new(11)));
    assert!(!inferred.playable_now.contains(&CardId::new(12)));
}

#[test]
fn delayed_green_four_is_not_clued_over_an_invalid_finesse() {
    let mut state = paired_sample_zero_state();
    for action in [
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Rank(Rank::Two),
        },
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Rank(Rank::Two),
        },
        Action::Play(CardId::new(14)),
        Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Suit(Suit::Blue),
        },
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Suit(Suit::Blue),
        },
        Action::Play(CardId::new(13)),
        Action::Play(CardId::new(0)),
    ] {
        state.apply(action).unwrap();
    }
    let deductions = LogicalDeductions::new(state.view_for(PlayerId::new(1)).unwrap()).unwrap();
    let clue_candidates = h_group_clue_candidates(&deductions, HGroupProfile::Max);

    assert!(
        !clue_candidates.iter().any(|candidate| {
            candidate.action
                == Action::Clue {
                    target: PlayerId::new(0),
                    clue: Clue::Suit(Suit::Green),
                }
        }),
        "candidates: {clue_candidates:#?}, inference: {:#?}",
        infer_h_group(&deductions, HGroupProfile::Max)
    );
}

#[test]
fn low_score_five_stall_does_not_also_finesse_the_next_player() {
    let mut state = paired_sample_zero_state();
    for action in [
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Rank(Rank::Two),
        },
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Rank(Rank::Two),
        },
        Action::Play(CardId::new(14)),
        Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Suit(Suit::Blue),
        },
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Suit(Suit::Blue),
        },
        Action::Play(CardId::new(13)),
        Action::Play(CardId::new(0)),
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Rank(Rank::Five),
        },
    ] {
        state.apply(action).unwrap();
    }
    let deductions = LogicalDeductions::new(state.view_for(PlayerId::new(2)).unwrap()).unwrap();
    let inferred = infer_h_group(&deductions, HGroupProfile::Max);

    assert_eq!(inferred.connection, None);
    assert_ne!(
        select_h_group_action(&deductions, HGroupProfile::Max),
        Some(Action::Play(CardId::new(16)))
    );
}

#[test]
fn completed_blue_prompt_plays_the_focus_before_same_clue_ancillary_cards() {
    let mut state = paired_sample_zero_state();
    for action in [
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Rank(Rank::Two),
        },
        Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Rank(Rank::One),
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
    ] {
        state.apply(action).unwrap();
    }
    let deductions = LogicalDeductions::new(state.view_for(PlayerId::new(0)).unwrap()).unwrap();
    let inferred = infer_h_group(&deductions, HGroupProfile::Max);

    assert_eq!(
        select_h_group_action(&deductions, HGroupProfile::Max),
        Some(Action::Play(CardId::new(0))),
        "inference: {inferred:#?}"
    );
}

#[test]
fn tempo_clue_does_not_reclue_an_active_play_promise() {
    let mut state = paired_sample_zero_state();
    for action in [
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Rank(Rank::Two),
        },
        Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Rank(Rank::One),
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
        Action::Play(CardId::new(0)),
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Rank(Rank::Five),
        },
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Suit(Suit::Blue),
        },
    ] {
        state.apply(action).unwrap();
    }
    let deductions = LogicalDeductions::new(state.view_for(PlayerId::new(0)).unwrap()).unwrap();
    let candidates = h_group_clue_candidates(&deductions, HGroupProfile::Max);

    assert!(
        !candidates.iter().any(|candidate| {
            candidate.action
                == Action::Clue {
                    target: PlayerId::new(1),
                    clue: Clue::Suit(Suit::Blue),
                }
        }),
        "candidates: {candidates:#?}"
    );
}

#[test]
fn out_of_order_fix_obligation_selects_the_repairing_rank_clue() {
    let mut state = paired_sample_one_state();
    for action in [
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Rank(Rank::Two),
        },
        Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Rank(Rank::One),
        },
        Action::Play(CardId::new(14)),
        Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Suit(Suit::Red),
        },
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Suit(Suit::Yellow),
        },
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Rank(Rank::One),
        },
        Action::Play(CardId::new(1)),
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Suit(Suit::Green),
        },
    ] {
        state.apply(action).unwrap();
    }
    let deductions = LogicalDeductions::new(state.view_for(PlayerId::new(2)).unwrap()).unwrap();
    let inferred = infer_h_group(&deductions, HGroupProfile::Max);

    assert_eq!(
        select_h_group_action(&deductions, HGroupProfile::Max),
        Some(Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Rank(Rank::Two),
        }),
        "inference: {inferred:#?}"
    );
}

#[test]
fn collateral_fix_cancels_the_false_finesse_it_would_otherwise_create() {
    let mut state = paired_sample_one_state();
    for action in [
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Rank(Rank::Two),
        },
        Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Rank(Rank::One),
        },
        Action::Play(CardId::new(14)),
        Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Suit(Suit::Red),
        },
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Suit(Suit::Yellow),
        },
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Rank(Rank::One),
        },
        Action::Play(CardId::new(1)),
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Suit(Suit::Green),
        },
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Rank(Rank::Two),
        },
        Action::Play(CardId::new(4)),
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Suit(Suit::Purple),
        },
    ] {
        state.apply(action).unwrap();
    }
    let deductions = LogicalDeductions::new(state.view_for(PlayerId::new(2)).unwrap()).unwrap();
    let inferred = infer_h_group(&deductions, HGroupProfile::Max);

    assert_ne!(
        inferred.connection.map(|connection| connection.card),
        Some(CardId::new(15)),
        "inference: {inferred:#?}"
    );
    assert_eq!(
        select_h_group_action(&deductions, HGroupProfile::Max),
        Some(Action::Play(CardId::new(13))),
        "inference: {inferred:#?}"
    );
}

#[test]
fn rank_two_clue_can_prompt_red_one_while_good_touching_a_purple_two() {
    let mut state = paired_sample_one_state();
    for action in [
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Rank(Rank::Two),
        },
        Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Rank(Rank::One),
        },
        Action::Play(CardId::new(14)),
        Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Suit(Suit::Red),
        },
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Suit(Suit::Yellow),
        },
        Action::Play(CardId::new(13)),
        Action::Play(CardId::new(1)),
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Rank(Rank::One),
        },
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Suit(Suit::Yellow),
        },
        Action::Play(CardId::new(4)),
        Action::Play(CardId::new(6)),
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Suit(Suit::Yellow),
        },
        Action::Play(CardId::new(17)),
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Rank(Rank::Five),
        },
        Action::Discard(CardId::new(11)),
        Action::Play(CardId::new(2)),
    ] {
        state.apply(action).unwrap();
    }
    let deductions = LogicalDeductions::new(state.view_for(PlayerId::new(1)).unwrap()).unwrap();
    let candidates = h_group_clue_candidates(&deductions, HGroupProfile::Max);

    assert!(
        candidates.iter().any(|candidate| {
            candidate.action
                == Action::Clue {
                    target: PlayerId::new(2),
                    clue: Clue::Rank(Rank::Two),
                }
        }),
        "the newer red 2 is the focus and the red 1 is its valid Self-Prompt: {candidates:#?}"
    );
}

#[test]
fn blue_three_clue_is_rejected_when_its_blue_two_finesse_would_misplay() {
    let mut state = paired_sample_one_state();
    for action in [
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Rank(Rank::One),
        },
        Action::Play(CardId::new(8)),
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Suit(Suit::Red),
        },
        Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Suit(Suit::Yellow),
        },
        Action::Play(CardId::new(5)),
        Action::Play(CardId::new(14)),
        Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Suit(Suit::Blue),
        },
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Suit(Suit::Yellow),
        },
        Action::Play(CardId::new(13)),
        Action::Play(CardId::new(1)),
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Rank(Rank::One),
        },
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Rank(Rank::Three),
        },
        Action::Play(CardId::new(4)),
        Action::Play(CardId::new(6)),
    ] {
        state.apply(action).unwrap();
    }
    let deductions = LogicalDeductions::new(state.view_for(PlayerId::new(2)).unwrap()).unwrap();
    let candidates = h_group_clue_candidates(&deductions, HGroupProfile::Max);

    assert!(
        !candidates.iter().any(|candidate| {
            candidate.action
                == Action::Clue {
                    target: PlayerId::new(1),
                    clue: Clue::Suit(Suit::Blue),
                }
        }),
        "candidates: {candidates:#?}"
    );
}

#[test]
fn two_save_does_not_finesse_the_next_players_green_four() {
    let mut state = paired_sample_three_state();
    for action in [
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Rank(Rank::Two),
        },
        Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Rank(Rank::One),
        },
        Action::Play(CardId::new(14)),
        Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Suit(Suit::Red),
        },
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Suit(Suit::Yellow),
        },
        Action::Play(CardId::new(13)),
        Action::Play(CardId::new(2)),
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Rank(Rank::Two),
        },
    ] {
        state.apply(action).unwrap();
    }
    let deductions = LogicalDeductions::new(state.view_for(PlayerId::new(2)).unwrap()).unwrap();
    let inferred = infer_h_group(&deductions, HGroupProfile::Max);

    assert_ne!(
        select_h_group_action(&deductions, HGroupProfile::Max),
        Some(Action::Play(CardId::new(16))),
        "inference: {inferred:#?}"
    );
}

#[test]
fn multi_card_red_clue_rejects_a_chop_moved_false_prompt() {
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
            clue: Clue::Suit(Suit::Green),
        },
        Action::Play(CardId::new(14)),
        Action::Play(CardId::new(0)),
        Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Suit(Suit::Red),
        },
        Action::Play(CardId::new(13)),
        Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Rank(Rank::Five),
        },
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Rank(Rank::Two),
        },
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Suit(Suit::Blue),
        },
        Action::Discard(CardId::new(17)),
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Suit(Suit::Yellow),
        },
        Action::Discard(CardId::new(11)),
        Action::Play(CardId::new(19)),
        Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Suit(Suit::Red),
        },
        Action::Discard(CardId::new(12)),
    ] {
        state.apply(action).unwrap();
    }
    let giver = LogicalDeductions::new(state.view_for(PlayerId::new(0)).unwrap()).unwrap();
    let replay = replay_h_group(&giver, HGroupProfile::Max);
    assert!(
        !replay.explicitly_clued.contains(&CardId::new(8))
            && !replay.invisibly_clued.contains(&CardId::new(8)),
        "the false Prompt card must not be promptable: {replay:#?}"
    );
    let candidates = h_group_clue_candidates(&giver, HGroupProfile::Max);
    assert!(
        !candidates.iter().any(|candidate| {
            candidate.action
                == Action::Clue {
                    target: PlayerId::new(1),
                    clue: Clue::Suit(Suit::Red),
                }
        }),
        "the giver must reject a clue its recipient would misplay: {candidates:#?}"
    );
    assert_ne!(
        select_h_group_action(&giver, HGroupProfile::Max),
        Some(Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Suit(Suit::Red),
        }),
        "candidate fallback selected the rejected clue: {candidates:#?}"
    );
}

#[test]
fn unknown_trash_discharge_is_rejected_when_third_finesse_would_misplay() {
    let mut state = paired_sample_two_state();
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
            clue: Clue::Suit(Suit::Purple),
        },
        Action::Play(CardId::new(0)),
        Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Suit(Suit::Red),
        },
        Action::Play(CardId::new(14)),
        Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Rank(Rank::One),
        },
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Rank(Rank::Five),
        },
        Action::Play(CardId::new(13)),
        Action::Play(CardId::new(4)),
        Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Suit(Suit::Purple),
        },
        Action::Play(CardId::new(10)),
    ] {
        state.apply(action).unwrap();
    }
    let deductions = LogicalDeductions::new(state.view_for(PlayerId::new(0)).unwrap()).unwrap();
    let candidates = h_group_clue_candidates(&deductions, HGroupProfile::Max);
    let invalid = Action::Clue {
        target: PlayerId::new(2),
        clue: Clue::Rank(Rank::One),
    };

    assert!(
        !candidates
            .iter()
            .any(|candidate| candidate.action == invalid),
        "candidates: {candidates:#?}"
    );
    assert_ne!(
        select_h_group_action(&deductions, HGroupProfile::Max),
        Some(invalid),
        "candidates: {candidates:#?}"
    );
}

#[test]
fn anxiety_plays_the_known_blue_four_instead_of_blind_yellow_four() {
    let mut state = paired_sample_zero_state();
    for action in [
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Rank(Rank::Two),
        },
        Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Rank(Rank::One),
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
        Action::Play(CardId::new(0)),
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Rank(Rank::Five),
        },
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Suit(Suit::Blue),
        },
        Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Rank(Rank::Four),
        },
        Action::Play(CardId::new(7)),
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Rank(Rank::Five),
        },
    ] {
        state.apply(action).unwrap();
    }
    let deductions = LogicalDeductions::new(state.view_for(PlayerId::new(0)).unwrap()).unwrap();
    let inferred = infer_h_group(&deductions, HGroupProfile::Max);

    assert_eq!(
        select_h_group_action(&deductions, HGroupProfile::Max),
        Some(Action::Play(CardId::new(2))),
        "inference: {inferred:#?}"
    );
}

#[test]
fn green_clue_is_rejected_when_the_next_player_would_misplay_yellow_four() {
    let mut state = paired_sample_zero_state();
    for action in [
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Rank(Rank::Two),
        },
        Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Rank(Rank::One),
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
        Action::Play(CardId::new(0)),
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Rank(Rank::Five),
        },
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Suit(Suit::Blue),
        },
        Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Rank(Rank::Four),
        },
        Action::Play(CardId::new(7)),
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Rank(Rank::Five),
        },
        Action::Play(CardId::new(2)),
        Action::Discard(CardId::new(6)),
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Suit(Suit::Blue),
        },
        Action::Play(CardId::new(4)),
        Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Suit(Suit::Red),
        },
        Action::Discard(CardId::new(11)),
        Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Suit(Suit::Yellow),
        },
        Action::Discard(CardId::new(8)),
        Action::Play(CardId::new(22)),
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Rank(Rank::Two),
        },
        Action::Discard(CardId::new(18)),
        Action::Play(CardId::new(12)),
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Rank(Rank::Five),
        },
        Action::Discard(CardId::new(23)),
    ] {
        state.apply(action).unwrap();
    }
    let deductions = LogicalDeductions::new(state.view_for(PlayerId::new(2)).unwrap()).unwrap();
    let clue = Action::Clue {
        target: PlayerId::new(1),
        clue: Clue::Suit(Suit::Green),
    };
    let candidates = h_group_clue_candidates(&deductions, HGroupProfile::Max);

    assert!(
        candidates.iter().any(|candidate| candidate.action == clue),
        "expected the direct green-1 clue to be valid: {candidates:#?}"
    );
    state.apply(clue).unwrap();
    let receiver = LogicalDeductions::new(state.view_for(PlayerId::new(0)).unwrap()).unwrap();
    let inferred = infer_h_group(&receiver, HGroupProfile::Max);
    assert_ne!(
        select_h_group_action(&receiver, HGroupProfile::Max),
        Some(Action::Play(CardId::new(17))),
        "inference: {inferred:#?}"
    );
}

#[test]
fn fixed_red_one_is_not_a_priority_alternative_to_blue_four() {
    let mut state = paired_sample_two_state();
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
            clue: Clue::Suit(Suit::Purple),
        },
        Action::Play(CardId::new(0)),
        Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Suit(Suit::Red),
        },
        Action::Play(CardId::new(14)),
        Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Rank(Rank::One),
        },
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Rank(Rank::Five),
        },
        Action::Play(CardId::new(13)),
        Action::Play(CardId::new(4)),
        Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Suit(Suit::Purple),
        },
        Action::Play(CardId::new(10)),
        Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Rank(Rank::Four),
        },
        Action::Discard(CardId::new(6)),
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Rank(Rank::Five),
        },
        Action::Discard(CardId::new(2)),
        Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Suit(Suit::Blue),
        },
        Action::Play(CardId::new(19)),
        Action::Discard(CardId::new(3)),
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Rank(Rank::Two),
        },
        Action::Discard(CardId::new(11)),
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Suit(Suit::Blue),
        },
        Action::Play(CardId::new(7)),
    ] {
        state.apply(action).unwrap();
    }
    let deductions = LogicalDeductions::new(state.view_for(PlayerId::new(2)).unwrap()).unwrap();
    let inferred = infer_h_group(&deductions, HGroupProfile::Max);

    assert_eq!(
        select_h_group_action(&deductions, HGroupProfile::Max),
        Some(Action::Play(CardId::new(16))),
        "inference: {inferred:#?}"
    );
    state.apply(Action::Play(CardId::new(16))).unwrap();
    let next = LogicalDeductions::new(state.view_for(PlayerId::new(0)).unwrap()).unwrap();
    let next_inferred = infer_h_group(&next, HGroupProfile::Max);
    assert_ne!(
        select_h_group_action(&next, HGroupProfile::Max),
        Some(Action::Play(CardId::new(23))),
        "a fixed card cannot induce a Priority Finesse: {next_inferred:#?}"
    );
}

#[test]
fn delayed_green_four_without_connectors_is_not_a_play_clue() {
    let mut state = paired_sample_zero_state();
    for action in [
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Rank(Rank::Two),
        },
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Rank(Rank::Two),
        },
        Action::Play(CardId::new(14)),
        Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Suit(Suit::Blue),
        },
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Suit(Suit::Blue),
        },
        Action::Play(CardId::new(13)),
        Action::Play(CardId::new(0)),
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Rank(Rank::Five),
        },
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Suit(Suit::Blue),
        },
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Rank(Rank::One),
        },
        Action::Play(CardId::new(7)),
    ] {
        state.apply(action).unwrap();
    }
    let deductions = LogicalDeductions::new(state.view_for(PlayerId::new(2)).unwrap()).unwrap();
    let candidates = h_group_clue_candidates(&deductions, HGroupProfile::Max);

    assert!(
        !candidates.iter().any(|candidate| {
            candidate.action
                == Action::Clue {
                    target: PlayerId::new(0),
                    clue: Clue::Suit(Suit::Green),
                }
        }),
        "candidates: {candidates:#?}, inference: {:#?}",
        infer_h_group(&deductions, HGroupProfile::Max)
    );
}

#[test]
fn existing_blue_one_satisfies_a_later_blue_connection() {
    let mut state = paired_sample_zero_state();
    for action in [
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Rank(Rank::Two),
        },
        Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Rank(Rank::One),
        },
        Action::Play(CardId::new(14)),
        Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Suit(Suit::Blue),
        },
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Suit(Suit::Blue),
        },
    ] {
        state.apply(action).unwrap();
    }
    let deductions = LogicalDeductions::new(state.view_for(PlayerId::new(2)).unwrap()).unwrap();
    assert_eq!(
        select_h_group_action(&deductions, HGroupProfile::Max),
        Some(Action::Play(CardId::new(13))),
        "inference: {:#?}; clues: {:#?}; actions: {:#?}",
        infer_h_group(&deductions, HGroupProfile::Max),
        h_group_clue_candidates(&deductions, HGroupProfile::Max),
        ordered_h_group_actions(&deductions, HGroupProfile::Max),
    );
}

#[test]
fn red_five_connection_uses_existing_red_one_prompt() {
    let mut state = paired_sample_zero_state();
    for action in [
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Rank(Rank::Two),
        },
        Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Rank(Rank::One),
        },
        Action::Play(CardId::new(14)),
        Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Suit(Suit::Blue),
        },
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Suit(Suit::Blue),
        },
        Action::Play(CardId::new(13)),
        Action::Play(CardId::new(0)),
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Rank(Rank::Five),
        },
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Suit(Suit::Blue),
        },
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Suit(Suit::Blue),
        },
        Action::Play(CardId::new(7)),
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Suit(Suit::Red),
        },
        Action::Discard(CardId::new(3)),
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Suit(Suit::Red),
        },
    ] {
        state.apply(action).unwrap();
    }
    let deductions = LogicalDeductions::new(state.view_for(PlayerId::new(2)).unwrap()).unwrap();
    let inferred = infer_h_group(&deductions, HGroupProfile::Max);

    assert_ne!(
        select_h_group_action(&deductions, HGroupProfile::Max),
        Some(Action::Play(CardId::new(16))),
        "inference: {inferred:#?}"
    );
}

#[test]
fn layered_red_four_connection_is_not_discarded() {
    let mut state = paired_sample_zero_state();
    for action in [
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Rank(Rank::Two),
        },
        Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Rank(Rank::One),
        },
        Action::Play(CardId::new(14)),
        Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Suit(Suit::Blue),
        },
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Suit(Suit::Blue),
        },
        Action::Play(CardId::new(13)),
        Action::Play(CardId::new(0)),
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Rank(Rank::Five),
        },
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Suit(Suit::Blue),
        },
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Suit(Suit::Blue),
        },
        Action::Play(CardId::new(7)),
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Suit(Suit::Red),
        },
        Action::Discard(CardId::new(3)),
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Suit(Suit::Red),
        },
        Action::Discard(CardId::new(15)),
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Suit(Suit::Blue),
        },
        Action::Play(CardId::new(18)),
    ] {
        state.apply(action).unwrap();
    }
    let deductions = LogicalDeductions::new(state.view_for(PlayerId::new(2)).unwrap()).unwrap();
    let inferred = infer_h_group(&deductions, HGroupProfile::Max);

    assert_ne!(
        select_h_group_action(&deductions, HGroupProfile::Max),
        Some(Action::Discard(CardId::new(16))),
        "inference: {inferred:#?}"
    );
}

#[test]
fn multi_yellow_clue_does_not_play_non_focus_yellow_four() {
    let mut state = paired_sample_zero_state();
    for action in [
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Rank(Rank::Two),
        },
        Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Rank(Rank::One),
        },
        Action::Play(CardId::new(14)),
        Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Suit(Suit::Blue),
        },
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Suit(Suit::Blue),
        },
        Action::Play(CardId::new(13)),
        Action::Play(CardId::new(0)),
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Rank(Rank::Five),
        },
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Suit(Suit::Blue),
        },
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Suit(Suit::Blue),
        },
        Action::Play(CardId::new(7)),
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Suit(Suit::Red),
        },
        Action::Play(CardId::new(2)),
        Action::Discard(CardId::new(18)),
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Rank(Rank::Five),
        },
        Action::Play(CardId::new(4)),
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Suit(Suit::Yellow),
        },
        Action::Discard(CardId::new(15)),
    ] {
        state.apply(action).unwrap();
    }
    let deductions = LogicalDeductions::new(state.view_for(PlayerId::new(0)).unwrap()).unwrap();
    let inferred = infer_h_group(&deductions, HGroupProfile::Max);

    assert_ne!(
        select_h_group_action(&deductions, HGroupProfile::Max),
        Some(Action::Play(CardId::new(17))),
        "inference: {inferred:#?}"
    );
}

#[test]
fn extra_clue_does_not_create_an_invalid_purple_two_finesse() {
    let mut state = paired_sample_one_state();
    for action in [
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Rank(Rank::Two),
        },
        Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Rank(Rank::One),
        },
        Action::Play(CardId::new(14)),
        Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Suit(Suit::Blue),
        },
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Suit(Suit::Yellow),
        },
        Action::Play(CardId::new(13)),
        Action::Play(CardId::new(1)),
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Rank(Rank::One),
        },
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Suit(Suit::Yellow),
        },
        Action::Play(CardId::new(4)),
        Action::Play(CardId::new(6)),
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Suit(Suit::Yellow),
        },
        Action::Play(CardId::new(17)),
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Suit(Suit::Red),
        },
        Action::Play(CardId::new(12)),
        Action::Discard(CardId::new(0)),
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Suit(Suit::Green),
        },
        Action::Discard(CardId::new(15)),
    ] {
        state.apply(action).unwrap();
    }
    let deductions = LogicalDeductions::new(state.view_for(PlayerId::new(0)).unwrap()).unwrap();
    let candidates = h_group_clue_candidates(&deductions, HGroupProfile::Max);

    assert!(
        !candidates.iter().any(|candidate| {
            candidate.action
                == Action::Clue {
                    target: PlayerId::new(2),
                    clue: Clue::Rank(Rank::Two),
                }
        }),
        "candidates: {candidates:#?}"
    );
}

#[test]
fn color_clue_to_an_unplayable_five_is_not_a_play_clue() {
    let mut state = paired_sample_two_state();
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
            clue: Clue::Suit(Suit::Purple),
        },
        Action::Play(CardId::new(0)),
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Suit(Suit::Purple),
        },
        Action::Play(CardId::new(14)),
        Action::Play(CardId::new(4)),
        Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Suit(Suit::Purple),
        },
        Action::Play(CardId::new(10)),
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Rank(Rank::Two),
        },
    ] {
        state.apply(action).unwrap();
    }
    let deductions = LogicalDeductions::new(state.view_for(PlayerId::new(1)).unwrap()).unwrap();
    let candidates = h_group_clue_candidates(&deductions, HGroupProfile::Max);

    assert!(
        !candidates.iter().any(|candidate| {
            candidate.action
                == Action::Clue {
                    target: PlayerId::new(0),
                    clue: Clue::Suit(Suit::Red),
                }
        }),
        "candidates: {candidates:#?}"
    );
}

#[test]
fn bluff_is_not_given_to_a_player_with_a_queued_play() {
    let mut state = paired_sample_two_state();
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
            clue: Clue::Suit(Suit::Purple),
        },
        Action::Play(CardId::new(0)),
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Suit(Suit::Purple),
        },
        Action::Play(CardId::new(14)),
        Action::Play(CardId::new(4)),
        Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Suit(Suit::Purple),
        },
        Action::Play(CardId::new(10)),
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Rank(Rank::Two),
        },
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Rank(Rank::One),
        },
        Action::Play(CardId::new(13)),
        Action::Play(CardId::new(17)),
    ] {
        state.apply(action).unwrap();
    }
    let deductions = LogicalDeductions::new(state.view_for(PlayerId::new(1)).unwrap()).unwrap();
    let candidates = h_group_clue_candidates(&deductions, HGroupProfile::Max);

    assert!(
        !candidates.iter().any(|candidate| {
            candidate.action
                == Action::Clue {
                    target: PlayerId::new(0),
                    clue: Clue::Suit(Suit::Yellow),
                }
        }),
        "James already has a queued red 1, so a Bluff cannot resolve immediately: {candidates:#?}"
    );
}

#[test]
fn trash_chop_move_requires_publicly_known_trash() {
    let mut state = paired_sample_two_state();
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
            clue: Clue::Suit(Suit::Purple),
        },
        Action::Play(CardId::new(0)),
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Suit(Suit::Purple),
        },
        Action::Play(CardId::new(14)),
        Action::Play(CardId::new(4)),
        Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Suit(Suit::Purple),
        },
        Action::Play(CardId::new(10)),
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Rank(Rank::Two),
        },
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Rank(Rank::One),
        },
        Action::Play(CardId::new(13)),
        Action::Play(CardId::new(17)),
    ] {
        state.apply(action).unwrap();
    }
    let deductions = LogicalDeductions::new(state.view_for(PlayerId::new(1)).unwrap()).unwrap();
    let candidates = h_group_clue_candidates(&deductions, HGroupProfile::Max);

    assert!(
        !candidates.iter().any(|candidate| {
            candidate.action
                == Action::Clue {
                    target: PlayerId::new(0),
                    clue: Clue::Suit(Suit::Purple),
                }
        }),
        "the recipient could interpret the fresh purple 1 as purple 4: {candidates:#?}"
    );
}

#[test]
fn ordinary_discard_does_not_finesse_the_matching_card() {
    let mut state = paired_sample_two_state();
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
            clue: Clue::Suit(Suit::Purple),
        },
        Action::Play(CardId::new(0)),
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Suit(Suit::Purple),
        },
        Action::Play(CardId::new(14)),
        Action::Play(CardId::new(4)),
        Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Suit(Suit::Purple),
        },
        Action::Play(CardId::new(10)),
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Rank(Rank::Two),
        },
        Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Suit(Suit::Red),
        },
        Action::Play(CardId::new(12)),
        Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Suit(Suit::Red),
        },
        Action::Discard(CardId::new(7)),
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Rank(Rank::Two),
        },
        Action::Discard(CardId::new(2)),
    ] {
        state.apply(action).unwrap();
    }
    let deductions = LogicalDeductions::new(state.view_for(PlayerId::new(1)).unwrap()).unwrap();
    let inferred = infer_h_group(&deductions, HGroupProfile::Max);

    assert!(
        !inferred.playable_now.contains(&CardId::new(6)),
        "inference: {inferred:#?}"
    );
    assert_ne!(
        select_h_group_action(&deductions, HGroupProfile::Max),
        Some(Action::Play(CardId::new(6))),
        "inference: {inferred:#?}"
    );
}

#[test]
fn no_information_reclue_does_not_fix_an_unqueued_card() {
    let mut state = paired_sample_zero_state();
    for action in [
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Rank(Rank::Two),
        },
        Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Rank(Rank::One),
        },
        Action::Play(CardId::new(14)),
        Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Suit(Suit::Blue),
        },
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Suit(Suit::Blue),
        },
        Action::Play(CardId::new(13)),
        Action::Play(CardId::new(0)),
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Rank(Rank::Five),
        },
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Suit(Suit::Blue),
        },
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Suit(Suit::Blue),
        },
        Action::Play(CardId::new(7)),
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Suit(Suit::Red),
        },
        Action::Play(CardId::new(2)),
        Action::Discard(CardId::new(18)),
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Rank(Rank::Five),
        },
        Action::Play(CardId::new(4)),
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Suit(Suit::Yellow),
        },
        Action::Discard(CardId::new(15)),
        Action::Play(CardId::new(21)),
    ] {
        state.apply(action).unwrap();
    }
    let deductions = LogicalDeductions::new(state.view_for(PlayerId::new(1)).unwrap()).unwrap();
    let candidates = h_group_clue_candidates(&deductions, HGroupProfile::Max);

    assert!(
        !candidates.iter().any(|candidate| {
            candidate.action
                == Action::Clue {
                    target: PlayerId::new(0),
                    clue: Clue::Suit(Suit::Yellow),
                }
        }),
        "the yellow 4 is not queued and the clue adds no information: {candidates:#?}"
    );
}

#[test]
fn out_of_order_red_clue_is_not_given_without_time_for_a_fix() {
    let mut state = paired_sample_zero_state();
    for action in [
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Rank(Rank::Two),
        },
        Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Rank(Rank::One),
        },
        Action::Play(CardId::new(14)),
    ] {
        state.apply(action).unwrap();
    }
    let deductions = LogicalDeductions::new(state.view_for(PlayerId::new(0)).unwrap()).unwrap();
    let candidates = h_group_clue_candidates(&deductions, HGroupProfile::Max);

    assert!(
        !candidates.iter().any(|candidate| {
            candidate.action
                == Action::Clue {
                    target: PlayerId::new(1),
                    clue: Clue::Suit(Suit::Red),
                }
        }),
        "the recipient would act before the out-of-order Fix Clue: {candidates:#?}"
    );
}

#[test]
fn duplicate_rank_ones_are_fixed_before_optional_play_clues() {
    let mut state = paired_sample_one_state();
    for action in [
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Rank(Rank::Two),
        },
        Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Rank(Rank::One),
        },
        Action::Play(CardId::new(14)),
    ] {
        state.apply(action).unwrap();
    }
    let deductions = LogicalDeductions::new(state.view_for(PlayerId::new(0)).unwrap()).unwrap();

    assert_eq!(
        select_h_group_action(&deductions, HGroupProfile::Max),
        Some(Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Suit(Suit::Red),
        })
    );
}

#[test]
fn out_of_order_red_chain_requires_its_fix_clue() {
    let mut state = paired_sample_zero_state();
    for action in [
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Rank(Rank::Two),
        },
        Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Rank(Rank::One),
        },
        Action::Play(CardId::new(14)),
        Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Suit(Suit::Red),
        },
        Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Suit(Suit::Red),
        },
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Rank(Rank::One),
        },
        Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Suit(Suit::Red),
        },
        Action::Play(CardId::new(8)),
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Suit(Suit::Red),
        },
    ] {
        state.apply(action).unwrap();
    }
    let deductions = LogicalDeductions::new(state.view_for(PlayerId::new(0)).unwrap()).unwrap();
    let inferred = infer_h_group(&deductions, HGroupProfile::Max);

    assert_eq!(
        select_h_group_action(&deductions, HGroupProfile::Max),
        Some(Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Rank(Rank::Four),
        }),
        "the out-of-order red-4 focus must be fixed before the recipient acts: {inferred:#?}"
    );
}

#[test]
fn corrected_two_save_continuation_uses_a_yellow_one_before_discarding_its_duplicate() {
    let convention = crate::SupportedConvention::HGroup(HGroupProfile::Max);
    let mut state = paired_sample_five_state();
    state
        .apply(Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Rank(Rank::Two),
        })
        .unwrap();
    let report = continuation_for_search(state, convention).unwrap();

    assert_eq!(
        report.outcome.actions().first(),
        Some(&Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Rank(Rank::One),
        })
    );
    let actions = report.outcome.actions();
    let discard = actions
        .iter()
        .position(|action| *action == Action::Discard(CardId::new(14)));
    if let Some(discard) = discard {
        assert!(
            actions[..discard]
                .iter()
                .any(|action| *action == Action::Play(CardId::new(3))),
            "a live yellow 1 was discarded before its duplicate played: {actions:?}",
        );
    }
    assert!(
        actions.contains(&Action::Play(CardId::new(3)))
            || actions.contains(&Action::Play(CardId::new(14))),
        "neither yellow 1 was used: {actions:?}",
    );
}

#[test]
fn five_save_does_not_also_pull_the_adjacent_purple_four() {
    let state = paired_sample_five_after_second_five_save();
    let deductions = LogicalDeductions::new(state.view_for(PlayerId::new(0)).unwrap()).unwrap();
    let inferred = infer_h_group(&deductions, HGroupProfile::Max);

    assert!(
        inferred.saved.contains(&CardId::new(19)),
        "inference: {inferred:#?}"
    );
    assert!(!inferred.playable_now.contains(&CardId::new(4)));
    assert!(
        !inferred
            .signals
            .iter()
            .any(|signal| { signal.turn == 14 && signal.kind == HGroupMoveKind::FivePull })
    );
}

#[test]
fn paired_sample_zero_does_not_blind_play_a_fresh_red_five() {
    let mut state = paired_sample_zero_state();
    for action in [
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Rank(Rank::Two),
        },
        Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Rank(Rank::One),
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
        Action::Play(CardId::new(0)),
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Rank(Rank::Five),
        },
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Suit(Suit::Blue),
        },
        Action::Discard(CardId::new(1)),
        Action::Play(CardId::new(7)),
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Rank(Rank::Four),
        },
        Action::Play(CardId::new(2)),
    ] {
        state.apply(action).unwrap();
    }
    let deductions = LogicalDeductions::new(state.view_for(PlayerId::new(1)).unwrap()).unwrap();
    let inferred = infer_h_group(&deductions, HGroupProfile::Max);

    assert_ne!(
        select_h_group_action(&deductions, HGroupProfile::Max),
        Some(Action::Play(CardId::new(19))),
        "fresh red 5 was falsely forced playable: {inferred:#?}"
    );
}

#[test]
fn paired_sample_one_rank_one_continuation_does_not_misplay() {
    let mut state = paired_sample_one_state();
    state
        .apply(Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Rank(Rank::One),
        })
        .unwrap();
    let convention = crate::SupportedConvention::HGroup(HGroupProfile::Max);
    let report = continuation_for_search(state.clone(), convention).unwrap();

    for action in report.outcome.actions() {
        if let Action::Play(card) = action {
            let identity = state.card(*card).unwrap();
            if identity.rank.number()
                != u8::try_from(state.play_stacks()[identity.suit.index()].len()).unwrap() + 1
            {
                let actor = LogicalDeductions::new(state.view_for(state.current_player()).unwrap())
                    .unwrap();
                panic!(
                    "played {identity:?} when its stack was not ready: action {action:?}; inference {:#?}",
                    infer_h_group(&actor, HGroupProfile::Max)
                );
            }
        }
        state.apply(*action).unwrap();
    }
}

#[test]
fn paired_sample_zero_green_four_save_does_not_finesse_a_trash_blue_one() {
    let mut state = paired_sample_zero_state();
    for action in [
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Rank(Rank::Two),
        },
        Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Rank(Rank::One),
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
        Action::Play(CardId::new(0)),
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Rank(Rank::Five),
        },
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Suit(Suit::Blue),
        },
        Action::Discard(CardId::new(1)),
        Action::Play(CardId::new(7)),
        Action::Discard(CardId::new(11)),
        Action::Play(CardId::new(2)),
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Rank(Rank::Two),
        },
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Rank(Rank::Five),
        },
        Action::Play(CardId::new(4)),
        Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Rank(Rank::One),
        },
        Action::Play(CardId::new(12)),
        Action::Play(CardId::new(21)),
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Suit(Suit::Blue),
        },
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Rank(Rank::Three),
        },
        Action::Discard(CardId::new(18)),
        Action::Play(CardId::new(5)),
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Rank(Rank::Five),
        },
        Action::Discard(CardId::new(24)),
        Action::Play(CardId::new(6)),
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Suit(Suit::Green),
        },
        Action::Play(CardId::new(27)),
        Action::Discard(CardId::new(26)),
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Suit(Suit::Yellow),
        },
        Action::Discard(CardId::new(25)),
    ] {
        state.apply(action).unwrap();
    }
    state
        .apply(Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Suit(Suit::Green),
        })
        .unwrap();
    let deductions = LogicalDeductions::new(state.view_for(PlayerId::new(2)).unwrap()).unwrap();
    let inferred = infer_h_group(&deductions, HGroupProfile::Max);

    assert_ne!(
        select_h_group_action(&deductions, HGroupProfile::Max),
        Some(Action::Play(CardId::new(23))),
        "critical green-4 Save created a false green-2 finesse: {inferred:#?}"
    );
}

#[test]
#[allow(clippy::too_many_lines)]
fn paired_sample_zero_ordinary_chop_does_not_block_a_purple_three_save() {
    let mut state = paired_sample_zero_state();
    for action in [
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Rank(Rank::Two),
        },
        Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Rank(Rank::One),
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
        Action::Play(CardId::new(0)),
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Rank(Rank::Five),
        },
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Suit(Suit::Blue),
        },
        Action::Discard(CardId::new(1)),
        Action::Play(CardId::new(7)),
        Action::Discard(CardId::new(11)),
        Action::Play(CardId::new(2)),
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Rank(Rank::Two),
        },
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Rank(Rank::Five),
        },
        Action::Play(CardId::new(4)),
        Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Rank(Rank::One),
        },
        Action::Play(CardId::new(12)),
        Action::Play(CardId::new(21)),
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Suit(Suit::Blue),
        },
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Rank(Rank::Three),
        },
        Action::Discard(CardId::new(18)),
        Action::Play(CardId::new(5)),
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Rank(Rank::Five),
        },
        Action::Discard(CardId::new(24)),
        Action::Play(CardId::new(6)),
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Suit(Suit::Green),
        },
        Action::Play(CardId::new(27)),
        Action::Discard(CardId::new(26)),
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Suit(Suit::Yellow),
        },
        Action::Discard(CardId::new(25)),
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Suit(Suit::Green),
        },
        Action::Discard(CardId::new(15)),
        Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Suit(Suit::Red),
        },
        Action::Play(CardId::new(9)),
        Action::Play(CardId::new(16)),
        Action::Discard(CardId::new(31)),
    ] {
        state.apply(action).unwrap();
    }
    let save_clue = Action::Clue {
        target: PlayerId::new(2),
        clue: Clue::Suit(Suit::Purple),
    };
    let giver_deductions =
        LogicalDeductions::new(state.view_for(PlayerId::new(1)).unwrap()).unwrap();
    let candidates = h_group_clue_candidates(&giver_deductions, HGroupProfile::Max);
    assert!(
        candidates
            .iter()
            .any(|candidate| candidate.action == save_clue && candidate.save),
        "the ordinary chop discard was falsely promoted to an Emergency Discard: {candidates:#?}"
    );
    state.apply(save_clue).unwrap();
    for player in [PlayerId::new(0), PlayerId::new(2)] {
        let deductions = LogicalDeductions::new(state.view_for(player).unwrap()).unwrap();
        let inferred = infer_h_group(&deductions, HGroupProfile::Max);
        assert!(inferred.playable_now.is_empty() && inferred.connection.is_none());
    }
}

#[test]
fn paired_sample_zero_plays_saved_red_two_before_red_three() {
    let mut state = paired_sample_zero_state();
    for action in [
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Rank(Rank::Two),
        },
        Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Rank(Rank::One),
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
        Action::Play(CardId::new(0)),
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Rank(Rank::Five),
        },
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Suit(Suit::Blue),
        },
        Action::Discard(CardId::new(1)),
        Action::Play(CardId::new(7)),
        Action::Discard(CardId::new(11)),
        Action::Play(CardId::new(2)),
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Rank(Rank::Two),
        },
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Rank(Rank::Five),
        },
        Action::Play(CardId::new(4)),
        Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Rank(Rank::One),
        },
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Rank(Rank::Three),
        },
        Action::Play(CardId::new(22)),
        Action::Play(CardId::new(6)),
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Suit(Suit::Yellow),
        },
        Action::Discard(CardId::new(21)),
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Suit(Suit::Red),
        },
        Action::Play(CardId::new(12)),
        Action::Play(CardId::new(17)),
        Action::Discard(CardId::new(19)),
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Suit(Suit::Green),
        },
        Action::Play(CardId::new(27)),
        Action::Discard(CardId::new(24)),
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Suit(Suit::Red),
        },
        Action::Discard(CardId::new(18)),
        Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Rank(Rank::Five),
        },
        Action::Play(CardId::new(20)),
        Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Suit(Suit::Red),
        },
    ] {
        state.apply(action).unwrap();
    }
    let deductions = LogicalDeductions::new(state.view_for(PlayerId::new(1)).unwrap()).unwrap();
    let inferred = infer_h_group(&deductions, HGroupProfile::Max);

    assert_eq!(
        select_h_group_action(&deductions, HGroupProfile::Max),
        Some(Action::Play(CardId::new(5))),
        "Red-4 connection skipped the saved Red 2: {inferred:#?}"
    );
}

#[test]
fn paired_sample_zero_does_not_play_red_four_before_red_three() {
    let mut state = paired_sample_zero_state();
    for action in [
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Rank(Rank::Two),
        },
        Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Rank(Rank::One),
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
        Action::Play(CardId::new(0)),
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Rank(Rank::Five),
        },
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Suit(Suit::Blue),
        },
        Action::Discard(CardId::new(1)),
        Action::Play(CardId::new(7)),
        Action::Discard(CardId::new(11)),
        Action::Play(CardId::new(2)),
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Rank(Rank::Two),
        },
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Rank(Rank::Five),
        },
        Action::Play(CardId::new(4)),
        Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Rank(Rank::One),
        },
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Rank(Rank::Three),
        },
        Action::Play(CardId::new(22)),
        Action::Play(CardId::new(6)),
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Suit(Suit::Yellow),
        },
        Action::Discard(CardId::new(21)),
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Suit(Suit::Red),
        },
        Action::Play(CardId::new(12)),
        Action::Play(CardId::new(17)),
        Action::Discard(CardId::new(19)),
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Suit(Suit::Green),
        },
        Action::Play(CardId::new(27)),
        Action::Discard(CardId::new(24)),
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Suit(Suit::Red),
        },
        Action::Discard(CardId::new(18)),
        Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Rank(Rank::Five),
        },
        Action::Play(CardId::new(20)),
        Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Suit(Suit::Red),
        },
        Action::Play(CardId::new(5)),
    ] {
        state.apply(action).unwrap();
    }
    let deductions = LogicalDeductions::new(state.view_for(PlayerId::new(2)).unwrap()).unwrap();
    let inferred = infer_h_group(&deductions, HGroupProfile::Max);

    assert_ne!(
        select_h_group_action(&deductions, HGroupProfile::Max),
        Some(Action::Play(CardId::new(16))),
        "Red 4 was played before the missing Red 3: {inferred:#?}"
    );
}

#[test]
fn paired_sample_zero_rejects_a_purple_two_save_that_causes_false_anxiety() {
    let mut state = paired_sample_zero_state();
    for action in [
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Rank(Rank::Two),
        },
        Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Rank(Rank::One),
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
        Action::Play(CardId::new(0)),
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Rank(Rank::Five),
        },
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Suit(Suit::Blue),
        },
        Action::Discard(CardId::new(1)),
        Action::Play(CardId::new(7)),
        Action::Discard(CardId::new(11)),
        Action::Play(CardId::new(2)),
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Rank(Rank::Two),
        },
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Rank(Rank::Five),
        },
        Action::Play(CardId::new(4)),
        Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Rank(Rank::One),
        },
        Action::Play(CardId::new(12)),
        Action::Play(CardId::new(21)),
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Suit(Suit::Blue),
        },
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Rank(Rank::Three),
        },
        Action::Discard(CardId::new(18)),
        Action::Play(CardId::new(5)),
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Rank(Rank::Five),
        },
        Action::Discard(CardId::new(24)),
        Action::Play(CardId::new(6)),
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Suit(Suit::Green),
        },
        Action::Play(CardId::new(27)),
        Action::Discard(CardId::new(26)),
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Suit(Suit::Yellow),
        },
        Action::Discard(CardId::new(25)),
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Suit(Suit::Green),
        },
        Action::Discard(CardId::new(15)),
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Rank(Rank::Two),
        },
        Action::Play(CardId::new(28)),
        Action::Discard(CardId::new(16)),
    ] {
        state.apply(action).unwrap();
    }
    let deductions = LogicalDeductions::new(state.view_for(PlayerId::new(0)).unwrap()).unwrap();
    let candidates = h_group_clue_candidates(&deductions, HGroupProfile::Max);

    assert_ne!(
        select_h_group_action(&deductions, HGroupProfile::Max),
        Some(Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Suit(Suit::Purple),
        }),
        "critical purple-2 Save would cause false Anxiety: {candidates:#?}"
    );
}

#[test]
#[allow(clippy::too_many_lines)]
fn paired_sample_one_clues_the_playable_red_four_before_it_is_discarded() {
    let mut state = paired_sample_one_state();
    for action in [
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Rank(Rank::Two),
        },
        Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Rank(Rank::One),
        },
        Action::Play(CardId::new(14)),
        Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Suit(Suit::Red),
        },
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Suit(Suit::Yellow),
        },
        Action::Play(CardId::new(13)),
        Action::Play(CardId::new(1)),
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Rank(Rank::One),
        },
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Suit(Suit::Yellow),
        },
        Action::Play(CardId::new(4)),
        Action::Play(CardId::new(6)),
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Suit(Suit::Yellow),
        },
        Action::Play(CardId::new(17)),
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Rank(Rank::Five),
        },
        Action::Discard(CardId::new(11)),
        Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Rank(Rank::One),
        },
        Action::Discard(CardId::new(7)),
        Action::Play(CardId::new(12)),
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Rank(Rank::Three),
        },
        Action::Play(CardId::new(5)),
        Action::Discard(CardId::new(10)),
        Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Suit(Suit::Purple),
        },
        Action::Play(CardId::new(9)),
        Action::Discard(CardId::new(21)),
    ] {
        state.apply(action).unwrap();
    }
    let deductions = LogicalDeductions::new(state.view_for(PlayerId::new(0)).unwrap()).unwrap();
    let selected = select_h_group_action(&deductions, HGroupProfile::Max);

    assert!(
        matches!(
            selected,
            Some(Action::Clue {
                target,
                clue,
            }) if target == PlayerId::new(2)
                && (clue == Clue::Suit(Suit::Red) || clue == Clue::Rank(Rank::Four))
        ),
        "playable red 4 on chop was not protected: selected={selected:?}; hand={:#?}; inference={:#?}; candidates={:#?}; hazard={:?}",
        deductions.view().hands[2],
        infer_h_group(&deductions, HGroupProfile::Max),
        h_group_clue_candidates(&deductions, HGroupProfile::Max),
        prospective_clue_hazard(
            deductions.view(),
            HGroupProfile::Max,
            PlayerId::new(2),
            CardId::new(23),
            Clue::Suit(Suit::Red),
            &[CardId::new(23)],
            true,
        ),
    );
    state
        .apply(selected.expect("a red-4 clue was selected"))
        .unwrap();
    for action in [
        Action::Discard(CardId::new(8)),
        Action::Play(CardId::new(23)),
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Suit(Suit::Purple),
        },
        Action::Play(CardId::new(22)),
        Action::Discard(CardId::new(27)),
    ] {
        state.apply(action).unwrap();
    }
    let deductions = LogicalDeductions::new(state.view_for(PlayerId::new(0)).unwrap()).unwrap();
    let candidates = h_group_clue_candidates(&deductions, HGroupProfile::Max);
    assert_ne!(
        select_h_group_action(&deductions, HGroupProfile::Max),
        Some(Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Rank(Rank::Two),
        }),
        "trash yellow 2 was incorrectly saved: {candidates:#?}"
    );
}

#[test]
fn saved_red_two_survives_the_corrected_rank_two_continuation() {
    let convention = crate::SupportedConvention::HGroup(HGroupProfile::Max);
    let mut state = paired_sample_five_state();
    state
        .apply(Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Rank(Rank::Two),
        })
        .unwrap();
    let report = continuation_for_search(state, convention).unwrap();

    assert!(
        !report
            .outcome
            .actions()
            .contains(&Action::Discard(CardId::new(5))),
        "saved red 2 was discarded: {:?}",
        report.outcome.actions()
    );
}

#[test]
fn saved_red_two_is_held_until_another_red_two_plays() {
    let convention = crate::SupportedConvention::HGroup(HGroupProfile::Max);
    let mut state = paired_sample_two_state();
    state
        .apply(Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Rank(Rank::Two),
        })
        .unwrap();
    let report = continuation_for_search(state, convention).unwrap();
    if let Some(index) = report
        .outcome
        .actions()
        .iter()
        .position(|action| *action == Action::Discard(CardId::new(5)))
    {
        let mut before = paired_sample_two_state();
        before
            .apply(Action::Clue {
                target: PlayerId::new(1),
                clue: Clue::Rank(Rank::Two),
            })
            .unwrap();
        for action in &report.outcome.actions()[..index] {
            before.apply(*action).unwrap();
        }
        assert!(
            before.play_stacks()[Suit::Red.index()].len() >= usize::from(Rank::Two.number()),
            "saved red 2 was discarded before the team played a red 2: {:?}",
            report.outcome.actions()
        );
    }
}

#[test]
fn paired_sample_three_rank_four_clue_occupies_green_five_holder() {
    let mut state = paired_sample_three_state();
    for (index, action) in [
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Rank(Rank::Two),
        },
        Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Rank(Rank::One),
        },
        Action::Play(CardId::new(14)),
        Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Suit(Suit::Red),
        },
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Suit(Suit::Yellow),
        },
        Action::Play(CardId::new(13)),
        Action::Play(CardId::new(2)),
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Rank(Rank::Two),
        },
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Suit(Suit::Blue),
        },
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Suit(Suit::Yellow),
        },
        Action::Play(CardId::new(6)),
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Rank(Rank::Three),
        },
        Action::Play(CardId::new(17)),
        Action::Play(CardId::new(7)),
        Action::Discard(CardId::new(11)),
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Rank(Rank::One),
        },
        Action::Play(CardId::new(8)),
        Action::Discard(CardId::new(12)),
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Suit(Suit::Green),
        },
        Action::Discard(CardId::new(22)),
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Suit(Suit::Green),
        },
        Action::Discard(CardId::new(3)),
        Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Rank(Rank::Four),
        },
        Action::Discard(CardId::new(15)),
        Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Suit(Suit::Blue),
        },
        Action::Discard(CardId::new(24)),
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Suit(Suit::Green),
        },
        Action::Discard(CardId::new(4)),
        Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Rank(Rank::Four),
        },
    ]
    .into_iter()
    .enumerate()
    {
        state
            .apply(action)
            .unwrap_or_else(|error| panic!("action {index} {action:?}: {error:?}"));
    }
    let player = state.current_player();
    let deductions = LogicalDeductions::new(state.view_for(player).unwrap()).unwrap();
    let inferred = infer_h_group(&deductions, HGroupProfile::Max);

    assert_ne!(
        select_h_group_action(&deductions, HGroupProfile::Max),
        Some(Action::Discard(CardId::new(21))),
        "player: {player:?}; candidates: {:#?}; clues: {:#?}; inference: {inferred:#?}",
        ordered_h_group_actions(&deductions, HGroupProfile::Max),
        h_group_clue_candidates(&deductions, HGroupProfile::Max)
    );
}

#[test]
fn paired_sample_three_does_not_discard_the_unique_yellow_five() {
    let mut state = paired_sample_three_state();
    for (index, action) in [
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Rank(Rank::Two),
        },
        Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Rank(Rank::One),
        },
        Action::Play(CardId::new(14)),
        Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Suit(Suit::Red),
        },
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Suit(Suit::Yellow),
        },
        Action::Play(CardId::new(13)),
        Action::Play(CardId::new(2)),
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Rank(Rank::Two),
        },
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Suit(Suit::Blue),
        },
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Suit(Suit::Yellow),
        },
        Action::Play(CardId::new(6)),
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Rank(Rank::Three),
        },
        Action::Play(CardId::new(17)),
        Action::Play(CardId::new(7)),
        Action::Discard(CardId::new(11)),
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Rank(Rank::One),
        },
        Action::Play(CardId::new(8)),
        Action::Discard(CardId::new(12)),
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Suit(Suit::Green),
        },
        Action::Discard(CardId::new(18)),
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Rank(Rank::Two),
        },
        Action::Discard(CardId::new(3)),
        Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Rank(Rank::Four),
        },
        Action::Play(CardId::new(23)),
        Action::Discard(CardId::new(4)),
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Rank(Rank::Two),
        },
        Action::Discard(CardId::new(15)),
        Action::Play(CardId::new(25)),
        Action::Play(CardId::new(9)),
    ]
    .into_iter()
    .enumerate()
    {
        state
            .apply(action)
            .unwrap_or_else(|error| panic!("action {index} {action:?}: {error:?}"));
    }
    let deductions = LogicalDeductions::new(state.view_for(PlayerId::new(2)).unwrap()).unwrap();
    let inferred = infer_h_group(&deductions, HGroupProfile::Max);

    assert_eq!(
        select_h_group_action(&deductions, HGroupProfile::Max),
        Some(Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Rank(Rank::Five),
        }),
        "inference: {inferred:#?}; clues: {:#?}",
        h_group_clue_candidates(&deductions, HGroupProfile::Max),
    );
}

#[test]
#[allow(clippy::too_many_lines)]
fn paired_sample_three_does_not_play_yellow_four_before_yellow_three() {
    let mut state = paired_sample_three_state();
    for (turn, action) in [
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
            clue: Clue::Rank(Rank::Two),
        },
        Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Suit(Suit::Red),
        },
        Action::Discard(CardId::new(6)),
        Action::Play(CardId::new(14)),
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Rank(Rank::One),
        },
        Action::Play(CardId::new(8)),
        Action::Play(CardId::new(13)),
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Suit(Suit::Red),
        },
        Action::Play(CardId::new(5)),
        Action::Discard(CardId::new(11)),
        Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Suit(Suit::Green),
        },
        Action::Play(CardId::new(9)),
        Action::Play(CardId::new(20)),
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Rank(Rank::Two),
        },
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Rank(Rank::Two),
        },
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Suit(Suit::Green),
        },
        Action::Play(CardId::new(0)),
        Action::Discard(CardId::new(7)),
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Suit(Suit::Blue),
        },
        Action::Play(CardId::new(4)),
        Action::Play(CardId::new(17)),
        Action::Play(CardId::new(16)),
        Action::Discard(CardId::new(23)),
        Action::Play(CardId::new(15)),
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Suit(Suit::Blue),
        },
        Action::Discard(CardId::new(3)),
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Rank(Rank::Five),
        },
        Action::Discard(CardId::new(12)),
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Rank(Rank::Five),
        },
        Action::Discard(CardId::new(24)),
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Suit(Suit::Red),
        },
        Action::Discard(CardId::new(30)),
        Action::Play(CardId::new(32)),
        Action::Discard(CardId::new(18)),
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Suit(Suit::Yellow),
        },
        Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Rank(Rank::One),
        },
        Action::Play(CardId::new(35)),
        Action::Play(CardId::new(28)),
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Suit(Suit::Purple),
        },
        Action::Discard(CardId::new(36)),
        Action::Play(CardId::new(2)),
    ]
    .into_iter()
    .enumerate()
    {
        if turn == 36 {
            let giver = LogicalDeductions::new(state.view_for(PlayerId::new(0)).unwrap()).unwrap();
            let receiver =
                LogicalDeductions::new(state.view_for(PlayerId::new(1)).unwrap()).unwrap();
            assert_eq!(
                h_group_clue_candidates(&giver, HGroupProfile::Max)
                    .iter()
                    .find(|candidate| candidate.action == action)
                    .map(|candidate| candidate.save),
                Some(true),
                "yellow 4 should be given as a Critical Save"
            );
            assert_eq!(
                infer_h_group(&receiver, HGroupProfile::Max).chops[1],
                Some(CardId::new(26)),
                "public chop must agree before the clue: {:#?}",
                infer_h_group(&receiver, HGroupProfile::Max)
            );
        }
        state.apply(action).unwrap();
        if turn == 36 {
            let receiver =
                LogicalDeductions::new(state.view_for(PlayerId::new(1)).unwrap()).unwrap();
            let clue = infer_h_group(&receiver, HGroupProfile::Max)
                .clues
                .into_iter()
                .last()
                .unwrap();
            assert!(clue.focus_was_chop && !clue.save_identities.is_empty());
        }
    }
    let deductions = LogicalDeductions::new(state.view_for(PlayerId::new(1)).unwrap()).unwrap();
    let inferred = infer_h_group(&deductions, HGroupProfile::Max);
    let saved_clue = inferred.clues.iter().find(|clue| clue.turn == 36).unwrap();

    assert!(saved_clue.focus_was_chop && !saved_clue.save_identities.is_empty());

    assert_ne!(
        select_h_group_action(&deductions, HGroupProfile::Max),
        Some(Action::Play(CardId::new(26))),
        "yellow 4 cannot play before yellow 3: {inferred:#?}"
    );
}

#[test]
fn paired_sample_eight_clears_the_layered_finesse_after_the_red_three_prompt() {
    let mut state = paired_sample_eight_state();
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
            clue: Clue::Rank(Rank::Two),
        },
        Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Suit(Suit::Red),
        },
        Action::Discard(CardId::new(6)),
        Action::Play(CardId::new(14)),
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Rank(Rank::One),
        },
        Action::Play(CardId::new(8)),
        Action::Play(CardId::new(13)),
        Action::Discard(CardId::new(1)),
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Suit(Suit::Purple),
        },
        Action::Discard(CardId::new(11)),
        Action::Play(CardId::new(2)),
        Action::Play(CardId::new(5)),
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Suit(Suit::Red),
        },
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Rank(Rank::Four),
        },
        Action::Play(CardId::new(9)),
    ] {
        state.apply(action).unwrap();
    }
    state
        .apply(Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Suit(Suit::Green),
        })
        .unwrap();
    let actor = LogicalDeductions::new(state.view_for(PlayerId::new(0)).unwrap()).unwrap();
    let inferred = infer_h_group(&actor, HGroupProfile::Max);

    assert_ne!(
        h_group_predictable_action(&actor, HGroupProfile::Max),
        Some(Action::Play(CardId::new(21))),
        "the successful red 3 Prompt must clear the false layered alternative: {inferred:#?}"
    );
}

#[test]
fn paired_sample_eight_rejects_rank_three_with_a_false_red_two_prompt() {
    let mut state = paired_sample_eight_state();
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
            clue: Clue::Rank(Rank::Two),
        },
        Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Suit(Suit::Red),
        },
        Action::Discard(CardId::new(6)),
        Action::Play(CardId::new(14)),
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Rank(Rank::One),
        },
        Action::Play(CardId::new(8)),
        Action::Play(CardId::new(13)),
        Action::Discard(CardId::new(1)),
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Suit(Suit::Blue),
        },
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Rank(Rank::Two),
        },
        Action::Play(CardId::new(19)),
        Action::Discard(CardId::new(7)),
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Suit(Suit::Red),
        },
    ] {
        state.apply(action).unwrap();
    }
    let giver = LogicalDeductions::new(state.view_for(PlayerId::new(0)).unwrap()).unwrap();
    let bad_clue = Action::Clue {
        target: PlayerId::new(1),
        clue: Clue::Rank(Rank::Three),
    };
    let touched = vec![CardId::new(9)];

    let candidates = h_group_clue_candidates(&giver, HGroupProfile::Max);
    assert!(
        !candidates
            .iter()
            .any(|candidate| candidate.action == bad_clue),
        "rank 3 would Prompt the newer red-clued red 4 before the saved red 2; candidates={candidates:#?}; hazard={:?}",
        prospective_clue_hazard(
            giver.view(),
            HGroupProfile::Max,
            PlayerId::new(1),
            CardId::new(9),
            Clue::Rank(Rank::Three),
            &touched,
            true,
        )
    );
}

#[test]
fn paired_sample_eleven_rejects_rank_two_after_an_ordinary_discard() {
    let mut state = paired_sample_eleven_state();
    for action in [
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Rank(Rank::Two),
        },
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Rank(Rank::One),
        },
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Rank(Rank::Five),
        },
        Action::Play(CardId::new(2)),
        Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Suit(Suit::Blue),
        },
        Action::Play(CardId::new(13)),
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Rank(Rank::One),
        },
        Action::Play(CardId::new(8)),
        Action::Discard(CardId::new(10)),
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Suit(Suit::Red),
        },
        Action::Play(CardId::new(5)),
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Suit(Suit::Purple),
        },
        Action::Discard(CardId::new(1)),
        Action::Play(CardId::new(19)),
        Action::Discard(CardId::new(11)),
        Action::Discard(CardId::new(3)),
        Action::Play(CardId::new(9)),
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Suit(Suit::Blue),
        },
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Rank(Rank::Four),
        },
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Rank(Rank::Five),
        },
    ] {
        state.apply(action).unwrap();
    }
    let giver = LogicalDeductions::new(state.view_for(PlayerId::new(2)).unwrap()).unwrap();
    let clue = Action::Clue {
        target: PlayerId::new(1),
        clue: Clue::Rank(Rank::Two),
    };
    let candidates = h_group_clue_candidates(&giver, HGroupProfile::Max);
    assert!(
        !candidates.iter().any(|candidate| candidate.action == clue),
        "the giver invented an Emergency Discard that the actor saw as ordinary: {candidates:#?}"
    );
    state.apply(clue).unwrap();
    let recipient = LogicalDeductions::new(state.view_for(PlayerId::new(1)).unwrap()).unwrap();
    let inferred = infer_h_group(&recipient, HGroupProfile::Max);
    let interpretation = inferred.clues.last().unwrap();
    assert!(
        !interpretation.focus_was_chop && interpretation.save_identities.is_empty(),
        "the recipient incorrectly treated the unsafe clue as a 2 Save: {interpretation:#?}"
    );
}

#[test]
fn paired_sample_twelve_rejects_a_rank_one_that_can_duplicate_own_focus() {
    let mut state = paired_sample_twelve_state();
    for action in [
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Rank(Rank::Two),
        },
        Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Rank(Rank::One),
        },
    ] {
        state.apply(action).unwrap();
    }
    let actor = LogicalDeductions::new(state.view_for(PlayerId::new(2)).unwrap()).unwrap();
    let bad_clue = Action::Clue {
        target: PlayerId::new(0),
        clue: Clue::Rank(Rank::One),
    };

    assert!(
        !h_group_clue_candidates(&actor, HGroupProfile::Max)
            .iter()
            .any(|candidate| candidate.action == bad_clue),
        "the visible yellow 1 overlaps the giver's unresolved clued 1: {:#?}",
        infer_h_group(&actor, HGroupProfile::Max)
    );
}

#[test]
#[allow(clippy::too_many_lines)]
fn paired_sample_one_saves_the_remaining_purple_three() {
    let mut state = paired_sample_one_state();
    let actions = [
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Rank(Rank::Two),
        },
        Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Rank(Rank::One),
        },
        Action::Play(CardId::new(14)),
        Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Suit(Suit::Red),
        },
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Suit(Suit::Yellow),
        },
        Action::Play(CardId::new(13)),
        Action::Play(CardId::new(1)),
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Rank(Rank::One),
        },
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Suit(Suit::Yellow),
        },
        Action::Play(CardId::new(4)),
        Action::Play(CardId::new(6)),
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Suit(Suit::Yellow),
        },
        Action::Play(CardId::new(17)),
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Rank(Rank::Five),
        },
        Action::Discard(CardId::new(11)),
        Action::Play(CardId::new(2)),
        Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Rank(Rank::Two),
        },
        Action::Play(CardId::new(12)),
        Action::Discard(CardId::new(18)),
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Suit(Suit::Purple),
        },
        Action::Play(CardId::new(21)),
        Action::Play(CardId::new(22)),
        Action::Play(CardId::new(5)),
        Action::Play(CardId::new(16)),
        Action::Discard(CardId::new(20)),
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Rank(Rank::Three),
        },
        Action::Discard(CardId::new(15)),
        Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Suit(Suit::Green),
        },
        Action::Discard(CardId::new(7)),
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Rank(Rank::Five),
        },
        Action::Discard(CardId::new(29)),
        Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Suit(Suit::Red),
        },
        Action::Play(CardId::new(30)),
        Action::Play(CardId::new(32)),
        Action::Discard(CardId::new(8)),
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Suit(Suit::Red),
        },
        Action::Play(CardId::new(0)),
        Action::Play(CardId::new(9)),
        Action::Play(CardId::new(23)),
        Action::Discard(CardId::new(34)),
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Suit(Suit::Red),
        },
        Action::Discard(CardId::new(25)),
        Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Suit(Suit::Green),
        },
        Action::Discard(CardId::new(27)),
        Action::Play(CardId::new(40)),
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Suit(Suit::Yellow),
        },
        Action::Discard(CardId::new(31)),
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Suit(Suit::Red),
        },
        Action::Play(CardId::new(36)),
    ];
    for (index, action) in actions.into_iter().enumerate() {
        state
            .apply(action)
            .unwrap_or_else(|error| panic!("action {index} {action:?}: {error:?}"));
    }
    let deductions = LogicalDeductions::new(state.view_for(PlayerId::new(1)).unwrap()).unwrap();
    let inferred = infer_h_group(&deductions, HGroupProfile::Max);

    assert_eq!(
        select_h_group_action(&deductions, HGroupProfile::Max),
        Some(Action::Play(CardId::new(35))),
        "inference: {inferred:#?}; clues: {:#?}; actions: {:#?}",
        h_group_clue_candidates(&deductions, HGroupProfile::Max),
        ordered_h_group_actions(&deductions, HGroupProfile::Max),
    );
}

#[test]
fn paired_sample_one_rejects_a_rank_two_clue_that_duplicates_saved_red_two() {
    let mut state = paired_sample_one_state();
    for action in [
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Rank(Rank::Two),
        },
        Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Rank(Rank::One),
        },
        Action::Play(CardId::new(14)),
        Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Suit(Suit::Red),
        },
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Suit(Suit::Yellow),
        },
        Action::Play(CardId::new(13)),
        Action::Play(CardId::new(1)),
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Rank(Rank::One),
        },
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Suit(Suit::Yellow),
        },
        Action::Play(CardId::new(4)),
        Action::Play(CardId::new(6)),
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Suit(Suit::Yellow),
        },
        Action::Play(CardId::new(17)),
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Rank(Rank::Five),
        },
        Action::Discard(CardId::new(11)),
        Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Rank(Rank::One),
        },
        Action::Discard(CardId::new(7)),
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Rank(Rank::One),
        },
        Action::Discard(CardId::new(2)),
        Action::Play(CardId::new(8)),
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Rank(Rank::Two),
        },
        Action::Play(CardId::new(0)),
        Action::Play(CardId::new(22)),
        Action::Discard(CardId::new(12)),
    ] {
        state.apply(action).unwrap();
    }
    let deductions = LogicalDeductions::new(state.view_for(PlayerId::new(0)).unwrap()).unwrap();
    let bad_clue = Action::Clue {
        target: PlayerId::new(2),
        clue: Clue::Rank(Rank::Two),
    };
    let candidates = h_group_clue_candidates(&deductions, HGroupProfile::Max);

    assert!(
        !candidates
            .iter()
            .any(|candidate| candidate.action == bad_clue),
        "Good Touch allowed a duplicate Red 2 clue: {candidates:#?}"
    );
}

#[test]
#[allow(clippy::too_many_lines)]
fn paired_sample_one_does_not_discard_the_last_blue_four_off_chop() {
    let mut state = paired_sample_one_state();
    for action in [
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Rank(Rank::Two),
        },
        Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Rank(Rank::One),
        },
        Action::Play(CardId::new(14)),
        Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Suit(Suit::Red),
        },
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Suit(Suit::Yellow),
        },
        Action::Play(CardId::new(13)),
        Action::Play(CardId::new(1)),
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Rank(Rank::One),
        },
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Suit(Suit::Yellow),
        },
        Action::Play(CardId::new(4)),
        Action::Play(CardId::new(6)),
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Rank(Rank::One),
        },
        Action::Discard(CardId::new(0)),
        Action::Play(CardId::new(8)),
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Suit(Suit::Green),
        },
        Action::Play(CardId::new(2)),
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Rank(Rank::Four),
        },
        Action::Discard(CardId::new(11)),
        Action::Play(CardId::new(17)),
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Suit(Suit::Purple),
        },
        Action::Discard(CardId::new(12)),
        Action::Play(CardId::new(22)),
        Action::Play(CardId::new(5)),
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Rank(Rank::Three),
        },
        Action::Play(CardId::new(26)),
        Action::Discard(CardId::new(7)),
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Rank(Rank::Five),
        },
        Action::Discard(CardId::new(28)),
        Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Rank(Rank::Two),
        },
        Action::Play(CardId::new(16)),
        Action::Discard(CardId::new(30)),
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Suit(Suit::Blue),
        },
        Action::Discard(CardId::new(10)),
        Action::Play(CardId::new(18)),
        Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Suit(Suit::Purple),
        },
        Action::Play(CardId::new(15)),
        Action::Play(CardId::new(20)),
        Action::Discard(CardId::new(9)),
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Rank(Rank::Four),
        },
        Action::Discard(CardId::new(34)),
        Action::Play(CardId::new(19)),
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Suit(Suit::Red),
        },
        Action::Play(CardId::new(36)),
        Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Rank(Rank::Five),
        },
        Action::Play(CardId::new(25)),
        Action::Play(CardId::new(3)),
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Rank(Rank::Two),
        },
        Action::Play(CardId::new(35)),
        Action::Play(CardId::new(32)),
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Suit(Suit::Green),
        },
        Action::Discard(CardId::new(23)),
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Suit(Suit::Red),
        },
        Action::Discard(CardId::new(21)),
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Suit(Suit::Purple),
        },
        Action::Discard(CardId::new(40)),
        Action::Play(CardId::new(46)),
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Suit(Suit::Green),
        },
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Suit(Suit::Green),
        },
        Action::Discard(CardId::new(37)),
    ] {
        state.apply(action).unwrap();
    }
    let deductions = LogicalDeductions::new(state.view_for(PlayerId::new(2)).unwrap()).unwrap();
    let selected = select_h_group_action(&deductions, HGroupProfile::Max);
    state
        .apply(selected.expect("the clue giver has an action"))
        .unwrap();
    let actor = LogicalDeductions::new(state.view_for(PlayerId::new(0)).unwrap()).unwrap();
    let inferred = infer_h_group(&actor, HGroupProfile::Max);

    assert_ne!(
        select_h_group_action(&actor, HGroupProfile::Max),
        Some(Action::Discard(CardId::new(47))),
        "the last Blue 4 was discarded off chop: chops={:?}; moved={:?}; invisible={:?}; discard_now={:?}; note={:?}; hand={:#?}; tokens={}; candidates={:#?}",
        inferred.chops,
        inferred.chop_moved,
        inferred.invisibly_clued,
        inferred.discard_now,
        inferred
            .cards
            .iter()
            .find(|card| card.card == CardId::new(47)),
        actor.view().hands[0],
        actor.view().clue_tokens,
        ordered_h_group_actions(&actor, HGroupProfile::Max),
    );
}

#[test]
fn paired_sample_two_rejects_a_discharge_whose_focus_is_not_known_trash() {
    let mut state = paired_sample_two_state();
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
            clue: Clue::Suit(Suit::Purple),
        },
        Action::Play(CardId::new(0)),
        Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Suit(Suit::Red),
        },
        Action::Play(CardId::new(14)),
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Rank(Rank::One),
        },
        Action::Play(CardId::new(8)),
        Action::Play(CardId::new(13)),
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Suit(Suit::Red),
        },
        Action::Play(CardId::new(5)),
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Suit(Suit::Blue),
        },
        Action::Discard(CardId::new(1)),
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Rank(Rank::Two),
        },
        Action::Discard(CardId::new(11)),
        Action::Play(CardId::new(4)),
        Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Suit(Suit::Purple),
        },
        Action::Play(CardId::new(10)),
        Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Rank(Rank::Two),
        },
        Action::Play(CardId::new(19)),
        Action::Play(CardId::new(21)),
        Action::Discard(CardId::new(3)),
        Action::Play(CardId::new(9)),
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Suit(Suit::Green),
        },
        Action::Play(CardId::new(26)),
        Action::Discard(CardId::new(17)),
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Suit(Suit::Green),
        },
        Action::Play(CardId::new(28)),
        Action::Discard(CardId::new(24)),
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Rank(Rank::Five),
        },
        Action::Discard(CardId::new(20)),
        Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Suit(Suit::Red),
        },
        Action::Discard(CardId::new(12)),
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Suit(Suit::Yellow),
        },
        Action::Discard(CardId::new(29)),
    ] {
        state.apply(action).unwrap();
    }
    let deductions = LogicalDeductions::new(state.view_for(PlayerId::new(2)).unwrap()).unwrap();
    let candidates = h_group_clue_candidates(&deductions, HGroupProfile::Max);

    assert!(
        !candidates.iter().any(|candidate| {
            candidate.action
                == Action::Clue {
                    target: PlayerId::new(0),
                    clue: Clue::Suit(Suit::Red),
                }
        }),
        "the giver called Red 2/R5 an Unknown Trash Discharge even though the recipient could not know that Red 2 was the focus: {candidates:#?}"
    );
}

#[test]
fn paired_sample_four_discards_instead_of_recluing_a_trash_blue_one() {
    let mut state = paired_sample_four_state();
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
        Action::Play(CardId::new(4)),
        Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Suit(Suit::Red),
        },
        Action::Play(CardId::new(14)),
        Action::Play(CardId::new(0)),
        Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Suit(Suit::Blue),
        },
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Rank(Rank::One),
        },
    ] {
        state.apply(action).unwrap();
    }
    let deductions = LogicalDeductions::new(state.view_for(PlayerId::new(0)).unwrap()).unwrap();
    let inferred = infer_h_group(&deductions, HGroupProfile::Max);

    assert_eq!(
        select_h_group_action(&deductions, HGroupProfile::Max),
        Some(Action::Discard(CardId::new(1))),
        "the policy burned a clue retouching a trash Blue 1 instead of recovering a token from chop: {inferred:#?}"
    );
}

#[test]
fn paired_sample_five_treats_a_number_two_on_chop_as_a_save() {
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
            target: PlayerId::new(2),
            clue: Clue::Suit(Suit::Red),
        },
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Rank(Rank::Five),
        },
        Action::Play(CardId::new(0)),
        Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Suit(Suit::Yellow),
        },
        Action::Play(CardId::new(13)),
        Action::Play(CardId::new(1)),
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Rank(Rank::Two),
        },
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Rank(Rank::One),
        },
        Action::Play(CardId::new(15)),
        Action::Discard(CardId::new(8)),
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Rank(Rank::Three),
        },
        Action::Play(CardId::new(19)),
        Action::Play(CardId::new(6)),
        Action::Discard(CardId::new(11)),
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Rank(Rank::Two),
        },
    ] {
        state.apply(action).unwrap();
    }
    let deductions = LogicalDeductions::new(state.view_for(PlayerId::new(1)).unwrap()).unwrap();
    let inferred = infer_h_group(&deductions, HGroupProfile::Max);

    assert_eq!(
        select_h_group_action(&deductions, HGroupProfile::Max),
        Some(Action::Discard(CardId::new(22))),
        "the number 2 clue on chop was incorrectly treated as a direct play: {inferred:#?}"
    );
    assert!(inferred.saved.contains(&CardId::new(20)));
}

#[test]
fn paired_sample_five_does_not_play_purple_three_before_purple_two() {
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
            target: PlayerId::new(2),
            clue: Clue::Suit(Suit::Red),
        },
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Rank(Rank::Five),
        },
        Action::Play(CardId::new(0)),
        Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Suit(Suit::Yellow),
        },
        Action::Play(CardId::new(13)),
        Action::Play(CardId::new(1)),
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Rank(Rank::Two),
        },
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Rank(Rank::One),
        },
        Action::Play(CardId::new(15)),
        Action::Discard(CardId::new(8)),
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Rank(Rank::Three),
        },
        Action::Play(CardId::new(19)),
        Action::Play(CardId::new(6)),
        Action::Discard(CardId::new(11)),
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Rank(Rank::Two),
        },
        Action::Discard(CardId::new(22)),
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Rank(Rank::Five),
        },
    ] {
        state.apply(action).unwrap();
    }
    let deductions = LogicalDeductions::new(state.view_for(PlayerId::new(0)).unwrap()).unwrap();
    let inferred = infer_h_group(&deductions, HGroupProfile::Max);

    assert_ne!(
        select_h_group_action(&deductions, HGroupProfile::Max),
        Some(Action::Play(CardId::new(18))),
        "Purple 3 was forced before the saved Purple 2: {inferred:#?}"
    );
}

#[test]
#[allow(clippy::too_many_lines)]
fn paired_sample_one_holds_saved_blue_four_until_blue_three_plays() {
    let mut state = paired_sample_one_state();
    for action in [
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Rank(Rank::Two),
        },
        Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Rank(Rank::One),
        },
        Action::Play(CardId::new(14)),
        Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Suit(Suit::Red),
        },
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Suit(Suit::Yellow),
        },
        Action::Play(CardId::new(13)),
        Action::Play(CardId::new(1)),
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Rank(Rank::One),
        },
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Suit(Suit::Yellow),
        },
        Action::Play(CardId::new(4)),
        Action::Play(CardId::new(6)),
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Rank(Rank::One),
        },
        Action::Discard(CardId::new(0)),
        Action::Play(CardId::new(8)),
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Suit(Suit::Green),
        },
        Action::Play(CardId::new(2)),
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Rank(Rank::Four),
        },
        Action::Discard(CardId::new(11)),
        Action::Play(CardId::new(17)),
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Suit(Suit::Purple),
        },
        Action::Discard(CardId::new(12)),
        Action::Play(CardId::new(22)),
        Action::Play(CardId::new(5)),
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Rank(Rank::Three),
        },
        Action::Play(CardId::new(26)),
        Action::Discard(CardId::new(7)),
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Rank(Rank::Five),
        },
        Action::Discard(CardId::new(28)),
        Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Rank(Rank::Two),
        },
        Action::Play(CardId::new(16)),
        Action::Discard(CardId::new(30)),
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Suit(Suit::Blue),
        },
        Action::Discard(CardId::new(10)),
        Action::Play(CardId::new(18)),
        Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Suit(Suit::Purple),
        },
        Action::Play(CardId::new(15)),
        Action::Play(CardId::new(20)),
        Action::Discard(CardId::new(9)),
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Rank(Rank::Four),
        },
        Action::Discard(CardId::new(34)),
        Action::Play(CardId::new(19)),
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Suit(Suit::Red),
        },
        Action::Play(CardId::new(36)),
        Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Rank(Rank::Five),
        },
        Action::Play(CardId::new(25)),
        Action::Play(CardId::new(3)),
        Action::Discard(CardId::new(27)),
        Action::Play(CardId::new(35)),
        Action::Discard(CardId::new(38)),
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Rank(Rank::Two),
        },
        Action::Discard(CardId::new(23)),
        Action::Play(CardId::new(32)),
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Suit(Suit::Blue),
        },
        Action::Play(CardId::new(46)),
    ] {
        state.apply(action).unwrap();
    }
    let deductions = LogicalDeductions::new(state.view_for(PlayerId::new(0)).unwrap()).unwrap();
    let inferred = infer_h_group(&deductions, HGroupProfile::Max);

    assert_ne!(
        select_h_group_action(&deductions, HGroupProfile::Max),
        Some(Action::Play(CardId::new(47))),
        "the critical Blue 4 Save was played before Blue 3: {inferred:#?}"
    );
}

#[test]
fn paired_sample_four_plays_red_two_before_red_five() {
    let mut state = paired_sample_four_state();
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
        Action::Play(CardId::new(4)),
        Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Suit(Suit::Red),
        },
        Action::Play(CardId::new(14)),
        Action::Play(CardId::new(0)),
        Action::Discard(CardId::new(6)),
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Rank(Rank::One),
        },
        Action::Discard(CardId::new(1)),
        Action::Play(CardId::new(8)),
        Action::Discard(CardId::new(11)),
        Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Suit(Suit::Red),
        },
        Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Suit(Suit::Purple),
        },
        Action::Play(CardId::new(16)),
        Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Suit(Suit::Red),
        },
        Action::Discard(CardId::new(7)),
        Action::Discard(CardId::new(12)),
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Suit(Suit::Red),
        },
    ] {
        state.apply(action).unwrap();
    }
    let deductions = LogicalDeductions::new(state.view_for(PlayerId::new(1)).unwrap()).unwrap();
    let inferred = infer_h_group(&deductions, HGroupProfile::Max);

    assert_eq!(
        select_h_group_action(&deductions, HGroupProfile::Max),
        Some(Action::Play(CardId::new(5))),
        "the delayed Red 5 focus bypassed its Red 2 connector: {inferred:#?}"
    );
}

#[test]
fn unknown_chop_moved_card_does_not_trigger_another_emergency_discard() {
    let mut state = paired_sample_one_state();
    for action in [
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Rank(Rank::Two),
        },
        Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Rank(Rank::One),
        },
        Action::Play(CardId::new(14)),
        Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Suit(Suit::Red),
        },
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Suit(Suit::Yellow),
        },
        Action::Play(CardId::new(13)),
        Action::Play(CardId::new(1)),
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Rank(Rank::One),
        },
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Suit(Suit::Yellow),
        },
        Action::Play(CardId::new(4)),
        Action::Play(CardId::new(6)),
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Suit(Suit::Yellow),
        },
        Action::Play(CardId::new(17)),
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Rank(Rank::Five),
        },
        Action::Discard(CardId::new(11)),
        Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Rank(Rank::One),
        },
        Action::Discard(CardId::new(7)),
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Suit(Suit::Purple),
        },
        Action::Discard(CardId::new(2)),
    ] {
        state.apply(action).unwrap();
    }
    let deductions = LogicalDeductions::new(state.view_for(PlayerId::new(1)).unwrap()).unwrap();
    let inferred = infer_h_group(&deductions, HGroupProfile::Max);
    assert!(
        !inferred.chop_moved.contains(&CardId::new(8)),
        "a merely chop-moved Green 2 was treated as known playable: {inferred:#?}"
    );
}

#[test]
fn play_that_unlocks_a_saved_card_preempts_an_unrelated_play_clue() {
    let mut state = paired_sample_one_state();
    for action in [
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Rank(Rank::Two),
        },
        Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Rank(Rank::One),
        },
        Action::Play(CardId::new(14)),
        Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Suit(Suit::Red),
        },
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Suit(Suit::Yellow),
        },
        Action::Play(CardId::new(13)),
        Action::Play(CardId::new(1)),
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Rank(Rank::One),
        },
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Suit(Suit::Yellow),
        },
        Action::Play(CardId::new(4)),
        Action::Play(CardId::new(6)),
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Suit(Suit::Yellow),
        },
        Action::Play(CardId::new(17)),
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Rank(Rank::Five),
        },
        Action::Discard(CardId::new(11)),
        Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Rank(Rank::One),
        },
        Action::Discard(CardId::new(7)),
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Suit(Suit::Purple),
        },
        Action::Discard(CardId::new(2)),
        Action::Play(CardId::new(22)),
    ] {
        state.apply(action).unwrap();
    }
    let deductions = LogicalDeductions::new(state.view_for(PlayerId::new(2)).unwrap()).unwrap();

    let selected = select_h_group_action(&deductions, HGroupProfile::Max);
    assert!(
        matches!(
            selected,
            Some(Action::Clue {
                target,
                clue: Clue::Rank(Rank::One) | Clue::Suit(Suit::Red),
            }) if target == PlayerId::new(1)
        ),
        "the intervening Green 2 play would expose Red 1 on chop: {selected:?}; hand={:#?}; inference={:#?}; clues={:#?}; rank1_hazard={:?}",
        deductions.view().hands[1],
        infer_h_group(&deductions, HGroupProfile::Max),
        h_group_clue_candidates(&deductions, HGroupProfile::Max),
        prospective_clue_hazard(
            deductions.view(),
            HGroupProfile::Max,
            PlayerId::new(1),
            CardId::new(8),
            Clue::Rank(Rank::One),
            &[CardId::new(8)],
            true,
        )
    );
}

#[test]
#[allow(clippy::too_many_lines)]
fn paired_sample_one_giver_rejects_a_rank_three_false_prompt() {
    let mut state = paired_sample_one_state();
    for action in [
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Rank(Rank::Two),
        },
        Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Rank(Rank::One),
        },
        Action::Play(CardId::new(14)),
        Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Suit(Suit::Red),
        },
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Suit(Suit::Yellow),
        },
        Action::Play(CardId::new(13)),
        Action::Play(CardId::new(1)),
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Rank(Rank::One),
        },
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Suit(Suit::Yellow),
        },
        Action::Play(CardId::new(4)),
        Action::Play(CardId::new(6)),
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Suit(Suit::Yellow),
        },
        Action::Play(CardId::new(17)),
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Rank(Rank::Five),
        },
        Action::Discard(CardId::new(11)),
        Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Rank(Rank::One),
        },
        Action::Discard(CardId::new(7)),
        Action::Play(CardId::new(12)),
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Rank(Rank::Three),
        },
        Action::Play(CardId::new(5)),
        Action::Discard(CardId::new(10)),
        Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Suit(Suit::Purple),
        },
        Action::Play(CardId::new(9)),
        Action::Discard(CardId::new(21)),
        Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Suit(Suit::Red),
        },
        Action::Discard(CardId::new(8)),
        Action::Play(CardId::new(23)),
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Suit(Suit::Purple),
        },
        Action::Play(CardId::new(22)),
        Action::Discard(CardId::new(27)),
        Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Rank(Rank::Three),
        },
        Action::Discard(CardId::new(19)),
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Rank(Rank::Two),
        },
        Action::Discard(CardId::new(2)),
        Action::Play(CardId::new(32)),
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Rank(Rank::Five),
        },
        Action::Discard(CardId::new(18)),
        Action::Discard(CardId::new(26)),
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Suit(Suit::Red),
        },
    ] {
        state.apply(action).unwrap();
    }
    let deductions = LogicalDeductions::new(state.view_for(PlayerId::new(0)).unwrap()).unwrap();
    let bad_clue = Action::Clue {
        target: PlayerId::new(2),
        clue: Clue::Rank(Rank::Three),
    };
    let candidates = h_group_clue_candidates(&deductions, HGroupProfile::Max);

    assert!(
        !candidates
            .iter()
            .any(|candidate| candidate.action == bad_clue),
        "giver knowingly created a false Purple-2 Prompt through Purple 4: {candidates:#?}"
    );
}

#[test]
fn paired_sample_one_no_information_purple_reclue_does_not_play_purple_four() {
    let mut state = paired_sample_one_state();
    for action in [
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Rank(Rank::Two),
        },
        Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Rank(Rank::One),
        },
        Action::Play(CardId::new(14)),
        Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Suit(Suit::Red),
        },
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Suit(Suit::Yellow),
        },
        Action::Play(CardId::new(13)),
        Action::Play(CardId::new(1)),
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Rank(Rank::One),
        },
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Suit(Suit::Yellow),
        },
        Action::Play(CardId::new(4)),
        Action::Play(CardId::new(6)),
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Suit(Suit::Yellow),
        },
        Action::Play(CardId::new(17)),
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Rank(Rank::Five),
        },
        Action::Discard(CardId::new(11)),
        Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Rank(Rank::One),
        },
        Action::Discard(CardId::new(7)),
        Action::Play(CardId::new(12)),
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Rank(Rank::Three),
        },
        Action::Play(CardId::new(5)),
        Action::Discard(CardId::new(10)),
        Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Suit(Suit::Purple),
        },
        Action::Play(CardId::new(9)),
        Action::Discard(CardId::new(21)),
        Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Suit(Suit::Red),
        },
        Action::Discard(CardId::new(8)),
        Action::Play(CardId::new(23)),
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Suit(Suit::Purple),
        },
        Action::Play(CardId::new(22)),
        Action::Discard(CardId::new(27)),
        Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Suit(Suit::Purple),
        },
        Action::Discard(CardId::new(19)),
    ] {
        state.apply(action).unwrap();
    }
    let deductions = LogicalDeductions::new(state.view_for(PlayerId::new(2)).unwrap()).unwrap();
    let inferred = infer_h_group(&deductions, HGroupProfile::Max);

    assert_ne!(
        select_h_group_action(&deductions, HGroupProfile::Max),
        Some(Action::Play(CardId::new(25))),
        "no-information Purple reclue falsely made Purple 4 playable: {inferred:#?}"
    );
}

#[test]
fn paired_sample_one_giver_rejects_green_clue_that_blind_plays_purple_four() {
    let mut state = paired_sample_one_state();
    for action in [
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Rank(Rank::Two),
        },
        Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Rank(Rank::One),
        },
        Action::Play(CardId::new(14)),
        Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Suit(Suit::Red),
        },
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Suit(Suit::Yellow),
        },
        Action::Play(CardId::new(13)),
        Action::Play(CardId::new(1)),
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Rank(Rank::One),
        },
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Suit(Suit::Yellow),
        },
        Action::Play(CardId::new(4)),
        Action::Play(CardId::new(6)),
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Suit(Suit::Yellow),
        },
        Action::Play(CardId::new(17)),
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Rank(Rank::Five),
        },
        Action::Discard(CardId::new(11)),
        Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Rank(Rank::One),
        },
        Action::Discard(CardId::new(7)),
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Rank(Rank::One),
        },
        Action::Discard(CardId::new(2)),
        Action::Play(CardId::new(8)),
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Suit(Suit::Red),
        },
        Action::Discard(CardId::new(18)),
        Action::Play(CardId::new(5)),
    ] {
        state.apply(action).unwrap();
    }
    let deductions = LogicalDeductions::new(state.view_for(PlayerId::new(2)).unwrap()).unwrap();
    let bad_clue = Action::Clue {
        target: PlayerId::new(1),
        clue: Clue::Suit(Suit::Green),
    };
    let candidates = h_group_clue_candidates(&deductions, HGroupProfile::Max);

    assert!(
        !candidates
            .iter()
            .any(|candidate| candidate.action == bad_clue),
        "giver knowingly forced a fresh Purple 4: {candidates:#?}"
    );
}

#[test]
fn paired_sample_one_giver_rejects_green_five_without_a_green_three() {
    let mut state = paired_sample_one_state();
    for action in [
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Rank(Rank::Two),
        },
        Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Rank(Rank::One),
        },
        Action::Play(CardId::new(14)),
        Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Suit(Suit::Red),
        },
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Suit(Suit::Yellow),
        },
        Action::Play(CardId::new(13)),
        Action::Play(CardId::new(1)),
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Rank(Rank::One),
        },
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Suit(Suit::Yellow),
        },
        Action::Play(CardId::new(4)),
        Action::Play(CardId::new(6)),
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Suit(Suit::Yellow),
        },
        Action::Play(CardId::new(17)),
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Rank(Rank::Five),
        },
        Action::Discard(CardId::new(11)),
        Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Rank(Rank::One),
        },
        Action::Discard(CardId::new(7)),
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Rank(Rank::One),
        },
        Action::Discard(CardId::new(2)),
        Action::Play(CardId::new(8)),
    ] {
        state.apply(action).unwrap();
    }
    let deductions = LogicalDeductions::new(state.view_for(PlayerId::new(2)).unwrap()).unwrap();
    let bad_clue = Action::Clue {
        target: PlayerId::new(0),
        clue: Clue::Suit(Suit::Green),
    };
    let candidates = h_group_clue_candidates(&deductions, HGroupProfile::Max);

    assert!(
        !candidates
            .iter()
            .any(|candidate| candidate.action == bad_clue),
        "giver knowingly created a Green-3 layered finesse without Green 3: {candidates:#?}"
    );
}

#[test]
fn charm_does_not_retouch_an_already_chop_moved_four() {
    let mut state = paired_sample_two_state();
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
            clue: Clue::Suit(Suit::Purple),
        },
        Action::Play(CardId::new(0)),
        Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Suit(Suit::Red),
        },
        Action::Play(CardId::new(14)),
        Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Rank(Rank::One),
        },
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Rank(Rank::Five),
        },
        Action::Play(CardId::new(13)),
        Action::Play(CardId::new(4)),
        Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Suit(Suit::Purple),
        },
        Action::Play(CardId::new(10)),
    ] {
        state.apply(action).unwrap();
    }
    let player = state.current_player();
    let deductions = LogicalDeductions::new(state.view_for(player).unwrap()).unwrap();
    let inferred = infer_h_group(&deductions, HGroupProfile::Max);

    assert_ne!(
        select_h_group_action(&deductions, HGroupProfile::Max),
        Some(Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Rank(Rank::Four),
        }),
        "player: {player:?}; candidates: {:#?}; clues: {:#?}; inference: {inferred:#?}",
        ordered_h_group_actions(&deductions, HGroupProfile::Max),
        h_group_clue_candidates(&deductions, HGroupProfile::Max)
    );
}

#[test]
#[allow(clippy::too_many_lines)]
fn paired_sample_six_plays_yellow_two_connection_before_yellow_three() {
    let mut state = paired_sample_six_state();
    for (index, action) in [
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Rank(Rank::Two),
        },
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Suit(Suit::Yellow),
        },
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Rank(Rank::Two),
        },
        Action::Play(CardId::new(1)),
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Suit(Suit::Blue),
        },
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Suit(Suit::Purple),
        },
        Action::Play(CardId::new(15)),
        Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Suit(Suit::Blue),
        },
        Action::Play(CardId::new(13)),
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Rank(Rank::One),
        },
        Action::Play(CardId::new(8)),
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Suit(Suit::Blue),
        },
        Action::Play(CardId::new(4)),
        Action::Play(CardId::new(7)),
        Action::Discard(CardId::new(10)),
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Suit(Suit::Red),
        },
        Action::Play(CardId::new(5)),
        Action::Discard(CardId::new(11)),
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Suit(Suit::Purple),
        },
        Action::Play(CardId::new(9)),
        Action::Discard(CardId::new(12)),
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Rank(Rank::Two),
        },
        Action::Discard(CardId::new(6)),
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Rank(Rank::Four),
        },
        Action::Discard(CardId::new(2)),
        Action::Play(CardId::new(24)),
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Suit(Suit::Blue),
        },
        Action::Play(CardId::new(27)),
        Action::Discard(CardId::new(26)),
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Suit(Suit::Yellow),
        },
        Action::Discard(CardId::new(3)),
        Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Suit(Suit::Green),
        },
        Action::Play(CardId::new(25)),
        Action::Play(CardId::new(0)),
        Action::Discard(CardId::new(30)),
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Rank(Rank::Two),
        },
        Action::Discard(CardId::new(16)),
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Rank(Rank::Five),
        },
        Action::Play(CardId::new(21)),
        Action::Discard(CardId::new(29)),
        Action::Play(CardId::new(22)),
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Suit(Suit::Blue),
        },
        Action::Play(CardId::new(37)),
        Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Rank(Rank::Five),
        },
        Action::Discard(CardId::new(23)),
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Suit(Suit::Green),
        },
        Action::Play(CardId::new(34)),
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Suit(Suit::Yellow),
        },
        Action::Discard(CardId::new(33)),
        Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Suit(Suit::Red),
        },
        Action::Discard(CardId::new(32)),
        Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Suit(Suit::Yellow),
        },
    ]
    .into_iter()
    .enumerate()
    {
        state
            .apply(action)
            .unwrap_or_else(|error| panic!("action {index} {action:?}: {error:?}"));
    }
    let player = state.current_player();
    let deductions = LogicalDeductions::new(state.view_for(player).unwrap()).unwrap();
    let inferred = infer_h_group(&deductions, HGroupProfile::Max);

    assert_ne!(
        select_h_group_action(&deductions, HGroupProfile::Max),
        Some(Action::Discard(CardId::new(38))),
        "player: {player:?}; candidates: {:#?}; inference: {inferred:#?}",
        ordered_h_group_actions(&deductions, HGroupProfile::Max)
    );
}

#[test]
fn delayed_purple_clue_is_rejected_without_a_demonstrable_connector() {
    let mut state = paired_sample_seven_state();
    for action in [
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Rank(Rank::Two),
        },
        Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Rank(Rank::One),
        },
        Action::Play(CardId::new(14)),
        Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Suit(Suit::Red),
        },
    ] {
        state.apply(action).unwrap();
    }
    let player = state.current_player();
    let deductions = LogicalDeductions::new(state.view_for(player).unwrap()).unwrap();
    let inferred = infer_h_group(&deductions, HGroupProfile::Max);

    assert_ne!(
        select_h_group_action(&deductions, HGroupProfile::Max),
        Some(Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Suit(Suit::Purple),
        }),
        "player: {player:?}; candidates: {:#?}; inference: {inferred:#?}",
        ordered_h_group_actions(&deductions, HGroupProfile::Max)
    );
}

#[test]
fn blue_three_clue_is_rejected_when_it_would_prompt_an_older_blue_five() {
    let mut state = paired_sample_five_state();
    for (index, action) in [
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
            clue: Clue::Suit(Suit::Green),
        },
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Rank(Rank::Five),
        },
        Action::Play(CardId::new(0)),
        Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Suit(Suit::Red),
        },
        Action::Play(CardId::new(14)),
        Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Suit(Suit::Blue),
        },
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Suit(Suit::Blue),
        },
        Action::Play(CardId::new(13)),
        Action::Play(CardId::new(15)),
        Action::Discard(CardId::new(6)),
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Suit(Suit::Yellow),
        },
        Action::Play(CardId::new(1)),
        Action::Discard(CardId::new(7)),
    ]
    .into_iter()
    .enumerate()
    {
        state
            .apply(action)
            .unwrap_or_else(|error| panic!("action {index} {action:?}: {error:?}"));
    }
    let deductions = LogicalDeductions::new(state.view_for(PlayerId::new(2)).unwrap()).unwrap();
    let inferred = infer_h_group(&deductions, HGroupProfile::Max);

    let bad_clue = Action::Clue {
        target: PlayerId::new(0),
        clue: Clue::Suit(Suit::Blue),
    };
    assert!(
        !ordered_h_group_actions(&deductions, HGroupProfile::Max).contains(&bad_clue),
        "selected: {:?}; inference: {inferred:#?}; clues: {:#?}",
        select_h_group_action(&deductions, HGroupProfile::Max),
        h_group_clue_candidates(&deductions, HGroupProfile::Max),
    );
    assert!(select_h_group_action(&deductions, HGroupProfile::Max).is_some());
}

#[test]
fn five_on_chop_is_a_save_and_is_not_played() {
    let mut state = state_with_prefix(
        2,
        &[
            Card::new(Suit::Red, Rank::One),
            Card::new(Suit::Blue, Rank::One),
            Card::new(Suit::Green, Rank::Three),
            Card::new(Suit::Yellow, Rank::Four),
            Card::new(Suit::Purple, Rank::One),
            Card::new(Suit::Red, Rank::Five),
        ],
    );
    state
        .apply(Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Rank(Rank::Five),
        })
        .unwrap();
    let deductions = LogicalDeductions::new(state.view_for(PlayerId::new(1)).unwrap()).unwrap();
    let inferred = infer_h_group(
        &deductions,
        HGroupProfile::Level(crate::HGroupLevel::Level1),
    );
    assert_eq!(
        inferred.clues[0].kind,
        HGroupClueKind::Save(HGroupSaveKind::Five)
    );
    assert_eq!(inferred.saved, vec![CardId::new(5)]);
    assert!(inferred.playable_now.is_empty());
}

#[test]
fn delayed_play_clue_finesses_newest_unclued_card() {
    let mut state = state_with_prefix(
        3,
        &[
            Card::new(Suit::Blue, Rank::Four),
            Card::new(Suit::Green, Rank::Four),
            Card::new(Suit::Yellow, Rank::Three),
            Card::new(Suit::Purple, Rank::Four),
            Card::new(Suit::Blue, Rank::Five),
            Card::new(Suit::Red, Rank::Three),
            Card::new(Suit::Blue, Rank::Two),
            Card::new(Suit::Green, Rank::Four),
            Card::new(Suit::Yellow, Rank::Five),
            Card::new(Suit::Red, Rank::One),
            Card::new(Suit::Red, Rank::Two),
        ],
    );
    state
        .apply(Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Suit(Suit::Red),
        })
        .unwrap();
    let deductions = LogicalDeductions::new(state.view_for(PlayerId::new(1)).unwrap()).unwrap();
    let convention =
        crate::SupportedConvention::HGroup(HGroupProfile::Level(crate::HGroupLevel::Level1));
    let inferred = infer_h_group(
        &deductions,
        HGroupProfile::Level(crate::HGroupLevel::Level1),
    );
    assert_eq!(
        inferred.connection,
        Some(HGroupConnection {
            card: CardId::new(9),
            identity: Card::new(Suit::Red, Rank::One),
            kind: HGroupConnectionKind::Finesse,
            focus: CardId::new(10),
        })
    );

    // A reconstructed or otherwise arbitrary view can invalidate a
    // convention promise while retaining its public clue history. Such a
    // stale promise must never escape as an illegal planning candidate.
    let mut stale_view = state.view_for(PlayerId::new(1)).unwrap();
    stale_view.hands[1]
        .iter_mut()
        .find(|card| card.id == CardId::new(9))
        .unwrap()
        .id = CardId::new(15);
    let stale_deductions = LogicalDeductions::new(stale_view).unwrap();
    let stale_legal = stale_deductions.view().legal_actions();
    let stale_candidates = ordered_h_group_actions(
        &stale_deductions,
        HGroupProfile::Level(crate::HGroupLevel::Level1),
    );
    assert!(!stale_candidates.contains(&Action::Play(CardId::new(9))));
    assert!(
        stale_candidates
            .iter()
            .all(|action| stale_legal.contains(action))
    );

    let finesse = convention.analyze(&deductions).preferred_action.unwrap();
    assert_eq!(finesse, Action::Play(CardId::new(9)));
    state.apply(finesse).unwrap();

    let deductions = LogicalDeductions::new(state.view_for(PlayerId::new(2)).unwrap()).unwrap();
    let inferred = infer_h_group(
        &deductions,
        HGroupProfile::Level(crate::HGroupLevel::Level1),
    );
    assert_eq!(
        convention.analyze(&deductions).preferred_action.unwrap(),
        Action::Play(CardId::new(10)),
        "{inferred:#?}"
    );
}

#[test]
fn prompt_takes_precedence_over_finesse_and_policy_plays_it() {
    let mut state = state_with_prefix(
        3,
        &[
            Card::new(Suit::Blue, Rank::Three),
            Card::new(Suit::Green, Rank::Four),
            Card::new(Suit::Yellow, Rank::Three),
            Card::new(Suit::Yellow, Rank::Five),
            Card::new(Suit::Purple, Rank::Two),
            Card::new(Suit::Red, Rank::Four),
            Card::new(Suit::Red, Rank::Four),
            Card::new(Suit::Red, Rank::One),
            Card::new(Suit::Yellow, Rank::Four),
            Card::new(Suit::Blue, Rank::Four),
            Card::new(Suit::Red, Rank::Two),
            Card::new(Suit::Blue, Rank::Three),
            Card::new(Suit::Green, Rank::Two),
            Card::new(Suit::Yellow, Rank::Three),
            Card::new(Suit::Purple, Rank::Five),
        ],
    );
    state
        .apply(Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Rank(Rank::Five),
        })
        .unwrap();
    state.apply(Action::Discard(CardId::new(5))).unwrap();
    state
        .apply(Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Suit(Suit::Red),
        })
        .unwrap();
    state
        .apply(Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Suit(Suit::Red),
        })
        .unwrap();

    let deductions = LogicalDeductions::new(state.view_for(PlayerId::new(1)).unwrap()).unwrap();
    let convention =
        crate::SupportedConvention::HGroup(HGroupProfile::Level(crate::HGroupLevel::Level1));
    let inferred = infer_h_group(
        &deductions,
        HGroupProfile::Level(crate::HGroupLevel::Level1),
    );
    assert_eq!(
        inferred.connection,
        Some(HGroupConnection {
            card: CardId::new(15),
            identity: Card::new(Suit::Red, Rank::One),
            kind: HGroupConnectionKind::Prompt,
            focus: CardId::new(10),
        }),
        "{inferred:#?}"
    );
    assert_eq!(
        convention.analyze(&deductions).preferred_action.unwrap(),
        Action::Play(CardId::new(15))
    );
}

#[test]
fn policy_gives_and_recipient_plays_a_direct_level_one_play_clue() {
    let mut state = state_with_prefix(
        2,
        &[
            Card::new(Suit::Blue, Rank::Three),
            Card::new(Suit::Green, Rank::Three),
            Card::new(Suit::Yellow, Rank::Four),
            Card::new(Suit::Purple, Rank::Five),
            Card::new(Suit::Blue, Rank::Three),
            Card::new(Suit::Red, Rank::One),
            Card::new(Suit::Blue, Rank::Four),
            Card::new(Suit::Green, Rank::Five),
            Card::new(Suit::Yellow, Rank::Two),
            Card::new(Suit::Purple, Rank::Three),
        ],
    );
    let convention =
        crate::SupportedConvention::HGroup(HGroupProfile::Level(crate::HGroupLevel::Level1));
    let deductions = LogicalDeductions::new(state.view_for(PlayerId::new(0)).unwrap()).unwrap();
    let clue = convention.analyze(&deductions).preferred_action.unwrap();
    assert_eq!(
        clue,
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Suit(Suit::Red),
        }
    );
    state.apply(clue).unwrap();

    let deductions = LogicalDeductions::new(state.view_for(PlayerId::new(1)).unwrap()).unwrap();
    assert_eq!(
        convention.analyze(&deductions).preferred_action.unwrap(),
        Action::Play(CardId::new(5))
    );
}

#[test]
fn save_principle_prefers_a_five_save_over_a_play_clue() {
    let state = state_with_prefix(
        3,
        &[
            Card::new(Suit::Blue, Rank::Two),
            Card::new(Suit::Green, Rank::Three),
            Card::new(Suit::Yellow, Rank::Four),
            Card::new(Suit::Purple, Rank::Two),
            Card::new(Suit::Blue, Rank::Three),
            Card::new(Suit::Red, Rank::Five),
            Card::new(Suit::Blue, Rank::Four),
            Card::new(Suit::Green, Rank::Two),
            Card::new(Suit::Yellow, Rank::Three),
            Card::new(Suit::Purple, Rank::Four),
            Card::new(Suit::Red, Rank::One),
            Card::new(Suit::Blue, Rank::Five),
            Card::new(Suit::Green, Rank::Four),
            Card::new(Suit::Yellow, Rank::Two),
            Card::new(Suit::Purple, Rank::Three),
        ],
    );
    let convention =
        crate::SupportedConvention::HGroup(HGroupProfile::Level(crate::HGroupLevel::Level1));
    let deductions = LogicalDeductions::new(state.view_for(PlayerId::new(0)).unwrap()).unwrap();
    assert_eq!(
        convention.analyze(&deductions).preferred_action.unwrap(),
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Rank(Rank::Five),
        }
    );
}

#[test]
#[allow(clippy::too_many_lines)]
fn chop_clue_note_keeps_both_play_and_critical_save_possibilities() {
    let blue = Clue::Suit(Suit::Blue);
    let blue_one = Card::new(Suit::Blue, Rank::One);
    let blue_two = Card::new(Suit::Blue, Rank::Two);
    let blue_four = Card::new(Suit::Blue, Rank::Four);
    let view = PlayerView {
        observer: PlayerId::new(1),
        current_player: PlayerId::new(1),
        turn: 7,
        hands: vec![
            vec![
                observed(2, Some(Card::new(Suit::Green, Rank::One)), &[]),
                observed(3, Some(Card::new(Suit::Yellow, Rank::One)), &[]),
                observed(4, Some(Card::new(Suit::Purple, Rank::One)), &[]),
                observed(15, Some(Card::new(Suit::Red, Rank::Two)), &[]),
                observed(17, Some(Card::new(Suit::Red, Rank::One)), &[]),
            ],
            vec![
                observed(5, None, &[blue]),
                observed(6, None, &[]),
                observed(7, None, &[]),
                observed(8, None, &[]),
                observed(9, None, &[]),
            ],
            vec![
                observed(11, Some(Card::new(Suit::Green, Rank::Three)), &[]),
                observed(12, Some(Card::new(Suit::Yellow, Rank::Three)), &[]),
                observed(13, Some(Card::new(Suit::Purple, Rank::Three)), &[]),
                observed(14, Some(Card::new(Suit::Red, Rank::Four)), &[]),
                observed(16, Some(Card::new(Suit::Blue, Rank::Five)), &[]),
            ],
        ],
        deck_size: 32,
        play_stacks: [
            Vec::new(),
            Vec::new(),
            Vec::new(),
            vec![(CardId::new(0), blue_one), (CardId::new(10), blue_two)],
            Vec::new(),
        ],
        discard_pile: vec![(CardId::new(1), blue_four)],
        clue_tokens: 7,
        strikes: 0,
        final_turns_remaining: None,
        status: GameStatus::InProgress,
        history: vec![
            ObservedHistoryEntry {
                turn: 0,
                event: ObservedEvent::Played {
                    player: PlayerId::new(0),
                    card: CardId::new(0),
                    identity: blue_one,
                    successful: true,
                },
            },
            ObservedHistoryEntry {
                turn: 0,
                event: ObservedEvent::Drew {
                    player: PlayerId::new(0),
                    card: CardId::new(15),
                    identity: Some(Card::new(Suit::Red, Rank::Two)),
                },
            },
            ObservedHistoryEntry {
                turn: 2,
                event: ObservedEvent::Played {
                    player: PlayerId::new(2),
                    card: CardId::new(10),
                    identity: blue_two,
                    successful: true,
                },
            },
            ObservedHistoryEntry {
                turn: 2,
                event: ObservedEvent::Drew {
                    player: PlayerId::new(2),
                    card: CardId::new(16),
                    identity: Some(Card::new(Suit::Blue, Rank::Five)),
                },
            },
            ObservedHistoryEntry {
                turn: 3,
                event: ObservedEvent::Discarded {
                    player: PlayerId::new(0),
                    card: CardId::new(1),
                    identity: blue_four,
                },
            },
            ObservedHistoryEntry {
                turn: 3,
                event: ObservedEvent::Drew {
                    player: PlayerId::new(0),
                    card: CardId::new(17),
                    identity: Some(Card::new(Suit::Red, Rank::One)),
                },
            },
            ObservedHistoryEntry {
                turn: 6,
                event: ObservedEvent::Clued {
                    giver: PlayerId::new(0),
                    target: PlayerId::new(1),
                    clue: blue,
                    touched: vec![CardId::new(5)],
                    untouched: vec![
                        CardId::new(6),
                        CardId::new(7),
                        CardId::new(8),
                        CardId::new(9),
                    ],
                },
            },
        ],
    };
    let deductions = LogicalDeductions::new(view).unwrap();
    let inferred = infer_h_group(
        &deductions,
        HGroupProfile::Level(crate::HGroupLevel::Level1),
    );
    let focus = inferred
        .cards
        .iter()
        .find(|card| card.card == CardId::new(5))
        .unwrap();
    assert_eq!(
        focus.identities,
        IdentitySet::singleton(Card::new(Suit::Blue, Rank::Three))
            .union(IdentitySet::singleton(blue_four))
    );
    assert!(focus.saved);
    assert!(!inferred.playable_now.contains(&CardId::new(5)));
}

#[test]
fn double_chop_twos_are_an_exception_to_the_visible_rule() {
    let red_two = Card::new(Suit::Red, Rank::Two);
    let state = state_with_prefix(
        3,
        &[
            Card::new(Suit::Blue, Rank::Three),
            Card::new(Suit::Green, Rank::Four),
            Card::new(Suit::Yellow, Rank::Five),
            Card::new(Suit::Purple, Rank::Three),
            Card::new(Suit::Blue, Rank::Four),
            red_two,
            Card::new(Suit::Green, Rank::Three),
            Card::new(Suit::Yellow, Rank::Four),
            Card::new(Suit::Purple, Rank::Five),
            Card::new(Suit::Blue, Rank::Five),
            red_two,
            Card::new(Suit::Green, Rank::Two),
            Card::new(Suit::Yellow, Rank::Three),
            Card::new(Suit::Purple, Rank::Four),
            Card::new(Suit::Blue, Rank::Two),
        ],
    );
    let deductions = LogicalDeductions::new(state.view_for(PlayerId::new(0)).unwrap()).unwrap();
    let candidates =
        h_group_clue_candidates(&deductions, HGroupProfile::Level(HGroupLevel::Level1));
    for target in [PlayerId::new(1), PlayerId::new(2)] {
        assert!(candidates.iter().any(|candidate| {
            candidate.action
                == Action::Clue {
                    target,
                    clue: Clue::Rank(Rank::Two),
                }
        }));
    }
}

#[test]
fn finesse_card_is_invisibly_clued_for_later_focus() {
    let mut state = state_with_prefix(
        3,
        &[
            Card::new(Suit::Blue, Rank::Three),
            Card::new(Suit::Green, Rank::Four),
            Card::new(Suit::Yellow, Rank::Five),
            Card::new(Suit::Purple, Rank::Three),
            Card::new(Suit::Blue, Rank::Four),
            Card::new(Suit::Green, Rank::Three),
            Card::new(Suit::Yellow, Rank::Four),
            Card::new(Suit::Purple, Rank::Five),
            Card::new(Suit::Blue, Rank::One),
            Card::new(Suit::Red, Rank::One),
            Card::new(Suit::Green, Rank::Five),
            Card::new(Suit::Blue, Rank::Two),
            Card::new(Suit::Yellow, Rank::Three),
            Card::new(Suit::Purple, Rank::Four),
            Card::new(Suit::Red, Rank::Two),
        ],
    );
    state
        .apply(Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Suit(Suit::Red),
        })
        .unwrap();
    state
        .apply(Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Rank(Rank::Five),
        })
        .unwrap();
    state
        .apply(Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Suit(Suit::Blue),
        })
        .unwrap();
    state
        .apply(Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Rank(Rank::One),
        })
        .unwrap();

    let deductions = LogicalDeductions::new(state.view_for(PlayerId::new(1)).unwrap()).unwrap();
    let inferred = infer_h_group(
        &deductions,
        HGroupProfile::Level(crate::HGroupLevel::Level1),
    );
    assert!(inferred.invisibly_clued.contains(&CardId::new(9)));
    assert_eq!(inferred.clues.last().unwrap().focus, CardId::new(8));
}

#[test]
fn early_game_ends_only_when_a_chop_is_discarded() {
    let mut state = state_with_prefix(
        2,
        &[
            Card::new(Suit::Blue, Rank::Two),
            Card::new(Suit::Green, Rank::Three),
            Card::new(Suit::Yellow, Rank::Four),
            Card::new(Suit::Purple, Rank::Five),
            Card::new(Suit::Blue, Rank::Three),
            Card::new(Suit::Red, Rank::One),
            Card::new(Suit::Blue, Rank::Four),
            Card::new(Suit::Green, Rank::Five),
            Card::new(Suit::Yellow, Rank::Two),
            Card::new(Suit::Purple, Rank::Three),
        ],
    );
    let initial = LogicalDeductions::new(state.view_for(PlayerId::new(0)).unwrap()).unwrap();
    assert!(infer_h_group(&initial, HGroupProfile::Level(crate::HGroupLevel::Level1)).early_game);
    state
        .apply(Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Suit(Suit::Red),
        })
        .unwrap();
    state.apply(Action::Discard(CardId::new(6))).unwrap();
    let deductions = LogicalDeductions::new(state.view_for(PlayerId::new(0)).unwrap()).unwrap();
    assert!(
        !infer_h_group(
            &deductions,
            HGroupProfile::Level(crate::HGroupLevel::Level1)
        )
        .early_game
    );
}

#[test]
#[allow(clippy::too_many_lines)]
fn prompt_promise_continues_left_to_right_after_a_wrong_card_plays() {
    let mut state = state_with_prefix(
        3,
        &[
            Card::new(Suit::Blue, Rank::Two),
            Card::new(Suit::Green, Rank::Three),
            Card::new(Suit::Yellow, Rank::Four),
            Card::new(Suit::Purple, Rank::Five),
            Card::new(Suit::Blue, Rank::Three),
            Card::new(Suit::Green, Rank::Four),
            Card::new(Suit::Yellow, Rank::Three),
            Card::new(Suit::Red, Rank::One),
            Card::new(Suit::Blue, Rank::One),
            Card::new(Suit::Purple, Rank::One),
            Card::new(Suit::Red, Rank::Two),
            Card::new(Suit::Green, Rank::Four),
            Card::new(Suit::Yellow, Rank::Five),
            Card::new(Suit::Purple, Rank::Two),
            Card::new(Suit::Blue, Rank::Four),
        ],
    );
    state
        .apply(Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Rank(Rank::One),
        })
        .unwrap();
    state
        .apply(Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Suit(Suit::Green),
        })
        .unwrap();
    state
        .apply(Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Suit(Suit::Blue),
        })
        .unwrap();
    state
        .apply(Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Suit(Suit::Red),
        })
        .unwrap();

    let information_set =
        crate::InformationSet::new(&state.view_for(PlayerId::new(1)).unwrap()).unwrap();
    let inferred = infer_h_group(
        information_set.deductions(),
        HGroupProfile::Level(crate::HGroupLevel::Level1),
    );
    assert_eq!(inferred.connection.unwrap().card, CardId::new(8));
    assert_eq!(
        inferred.connection_promises,
        vec![HGroupConnectionPromise {
            cards: vec![CardId::new(8), CardId::new(7)],
            identity: Card::new(Suit::Red, Rank::One),
        }]
    );
    let convention =
        crate::SupportedConvention::HGroup(HGroupProfile::Level(crate::HGroupLevel::Level1));
    let expected = Card::new(Suit::Red, Rank::One);
    let belief = convention
        .analyze(information_set.deductions())
        .belief_constraints;
    assert_eq!(belief.branches.len(), 2);
    assert!(
        belief
            .branches
            .iter()
            .any(|branch| { branch.contains(&(CardId::new(8), IdentitySet::singleton(expected))) })
    );
    assert!(belief.branches.iter().any(|branch| {
        branch.contains(&(CardId::new(7), IdentitySet::singleton(expected)))
            && branch.iter().any(|(card, identities)| {
                *card == CardId::new(8)
                    && identities.iter().all(|identity| identity.rank == Rank::One)
                    && !identities.contains(expected)
            })
    }));
    state.apply(Action::Play(CardId::new(8))).unwrap();
    state
        .apply(Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Suit(Suit::Green),
        })
        .unwrap();
    state
        .apply(Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Suit(Suit::Blue),
        })
        .unwrap();

    let deductions = LogicalDeductions::new(state.view_for(PlayerId::new(1)).unwrap()).unwrap();
    let inferred = infer_h_group(
        &deductions,
        HGroupProfile::Level(crate::HGroupLevel::Level1),
    );
    assert_eq!(
        inferred.connection,
        Some(HGroupConnection {
            card: CardId::new(7),
            identity: Card::new(Suit::Red, Rank::One),
            kind: HGroupConnectionKind::Prompt,
            focus: CardId::new(10),
        })
    );
}

#[test]
fn delayed_play_can_use_one_finesse_then_an_accounted_chain() {
    let mut state = state_with_prefix(
        3,
        &[
            Card::new(Suit::Blue, Rank::One),
            Card::new(Suit::Green, Rank::Three),
            Card::new(Suit::Yellow, Rank::Four),
            Card::new(Suit::Purple, Rank::Five),
            Card::new(Suit::Blue, Rank::Three),
            Card::new(Suit::Green, Rank::Four),
            Card::new(Suit::Yellow, Rank::Three),
            Card::new(Suit::Purple, Rank::Four),
            Card::new(Suit::Blue, Rank::Five),
            Card::new(Suit::Red, Rank::One),
            Card::new(Suit::Red, Rank::Two),
            Card::new(Suit::Red, Rank::Three),
            Card::new(Suit::Red, Rank::Four),
            Card::new(Suit::Yellow, Rank::Five),
            Card::new(Suit::Purple, Rank::Two),
        ],
    );
    state
        .apply(Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Rank(Rank::Two),
        })
        .unwrap();
    state
        .apply(Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Rank(Rank::Three),
        })
        .unwrap();
    // Advance to player 0 without giving a clue that itself creates a
    // same-hand Self-Prompt and masks the delayed red chain under test.
    state.apply(Action::Discard(CardId::new(14))).unwrap();

    let deductions = LogicalDeductions::new(state.view_for(PlayerId::new(0)).unwrap()).unwrap();
    let convention =
        crate::SupportedConvention::HGroup(HGroupProfile::Level(crate::HGroupLevel::Level1));
    let candidates = convention
        .analyze(&deductions)
        .actions
        .into_iter()
        .map(|candidate| candidate.action)
        .collect::<Vec<_>>();
    assert!(
        candidates.contains(&Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Suit(Suit::Red),
        }),
        "candidates: {candidates:?}; clues: {:?}",
        h_group_clue_candidates(&deductions, HGroupProfile::Level(HGroupLevel::Level1))
    );
}

#[test]
fn good_touch_notes_and_root_belief_exclude_a_duplicate_focus() {
    let mut state = state_with_prefix(
        2,
        &[
            Card::new(Suit::Blue, Rank::Two),
            Card::new(Suit::Green, Rank::Three),
            Card::new(Suit::Yellow, Rank::Four),
            Card::new(Suit::Purple, Rank::Five),
            Card::new(Suit::Blue, Rank::Three),
            Card::new(Suit::Green, Rank::Two),
            Card::new(Suit::Yellow, Rank::Three),
            Card::new(Suit::Purple, Rank::Four),
            Card::new(Suit::Red, Rank::Two),
            Card::new(Suit::Red, Rank::One),
        ],
    );
    state
        .apply(Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Suit(Suit::Red),
        })
        .unwrap();
    let information_set =
        crate::InformationSet::new(&state.view_for(PlayerId::new(1)).unwrap()).unwrap();
    let inferred = infer_h_group(
        information_set.deductions(),
        HGroupProfile::Level(crate::HGroupLevel::Level1),
    );
    let focus = inferred
        .cards
        .iter()
        .find(|card| card.card == CardId::new(9))
        .unwrap();
    let non_focus = inferred
        .cards
        .iter()
        .find(|card| card.card == CardId::new(8))
        .unwrap();
    assert_eq!(
        focus.identities,
        IdentitySet::singleton(Card::new(Suit::Red, Rank::One))
    );
    assert!(
        !non_focus
            .identities
            .contains(Card::new(Suit::Red, Rank::One))
    );

    let convention =
        crate::SupportedConvention::HGroup(HGroupProfile::Level(crate::HGroupLevel::Level1));
    let belief = convention
        .analyze(information_set.deductions())
        .belief_constraints;
    let focus_constraint = belief
        .constraints
        .iter()
        .find(|(card, _)| *card == CardId::new(9))
        .unwrap();
    let non_focus_constraint = belief
        .constraints
        .iter()
        .find(|(card, _)| *card == CardId::new(8))
        .unwrap();
    assert_eq!(
        focus_constraint.1,
        IdentitySet::singleton(Card::new(Suit::Red, Rank::One))
    );
    assert!(
        !non_focus_constraint
            .1
            .contains(Card::new(Suit::Red, Rank::One))
    );
}

#[test]
fn visible_two_off_chop_prevents_a_two_save() {
    let red_two = Card::new(Suit::Red, Rank::Two);
    let mut state = state_with_prefix(
        3,
        &[
            Card::new(Suit::Blue, Rank::Three),
            Card::new(Suit::Green, Rank::Three),
            Card::new(Suit::Yellow, Rank::Four),
            Card::new(Suit::Purple, Rank::Four),
            Card::new(Suit::Blue, Rank::Four),
            red_two,
            Card::new(Suit::Green, Rank::Four),
            Card::new(Suit::Yellow, Rank::Three),
            Card::new(Suit::Purple, Rank::Three),
            Card::new(Suit::Blue, Rank::Five),
            Card::new(Suit::Green, Rank::Five),
            red_two,
            Card::new(Suit::Yellow, Rank::Two),
            Card::new(Suit::Purple, Rank::Two),
            Card::new(Suit::Blue, Rank::Two),
        ],
    );
    let deductions = LogicalDeductions::new(state.view_for(PlayerId::new(0)).unwrap()).unwrap();
    let convention =
        crate::SupportedConvention::HGroup(HGroupProfile::Level(crate::HGroupLevel::Level1));
    assert!(
        !convention
            .analyze(&deductions)
            .actions
            .into_iter()
            .map(|candidate| candidate.action)
            .collect::<Vec<_>>()
            .contains(&Action::Clue {
                target: PlayerId::new(1),
                clue: Clue::Rank(Rank::Two),
            })
    );

    state
        .apply(Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Rank(Rank::Two),
        })
        .unwrap();
    let deductions = LogicalDeductions::new(state.view_for(PlayerId::new(0)).unwrap()).unwrap();
    let inferred = infer_h_group(
        &deductions,
        HGroupProfile::Level(crate::HGroupLevel::Level1),
    );
    assert_eq!(
        inferred.clues.last().unwrap().kind,
        HGroupClueKind::Unrecognized
    );
    assert!(inferred.clues.last().unwrap().save_identities.is_empty());
}

#[test]
fn playable_two_on_chop_is_still_a_two_save() {
    let mut state = state_with_prefix(
        3,
        &[
            Card::new(Suit::Red, Rank::One),
            Card::new(Suit::Blue, Rank::Three),
            Card::new(Suit::Green, Rank::Four),
            Card::new(Suit::Yellow, Rank::Three),
            Card::new(Suit::Purple, Rank::Four),
            Card::new(Suit::Blue, Rank::Four),
            Card::new(Suit::Green, Rank::Three),
            Card::new(Suit::Yellow, Rank::Four),
            Card::new(Suit::Purple, Rank::Three),
            Card::new(Suit::Blue, Rank::Five),
            Card::new(Suit::Red, Rank::Two),
            Card::new(Suit::Green, Rank::Five),
            Card::new(Suit::Yellow, Rank::Two),
            Card::new(Suit::Purple, Rank::Two),
            Card::new(Suit::Blue, Rank::Two),
        ],
    );
    state.apply(Action::Play(CardId::new(0))).unwrap();
    state
        .apply(Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Rank(Rank::Two),
        })
        .unwrap();
    let deductions = LogicalDeductions::new(state.view_for(PlayerId::new(0)).unwrap()).unwrap();
    let inferred = infer_h_group(
        &deductions,
        HGroupProfile::Level(crate::HGroupLevel::Level1),
    );
    assert_eq!(
        inferred.clues.last().unwrap().kind,
        HGroupClueKind::Save(HGroupSaveKind::Two)
    );
    assert!(inferred.clues.last().unwrap().play_identities.is_empty());
}

#[test]
fn convention_known_trash_is_discarded_before_the_chop() {
    let mut state = state_with_prefix(
        2,
        &[
            Card::new(Suit::Red, Rank::One),
            Card::new(Suit::Blue, Rank::Three),
            Card::new(Suit::Green, Rank::Three),
            Card::new(Suit::Yellow, Rank::Four),
            Card::new(Suit::Purple, Rank::Four),
            Card::new(Suit::Red, Rank::One),
            Card::new(Suit::Blue, Rank::Four),
            Card::new(Suit::Green, Rank::Four),
            Card::new(Suit::Yellow, Rank::Three),
            Card::new(Suit::Purple, Rank::Three),
            Card::new(Suit::Blue, Rank::Four),
        ],
    );
    state
        .apply(Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Suit(Suit::Red),
        })
        .unwrap();
    state
        .apply(Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Suit(Suit::Red),
        })
        .unwrap();
    state.apply(Action::Play(CardId::new(0))).unwrap();

    let deductions = LogicalDeductions::new(state.view_for(PlayerId::new(1)).unwrap()).unwrap();
    let convention =
        crate::SupportedConvention::HGroup(HGroupProfile::Level(crate::HGroupLevel::Level1));
    let inferred = infer_h_group(
        &deductions,
        HGroupProfile::Level(crate::HGroupLevel::Level1),
    );
    assert_eq!(inferred.chops[1], Some(CardId::new(6)));
    assert_eq!(
        inferred
            .cards
            .iter()
            .find(|card| card.card == CardId::new(5))
            .unwrap()
            .identities,
        IdentitySet::singleton(Card::new(Suit::Red, Rank::One))
    );
    assert_eq!(
        convention.analyze(&deductions).preferred_action.unwrap(),
        Action::Discard(CardId::new(5))
    );
}

#[test]
fn no_chop_forces_a_tempo_clue_instead_of_a_card_action() {
    let mut state = state_with_prefix(
        2,
        &[
            Card::new(Suit::Red, Rank::Five),
            Card::new(Suit::Blue, Rank::Five),
            Card::new(Suit::Green, Rank::Five),
            Card::new(Suit::Yellow, Rank::Five),
            Card::new(Suit::Purple, Rank::Five),
            Card::new(Suit::Red, Rank::One),
            Card::new(Suit::Blue, Rank::One),
            Card::new(Suit::Green, Rank::One),
            Card::new(Suit::Yellow, Rank::One),
            Card::new(Suit::Purple, Rank::One),
        ],
    );
    state
        .apply(Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Rank(Rank::One),
        })
        .unwrap();
    state
        .apply(Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Rank(Rank::Five),
        })
        .unwrap();
    let deductions = LogicalDeductions::new(state.view_for(PlayerId::new(0)).unwrap()).unwrap();
    let convention =
        crate::SupportedConvention::HGroup(HGroupProfile::Level(crate::HGroupLevel::Level1));
    let inferred = infer_h_group(
        &deductions,
        HGroupProfile::Level(crate::HGroupLevel::Level1),
    );
    assert_eq!(inferred.chops[0], None);
    let candidates = convention
        .analyze(&deductions)
        .actions
        .into_iter()
        .map(|candidate| candidate.action)
        .collect::<Vec<_>>();
    assert!(!candidates.is_empty());
    assert!(
        candidates
            .iter()
            .all(|action| matches!(action, Action::Clue { .. }))
    );
}

#[test]
fn rank_clue_beats_color_only_when_it_gets_more_cards() {
    let state = state_with_prefix(
        2,
        &[
            Card::new(Suit::Blue, Rank::Three),
            Card::new(Suit::Green, Rank::Three),
            Card::new(Suit::Yellow, Rank::Four),
            Card::new(Suit::Purple, Rank::Four),
            Card::new(Suit::Blue, Rank::Four),
            Card::new(Suit::Red, Rank::One),
            Card::new(Suit::Blue, Rank::One),
            Card::new(Suit::Green, Rank::Four),
            Card::new(Suit::Yellow, Rank::Three),
            Card::new(Suit::Purple, Rank::Three),
        ],
    );
    let deductions = LogicalDeductions::new(state.view_for(PlayerId::new(0)).unwrap()).unwrap();
    let convention =
        crate::SupportedConvention::HGroup(HGroupProfile::Level(crate::HGroupLevel::Level1));
    assert_eq!(
        convention.analyze(&deductions).preferred_action.unwrap(),
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Rank(Rank::One),
        }
    );
}

#[test]
fn play_clue_to_the_same_player_preoccupies_before_a_save() {
    let state = state_with_prefix(
        2,
        &[
            Card::new(Suit::Red, Rank::Three),
            Card::new(Suit::Green, Rank::Three),
            Card::new(Suit::Yellow, Rank::Four),
            Card::new(Suit::Purple, Rank::Four),
            Card::new(Suit::Blue, Rank::Three),
            Card::new(Suit::Red, Rank::Five),
            Card::new(Suit::Green, Rank::Four),
            Card::new(Suit::Yellow, Rank::Three),
            Card::new(Suit::Purple, Rank::Three),
            Card::new(Suit::Blue, Rank::One),
        ],
    );
    let deductions = LogicalDeductions::new(state.view_for(PlayerId::new(0)).unwrap()).unwrap();
    let convention =
        crate::SupportedConvention::HGroup(HGroupProfile::Level(crate::HGroupLevel::Level1));
    assert_eq!(
        convention.analyze(&deductions).preferred_action.unwrap(),
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Suit(Suit::Blue),
        }
    );
}

#[test]
fn next_players_unique_playable_chop_preempts_own_play() {
    let mut state = state_with_prefix(
        3,
        &[
            Card::new(Suit::Red, Rank::One),
            Card::new(Suit::Green, Rank::Three),
            Card::new(Suit::Yellow, Rank::Four),
            Card::new(Suit::Purple, Rank::Four),
            Card::new(Suit::Blue, Rank::Three),
            Card::new(Suit::Blue, Rank::One),
            Card::new(Suit::Green, Rank::Four),
            Card::new(Suit::Yellow, Rank::Three),
            Card::new(Suit::Purple, Rank::Three),
            Card::new(Suit::Red, Rank::Three),
            Card::new(Suit::Purple, Rank::Five),
            Card::new(Suit::Green, Rank::Two),
            Card::new(Suit::Yellow, Rank::Two),
            Card::new(Suit::Purple, Rank::Two),
            Card::new(Suit::Blue, Rank::Four),
        ],
    );
    state
        .apply(Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Rank(Rank::Five),
        })
        .unwrap();
    state
        .apply(Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Suit(Suit::Purple),
        })
        .unwrap();
    state
        .apply(Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Suit(Suit::Red),
        })
        .unwrap();

    let deductions = LogicalDeductions::new(state.view_for(PlayerId::new(0)).unwrap()).unwrap();
    let convention =
        crate::SupportedConvention::HGroup(HGroupProfile::Level(crate::HGroupLevel::Level1));
    let candidates = convention
        .analyze(&deductions)
        .actions
        .into_iter()
        .map(|candidate| candidate.action)
        .collect::<Vec<_>>();
    assert_eq!(
        candidates.first(),
        Some(&Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Suit(Suit::Blue),
        })
    );
    assert!(candidates.contains(&Action::Play(CardId::new(0))));
}

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

#[test]
fn every_numbered_and_max_profile_rolls_to_completion() {
    let profiles = H_GROUP_LEVELS.iter().map(|descriptor| descriptor.profile);
    for profile in profiles {
        let mut deck = standard_deck();
        deck.rotate_left(11);
        let state = FullState::new_standard(3, deck).unwrap();
        let convention = crate::SupportedConvention::HGroup(profile);
        let outcome = continuation_to_terminal(state, convention)
            .unwrap_or_else(|error| panic!("{profile} continuation failed: {error}"));
        assert!(outcome.turns() > 0, "{profile}");
        assert!(outcome.turns() < MAX_TEST_CONTINUATION_TURNS, "{profile}");
    }
}

#[test]
#[allow(clippy::too_many_lines)]
fn positional_five_save_is_recognized_by_the_indicated_player() {
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
        Action::Play(CardId::new(14)),
        Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Suit(Suit::Red),
        },
        Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Suit(Suit::Blue),
        },
        Action::Play(CardId::new(13)),
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Rank(Rank::One),
        },
        Action::Play(CardId::new(8)),
        Action::Play(CardId::new(15)),
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Suit(Suit::Red),
        },
        Action::Play(CardId::new(5)),
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Suit(Suit::Purple),
        },
        Action::Play(CardId::new(0)),
        Action::Play(CardId::new(9)),
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Suit(Suit::Green),
        },
        Action::Play(CardId::new(1)),
        Action::Discard(CardId::new(6)),
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Rank(Rank::Two),
        },
        Action::Play(CardId::new(20)),
        Action::Discard(CardId::new(7)),
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Rank(Rank::Five),
        },
        Action::Discard(CardId::new(3)),
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Suit(Suit::Red),
        },
        Action::Discard(CardId::new(11)),
        Action::Play(CardId::new(26)),
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Suit(Suit::Yellow),
        },
        Action::Discard(CardId::new(12)),
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Suit(Suit::Blue),
        },
        Action::Play(CardId::new(21)),
        Action::Discard(CardId::new(10)),
        Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Suit(Suit::Purple),
        },
        Action::Discard(CardId::new(17)),
        Action::Play(CardId::new(18)),
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Rank(Rank::Two),
        },
        Action::Play(CardId::new(19)),
        Action::Discard(CardId::new(16)),
        Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Rank(Rank::One),
        },
        Action::Discard(CardId::new(23)),
        Action::Discard(CardId::new(29)),
        Action::Play(CardId::new(2)),
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Rank(Rank::Four),
        },
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Rank(Rank::Four),
        },
        Action::Play(CardId::new(4)),
        Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Rank(Rank::Five),
        },
        Action::Play(CardId::new(37)),
        Action::Discard(CardId::new(22)),
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Rank(Rank::Three),
        },
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Suit(Suit::Yellow),
        },
        Action::Play(CardId::new(38)),
        Action::Discard(CardId::new(30)),
        Action::Discard(CardId::new(27)),
        Action::Discard(CardId::new(28)),
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Rank(Rank::Three),
        },
        Action::Discard(CardId::new(33)),
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Rank(Rank::Five),
        },
    ] {
        state.apply(action).unwrap();
    }
    let giver = LogicalDeductions::new(state.view_for(PlayerId::new(1)).unwrap()).unwrap();
    let bad_clue = Action::Clue {
        target: PlayerId::new(2),
        clue: Clue::Suit(Suit::Green),
    };
    assert_eq!(
        select_h_group_action(&giver, HGroupProfile::Max),
        Some(bad_clue),
        "target={:#?}; inference={:#?}; clues={:#?}; hazard={:?}",
        giver.view().hands[2],
        infer_h_group(&giver, HGroupProfile::Max),
        h_group_clue_candidates(&giver, HGroupProfile::Max),
        prospective_clue_hazard(
            giver.view(),
            HGroupProfile::Max,
            PlayerId::new(2),
            CardId::new(35),
            Clue::Suit(Suit::Green),
            &[
                CardId::new(31),
                CardId::new(35),
                CardId::new(44),
                CardId::new(46)
            ],
            true,
        )
    );
    state.apply(bad_clue).unwrap();
    let recipient = LogicalDeductions::new(state.view_for(PlayerId::new(2)).unwrap()).unwrap();
    let inferred = infer_h_group(&recipient, HGroupProfile::Max);
    assert_ne!(
        inferred.connection.map(|connection| connection.card),
        Some(CardId::new(44)),
        "the prior rank-5 stall created a false layered finesse: {inferred:#?}"
    );
    assert_ne!(
        select_h_group_action(&recipient, HGroupProfile::Max),
        Some(Action::Play(CardId::new(44)))
    );
}

#[test]
#[allow(clippy::too_many_lines)]
fn chop_moved_cards_do_not_claim_good_touch_identities() {
    let mut state = paired_sample_six_state();
    for action in [
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Rank(Rank::Two),
        },
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Suit(Suit::Yellow),
        },
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Rank(Rank::Two),
        },
        Action::Play(CardId::new(1)),
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Suit(Suit::Blue),
        },
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Rank(Rank::One),
        },
        Action::Discard(CardId::new(2)),
        Action::Play(CardId::new(8)),
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Suit(Suit::Purple),
        },
        Action::Play(CardId::new(15)),
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Suit(Suit::Purple),
        },
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Rank(Rank::Two),
        },
        Action::Play(CardId::new(18)),
        Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Suit(Suit::Purple),
        },
        Action::Play(CardId::new(10)),
        Action::Discard(CardId::new(16)),
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Rank(Rank::Five),
        },
        Action::Discard(CardId::new(11)),
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Suit(Suit::Red),
        },
        Action::Play(CardId::new(5)),
        Action::Discard(CardId::new(12)),
        Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Suit(Suit::Blue),
        },
        Action::Play(CardId::new(9)),
        Action::Play(CardId::new(13)),
        Action::Discard(CardId::new(21)),
        Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Rank(Rank::Four),
        },
        Action::Play(CardId::new(24)),
        Action::Discard(CardId::new(27)),
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Suit(Suit::Blue),
        },
        Action::Discard(CardId::new(14)),
        Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Suit(Suit::Yellow),
        },
        Action::Discard(CardId::new(7)),
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Rank(Rank::Four),
        },
        Action::Play(CardId::new(4)),
        Action::Discard(CardId::new(17)),
        Action::Play(CardId::new(20)),
        Action::Play(CardId::new(29)),
        Action::Discard(CardId::new(23)),
        Action::Play(CardId::new(28)),
        Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Suit(Suit::Blue),
        },
    ] {
        state.apply(action).unwrap();
    }
    // Force the ordinary chop discard whose observer-relative Good Touch
    // interpretation this test isolates. The policy now correctly prefers
    // disposing of the actor's older known trash first; that independent
    // ordering choice is not part of this inference invariant.
    state.apply(Action::Discard(CardId::new(25))).unwrap();
    let recipient = LogicalDeductions::new(state.view_for(PlayerId::new(2)).unwrap()).unwrap();
    let inferred = infer_h_group(&recipient, HGroupProfile::Max);
    assert!(
        inferred
            .cards
            .iter()
            .find(|card| card.card == CardId::new(22))
            .is_some_and(|card| card.identities.contains(Card::new(Suit::Green, Rank::Four))),
        "the rank-4 clue eliminated green 4 merely because another green 4 was chop-moved: {inferred:#?}"
    );
    assert_ne!(
        select_h_group_action(&recipient, HGroupProfile::Max),
        Some(Action::Play(CardId::new(22)))
    );
}

#[test]
fn out_of_order_fix_uses_the_historical_stack_height() {
    let mut state = paired_sample_eight_state();
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
            clue: Clue::Rank(Rank::Two),
        },
        Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Suit(Suit::Red),
        },
        Action::Discard(CardId::new(6)),
        Action::Play(CardId::new(14)),
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Rank(Rank::One),
        },
        Action::Play(CardId::new(8)),
        Action::Play(CardId::new(13)),
        Action::Discard(CardId::new(1)),
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Suit(Suit::Purple),
        },
        Action::Discard(CardId::new(11)),
        Action::Play(CardId::new(2)),
        Action::Play(CardId::new(5)),
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Suit(Suit::Red),
        },
    ] {
        state.apply(action).unwrap();
    }
    let giver = LogicalDeductions::new(state.view_for(PlayerId::new(0)).unwrap()).unwrap();
    assert_eq!(
        select_h_group_action(&giver, HGroupProfile::Max),
        Some(Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Rank(Rank::Four),
        })
    );
    for action in [
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Rank(Rank::Four),
        },
        Action::Play(CardId::new(9)),
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Suit(Suit::Green),
        },
    ] {
        state.apply(action).unwrap();
    }
    let deductions = LogicalDeductions::new(state.view_for(PlayerId::new(0)).unwrap()).unwrap();
    let inferred = infer_h_group(&deductions, HGroupProfile::Max);
    assert!(
        inferred
            .clues
            .iter()
            .any(|clue| { clue.turn == 15 && clue.kind == HGroupClueKind::Unrecognized })
    );
    assert_ne!(
        inferred.connection.map(|connection| connection.card),
        Some(CardId::new(4)),
        "the fix clue was replayed as a new Prompt: {inferred:#?}"
    );
    assert_ne!(
        select_h_group_action(&deductions, HGroupProfile::Max),
        Some(Action::Play(CardId::new(4)))
    );
}

#[test]
#[allow(clippy::too_many_lines)]
fn critical_save_is_not_treated_as_an_out_of_order_play() {
    let mut state = paired_sample_three_state();
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
            clue: Clue::Rank(Rank::Two),
        },
        Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Suit(Suit::Red),
        },
        Action::Discard(CardId::new(6)),
        Action::Play(CardId::new(14)),
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Rank(Rank::One),
        },
        Action::Play(CardId::new(8)),
        Action::Play(CardId::new(13)),
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Suit(Suit::Red),
        },
        Action::Play(CardId::new(5)),
        Action::Discard(CardId::new(11)),
        Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Suit(Suit::Green),
        },
        Action::Play(CardId::new(9)),
        Action::Play(CardId::new(20)),
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Rank(Rank::Two),
        },
        Action::Discard(CardId::new(7)),
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Suit(Suit::Blue),
        },
    ] {
        state.apply(action).unwrap();
    }
    let deductions = LogicalDeductions::new(state.view_for(PlayerId::new(0)).unwrap()).unwrap();
    let inferred = infer_h_group(&deductions, HGroupProfile::Max);
    assert!(
        inferred
            .clues
            .iter()
            .any(|clue| { clue.turn == 17 && matches!(clue.kind, HGroupClueKind::Save(_)) })
    );
    assert_ne!(
        select_h_group_action(&deductions, HGroupProfile::Max),
        Some(Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Rank(Rank::Three),
        }),
        "a Save clue incorrectly created a mandatory out-of-order fix: {inferred:#?}"
    );
    for action in [
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Rank(Rank::Five),
        },
        Action::Play(CardId::new(17)),
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Suit(Suit::Blue),
        },
        Action::Discard(CardId::new(3)),
        Action::Play(CardId::new(15)),
        Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Suit(Suit::Yellow),
        },
        Action::Play(CardId::new(2)),
        Action::Discard(CardId::new(26)),
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Rank(Rank::Five),
        },
        Action::Discard(CardId::new(25)),
        Action::Discard(CardId::new(24)),
        Action::Clue {
            target: PlayerId::new(1),
            clue: Clue::Suit(Suit::Yellow),
        },
        Action::Discard(CardId::new(27)),
    ] {
        state.apply(action).unwrap();
    }
    let deductions = LogicalDeductions::new(state.view_for(PlayerId::new(1)).unwrap()).unwrap();
    let diagnostics = format!(
        "tokens {}; clues {:#?}; candidates {:?}; chops {:?}",
        deductions.view().clue_tokens,
        h_group_clue_candidates(&deductions, HGroupProfile::Max),
        ordered_h_group_actions(&deductions, HGroupProfile::Max),
        infer_h_group(&deductions, HGroupProfile::Max).chops
    );
    let fallback = select_h_group_action(&deductions, HGroupProfile::Max);
    assert_ne!(
        fallback,
        Some(Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Suit(Suit::Red),
        }),
        "the no-candidate fallback selected a trash clue that looks like a Play clue: {diagnostics}"
    );
    assert_ne!(
        fallback,
        Some(Action::Clue {
            target: PlayerId::new(2),
            clue: Clue::Rank(Rank::Three),
        }),
        "the fallback treated an off-chop critical card as a Save: {diagnostics}"
    );
}
