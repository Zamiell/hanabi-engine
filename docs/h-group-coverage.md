# H-Group documentation coverage

The engine's convention inventory is pinned to `hanabi.github.io` revision
`dd55b51aa622f709059a17e0d6afc2adb8402408`. It contains every level-three
heading in levels 1–25 and the Max extras pages: 357 unique sections in total.
The machine-readable inventory, including an exact website URL for every
section, lives in `H_GROUP_DOCUMENTATION_SECTIONS`.

Not every heading defines an independent state transition. The documentation
also contains definitions, examples, principles, precedence rules, mistakes,
illegal moves, and flowcharts. Those sections are implemented through the
shared clue interpreter, identity constraints, connection graph, candidate
rejections, and strategic evaluator. Named moves and strategies have a
dedicated `HGroupMoveKind`, and their production implementation and source URL
are enforced by architecture tests. This avoids pretending that a chapter-wide
handler proves coverage while also avoiding duplicate transition systems for
compositions such as a Trash Push Finesse.

## Numbered levels

| Effective level | Area | Documentation sections | Primary executable coverage |
| ---: | --- | ---: | --- |
| 1 | Basic conventions | 19 | clue focus, play/save precedence, Good Touch, Prompt/Finesse graph |
| 2 | Basic moves | 11 | 5 Stall, repeated connections, Reverse/Self-Finesse |
| 3 | Basic strategy | 9 | Fixes, repeated 1s, Sarcastic Discard, efficiency constraints |
| 4 | Chop moves | 9 | TCM, 5CM, OCM, chop-move precedence |
| 5 | Special finesses | 9 | Hidden, Layered, Clandestine, Queued, Ambiguous Finesses |
| 6 | Tempo clues | 10 | Tempo Clue, Tempo Clue Chop Move, and both parts of the Clarity Principle |
| 7 | Emergency discards | 8 | Scream, Shout, Generation, riding/permission checks |
| 8 | End-game | 10 | positional discard/misplay, distribution, end-game phase |
| 9 | Stalling | 11 | severity ordering, DDA, locked-hand, fill-in, anxiety, 8CS, burn |
| 10 | Special discards | 7 | Gentleman/Baton transfers and Sarcastic/Certain/Composition Finesses |
| 11 | Bluffs | 11 | Bluff, Self-Bluff, connecting and precedence constraints |
| 12 | Context | 10 | Selfish Clue/Finesse, stale 1s, Focus Inversion |
| 13 | Intermediate bluffs | 7 | 3, Critical Color, Hard, Hard-3, Known, Good Touch Bluff |
| 14 | Trash moves | 9 | Trash Push/Prompt/Finesse/Bluff, reverse and forced-GD interactions |
| 15 | Double bluffs | 8 | Double, Hard Double, Pestilent Double Bluff |
| 16 | Ejections/discharges | 6 | 5CE, UTD, UDD and ordered forced positions |
| 17 | Duplication | 6 | duplicitous value/play/tempo, ATCM, Time Travel CM |
| 18 | Elimination | 11 | notes, single-out, blind play, riding, self-CM, Finesse, TTE |
| 19 | 5 tech | 15 | 5 Pull, 5NE, 5ND and rank/suit interaction rules |
| 20 | Out-of-order play | 7 | occupied/OOO connections, suboptimal and no-information forms |
| 21 | Ignition | 9 | replay/trash/poke/CM and bomb double/triple ignition |
| 22 | Phantom playable | 8 | phantom state, sacrifice, echo scream, composition/rebellious discards |
| 23 | Charms | 4 | Charm, Blaze, Hesitation |
| 24 | Unnecessary moves | 5 | unnecessary ignition, chop move, and trash push classifications |
| 25 | Priority | 11 | shared play-priority ordering and priority connections |

## Max (effective level 26)

The 127 Max headings are covered by the same primitives plus explicit Max
recognizers for chop-move extensions, discard/misplay extensions, ejections,
discharges, fix clues, pushes/pulls, save clues, special bluffs, and special
finesses. This includes Elimination Bluff, Pestilent Triple Bluff, Pass/Purge
Bluff, Ambiguous Finesse Pass-Back, Certain Priority/Patch/Surreptitious/Lie
Component/Declined-5 Finesses, Rank Choice Save Finesse, and the
unpromptable-predecessor Save.

## Enforcement

The test suite checks that:

- all 357 pinned subsection URLs are unique and use `hanabi.github.io`;
- every named semantic move links to its exact documentation section;
- every semantic move has a production reference beyond level metadata;
- hypothetical clue interpretation agrees with recipient replay;
- replay state and incremental convention facts satisfy their invariants;
- every legal clue is either admitted or has an inspectable rejection reason;
- all numbered profiles and Max can roll deterministic scenarios to completion.

When the upstream documentation changes, update the pinned inventory, add or
change the production behavior, and add a focused regression before changing
the pinned revision recorded above.
