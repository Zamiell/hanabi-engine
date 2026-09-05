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

## Test provenance

Do not add invented game histories as authorities for convention meanings or
optimal moves. Use a human-reviewed replay position, identify its fixture and
turn, and explain the reviewed expectation. Hypothetical alternatives may branch
from that position, but must not silently replace its recorded moves.

Self-play recordings are bug reproductions, not validated strategy. Keep their
assertions limited to established rules, legality, consistency, or a specific
human-reviewed interpretation; do not freeze an old engine choice as the best
move. Artificial inputs remain appropriate for ordinary game-rule, codec,
algorithm, and data-structure unit tests and invariant-only smoke tests. See
`docs/testing.md` for the categories and known coverage gaps.

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

- The "[hanabi-live](https://github.com/Hanabi-Live/hanabi-live)" repository
  contains the source code for the website where everybody plays the game. This
  engine has to integrate with it in various ways. The repository should be
  checked-out next to this one. You can reference the source code when you need
  to confirm a specific game mechanic or server data structure.
- The ["hanabi.github.io"](https://github.com/hanabi/hanabi.github.io/)
  repository contains the source code for the website that documents every
  H-Group convention. The engine uses these conventions when playing in H-Group
  mode. The repository should be checked-out next to this one. You can reference
  the source code when you need to confirm how a specific convention should
  work.

## Replay Positions and Human Review

Whenever reporting a replay disagreement, convention bug, or position requiring
human review, use the following instructions.

### Replay Link

Always include a generated clickable Hanab Live replay link, the seed, the
one-based turn, and the competing actions or concrete issue. This applies even
when reporting status rather than explicitly asking a question.

The expert replay comparison prints a link on disagreement. Include that link in
the user-facing response. Otherwise, generate it with:

```bash
scripts/generate-hanab-live-link.sh path/to/game.json --turn 1
```

Use Hanab Live's **one-based** turn number.

### Canditate Clues

Always enumerate all canditate clues that the engine considered.

### Move Reasoning

Explain the full reasoning of why the engine chose one canditate clue over the
others.
