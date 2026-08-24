from __future__ import annotations

import os
import json
import queue
import subprocess
import threading
from collections import deque
from pathlib import Path
from typing import Any

def engine_environment() -> dict[str, str]:
    return {
        key: value
        for key, value in os.environ.items()
        if key not in {"HANABI_USERNAME", "HANABI_PASSWORD"}
    }


def validate_engine_binary(engine: Path) -> None:
    try:
        result = subprocess.run(
            [str(engine), "--help"],
            text=True,
            capture_output=True,
            timeout=10,
            check=False,
            env=engine_environment(),
        )
    except (OSError, subprocess.TimeoutExpired) as error:
        raise EngineProcessError(f"could not inspect engine binary: {error}") from error
    help_text = result.stdout + result.stderr
    if result.returncode != 0:
        detail = help_text.strip() or f"exit status {result.returncode}"
        raise EngineProcessError(f"engine self-check failed: {detail}")
    if (
        "hanabi-engine live-session" not in help_text
        or "--include-planning-details" not in help_text
        or "--exact-world-limit" not in help_text
    ):
        raise EngineProcessError(
            "engine binary is older than the live bridge and does not support "
            "the deterministic live-session protocol; rebuild it with "
            "'cargo build --release --locked'"
        )


class EngineProcessError(RuntimeError):
    pass


class PersistentEngine:
    """One newline-delimited Rust engine session for one live table."""

    def __init__(self, command: list[str], timeout: float) -> None:
        self.command = command
        self.timeout = timeout
        self.process: subprocess.Popen[str] | None = None
        self.responses: queue.Queue[str | None] = queue.Queue()
        self.stderr: deque[str] = deque(maxlen=20)
        self.reader_threads: list[threading.Thread] = []
        self.request_lock = threading.Lock()
        self.process_lock = threading.Lock()
        self.closed = threading.Event()

    def request(self, payload: dict[str, Any]) -> dict[str, Any]:
        with self.request_lock:
            self._ensure_started()
            assert self.process is not None
            assert self.process.stdin is not None
            process = self.process
            responses = self.responses
            try:
                process.stdin.write(
                    json.dumps(payload, separators=(",", ":")) + "\n"
                )
                process.stdin.flush()
            except (BrokenPipeError, OSError) as error:
                detail = self._failure_detail()
                self._stop()
                raise EngineProcessError(f"engine input failed: {detail}") from error

            try:
                line = responses.get(timeout=self.timeout)
            except queue.Empty as error:
                self._stop()
                raise EngineProcessError(
                    f"engine did not respond within {self.timeout:g} seconds"
                ) from error
            if line is None:
                detail = self._failure_detail()
                self._stop()
                raise EngineProcessError(f"engine exited without a response: {detail}")
            try:
                response = json.loads(line)
            except json.JSONDecodeError as error:
                self._stop()
                raise EngineProcessError(f"engine returned invalid JSON: {error}") from error
            if not isinstance(response, dict):
                self._stop()
                raise EngineProcessError("engine response is not a JSON object")
            if "error" in response:
                message = str(response["error"])
                self._stop()
                raise EngineProcessError(message)
            return response

    def close(self) -> None:
        self.closed.set()
        self._stop()

    def _ensure_started(self) -> None:
        with self.process_lock:
            if self.closed.is_set():
                raise EngineProcessError("engine session is closed")
            if self.process is not None and self.process.poll() is None:
                return
            try:
                self.process = subprocess.Popen(
                    self.command,
                    stdin=subprocess.PIPE,
                    stdout=subprocess.PIPE,
                    stderr=subprocess.PIPE,
                    text=True,
                    bufsize=1,
                    env=engine_environment(),
                )
            except OSError as error:
                raise EngineProcessError(f"could not start engine: {error}") from error
            self.responses = queue.Queue()
            self.stderr.clear()
            assert self.process.stdout is not None
            assert self.process.stderr is not None
            stdout_thread = threading.Thread(
                target=self._read_stdout,
                args=(self.process.stdout, self.responses),
                name="hanabi-engine-stdout",
                daemon=True,
            )
            stderr_thread = threading.Thread(
                target=self._read_stderr,
                args=(self.process.stderr,),
                name="hanabi-engine-stderr",
                daemon=True,
            )
            self.reader_threads = [stdout_thread, stderr_thread]
            stdout_thread.start()
            stderr_thread.start()

    @staticmethod
    def _read_stdout(stream: Any, responses: queue.Queue[str | None]) -> None:
        try:
            for line in stream:
                responses.put(line)
        finally:
            responses.put(None)

    def _read_stderr(self, stream: Any) -> None:
        for line in stream:
            self.stderr.append(line.rstrip())

    def _failure_detail(self) -> str:
        if self.stderr:
            return self.stderr[-1]
        if self.process is not None and self.process.poll() is not None:
            return f"exit status {self.process.returncode}"
        return "no diagnostic output"

    def _stop(self) -> None:
        with self.process_lock:
            process = self.process
            self.process = None
            reader_threads = self.reader_threads
            self.reader_threads = []
        if process is None:
            return
        if process.stdin is not None:
            try:
                process.stdin.close()
            except OSError:
                pass
        if process.poll() is None:
            process.terminate()
            try:
                process.wait(timeout=2)
            except subprocess.TimeoutExpired:
                process.kill()
                process.wait()
        for thread in reader_threads:
            thread.join(timeout=0.2)
        for stream in (process.stdout, process.stderr):
            if stream is not None:
                stream.close()

