# Hanabi Engine

An engine for standard five-suit Hanabi. It analyzes legal player
observations under a selected convention and can connect a bot to
[hanab.live](https://hanab.live).

The engine includes:

- deterministic game logic and player-safe observations;
- a deterministic symbolic planner with exhaustive late-game solving;
- a deliberately convention-agnostic baseline;
- cumulative H-Group profiles from Level 1 through Level 25, plus `max` as the
  effective Level 26;
- Hanabi Live replay analysis and a persistent online bot.

Only Hanabi Live `No Variant` games are currently supported.

## Build and test

Rust 1.85 or newer is required. From WSL or another Linux environment:

```sh
cargo build --release --locked
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
cargo test --workspace --all-targets --all-features --locked
```

Run `scripts/check.sh` for the complete local CI suite, including Rust
documentation, Python typing/tests, and closed-workspace dead-code analysis.
The additional setup is documented in the
[technical overview](docs/technical-overview.md#development-and-ci).

## Example usage

Analyze turn 17 of a Hanabi Live replay with H-Group max. The default planner
keeps unknown identities symbolic and automatically switches to exact endgame
search when the complete belief is small enough:

```sh
cargo run --release -p hanabi-cli --bin hanabi-engine -- \
  analyze /path/to/replay.json --turn 17 \
  --convention h-group --h-group-level max \
  --objective perfect-score
```

Run `cargo run -p hanabi-cli --bin hanabi-engine -- --help` for all CLI options.

## Try the bot on Hanabi Live

Use a dedicated bot account. Build the engine, create a Python environment, and
install the bridge dependency:

```sh
cargo build --release --locked

# Install python3-venv first if your distribution does not include it.
python3 -m venv .venv
. .venv/bin/activate
python -m pip install -r scripts/requirements.txt
```

Provide the bot credentials without saving them in the repository:

```sh
export HANABI_USERNAME="your-bot-account"
read -rsp "Hanabi Live password: " HANABI_PASSWORD
export HANABI_PASSWORD
echo

python scripts/hanabi_live_bot.py
```

The bot defaults to H-Group `max` and deterministic planning.

Create a public `No Variant` table on Hanabi Live, then privately invite the
bot:

```text
/msg your-bot-account /join
```

Any player at the table can select or inspect its convention level:

```text
/msg your-bot-account /level 3
/msg your-bot-account /level
```

Levels `1` through `25` and `max` are accepted. The bot reconnects to an
ongoing game after a network interruption or launcher restart. `Ctrl+C` and
`SIGTERM` shut it down cleanly. Player-safe snapshots and decision logs are
written to `logs/hanabi-live/` by default.

## Documentation

- [Self-play benchmark](docs/self-play.md): the opt-in 200-seed strength test,
  reports, and baseline updates.
- [Technical overview](docs/technical-overview.md): architecture, planning,
  conventions, APIs, bridge internals, diagnostics, and CI.
- [H-Group architecture](docs/architecture.md): reducer boundaries,
  connection lifecycle, invariants, and extension guidance.
- [H-Group convention interpreter](docs/h-group.md): level matrix and
  convention algorithm.
- [H-Group documentation coverage](docs/h-group-coverage.md): pinned source
  revision, section inventory, and coverage enforcement.
