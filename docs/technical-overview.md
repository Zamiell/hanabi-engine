# Technical overview

Hanabi Engine is a deterministic belief-state planner for standard five-suit
Hanabi. For installation and common commands, start with the
[README](../README.md).

## Workspace

- `hanabi-core` implements cards, legal actions, authoritative game state,
  event history, and player-safe observations.
- `hanabi-protocol` reconstructs games from Hanabi Live replay and live-action
  formats.
- `hanabi-search` derives logical knowledge, interprets conventions, and plans
  actions.
- `hanabi-cli` exposes replay analysis and the persistent protocol used by the
  online bridge.

Only Hanabi Live `No Variant` games are currently supported.

## State and knowledge

- `FullState` is simulator truth, including hidden identities and deck order.
- `PlayerView` is everything one player may legally observe.
- `LogicalDeductions` derives convention-free identity domains from direct
  clues, visible cards, and card-count elimination.
- `InformationSet` represents correlated hidden identities and can count or
  enumerate complete consistent worlds.
- `ConventionInferences` stores interpretations such as focus, saves,
  finesses, chop movement, and connection promises separately from logical
  facts.
- A Finesse card separately retains its convention-compatible physical domain
  and its exact promised identity. For example, a card promised as yellow 1
  may physically be another card that will successfully blind-play; the
  connection still requires the player to act on the yellow-1 promise.

Hidden simulator truth is never passed to an action-selection policy.

Logical feasibility uses compact 25-bit identity sets and Hall-capacity
matching. Convention constraints are intersected with those domains, including
mutually exclusive branches for ambiguous connections.

## Deterministic planning

The engine has one planning path. In openings and midgames, unknown identities
remain symbolic. Every convention-permitted root action receives a stable
priority derived from convention semantics and public facts: certain
playability or uselessness, newly touched cards, immediately playable touches,
critical-card protection, and oldest-card protection. Identical input therefore
produces identical output without a random seed or iteration budget.

When convention priority and explicit preference do not separate root actions,
the planner projects each convention-predictable continuation. Unknown draws
remain blank rather than receiving sampled identities, and the projection stops
at the first real policy or identity branch. The reported trajectory compares
score gain, strikes, discards, and clue flow without pretending to know hidden
cards.

Before attempting an exact endgame, the planner counts worlds only up to
`--exact-world-limit` (4096 by default) and performs a conservative complexity
preflight against `--exact-node-limit` (50000 by default). If the belief is too
large, it stays entirely symbolic.

When both gates pass, the planner enumerates every admitted own-hand and deck
identity assignment and solves through the final turn. Exact recursion groups
worlds by the acting player's complete `PlayerView` and selects one action per
observation group. This avoids strategy fusion: a player cannot act on their
own identity or a future draw before it becomes observable.

Convention-forced continuations collapse to one action. For `perfect-score`,
exact values compare perfect worlds first, then total official score, fewer
strikeouts, and reachable score ceiling. `expected-score` compares total score
first. If the node ceiling is reached, partial exact results are discarded and
the symbolic result is used.

Planning details report the `symbolic` or `exact` phase, bounded world count,
exact node count, root priorities, and exact action values when available.

## Convention architecture

`SupportedConvention` is the user-facing registry. `none` uses direct logic
only. `h-group` requires a cumulative level from 1 through 25, or `max`, which
is treated as effective level 26. `H_GROUP_LEVELS` is the machine-readable
catalog and `H_GROUP_RULESET_REVISION` pins the implemented documentation. See
the [section-by-section H-Group coverage audit](h-group-coverage.md) for the
pinned 357-section inventory and its enforcement rules.

`SupportedConvention::analyze` returns one `ConventionAnalysis` containing:

- typed convention inferences;
- an ordered candidate list with structured semantic values and admission
  reasons;
- a typed rejection reason for every excluded legal clue;
- preferred and convention-forced actions; and
- hard identity constraints for exact endgame planning.

H-Group uses shared internal boundaries to keep these answers consistent:

- `PerspectiveProjector` reconstructs what another player could know.
- `EpistemicState` makes observer-owned identity domains and their provenance
  explicit without exposing simulator truth; action obligations stay in the
  canonical convention state.
- `HistoricalView` prevents future identity reveals from changing old moves.
- `HGroupTurnContext` exposes explicit pre-event and post-event facts.
- `ConnectionManager` owns the auditable Prompt/Finesse lifecycle.
- the provenance ledger, implemented by `ProvenancedCardSet`, gives every
  materialized clue, protection, play, and chop-move fact one or more event,
  rule, or `PromiseId` sources. Cancelling a promise atomically retracts only
  its own consequences.
- `ClueInterpretationPlan` is the single primary precedence decision for
  Play, Save, Fix, 5 Chop Move, and Stall meanings.
- typed effects update a `ConventionJournal` incrementally: signals preserve
  provenance, while relational `ConventionFacts` indexes only current truth.
- one executable rule registry dispatches historical and hypothetical events
  in the same semantic order; rule phases and dependencies are validated.
- `ConventionCardState` exposes compact materialized indexes backed by the
  provenance ledger rather than independently maintained truth sets.
- shared identity and hand modules give clue givers, recipients, hypothetical
  projection, and planning one definition of playability, trash, focus, chop,
  and finesse position.
- `ConventionConstraints` applies mandatory semantics before numeric strategy
  priorities.
- `LineOutcome` retains promised actions, protected cards, known trash, and
  connections so strategic comparisons operate on semantics before scores.
  Teamwork compares public action coverage and protection; Directness compares
  owner-relative promised actions and the identity superpositions on every
  clued card.
- `LineOutcome` causality comes from transition deltas, not by mining the
  explanation signal journal or diffing unrelated observer reconstructions.
