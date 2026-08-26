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
   |-- RuleProposal           typed per-rule transition output
   |-- ConventionTransitionResult  causal record for one public event
   |-- ConventionTransitionDelta   exact materialized card-fact changes
   |-- ConventionJournal      incremental facts plus explanation history
   |-- ProvenancedCardSet     source-owned materialized card facts
   |-- ConnectionManager      Prompt/Finesse lifecycle state machine
   |-- HGroupSignal           append-only explanation log
   |-- ConventionFacts        relational current convention truth
   |-- ClueInterpretationPlan one Play/Save/Fix/5CM/Stall precedence result
   `-- TeamConventionSnapshot coherent lazy per-player epistemic overlays
   |
   v
EpistemicState               observer-owned identity domains and provenance
   |
   v
ConventionConstraints        hard required/admissible actions
   |
   v
typed candidates / rejections / recognition evidence
   |
   v
LineOutcome                  actions, protection, trash, and connections
   |
   v
blank-card forced-line projection / exact endgame solver
```

The giver, recipient, and hypothetical analyzer all use
`PerspectiveProjector`. Projection depth is an enum, not a boolean, so a call
site must explicitly request either observer-only interpretation or nested
recipient modeling.

## Boundary responsibilities

- `turn_context.rs` owns temporal access. A historical rule cannot ask for an
  identity revealed by a future play, discard, or draw.
- `connection.rs` is the sole mutation boundary for delayed connections. It
  records starts, completions, invalidations, fixes, displacement, and
  supersession with a turn and reason. Every lifecycle has a stable
  `PromiseId` and immutable provenance, including its creation turn, actor,
  focus, expected identity, and connection kind. It also removes stale
  candidates from blocked later layers and rejects duplicate obligations.
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
  something was inferred; `ConventionFacts` is what downstream code queries.
  Ambiguous identities are retained as relational `OneOf` claims rather than
  being incorrectly copied onto every candidate card.
- `model.rs` owns `ConventionCardState`, the canonical grouping for clue,
  play, discard, invalidation, and fact state associated with cards.
- `epistemic.rs` is the strategic representation of identity domains one
  named observer may retain, including the provenance of each domain. Action
  obligations remain in the canonical convention state instead of being
  copied into a second, independently mutable status aggregate. It deliberately
  exposes neither deck order nor simulator truth.
- `outcome.rs` retains clue consequences before strategy converts genuine
  preferences to numeric ordering. Giver-visible team coverage and
  owner-relative promised-action and clued-card-superposition equivalence for
  Directness remain separate projections of one outcome.
- `rule_engine.rs` owns ordered post-event execution. Real replay and
  prospective transitions both enter this registry through the same history
  reducer. Each `RuleSpec` declares its semantic phase and dependencies, and
  diagnostic reductions retain every non-empty contribution as a
  `RuleProposal` in the event's `ConventionTransitionResult`; `rules.rs` proves
  every level has exactly one valid execution path.
- `identity.rs` and `hand.rs` own the shared identity, trash, playability,
  focus, chop, and finesse-position semantics used by giver, recipient, and
  planner paths.
- `constraints.rs` separates semantic obligations from utility. Urgent clues,
  connection responses, required discards, and must-clue states restrict the
  action set before numeric strategy priorities are compared.
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
  semantically admitted candidates become recipient-checked candidates and
  only then become causally compared, ranked candidates.
- `interpretation.rs` owns observer-relative clue meaning, convention card
  inference, and convention-admissible clue generation.
- `symbolic_line.rs` advances only convention-predictable actions. New draws
  remain blank, and projection stops at the first identity or policy branch.
- `recognition.rs` is now only the level-gated registry surface and shared
  imports. Cohesive modules own Basic moves, Tempo and emergency discards,
  Chop Moves, Bluffs, advanced connections, special discards, Trash moves,
  late-game rules, and Extras. Observer knowledge derivation and candidate
  validation likewise live in focused `interpretation/` modules. `h_group.rs`
  retains history reduction and connection scheduling.
- `transition.rs` is the production causal boundary. Every retained public
  event has an exact compact `ConventionTransitionDelta` of materialized card
  facts; rule proposals also retain exact card replacements rather than
  comparing collection lengths. Strategic evaluation consumes these deltas.
  Human-readable signals remain explanations rather than an alternative
  causality source.
- `strategic_value.rs` compares structured `LineOutcome` values. It may use
  teammate identities visible to the giver for team coverage, but Directness
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
- every active connection has registered promise provenance; and
- every materialized clue, protection, play, chop-move, and forced-play fact
  has at least one typed source;
- every promise-sourced retraction names registered promise provenance; and
- proposals are phase ordered, non-empty, tied to the event turn, and form a
  unique partition of post-event signals.

Architecture properties additionally require that leaked own-hand truth does
not change an observer's `EpistemicState`, and that candidate signal and hazard
inspection share one prospective convention snapshot.

The `game-194321.json` expert replay regression runs these checks for every
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
3. Register the family with the executable rule engine, emit a typed effect,
   and use `ConnectionManager` for connection changes.
4. Add current query state to `ConventionFacts` rather than searching the
   signal log at decision time.
5. Express mandatory behavior as a `ConventionConstraints` rule. Use a numeric
   priority only for a genuine strategic preference among admissible actions.
6. Test giver and recipient interpretations, prospective/retrospective
   equivalence, hidden-truth noninterference when relevant, and at least one
   replay prefix.

These boundaries are deliberately stricter than a collection of move-specific
helpers: most past bugs came from one interpretation path updating only part of
the shared state or consulting information from the wrong observer or time.
