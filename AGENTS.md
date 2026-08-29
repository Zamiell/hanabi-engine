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
