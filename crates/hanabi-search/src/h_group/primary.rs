use hanabi_core::{Card, CardId, Clue, ClueFacts, Rank};

use super::{HGroupClueInterpretation, HGroupClueKind, HGroupSaveKind, IdentitySet};

pub(super) fn strictly_narrows(before: IdentitySet, after: IdentitySet) -> bool {
    !after.is_empty() && after != before && after.intersection(before) == after
}

/// Pre-clue promise narrowed by accumulated literal information. This is
/// deliberately independent of the current physical face and current clue.
pub(super) fn prior_superposition<'a>(
    card: CardId,
    facts: ClueFacts,
    stacks: [u8; 5],
    clues: impl DoubleEndedIterator<Item = &'a HGroupClueInterpretation>,
) -> IdentitySet {
    let literal = IdentitySet::from_mask(facts.identity_mask());
    let Some(prior) = clues.rev().find(|prior| prior.focus == card) else {
        return literal;
    };
    let promised = prior
        .play_identities
        .union(prior.save_identities)
        .intersection(literal);
    if promised.is_empty() {
        return literal;
    }
    let useful = IdentitySet::from_mask(
        promised
            .iter()
            .filter(|identity| identity.rank.number() > stacks[identity.suit.index()])
            .fold(0, |mask, identity| mask | (1 << identity.index())),
    );
    if useful.is_empty() { promised } else { useful }
}

/// Exact identity established for a protected card without looking at its
/// hidden face. Shared by Save and Chop Move value checks.
pub(super) fn protected_identity_from_clues<'a>(
    card: CardId,
    facts: ClueFacts,
    stacks: [u8; 5],
    clues: impl DoubleEndedIterator<Item = &'a HGroupClueInterpretation>,
) -> Option<Card> {
    let literal = IdentitySet::from_mask(facts.identity_mask());
    if literal.len() == 1 {
        return literal.iter().next();
    }
    let remaining = remaining_prior_play_identities(card, facts, stacks, clues);
    (remaining.len() == 1)
        .then(|| remaining.iter().next())
        .flatten()
}

/// Evaluate an existing useful-card promise against the literal information
/// and stacks *before* a reclue. Later fill-ins and completed predecessors can
/// resolve an originally ambiguous Play promise without another Play Clue.
pub(super) fn prior_play_is_ready<'a>(
    card: CardId,
    facts: ClueFacts,
    stacks: [u8; 5],
    clues: impl DoubleEndedIterator<Item = &'a HGroupClueInterpretation>,
) -> bool {
    let remaining = remaining_prior_play_identities(card, facts, stacks, clues);
    !remaining.is_empty()
        && remaining
            .iter()
            .all(|identity| identity.rank.number() == stacks[identity.suit.index()] + 1)
}

/// Remaining identities of an existing useful Play promise, using only the
/// literal facts and stack heights available at the event being interpreted.
pub(super) fn remaining_prior_play_identities<'a>(
    card: CardId,
    facts: ClueFacts,
    stacks: [u8; 5],
    clues: impl DoubleEndedIterator<Item = &'a HGroupClueInterpretation>,
) -> IdentitySet {
    clues
        .rev()
        .find(|clue| clue.focus == card)
        .map_or_else(IdentitySet::default, |clue| {
            IdentitySet::from_mask(
                clue.play_identities
                    .iter()
                    .filter(|identity| {
                        facts.allows(*identity)
                            && identity.rank.number() > stacks[identity.suit.index()]
                    })
                    .fold(0, |mask, identity| mask | (1 << identity.index())),
            )
        })
}

/// A higher-precedence meaning that suppresses an ordinary Play/Save reading.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum PrimarySuppression {
    Fix,
    FiveChopMove,
    LowScoreFive,
    EarlyFiveStall,
    EightClueFiveStall,
    NoInformationReclue,
    AlreadyPlayingReclue,
}

/// Inputs to the one primary clue-precedence resolver.
#[derive(Clone, Copy, Debug)]
pub(super) struct PrimaryClueInputs {
    pub(super) clue: Clue,
    pub(super) play_identities: IdentitySet,
    pub(super) save_identities: IdentitySet,
    pub(super) stack_heights: [u8; 5],
    pub(super) eight_clue_save: bool,
    /// Ordered highest-to-lowest precedence overrides that apply to this
    /// clue. An array keeps the hot path allocation-free while avoiding an
    /// error-prone bag of unrelated boolean parameters.
    pub(super) suppressions: [Option<PrimarySuppression>; 7],
}

