from __future__ import annotations

import atexit
import datetime
import json
import os
import threading
from pathlib import Path
from typing import Any

TRACE_SCHEMA_VERSION = 1

_LOG_LOCK = threading.Lock()
_LOG_STREAM: Any = None


def configure_log_file(path: Path) -> None:
    global _LOG_STREAM
    path.parent.mkdir(parents=True, exist_ok=True)
    with _LOG_LOCK:
        if _LOG_STREAM is not None:
            _LOG_STREAM.close()
        _LOG_STREAM = path.open("a", encoding="utf-8", buffering=1)


def close_log_file() -> None:
    global _LOG_STREAM
    with _LOG_LOCK:
        stream = _LOG_STREAM
        _LOG_STREAM = None
        if stream is not None:
            stream.close()


atexit.register(close_log_file)


def log(message: str) -> None:
    with _LOG_LOCK:
        print(message, flush=True)
        if _LOG_STREAM is not None:
            timestamp = datetime.datetime.now(datetime.timezone.utc).isoformat()
            try:
                _LOG_STREAM.write(f"{timestamp} {message}\n")
            except OSError:
                # Console logging must remain usable if the trace volume disappears.
                pass


class TraceRecorder:
    """Writes player-safe, replayable diagnostics for one bridge invocation."""

    def __init__(self, root: Path, metadata: dict[str, Any]) -> None:
        root.mkdir(parents=True, exist_ok=True)
        timestamp = datetime.datetime.now(datetime.timezone.utc).strftime(
            "%Y%m%dT%H%M%S.%fZ"
        )
        candidate = root / f"{timestamp}-{os.getpid()}"
        suffix = 1
        while candidate.exists():
            candidate = root / f"{timestamp}-{os.getpid()}-{suffix}"
            suffix += 1
        candidate.mkdir()
        self.run_directory = candidate
        self.events_path = candidate / "events.jsonl"
        self.lock = threading.RLock()
        self.next_decision_id = 1
        manifest = {
            "schemaVersion": TRACE_SCHEMA_VERSION,
            "startedAt": datetime.datetime.now(datetime.timezone.utc).isoformat(),
            **metadata,
        }
        self._write_json(candidate / "run.json", manifest)

    def begin_decision(
        self,
        table_id: int,
        turn: int,
        action_count: int,
        generation: int,
        snapshot: dict[str, Any],
    ) -> dict[str, Any]:
        with self.lock:
            decision_id = self.next_decision_id
            self.next_decision_id += 1
            stem = f"decision-{decision_id:06d}-turn-{turn:06d}"
            table_directory = self.run_directory / "tables" / str(table_id)
            table_directory.mkdir(parents=True, exist_ok=True)
            snapshot_path = table_directory / f"{stem}.snapshot.json"
            self._write_json(snapshot_path, snapshot)
            context = {
                "decisionId": decision_id,
                "tableID": table_id,
                "turn": turn,
                "actionCount": action_count,
                "generation": generation,
                "stem": stem,
                "tableDirectory": table_directory,
            }
            self._record_event(
                "decisionStarted",
                context,
                {"snapshot": self._relative(snapshot_path)},
            )
            return context

    def record_request(
        self,
        context: dict[str, Any],
        attempt: int,
        payload: dict[str, Any],
    ) -> None:
        path = self._decision_path(context, f"attempt-{attempt:02d}.request.json")
        self._write_json(path, payload)
        self._record_event(
            "engineRequest",
            context,
            {"attempt": attempt, "request": self._relative(path)},
        )

    def record_response(
        self,
        context: dict[str, Any],
        attempt: int,
        response: dict[str, Any],
    ) -> None:
        path = self._decision_path(context, f"attempt-{attempt:02d}.response.json")
        self._write_json(path, response)
        self._record_event(
            "engineResponse",
            context,
            {"attempt": attempt, "response": self._relative(path)},
        )

    def record_engine_error(
        self,
        context: dict[str, Any],
        attempt: int,
        error: Exception,
    ) -> None:
        path = self._decision_path(context, f"attempt-{attempt:02d}.error.json")
        self._write_json(path, {"error": str(error)})
        self._record_event(
            "engineError",
            context,
            {"attempt": attempt, "error": str(error), "details": self._relative(path)},
        )

    def finish_decision(
        self,
        context: dict[str, Any],
        status: str,
        *,
        action: dict[str, Any] | None = None,
        error: Exception | None = None,
    ) -> None:
        result: dict[str, Any] = {"status": status}
        if action is not None:
            result["action"] = action
        if error is not None:
            result["error"] = str(error)
        path = self._decision_path(context, "result.json")
        self._write_json(path, result)
        self._record_event(
            "decisionFinished",
            context,
            {**result, "result": self._relative(path)},
        )

    def _decision_path(self, context: dict[str, Any], suffix: str) -> Path:
        return context["tableDirectory"] / f"{context['stem']}.{suffix}"

    def _relative(self, path: Path) -> str:
        return path.relative_to(self.run_directory).as_posix()

    def _record_event(
        self,
        kind: str,
        context: dict[str, Any],
        details: dict[str, Any],
    ) -> None:
        event = {
            "timestamp": datetime.datetime.now(datetime.timezone.utc).isoformat(),
            "kind": kind,
            "decisionId": context["decisionId"],
            "tableID": context["tableID"],
            "turn": context["turn"],
            **details,
        }
        with self.lock:
            with self.events_path.open("a", encoding="utf-8") as stream:
                stream.write(json.dumps(event, separators=(",", ":")) + "\n")

    @staticmethod
    def _write_json(path: Path, value: Any) -> None:
        temporary = path.with_name(
            f".{path.name}.{os.getpid()}.{threading.get_ident()}.tmp"
        )
        try:
            with temporary.open("w", encoding="utf-8") as stream:
                json.dump(value, stream, indent=2, ensure_ascii=False)
                stream.write("\n")
            os.replace(temporary, path)
        finally:
            try:
                temporary.unlink()
            except FileNotFoundError:
                pass


