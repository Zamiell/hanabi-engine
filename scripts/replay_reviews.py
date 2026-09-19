"""Look up human review notes without running the engine or modifying fixtures."""

import argparse
import hashlib
import json
from pathlib import Path
from typing import Any

ROOT = Path(__file__).resolve().parents[1]
FIXTURES = ROOT / "crates/hanabi-protocol/tests/fixtures"
REVIEWS = ROOT / "docs/replay-reviews"


def position_key(game: dict[str, Any], turn: int) -> str:
    """Bind to all setup fields and preceding actions, not the proposed move.

    Include unknown setup fields conservatively. Formatting/key order and later
    moves do not invalidate a review; any earlier action or setup edit does.
    """
    actions = game["actions"]
    if turn < 1 or turn > len(actions) + 1:
        raise ValueError("turn must identify a position in the replay")
    position = {**game, "actions": actions[:turn - 1]}
    canonical = json.dumps(position, sort_keys=True, separators=(",", ":"))
    return hashlib.sha256(canonical.encode()).hexdigest()


def anchor_status(entry: dict[str, Any], game: dict[str, Any]) -> str:
    if entry["positionKey"] is None:
        return "UNANCHORED: historical context only"
    try:
        current = position_key(game, entry["turn"])
    except ValueError:
        return "STALE: turn no longer exists"
    return "MATCH" if current == entry["positionKey"] else "STALE: position changed"


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("seed")
    parser.add_argument("--turn", type=int, required=True, help="Hanab Live one-based turn")
    args = parser.parse_args()
    # Resolve only filenames in the fixture directory, never arbitrary paths.
    if Path(args.seed).name != args.seed or "/" in args.seed or "\\" in args.seed:
        parser.error("seed must be a filename stem, not a path")
    try:
        game = json.loads((FIXTURES / f"game-{args.seed}.json").read_text())
        key = position_key(game, args.turn)
        notes = REVIEWS / f"{args.seed}.json"
        entries = json.loads(notes.read_text()) if notes.exists() else []
        print(f"{args.seed}, turn {args.turn}\npositionKey: {key}")
        print("Position match is not approval or proof of regression coverage.")
        matching = [entry for entry in entries if entry["turn"] == args.turn]
        if not matching:
            print("No recorded review for this turn. Search the ledger/history before asking again.")
        for entry in matching:
            print(f"\n{entry['id']}: {anchor_status(entry, game)}")
            print(json.dumps(entry, indent=2))
        others = [entry["id"] for entry in entries if entry["turn"] != args.turn]
        if others:
            print("\nOther reviews in this replay: " + ", ".join(others))
        return 0
    except (OSError, ValueError, KeyError, TypeError) as error:
        parser.exit(1, f"Review lookup failed: {error}\n")


if __name__ == "__main__":
    raise SystemExit(main())
