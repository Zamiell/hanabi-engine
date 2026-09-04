# H-Group architecture

The H-Group implementation is a public-history reducer followed by a decision
pipeline. Its central rule is that convention interpretation never receives
simulator truth.

## Data flow

```text
PlayerView
   |
   v
LogicalDeductions             convention-free identity domains
   |
   v
H-Group history reducer
   |-- HistoricalView         identities legal at the event's turn
   |-- HGroupTurnContext      explicit before/after event snapshots
   |-- RuleSpec               phase and dependency metadata
   |-- RuleProposal           audited per-rule transition record
   |-- ConventionTransitionResult  causal record for one public event
   |-- ConventionTransitionDelta   exact card-fact and epistemic changes
   |-- ConventionJournal      incremental facts plus explanation history
   |-- ProvenancedCardSet     source-owned materialized card facts
   |-- ConnectionManager      Prompt/Finesse lifecycle state machine
   |-- HGroupSignal           append-only explanation log
   |-- ConventionFacts        relational current convention truth
   |-- CardKnowledgeEffect    typed identity/promise/status change
   |-- ConventionKnowledge   event-sourced owner-knowledge program and provenance index
   |-- ConventionConstraintGraph relational OneOf/connection constraints
   |-- ActionSchedule         unified live play/discard commitments
   |-- StackTimeline          clue/current/before-player stack horizons
   |-- ClueInterpretationPlan one Play/Save/Fix/5CM/Stall precedence result
   |-- ClueInterpretationHypothesis identity-correlated connection/fix branch
   |-- FixObligations         conditional and unconditional repair duties
   |-- TeamConventionSnapshot coherent lazy per-player epistemic overlays
   |-- InterpretationHypotheses correlated whole-state alternatives
   `-- ActorBeliefBefore      acting player's pre-event knowledge
   |
   v
EpistemicState               observer-owned identity domains and provenance
   |
   v
ConventionConstraints        hard required/admissible actions
   |
   v
typed candidates / rejections / recipient assessments
   |
   v
LineOutcome                  actions, protection, trash, and connections
   |
   v
ConditionalPlan             projected steps and dependency frontier
   |
   v
