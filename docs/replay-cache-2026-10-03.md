# Bounded replay cache eviction experiment

The previous request cache cleared all 1,024 entries when it filled. The new
policy divides that same entry allowance into two generations of 512 entries. A
hit in the older generation promotes the entry. When the current generation
fills, only the remaining older entries are discarded; the current generation
becomes the older one.

The exact observation/profile/perspective/empathy/inverse-planning keys and
owned return values are unchanged. Entries remain request-local. This does not
remove cloning or change convention admission, scoring, or search budgets.

## Fixed-work benchmark

Both policies are compiled into the same optimized test binary. The original
clear-all policy is available only in tests. Each run clears inverse-planning
caches, starts a fresh request memo, analyzes the same observation, and projects
every admitted candidate to the same four-action horizon. Fixture decoding and
position reconstruction are outside the measured interval. Profiling is off.
There are three measurements per policy/position, alternating execution order.
No builds, broad tests, or other benchmarks run concurrently.

Inputs are the existing p4v0s9 fixture at one-based turns 20, 31, 32, and 41.
Replay entry point:
[p4v0s9, turn 31](https://hanab.live/replay-json/415bpilhckfaxutkacql-wuvnouykigfdemdmfrhr-sqppwjbnvxgas,03tbeh-ldenwcpaekemedxcsaet-ebsdlaereafgejkafwwd-e0exxcefe2epfyeefixa-1ce8fsscece4eBfoeu1d-elevf11akae3eG,0,p4v0s9#31).
The benchmark asserts equality of the complete convention analyses and all
projection outputs after each pair; it does not assert that an old engine move
is strategically optimal. All twelve paired comparisons passed.

| Turn | Candidates | Clear-all median | Generations median | Time reduction |
| ---: | ---------: | ---------------: | -----------------: | -------------: |
|   20 |          6 |          3.416 s |            3.322 s |           2.8% |
|   31 |          7 |         12.699 s |           11.628 s |           8.4% |
|   32 |          8 |         18.765 s |           17.401 s |           7.3% |
|   41 |          5 |          7.829 s |            7.240 s |           7.5% |

The sum of per-position medians falls from 42.709 to 39.590 seconds: **7.3% less
computation time** for this workload. This is a modest local measurement, not a
claim that every move is faster or that the two-minute cap is eliminated. It
measures engine work, not compilation or reduced test setup. The optimized test
profile retains debug assertions; these are not release-build timings.

Reproduce after building once (use the test executable path printed by Cargo):

```bash
cargo test --locked -p hanabi-search --lib --no-run
target/debug/deps/hanabi_search-<hash> \
  h_group::tests::replay_memo::benchmark_reviewed_replay_cache \
  --exact --ignored --nocapture
```

`HANABI_CACHE_BENCH_TURNS`, `HANABI_CACHE_BENCH_HORIZON`, and
`HANABI_CACHE_BENCH_REPEATS` select positions, depth, and repetitions. Optional
`HANABI_PROFILE_REPLAY=1` records cache counters separately from uninstrumented
timing measurements. `benchmark_reviewed_move_cache` measures normal complete
move calculations with the normal two-minute budget. It reports completed
candidate counts, budget exhaustion, and action equality; it only asserts full
analysis equality when both calculations finish without exhausting the budget.

Raw local records are under ignored `target/replay-audit/cache-fixed-repeat.txt`
and the workflow command record `e69907a974e1454da143de070ef4b4c1`.

A separate instrumented pass of turn 32's fixed workload recorded 32,253 misses
with clear-all versus 28,373 with generations (12.0% fewer). Clear-all flushed
31 times. Generations rotated 75 times, evicting 27,632 older entries while
retaining the current generation; peak live entries were 1,015, within the
unchanged 1,024-entry bound. Full output equality passed again. These counters
are in `target/replay-audit/cache-counters.txt`, workflow record
`0dba17b06ca74a51bbb29dd1547572da`; its instrumented times are not included in
the uninstrumented medians above.

## Correctness checks

The separate uninstrumented full-move benchmark used the normal perfect-score
planner configuration and 120-second cap, with cold caches for each position:

| Turn | Clear-all | Generations | Complete candidates |
| ---: | --------: | ----------: | ------------------: |
|   20 |  13.061 s |    12.191 s |                 6/6 |
|   31 |  89.342 s |    85.332 s |                 7/7 |
|   41 |  33.873 s |    31.779 s |                 5/5 |

All six calculations completed without exhausting the budget. Complete
`PositionAnalysis` equality passed for all three pairs, including selected
actions and comparison evidence. These are single paired measurements, showing
4.5–6.7% less elapsed time. Raw output is
`target/replay-audit/cache-full-move.txt`, workflow record
`8684ed278fa64da8bbbcf3508e0f772e`.

The earlier continuous replay scan was instrumented and had a different cache
history. Its 120-second observations cannot be compared directly with these
isolated cold-cache timings, or used to attribute that entire difference to this
optimization. This experiment has not demonstrated elimination of those caps.

The focused cache suite passed all five ordinary tests. Coverage includes hot
entry promotion, eviction, replacement, small/odd capacity bounds, nested
request ownership, cancellation and unwind cleanup, key isolation, and the
existing cold-cache comparison of complete reviewed analyses/effects/proofs. The
original assertions remain intact.

One full `scripts/check.sh` completed: **464 passed, 51 failed, 32 skipped**.
Every non-test stage passed, including formatting, build, Clippy, documentation,
Python, and dead-code checks. The full p4v0s9 comparison passed all 49 moves;
this is agreement under the normal bounded planner, not an exhaustive optimality
proof. The raw log is `target/replay-audit/cache-full-check.txt`, workflow
record `0473534c522246679fb6f0961c1d7123`.

Of the 51 failures, 45 names were already in the recorded full-run baseline at
`82bd9a5846daa5ab0c4c742fbe72e94787430e75`. All six additional failures
reproduced with `HANABI_REPLAY_MEMO=clear`, using the original eviction policy:

- `hanabi-cli::replay_link::link_round_trips_every_expert_replay_deck_and_action`
- `hanabi-cli::replay_link::seed_replay_link_matches_hanab_live_codec_and_turn_number`
- `h_group::tests::move_35_uses_the_oldest_matching_elimination_note`
- `h_group::tests::replay::demonstrated_layer_pause_is_not_vetoed_by_generic_play_order`
- `h_group::tests::replay::third_replay_opening_rank_two_makes_a_later_green_clue_duplicate`
- `planner::tests::reviewed_two_for_one_beats_a_speculative_finesse_tiebreaker`

The two CLI tests still expect shared replay URLs after the earlier solo-link
change. These failures were not silently accepted into a new baseline or fixed
as part of this cache optimization. The old-policy rerun log is
`target/replay-audit/cache-old-policy-failures.txt`, workflow record
`4f30d8c140be47cc90d538a3c0cbfe84`. Full validation therefore remains
**failing**; the optimization has not established a clean repository-wide pass.

The separate ignored 47-position recursive-scope differential was not rerun.
This change was checked with the existing ordinary differential, the twelve
fixed-work comparisons, the three full-move pairs, the profiled comparison, and
the full ordinary suite. No test assertions were weakened or removed.
