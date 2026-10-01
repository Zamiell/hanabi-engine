//! Keep a teammate's private-information policy and its consequences in the
//! same possible world. Only bounded, exhaustive splits earn branch evidence.
use super::{HGroupProfile, PerspectiveDepth, PerspectiveProjector, infer_h_group_from_replay};
use crate::{AnalysisControl, AnalysisStopped, IdentitySet, LogicalDeductions};
use hanabi_core::{Card, CardId, PlayerView};

pub(super) enum Split {
    Unneeded,
    Unresolved,
    Worlds(Vec<Vec<(CardId, Card)>>),
}

pub(super) fn threatened_chop(
    source: &PlayerView,
    public: &PlayerView,
    profile: HGroupProfile,
    budget: usize,
    control: &AnalysisControl,
) -> Result<Split, AnalysisStopped> {
    control.checkpoint()?;
    if public.clue_tokens == 0 || super::inverse_planning::is_active() {
        return Ok(Split::Unneeded);
    }
    let actor = public.current_player;
    let target = super::next_player(actor, public.hands.len());
    let Some((d, r)) = PerspectiveProjector::new(public, profile)
        .project(target, PerspectiveDepth::NestedRecipients)
    else {
        return Ok(Split::Unneeded);
    };
    let notes = infer_h_group_from_replay(&d, r, profile);
    if !notes.playable_now.is_empty() || notes.connection.is_some() {
        return Ok(Split::Unneeded);
    }
    let Some(chop) = notes.chops[target.index()] else {
        return Ok(Split::Unneeded);
    };
    let (loss, risk) = super::symbolic_line::important_discard(public, profile, chop);
    let Some(identity) = risk.filter(|_| loss.is_some()) else {
        return Ok(Split::Unneeded);
    };
    let Ok(d) = LogicalDeductions::new(public.clone()) else {
        return Ok(Split::Unresolved);
    };
    let r = super::replay_h_group(&d, profile);
    let notes = super::convention_card_inferences(&d, &r);
    let mut domains = Vec::new();
    for (owner, hand) in public.hands.iter().enumerate() {
        if owner == actor.index() {
            continue;
        }
        for held in hand {
            if held.identity.is_some()
                || !super::was_clued_before(public, public.turn, held.id)
                || !source
                    .hands
                    .iter()
                    .flatten()
                    .any(|old| old.id == held.id && old.identity.is_none())
            {
                continue;
            }
            let domain = notes
                .iter()
                .find(|n| n.card == held.id)
                .map_or_else(
                    || IdentitySet::from_mask(held.clues.identity_mask()),
                    |n| n.identities,
                )
                .intersection(IdentitySet::from_mask(held.clues.identity_mask()));
            if domain.contains(identity) {
                domains.push((held.id, domain));
            }
        }
    }
    if domains.is_empty() {
        return Ok(Split::Unneeded);
    }
    let size = domains
        .iter()
        .try_fold(1_usize, |n, (_, d)| n.checked_mul(d.len()));
    if size.is_none_or(|size| size > budget) {
        return Ok(Split::Unresolved);
    }
    Ok(enumerate(public, &r, domains))
}