blank-card plan summary / exact endgame solver
```

The giver, recipient, and hypothetical analyzer all use
`PerspectiveProjector`. Projection depth is an enum, not a boolean, so a call
site must explicitly request either observer-only interpretation or nested
recipient modeling.

## Boundary responsibilities

- `turn_context.rs` owns temporal access. A historical rule cannot ask for an
  identity revealed by a future play, discard, or draw. `ActorBeliefBefore`
  groups the acting player's pre-event discard knowledge so a visible card
  face cannot be substituted for what that actor knew.
- `action_schedule.rs` is the unified read model for direct plays, active
  connections, forced plays, and required discards. `StackTimeline` labels
  stack heights as clue-time, current, or before a named player's turn; code
  projecting a future forced play cannot silently use current stack heights.
- `claims.rs` is the observer-safe exact-identity and Good Touch claim
  boundary. Relational `OneOf` claims never become exact claims on every
  candidate card.
- `connection.rs` is the sole mutation boundary for delayed connections. It
  records starts, completions, invalidations, fixes, displacement, and
  supersession with a turn and reason. Every lifecycle has a stable
  `PromiseId` and immutable provenance, including its creation turn, actor,
  focus, expected identity, and connection kind. It also removes stale
  candidates from blocked later layers and rejects duplicate obligations.
- `ConnectionPlanningContext` in `h_group.rs` is the shared read-only planner
  for a clue's delayed connection. It distinguishes cards promptable before
  the clue from cards touched by the current clue, simulates every focus
  identity without mutation, and commits the selected branch exactly once. Its
  event turn is explicit rather than inferred from a partially updated clue
  list.
- Every clue retains a `ClueInterpretationHypothesis` for each possible Play
  identity. Connection steps and required Fixes stay attached to that identity
  instead of being flattened into independent unions. `FixObligations` may
  retain several conditional repairs; action selection activates only the
  conditions visible in its observer's state.
- `ledger.rs` owns provenance for materialized card facts through
  `ProvenancedCardSet`. A fact can have independent event, rule, and
  `PromiseId` sources; retracting a promise removes only the consequences it
  established. Materialized sets are compact read indexes, not independent
  sources of truth.
- `primary.rs` is the one primary clue-precedence resolver. Fixes, 5 Chop
  Moves, low-score 5 handling, stalls, saves, and ordinary Play meanings are
  resolved once before secondary connections and named explanations run.
- `effects.rs` owns `ConventionJournal` and reduces recognized journal effects
  idempotently. Materialized rule changes are committed at the rule boundary,
  where the ledger attaches the rule source and records an exact delta; direct
  mutation of an unsourced final state fails validation.
- `facts.rs` separates current truth from history. `HGroupSignal` explains why
  something was inferred and remains available to recognizers needing
  cross-event causality; `ConventionFacts` is what downstream code queries for
  live semantic state, including exact knowledge transfers.
  Ambiguous identities are retained as relational `OneOf` claims rather than
  being incorrectly copied onto every candidate card.
- `model.rs` owns `ConventionCardState`, the canonical grouping for clue,
  play, discard, invalidation, and fact state associated with cards.
- `epistemic.rs` is the strategic representation of identity domains one
  named observer may retain, including the provenance of each domain. Action
  obligations remain in the canonical convention state instead of being
  copied into a second, independently mutable status aggregate. It deliberately
  exposes neither deck order nor simulator truth. This canonical owner card
  read model is shared by production analysis and regression serialization;
  serializers do not recalculate its logical identities, convention
  identities, provenance, obligations, positional state, or derived
  convention-only trash.
- `knowledge_effects.rs` owns the immutable, event-sourced
  `ConventionKnowledge` program, its per-card provenance index, and its pure
  reducer. The owner-knowledge compiler records each identity restriction,
  reinterpretation, promise, typed fact change, and play obligation at the
  semantic mutation that caused it. It no longer diffs one final card note and
  guesses a source afterward. Owner projection applies the ordered effects to
  logical domains without recognizing convention moves a second time.
  Ordinary inference cannot widen a domain; only a typed
  Fix/reinterpretation may replace it.
- `ConventionKnowledgeCompiler` in `interpretation/knowledge.rs` applies owner
  knowledge in named, ordered passes: replay closure, declined alternatives,
  established and promised Good Touch, transfer/ejection reinterpretations,
  connection promises, current focus, forced plays, and implicit saves. A new
  inference belongs in one pass rather than an unstructured final-note sweep.
- `constraint_graph.rs` is the single bridge from convention state to exact
  world constraints. Per-card domains, unresolved relational `OneOf` claims,
  and ordered connection alternatives are retained symbolically. A claim
  demonstrated by a revealed card excludes that identity from its surviving
  candidates instead of incorrectly forcing one of them to inherit it.
- `outcome.rs` retains clue consequences before strategy converts genuine
  preferences to numeric ordering. Giver-visible team coverage and
  owner-relative promised-action and clued-card-superposition equivalence for
  Clarity comparisons remain separate projections of one outcome.
- `rule_engine.rs` owns ordered post-event execution. Real replay and
  prospective transitions both enter this registry through the same history
  reducer. Each `RuleSpec` declares its semantic phase and dependencies, and
  diagnostic reductions retain every non-empty contribution as an audited
  `RuleProposal` transition record in the event's
  `ConventionTransitionResult`; `rules.rs` proves every level has exactly one
  valid execution path. Recognizers mutate only through the single
  `HGroupRuleEffects` capability, and the resulting delta is the authoritative
  account of what that transaction changed.
- `event_reducer.rs` owns the mutable capability passed to recognizers.
  `RuleExecutionContext` binds an immutable turn context, observer view, and
  profile into one value, preventing mismatched temporal or perspective
  arguments at the dispatch boundary.
- `identity.rs` and `hand.rs` own the shared identity, trash, playability,
  focus, chop, and finesse-position semantics used by giver, recipient, and
  planner paths.
- `constraints.rs` separates semantic obligations from utility. Urgent clues,
  connection responses, required discards, and must-clue states restrict the
  action set before numeric strategy priorities are compared. Planner actions
  also carry a lexicographic `ConventionPolicyTier`, so a heuristic number
  cannot outweigh a required action.
- `perspective.rs` owns observer projection and hypothetical public
  transitions. `TeamConventionSnapshot` groups lazy observer overlays for one
  coherent public position, so candidate consumers share lifecycle state.
- `prospective.rs` applies a proposed action once and shares its cached team
  snapshot between primary-meaning recognition, signal inspection, hazard
  checks, and strategic evaluation.
- `information_value.rs` compares valid alternative Fixes using the
  recipient's before/after identity domains. It prioritizes action certainty
  on convention-promised cards, estimated future clue savings, and identities
  weighted by play timing and criticality; color over rank is only a final
  tie-break for identical touch sets.
- `decision.rs` builds the one canonical action analysis consumed by direct
  selection and planning.
- `candidate.rs` gives each admitted clue a semantic purpose, recognition
  evidence, and named value components. `RecipientReplay` records direct
  recognition by the shared reducer; `GeneratorProof` is explicit for meanings
  that legitimately branch after the recipient sees the giver's hidden hand.
  Every excluded legal clue receives a typed rejection reason.
- `candidate_pipeline.rs` makes admission stages explicit in the type system:
  semantically admitted candidates become recipient-assessed candidates and
  only then become causally compared, ranked candidates. The assessment is
  explicitly either `RecipientReplay` or `GeneratorProof`.
- `interpretation.rs` owns observer-relative clue meaning, convention card
  inference, and convention-admissible clue generation.
- `plan.rs` distinguishes `ProjectedAction` from authoritative replay actions
  and stores projected consequences in dependency-linked `PlanStep` nodes.
  `symbolic_line.rs` builds this `ConditionalPlan`; new draws remain blank and
  projection stops at an explicit frontier before the plan is summarized.
- `hypothesis.rs` owns mutually exclusive whole-history interpretations. Each
  alternative retains its own connections, promises, and identity claims, so
  ordinary and empathy readings cannot be merged card-by-card.
- `rationality.rs` owns narrowly scoped inverse-planning deductions. It may
  infer an identity from a clue giver declining a strictly stronger,
  convention-valid alternative only when the counterfactual is unique. The
  resulting `DeclinedAlternativeInference` records the actor, turn, chosen
  clue, superior clue, card, and identity; owner knowledge consumes that fact
  through an explicit provenance-bearing effect rather than a replay-specific
  exception.
- `recognition.rs` is now only the level-gated registry surface and shared
  imports. Cohesive modules own Basic moves, Tempo and emergency discards,
  Chop Moves, Bluffs, advanced connections, special discards, Trash moves,
  late-game rules, and Extras. Observer knowledge derivation and candidate
  validation likewise live in focused `interpretation/` modules. `h_group.rs`
  retains history reduction and connection scheduling.
- `transition.rs` is the production causal boundary. Every retained public
  event has an exact compact `ConventionTransitionDelta` of materialized card
  facts and owner-knowledge effects; rule proposals also retain exact card
  replacements rather than comparing collection lengths. Strategic evaluation
  consumes these deltas.
  Human-readable signals remain explanations and recognizer history rather
  than an alternative source of current decision truth.
- `strategic_value.rs` compares structured `LineOutcome` values. It may use
  teammate identities visible to the giver for team coverage, but Clarity
  uses only each card owner's `EpistemicState`.

Shared card and hand semantics live in focused modules. New rule families go
in a focused recognition module and register with the one rule engine; they do
not create a second interpretation path.

## Invariants

Every completed replay reduction validates that:

- an active connection has at least one candidate;
- every active connection was scheduled through the lifecycle manager;
- the same actor/focus/step obligation is not duplicated; and
- every connection candidate is still in the promised actor's hand;
- every discard-now entry is unique;
- every relational identity claim has at least one candidate card;
- every clue has exactly one hypothesis for each Play identity, no duplicate
  identity branch, and no empty connection step;
- every conditional Fix points back to the clue identity hypothesis that
  created it;
- exact transferred knowledge is materialized in current facts rather than
  reconstructed from the diagnostic signal log;
- every active connection has registered promise provenance; and
- every materialized clue, protection, play, chop-move, and forced-play fact
  has at least one typed source;
- every promise-sourced retraction names registered promise provenance; and
- proposals are phase ordered, non-empty, tied to the event turn, and form a
  unique partition of post-event signals.
- every owner-knowledge effect references a live card and occurs in exactly one
  public transition delta;
- every owner-knowledge effect is attached to the transition named by its
  provenance turn, and the per-card provenance index exactly matches the
  ordered effect program;
- ordinary knowledge effects only narrow identity domains; replacement requires
  explicit reinterpretation provenance; and
- inverse-planning deductions retain the observed and counterfactual actions
  that justify them, and ambiguous or conflicting counterfactuals produce no
  identity restriction; and
- pure owner projection reproduces the convention compiler's result without
  recognizing clue meaning again.

Architecture properties additionally require that leaked own-hand truth does
not change an observer's `EpistemicState`, and that candidate signal and hazard
inspection share one prospective convention snapshot. They also run all expert
replays through every observer, prove canonical focus domains cannot be widened
by owner projection, and exercise generated legal histories.

The `game-p4v0s415.json` expert replay regression runs these checks for every
prefix and every observer. Temporal tests separately assert that future
own-card reveals and future draws cannot affect an earlier interpretation.
Perspective tests should construct hypotheses through `PerspectiveProjector`
rather than manually editing a recipient view. Architecture properties
additionally verify that the incremental facts equal a complete journal
reduction, every legal clue is exactly one of admitted or rejected, and a
resolved-world hypothetical clue produces the same recipient convention state
as the corresponding real event. The test corpus is physically separated into
shared fixtures, learning-path rules, historical regressions, strategy
behavior, architecture properties, and golden replay parity.

## Adding a convention rule

1. Identify the documented level and add or reuse its `HGroupRuleId` gate.
2. Read only the required side of `HGroupTurnContext`; use `HistoricalView` for
   old identity questions.
3. Register the family with the executable rule engine, emit typed public and
   owner-knowledge effects with their causal source, and use
   `ConnectionManager` for connection changes.
4. Add current query state to `ConventionFacts` rather than searching the
   signal log at decision time.
5. Express mandatory behavior as a `ConventionConstraints` rule. Use a numeric
   priority only for a genuine strategic preference among actions in the same
   `ConventionPolicyTier`.
6. Test giver and recipient interpretations, prospective/retrospective
   equivalence, hidden-truth noninterference when relevant, and at least one
   replay prefix.

These boundaries are deliberately stricter than a collection of move-specific
helpers: most past bugs came from one interpretation path updating only part of
the shared state or consulting information from the wrong observer or time.
