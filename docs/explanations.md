# Decision explanations

```sh
cargo run --release -p hanabi-cli --bin hanabi-engine -- \
  analyze crates/hanabi-protocol/tests/fixtures/game-p4v0s1.json \
  --turn 14 --convention h-group --h-group-level max \
  --objective perfect-score --explain
```

`--turn` is the number of **completed actions**: 14 means Hanab Live turn 15.
The report prints both numbers and a verified replay link. It never reads future
card identities to label hypothetical plays.

Prefer `--live-turn 15` when discussing a position with someone on Hanab Live.
It addresses the same position without subtracting one.

To request particular competing moves, use their selectors (also printed in the
candidate table):

```sh
cargo run --release -p hanabi-cli --bin hanabi-engine -- \
  analyze crates/hanabi-protocol/tests/fixtures/game-p4v0s1.json \
  --live-turn 18 --convention h-group --h-group-level max \
  --objective perfect-score --candidate purple:Donald --candidate red:Cathy \
  --format json > decision.json
```

Selectors are case-insensitive: `purple:Donald`, `4:Alice`, `play:17`, or
`discard:12`. Card numbers are stable card IDs, not slots. Repeat `--candidate`
to include several actions; rejected candidates remain visible without an
invented continuation. `--lines all` includes every evaluated root line.

`--lines N` selects N evaluated continuations (default 2). The selected action
comes first, followed by pairwise-win count and heuristic priority. This is a
display order, not a second selection algorithm: the engine's comparisons may be
cyclic and do not always define a runner-up. The fixture's next action is
included even outside that limit if it was evaluated; otherwise the candidate
table reports its rejection or exclusion. No fabricated line is generated for an
action the engine never projected.

`--format json` emits only a versioned JSON object on stdout (schema version 2).
Both `--format` and `--lines` imply `--explain`. The plain `analyze` output
remains unchanged.

The report contains:

- `discardBranches` retains each possible safe-discard reveal and its separate
  continuation. Shared actions do not imply a known discarded face. An exhausted
  reveal budget is shown as `SafeDiscardReveal`; its refund is recorded, but no
  concrete post-discard endpoint is invented. `FundedProgress` comparisons name
  the shared horizon and retain all branch operands. `ClueEfficiency` also
  retains the minimum/maximum clue cost over the mutually exclusive branches.

- All rules-legal actions, their admission status, conventional interpretation,
  priority, available rejection reason, and clue score components. A separate
  scheduling adjustment reconciles the compiled clue score with final priority.
  Non-clue actions have no fabricated clue breakdown.
- Recipient-side focus, touched cards, play/Save superpositions, recognition
  provenance, and convention-documentation links when retained. Rejection
  diagnostics include touched cards and a description of the exclusion rule. A
  generic `NoConventionMeaning` is explicitly identified as an exclusion
  classification, not a detailed proof from every semantic generator.
- The actual pairwise comparisons, including endpoint decisions and cycles.
  Higher raw scores do not necessarily win. Exact-phase outcome statistics are
  reported separately; an exact principal variation is not currently retained.
- `actualBasis` retains the resource operands used by endpoint comparisons,
  including the elapsed horizon and normalized token values. Raw endpoints and a
  shared checkpoint are also included, but are labeled as context rather than
  falsely presented as the decisive comparison. Safety/policy decisions need not
  have a resource-checkpoint basis.
- Projected actions with actors, one-based turns, actor-relative interpreted
  card domains where recorded, token changes, strikes, Save Principle
  violations, and BDR. Conditional branches, assumptions, and dependency
  evidence remain explicit. Missing interpretations are null, not invented from
  the deck.
- Endpoint values, stopping reasons, and pending unknown discards. A selected
  forecast action is not necessarily forced; pending BDR is a possible risk, not
  proof of an inevitable loss. `forcedRoot` records the root convention
  constraint, not a claim that every continuation is forced.
- `projectedDecisions` records the actual bounded follow-up searches: root
  candidate, turn, actor-relative board/knowledge, admitted and rejected
  actions, numeric terms, selected action, and pairwise comparisons. The
  `alternatives` field includes the leaf projections actually used to evaluate
  each competing move. IDs distinguish multiple conditional decisions on the
  same turn. These records apply to strategic follow-up searches; their
  lower-order leaf policies do not run another strategic search. The JSON
  retains all details; text summarizes competing admitted actions and
  comparisons favoring the selected move.
- Starting board, observer knowledge, search limits, objective, build revision
  and dirty status, ruleset revision, and current source-checkout provenance.
  The build revision and current checkout are distinct to expose stale binaries.

Both renderers consume one `PositionAnalysis`. They do not rerun candidate
generation or search to reconstruct an explanation. The live planning-details
serializer and CLI share projection/comparison serialization.

Detailed decision capture is enabled only around explanation analysis and is
restored on return or panic. Ordinary gameplay does not allocate these traces.
No selected move, scoring term, or convention rule is changed by enabling it.
Reports can be large with `--lines all`; use JSON redirection for complete data.
