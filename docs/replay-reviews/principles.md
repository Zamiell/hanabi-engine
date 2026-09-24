# Cross-position user rulings

Except for the current turn-18 teammate-clue, turn-19 clue-efficiency, and
turn-26 discard-safety, turn-27 duplicate-response, and turn-33
minimum-clue-value rulings, the p4v0s1 positions below refer to the preserved
[historical fixture](../../crates/hanabi-search/src/h_group/tests/fixtures/game-p4v0s1-before-turn18-revision.json),
not the replacement continuation supplied on 2026-09-23. Their original review
anchors and regression assertions are retained.

These are human-reviewed principles, not additional engine configuration. Read
the linked position records for evidence and boundaries; do not introduce
fixture IDs or turn numbers into production decision logic.

## Play and leave a productive clue to a teammate

Current fixture `p4v0s1`, live turn 18, record
`t18-play-and-leave-the-clue-to-cathy`: the user explains that a player with a
card to play should usually play it and let a teammate give the clue. Bob should
play b5 instead of taking Cathy's clue-giving turn. His projection already has
Cathy clue 4s to Alice to bluff Donald's r1.

This is a scheduling preference, not an absolute prohibition on cluing while
holding a play. Evidence of a better outcome, urgent obligations, and safety
still take precedence. Do not require the teammate to give the identical clue,
assume unseen cards or draws, or treat an unfinished forecast as successful. The
current implementation credits a certain play followed by a funded teammate clue
and an explicit immediate scoring response before the original player acts
again. Longer or uncertain handoffs remain outside this conservative rule.

Regression: `reviewed_turn_eighteen_leaves_the_clue_to_a_teammate` verifies the
bluff projection, scheduling comparison and chosen action, with
evidence-ablation and urgent-policy controls. Historical
`reviewed_turn_thirty_compares_equal_elapsed_time` retains a reviewed case where
endpoint evidence favors a clue over b5.

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
could protect it, and the recipient was not occupied by a promised response. The
current turn-26 ruling also covers earlier teammates, not only the player seated
immediately before the discarder: Alice's zero-token discard does not erase
Donald's earlier opportunity to protect Bob. See
`t26-team-save-principle-removes-all-discard-bdr` for the user's per-identity
reasoning. Retain valid historical evidence across later token exhaustion. Do
not assume that all unknown cards are safe, use their actual faces, or turn
conditional discard safety into literal knowledge or permission to blind-play.

Regression contract:

- `reviewed_turn_twenty_six_discard_has_no_bottom_deck_risk` covers the current
  turn26 ruling, unchanged literal knowledge, missing-history control, and the
  safe reveal branches' zero losses and guaranteed token refund.

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

## Clue efficiency counts newly obtained cards

Current `p4v0s1`, live turn 19, record
`t19-two-bluffs-have-higher-clue-efficiency`: the user compares two 2-for-1
clues (4s to Alice, then purple to Donald) against a 2-for-1 and a 1-for-1
(purple to Bob, then green to Bob). Already-clued p3 and g5 do not become new
cards obtained by the later clue merely because it clarifies or releases them.
Keep their timing and continuation benefits separate from clue efficiency.

A Bluff through an already-clued intermediate can identify that intermediate
using literal information from the same clue. Donald's rank-clued p3 becomes
literally purple 3 when the purple clue touches it; overlooking that evidence
must not erase the newly secured p4. Do not use later clues or speculative
same-clue identity promises to establish the intermediate.

Regressions: `reviewed_turn_nineteen_counts_new_cards_not_old_connectors` checks
all four clue counts; `reviewed_two_bluffs_secure_both_fours` checks Donald's
inference and the endpoint's secured cards, with a historical cutoff control and
future draws hidden.

At equal realized points and clue resources, an additional secured future card
is a concrete efficiency gain; an unclued visible successor must not by itself
veto that gain. Readiness of already secured cards is a tempo benefit, not
additional clue efficiency. Retain guards for realized points, clue costs,
future clue demand and safety.
`reviewed_turn_nineteen_prefers_two_two_for_one_clues` checks the actual planner
choice and endpoint counts, with controls for lost points, extra clue cost, lost
committed plays, exposed critical cards and equal secured coverage.

## Minimum clue value is an admission requirement

Current `p4v0s1` turn 33 in the turn-28 purple-to-Alice projection, record
`t33-green-bluff-zero-minimum-clue-value`: the user rules that green to Cathy is
an illegal 0-for-1 under
[MCVP](https://hanabi.github.io/beginner/minimum-clue-value-principle/). Cathy's
g4 is already clued; bluffing Bob's p3 merely replaces Donald's secured p3.
Neither a new physical clue fact nor playing a different copy earns new card
value. This is not a preference to resolve with a heuristic penalty.

Count newly secured useful identities, including indirect plays and protection.
Do not count previously secured cards again or infer duplication from unknown
identities. Preserve the documented Fix, valuable Tempo, chop-move and forced
Stall exceptions; do not prohibit all clues on already-clued cards.

Regressions: `reviewed_green_bluff_requires_minimum_clue_value` checks
rejection, legitimate turn19/21 bluffs, and unknown-identity controls;
`reviewed_turn_nineteen_counts_new_cards_not_old_connectors` checks zero value
for this clue alongside the previously reviewed productive lines.

The current turn-27 review (`t27-indirect-red-two-duplicates-givers-known-play`)
also checks the giver's own known card. A hidden physical face is not an unknown
identity when the giver already knows it from prior clues. Use only that giver's
pre-clue evidence; unresolved domains and other observers' hypotheses do not
prove duplication. Donald's r2 does not obtain another useful identity when
Cathy already knows she holds the secured r2.

`reviewed_turn_twenty_seven_counts_givers_known_duplicate` checks the corrected
acquisition count and removes the giver's identity evidence as a negative
control. The existing turn33 green Bluff regression covers the parallel
visible-copy case. The turn27 ledger separately records an unresolved question
about the extra Ignition: the clue also Trash Pushes Alice's p2, so correcting
its false extra r2 credit is not a ruling that the entire clue has zero value or
that every indirect duplicate is prohibited.
