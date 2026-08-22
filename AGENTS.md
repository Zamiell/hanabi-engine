# Repository Instructions

## WSL file ownership

This repository lives in Ubuntu WSL at `/home/james/repositories/hanabi-engine`.
All repository files and directories must remain owned by `james:james`.

Windows-hosted editing tools can create new WSL files as `root:root`. After
creating any file or directory, verify its ownership and correct it immediately
when necessary:

```powershell
wsl.exe -d Ubuntu -u root -- chown james:james /home/james/repositories/hanabi-engine/<exact-path>
```

Use `chown -R` only when the exact directory being corrected has first been
verified to be inside this repository. Never run a recursive ownership command
against `/home/james`, `/home/james/repositories`, `/`, or an unresolved path.

Before completing work, audit the repository for ownership problems:

```bash
find /home/james/repositories/hanabi-engine -xdev \
  \( ! -user james -o ! -group james \) -print
```

If that audit returns paths, correct only those results. From a Windows-hosted
session, the following command safely confines the repair to this repository:

```powershell
wsl.exe -d Ubuntu -u root -- find /home/james/repositories/hanabi-engine -xdev `
  \( ! -user james -o ! -group james \) -exec chown james:james -- {} +
```
