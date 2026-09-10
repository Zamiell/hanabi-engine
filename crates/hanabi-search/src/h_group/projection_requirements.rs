//! Compiled semantic dependencies of projected actions. The projector does
//! not recognize conventions or scan the signal journal itself.

use hanabi_core::{Action, Card, CardId, ObservedEvent, PlayerId, PlayerView};

use super::{
    HGroupInferences, HGroupMoveKind, HGroupPlayObligation, HiddenCardCondition, LogicalDeductions,
    identity_of, was_clued_before,
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ProjectionRequirementKind {
    /// Priority cannot be inferred from absence if another hidden hand may
    /// contain this connector. Cards drawn after the signal do not count.
    NoUnobservedConnector { identity: Card, signal_turn: u32 },
    /// The Charm's threshold must survive a hidden external connector.
    BlindPlayThreshold {
        giver: PlayerId,
        focus: CardId,
        minimum: usize,
        clue_turn: u32,
        heights: [u8; 5],
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProjectionRequirement {
    pub actor: PlayerId,
    pub action: Action,
    pub evidence_turn: u32,
    pub kind: ProjectionRequirementKind,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DependencyStatus {
    Supported,
    Conditional {
        witness: Option<HiddenCardCondition>,
    },
    Contradicted,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DependencyAssessment {
    pub requirement: ProjectionRequirement,
    pub status: DependencyStatus,
}

/// Declaration belongs to compilation, alongside action scheduling. Signals
/// supply causal evidence here; consumers receive typed requirements only.
pub(super) fn compile(view: &PlayerView, notes: &HGroupInferences) -> Vec<ProjectionRequirement> {
    let actor = view.observer;
    let mut requirements = Vec::new();
    for card in &view.hands[actor.index()] {
        // https://hanabi.github.io/level-25/#the-load-clue
        if notes.cards.iter().any(|note| {
            note.card == card.id && note.play_obligation == Some(HGroupPlayObligation::Forced)
        }) {
            if let Some(signal) = notes.signals.iter().rev().find(|signal| {
                signal.kind == HGroupMoveKind::Priority
                    && signal.target == Some(actor)
                    && signal.cards.contains(&card.id)
            }) {
                if let Some(identity) = signal.identity {
                    requirements.push(ProjectionRequirement {
                        actor,
                        action: Action::Play(card.id),
                        evidence_turn: signal.turn,
                        kind: ProjectionRequirementKind::NoUnobservedConnector {
                            identity,
                            signal_turn: signal.turn,
                        },
                    });
                }
            }
        }
        // https://hanabi.github.io/level-23/#the-4-charm
        if let Some(signal) = notes.signals.iter().rev().find(|signal| {
            signal.turn + 1 == view.turn
                && signal.kind == HGroupMoveKind::Charm
                && signal.target == Some(actor)
                && signal.cards.contains(&card.id)
        }) {
            if let Some(clue) = notes.clues.iter().find(|clue| clue.turn == signal.turn) {
                requirements.push(ProjectionRequirement {
                    actor,
                    action: Action::Play(card.id),
                    evidence_turn: signal.turn,
                    kind: ProjectionRequirementKind::BlindPlayThreshold {
                        giver: clue.giver,
                        focus: clue.focus,
                        minimum: 3,
                        clue_turn: signal.turn,
                        heights: clue.stack_heights,
                    },
                });
            }
        }
    }
    requirements
}

pub(super) fn assess(
    view: &PlayerView,
    requirement: &ProjectionRequirement,
) -> DependencyAssessment {
    let status = if let Action::Play(card) = requirement.action {
        if view.hands[requirement.actor.index()]
            .iter()
            .any(|slot| slot.id == card)
        {
            assess_kind(view, requirement)
        } else {
            DependencyStatus::Contradicted
        }
    } else {
        DependencyStatus::Supported
    };
    DependencyAssessment {
        requirement: requirement.clone(),
        status,
    }
}

fn assess_kind(view: &PlayerView, requirement: &ProjectionRequirement) -> DependencyStatus {
    let Ok(deductions) = LogicalDeductions::new(view.clone()) else {
        return DependencyStatus::Conditional { witness: None };
    };
    match requirement.kind {
        ProjectionRequirementKind::NoUnobservedConnector {
            identity,
            signal_turn,
        } => {
            for (owner, hand) in view.hands.iter().enumerate() {
                if owner == requirement.actor.index() {
                    continue;
                }
                for card in hand.iter().filter(|card| card.identity.is_none()) {
                    if view.history.iter().any(|entry| entry.turn >= signal_turn
                        && matches!(entry.event, ObservedEvent::Drew { card: drawn, .. } if drawn == card.id))
                    { continue; }
                    if deductions
                        .possible_identities(card.id)
                        .is_none_or(|domain| domain.contains(identity))
                    {
                        return conditional(view, owner, card.id, identity);
                    }
                }
            }
        }
        ProjectionRequirementKind::BlindPlayThreshold {
            giver,
            focus,
            minimum,
            clue_turn,
            heights,
        } => {
            let Some(focus) = identity_of(view, focus) else {
                return DependencyStatus::Conditional { witness: None };
            };
            let clued = view
                .hands
                .iter()
                .flatten()
                .filter(|card| was_clued_before(view, clue_turn, card.id))
                .map(|card| card.id)
                .collect();
            for (owner, hand) in view.hands.iter().enumerate() {
                if owner == requirement.actor.index() || owner == giver.index() {
                    continue;
                }
                for (slot, card) in hand
                    .iter()
                    .enumerate()
                    .filter(|(_, card)| card.identity.is_none())
                {
                    let Some(domain) = deductions.possible_identities(card.id) else {
                        return DependencyStatus::Conditional { witness: None };
                    };
                    for identity in domain.iter().filter(|identity| {
                        identity.suit == focus.suit && identity.rank < focus.rank
                    }) {
                        let mut possible = view.clone();
                        possible.hands[owner][slot].identity = Some(identity);
                        if super::recognition::four_charm_blind_plays(
                            &possible,
                            giver,
                            requirement.actor,
                            focus,
                            heights,
                            &clued,
                            clue_turn,
                        ) < minimum
                        {
                            return conditional(view, owner, card.id, identity);
                        }
                    }
                }
            }
        }
    }
    DependencyStatus::Supported
}

fn conditional(view: &PlayerView, owner: usize, card: CardId, identity: Card) -> DependencyStatus {
    DependencyStatus::Conditional {
        witness: Some(HiddenCardCondition {
            observer: view.observer,
            owner: PlayerId::new(u8::try_from(owner).expect("at most five players")),
            card,
            identity,
        }),
    }
}