fn enumerate(
    public: &PlayerView,
    r: &super::HGroupState,
    domains: Vec<(CardId, IdentitySet)>,
) -> Split {
    // An ordinary first rank Play Clue promises distinct useful copies. Do
    // not invent same-identity worlds that already violate that promise.
    let distinct: Vec<_> = r
        .clues
        .iter()
        .filter(|c| {
            c.kind == super::HGroupClueKind::Play
                && matches!(c.clue, hanabi_core::Clue::Rank(_))
                && c.non_focus_trash_identities.is_empty()
                && !c.previously_gotten.contains(&c.focus)
        })
        .flat_map(|c| {
            let cards: Vec<_> = std::iter::once(c.focus)
                .chain(c.new_non_focus.iter().copied())
                .collect();
            let mut pairs = Vec::new();
            for (i, a) in cards.iter().enumerate() {
                for b in &cards[i + 1..] {
                    pairs.push((*a, *b));
                }
            }
            pairs
        })
        .collect();
    let mut counts = [0_u8; 25];
    for suit in hanabi_core::Suit::ALL {
        for rank in hanabi_core::Rank::ALL {
            counts[Card::new(suit, rank).index()] = rank.copies();
        }
    }
    for id in public
        .hands
        .iter()
        .flatten()
        .filter_map(|c| c.identity)
        .chain(public.play_stacks.iter().flatten().map(|(_, c)| *c))
        .chain(public.discard_pile.iter().map(|(_, c)| *c))
    {
        counts[id.index()] = counts[id.index()].saturating_sub(1);
    }
    let mut worlds = vec![Vec::new()];
    for (id, domain) in domains {
        let mut next = Vec::new();
        for world in worlds {
            for identity in domain.iter() {
                if world.iter().filter(|(_, c)| *c == identity).count()
                    >= usize::from(counts[identity.index()])
                {
                    continue;
                }
                if distinct.iter().any(|(a, b)| {
                    let other = if *a == id {
                        Some(*b)
                    } else if *b == id {
                        Some(*a)
                    } else {
                        None
                    };
                    other.and_then(|o| {
                        world
                            .iter()
                            .find(|(c, _)| *c == o)
                            .map(|(_, i)| *i)
                            .or_else(|| super::identity_of(public, o))
                    }) == Some(identity)
                }) {
                    continue;
                }
                let mut assigned = world.clone();
                assigned.push((id, identity));
                next.push(assigned);
            }
        }
        worlds = next;
    }
    if worlds.is_empty() {
        Split::Unresolved
    } else {
        Split::Worlds(worlds)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use hanabi_core::{Action, Clue, PlayerId, Rank, Suit};

    fn reviewed_window() -> (PlayerView, PlayerView) {
        let fixture = hanabi_protocol::HanabiLiveReplay::from_json(include_str!(
            "../../../hanabi-protocol/tests/fixtures/game-p4v0s9.json"
        ))
        .unwrap();
        let source = fixture
            .state_at_turn(5)
            .unwrap()
            .view_for(PlayerId::new(1))
            .unwrap();
        let mut state = fixture.state_at_turn(6).unwrap();
        for action in [
            Action::Clue {
                target: PlayerId::new(0),
                clue: Clue::Suit(Suit::Blue),
            },
            Action::Play(CardId::new(12)),
            Action::Play(CardId::new(1)),
            Action::Clue {
                target: PlayerId::new(3),
                clue: Clue::Suit(Suit::Blue),
            },
            Action::Play(CardId::new(10)),
            Action::Play(CardId::new(15)),
            Action::Play(CardId::new(3)),
            Action::Clue {
                target: PlayerId::new(2),
                clue: Clue::Rank(Rank::Five),
            },
        ] {
            state.apply(action).unwrap();
        }
        let mut public = state.view_for(PlayerId::new(1)).unwrap();
        for held in public.hands.iter_mut().flatten() {
            if !source.hands.iter().flatten().any(|c| c.id == held.id) {
                held.identity = None;
            }
        }
        (source, public)
    }

    #[test]
    fn reviewed_private_protection_keeps_conditions_and_losses_together() {
        let (source, public) = reviewed_window();
        let original = public.clone();
        let control = AnalysisControl::default();
        let Split::Worlds(worlds) =
            threatened_chop(&source, &public, HGroupProfile::Max, 32, &control).unwrap()
        else {
            panic!("reviewed ambiguity must branch");
        };
        assert!(worlds.len() > 1);
        let red = Card::new(Suit::Red, Rank::Three);
        let mut covered = 0;
        let mut protected = 0;
        for assignments in worlds {
            let mut world = public.clone();
            for (id, identity) in &assignments {
                world
                    .hands
                    .iter_mut()
                    .flatten()
                    .find(|c| c.id == *id)
                    .unwrap()
                    .identity = Some(*identity);
            }
            let root = Action::Clue {
                target: PlayerId::new(0),
                clue: Clue::Rank(Rank::Two),
            };
            assert_eq!(
                super::super::frontier_value::evaluate(&source, &world, HGroupProfile::Max, root),
                super::super::frontier_value::evaluate(&source, &public, HGroupProfile::Max, root),
                "private hypotheses cannot earn free information: {assignments:?}"
            );
            let (d, r) = PerspectiveProjector::new(&world, HGroupProfile::Max)
                .project(PlayerId::new(2), PerspectiveDepth::NestedRecipients)
                .unwrap();
            let clue = Action::Clue {
                target: PlayerId::new(3),
                clue: Clue::Suit(Suit::Red),
            };
            if assignments.iter().any(|(_, c)| *c == red) {
                covered += 1;
                assert_eq!(
                    super::super::symbolic_line::important_discard(
                        &world,
                        HGroupProfile::Max,
                        CardId::new(14)
                    ),
                    (None, None)
                );
                assert!(
                    !super::super::candidate_pipeline::compile(&d, HGroupProfile::Max, &r)
                        .admitted
                        .iter()
                        .any(|c| c.action == clue)
                );
            } else {
                protected += 1;
                assert!(
                    super::super::candidate_pipeline::compile(&d, HGroupProfile::Max, &r)
                        .admitted
                        .iter()
                        .any(|c| c.action == clue),
                    "{assignments:?}"
                );
                assert_eq!(
                    super::super::select_h_group_action(&d, HGroupProfile::Max),
                    Some(clue),
                    "{assignments:?}"
                );
            }
            // The original observer's own decision view still hides every
            // assigned face. These are private branches, not public revelations.
            let (bob, _) = PerspectiveProjector::new(&world, HGroupProfile::Max)
                .project(PlayerId::new(1), PerspectiveDepth::NestedRecipients)
                .unwrap();
            assert!(bob.view().hands[1].iter().all(|c| c.identity.is_none()));
        }
        assert!(covered > 0 && protected > 0);
        assert_eq!(public, original);
        assert!(matches!(
            threatened_chop(&source, &public, HGroupProfile::Max, 1, &control).unwrap(),
            Split::Unresolved
        ));
        let mut unfunded = public.clone();
        unfunded.clue_tokens = 0;
        assert!(matches!(
            threatened_chop(&source, &unfunded, HGroupProfile::Max, 32, &control).unwrap(),
            Split::Unneeded
        ));
    }
    #[test]
    fn reviewed_root_projection_retains_private_protection_branches() {
        let (source, _) = reviewed_window();
        let root = Action::Clue {
            target: PlayerId::new(0),
            clue: Clue::Rank(Rank::Two),
        };
        let (_, evidence) = super::super::symbolic_line::project_leaf_projection(
            &source,
            HGroupProfile::Max,
            root,
            &AnalysisControl::default(),
        )
        .unwrap();
        assert!(!evidence.private_branches.is_empty(), "{evidence:#?}");
        assert_eq!(evidence.save_violations_at(12), 0);
        assert!(source.hands[1].iter().all(|c| c.identity.is_none()));
        for branch in &evidence.private_branches {
            let covered = branch
                .assignments
                .iter()
                .any(|(_, c)| *c == Card::new(Suit::Red, Rank::Three));
            if !covered {
                assert!(
                    !branch
                        .continuation
                        .steps
                        .iter()
                        .any(|s| s.projected.action == Action::Discard(CardId::new(14)))
                );
            }
            // A visible clued b3 can instead justify advancing b2; those
            // branches stop before the next unresolved source-hand action.
            // Without that extra successor, Cathy directly protects r3.
            let blue_successor = branch
                .assignments
                .iter()
                .any(|(_, c)| *c == Card::new(Suit::Blue, Rank::Three));
            if !covered && !blue_successor {
                assert!(branch.continuation.steps.iter().any(|s| s.projected.actor
                    == PlayerId::new(2)
                    && s.projected.action
                        == Action::Clue {
                            target: PlayerId::new(3),
                            clue: Clue::Suit(Suit::Red)
                        }));
            }
        }
    }
}
