# H-Group Convention Interpreter

The engine implements the pinned H-Group learning path as a cumulative public-history interpreter. `h-group:N` enables Levels 1 through N; `h-group:max` is treated internally as the effective 26th cumulative level. The source revision is exposed as `H_GROUP_RULESET_REVISION`.

Named H-Group moves are compositions of a smaller set of typed state effects. The interpreter records an append-only `HGroupSignal` explanation log, maintains separate indexed `ConventionFacts` for current truth, and routes Prompt/Finesse changes through one lifecycle manager. For example, a Trash Push Finesse combines `TrashPush` and `Finesse`; it does not need a second implementation of finesse resolution.

| Level | Documentation topic | Engine effect |
| ---: | --- | --- |
| 1 | Basic conventions | Play/Save focus, Good Touch, Prompt, Finesse |
| 2 | Basic moves | 5 Stall; multiple, Reverse, and Self-Finesse connections |
| 3 | Basic strategy | 1 ordering, Fix Clue, Sarcastic Discard, information-lock-compatible notes |
| 4 | Chop moves | trash, 5, and order chop movement; invisible chop status |
| 5 | Special finesses | hidden/layered/queued/ambiguous connection disjunctions |
| 6 | Tempo clues | valuable tempo, stall tempo, TCCM, focus shifting |
| 7 | Emergency discards | Scream/Shout chop movement and must-clue obligations |
| 8 | End-game | pace phase, no chop moves, positional play signals |
| 9 | Stalling | severity-gated stalls, fill-ins, locked-hand saves, Anxiety Play |
| 10 | Special discards | sarcastic/gentleman/baton transfer and Certain Discard |
| 11 | Bluffs | immediate unrelated blind-play and clue-target reinterpretation |
| 12 | Context | asymmetric/selfish connections and contextual interpretation |
| 13 | Intermediate bluffs | 3, critical-color, hard, known, and Good Touch forms |
| 14 | Trash moves | trash order, pushes, trash connections, and trash chop movement |
| 15 | Double bluffs | two-seat immediate blind-play obligations |
| 16 | Ejections/discharges | second/third finesse-position overrides |
| 17 | Dupe tech | duplication, assisted chop movement, and time-travel effects |
| 18 | Elimination | positive/negative single-out and elimination play effects |
| 19 | 5 tech | low-score phase, pulls, 5NE/5ND, and rank-5 precedence |
| 20 | Out-of-order play | occupied/suboptimal/no-information and OOO connection redirection |
| 21 | Ignition | multi-play promises and double/triple ignition compositions |
| 22 | Phantom playables | Scream/Sacrifice/Echo/Composition/Rebellious discard effects |
| 23 | Charms | 4 Charm, Blaze, and Hesitation effects |
| 24 | Unnecessary moves | reinterpretation as ignition, chop movement, or trash push |
| 25 | Priority | blind/connection/5/rank/position play order and follow-on connections |
| max (26) | Rare strategies | the uncommon extensions collected in the H-Group extras chapters |

`H_GROUP_LEVELS` is the machine-readable version of this single 26-entry sequence.

## Algorithm

1. Reconstruct each historical hand in oldest-to-newest storage order while replaying public events.
2. Derive direct clue facts and Level 1 focus at clue time, then run only the semantic passes enabled by the selected cumulative profile.
3. Persist invisible clues, chop movement, queued connections, forced plays/discards, and must-clue obligations across turns. Connection starts, advancement, invalidation, repair, and cancellation are audited by one state machine.
4. Keep ambiguous Prompt and layered-Finesse candidates as exact disjunctions: every earlier candidate must be a wrong but immediately playable card, and one candidate must have the promised identity.
5. Intersect those promises with the observer's logical identity sets. The planner keeps the resulting constraints symbolic until exhaustive endgame enumeration is feasible.
6. Generate only rule-legal convention candidates, apply hard convention constraints first, and only then order otherwise-equivalent actions with strategy priorities (including Level 25 Priority).
7. Use the same selected framework for belief constraints, action availability, priorities, and predictable continuations.

The current simulator is standard five-suit Hanabi. Rules whose only observable trigger exists in a non-standard variant remain unreachable until that variant is represented by `hanabi-core`; they are not guessed from a standard-game state.
