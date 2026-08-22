# Hanabi Engine

A Hanabi engine written in Rust. The first crate, `hanabi-core`, provides a
deterministic standard-game simulator and player-safe observations. Search,
belief modeling, conventions, and integration with Hanabi Live will be layered
on top of this rules core.

## Architecture

- `FullState`: authoritative simulator truth, including hidden identities and
  deck order.
- `PlayerView`: the legal observation projected for one player.
- `InformationSet`: search-layer constraints and card-copy-weighted sampling of
  worlds consistent with a `PlayerView`.
- `ConventionInferences`: planned policy-layer interpretations of player intent.

Action-selection code must consume `PlayerView`, never `FullState`.

The workspace currently contains:

- `hanabi-cli`: the `hanabi-engine` executable for analyzing actionable turns
  from Hanabi Live replay JSON.
- `hanabi-core`: deterministic rules, event history, observations, and sampled
  world reconstruction.
- `hanabi-search`: direct-clue/card-count information sets, reproducible
  determinization, and a convention-agnostic rollout baseline. The baseline
  acts only on certainly playable or useless cards, then falls back to the
  oldest discard or (at full clues) the newest blind play. It never clues.
  Logical feasibility is computed on the fast policy path; exact assignment
  counts and sampling weights are initialized lazily only when sampling. A
  flat Monte Carlo evaluator compares every legal root action on the same
  stream of sampled worlds and reports score and strikeout statistics. A
  single-observer ISMCTS implementation adds availability-aware UCB tree
  selection, expansion, rollout, cooperative backpropagation, and robust-child
  root selection.
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
  analyze /path/to/replay.json --turn 17 --iterations 10000 --seed 42

cargo run --release -p hanabi-cli --bin hanabi-engine -- \
  analyze /path/to/replay.json --turn 17 --mode flat --samples 100 --seed 42
```

The report marks the selected action with `*`, uses slot 1 for the newest card,
and includes score, strikeout, visit, action-availability, and throughput data.
