# Charm giver exclusion — September 10, 2026

The user's correction resolves the [previous question](self-play-2026-09-08.md):
Alice cannot intend connections through unknown cards in her own hand. Another
player seeing those cards does not make them valid connectors for her clue.

The shared 4 Charm counter now receives the actual clue giver explicitly and
excludes their hand. Candidate generation, history recognition, demonstrated
Charms, and projection dependency checks all carry that same giver. The ordinary
connection search already excluded the giver; this fixes the separate Charm path
rather than introducing a strategic exemption.

The reviewed p4v0s28 opening regression checks both Alice's and Bob's Charm
recognition and Bob's fourth-position b1 play. Two older tests that explicitly
expected the erroneous hidden-giver uncertainty were corrected. Existing tests
still check legitimate connectors in other players' hands.

[The fresh p4v0s28 recording at turn 2](https://hanab.live/shared-replay-json/415uqfppgbcqftogikrf-psnhwdhaakxnlecsmuuk-valxmbvrwijyd,03xdee-kdscodeqeieosdscwaep-eatdesemfbgaekeueyld-fjen1cffode3fclcfzgb-fdegtae5f1xde7etf6pa-f2f0eBfrxaxcev,0#2)
shows Bob playing b1. This is not a validated expert game: the rerun completed
49 moves in 92.38 seconds before a contradictory-belief error at turn 50. The
diagnostic report is `target/self-play/charm-september10.json`. It is not a
passing baseline, and the later error is still outstanding.

## Next strategic disagreement

[p4v0s1, turn 1](https://hanab.live/shared-replay-json/415ifirpxqufunsxcwgc-tbokayavlbjdgwqvekus-pdfanhhmkrpml,03tdeh-sbxckceeeisbkdegfjep-edgcelevwbereseyocwa-eueofaefobfmew1a1den-fbeze2xae7fqfxf3f4f5-f0lceGodeIeFecetfEe1-,0#1):
fixture 3s to Donald; engine 4s to Cathy. The fixture remains unchanged.

- **3s to Donald:** a 3 Bluff, followed by Bob playing p1 (#7). It protects
  Donald's r3 and p3. The projection then stops at Cathy's turn because her next
  action depends on Alice's unknown hand.
- **4s to Cathy:** a 4 Charm, followed by Bob playing b1 (#4). It protects
  Cathy's g4 and b4. The projection likewise stops at Cathy's turn, not at a
  promised immediate play of either 4.

Both recorded endpoints have one point, seven tokens, two blocked clued cards,
and two secured future identities. The existing endpoint evaluator credits the
Charm with three protected bottom-deck-risk identities (b1, g4, b4), versus two
for the Bluff (r3, p3); two p1 copies are visible, so p1 is not counted as a
single-visible-copy risk. It also credits a visible b2 successor after b1,
whereas p2 is not visible to Alice. Thus it prunes the Bluff as
endpoint-dominated despite the Bluff's higher action-priority score. The
possible p2 in Alice's unknown hand is recorded as a conditional opportunity,
not a known successor.

This is a strategic comparison, not evidence that the reviewed Bluff is wrong.
The blocked-card count is equal but does not distinguish how much work remains
before a 3 versus a 4 can play. The existing deadline preference for the Bluff
is in its score, which endpoint pruning overrides. No new weight, exemption, or
fixture change has been added to force either result.

**Review question:** should b1 plus protecting g4/b4 beat p1 plus protecting
r3/p3 here, or should the time-sensitive Bluff and the lower-rank promises take
precedence? If the latter, which concrete continuation/resource difference
should prevent the claimed endpoint dominance?

All admitted candidates and their heuristic priorities (not endpoint rankings):

| Clue             | Priority | Interpretation |
| ---------------- | -------: | -------------- |
| 3s to Donald     |      596 | 3 Bluff        |
| 4s to Cathy      |      500 | 4 Charm        |
| Blue to Bob      |      475 | Play Clue      |
| Purple to Bob    |      435 | Play Clue      |
| 1s to Bob        |      434 | Play Clue      |
| 1s to Cathy      |      432 | Play Clue      |
| Yellow to Cathy  |      431 | Play Clue      |
| Green to Cathy   |      421 | 4 Charm        |
| Blue to Cathy    |      421 | 4 Charm        |
| Purple to Donald |      421 | 4 Charm        |
| 4s to Donald     |      340 | 4 Charm        |

Rejected as `NoConventionMeaning`: 2s/4s to Bob; purple to Cathy; red/yellow/2s
to Donald. Empty clues are not legal candidates.

## Validation

The new recorded-position test and all eight Charm-focused library tests pass.
`scripts/check.sh` passes ownership, formatting, build, and Clippy, then stops
at the new fifth-replay opening disagreement. The non-fail-fast Rust run
finishes in 226.57 seconds: 329 passed, 4 failed, 28 skipped. Three failures are
the pre-existing inverse-planning regressions; the fourth is the opening
comparison above. No expert fixture or expected move was changed. Separately run
rustdoc, documentation build, Python typing, all 21 Python tests, and
dead-public-code checks pass.
