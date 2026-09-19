# Cross-position user rulings

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
  negative control.
- `first_seed_donald_discard_does_not_invent_playable_three_risk` verifies that
  the turn-40 planner consumes the reduced risk domain, not the raw logical one.
- Existing turn-33/38 tests cover fresh non-chop cards, visible replacements,
  delayed evidence, and planner risk evaluation.

The former ordinary-play-only implementation was an incomplete implementation,
not a user-approved boundary on the convention.