- candidate primary-meaning checks, signal inspection, hazard checks, and
  strategic comparison reuse a lazy `TeamConventionSnapshot` for one coherent
  hypothetical public position.
- every production transition retains an exact compact delta of added and
  removed materialized card facts. Per-rule proposals cover explanation
  signals, promise transitions, and exact card replacements.
- connection promises have stable IDs and durable origin metadata rather than
  being inferred later from whichever card sets survived.
- clue selection has explicit semantic-admission, recipient-replay, and causal
  outcome-ranking stages.
- Convention-admissible Fix alternatives compare recipient-visible negative
  information using active promises, likely play timing, criticality, and
  future clue economy before applying the color-over-rank tie-break.
- `HGroupActionSet` is the canonical action analysis used by selection,
  candidate generation, priorities, safety checks, and continuation detection.
- otherwise-equivalent candidates use deterministic blank-card forced-line
  projection before exact endgame solving is considered.

See [H-Group architecture](architecture.md) for invariants and extension rules,
and [H-Group convention interpreter](h-group.md) for the level matrix.

The convention-free fallback plays the oldest certainly playable card. If
discarding is legal, it discards the oldest certainly useless card or otherwise
the oldest card. At full clue tokens it blind-plays the newest card. It never
gives or interprets clues.

## APIs and command line

Turn `N` is the position after `N` completed actions; turn zero is the initial
deal.

```sh
cargo run --release -p hanabi-cli --bin hanabi-engine -- \
  analyze /path/to/replay.json --turn 17 \
  --convention h-group --h-group-level max \
  --objective perfect-score
```

Applications with a legal `PlayerView` can use the library facade:

```rust
let result = hanabi_search::analyze_position(
    &view,
    hanabi_search::SupportedConvention::None,
    hanabi_search::PlannerConfig {
        objective: hanabi_search::PlanningObjective::PerfectScore,
        exact_world_limit: 4_096,
        exact_node_limit: 50_000,
    },
)?;
```

The same closed convention enum is used by the library, CLI, and live bridge,
so persisted selections and match dispatch remain exhaustive.

Replay analysis defaults to the convention-free policy and the
`expected-score` objective; H-Group analysis must specify both
`--convention h-group` and `--h-group-level`. The live commands instead default
to H-Group `max` and `perfect-score`. Add `--include-planning-details` to a live
command to receive the diagnostic action envelope used by the bridge.

## Hanabi Live bridge

The Python launcher owns authentication, the WebSocket, invitations, and
reconnection. Small bridge modules separately own per-table state, persistent
engine processes, and trace recording. A persistent
`hanabi-engine live-session` process reconstructs a player-safe `PlayerView`
from newline-delimited updates and returns one action. `live-action` is the
one-shot equivalent used for testing and snapshot reproduction.

Live play defaults to H-Group `max` and the `perfect-score` objective. A failed
engine session is rebuilt from the complete scrubbed snapshot. Reconnection
reattends ongoing tables, while `Ctrl+C` and `SIGTERM` stop cleanly.

Each launcher run writes player-safe traces under `logs/hanabi-live/`, including
configuration, scrubbed snapshots, engine requests and responses, and result
status. Credentials and hidden identities are not logged.

The launcher accepts exact world/node limits, convention and H-Group level,
objective, engine path and timeout, server URL, and debug logging. At the table,
`/level 3` changes that game's cumulative level and `/level` reports it.

## Development and CI

The workspace uses Rust 2024 and supports Rust 1.85 or newer. The checks used by
GitHub Actions are:

```sh
cargo fmt --all -- --check
cargo build --workspace --all-targets --all-features --locked
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
cargo nextest run --workspace --all-targets --all-features --locked --fail-fast
cargo test --workspace --all-features --doc --locked
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --all-features --no-deps --locked
cargo +1.97.1 hawk check --manifest-path Cargo.toml --only dead-public -D warnings
scripts/check-exhaustive.sh

.venv/bin/python -m pip install --requirement scripts/requirements-dev.txt
.venv/bin/ty check
.venv/bin/python -m py_compile \
  scripts/hanabi_live_bot.py \
  scripts/hanabi_live_engine.py \
  scripts/hanabi_live_game.py \
  scripts/hanabi_live_trace.py \
  scripts/tests/test_hanabi_live_bot.py
.venv/bin/python -W error::ResourceWarning -m unittest discover -s scripts/tests -v
```

Compatibility tests use the sibling `hanabi-live` repository when available
and skip only those cross-repository assertions when it is absent.

The ordinary suite keeps representative full-game rollouts for Levels 1, 10,
18, 25, and Max. The exhaustive script runs the independently scheduled
full-game matrix for every cumulative profile; CI runs it in a separate job.
Rust test binaries use `opt-level = 2` while retaining debug assertions and
overflow checks. The repository pins cargo-nextest 0.9.143 for fail-fast test
scheduling; nextest's missing rustdoc support is covered by the separate
`cargo test --doc` command.

The curated `game-194321.json` replay is an active golden oracle: the planner
must choose the fixture action at every position. Full parity for
`game-p4v0s9.json` remains ignored pending expert review of move 28; its
settled convention behaviors are still covered by focused active regressions.

The Python development requirements pin ty, and `ty.toml` checks every Python
bridge and test module against Python 3.10. The test suite also rejects any
function with a missing parameter or return annotation.

The workspace also enables Rust's `dead_code` and `unreachable_pub` lints.
Hawk treats the `hanabi-engine` binary declared in `hawk.toml` as the shipped
product and fails CI for public items that are dead across that closed world.
Visibility-narrowing suggestions remain an optional manual audit because
integration tests can legitimately require public APIs. CI pins Hawk 0.1.10
and the Rust 1.97.1 toolchain embedded in its Linux release binary because Hawk
uses version-specific compiler internals.
