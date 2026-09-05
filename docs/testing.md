# Test provenance

Convention tests must not make an invented game authoritative merely because the
engine once played it successfully. A legal replay is not necessarily a
strategically valid replay, and a passing test is not evidence that its expected
convention interpretation was correct.

## Retained coverage

| Category                        | Evidence and permitted assertions                                                                                                                                                      |
| ------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Reviewed expert replays         | `game-p4v0s415`: action parity through move 35; `game-p4v0s9`, `game-p4v0s2`, `game-p4v0s3`, and `game-p4v0s1`: full action parity. All retain focused, reviewed interpretation tests. |
| Replay-based hypotheses         | Alternative clues and hidden-world branches from identified reviewed positions; these are comparisons, not permission to rewrite the fixture.                                          |
| Superpositions and architecture | Reviewed snapshot expectations; incremental/replayed equivalence, perspective isolation, causal bookkeeping, and consistency invariants.                                               |
| Recorded self-play failures     | Specific established-rule failures and legality/consistency checks. Earlier moves and whole-game strategy are not certified.                                                           |
| Ordinary unit tests             | Game rules, seed generation, codecs, exact-search algorithms, identity sets, focus ordering, and data structures may use artificial inputs with independently defined expectations.    |
| Completion smoke tests          | Generated games check legal actions and bounded completion across profiles; they do not prescribe moves, identities, or a target score.                                                |

The expensive `h_group_max_self_play_200` benchmark remains separate from
`check.sh`. Its scores measure performance, not convention correctness.

## Pending continuation review: p4v0s415

Move 35 was changed to the user-approved rank-4 clue to Alice. Moves 36–49 were
generated using H-Group Max, the perfect-score objective, and default planner
limits; each decision received only the acting player's view. This continuation
finishes at 25 with no strikes, but is **not yet reviewed**. The active
fixture's suffix is checked for legality, not optimal-action parity.

`crates/hanabi-search/src/h_group/tests/fixtures/game-p4v0s415-reviewed-branch.json`
preserves the previous rank-3 branch for existing position-specific convention
tests and its reviewed superposition snapshot. These historical interpretations
remain useful even though rank 3 is no longer the preferred move at turn 35. The
`reviewed_rank_three_branch_p4v0s415` test helper explicitly loads that
historical branch; the active action-parity test explicitly loads the protocol
fixture.

## Synthetic corpus retirement (2026-09-05)

Removed 93 tests from the old H-Group scenario corpus:

- 66 tests using hard-coded `paired_sample_*` deals and scripted continuations.
- 26 tests using `state_with_prefix` to construct convention situations.
- One additional hand-written `PlayerView` with a fabricated clue history.

The unused deck builders and observation helper were removed too. This is in
addition to the two duplicate-rank-one tests removed previously. The deleted
tests and their setup remain recoverable in Git history; do not restore their
expectations without reviewed replay evidence.

No production convention or strategy code was changed by this retirement. The
five expert replay fixtures and the superposition golden file were not changed.
The retained completion/profile smoke tests assert only successful execution,
not correctness of a convention line.

## Self-play assertion audit

- `p4v0s10`, turn 4: preserve the pending play as an admissible action; do not
  assert that it beats every possible clue.
- `p4v0s10`, turn 23: retain the user's specific purple-Fix decision and the
  comparison against rank 3. This does not certify the whole preceding game or
  every older note in the engine's belief history.
- `p4v0s15`, turn 9: require that the 5 Save is available and interpreted as a
  Save, not that it is the unique best action.
- `p4v0s20`, turn 11: retain the user-reviewed immediate Finesse despite an
  off-position visible copy. The earlier recording is not validated strategy.
- `p4v0s15`, turn 48: retain the user-reviewed Good Touch exclusion used by the
  Gentleman's Discard interpretation.
- `p4v0s14`, turn 17: replace unreviewed exact Hard-3 identity expectations with
  a nonempty-belief invariant.
- Other self-play checks retain narrow duplication, impossible-belief,
  causality, or legal-action checks. The ignored blue-Clarity reproduction is
  still an unresolved diagnostic, not a passing convention oracle.

## Coverage gaps and adding tests

Retiring the synthetic corpus reduces dedicated coverage of saves, layered
connections, emergency discards, advanced clues, and strategic preferences. The
implementation inventory in `h-group-coverage.md` must not be interpreted as
validated behavioral coverage of all those cases.

Previously reviewed rules still apply even when their synthetic reproduction was
removed. In particular, the rank-2 direct-play/delayed-play/2-Save superposition
must not be collapsed into a mandatory Self-Prompt. The removed three-way
synthetic example has no newly validated replacement in this cleanup; add one
when an appropriate reviewed replay position is available. Do not substitute a
different two-way example and claim equivalent coverage.

For a new convention assertion, record the replay fixture, one-based Hanab Live
turn, observer, and the reviewed reason. Use the replay-link generator when
requesting human review. If no reviewed position establishes an expected move or
identity, retain it as a question rather than inventing an answer to make the
suite pass.
