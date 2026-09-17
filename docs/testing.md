# Test provenance

Seeded replay fixtures may omit `players`: the canonical seed supplies the
player count and the loader uses Alice, Bob, Cathy, Donald, and Emily in order.
Explicit names are preserved. Replays without a seed must supply `players`.
Generated Hanab Live replay links are unchanged by omitting default names.

Convention tests must not make an invented game authoritative merely because the
engine once played it successfully. A legal replay is not necessarily a
strategically valid replay, and a passing test is not evidence that its expected
convention interpretation was correct.

## Retained coverage

The all-prefix architecture test compiles each fixture/turn/observer once and
runs state validation, independent knowledge rebuilding, causal-effect checks,
and focus-domain checks against that same immutable result. These are separate
assertions, not three independent repetitions of the expensive inference pass.
The independent knowledge rebuild remains intentional and must not be cached
away: it verifies the compiler result.

Run full validations sequentially. `check.sh` rejects another simultaneous
`check.sh` invocation using a repository-local lock. Avoid running standalone
benchmarks or other Cargo test suites concurrently with it; the lock does not
govern arbitrary Cargo invocations.

`check.sh` runs all ordinary tests without fail-fast and continues to the other
validation stages after failures. It reports each stage's elapsed seconds and a
total on success or failure, and exits nonzero if any stage failed. Missing
prerequisites still stop the run immediately. Use focused tests while editing,
then one full validation at task completion; do not repeat completed checks just
because an unrelated regression remains failing.

For per-turn, nested stage timings of an expert action-parity test:

```bash
HANABI_PROFILE_REPLAY=1 cargo test --locked -p hanabi-search --lib \
  h_group::tests::replay::third_expert_replay_matches_engine -- --exact --nocapture
```

`REPLAY_PROFILE` lines are tab-separated: seed, one-based turn, stage, calls,
inclusive seconds, exclusive seconds (preceded by the marker). Inclusive times
overlap; exclusive times subtract nested instrumented scopes. The `total` row is
a separate wall-clock measurement, not an additional stage to sum. Compile
before measuring if comparing execution times. Instrumentation is test-only,
disabled unless requested, and does not alter planner limits or assertions. See
[the replay profile report](replay-profile.md) for the measured bottleneck.

`HANABI_REPLAY_MEMO=recursive` selects the previous recursive-only cache scope
in test binaries, for before/after measurements. It has no effect on production
builds. With profiling enabled, `REPLAY_MEMO` rows report hits, misses, repeated
exact-key misses, untracked misses, peak entries, and capacity flushes. The
diagnostic exact-key tracker retains at most 8,192 keys per turn; nonzero
untracked counts mean repeated-miss counts are lower bounds, not exhaustive.

When changing replay memoization, also run the full-output differential check:

```bash
cargo test --locked -p hanabi-search --lib \
  h_group::tests::replay_memo::request_memo_preserves_full_replay_analyses \
  -- --exact --ignored --nocapture
```

This deliberately expensive check runs all 47 positions of p4v0s2 with the old
and new cache scopes, clearing inverse-planning certificate caches before each
pass. It compares complete `PositionAnalysis` values, knowledge effects, and
strategic deductions including their per-world witnesses. Lightweight ordinary
tests cover representative positions, key isolation, eviction, nesting,
cancellation, and unwinding. None replace the existing replay assertions.

For changes to inverse-planning projection reuse, additionally run the
cold-cache differential certificate check:

```bash
cargo test -p hanabi-search --lib projection_cache_preserves_complete_reviewed_proof -- --ignored --nocapture
```

It compares all inferred card domains, knowledge effects, assignment counts, and
full per-world witnesses with reuse disabled/enabled, rather than merely
checking the selected move. This deliberately expensive diagnostic is separate
from the ordinary suite; the reviewed inverse-planning regression still runs
normally. Perspective selection/execution reuse also has an ordinary
differential test covering several projection horizons.

| Category                        | Evidence and permitted assertions                                                                                                                                                         |
| ------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Reviewed expert replays         | `game-p4v0s415`: approved actions through move 36; `game-p4v0s9`, `game-p4v0s2`, `game-p4v0s3`, and `game-p4v0s1`: full action parity. All retain focused, reviewed interpretation tests. |
| Replay-based hypotheses         | Alternative clues and hidden-world branches from identified reviewed positions; these are comparisons, not permission to rewrite the fixture.                                             |
| Superpositions and architecture | Reviewed snapshot expectations; incremental/replayed equivalence, perspective isolation, causal bookkeeping, and consistency invariants.                                                  |
| Recorded self-play failures     | Specific established-rule failures and legality/consistency checks. Earlier moves and whole-game strategy are not certified.                                                              |
| Ordinary unit tests             | Game rules, seed generation, codecs, exact-search algorithms, identity sets, focus ordering, and data structures may use artificial inputs with independently defined expectations.       |
| Completion smoke tests          | Generated games check legal actions and bounded completion across profiles; they do not prescribe moves, identities, or a target score.                                                   |

The expensive `h_group_max_self_play_200` benchmark remains separate from
`check.sh`. Its scores measure performance, not convention correctness.

`crates/hanabi-protocol/tests/fixtures/expert-manifest.json` is the
action-parity contract: it records each seed's reviewed boundary, continuation
provenance, convention profile, objective, and checks. The comparison helper
consumes those values and prints the actual reviewed/total move counts. A
catalog test requires exactly one entry per expert fixture. A generated
continuation is never silently promoted to reviewed strategy by a passing
legality check.

Infrastructure contracts additionally cover cancellation without partial
decisions, lazy constraint traversal, shared snapshot identity, versioned CLI
handshakes, and atomic stale-result rejection in the live bridge. These are
algorithm/protocol assertions, not new invented convention examples. CI includes
a Rust 1.85 workspace build, independently of stable-toolchain linting.

## Pending continuation review: p4v0s415

Move 35 is the user-approved purple clue to Alice; move 36 is the approved
discard of Donald's purple 1 (#26). Moves 1–39 were preserved while moves 40–53
were regenerated using H-Group Max, the perfect-score objective, and default
planner limits. Every decision receives only the acting player's view. The
revised game is legal and finishes with 24 points and no strikes; purple 5
remains unplayed. Moves 37 onward are **not yet reviewed as optimal** (apart
from the separately reviewed turn-43 5s clue). The full continuation is checked
for legality, while action parity remains limited to the approved 36-move
prefix.

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
