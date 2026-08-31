# Refactor history

This file records architectural changes whose intent is easy to lose when
fixing an individual convention regression. Entries describe the problem, the
boundary introduced to solve it, and the property that future changes should
preserve. Commit hashes refer to this repository's Git history.

## Design guardrails

- Public history is the only input to convention interpretation. Simulator
  truth may validate a replay, but it may not select an action or interpretation.
- Time, observer, and ownership are part of a fact. Do not substitute a current
  visible identity for what an actor knew before an earlier event.
- Mutually exclusive readings stay correlated. Do not merge their identities,
  connections, or required repairs card-by-card.
- Convention recognition produces typed, provenance-bearing facts. Signals are
  explanations and recognizer history, not a second source of current truth.
- Materialized card sets, epistemic views, action schedules, and plan summaries
  are read models. Each has one authoritative derivation path.
- Mandatory convention behavior is an admissibility constraint or policy tier;
  numeric utility only compares actions that remain semantically equivalent.
- Unknown future cards remain blank until exact endgame enumeration is both
  sound and computationally bounded.
- A prospective clue is compiled once. Admission, recipient validation,
  strategic comparison, explanation, and planning consume that compiled
  result rather than replaying the clue through independent semantic paths.
- Connection lifecycle questions are answered by `ConnectionManager`; callers
  may inspect obligations but must not reconstruct active-versus-queued status.
- Caches are scoped to one immutable position or exact solve. They may reuse a
  pure semantic result, but may not become a second mutable convention state.

## 2026-08-31: prefix replay memoization and bounded world validation

### Why

The representative Max-profile rollout had grown to 782 seconds. Convention
reduction repeatedly rebuilt the same actor-relative history prefixes, first
for ordinary interpretation and again for blind-reverse empathy. Prospective
Save validation compounded that work by materializing as many as 256 complete
hidden-hand worlds for every candidate, replaying each world, and then treating
reaching the cap as if every legal world had been checked. The latter was both
slow and an unsound safety proof.

### Changes

- Added a thread-local replay memo scoped to one top-level immutable reduction.
  Its key contains the complete `PlayerView`, profile, perspective depth, and
  empathy mode, so recursive actor-prefix queries reuse an `HGroupState`
  without leaking results between positions or solves.
- Replaced the collected prospective-world vector with a streaming visitor
  that stops at the first unsafe contextual Save world. A contextual Save is
  accepted only when enumeration reports `Exhausted`; `LimitReached` and
  `VisitorStopped` are not proofs of safety.
- Kept ordinary Level-1 rank-2/rank-5 Save precedence and critical Saves on a
  typed invariant path. Their recipient reading can be `Save` or
  `PlayOrSave`, but resolving the giver's hidden hand cannot remove the Save
  branch. Eight-Clue and other contextual Saves continue through exact world
  validation.
- Cached each prospective Save verdict inside the existing per-position
  analysis scope and added a regression test for traversal termination
  semantics.

The isolated Max-profile rollout fell from 782.08 seconds to 7.23 seconds
(about 108x faster). Memoization alone reduced it to 51.75 seconds; streaming
and bypassing irrelevant hidden-world enumeration provided the remaining
improvement.

### Preserve

Replay memo keys must contain every semantic input and the cache lifetime must
not outlive one top-level reduction. Do not use a sample limit as evidence that
a contextual clue is safe. Add a typed, convention-level invariance proof when
a clue meaning does not depend on hidden worlds; otherwise require exhaustive
enumeration or conservatively reject the candidate.

## 2026-08-31: compiled actions, owned connection queries, and scoped semantic caches

### Why

The fourth expert replay exposed several failures with a common cause. A clue
could be classified during candidate generation, reconstructed again during
recipient replay, and then partially reconstructed a third time for strategic
comparison. Those paths disagreed about fixed Prompt candidates, whether a
connection step was active or merely queued, whether a multi-step Finesse was
a Bluff, and whether touching a later connection layer was a redundant clue or
a valid Continuation Clue. `ClueCandidate` also stored its target, Save status,
purpose, connection counts, and named move as independent fields, allowing
internally contradictory values.

