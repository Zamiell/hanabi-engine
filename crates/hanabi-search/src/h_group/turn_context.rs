use hanabi_core::{CardId, ClueFacts, ObservedHistoryEntry};

/// Public convention-relevant state at one side of an observed event.
#[derive(Clone, Debug)]
pub(super) struct HGroupTurnSnapshot {
    pub(super) hands: Vec<Vec<CardId>>,
    pub(super) facts: Vec<ClueFacts>,
    pub(super) stack_heights: [u8; 5],
    pub(super) clue_tokens: u8,
    pub(super) deck_size: usize,
    pub(super) early_game: bool,
}

impl HGroupTurnSnapshot {
    pub(super) fn new(
        hands: &[Vec<CardId>],
        facts: &[ClueFacts],
        stack_heights: [u8; 5],
        clue_tokens: u8,
        deck_size: usize,
        early_game: bool,
    ) -> Self {
        Self {
            hands: hands.to_vec(),
            facts: facts.to_vec(),
            stack_heights,
            clue_tokens,
            deck_size,
            early_game,
        }
    }
}

/// Borrowed convention state after an observed event has been reduced.
///
/// Unlike the pre-event snapshot this does not clone the replay's hot hand and
/// clue-fact tables. Rule evaluation is complete before the next event mutates
/// either table.
pub(super) struct HGroupTurnView<'a> {
    pub(super) hands: &'a [Vec<CardId>],
    pub(super) facts: &'a [ClueFacts],
    pub(super) stack_heights: [u8; 5],
    pub(super) clue_tokens: u8,
    pub(super) deck_size: usize,
    pub(super) early_game: bool,
}

/// One event with explicit pre- and post-event convention state.
///
/// Convention rules must select the side they require rather than depending
/// on where an effect function happens to be called in the replay loop.
pub(super) struct HGroupTurnContext<'a> {
    pub(super) entry: &'a ObservedHistoryEntry,
    pub(super) before: HGroupTurnSnapshot,
    pub(super) after: HGroupTurnView<'a>,
    /// Whether the acting player considered this an ordinary chop discard
    /// before the public event changed their hand.
    pub(super) actor_saw_normal_discard: bool,
}
