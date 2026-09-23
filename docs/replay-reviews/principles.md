# Cross-position user rulings

The p4v0s1 positions below refer to the preserved
[historical fixture](../../crates/hanabi-search/src/h_group/tests/fixtures/game-p4v0s1-before-turn18-revision.json),
not the replacement continuation supplied on 2026-09-23. Their original review
anchors and regression assertions are retained.

These are human-reviewed principles, not additional engine configuration. Read
the linked position records for evidence and boundaries; do not introduce
fixture IDs or turn numbers into production decision logic.

## Save Principle: declined protection constrains discard risk

Source: [Save Principle](https://hanabi.github.io/beginner/save-principle/).
Reviewed examples: `p4v0s1.json`, live turns 33, 38, and 40 in this directory.

The user repeatedly ruled out unseen y3/g3 on these exposed chops because the
team would have protected unique playable cards. This is not limited to one
player, the immediately preceding turn, or a preceding ordinary play. Giving a
different clue—including a Save to another player—can also establish declined
protection. An available clue for the discarder does not itself invalidate that
evidence. Do not treat old implementation restrictions as exceptions to Save
Principle.

Evaluate the actual historical opportunity: the same card was exposed, the team
could protect it, and the recipient was not occupied by a promised response.
Retain valid historical evidence across later token exhaustion. Do not assume
that all unknown cards are safe, use their actual faces, or turn conditional
discard safety into literal knowledge or permission to blind-play.

Regression contract:

- `reviewed_save_principle_exclusions_apply_after_plays_and_clues` checks all
  three reviewed positions, unchanged literal knowledge, and the missing-history
  negative control. Its derived turn-42 case also covers a Save exposing a new
  chop: evaluate the recipient's post-clue response position, not just the
  pre-clue chop. Its turn-44 case covers a voluntary trash discard with a
  protection token already available. These are implementation regressions, not
  new user rulings; zero-token discards and useful-card transfers are not
  treated as equivalent evidence.
- `first_seed_donald_discard_does_not_invent_playable_three_risk` verifies that
  the turn-40 planner consumes the reduced risk domain, not the raw logical one.
- Existing turn-33/38 tests cover fresh non-chop cards, visible replacements,
  delayed evidence, and planner risk evaluation.
- `save_principle_does_not_infer_safe_chop_for_an_occupied_player` checks that
  queued responses do not turn into permissions to discard instead.

The former ordinary-play-only implementation was an incomplete implementation,
not a user-approved boundary on the convention.

## Funded protection does not erase earlier productive progress

Reviewed example: `p4v0s1.json`, live turn 28
(`t28-reviewed-purple-four-progress`).

The reviewed Donald-rooted p4 line plays p4, r2, p5, and r3 with no BDR through
its eight-action prefix. An alternative completing a Save earlier needs a real
benefit to outweigh that progress; a later snapshot must not forget when the
points were earned. Count a pending critical Save's funding obligation once,
including it in the compared clue bill. Do not turn the obligation into a
secured or playable card, ignore unfunded Saves, or trade away ready successors
and hand quality merely for earlier points. This is not authority to assume
Donald's unknown r4 in his own forecast or to freeze other observers' lines.

## Team Distribution: who draws when a clue is interchangeable

Source:
[Team Distribution Principle](https://hanabi.github.io/level-8/#team-distribution-principle).
Reviewed example: `p4v0s1.json`, live turn 40 (`t40-donald-draws-alice-saves`).

When either player can give the same clue before its recipient acts, and the
discard is safe, prefer the player with less known useful work to draw. This
applies to Save Clues too, not only Play Clues. Also compare conditional draw
schedules: drawing a predecessor into the hand already holding its successor can
delay the stack. Unknown draws remain unknown; this is a conditional comparison,
not a prediction of their identities.

An unfunded later part of the chain must not erase an earlier funded advantage.
Compare the same funded prefix on both sides. Preserve clue availability,
recipient deadlines, interpretation, and protection when handing off the clue.
