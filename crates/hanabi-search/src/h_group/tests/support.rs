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
