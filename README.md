# Hanabi Engine

A Hanabi engine written in Rust. The first crate, `hanabi-core`, provides a
deterministic standard-game simulator and player-safe observations. Search,
belief modeling, conventions, and integration with Hanabi Live will be layered
on top of this rules core.

## Architecture

- `FullState`: authoritative simulator truth, including hidden identities and
  deck order.
- `PlayerView`: the legal observation projected for one player.
- `LogicalDeductions`: convention-free certainties derived from direct clues,
  visible cards, and exact card-count elimination.
- `InformationSet`: search-layer constraints and card-copy-weighted sampling of
  worlds consistent with a `PlayerView`.
- `ConventionInferences`: a closed, typed registry of framework-specific
  interpretations kept separate from logical truth.

Action-selection code must consume `PlayerView`, `LogicalDeductions`, or the
equivalent compact rollout observation, never `FullState`.

The workspace currently contains:

- `hanabi-cli`: the `hanabi-engine` executable for analyzing actionable turns
  from Hanabi Live replay JSON.
- `hanabi-core`: deterministic rules, event history, observations, and sampled
  world reconstruction.
- `hanabi-search`: direct-clue/card-count information sets, reproducible
  determinization, and a convention-agnostic rollout baseline. The baseline
  acts only on certainly playable or useless cards, then falls back to the
  oldest discard or (at full clues) the newest blind play. It never clues.
  Logical feasibility uses compact 25-bit identity sets and exact Hall-capacity
  matching. Exact assignment counts and card-copy sampling weights are
  initialized lazily, with packed memoization keys and a reusable validated
  determinization template. Rollouts incrementally retain clue facts and use a
  compact history-free observation until convention-aware policy work begins. A
  flat Monte Carlo evaluator compares every legal root action on the same
  stream of sampled worlds and reports official score, raw stack progress,
  terminal utility, and strikeout statistics. Search utility is
  `official_score * 26 + raw_stack_score`: official score remains primary,
  while raw progress distinguishes otherwise identical strikeout outcomes. A
  single-observer ISMCTS implementation adds availability-aware UCB tree
  selection, expansion, rollout, cooperative backpropagation, and robust-child
  root selection. Search traversal reuses legal-action buffers, while the
  non-diagnostic APIs avoid profiling timers entirely.
- `SupportedConvention`: the built-in convention registry. `none` preserves the
  convention-agnostic baseline. Every framework supplies both rollout behavior
  and root-world sampling, so future convention beliefs cannot be applied to
  one part of search and accidentally omitted from the other.
- `hanabi-protocol`: Hanabi Live compact replay parsing for standard games.

The Hanabi Live compatibility test uses the sibling `hanabi-live` repository
when it is available and otherwise skips that cross-repository assertion.

## Development

```sh
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
```

## Analyze a Hanabi Live position

Turn `N` means the position after `N` completed game actions; turn zero is the
initial deal. Run searches in release mode for representative throughput.

```sh
cargo run --release -p hanabi-cli --bin hanabi-engine -- \
  analyze /path/to/replay.json --turn 17 --convention none \
  --iterations 10000 --seed 42

cargo run --release -p hanabi-cli --bin hanabi-engine -- \
  analyze /path/to/replay.json --turn 17 --convention none \
  --mode flat --samples 100 --seed 42
```

The report marks the selected action with `*`, uses slot 1 for the newest card,
and includes official score, raw stack score, terminal utility, strikeout,
visit, action-availability, and throughput data.

Applications with an arbitrary legal `PlayerView` can use the library façade:

```rust
let result = hanabi_search::best_move(
    view,
    hanabi_search::SupportedConvention::None,
    hanabi_search::SearchConfig::Ismcts(hanabi_search::IsmctsConfig {
        iterations: 10_000,
        exploration: core::f64::consts::SQRT_2,
        seed: 42,
    }),
)?;
```

The same entry point accepts `SearchConfig::Flat`. Lower-level search APIs also
accept any Rust type implementing `ConventionFramework`, while user-facing
configuration remains the closed `SupportedConvention` enum.

## Benchmark search

The benchmark command runs both ISMCTS and flat Monte Carlo over one or more
fixed replay positions. Each trial uses the next consecutive seed, making
selected actions and search statistics reproducible while still sampling
multiple hidden worlds.

```sh
cargo run --release -p hanabi-cli --bin hanabi-engine -- \
  benchmark /path/to/replay.json \
  --turn 0 --turn 17 \
  --convention none --trials 5 \
  --iterations 10000 --samples 100 --seed 42 \
  > benchmark.json
```

The versioned JSON report records the selected convention and contains every
trial's selected action, mean official score, raw stack score, terminal utility,
strikeout rate, work count, elapsed
time, and throughput. It also aggregates selection frequencies and reports
`action_stability`, the fraction of trials choosing the most common action.
Per-trial diagnostics break the work down into sampled worlds, explicit
candidate-state clones, expanded tree nodes, search actions, rollouts, rollout
turns, maximum tree depth, and time spent sampling, traversing the tree, and
rolling out. Rollout time is further separated into observation, logical
deduction, policy selection, action application, and remaining loop overhead.
Wall-clock fields are diagnostic; seeded actions and score statistics are the
appropriate fields for behavioral regression checks.
