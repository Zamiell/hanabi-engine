# Hanabi Engine

A Hanabi engine written in Rust. The workspace includes a deterministic
standard-game simulator, player-safe observations, Hanabi Live replay parsing,
hidden-world sampling, flat Monte Carlo and ISMCTS search, a deliberately
convention-free baseline, and cumulative H-Group convention profiles.

The current rules implementation targets standard five-suit Hanabi. The replay
adapter accepts Hanabi Live no-variant games; variant-specific rules are not yet
modeled.

## Architecture

- `FullState`: authoritative simulator truth, including hidden identities and
  deck order.
- `PlayerView`: the legal observation projected for one player.
- `LogicalDeductions`: convention-free certainties derived from direct clues,
  visible cards, and exact card-count elimination.
- `InformationSet`: search-layer identity constraints and card-copy-weighted
  sampling of worlds consistent with a `PlayerView`.
- `ConventionInferences`: a closed, typed registry of framework-specific
  interpretations kept separate from logical truth.

Policy decisions consume `PlayerView`, `LogicalDeductions`, or the equivalent
compact rollout observation. Search uses sampled `FullState` values to play
hypothetical games forward, but hidden simulator truth is never exposed to a
player's decision policy.

The workspace contains:

- `hanabi-cli`: the `hanabi-engine` executable for analyzing actionable turns
  from Hanabi Live replay JSON, producing live-game actions, and benchmarking
  both search modes.
- `hanabi-core`: deterministic rules, complete state, event history, legal
  player observations, and world determinization.
- `hanabi-protocol`: Hanabi Live compact replay parsing plus player-safe live
  action-stream reconstruction for standard no-variant games.
- `hanabi-search`: logical deduction, information sets, convention
  interpretation, rollout policies, flat Monte Carlo, ISMCTS, diagnostics, and
  the high-level `best_move` API.

The convention-free baseline plays the oldest certainly playable card.
Otherwise, when discarding is legal, it discards the oldest certainly useless
card or falls back to the oldest card. At full clue tokens, when discarding is
illegal, it blind-plays the newest card. It never gives or interprets a clue.

Logical feasibility uses compact 25-bit identity sets and exact Hall-capacity
matching. Assignment counts and card-copy sampling weights are initialized
lazily, with packed memoization keys and a reusable validated determinization
template. Convention-free rollouts use a compact history-free observation;
H-Group rollouts retain public history and convention state.

Flat Monte Carlo compares every legal root action on the same stream of sampled
worlds. Single-observer ISMCTS uses availability-aware UCB selection,
expansion, rollout, cooperative backpropagation, and robust-child root
selection. Both report official score, raw stack progress, terminal utility,
strikeout statistics, and optional diagnostic timings. Search utility is
`official_score * 26 + raw_stack_score`: official score remains primary, while
raw progress distinguishes otherwise identical strikeout outcomes.

## Convention support

`SupportedConvention` is the closed registry used by the CLI and high-level
API. `none` selects the convention-agnostic baseline. `h-group` requires a
cumulative profile: levels 1 through 25 include all preceding levels, and
`max` is the effective 26th level. The single machine-readable
`H_GROUP_LEVELS` catalog contains all 26 entries.

Every convention framework supplies rollout behavior,
convention-permitted search actions, and root-world sampling. This prevents a
clue from being interpreted under one belief model while search samples worlds
under another. H-Group is pinned to the documentation revision exposed as
`H_GROUP_RULESET_REVISION`. Its event-history interpreter records typed signals
for play/save clues, connections, chop movement, tempo and stalls, special
discards, bluffs, ejections and discharges, elimination, 5 tech, ignition,
charms, and Priority. Ambiguous Prompt and layered-Finesse promises are sampled
as exact mutually exclusive branches. Convention-inconsistent arbitrary inputs
fall back to logical world sampling and safe default card behavior.

See [the H-Group interpreter design](docs/h-group.md) for the complete level
matrix and algorithm.

## Development

The workspace uses Rust 2024 and supports Rust 1.85 or newer. Development from
WSL works normally as long as the Rust toolchain is installed inside the WSL
distribution.

The checks below match the GitHub Actions workflow:

