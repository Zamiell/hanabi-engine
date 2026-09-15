# Inference performance: September 15, 2026

## Method

Measurements used Ubuntu WSL as `james`, the optimized Cargo test profile, and
precompiled test binaries. Reported times exclude compilation. Timed tests ran
sequentially, without another test suite or build competing for CPU. These are
single-run measurements, not confidence intervals; small differences should not
be treated as established speedups.

The original isolated reviewed inverse-planning regression took **37.94 s**. Its
original all-prefix state-invariant check took **58.44 s**. The much larger
roughly 98-second all-prefix timings reported previously came from loaded,
parallel suite runs and are not the baseline for these isolated comparisons.

## Incremental experiments

| Change                                                               | Reviewed inverse-planning test |
| -------------------------------------------------------------------- | -----------------------------: |
| Original implementation                                              |                        37.94 s |
| Exact action/horizon projection reuse                                |                        38.72 s |
| Also share immutable proof witnesses                                 |                        38.24 s |
| Also reject already-incompatible action prefixes before reprojection |                        38.76 s |
| Also reuse the selected actor perspective during execution           |                        37.99 s |

These changes did **not** establish a material speedup on this position.
Immutable sharing avoids copying full witness payloads; projection reuse avoids
identical queries, but neither changes the costly complete-hand proof contract.
Do not describe these measurements as a large engine-speed improvement.

## Validation setup reuse

Before consolidation, the three architecture checks independently compiled the
same fixture/turn/observer positions. Sequential timings immediately before the
change were:

- State invariants: **56.77 s**.
- Canonical knowledge reconstruction: **55.64 s**.
- Focus-domain invariants: **56.06 s**.
- Total: **168.47 s**.

The consolidated test compiles each position once, then runs all three original
assertion groups. It still independently rebuilds knowledge, compares all
effects and card domains, checks causal transition bookkeeping, and checks that
focus restrictions cannot be undone. No fixtures, turns, observers, or
assertions were dropped. Test function count decreases by two because these
assertion groups now share setup, not because coverage was disabled.

The combined test passed in **56.25 s**, down from **168.47 s** for the three
separate passes: **112.22 seconds less**, or **66.6% less elapsed time** when
measured sequentially. This is a reduction in test work, not a promise that
parallel `check.sh` wall time falls by the same amount: expert move comparisons
can still dominate the suite's critical path.

## Accuracy safeguards

- Hand enumeration remains at 4,096 assignments, historical-view coverage at
  128, historical clue choices at eight, and projection horizons at 16.
- Identical historical queries still require complete present-hand coverage.
- Inverse-proof reuse has a cold-certificate-cache differential diagnostic
  comparing full domains, effects, assignment counts, and all witness contents.
  The initial run passed (38.36 s without reuse, 35.73 s with reuse); its run
  order and other warm caches mean this is principally an equivalence check, not
  a controlled speedup estimate.
- Symbolic projection retains the selected actor's immutable perspective for
  execution. A normal differential test compares the full plans at horizons
  zero, one, two, and sixteen with that reuse disabled/enabled.
- Full validations are sequential; a lock rejects simultaneous `check.sh`
  invocations. Standalone Cargo invocations are not covered by that lock.

No convention rules, evaluation preferences, replay fixtures, or expected moves
were changed. The existing fifth-replay exact-red-three assertion failure is
outside this optimization task; it must not be weakened to make validation
green.

## Validation results

`check.sh` passed formatting, compilation, and Clippy, then stopped after
**151.896 s** in Nextest at that pre-existing assertion (186 passed, one
failed). The **156 tests not reached by fail-fast** were selected explicitly and
all passed in **228.451 s**. Thus all 343 ordinary tests were exercised: **342
passed, one pre-existing failure**. The existing 28 excluded/expensive tests
remain excluded, plus the new explicitly runnable differential diagnostic.

All enabled expert action comparisons passed: complete `p4v0s1`, `p4v0s2`,
`p4v0s3`, and `p4v0s9`, plus the reviewed first 36 moves of `p4v0s415`. No
review boundary or fixture was changed. The unrelated manifest reordering
present before this task was left untouched.

These split fail-fast/completion runs are not a clean total-suite speed
benchmark. In particular, they must not be compared directly with an earlier
concurrent no-fail-fast run to claim a percentage improvement in `check.sh`.

The remaining `check.sh` stages passed when run afterward: rustdoc tests and
documentation, Python typing and compilation, all 21 Python tests, the bot CLI,
and Hawk (zero findings). The final full-proof differential diagnostic also
passed in **73.46 s** (37.61 s without inverse-projection reuse, 35.77 s with
it). Both comparisons retained identical full proof evidence. The normal
selected- perspective differential test passed in **1.14 s** in isolation.
