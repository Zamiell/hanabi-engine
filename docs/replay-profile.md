# Expert replay profile: p4v0s2

Measured on 2026-09-16 in Ubuntu WSL, with the optimized test profile
(`opt-level = 2`, debug assertions retained). The precompiled
`third_expert_replay_matches_engine` test ran alone, with
`HANABI_PROFILE_REPLAY=1`. All 47 actions agreed. No search limits, assertions,
or convention behavior were changed.

## Results

Wall time: **235.43 seconds**. Per-turn measured work totaled 235.34 seconds.
The previous full-suite run reported 248.497 seconds for this test; that run had
concurrent tests and no instrumentation, so this is not an optimization speedup
comparison.

| Instrumented scope                           | Exclusive seconds |   Calls |
| -------------------------------------------- | ----------------: | ------: |
| History replay                               |            222.11 | 932,718 |
| Symbolic projection                          |              9.55 |     424 |
| Convention analysis outside the above scopes |              3.57 |      90 |
| Inverse planning outside nested scopes       |              0.08 | 585,152 |
| Planner outside nested scopes                |              0.02 |      47 |

History replay accounts for approximately **94%** of measured time. Its scope
includes memo-key construction, lookup, cloning, history reduction, and other
uninstrumented descendants. Calls include cache hits and distinct hypothetical
views: this measurement does not prove that all calls are redundant. Inverse
planning's call count includes early returns.

`run_exact_search` was never entered. Reducing exhaustive-search budgets would
not address this replay's bottleneck.

| Hanab Live turn | Seconds |
| --------------- | ------: |
| 37              |  56.256 |
| 44              |  22.224 |
| 39              |  18.232 |
| 20              |  14.378 |
| 40              |  12.852 |
| 33              |  11.982 |

Turn 37 includes 37.841 seconds of inverse planning (inclusive), 410,800
history-replay calls, and 192 symbolic-projection calls. These nested costs must
not be added together. Its history-replay exclusive time is 48.777 seconds.

## Code inspection and recommended next experiment

`with_replay_memo` in `h_group.rs` owns a memo for one recursive reduction and
clears it when that reduction ends.
`PerspectiveProjector::project_with_evidence` can separately reconstruct the
source observer and recipient. Symbolic line projection subsequently calls
`select_h_group_action`, which builds another analysis. Selection/execution
reuse already exists; simply proposing it again would repeat an earlier
refactor.

The next experiment should count exact-key cache hits/misses and repeated keys
across those boundaries, then test bounded request-scoped reuse of immutable
history reductions. Preserve all semantic key inputs: complete observation,
profile, perspective depth, empathy mode, and counterfactual stage. Audit any
other ambient reasoning guards before extending the lifetime. The existing
refactor history deliberately restricts memo lifetime; do not casually make it a
global or cross-turn cache.

Require enabled/disabled differential comparisons of complete inference and
projection evidence, not just selected moves, and measure peak memory as well as
time. If most reductions are genuinely distinct, profile the history reducer
internals next rather than adding a larger ineffective cache. No such cache
change is included in this profiling task.

## Validation handoff

After changing p4v0s1 turn 53 to green to Bob, all 54 moves of that replay
agree. Other existing replay regressions remain. The first replay in fixture
order, p4v0s415, disagrees at
[turn 30](https://hanab.live/shared-replay-json/415pagnsvvmkcjbqfqph-drurfleyguthflinwxdi-awmkscaoxpbuk,03ocka-eiepkcgdoaeoebtdeles-sbeeejxafaewwaereteh-exeneckaeqfme5saekfv-e1ffxaf0edegfuf4fBeC-1af7eIwaf3wae8fzwafA-ey,0#30):
fixture blue to Alice; engine purple to Donald. This separate disagreement is
not changed by the profiling work.

All admitted clues:

| Clue             | Engine interpretation | Preference score |
| ---------------- | --------------------- | ---------------: |
| Purple to Donald | Bluff                 |              517 |
| 3 to Donald      | Bluff                 |              514 |
| Blue to Alice    | Play Clue             |              269 |
| 1 to Cathy       | Trash Chop Move       |              100 |

The purple clue's score includes an 80-point protection bonus. Blue receives a
160-point teamwork penalty and a 4-point delay penalty. The engine projects one
strike for rank 3 but none for purple, blue, or rank 1. Purple's incomplete line
gains one point and two discard tokens; blue's gains two points including a five
refund. These are the engine's current assessments, not newly validated human
convention interpretations or proof that purple is superior.

Rejected clues: Alice green (`NoNewInformation`); Alice purple/4/5, Cathy
yellow/green/blue/3/4/5, and Donald yellow/green/1/2 (`NoConventionMeaning`).
Investigating this strategic discrepancy remains separate from profiling;
neither fixture expectations nor strategic logic were modified to hide it.
