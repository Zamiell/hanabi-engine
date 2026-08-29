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