/// Canonical primary meaning shared by replay and prospective interpretation.
/// Secondary rules may add connections or explanations, but cannot silently
/// replace this meaning through a separate precedence ladder.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct ClueInterpretationPlan {
    pub(super) kind: HGroupClueKind,
    pub(super) play_identities: IdentitySet,
    pub(super) save_identities: IdentitySet,
    pub(super) suppression: Option<PrimarySuppression>,
}

impl ClueInterpretationPlan {
    pub(super) fn resolve(inputs: PrimaryClueInputs) -> Self {
        // Rank 2/5 saves and non-playable critical saves take precedence over
        // a hypothetical play reading. This preserves explicit Play-or-Save
        // superpositions only where neither branch outranks the other.
        let save_precedence = IdentitySet::from_mask(
            inputs
                .save_identities
                .iter()
                .filter(|identity| {
                    inputs.eight_clue_save
                        || matches!(
                            (inputs.clue, identity.rank),
                            (Clue::Rank(Rank::Two), Rank::Two)
                                | (Clue::Rank(Rank::Five), Rank::Five)
                        )
                        || identity.rank.number() > inputs.stack_heights[identity.suit.index()] + 1
                })
                .fold(0, |mask, identity| mask | (1 << identity.index())),
        );
        let mut play_identities = inputs.play_identities.without(save_precedence);
        let suppression = inputs.suppressions.into_iter().flatten().next();
        let kind = if suppression.is_some() {
            play_identities = IdentitySet::default();
            HGroupClueKind::Unrecognized
        } else if inputs.eight_clue_save && !inputs.save_identities.is_empty() {
            HGroupClueKind::Save(HGroupSaveKind::EightClue)
        } else {
            kind_from_masks(inputs.clue, play_identities, inputs.save_identities)
        };
        Self {
            kind,
            play_identities,
            save_identities: inputs.save_identities,
            suppression,
        }
    }
}

fn kind_from_masks(clue: Clue, play: IdentitySet, save: IdentitySet) -> HGroupClueKind {
    match (play.is_empty(), save.is_empty()) {
        (false, false) => HGroupClueKind::PlayOrSave,
        (false, true) => HGroupClueKind::Play,
        (true, false) => match clue {
            Clue::Rank(Rank::Five) => HGroupClueKind::Save(HGroupSaveKind::Five),
            Clue::Rank(Rank::Two) => HGroupClueKind::Save(HGroupSaveKind::Two),
            Clue::Suit(_) | Clue::Rank(_) => HGroupClueKind::Save(HGroupSaveKind::Critical),
        },
        (true, true) => HGroupClueKind::Unrecognized,
    }
}

#[cfg(test)]
mod tests {
    use hanabi_core::{Card, Suit};

    use super::*;

    fn identity(suit: Suit, rank: Rank) -> IdentitySet {
        IdentitySet::singleton(Card::new(suit, rank))
    }

    #[test]
    fn information_gain_requires_a_nonempty_strict_subset() {
        let four = identity(Suit::Purple, Rank::Four);
        let five = identity(Suit::Purple, Rank::Five);
        let both = four.union(five);
        assert!(strictly_narrows(both, four));
        assert!(!strictly_narrows(four, four));
        assert!(!strictly_narrows(four, five));
        assert!(!strictly_narrows(four, both));
        assert!(!strictly_narrows(four, IdentitySet::default()));
    }

    #[test]
    fn five_chop_move_suppresses_a_hypothetical_play_reading() {
        let plan = ClueInterpretationPlan::resolve(PrimaryClueInputs {
            clue: Clue::Rank(Rank::Five),
            play_identities: identity(Suit::Red, Rank::Five),
            save_identities: IdentitySet::default(),
            stack_heights: [4; 5],
            eight_clue_save: false,
            suppressions: [
                None,
                Some(PrimarySuppression::FiveChopMove),
                None,
                None,
                None,
                None,
                None,
            ],
        });
        assert_eq!(plan.kind, HGroupClueKind::Unrecognized);
        assert!(plan.play_identities.is_empty());
        assert_eq!(plan.suppression, Some(PrimarySuppression::FiveChopMove));
    }

    #[test]
    fn playable_rank_two_on_chop_remains_a_save() {
        let red_two = identity(Suit::Red, Rank::Two);
        let plan = ClueInterpretationPlan::resolve(PrimaryClueInputs {
            clue: Clue::Rank(Rank::Two),
            play_identities: red_two,
            save_identities: red_two,
            stack_heights: [1, 0, 0, 0, 0],
            eight_clue_save: false,
            suppressions: [None; 7],
        });
        assert_eq!(plan.kind, HGroupClueKind::Save(HGroupSaveKind::Two));
        assert!(plan.play_identities.is_empty());
    }
}
