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
  retains the minimum/maximum clue cost over the mutually exclusive branches,
  including outstanding critical Saves. `ProgressTiming` requires equal endpoint
  progress, no greater completed/pending clue cost, and an earlier score lead
  without falling behind at any shared checkpoint. It retains the final
  comparison operands; earlier checkpoints remain in each projection.

- All rules-legal actions, their admission status, conventional interpretation,
  priority, available rejection reason, and clue score components. A separate
  scheduling adjustment reconciles the compiled clue score with final priority.
  Non-clue actions have no fabricated clue breakdown.
- Recipient-side focus, touched cards, play/Save superpositions, recognition
  provenance, and convention-documentation links when retained. Rejection
  diagnostics include touched cards and a description of the exclusion rule. A
  generic `NoConventionMeaning` is explicitly identified as an exclusion
  classification, not a detailed proof from every semantic generator.
- Common principle checks for Minimum Clue Value, Good Touch, and response
  safety. Each records `Pass`, `Exception`, `Unresolved`, or `Fail`, its
  evidence, and an exception source when applicable. A rejection from this
  boundary uses that recorded evidence. Missing causal evidence is conditional
  admission; projections stop before relying on it and exact search reports
  `ConditionalAdmission` instead of treating it as proven.
- A duplicate-touch exception retains `semanticEvidence.safetyContinuation`: the
  actual funded actions establishing safety, the collateral cards, and the real
  successor selected by ordinary play order. This is admission evidence, not a
  promise about unknown future draws or an exact-search principal variation.
- The actual pairwise comparisons, including endpoint decisions and cycles.
  Their `authority` distinguishes policy, forecast evidence, and heuristic
  preference. Higher raw scores do not necessarily win. Exact-phase outcome
  statistics are reported separately; an exact principal variation is not
  currently retained.
- `actualBasis` retains the resource operands used by endpoint comparisons,
  including the elapsed horizon and normalized token values. Raw endpoints and a
  shared checkpoint are also included, but are labeled as context rather than
  falsely presented as the decisive comparison. Safety/policy decisions need not
  have a resource-checkpoint basis.
- Projected actions with actors, one-based turns, actor-relative interpreted
  card domains where recorded, token changes, strikes, Save Principle
  violations, and BDR. Conditional branches, assumptions, and dependency
  evidence remain explicit. Assumptions retain their originating turn, focus,
  and prerequisite response decision; they cannot establish that same response.
  Missing interpretations are null, not invented from the deck.
- Endpoint values distinguish secured cards, identified future cards, and
  executable committed plays. An identified card can still have missing
  predecessors; it is not an extra immediate point.
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

## Move calculation budget

Every public move calculation defaults to a 120-second wall-clock budget, capped
at 120 seconds even if a library caller requests more. `--move-time-ms N` on
`analyze`, `live-action`, and `live-session` accepts a shorter budget
(1–120000). The budget includes initial convention inference, belief
enumeration, exact search, and symbolic forecasts. Existing world/node
thresholds still avoid predictably expensive exact searches; a small number of
worlds alone does not bound recursive convention inference.

At expiry, the planner returns its best-so-far convention-admitted move, using
completed root forecasts where available and otherwise the convention-preferred
move. The preferred move is projected first. Interrupted projections and partial
exact outcomes cannot establish superiority or optimality. If initial admission
or the existence of a consistent world has not yet been established, calculation
returns a deadline error instead of guessing a move. Explicit caller
cancellation, request deadlines, and work limits still return errors rather than
partial moves.

JSON reports expose `planning.budgetExhausted`, `lines[].projectionEvaluated`,
and `configuration.moveTimeMs`; live planning details also mark each root's
projection availability. An interrupted exact solve reports `TimeLimit`.
Symbolic timeouts retain the reason exact search was skipped. Text reports flag
the best-so-far result and unavailable forecasts. Best-so-far choices can depend
on machine speed; a timed-out result is not a completed replay audit.

Deadline checks occur inside recursive inference as well as search loops. A
private unwind signal crosses infallible inference interfaces and is caught at
calculation boundaries; scoped caches/guards are restored and ordinary panics
propagate. This requires Rust's unwind panic strategy (the repository default).
The cap is cooperative: scheduling and final result serialization can add a
small amount of elapsed time beyond the search deadline.

`PriorityRefundTiming` compares two plays that both lead into clued teammates.
Its `actualBasis` uses the equal known prefix before the actor returns and
retains both position values. At this stage, `scheduledRefunds` counts the play
refunds actually scheduled within that horizon. It does not credit a five merely
because a chain could eventually reach it. This narrow preference precedes the
ordinary lower-rank play-order tiebreak while preserving urgency, protection and
progress.

Projected decisions can report `MajorityCoverage` with an `actualBasis.stage` of
`majorityCoverage`. This is an observer-weighted choice of a teammate's likely
response, not an endpoint dominance proof. `coverageProbability` records the
identity and the covered/total physical-assignment weights. A strict majority
can favor playing over a redundant loading clue when the only projected loss has
a likely duplicate in the source observer's hidden hand. It does not reveal that
hand or remove the conditional loss from either projected line. The projected
decision's `selection` explains this override. Incomplete enumeration and
exactly50% retain the ordinary selector's choice.

### Conditional private-information policies

`projection.privateBranches` retains bounded, mutually exclusive assignments for
cards a projected teammate can see but the original observer cannot. A branch
records its decision turn, modeled actor, relevant card assignments, and
complete continuation. These assignments are hypotheses, not revealed cards.
Future draws stay blank, and an owner's decision view still hides their own
assigned cards.

When a threatened chop could instead have a clued replacement in an unknown
external hand, the forecast can split before the teammate's protection decision.
Each branch evaluates the action and its consequences together. It must not
combine a duplicate world's rejected clue with a nonduplicate world's loss.
Branches use bounded convention continuations, not a claim of optimal play.
Comparisons use the common elapsed horizon and retain worst-branch losses;
mutually exclusive card values are never added together. If the relevant domain
exceeds the branch budget, the projection reports an interpretation frontier
instead of treating an unexpanded possibility as a certain discard.

Private assignments can condition actions and their losses, but are masked again
when valuing the original observer’s information. They cannot earn endpoint
credit as if the observer had learned those hidden faces.

A witnessed loss in a longer conditional branch remains comparable when the
other candidate is fully projected through that loss's turn. An unfinished
sibling is not evidence that the witnessed loss disappeared. This does not
permit a shorter competing forecast to claim avoidance. Majority-response
selection can inspect the covered continuations without altering the original
observer's probability or the minority branches' recorded losses.

`knownCoverageScheduling` is the certain counterpart: a visible, touched
replacement makes the projected discard safe, so the same scheduling conditions
can favor the ready play at the root as well as in a teammate forecast. Its
basis records both endpoints at the common horizon; it supplies no private-card
probability or hidden identity. The uncertain `majorityCoverage` rule remains
separate.
