# Repository Instructions

## Windows Subsystem for Linux (WSL) file ownership

If this repository is cloned inside WSL, ensure that newly created files are
owned by the same user and group as the repository parent. Windows-hosted
editing tools can accidentally create files owned by `root:root`.

## Validation

Optimize for short, evidence-backed interactive turns. Full validation is a
checkpoint, not an automatic tax on every prompt. Preserve all tests and their
assertions; change when they run, not their strength.

| Change                                        | Local validation before responding                                                     |
| --------------------------------------------- | -------------------------------------------------------------------------------------- |
| Discussion / diagnosis                        | Relevant read-only inspection; no full suite                                           |
| Documentation                                 | Formatting and any affected examples/scripts                                           |
| Fixture edit                                  | Parse and replay legality; inspect/test the affected position                          |
| Localized convention fix                      | Reproduce the failure; focused regression and related tests; `scripts/check.sh --fast` |
| Shared inference/planner refactor             | Focused checks first; one full `scripts/check.sh` after implementation settles         |
| Milestone or explicit full-validation request | Full `scripts/check.sh`                                                                |

`--fast` retains build, lint, documentation, Python, and dead-code checks but
omits the expensive ordinary Rust suite. Run relevant Rust tests separately. Run
inexpensive formatting/Clippy before launching broad tests. A failing focused
test is a reason to keep debugging, not to start full validation.

CI runs full validation on each push, plus the existing MSRV and exhaustive
profile jobs. Do not wait for CI unless requested or necessary for the task.
Never describe local fast/focused success or an unobserved CI run as a full
pass. Report what ran, what was deferred to CI, and any known failures.

Do not overlap full validation runs or CPU-heavy benchmarks. For performance
comparisons, use the same precompiled workload and configuration, run before and
after sequentially, and distinguish reduced test setup from engine speed.
`check.sh` completes all stages even when tests fail. Do not restart the entire
suite for a formatting-only fix or test-only assertion adjustment: rerun the
affected stage/tests. Rerun the full suite only if subsequent implementation
changes invalidate its coverage. Do not run full validation repeatedly to
rediscover unchanged pre-existing failures.

### Timing and failure records

For implementation/investigation tasks, start
`python3 scripts/workflow.py start "short task label"` at the beginning and use
`finish` before the final response. These measure the recorded work session, not
the app's complete prompt latency or model thinking time. If interrupted, finish
the stale session before starting another; do not quietly overwrite it. Pure
conversational answers need no timer.

`check.sh` automatically records commands, output logs, stage times, exit codes,
and Git/worktree identity under ignored `target/workflow/`. Wrap other
substantive commands with
`python3 scripts/workflow.py run --label LABEL -- COMMAND ...`. Use `wait-start`
/ `wait-end` only for intervals spent waiting, not intervals used to investigate
or edit while checks run. Unrecorded waits are unknown. `report` shows task
durations, command execution, explicitly recorded waiting, and full-run counts.
Do not add overlapping command durations to task duration.

Use an explicit `baseline LOG --revision REVISION` from a completed full run to
distinguish new, persistent, and no-longer-reported test failures. Never
automatically accept new failures into the baseline. Baselines are diagnostic:
all failures retain nonzero exit status, including in CI. Stage failures and
incomplete runs must also be reported; missing results are not proof of fixes.

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

## Clear bugs discovered during examination

When examining code, explaining an engine decision, auditing behavior, or
investigating a replay, autonomously fix clear implementation bugs discovered
within that task. This is standing authorization to make those scoped fixes,
even when the immediate question asks why something happens. Do not stop after
identifying the bug or ask for another prompt merely to implement it. Reproduce
the issue, add or update appropriate regression coverage, fix it, run the
applicable implementation checks above, and resume the original examination.

A clear bug has an established expected behavior supported by the convention
documentation, a reviewed user interpretation, or an ordinary software
invariant. An unexplained score difference or uncertain convention is not
enough. Ask for judgment when the intended behavior is genuinely ambiguous; do
not invent convention rules, one-off exceptions, or fixture changes to force
parity. Respect an explicit request for read-only analysis or no edits. This
standing authorization does not expand a localized task into unrelated fixes or
a full replay scan, and documentation-only requests remain documentation-only.

## Version control

After every user prompt that changes this repository:

1. Run the relevant checks according to the validation matrix above.
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

### Scope and replay checkpoints

For a request to fix a specific bug, verify that position and related behavior,
then report the fix. Do not automatically search for the next disagreement or
expand into other replays. Fixture edits likewise do not authorize changing
subsequent moves. A full replay scan is a separate task or checkpoint.

When the user explicitly asks to continue through a replay, re-run that replay
from the beginning and proceed until a non-obvious convention/strategy question
or genuine blocker. Do not silently switch replays. Full replay scans belong in
this workflow, not every small fix or explanation.

Within the user's authorized bug-fixing scope, investigate and fix obvious bugs
without waiting for another prompt. Stop for a non-obvious convention or
strategy question and provide the review context below. Do not change fixtures
or expected moves merely to obtain agreement.

An unexplained disagreement is an investigation task, not automatically a
request for user input. Do not stop merely because a candidate has a higher
heuristic score, its projection ends at an unknown identity, or the next bug is
in a different helper. Inspect scoring terms, actual projected actions, and
uncertainty handling, then fix clear implementation errors and continue. Unknown
cards must remain unknown; an unfinished forecast is neither proof that an
action is good nor grounds to reject it automatically.

When an explicitly requested replay investigation ends with an unresolved
disagreement, state the specific convention/strategy question that requires the
user's judgment (and why the documentation and already-reviewed expectations do
not answer it), or the concrete external blocker. If no such question or blocker
exists, continue working rather than asking the user to authorize the same
investigation again.

For that replay investigation, report the next unresolved disagreement (with
link, seed, turn, candidate clues, and reasoning), agreement for the tested
replay/range, or the concrete blocker. For localized fixes, report the fix and
focused validation; do not claim untested replay-wide agreement.
Documentation/workflow-only requests do not authorize additional engine fixes.

### Replay Link

Always include a generated clickable Hanab Live replay link, the seed, the
one-based turn, and the competing actions or concrete issue. This applies even
when reporting status rather than explicitly asking a question.

The expert replay comparison prints a link on disagreement. For a ready-to-paste
Markdown link, generate it with:

```bash
scripts/generate-hanab-live-link.sh path/to/game.json --turn 1
```

Use Hanab Live's **one-based** turn number.

Copy the generator's complete Markdown link verbatim into the response. Never
retype, reconstruct, shorten, or manually edit the compressed URL, including its
turn fragment; rerun the generator with the desired turn instead. The generator
checks that its payload decodes to the original player count, deck, actions,
variant, and seed before printing. That check protects generation, not later
transcription: compare the final response link against the tool output before
sending. If using the comparison tool's URL directly, copy that URL verbatim.

### Candidate Clues

When reporting a disagreement, enumerate the candidate clues considered.

### Move Reasoning

Explain the full reasoning of why the engine chose one canditate clue over the
others.