`ConnectionManager` already owned promise mutation and provenance, but its
slice `Deref` let every consumer independently implement lifecycle queries.
Finally, required behavior such as the first 5 Stall was filtered in action
ordering while other obligations used typed constraints, and exact identity
branches repeatedly recompiled the same public observation.

### Changes

- Replaced the candidate bag with `CompiledClueAction`,
  `CompiledClueSemantics`, and `CompiledClueLine`. The target is derived from
  the `Action`; Save status is derived from `CluePurpose`; fallback play and
  fallback Save are distinct variants; and recipient-derived line metrics are
  committed together. Internal validation rejects inconsistent compiled
  meanings before policy or planning consumes them.
- Renamed the complete non-clue decision record to `CompiledHGroupAction` and
  observer projections to `CompiledObserverProjection`, making the boundaries
  between visible truth, observer-relative compilation, and final action
  policy explicit.
- Added `CompiledProspectiveClue`. The normal history reducer now applies each
  hypothetical clue once, and candidate admission, recipient assessment,
  hazard checks, named-line measurement, and strategic comparison share that
  immutable transition and its lazy team projections. The existing
  prospective-versus-observed replay invariant remains the transactional
  equivalence check.
- Removed `ConnectionManager`'s `Deref` implementation. Active-step checks,
  queued-identity checks, actor occupancy, and clue matching now go through the
  manager. `ConnectionClueMatch` distinguishes an active redundant touch from
  a valid later-layer continuation in one place.
- Replaced the loose constraint reason/action pair with a typed
  `ConventionRequirement`. Hard alternatives are represented together, and
  the early-game 5 Stall is now also installed as an `EarlyFiveStall`
  requirement rather than relying only on candidate scores. Numeric utility
  remains a tie-break among actions that satisfy the same requirement.
- Reused the candidate pass's baseline and hypothetical team projections in
  strategic evaluation. Added a per-solve `ConventionAnalysisCache` so exact
  identity branches that converge on the same `PlayerView` compile convention
  semantics once without introducing global mutable state.
- Added architecture tests for the compiled-action boundary and connection
  ownership, a lifecycle test for active-versus-queued clue matching, and a
  planner test proving that a repeated public observation is compiled once.

### Preserve

New clue semantics belong in the compiled clue transition, not in a new
consumer-side replay. Do not add stored `target` or `save` fields back to a
compiled clue, expose `ConnectionManager` as a slice, or use numeric priority
to enforce a mandatory convention response. Caches must either key on the
complete immutable observation and convention profile or, as in one exact
solve, be scoped to a single fixed profile; cached results must never be patched
after compilation. The event-sourced knowledge program,
branch-local clue hypotheses, and public-history-only interpretation rules
from the previous refactors remain authoritative.

## 2026-08-29: branch-local clue plans and staged knowledge compilation

### Why

Recent replay debugging exposed the same failure in several forms: an
ambiguous clue was represented as a union of focus identities, while the
connection, required Fix, and subsequent owner knowledge were taken from one
selected identity. That flattened mutually exclusive worlds and allowed a
repair inferred from visible card truth in one perspective to become an
unconditional obligation in another. Connection searches also accepted cards
newly touched by the current clue as if they had been Prompt candidates before
the clue.

The owner-knowledge compiler had meanwhile become a long sequence of inline
mutations. Its semantic order was real but implicit, making it easy for a new
Good Touch, transfer, or connection rule to run at the wrong stage or duplicate
an existing pass.

### Changes

- `ClueInterpretationHypothesis` retains one branch per possible Play identity,
  including that branch's connection steps, optional repair, and loaded state.
- `ConnectionPlanningContext` provides a shared immutable simulation path and
  a single commit path. Its typed inputs distinguish pre-clue Prompt candidates
  from current-clue touches and already protected cards. The event turn is an
  explicit required input; connection scheduling no longer guesses it from the
  last clue already stored in a partially reduced history.
- `FixObligations` replaces the single global optional Fix. A repair can be
  unconditional or conditional on a clue's focus having a particular identity;
  candidate generation activates only a condition supported by the observer.
