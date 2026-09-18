"""Local task/command timing and failure comparison; never changes check results."""

from __future__ import annotations

import argparse
import json
from pathlib import Path
import re
import subprocess
import sys
import time
from typing import Any
import uuid

ROOT = Path(__file__).resolve().parents[1]
STORE = ROOT / "target" / "workflow"
ANSI = re.compile(r"\x1b\[[0-9;]*m")
FAILURE = re.compile(r"^\s*(?:FAIL|TIMEOUT|XPASS|LEAK)\s+\[[^]]*\]\s+(?:\([^)]*\)\s+)?(.+)$")
STAGE = re.compile(r"^CHECK TIMING: (.*): (\d+)s \(exit (\d+)\)$")


def read_json(path: Path) -> dict[str, Any]:
    return json.loads(path.read_text())


def write_json(path: Path, value: dict[str, Any]) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    temporary = path.with_suffix(".tmp")
    temporary.write_text(json.dumps(value, indent=2) + "\n")
    temporary.replace(path)


def git(*args: str) -> str:
    return subprocess.check_output(["git", *args], cwd=ROOT, text=True).strip()


def identity() -> dict[str, str]:
    # The diff hash distinguishes successive dirty trees on the same commit.
    diff = subprocess.check_output(["git", "diff", "HEAD", "--binary"], cwd=ROOT)
    import hashlib

    return {"commit": git("rev-parse", "HEAD"), "diff_sha256": hashlib.sha256(diff).hexdigest(),
            "status": git("status", "--short")}


def parse_log(text: str) -> tuple[list[str], dict[str, dict[str, int]]]:
    failures: set[str] = set()
    stages: dict[str, dict[str, int]] = {}
    for line in ANSI.sub("", text).splitlines():
        failure = FAILURE.match(line)
        stage = STAGE.match(line)
        if failure:
            failures.add(failure[1])
        if stage:
            stages[stage[1]] = {"seconds": int(stage[2]), "exit_code": int(stage[3])}
    return sorted(failures), stages


def failure_delta(current: list[str], baseline: list[str]) -> dict[str, list[str]]:
    return {"new": sorted(set(current) - set(baseline)),
            "still_failing": sorted(set(current) & set(baseline)),
            "no_longer_reported": sorted(set(baseline) - set(current))}


def active_task() -> dict[str, Any] | None:
    path = STORE / "active.json"
    return read_json(path) if path.exists() else None


def run_command(label: str, command: list[str]) -> int:
    if command[:1] == ["--"]:
        command = command[1:]
    if not command:
        raise ValueError("run requires a command after --")
    STORE.mkdir(parents=True, exist_ok=True)
    run_id = uuid.uuid4().hex
    task = active_task()
    record: dict[str, Any] = {"kind": "command", "id": run_id, "label": label,
                              "task_id": task["id"] if task else None,
                              "command": command, "started": time.time(), **identity()}
    log_path = STORE / f"{run_id}.log"
    started = time.monotonic()
    code = 127
    try:
        with log_path.open("w") as log:
            with subprocess.Popen(command, cwd=ROOT, stdout=subprocess.PIPE,
                                  stderr=subprocess.STDOUT, text=True, errors="replace") as process:
                assert process.stdout is not None
                try:
                    for line in process.stdout:
                        sys.stdout.write(line)
                        sys.stdout.flush()
                        log.write(line)
                    code = process.wait()
                except KeyboardInterrupt:
                    process.terminate()
                    try:
                        process.wait(timeout=5)
                    except subprocess.TimeoutExpired:
                        process.kill()
                        process.wait()
                    code = 130
    finally:
        record.update(seconds=time.monotonic() - started, finished=time.time(), exit_code=code,
                      log=str(log_path))
        failures, stages = parse_log(log_path.read_text() if log_path.exists() else "")
        record.update(failures=failures, stages=stages)
        baseline = STORE / "failure-baseline.json"
        # Compare only full runs that reached the end. A focused run cannot
        # establish that omitted tests have been fixed.
        complete = "Final ownership" in stages and "Rust tests" in stages
        if label == "check-full" and complete and baseline.exists():
            record["failure_delta"] = failure_delta(failures, read_json(baseline)["failures"])
            print("Failure comparison (does not waive failures):", json.dumps(record["failure_delta"]))
        write_json(STORE / f"{run_id}.json", record)
        print(f"WORKFLOW: {label}: {record['seconds']:.2f}s; exit {code}; record {run_id}")
    return code if code >= 0 else 128 - code


