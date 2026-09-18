#!/usr/bin/env bash

set -euo pipefail # Exit on errors and undefined variables.

check_started_seconds=$SECONDS
check_failed=0

# Complete the full suite even when a stage fails. Re-running the omitted tail
# manually delays feedback and makes full-validation timings misleading.
run_check() {
  local label="$1"
  shift
  local started=$SECONDS
  local status=0
  "$@" || status=$?
  printf 'CHECK TIMING: %s: %ds (exit %d)\n' \
    "$label" "$((SECONDS - started))" "$status"
  if (( status != 0 )); then
    check_failed=1
  fi
}

repository_root="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repository_root"

mode=full
case "${1:-}" in
  "") ;;
  --fast) mode=fast; shift ;;
  --help)
    echo 'Usage: scripts/check.sh [--fast]'
    echo 'Default: full validation. --fast: all stages except the ordinary Rust test suite.'
    echo 'Run focused Rust tests separately; fast success is not full validation.'
    exit 0 ;;
  *) echo "Unknown option: $1" >&2; exit 2 ;;
esac
if (( $# != 0 )); then
  echo 'Unexpected arguments; see --help.' >&2
  exit 2
fi
if [[ "${HANABI_CHECK_TIMED:-0}" != 1 ]]; then
  args=()
  if [[ "$mode" == fast ]]; then args+=(--fast); fi
  exec env HANABI_CHECK_TIMED=1 python3 scripts/workflow.py run \
    --label "check-$mode" -- bash scripts/check.sh "${args[@]}"
fi

# Full validations compete for the same CPU and build cache. Do not silently
# queue or overlap another full run, which also invalidates timing comparisons.
mkdir -p target
exec 9>target/check.lock
if ! flock --nonblock 9; then
  echo 'Another check.sh run is already active in this repository.' >&2
  exit 1
fi

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
run_check 'Initial ownership' check_file_ownership

if ! command -v npm &> /dev/null; then
  echo 'npm is required to run this script.' >&2
  exit 1
fi

if [[ ! -x node_modules/.bin/prettier ]]; then
  echo 'Prettier was not found. Run "npm ci" in the repository root.' >&2
  exit 1
fi

echo 'Checking repository formatting with Prettier...'
run_check 'Prettier' npm run format:check

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

if [[ "$mode" == full ]] && ! cargo nextest --version >/dev/null 2>&1; then
  cat >&2 <<'EOF'
cargo-nextest 0.9.143 is missing. Install its prebuilt WSL/Linux binary with:

  curl --proto '=https' --tlsv1.2 -LsSf \
    https://get.nexte.st/0.9.143/linux \
    | tar zxf - -C "$HOME/.cargo/bin"
EOF
  exit 1
fi

echo 'Checking Rust formatting...'
run_check 'Rust formatting' cargo fmt --all -- --check

echo 'Building all Rust targets...'
run_check 'Rust build' cargo build --workspace --all-targets --all-features --locked

echo 'Running Clippy...'
run_check 'Clippy' cargo clippy --workspace --all-targets --all-features --locked -- -D warnings

echo 'Running all ordinary Rust tests...'
if [[ "$mode" == full ]]; then
run_check 'Rust tests' cargo nextest run \
  --workspace \
  --all-targets \
  --all-features \
  --locked \
  --no-fail-fast
else
  echo 'FAST CHECK: ordinary Rust tests skipped; run focused tests for this change.'
fi

# Nextest deliberately does not run rustdoc tests.
echo 'Running Rust documentation tests...'
run_check 'Rust documentation tests' cargo test --workspace --all-features --doc --locked

echo 'Checking Rust documentation...'
run_check 'Rust documentation' env RUSTDOCFLAGS='-D warnings' \
  cargo doc --workspace --all-features --no-deps --locked

echo 'Type-checking Python...'
run_check 'Python types' .venv/bin/ty check

echo 'Compiling Python sources...'
run_check 'Python compilation' .venv/bin/python -m py_compile \
  scripts/hanabi_live_bot.py \
  scripts/hanabi_live_engine.py \
  scripts/hanabi_live_game.py \
  scripts/hanabi_live_trace.py \
  scripts/workflow.py \
  scripts/tests/test_hanabi_live_bot.py

echo 'Running Python tests...'
run_check 'Python tests' .venv/bin/python -W error::ResourceWarning -m unittest discover -s scripts/tests -v

echo 'Checking the Hanabi Live bot CLI...'
run_check 'Bot CLI' bash -c '.venv/bin/python scripts/hanabi_live_bot.py --help >/dev/null'

echo 'Checking workspace-wide dead public code...'
run_check 'Dead public code' cargo +1.97.1 hawk check \
  --manifest-path Cargo.toml \
  --only dead-public \
  -D warnings

echo 'Rechecking repository file ownership...'
run_check 'Final ownership' check_file_ownership

check_elapsed_seconds=$((SECONDS - check_started_seconds))
if (( check_failed != 0 )); then
  printf '%s checks failed; completed in %dm %ds.\n' "$mode" \
    "$((check_elapsed_seconds / 60))" "$((check_elapsed_seconds % 60))"
  exit 1
fi
printf '%s checks passed in %dm %ds.\n' "$mode" \
  "$((check_elapsed_seconds / 60))" "$((check_elapsed_seconds % 60))"
