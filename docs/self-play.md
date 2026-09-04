# H-Group max self-play benchmark

`h_group_max_self_play_200` plays seeds `p4v0s1` through `p4v0s200`, with
four H-Group max players and the normal perfect-score planner budgets
(4,096 worlds and 50,000 exact nodes). Each decision receives only the acting
player's legal view. The seed and hidden deck remain with the simulator.

Run from WSL after establishing the baseline:

```sh
scripts/check-self-play.sh
```

This opt-in test uses a release build and four independent game workers. It is
excluded from the routine `scripts/check.sh` test run, but is built and linted
there. A manually dispatched GitHub Actions workflow runs the complete benchmark
and uploads its report even on failure.

The baseline will live in
`crates/hanabi-search/tests/fixtures/h_group_max_self_play_200.json`.
It has not yet been established. Broad diagnostic runs have exposed convention
contradictions, and some exact endgames remain slow. Local partial checkpoints
are diagnostic measurements, not a passing 200-game baseline.
Use `HANABI_SELF_PLAY_UPDATE=1 scripts/check-self-play.sh` for the first complete,
error-free measurement. Convention failures still need investigation before
this can become a passing strength gate.
The test fails if total score or perfect-game count decreases. Any engine error
also fails the run, including contradictory beliefs, no candidate actions,
illegal actions, panics, or failure to finish within 200 turns. Three strikes
score zero according to the game rules. Aborted games receive zero credit and
are explicitly counted as errors, not as completed games. A baseline with
nonzero `engineErrors` is provisional and does not make the test pass.

Reports go to `target/self-play/report.json`, with per-seed scores, turns,
strikes, errors, runtimes, complete action histories, and differences from the
baseline. `report.ndjson` is flushed after each game so completed results survive
an interruption. `report.active/<seed>.json` is also rewritten before each
decision; these seed-and-action replays reproduce a stalled or failing position
without waiting for that game to finish. Custom report names use the equivalent
`<name>.active` directory. Results are sorted by seed before comparison; worker completion
order cannot affect decisions. Action histories are cheap to retain and allow
reconstruction of the position before an error. Detailed rejected-clue diagnostics
can then be obtained by analyzing that replay position.

## Configuration and measurements

```sh
# Choose concurrency (does not change per-move budgets).
HANABI_SELF_PLAY_WORKERS=8 scripts/check-self-play.sh

# Small measurement run; does not compare or update the 200-game baseline.
HANABI_SELF_PLAY_GAMES=4 HANABI_SELF_PLAY_WORKERS=1 \
  HANABI_SELF_PLAY_REPORT=target/self-play/pilot.json scripts/check-self-play.sh

# Reproduce a particular seed.
HANABI_SELF_PLAY_START=14 HANABI_SELF_PLAY_GAMES=1 scripts/check-self-play.sh

# Explicit baseline update, after reviewing changes. Requires all 200 seeds.
HANABI_SELF_PLAY_UPDATE=1 scripts/check-self-play.sh
```

Baseline updates fail before replacing the baseline if engine errors occurred;
the diagnostic report still retains their count and action histories. Normal
checks never replace the baseline automatically. Review changed seeds even if aggregate performance
improves. This benchmark supplements expert convention tests; shared convention
mistakes can remain hidden when all four players use the same engine.

The benchmark calls the normal `plan_move` API once per turn and advances its
simulator in place. It does not rerun convention inference to prepare failure
messages at every position. Search budgets and all decision-relevant reasoning
are the same as normal play.

The planner validates beliefs but skips exhaustive continuation search when
there is only one admissible candidate. With no alternative, that search cannot
change the move. Immediate terminal proofs are still retained. Such positions
may report a symbolic result with no exact outcome statistics. On seed 4 this
reduced game time from 178.97 to 99.83 seconds with all 58 actions unchanged.

The expert replay assertion now computes additional failure diagnostics only
when the selected move mismatches. On the development machine, the fifth replay
test decreased from 31.07 to 17.93 seconds (optimized test profile). The separate
release-mode equivalence audit measured 16.025 seconds with full diagnostics and
16.854 seconds without them under concurrent load, so omitting rejection
explanations did not demonstrate an additional speedup. Omitting them in exact
descendants likewise took 196.64 seconds on seed 4 versus 178.97 seconds in the
pilot, with identical action histories. These bypasses were removed; the engine
API remains unchanged. The pilot's action histories also
matched with four versus eight game workers. Parallel games improve throughput,
but a single slow endgame can dominate elapsed time.

Exact search now orders the convention-preferred action first and stops when a
candidate reaches a proved outcome upper bound. In the final round, that bound
uses an optimistic full-information, free-pass continuation; it is only a
pruning bound, never a player policy. Skipped root alternatives have no measured
outcome rather than a fabricated value. Prospective transitions also stop at
the same terminal point as the simulator, including final-round countdowns and
the absence of a draw after a terminal action.

## Self-play bug reproductions

`crates/hanabi-search/tests/self_play_regressions.rs` checks focused invariants
from failed self-play seeds. These are not expert optimal-move fixtures: an
earlier engine move may itself be wrong. The ignored blue-Clarity diagnostic
explicitly records one unresolved giver/recipient disagreement; it is not part
of the passing correctness gate. The full 200-game benchmark still fails on
that contradiction and all other engine errors.
