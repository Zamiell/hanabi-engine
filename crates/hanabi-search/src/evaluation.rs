use core::{fmt, str::FromStr};

use hanabi_core::{Action, Card, EndReason, FullState, GameStatus, Rank, Suit};

/// The primary result that search should optimize.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub enum SearchObjective {
    /// Maximize expected official score, with strategic terms as tie-breakers.
    #[default]
    ExpectedScore,
    /// Maximize the chance of scoring 25 before preferring lesser outcomes.
    PerfectScore,
}

impl fmt::Display for SearchObjective {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::ExpectedScore => "expected-score",
            Self::PerfectScore => "perfect-score",
        })
    }
}

impl FromStr for SearchObjective {
    type Err = ParseSearchObjectiveError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "expected-score" => Ok(Self::ExpectedScore),
            "perfect-score" => Ok(Self::PerfectScore),
            _ => Err(ParseSearchObjectiveError(value.to_owned())),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ParseSearchObjectiveError(String);

impl fmt::Display for ParseSearchObjectiveError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "unknown search objective {:?}; expected expected-score or perfect-score",
            self.0
        )
    }
}

impl std::error::Error for ParseSearchObjectiveError {}

/// Strategic facts collected while simulator truth advances a rollout.
///
/// These facts evaluate a completed line. They are deliberately not passed to
/// the action-selection policy, so exact deck order cannot leak into play.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct StrategicMetrics {
    pub perfect: bool,
    pub score_ceiling: u8,
    pub clue_actions: u16,
    pub newly_touched_cards: u16,
    pub tempo_clues: u16,
    pub critical_discards: u16,
    /// Severity-weighted critical discards whose remaining copy was late in
    /// the deck. One unit is an ordinary critical discard; values approach two
    /// for a sole alternative at the bottom of a full deck.
    pub bottom_deck_risk: f64,
    /// Mean risk-weighted number of useful, unclued chop cards still requiring
    /// protection along the simulated line.
    pub clue_debt: f64,
    pub evaluated_positions: u16,
    pub predictable_turns: u16,
}

impl SearchObjective {
    #[must_use]
    pub(crate) const fn normalization(self) -> f64 {
        match self {
            Self::ExpectedScore => 252_600.0,
            Self::PerfectScore => 1_252_600.0,
        }
    }

    /// Scalar team value used for tree learning and root comparison.
    /// Official score always dominates secondary terms for expected-score;
    /// perfect games dominate every non-perfect game for perfect-score.
    #[must_use]
    pub fn utility(self, official_score: u8, raw_score: u8, metrics: StrategicMetrics) -> f64 {
        let score = f64::from(official_score);
        let raw = f64::from(raw_score);
        let ceiling = f64::from(metrics.score_ceiling);
        let clues = f64::from(metrics.clue_actions);
        let critical = f64::from(metrics.critical_discards);
        let risk = metrics.bottom_deck_risk;
        let debt = metrics.clue_debt;
        match self {
            Self::ExpectedScore => {
                score * 10_000.0 + raw * 100.0 + ceiling * 4.0
                    - clues * 0.5
                    - critical * 12.0
                    - risk * 20.0
                    - debt * 5.0
            }
            Self::PerfectScore => {
                f64::from(metrics.perfect) * 1_000_000.0
                    + score * 10_000.0
                    + raw * 100.0
                    + ceiling * 40.0
                    - clues
                    - critical * 120.0
                    - risk * 250.0
                    - debt * 100.0
            }
        }
    }
}

pub(crate) fn observe_action(state: &FullState, action: Action, metrics: &mut StrategicMetrics) {
    match action {
        Action::Clue { target, clue } => {
            metrics.clue_actions = metrics.clue_actions.saturating_add(1);
            let newly_touched = state
                .view_for(state.current_player())
                .and_then(|view| view.hands.get(target.index()).cloned())
                .map_or(0, |hand| {
                    hand.into_iter()
                        .filter(|observed| {
                            state
                                .card(observed.id)
                                .is_some_and(|identity| clue.matches(identity))
                                && !observed.clues.has_positive_clue(clue)
                        })
                        .count()
                });
            let newly_touched = u16::try_from(newly_touched)
                .expect("a standard Hanabi hand has at most five cards");
            metrics.newly_touched_cards = metrics.newly_touched_cards.saturating_add(newly_touched);
            if newly_touched == 0 {
                metrics.tempo_clues = metrics.tempo_clues.saturating_add(1);
            }
        }
        Action::Discard(card_id) => {
            let Some(card) = state.card(card_id) else {
                return;
            };
            if is_critical_now(state, card, card_id) {
                metrics.critical_discards = metrics.critical_discards.saturating_add(1);
                metrics.bottom_deck_risk += bottom_deck_severity(state, card, card_id);
            }
        }
        Action::Play(_) => {}
    }
}

