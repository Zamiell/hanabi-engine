# Replay review memory

Store human explanations in `<seed>.json` beside this file. Keep importable
fixtures minimal: these records are reasoning and provenance, not game data or
inputs to engine move selection.

Before investigating a turn:

```bash
python3 scripts/replay_reviews.py p4v0s1 --turn 11
rg -n 'discard|finesse' docs/replay-reviews
```

The lookup prints the current position fingerprint and all notes for that turn,
including stale and historical notes. It does not run the engine. `MATCH` means
only that the setup and preceding actions match, not that the ruling is approved
or its tests pass. Changing the proposed move or later actions preserves the
anchor; changing setup or an earlier action invalidates it. The hash is over
canonical JSON, so whitespace and object key order do not matter.

## Recording a review

Each entry has a stable `id`, one-based `turn`, `positionKey`, `status`,
`perspective`, `source`, `explanation`, and `regressions`. Use status
`user-explained`, `unresolved`, or `superseded`. A source should contain a short
exact user excerpt and a conversation reference/date when available. Never
invent unavailable message IDs or dates. Record hypothetical branches explicitly
and distinguish what the player knows from simulator truth. Include assumptions,
candidate actions, intended line, and rejected reasoning where relevant.

For a new current-position review, copy the key printed by the lookup. If only a
historical explanation is recoverable and its exact position cannot be verified,
use `null`, explain the uncertainty, and treat it as historical context only.
Never bind an old explanation to today's position just because the turn matches.

When the user revises a ruling, preserve it as `superseded` and add a new record
referencing the old ID. Do not automatically update stale hashes. Verify the
prefix/knowledge assumptions first; if necessary, recover the old fixture from
Git. Notes about a future continuation must be rechecked even when the root
position key matches.

Record the focused test names enforcing the interpretation/line, or explicitly
state that coverage is missing. Move-parity tests alone do not verify reasoning.
This ledger is not a second oracle, an instruction to force fixture agreement,
or permission to introduce replay-specific engine exceptions. Tests for the
lookup are discovered by the existing Python lint/test pipeline.

## Backfill limits

The initial records contain only explanations recoverable from this
conversation. They are not an exhaustive import of past discussions. In
particular, the earlier assistant's assertion that the current turn-14 question
was already settled did not identify the exact prior explanation; do not turn
that assertion into a user-approved rule.
