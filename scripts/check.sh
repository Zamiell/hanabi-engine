#!/usr/bin/env bash

set -euo pipefail # Exit on errors and undefined variables.

check_started_seconds=$SECONDS

repository_root="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repository_root"

check_file_ownership() {
  local expected_owner
  local expected_group
  local repository_parent
  repository_parent="$(dirname -- "$repository_root")"
  expected_owner="$(stat --format='%U' -- "$repository_parent")"
  expected_group="$(stat --format='%G' -- "$repository_parent")"

  local -a mismatched_paths=()
  mapfile -d '' mismatched_paths < <(
    find "$repository_root" -xdev \
      \( ! -user "$expected_owner" -o ! -group "$expected_group" \) \
      -print0
  )
  if (( ${#mismatched_paths[@]} == 0 )); then
    return
  fi

  printf >&2 \
    'Repository paths must be owned by %s:%s; found ownership mismatches:\n' \
    "$expected_owner" "$expected_group"
  printf >&2 '  %s\n' "${mismatched_paths[@]}"
  return 1
}

echo 'Checking repository file ownership...'
check_file_ownership

if ! command -v npm >/dev/null 2>&1 || [[ ! -x node_modules/.bin/prettier ]]; then
  printf >&2 'Prettier development dependencies are missing. Install Node.js/npm, then run npm ci in the repository root.\n'
  exit 1
fi

echo 'Checking repository formatting with Prettier...'
npm run format:check

if [[ -d "$HOME/.cargo/bin" ]]; then
  export PATH="$HOME/.cargo/bin:$PATH"
fi

if [[ ! -x .venv/bin/python || ! -x .venv/bin/ty ]]; then
  cat >&2 <<'EOF'
Python development dependencies are missing. Set them up with:

  python3 -m venv .venv
  .venv/bin/python -m pip install --requirement scripts/requirements-dev.txt
EOF
  exit 1
fi

if ! rustup run 1.97.1 cargo hawk --version >/dev/null 2>&1; then
  cat >&2 <<'EOF'
cargo-hawk or its Rust toolchain is missing. Set them up with:

  rustup toolchain install 1.97.1 --profile minimal
  curl --proto '=https' --tlsv1.2 -LsSf \
    https://github.com/astral-sh/hawk/releases/download/0.1.10/cargo-hawk-installer.sh \
    | sh
EOF
  exit 1
fi

if ! cargo nextest --version >/dev/null 2>&1; then
  cat >&2 <<'EOF'
cargo-nextest 0.9.143 is missing. Install its prebuilt WSL/Linux binary with:

  curl --proto '=https' --tlsv1.2 -LsSf \
    https://get.nexte.st/0.9.143/linux \
    | tar zxf - -C "$HOME/.cargo/bin"
EOF
  exit 1
fi

echo 'Checking Rust formatting...'
cargo fmt --all -- --check

echo 'Building all Rust targets...'
cargo build --workspace --all-targets --all-features --locked

echo 'Running Clippy...'
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings

echo 'Running fast Rust tests with fail-fast scheduling...'
cargo nextest run \
  --workspace \
  --all-targets \
  --all-features \
  --locked \
  --fail-fast

# Nextest deliberately does not run rustdoc tests.
echo 'Running Rust documentation tests...'
cargo test --workspace --all-features --doc --locked

echo 'Checking Rust documentation...'
RUSTDOCFLAGS='-D warnings' \
  cargo doc --workspace --all-features --no-deps --locked

echo 'Type-checking Python...'
.venv/bin/ty check

echo 'Compiling Python sources...'
.venv/bin/python -m py_compile \
  scripts/hanabi_live_bot.py \
  scripts/hanabi_live_engine.py \
  scripts/hanabi_live_game.py \
  scripts/hanabi_live_trace.py \
  scripts/tests/test_hanabi_live_bot.py

echo 'Running Python tests...'
.venv/bin/python -W error::ResourceWarning -m unittest discover -s scripts/tests -v

echo 'Checking the Hanabi Live bot CLI...'
.venv/bin/python scripts/hanabi_live_bot.py --help >/dev/null

echo 'Checking workspace-wide dead public code...'
cargo +1.97.1 hawk check \
  --manifest-path Cargo.toml \
  --only dead-public \
  -D warnings

echo 'Rechecking repository file ownership...'
check_file_ownership

check_elapsed_seconds=$((SECONDS - check_started_seconds))
printf 'All checks passed in %dm %ds.\n' \
  "$((check_elapsed_seconds / 60))" "$((check_elapsed_seconds % 60))"