- `ConventionKnowledgeCompiler` names and orders the owner-knowledge passes:
  replay closure, declined alternatives, Good Touch, transfer/ejection
  reinterpretation, connection promises, focus, forced plays, and saves.
- Replay validation proves clue hypotheses uniquely cover the complete Play
  domain and that every retained connection step has candidates.

### Preserve

Adding a new clue interpretation must add consequences to its own hypothesis,
not to global connection or repair state. Planning code should simulate through
`ConnectionPlanningContext` and mutate lifecycle state only during commit. New
owner deductions belong in one named compiler pass with typed provenance.

## 2026-08-29: correlated plans and auditable decisions (`2b11078`)

### Why

Earlier code could merge ordinary and empathy interpretations, confuse a
hypothetical projected action with an observed replay action, and allow a large
heuristic score to outweigh mandatory convention behavior. Candidate admission,
recipient interpretation, and strategic ranking also lacked an explicit typed
handoff, which made a plausible generator-side story look equivalent to a
recipient-confirmed interpretation.

### Changes

- Added correlated whole-history `InterpretationHypotheses`.
- Added `ConditionalPlan`, dependency-linked `PlanStep` values, and a distinct
  `ProjectedAction` type for symbolic blank-card continuations.
- Split candidate processing into semantic admission, typed recipient
  assessment, causal comparison, and ranking.
- Added `ConventionPolicyTier` and strengthened hard convention constraints.
- Added actor-before-event belief projections, audited rule proposals, exact
  transfer facts, inverse-planning deductions, and broader expert replay
  architecture coverage.

### Preserve

Do not collapse alternative worlds into independent per-card masks, turn a
projection into authoritative history, or replace semantic tiers with score
constants. A clue supported only by generator reasoning must remain visibly
different from one reproduced by recipient replay.

## 2026-08-28: causal knowledge transfer (`1d60492`)

### Why

Owner knowledge was previously derived by comparing a final card note with its
starting state and then guessing which event caused the difference. That lost
causality, made transient knowledge difficult to retract, and encouraged exact
endgame code and serializers to reconstruct different meanings. Relational
`OneOf` claims could also be flattened incorrectly into exact per-card facts.

### Changes

- Made `ConventionKnowledge` an event-sourced, provenance-indexed effect
  program whose changes are attached to their causal public transition.
- Added the canonical owner `EpistemicState` read model used by production and
  regression serialization.
- Added `ConventionConstraintGraph` as the one bridge from per-card and
  relational convention constraints to exact world enumeration.
- Added `RuleExecutionContext` and an event-reducer mutation boundary so a
  recognizer receives one coherent time, observer, and profile.

### Preserve

Record knowledge when its semantic cause occurs. Do not infer provenance from
the final state, rebuild convention notes in serializers, or force every member
of a relational claim to the same identity.

## 2026-08-27: immutable owner knowledge (`ac5619a`)

### Why

Several consumers independently maintained card knowledge, playability, and
Good Touch claims. A fix in clue interpretation therefore could leave action
selection, snapshot output, or prospective analysis with stale or wider facts.
Current stack heights were also used accidentally when a clue-time or
before-player horizon was required.

### Changes

- Introduced typed `CardKnowledgeEffect` values and a pure
  `ConventionKnowledge` reducer; ordinary deductions can only narrow a domain,
  while an explicit reinterpretation is required to replace one.
- Added `ActionSchedule` for direct plays, connections, forced plays, and
  required discards.
- Added `StackTimeline` to label clue-time, current, and before-player stack
  horizons.
- Added the shared `IdentityClaims` boundary so exact claims and relational
  alternatives cannot be conflated by individual consumers.
- Extended transition deltas and architecture tests to cover owner-knowledge
  effects and hidden-truth noninterference.

### Preserve

Project owner knowledge by reducing typed effects once. Do not introduce a
second mutable knowledge aggregate, hand-roll an action schedule in a consumer,
or pass an unlabeled stack-height array across temporal boundaries.
