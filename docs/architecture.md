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
   |-- ConventionEffect       typed rule output
   |-- ConnectionManager      Prompt/Finesse lifecycle state machine
   |-- HGroupSignal           append-only explanation log
   `-- ConventionFacts        indexed current convention truth
   |
   v
ConventionConstraints        hard required/admissible actions
   |
   v
strategy priority            ranks only convention-admissible actions
   |
   v
symbolic planner / exact endgame solver
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
  supersession with a turn and reason. It also removes stale candidates from
  blocked later layers and rejects duplicate obligations.
- `effects.rs` reduces typed recognized effects idempotently. Rule recognizers
  describe effects; they do not append duplicate signals directly.
- `facts.rs` separates current truth from history. `HGroupSignal` explains why
  something was inferred; `ConventionFacts` is what downstream code queries.
- `constraints.rs` separates semantic obligations from utility. Urgent clues,
  connection responses, required discards, and must-clue states restrict the
  action set before numeric strategy priorities are compared.
- `perspective.rs` owns observer projection and hypothetical public
  transitions.
- `prospective.rs` checks a proposed action from affected players'
  perspectives.
- `decision.rs` builds the one canonical action analysis consumed by direct
  selection and planning.

The remaining functions in `h_group.rs` are rule recognition and shared card
semantics. New rule families should be placed in a focused module once they
need independent state or more than a small recognizer.

## Invariants

Every completed replay reduction validates that:

- an active connection has at least one candidate;
- every active connection was scheduled through the lifecycle manager;
- the same actor/focus/step obligation is not duplicated; and
- every connection candidate is still in the promised actor's hand.

The expert replay regression runs these checks for every prefix and every
observer. Temporal tests separately assert that future own-card reveals and
future draws cannot affect an earlier interpretation. Perspective tests should
construct hypotheses through `PerspectiveProjector` rather than manually
editing a recipient view.

## Adding a convention rule

1. Identify the documented level and add or reuse its `HGroupRuleId` gate.
2. Read only the required side of `HGroupTurnContext`; use `HistoricalView` for
   old identity questions.
3. Emit a typed effect and use `ConnectionManager` for connection changes.
4. Add current query state to `ConventionFacts` rather than searching the
   signal log at decision time.
5. Express mandatory behavior as a `ConventionConstraints` rule. Use a numeric
   priority only for a genuine strategic preference among admissible actions.
6. Test giver and recipient interpretations, a future-information
   noninterference case when relevant, and at least one replay prefix.

These boundaries are deliberately stricter than a collection of move-specific
helpers: most past bugs came from one interpretation path updating only part of
the shared state or consulting information from the wrong observer or time.
