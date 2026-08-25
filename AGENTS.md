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