```sh
cargo fmt --all -- --check
cargo build --workspace --all-targets --all-features --locked
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
cargo test --workspace --all-targets --all-features --locked
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --all-features --no-deps --locked
cargo +1.85.0 check --workspace --all-targets --all-features --locked

python3 -m venv .venv
.venv/bin/python -m pip install --requirement scripts/requirements.txt
.venv/bin/python -m py_compile scripts/hanabi_live_bot.py scripts/tests/test_hanabi_live_bot.py
.venv/bin/python -W error::ResourceWarning -m unittest discover -s scripts/tests -v
.venv/bin/python scripts/hanabi_live_bot.py --help
```

The Hanabi Live compatibility tests use the sibling `hanabi-live` repository
when it is available and skip only those cross-repository assertions when it is
absent.

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

cargo run --release -p hanabi-cli --bin hanabi-engine -- \
  analyze /path/to/replay.json --turn 17 --convention h-group \
  --h-group-level 5 --iterations 10000 --seed 42

cargo run --release -p hanabi-cli --bin hanabi-engine -- \
  analyze /path/to/replay.json --turn 17 --convention h-group \
  --h-group-level max --iterations 10000 --seed 42
```

`--h-group-level` accepts `1` through `25`, or `max`. Level 5 means levels 1-5
cumulatively; `max` is the effective cumulative level 26. H-Group analysis
output records the source documentation revision implemented by the engine.
Game variants remain game-state metadata rather than convention-selection
metadata, so the convention interpreter cannot be configured for a different
variant than the simulator.

The report marks the selected action with `*`, uses slot 1 for the newest card,
and includes official score, raw stack score, terminal utility, strikeout,
visit, action-availability, and throughput data.

Applications with an arbitrary legal `PlayerView` can use the library facade:

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

## Play on Hanabi Live

The online adapter has two deliberately separate pieces:

1. `scripts/hanabi_live_bot.py` handles login, the authenticated WebSocket,
   lobby invitations, and the server's scrubbed action stream.
2. A persistent `hanabi-engine live-session` process per table reconstructs a
   player-safe `PlayerView` once, incrementally applies new actions, searches
   it, and emits Hanabi Live actions as newline-delimited JSON. The
   `live-action` command remains available for one-shot analysis and testing.

This keeps credentials and the changing network protocol outside the search
engine, while the Rust boundary prevents the bot from filling its own hidden
cards with simulator truth. The live command defaults to ISMCTS with 1,000
iterations and H-Group `max`. Searches run on background workers, independently
per table, so the WebSocket receive loop remains responsive. A failed engine
session is restarted from the complete scrubbed snapshot, and a dropped server
connection is reauthenticated with bounded exponential backoff.

Use a dedicated Hanabi Live bot account, then build the release binary and set
up the one Python dependency:

```sh
cargo build --release --locked

# On Ubuntu/WSL, install this first if the venv module is unavailable:
# sudo apt install python3-venv  # or the versioned package, e.g. python3.14-venv
python3 -m venv .venv
. .venv/bin/activate
python -m pip install -r scripts/requirements.txt
```

Pass credentials through the environment rather than storing them in the
repository:

```sh
export HANABI_USERNAME="your-bot-account"
read -rsp "Hanabi Live password: " HANABI_PASSWORD
export HANABI_PASSWORD
echo

python scripts/hanabi_live_bot.py
```

In a browser, create a public `No Variant` table with room for the bot, then
privately message it:

```text
/msg your-bot-account /join
```

The launcher supports `--iterations`, `--mode`, `--seed`,
`--h-group-level 1` through `--h-group-level 25`, and
`--h-group-level max`. Use `--convention none` to exercise the
convention-agnostic baseline. `--engine-timeout` bounds one search attempt.
`--base-url` can point at a local Hanabi Live server for integration testing;
the default is `https://hanab.live`.

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

The schema-version-4 JSON report records the selected convention and contains
every trial's selected action, mean official score, raw stack score, terminal
utility, strikeout rate, work count, elapsed time, and throughput. An H-Group
profile is recorded as one effective `level` value, including `26` for `max`.
The report also aggregates selection frequencies and reports
`action_stability`, the fraction of trials choosing the most common action.
Per-trial diagnostics break work down into sampled worlds, explicit
candidate-state clones, expanded tree nodes, search actions, rollouts, rollout
turns, maximum tree depth, and time spent sampling, traversing the tree, and
rolling out. Rollout time is further separated into observation, logical
deduction, policy selection, action application, and remaining loop overhead.
Wall-clock fields are diagnostic; seeded actions and score statistics are the
appropriate fields for behavioral regression checks.
