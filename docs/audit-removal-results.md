# Suspicious inference removal — 2026-09-08

## Scope

The four exemptions listed in audit item 4 were already removed in commit
`519c560`: first-turn-only bottom-deck-risk comparison, skipping opening
named-line deficits, opening Bluff coverage exemption, and the one-action
critical-chop-deadline restriction. They remain absent.

Removed audit item 5: `rationality.rs` and its exact-identity deduction from
declining a longer clue line, including dedicated facts, effects, and
provenance. A longer chain alone does not prove that the alternative was
strategically superior or unavailable. No replacement inference or fixture
changes were made.

The reviewed identity and action assertions remain. Only assertions inspecting
the removed implementation's private provenance were removed.

## Regressions

`scripts/check.sh` fails at Rust tests: its fail-fast run completed 182 tests
(181 passed, 1 failed), leaving 147 unrun. A subsequent no-fail-fast run
completed all 329 enabled Rust tests in 231 seconds: **326 passed, 3 failed**,
with 28 tests skipped by the normal configuration. The failures are listed
below.

Build, Clippy, formatting, and separately executed documentation, Python typing,
all 21 Python tests, bot CLI, and dead-public-code checks pass (zero Hawk
findings). The other four replay comparisons pass, with p4v0s415 covering only
its reviewed prefix through move 36. No claim is made about its unreviewed
generated suffix.

The first fail-fast failure is
`fifth_replay_move_twenty_one_does_not_reclue_an_exact_playable_red_three`. At
[p4v0s1, turn 21](https://hanab.live/shared-replay-json/415ifirpxqufunsxcwgc-tbokayavlbjdgwqvekus-pdfanhhmkrpml,03tdeh-sbxckceeeisbkdegfjep-edgcelevwbereseyocwa-eueofaefobfmew1a1den-fbeze2xae7fqfxf3f4f5-f0lceGodeIeFecetfEe1-,0#21),
the subjective projection no longer identifies Donald's #13 as exactly red 3.
This is an inference assertion, not a recorded-action disagreement.

The focused
`third_replay_declined_rank_four_resolves_alices_card_as_yellow_four` test also
fails: Alice's #2 remains `{y4, g4}`, rather than exactly `y4`.

The third replay agrees through move 36. At
[p4v0s2, turn 37](https://hanab.live/shared-replay-json/415wpiksfxldvautbukq-caxdvrochugihfywsepn-qjnmflmaprgbk,03obpd-elepgcsdejwaeaxaeken-ftehsaerebsceqsbewee-eiemkdegfse4kce5pafo-e1ffe3gbece8tbobfdeE-fukceDexeC,0#37),
the fixture plays Alice's yellow 4 (#2), while the engine clues 5s to Bob.

### Why 5s wins after removal

Alice no longer considers #2 certainly playable. The engine classifies 5s to Bob
as an urgent Save, scoring 540 plus the ordinary 100 action-priority offset. Its
forecast is Alice giving 5s followed by Bob playing red 5 (#34): one stack
point, one clue spent and refunded, no strikes, and two protected bottom-deck
risks. The forecast stops at unknown identity; this is not a proof of
optimality. Discard #3 has priority 400; every other admitted clue has priority
at most 391. The removed inference previously enabled the reviewed yellow-4 play
instead.

### All admitted clue candidates

These are the engine's current interpretations, not newly validated convention
claims. Scores are final within-category action priorities from the replay
comparison, not raw clue base scores.

| Clue             | Score | Interpretation       |
| ---------------- | ----: | -------------------- |
| 5s to Bob        |   640 | Urgent Save          |
| Red to Donald    |   391 | Trash Ejection       |
| 1s to Cathy      |   390 | Trash Ejection       |
| 1s to Donald     |   390 | Trash Ejection       |
| 3s to Donald     |   390 | Trash Ejection       |
| 4s to Donald     |   390 | Trash Ejection       |
| Blue to Bob      |   325 | Trash Chop Move      |
| 2s to Bob        |   324 | Trash Chop Move      |
| Green to Bob     |   305 | Ejection             |
| Red to Cathy     |   305 | Trash Push Discharge |
| Purple to Donald |   305 | Trash Push Discharge |
| 4s to Cathy      |   304 | Trash Push Discharge |
| 2s to Donald     |   304 | Trash Push Discharge |

### All rejected clue candidates

| Clue            | Rejection           |
| --------------- | ------------------- |
| Red to Bob      | NoNewInformation    |
| Yellow to Bob   | NoConventionMeaning |
| 4s to Bob       | NoConventionMeaning |
| Yellow to Cathy | NoConventionMeaning |
| Purple to Cathy | NoConventionMeaning |
| 3s to Cathy     | NoConventionMeaning |
| 5s to Cathy     | NoConventionMeaning |
| Blue to Donald  | NoConventionMeaning |

The removal experiment deliberately leaves these regressions unresolved rather
than replacing the deleted assumption or weakening the reviewed expectations.
