# Architecture review: September 18, 2026

This is a review of the recent work and existing refactor history, not a claim
that the proposed changes below have been implemented.

## Are we progressing?

Yes in observability, information boundaries, and individual reviewed cases; not
yet demonstrably in overall playing strength or stability. Concrete gains
include blank-draw projections, recipient-relative connection handling,
action-by-action diagnostics, explicit risk frontiers, and measured inference
reuse. The September 16 exact-key memoization report records a p4v0s2 reduction
from 237.75 to 102.62 seconds without changing the compared analyses.

There is also negative evidence. The last completed check before this change had
349 passing and 36 failing ordinary tests. The September 16 refactor entry
reported nine existing failures. Fixture revisions and changed expectations make
these counts imperfect measures of engine regressions, but a persistently red
suite weakens every subsequent claim of correctness. The complete, error-free
200-game strength baseline is still absent. Winning a comparison against a
repeatedly edited fixture is not independent evidence of strength.

## Recurring problems

1. **Incorrect intermediate behavior masquerades as strategic evidence.** A
   wrong teammate action produces a bad endpoint; changing BDR or efficiency
   ranking then treats the symptom. The current Bob/yellow example exposes
   exactly this sequence. Diagnose the earliest unsupported action first.
2. **Facts and estimates become interchangeable.** Saved identities, executable
   commitments, actual points, positional opportunities, and possible losses are
   separate fields, but comparison shortcuts sometimes make them fungible.
   `score + secured_future_plays` is the immediate example. Existing enum names
   and diagnostics do not themselves enforce the necessary contracts.
3. **Projection quality varies with depth.** Root choices use strategic
   forecasts; their leaf continuations use a cheaper convention policy.
   Actor-visible information also changes across perspective projections. An
   unsearched fallback is not equivalent evidence to a forced response or a
   strategically evaluated choice, even when their action types match.
4. **Mutable fixtures move underneath position-specific tests.** Tests named
   after a turn number can describe a different position after an earlier action
   changes. Stale tests, new engine bugs, and unreviewed generated suffixes then
   become mixed in the failure list.
5. **Validation has not closed the loop.** More local regressions and better
   timings help, but leaving accumulated failures unresolved makes broad fixes
   difficult to assess. Full checks are checkpoints, not a substitute for a
   small reproducer and explicit before/after behavior.

## Recommended next work, in order

### 1. Restore a trustworthy correctness baseline

Classify each failure as an implementation defect, a superseded reviewed
position, or an unresolved human decision. Repair or explicitly retire stale
cases under the existing provenance policy; do not bless new failures into a
passing baseline. Add fixture-prefix fingerprints to position-specific test
contracts so fixture drift is reported before a convention assertion runs. This
extends the review manifest; it does not preserve every obsolete position or
create new synthetic convention authorities.

### 2. Tighten the existing comparison boundary

`compare_endpoints` currently sends `RotationDevelopment`,
`ProtectedDevelopment`, and BDR results into the strong `evidence_reaches`
graph. Some are preferences, not proofs of superiority. Give comparison results
an explicit contract: proved result, constrained forecast comparison, or
heuristic preference. Keep heuristic evidence from silently acquiring proof-like
elimination authority. Preserve deterministic, cycle-aware selection while
making the allowed precedence explicit.

Group the existing endpoint fields by realized resources, executable
commitments, protected identities, and speculative opportunities. Retain the
existing canonical derivation; do not create another mutable evaluator or
duplicate convention inference. Require each comparison rule to state which
evidence classes and horizons it accepts. Test these contracts independently of
Hanabi convention histories.

### 3. Make projection assumptions easier to audit

Extend the retained `ConditionalPlan` evidence with the decision model used at
each step: forced convention response, evaluated strategic choice, or leaf
policy fallback. The recent unresolved-discard flags are a limited start, not a
complete provenance model. Keep root knowledge separate from actor knowledge; do
not cure forecast gaps by revealing hidden cards or inventing draws.

For human-reviewed lines, assert the next actions, observer-relative promises,
stop reason, and assessed losses, not merely the final selected move. Provide
these through one diagnostic command so inspection does not repeatedly require
temporary Rust tests. Reuse the existing projection serializer.

### 4. Measure strength on stable, separate games

Once correctness failures are resolved, run a fixed small diagnostic self-play
set before attempting the complete 200-game baseline. Track engine errors,
strikes, score, perfect games, changed decisions, and runtime separately. Do not
change benchmark expectations merely because the engine chose a new move.

## What should not be rewritten

Keep the FullState/PlayerView separation, event-sourced convention knowledge,
compiled prospective clues, ConnectionManager, observer-relative conditional
plans, and exact-key request-scoped caches. Earlier refactors already introduced
these boundaries for good reasons. The next work is to enforce their contracts
at evaluation and testing boundaries, not replace them or reintroduce MCTS.
