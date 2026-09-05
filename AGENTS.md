# Repository Instructions

## Windows Subsystem for Linux (WSL) file ownership

If this repository is cloned inside WSL, ensure that newly created files are
owned by the same user and group as the repository parent. Windows-hosted
editing tools can accidentally create files owned by `root:root`.

## Validation

While working, run the checks most relevant to the files being changed. Before
completing a task, run the repository's full validation suite:

```bash
scripts/check.sh
```

If the full suite cannot be run, report which checks were skipped and why.

## Version control

After every user prompt that changes this repository:

1. Run the relevant checks, including `scripts/check.sh` before completing the
   prompt.
2. Commit all in-scope changes with a descriptive commit message.
3. Push the commit to the current branch's configured upstream.

Do not create empty commits for prompts that make no repository changes. Do not
include unrelated pre-existing worktree changes in the commit. If validation,
the commit, or the push fails, report the failure instead of claiming the task
is complete.

## Other Repositories

- The "[hanabi-live](https://github.com/Hanabi-Live/hanabi-live)" repository contains the source code for the website where everybody plays the game. This engine has to integrate with it in various ways. The repository should be checked-out next to this one. You can reference the source code when you need to confirm a specific game mechanic or server data structure.
- The ["hanabi.github.io"](https://github.com/hanabi/hanabi.github.io/) repository contains the source code for the website that documents every H-Group convention. The engine uses these conventions when playing in H-Group mode. The repository should be checked-out next to this one. You can reference the source code when you need to confirm how a specific convention should work.

## Strategy or Convention Questions

When asking the human a strategy or convention question, include a clickable
Hanab Live replay link generated with:

```bash
scripts/generate-hanab-live-link.sh path/to/game.json --turn 23
```

The script prints `https://hanab.live/shared-replay-json/<compressed-data>#23`.
Use Hanab Live's **one-based** turn number: turn 1 is the initial deal, and
turn 23 is the position after 22 actions. This differs from `analyze --turn`.
Pass a standard-game fixture or a self-play `.active/<seed>.json` replay;
seed-only decks are expanded automatically. The entire supplied replay is
validated, so use a legal prefix if later actions are malformed.

The link embeds the deck and actions; no upload, clipboard access, or running
local server is needed to generate it. Opening it imports the replay on Hanab
Live. The upstream format replaces player names with Alice/Bob/etc. and omits
notes, seed metadata, and custom options. Do not use it for nonstandard games.
The codec mirrors `hanabi-live/packages/client/src/lobby/hypoCompress.ts`.
Include the seed, turn, and concrete question alongside the link, rather than
requiring the human to infer the issue from the replay alone.
