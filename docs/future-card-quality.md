# Future-card quality

Source: the user's September 15, 2026 review of the p4v0s1 opening, not an
additional H-Group convention or a new clue-admission exception.

## Comparison

The endpoint evaluator retains the quality of each distinct secured future
identity instead of treating all secured cards as interchangeable:

- Prefer lower ranks, and fewer missing predecessors within a rank.
- Prefer a visible next card when rank and predecessor distance match.
- A 3 with two missing predecessors outranks a 4 with one missing predecessor.
- A 4 with at most one missing predecessor and its visible 5 outranks that
  distant 3. A 3 with at most one missing predecessor remains above it.

These are ordered categories, not additive point weights. The near-4 category
also includes zero missing predecessors so improving its accessibility cannot
make it worse. "Missing" means neither played, secured, nor visible from the
planning player's view. It is not a predicted number of turns. Unknown cards and
draws are not silently assigned identities to make a chain accessible.

The collection of future cards is sorted and compared pairwise. One excellent
promise does not numerically cancel out a weaker second promise. This quality
comparison participates in endpoint dominance when the two lines have equal
current score and equal numbers of secured future identities. Existing score,
token, danger, blockage, and opportunity comparisons remain; quality cannot
block converting a secured card into a point or substitute for an extra play.

## Reviewed opening

[p4v0s1, turn 1](https://hanab.live/shared-replay-json/415ifirpxqufunsxcwgc-tbokayavlbjdgwqvekus-pdfanhhmkrpml,03tdeh-sbxckceeeisbkdegfjep-edgcelevwbereseyocwa-eueofaefobfmew1a1den-fbeze2xae7fqfxf3f4f5-f0lceGodeIeFecetfEe1-,0#1).

| Line    | Protected future card | Missing predecessors | Visible successor |
| ------- | --------------------- | -------------------: | ----------------- |
| 3 Bluff | p3                    |                    1 | p4                |
| 3 Bluff | r3                    |                    2 | None              |
| 4 Charm | b4                    |                    1 | None              |
| 4 Charm | g4                    |                    3 | None              |

The Charm can no longer be declared endpoint-superior merely because it has an
extra counted protected risk or a visible successor to its b1 play. Its future
cards are weaker. The raw clue priorities remain 596 for the Bluff and 500 for
the Charm; without the erroneous dominance pruning, the Bluff wins. No clue
bonus, seed condition, or fixture change was added.

Tests cover the ordering, the visible-5 reversal, the actual four cards and
distances from the reviewed replay, and the existing full-replay action oracle.

## Validation and remaining disagreement

All 52 actions in p4v0s1 agree again, without editing the fixture. The full
non-fail-fast Rust run completed in 249.04 seconds: 333 passed, 3 failed, 28
skipped. `scripts/check.sh` passes formatting/build/Clippy but stops at the
existing red-3 inference assertion. The separately run documentation, Python
typing/tests, and dead-public-code checks pass.

The three failures are the same inverse-planning failures documented in the
[removal report](audit-removal-results.md), not regressions introduced here. The
next action disagreement remains
[p4v0s2, turn 37](https://hanab.live/shared-replay-json/415wpiksfxldvautbukq-caxdvrochugihfywsepn-qjnmflmaprgbk,03obpd-elepgcsdejwaeaxaeken-ftehsaerebsceqsbewee-eiemkdegfse4kce5pafo-e1ffe3gbece8tbobfdeE-fukceDexeC,0#37):
Alice clues 5s to Bob instead of playing y4 (#2), because her possibilities
remain y4/g4 after removal of the inference from a declined alternative clue.
The candidate scores and interpretations in that report were rechecked and are
unchanged. That removed inference was not restored as part of these
rank/distance comparisons.
