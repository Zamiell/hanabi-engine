#!/usr/bin/env python3

from __future__ import annotations

import argparse
import concurrent.futures
import http.cookies
import json
import os
import queue
import subprocess
import sys
import threading
import time
import urllib.error
import urllib.parse
import urllib.request
from collections import deque
from pathlib import Path
from typing import Any, Callable

try:
    import websocket
except ImportError:
    websocket = None


DEFAULT_BASE_URL = "https://hanab.live"
DEFAULT_ITERATIONS = 1_000
DEFAULT_SEED = 0
DEFAULT_ENGINE_TIMEOUT = 180.0
INITIAL_RECONNECT_DELAY = 1.0
MAX_RECONNECT_DELAY = 30.0
STABLE_CONNECTION_SECONDS = 30.0


def log(message: str) -> None:
    print(message, flush=True)


def authenticate(base_url: str, username: str, password: str) -> tuple[str, str]:
    parsed = urllib.parse.urlsplit(base_url)
    if parsed.scheme not in {"http", "https"} or not parsed.netloc:
        raise ValueError("the base URL must use http or https and include a host")

    login_url = urllib.parse.urljoin(base_url.rstrip("/") + "/", "login")
    request = urllib.request.Request(
        login_url,
        data=urllib.parse.urlencode(
            {
                "username": username,
                "password": password,
                "version": "bot",
            }
        ).encode(),
        method="POST",
        headers={"User-Agent": "hanabi-engine-bot/0.1"},
    )
    try:
        with urllib.request.urlopen(request, timeout=30) as response:
            if response.status != 200:
                raise RuntimeError(f"authentication returned HTTP {response.status}")
            set_cookie_headers = response.headers.get_all("Set-Cookie") or []
    except urllib.error.HTTPError as error:
        body = error.read().decode(errors="replace").strip()
        suffix = f": {body}" if body else ""
        raise RuntimeError(f"authentication returned HTTP {error.code}{suffix}") from error
    except urllib.error.URLError as error:
        raise RuntimeError(f"could not reach {login_url}: {error.reason}") from error

    cookies: list[str] = []
    for header in set_cookie_headers:
        parsed_cookies = http.cookies.SimpleCookie()
        parsed_cookies.load(header)
        cookies.extend(f"{key}={morsel.value}" for key, morsel in parsed_cookies.items())
    if not cookies:
        raise RuntimeError("authentication response did not contain a session cookie")

    ws_scheme = "wss" if parsed.scheme == "https" else "ws"
    ws_url = urllib.parse.urlunsplit((ws_scheme, parsed.netloc, "/ws", "", ""))
    return ws_url, "; ".join(cookies)


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
        self._stop()

    def _ensure_started(self) -> None:
        with self.process_lock:
            if self.process is not None and self.process.poll() is None:
                return
            environment = {
                key: value
                for key, value in os.environ.items()
                if key not in {"HANABI_USERNAME", "HANABI_PASSWORD"}
            }
            try:
                self.process = subprocess.Popen(
                    self.command,
                    stdin=subprocess.PIPE,
                    stdout=subprocess.PIPE,
                    stderr=subprocess.PIPE,
                    text=True,
                    bufsize=1,
                    env=environment,
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


def next_reconnect_delay(previous: float, connected_seconds: float) -> float:
    if connected_seconds >= STABLE_CONNECTION_SECONDS:
        return INITIAL_RECONNECT_DELAY
    return min(previous * 2, MAX_RECONNECT_DELAY)


class HanabiEngineBot:
    def __init__(
        self,
        base_url: str,
        engine_command: list[str],
        username: str,
        password: str,
        debug: bool,
        engine_timeout: float = DEFAULT_ENGINE_TIMEOUT,
        authenticator: Callable[[str, str, str], tuple[str, str]] = authenticate,
        engine_factory: Callable[[list[str], float], PersistentEngine] = PersistentEngine,
    ) -> None:
        self.base_url = base_url
        self.username = username
        self.password = password
        self.engine_command = engine_command
        self.debug = debug
        self.engine_timeout = engine_timeout
        self.authenticator = authenticator
        self.engine_factory = engine_factory
        self.tables: dict[int, dict[str, Any]] = {}
        self.games: dict[int, dict[str, Any]] = {}
        self.ws: Any = None
        self.connection_generation = 0
        self.game_generation = 0
        self.opened_at: float | None = None
        self.lock = threading.RLock()
        self.stop_event = threading.Event()
        self.executor = concurrent.futures.ThreadPoolExecutor(
            max_workers=5,
            thread_name_prefix="hanabi-decision",
        )

    def run(self) -> None:
        reconnect_delay = INITIAL_RECONNECT_DELAY
        try:
            while not self.stop_event.is_set():
                try:
                    ws_url, cookie = self.authenticator(
                        self.base_url,
                        self.username,
                        self.password,
                    )
                    app = websocket.WebSocketApp(
                        ws_url,
                        cookie=cookie,
                        on_open=self.on_open,
                        on_message=self.on_message,
                        on_error=self.on_error,
                        on_close=self.on_close,
                    )
                    with self.lock:
                        self.connection_generation += 1
                        self.ws = app
                    app.run_forever()
                except (OSError, RuntimeError, ValueError) as error:
                    log(f"Connection attempt failed: {error}")
                finally:
                    with self.lock:
                        opened_at = self.opened_at
                    connected_seconds = (
                        0.0 if opened_at is None else time.monotonic() - opened_at
                    )
                    self._reset_connection()

                if self.stop_event.is_set():
                    break
                if connected_seconds >= STABLE_CONNECTION_SECONDS:
                    reconnect_delay = INITIAL_RECONNECT_DELAY
                log(f"Reconnecting in {reconnect_delay:g} seconds.")
                if self.stop_event.wait(reconnect_delay):
                    break
                reconnect_delay = next_reconnect_delay(
                    reconnect_delay,
                    0,
                )
        finally:
            self.shutdown()

    def shutdown(self) -> None:
        self.stop_event.set()
        with self.lock:
            ws = self.ws
        if ws is not None:
            ws.close()
        self._reset_connection()
        self.executor.shutdown(wait=True, cancel_futures=True)

    def on_open(self, _ws: websocket.WebSocketApp) -> None:
        with self.lock:
            self.opened_at = time.monotonic()
        log("Connected to Hanabi Live.")

    def on_error(self, _ws: websocket.WebSocketApp, error: object) -> None:
        log(f"WebSocket error: {error}")

    def on_close(
        self,
        _ws: websocket.WebSocketApp,
        status_code: int | None,
        message: str | None,
    ) -> None:
        detail = "" if status_code is None else f" ({status_code}: {message or ''})"
        log(f"Hanabi Live connection closed{detail}.")

    def on_message(self, _ws: websocket.WebSocketApp, message: str) -> None:
        command, separator, raw_data = message.partition(" ")
        if not separator:
            log(f"Ignoring malformed WebSocket message: {message!r}")
            return
        try:
            data = json.loads(raw_data)
        except json.JSONDecodeError as error:
            log(f"Ignoring invalid JSON for {command}: {error}")
            return

        if self.debug:
            log(f"received {command}")
        handlers = {
            "welcome": self.handle_welcome,
            "warning": self.handle_server_notice,
            "error": self.handle_server_notice,
            "chat": self.handle_chat,
            "table": self.handle_table,
            "tableList": self.handle_table_list,
            "tableGone": self.handle_table_gone,
            "tableStart": self.handle_table_start,
            "init": self.handle_init,
            "gameActionList": self.handle_game_action_list,
            "gameAction": self.handle_game_action,
            "databaseID": self.handle_game_finished,
            "finishOngoingGame": self.handle_game_finished,
        }
        handler = handlers.get(command)
        if handler is not None:
            try:
                handler(data)
            except (KeyError, TypeError, ValueError) as error:
                log(f"Could not handle {command}: {error}")

    def handle_welcome(self, data: dict[str, Any]) -> None:
        self.username = str(data["username"])
        log(f"Authenticated as {self.username}.")
        log(f"To invite the bot, privately message it: /msg {self.username} /join")

    @staticmethod
    def handle_server_notice(data: object) -> None:
        log(f"Hanabi Live: {data}")

    def handle_chat(self, data: dict[str, Any]) -> None:
        if data.get("recipient") != self.username or data.get("msg") != "/join":
            return
        requester = str(data["who"])
        with self.lock:
            table = next(
                (
                    candidate.copy()
                    for candidate in self.tables.values()
                    if not candidate.get("running", False)
                    and requester in candidate.get("players", [])
                ),
                None,
            )
        if table is None:
            self.reply(requester, "Create a table before asking me to join.")
            return
        if table.get("variant") != "No Variant":
            self.reply(requester, "I currently support only No Variant games.")
            return
        if len(table.get("players", [])) >= 5:
            self.reply(requester, "I support at most five total players.")
            return
        self.send("tableJoin", {"tableID": table["id"]})

    def handle_table(self, data: dict[str, Any]) -> None:
        with self.lock:
            self.tables[int(data["id"])] = data

    def handle_table_list(self, data: list[dict[str, Any]]) -> None:
        for table in data:
            self.handle_table(table)

    def handle_table_gone(self, data: dict[str, Any]) -> None:
        with self.lock:
            self.tables.pop(int(data["tableID"]), None)

    def handle_table_start(self, data: dict[str, Any]) -> None:
        self.send("getGameInfo1", {"tableID": int(data["tableID"])})

    def handle_init(self, data: dict[str, Any]) -> None:
        table_id = int(data["tableID"])
        with self.lock:
            old = self.games.pop(table_id, None)
            self.game_generation += 1
            self.games[table_id] = {
                "tableID": table_id,
                "playerNames": data["playerNames"],
                "ourPlayerIndex": data["ourPlayerIndex"],
                "spectating": data.get("spectating", False),
                "replay": data.get("replay", False),
                "options": data["options"],
                "actions": [],
                "turn": 0,
                "currentPlayer": 0,
                "terminal": False,
                "inFlight": False,
                "lastDecidedTurn": None,
                "syncedActions": 0,
                "engineInitialized": False,
                "engine": None,
                "generation": self.game_generation,
            }
        self._close_game_engine(old)
        self.send("getGameInfo2", {"tableID": table_id})

    def handle_game_action_list(self, data: dict[str, Any]) -> None:
        table_id = int(data["tableID"])
        with self.lock:
            game = self.games[table_id]
            old_engine = game["engine"]
            self.game_generation += 1
            game["generation"] = self.game_generation
            game["engine"] = None
            game["engineInitialized"] = False
            game["syncedActions"] = 0
            game["actions"] = list(data["list"])
            game["turn"] = 0
            game["currentPlayer"] = 0
            game["terminal"] = False
            game["inFlight"] = False
            game["lastDecidedTurn"] = None
            for action in game["actions"]:
                self._update_progress(game, action)
        if old_engine is not None:
            old_engine.close()
        self.send("loaded", {"tableID": table_id})
        self.maybe_move(table_id)

    def handle_game_action(self, data: dict[str, Any]) -> None:
        table_id = int(data["tableID"])
        with self.lock:
            game = self.games.get(table_id)
            if game is None:
                return
            action = data["action"]
            game["actions"].append(action)
            self._update_progress(game, action)
        self.maybe_move(table_id)

    def handle_game_finished(self, data: dict[str, Any]) -> None:
        table_id = int(data.get("tableID", -1))
        if table_id >= 0:
            self.send("tableUnattend", {"tableID": table_id})
            with self.lock:
                game = self.games.pop(table_id, None)
            self._close_game_engine(game)

    def maybe_move(self, table_id: int) -> None:
        with self.lock:
            game = self.games.get(table_id)
            if game is None:
                return
            if (
                game.get("spectating")
                or game.get("replay")
                or game["terminal"]
                or game["currentPlayer"] != int(game["ourPlayerIndex"])
                or game["lastDecidedTurn"] == game["turn"]
                or game["inFlight"]
            ):
                return
            game["inFlight"] = True
            turn = int(game["turn"])
            action_count = len(game["actions"])
            generation = int(game["generation"])
        try:
            self.executor.submit(
                self._decide,
                table_id,
                turn,
                action_count,
                generation,
            )
        except RuntimeError:
            with self.lock:
                game = self.games.get(table_id)
                if game is not None:
                    game["inFlight"] = False

    def _decide(
        self,
        table_id: int,
        turn: int,
        action_count: int,
        generation: int,
    ) -> None:
        try:
            action = self._request_action(table_id, action_count, generation)
            with self.lock:
                game = self.games.get(table_id)
                current = (
                    game is not None
                    and game["generation"] == generation
                    and game["turn"] == turn
                    and game["currentPlayer"] == int(game["ourPlayerIndex"])
                    and not game["terminal"]
                )
            if not current:
                log(f"Discarding stale engine result for table {table_id}, turn {turn}.")
                return
            if int(action.get("tableID", -1)) != table_id:
                raise EngineProcessError("engine response targets the wrong table")
            log(f"Table {table_id}, turn {turn}: sending {action}")
            self.send("action", action)
            with self.lock:
                game = self.games.get(table_id)
                if game is not None and game["generation"] == generation:
                    game["lastDecidedTurn"] = turn
        except (EngineProcessError, OSError, TypeError, ValueError) as error:
            log(f"Engine failed on table {table_id}, turn {turn}: {error}")
        finally:
            with self.lock:
                game = self.games.get(table_id)
                if game is not None and game["generation"] == generation:
                    game["inFlight"] = False

    def _request_action(
        self,
        table_id: int,
        action_count: int,
        generation: int,
    ) -> dict[str, Any]:
        last_error: EngineProcessError | None = None
        for _attempt in range(2):
            with self.lock:
                game = self.games.get(table_id)
                if game is None or game["generation"] != generation:
                    raise EngineProcessError("game session is no longer current")
                engine = game["engine"]
                if engine is None:
                    engine = self.engine_factory(
                        self.engine_command,
                        self.engine_timeout,
                    )
                    game["engine"] = engine
                    game["engineInitialized"] = False
                    game["syncedActions"] = 0
                initialized = bool(game["engineInitialized"])
                synced = int(game["syncedActions"])
                if initialized and synced <= action_count:
                    payload = {
                        "kind": "append",
                        "tableID": table_id,
                        "actions": game["actions"][synced:action_count],
                    }
                else:
                    snapshot = {
                        key: game[key]
                        for key in (
                            "tableID",
                            "playerNames",
                            "ourPlayerIndex",
                            "spectating",
                            "replay",
                            "options",
                        )
                    }
                    snapshot["actions"] = game["actions"][:action_count]
                    payload = {"kind": "initialize", "snapshot": snapshot}
            try:
                response = engine.request(payload)
            except EngineProcessError as error:
                last_error = error
                with self.lock:
                    game = self.games.get(table_id)
                    if game is not None and game.get("engine") is engine:
                        game["engine"] = None
                        game["engineInitialized"] = False
                        game["syncedActions"] = 0
                engine.close()
                continue
            with self.lock:
                game = self.games.get(table_id)
                if game is None or game["generation"] != generation:
                    raise EngineProcessError("game session changed during search")
                game["engineInitialized"] = True
                game["syncedActions"] = action_count
            return response
        assert last_error is not None
        raise last_error

    @staticmethod
    def _update_progress(game: dict[str, Any], action: dict[str, Any]) -> None:
        action_type = action.get("type")
        if action_type == "turn":
            game["turn"] = int(action["num"])
            game["currentPlayer"] = int(action["currentPlayerIndex"])
        elif action_type == "gameOver":
            game["terminal"] = True

    @staticmethod
    def _close_game_engine(game: dict[str, Any] | None) -> None:
        if game is not None and game.get("engine") is not None:
            game["engine"].close()

    def _reset_connection(self) -> None:
        with self.lock:
            self.ws = None
            self.opened_at = None
            self.connection_generation += 1
            games = list(self.games.values())
            self.games.clear()
            self.tables.clear()
        for game in games:
            self._close_game_engine(game)

    def reply(self, recipient: str, message: str) -> None:
        self.send(
            "chatPM",
            {"msg": message, "recipient": recipient, "room": "lobby"},
        )

    def send(self, command: str, data: dict[str, Any]) -> None:
        with self.lock:
            ws = self.ws
        if ws is None:
            raise OSError("WebSocket is not connected")
        ws.send(f"{command} {json.dumps(data, separators=(',', ':'))}")
        if self.debug:
            log(f"sent {command}")


def parse_arguments() -> argparse.Namespace:
    repository = Path(__file__).resolve().parents[1]
    default_engine = repository / "target" / "release" / "hanabi-engine"
    parser = argparse.ArgumentParser(
        description="Run Hanabi Engine as a player-safe Hanabi Live bot."
    )
    parser.add_argument(
        "--base-url",
        default=os.getenv("HANABI_BASE_URL", DEFAULT_BASE_URL),
        help=f"Hanabi Live HTTP origin (default: {DEFAULT_BASE_URL})",
    )
    parser.add_argument(
        "--engine",
        type=Path,
        default=Path(os.getenv("HANABI_ENGINE_BIN", default_engine)),
        help="path to the prebuilt hanabi-engine binary",
    )
    parser.add_argument("--mode", choices=("ismcts", "flat"), default="ismcts")
    parser.add_argument(
        "--iterations",
        type=int,
        default=int(os.getenv("HANABI_ENGINE_ITERATIONS", DEFAULT_ITERATIONS)),
        help=f"ISMCTS iterations per move (default: {DEFAULT_ITERATIONS})",
    )
    parser.add_argument("--samples", type=int, default=100)
    parser.add_argument("--seed", type=int, default=DEFAULT_SEED)
    parser.add_argument("--exploration", type=float)
    parser.add_argument(
        "--engine-timeout",
        type=float,
        default=DEFAULT_ENGINE_TIMEOUT,
        help=f"seconds to wait for one move (default: {DEFAULT_ENGINE_TIMEOUT:g})",
    )
    parser.add_argument(
        "--convention",
        choices=("none", "h-group"),
        default="h-group",
    )
    parser.add_argument(
        "--h-group-level",
        choices=tuple(str(level) for level in range(1, 26)) + ("max",),
        default="max",
    )
    parser.add_argument("--debug", action="store_true")
    return parser.parse_args()


def main() -> int:
    arguments = parse_arguments()
    if websocket is None:
        log(
            "error: websocket-client is not installed; run "
            "'python3 -m pip install -r scripts/requirements.txt'"
        )
        return 2
    username = os.getenv("HANABI_USERNAME", "")
    password = os.getenv("HANABI_PASSWORD", "")
    if not username or not password:
        log("error: set HANABI_USERNAME and HANABI_PASSWORD in the environment")
        return 2
    engine = arguments.engine.expanduser().resolve()
    if not engine.is_file():
        log(
            f"error: engine binary not found at {engine}; "
            "run 'cargo build --release --locked'"
        )
        return 2
    if arguments.iterations <= 0 or arguments.samples <= 0:
        log("error: search budgets must be positive")
        return 2
    if arguments.engine_timeout <= 0:
        log("error: the engine timeout must be positive")
        return 2
    if arguments.seed < 0:
        log("error: the search seed must be nonnegative")
        return 2
    if arguments.exploration is not None and arguments.exploration <= 0:
        log("error: the exploration coefficient must be positive")
        return 2

    engine_command = [
        str(engine),
        "live-session",
        "--mode",
        arguments.mode,
        "--iterations",
        str(arguments.iterations),
        "--samples",
        str(arguments.samples),
        "--seed",
        str(arguments.seed),
        "--convention",
        arguments.convention,
    ]
    if arguments.exploration is not None:
        engine_command.extend(["--exploration", str(arguments.exploration)])
    if arguments.convention == "h-group":
        engine_command.extend(["--h-group-level", arguments.h_group_level])

    profile = (
        f"H-Group {arguments.h_group_level}"
        if arguments.convention == "h-group"
        else "no convention"
    )
    log(f"Connecting to {arguments.base_url} with {profile}.")
    HanabiEngineBot(
        base_url=arguments.base_url,
        engine_command=engine_command,
        username=username,
        password=password,
        debug=arguments.debug,
        engine_timeout=arguments.engine_timeout,
    ).run()
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