pub(crate) fn observe_position(state: &FullState, metrics: &mut StrategicMetrics) {
    metrics.evaluated_positions = metrics.evaluated_positions.saturating_add(1);
    let Some(view) = state.view_for(state.current_player()) else {
        return;
    };
    for (player, hand) in state.hands().iter().enumerate() {
        let Some(chop) = hand.first().copied() else {
            continue;
        };
        let Some(observed) = view.hands[player].iter().find(|card| card.id == chop) else {
            continue;
        };
        if !observed.clues.is_empty() {
            continue;
        }
        let Some(identity) = state.card(chop) else {
            continue;
        };
        if state.play_stacks()[identity.suit.index()].len() >= usize::from(identity.rank.number()) {
            continue;
        }
        let deck = state.draw_pile().collect::<Vec<_>>();
        if let Some(index) = deck
            .iter()
            .rposition(|candidate| state.card(*candidate) == Some(identity))
        {
            let index = u32::try_from(index).expect("a standard deck has fifty cards");
            let deck_size =
                u32::try_from(deck.len().max(1)).expect("a standard deck has fifty cards");
            let lateness = f64::from(index) / f64::from(deck_size);
            if lateness >= 0.5 {
                metrics.clue_debt += lateness;
            }
        } else if identity.rank.copies() == 1 {
            metrics.clue_debt += 1.0;
        }
    }
}

pub(crate) fn finish_metrics(state: &FullState, mut metrics: StrategicMetrics) -> StrategicMetrics {
    metrics.perfect = state.final_score() == Some(25);
    metrics.score_ceiling = score_ceiling(state);
    if metrics.evaluated_positions > 0 {
        metrics.clue_debt /= f64::from(metrics.evaluated_positions);
    }
    metrics
}

fn is_critical_now(state: &FullState, identity: Card, discarded: hanabi_core::CardId) -> bool {
    if state.play_stacks()[identity.suit.index()].len() >= usize::from(identity.rank.number()) {
        return false;
    }
    let remaining = state
        .hands()
        .iter()
        .flatten()
        .copied()
        .chain(state.draw_pile())
        .filter(|candidate| *candidate != discarded && state.card(*candidate) == Some(identity))
        .count();
    remaining <= 1
}

fn bottom_deck_severity(state: &FullState, identity: Card, discarded: hanabi_core::CardId) -> f64 {
    let deck = state.draw_pile().collect::<Vec<_>>();
    let last = deck
        .iter()
        .rposition(|candidate| *candidate != discarded && state.card(*candidate) == Some(identity));
    match last {
        None => 2.0,
        Some(index) => {
            let denominator =
                u32::try_from(deck.len().max(1)).expect("a standard deck has fifty cards");
            let index = u32::try_from(index).expect("a standard deck has fifty cards");
            1.0 + f64::from(index) / f64::from(denominator)
        }
    }
}

/// Highest score still reachable in this determinization after all discards.
#[must_use]
pub fn score_ceiling(state: &FullState) -> u8 {
    let mut discarded = [[0_u8; 5]; 5];
    for id in state.discard_pile() {
        if let Some(card) = state.card(*id) {
            discarded[card.suit.index()][card.rank.index()] += 1;
        }
    }
    Suit::ALL
        .iter()
        .map(|suit| {
            let played = state.play_stacks()[suit.index()].len();
            let blocked = Rank::ALL.iter().position(|rank| {
                rank.index() >= played && discarded[suit.index()][rank.index()] >= rank.copies()
            });
            u8::try_from(blocked.unwrap_or(5)).unwrap_or(5)
        })
        .sum()
}

#[must_use]
pub(crate) fn is_strikeout(state: &FullState) -> bool {
    state.status() == GameStatus::Finished(EndReason::TooManyStrikes)
}