def union_seconds(intervals: list[tuple[float, float]]) -> float:
    total = 0.0
    end = float("-inf")
    for start, finish in sorted(intervals):
        total += max(0.0, finish - max(start, end))
        end = max(end, finish)
    return total


def report() -> None:
    records = [read_json(path) for path in STORE.glob("*.json")
               if path.name not in {"active.json", "failure-baseline.json"}]
    for task in sorted((r for r in records if r.get("kind") == "task"), key=lambda r: r["started"]):
        runs = [r for r in records if r.get("kind") == "command" and r.get("task_id") == task["id"]]
        command_time = union_seconds([(r["started"], r["finished"]) for r in runs])
        waits = task.get("waits", [])
        print(json.dumps({"task": task["label"], "id": task["id"],
                          "started": task["started"], "elapsed_seconds": task["finished"] - task["started"],
                          "command_wall_seconds": command_time,
                          "recorded_wait_seconds": union_seconds([tuple(w) for w in waits]),
                          "full_runs": sum(r["label"] == "check-full" for r in runs),
                          "commands": [{"label": r["label"], "seconds": r["seconds"],
                                        "exit_code": r["exit_code"]} for r in runs]}))


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="operation", required=True)
    commands.add_parser("start").add_argument("label")
    for name in ("finish", "wait-start", "wait-end", "report"):
        commands.add_parser(name)
    baseline = commands.add_parser("baseline", help="Explicitly record failures from a completed full-check log")
    baseline.add_argument("log", type=Path)
    baseline.add_argument("--revision", required=True, help="Revision that produced the log")
    run = commands.add_parser("run")
    run.add_argument("--label", required=True)
    run.add_argument("command", nargs=argparse.REMAINDER)
    args = parser.parse_args()
    if args.operation == "run":
        return run_command(args.label, args.command)
    if args.operation == "report":
        report()
        return 0
    if args.operation == "baseline":
        failures, stages = parse_log(args.log.read_text())
        if "Final ownership" not in stages or "Rust tests" not in stages:
            raise ValueError("baseline requires a completed full-check log")
        write_json(STORE / "failure-baseline.json", {"revision": args.revision,
                   "log": str(args.log.resolve()), "failures": failures, "stages": stages})
        print(f"Recorded {len(failures)} failing tests; this does not change any check exit status.")
        return 0
    task = active_task()
    if args.operation == "start":
        if task:
            raise ValueError("a task is already active; finish it before starting another")
        task = {"kind": "task", "id": uuid.uuid4().hex, "label": args.label,
                "started": time.time(), "waits": [], **identity()}
    else:
        if not task:
            raise ValueError("no active task")
        if args.operation == "wait-start":
            if "wait_started" in task:
                raise ValueError("already recording a wait")
            task["wait_started"] = time.time()
        elif args.operation == "wait-end":
            if "wait_started" not in task:
                raise ValueError("no wait is active")
            task["waits"].append([task.pop("wait_started"), time.time()])
        elif args.operation == "finish":
            task["finished"] = time.time()
            if "wait_started" in task:
                task["waits"].append([task.pop("wait_started"), task["finished"]])
            write_json(STORE / f"{task['id']}.json", task)
            (STORE / "active.json").unlink()
            report()
            return 0
    write_json(STORE / "active.json", task)
    return 0


if __name__ == "__main__":
    try:
        sys.exit(main())
    except (ValueError, OSError) as error:
        print(f"workflow: {error}", file=sys.stderr)
        sys.exit(1)
