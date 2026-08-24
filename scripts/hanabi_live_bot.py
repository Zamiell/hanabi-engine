#!/usr/bin/env python3

from __future__ import annotations

import argparse
import concurrent.futures
import http.cookies
import json
import os
import signal
import sys
import threading
import time
import urllib.error
import urllib.parse
import urllib.request
from pathlib import Path
from typing import TYPE_CHECKING, Any, Callable, Protocol

from hanabi_live_engine import (
    EngineProcessError,
    EngineSession,
    PersistentEngine,
    validate_engine_binary,
)
from hanabi_live_game import LiveGame
from hanabi_live_trace import TraceRecorder, close_log_file, configure_log_file, log

if TYPE_CHECKING:
    from websocket import WebSocketApp

websocket: Any
try:
    import websocket as _websocket
except ImportError:
    websocket = None
else:
    websocket = _websocket


DEFAULT_BASE_URL = "https://hanab.live"
DEFAULT_EXACT_WORLD_LIMIT = 4_096
DEFAULT_EXACT_NODE_LIMIT = 50_000
DEFAULT_ENGINE_TIMEOUT = 180.0
INITIAL_RECONNECT_DELAY = 1.0
MAX_RECONNECT_DELAY = 30.0
STABLE_CONNECTION_SECONDS = 30.0


class WebSocketConnection(Protocol):
    """Operations the bot needs from a live or test WebSocket."""

    def send(self, message: str) -> object: ...

    def close(self) -> object: ...


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
        engine_factory: Callable[[list[str], float], EngineSession] = PersistentEngine,
        trace_recorder: TraceRecorder | None = None,
    ) -> None:
        self.base_url = base_url
        self.username = username
        self.password = password
        self.engine_command = engine_command
        self.debug = debug
        self.engine_timeout = engine_timeout
        self.authenticator = authenticator
        self.engine_factory = engine_factory
        self.trace_recorder = trace_recorder
        self.default_h_group_level = self._command_option(
            engine_command,
            "--h-group-level",
        )
        self.game_levels: dict[int, str] = {}
        self.tables: dict[int, dict[str, Any]] = {}
        self.games: dict[int, LiveGame] = {}
        self.ws: WebSocketConnection | None = None
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
                    if self.stop_event.is_set():
                        break
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
        self.stop()
        self._reset_connection()
        self.executor.shutdown(wait=True, cancel_futures=True)

    def stop(self) -> None:
        """Requests a clean shutdown without allowing a reconnect."""
        already_stopping = self.stop_event.is_set()
        self.stop_event.set()
        with self.lock:
            ws = self.ws
        if ws is not None and not already_stopping:
            try:
                ws.close()
            except Exception as error:
                if self.debug:
                    log(f"WebSocket close failed during shutdown: {error}")

    def on_open(self, _ws: WebSocketApp) -> None:
        with self.lock:
            self.opened_at = time.monotonic()
        log("Connected to Hanabi Live.")

    def on_error(self, _ws: WebSocketApp, error: object) -> None:
        log(f"WebSocket error: {error}")

    def on_close(
        self,
        _ws: WebSocketApp,
        status_code: int | None,
        message: str | None,
    ) -> None:
        detail = "" if status_code is None else f" ({status_code}: {message or ''})"
        log(f"Hanabi Live connection closed{detail}.")

    def on_message(self, _ws: WebSocketApp, message: str) -> None:
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
        playing_at_tables = data.get("playingAtTables", [])
        if playing_at_tables:
            table_id = int(playing_at_tables[0])
            if len(playing_at_tables) > 1:
                log(
                    "Account is seated at multiple ongoing tables; "
                    f"reattending table {table_id} first."
                )
            else:
                log(f"Reattending ongoing table {table_id}.")
            self.send("tableReattend", {"tableID": table_id})
            return
        log(f"To invite the bot, privately message it: /msg {self.username} /join")

    @staticmethod
    def handle_server_notice(data: object) -> None:
        log(f"Hanabi Live: {data}")

    def handle_chat(self, data: dict[str, Any]) -> None:
        if data.get("recipient") != self.username:
            return
        message = str(data.get("msg", "")).strip()
        requester = str(data["who"])
        if message.startswith("/level"):
            self._handle_level(requester, message)
            return
        if message != "/join":
            return
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

    def _handle_level(self, requester: str, message: str) -> None:
        parts = message.split()
        if not 1 <= len(parts) <= 2 or parts[0] != "/level":
            self.reply(requester, "Usage: /level <1-25|max>")
            return
        query = len(parts) == 1
        level = None if query else parts[1].lower()
        if level is not None and level != "max":
            try:
                number = int(level)
            except ValueError:
                number = 0
            if not 1 <= number <= 25 or level != str(number):
                self.reply(requester, "Level must be 1 through 25, or max.")
                return
        if self.default_h_group_level is None:
            self.reply(requester, "This bot is not running the H-Group convention.")
            return

        old_engine: EngineSession | None = None
        active_table: int
        with self.lock:
            target = self._level_target(requester)
            if target is None:
                self.reply(requester, "You must be seated at my table to query or set its level.")
                return
            active_table, game = target

            current_level = (
                game.h_group_level
                if game is not None
                else self.game_levels.get(active_table, self.default_h_group_level)
            )
            if query:
                display = self._level_display(str(current_level))
                self.reply(requester, f"Current H-Group level: {display}.")
                return

            assert level is not None
            self.game_levels[active_table] = level
            if game is not None and game.h_group_level != level:
                self.game_generation += 1
                game.generation = self.game_generation
                game.h_group_level = level
                old_engine = game.reset_engine()
                game.in_flight = False

        if old_engine is not None:
            old_engine.close()
        display = self._level_display(level)
        log(f"Table {active_table}: {requester} selected H-Group {display}.")
        self.reply(requester, f"H-Group {display} selected for this game.")
        self.maybe_move(active_table)

    def _level_target(
        self,
        requester: str,
    ) -> tuple[int, LiveGame | None] | None:
        active = next(
            (
                (table_id, game)
                for table_id, game in self.games.items()
                if requester in game.player_names
            ),
            None,
        )
        if active is not None:
            return active
        table = next(
            (
                candidate
                for candidate in self.tables.values()
                if requester in candidate.get("players", [])
            ),
            None,
        )
        if table is None:
            return None
        return int(table["id"]), None

    @staticmethod
    def _level_display(level: str) -> str:
        return "max" if level == "max" else f"level {level}"

    def handle_table(self, data: dict[str, Any]) -> None:
        with self.lock:
            self.tables[int(data["id"])] = data

    def handle_table_list(self, data: list[dict[str, Any]]) -> None:
        for table in data:
            self.handle_table(table)

    def handle_table_gone(self, data: dict[str, Any]) -> None:
        table_id = int(data["tableID"])
        with self.lock:
            self.tables.pop(table_id, None)
            if table_id not in self.games:
                self.game_levels.pop(table_id, None)

    def handle_table_start(self, data: dict[str, Any]) -> None:
        self.send("getGameInfo1", {"tableID": int(data["tableID"])})

    def handle_init(self, data: dict[str, Any]) -> None:
        table_id = int(data["tableID"])
        with self.lock:
            old = self.games.pop(table_id, None)
            self.game_generation += 1
            self.games[table_id] = LiveGame(
                table_id=table_id,
                player_names=list(data["playerNames"]),
                our_player_index=int(data["ourPlayerIndex"]),
                spectating=bool(data.get("spectating", False)),
                replay=bool(data.get("replay", False)),
                options=data["options"],
                h_group_level=self.game_levels.get(
                    table_id,
                    self.default_h_group_level,
                ),
                generation=self.game_generation,
            )
        self._close_game_engine(old)
        self.send("getGameInfo2", {"tableID": table_id})

    def handle_game_action_list(self, data: dict[str, Any]) -> None:
        table_id = int(data["tableID"])
        with self.lock:
            game = self.games[table_id]
            self.game_generation += 1
            old_engine = game.engine
            game.load_actions(list(data["list"]), self.game_generation)
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
            game.actions.append(action)
            game.update_progress(action)
        self.maybe_move(table_id)

    def handle_game_finished(self, data: dict[str, Any]) -> None:
        table_id = int(data.get("tableID", -1))
        if table_id >= 0:
            self.send("tableUnattend", {"tableID": table_id})
            with self.lock:
                game = self.games.pop(table_id, None)
                self.game_levels.pop(table_id, None)
            self._close_game_engine(game)

    def maybe_move(self, table_id: int) -> None:
        with self.lock:
            game = self.games.get(table_id)
            if game is None:
                return
            if (
                game.spectating
                or game.replay
                or not game.action_list_loaded
                or game.terminal
                or game.current_player != int(game.our_player_index)
                or game.last_decided_turn == game.turn
                or game.in_flight
            ):
                return
            game.in_flight = True
            turn = int(game.turn)
            action_count = len(game.actions)
            generation = int(game.generation)
            snapshot = game.snapshot(action_count)
        try:
            self.executor.submit(
                self._decide,
                table_id,
                turn,
                action_count,
                generation,
                snapshot,
            )
        except RuntimeError:
            with self.lock:
                game = self.games.get(table_id)
                if game is not None:
                    game.in_flight = False

    def _decide(
        self,
        table_id: int,
        turn: int,
        action_count: int,
        generation: int,
        snapshot: dict[str, Any],
    ) -> None:
        trace_context = None
        try:
            trace_context = self._trace(
                "begin_decision",
                table_id,
                turn,
                action_count,
                generation,
                snapshot,
            )
            engine_response = self._request_action(
                table_id,
                action_count,
                generation,
                trace_context,
            )
            action_value = engine_response.get("action", engine_response)
            if not isinstance(action_value, dict):
                raise EngineProcessError("engine response action is not a JSON object")
            action = action_value
            with self.lock:
                game = self.games.get(table_id)
                current = (
                    game is not None
                    and game.generation == generation
                    and game.turn == turn
                    and game.current_player == int(game.our_player_index)
                    and not game.terminal
                )
            if not current:
                log(f"Discarding stale engine result for table {table_id}, turn {turn}.")
                if trace_context is not None:
                    self._trace(
                        "finish_decision",
                        trace_context,
                        "stale",
                        action=action,
                    )
                return
            if int(action.get("tableID", -1)) != table_id:
                raise EngineProcessError("engine response targets the wrong table")
            log(f"Table {table_id}, turn {turn}: sending {action}")
            self.send("action", action)
            if trace_context is not None:
                self._trace(
                    "finish_decision",
                    trace_context,
                    "sent",
                    action=action,
                )
            with self.lock:
                game = self.games.get(table_id)
                if game is not None and game.generation == generation:
                    game.last_decided_turn = turn
        except (EngineProcessError, OSError, TypeError, ValueError) as error:
            log(f"Engine failed on table {table_id}, turn {turn}: {error}")
            if trace_context is not None:
                self._trace(
                    "finish_decision",
                    trace_context,
                    "failed",
                    error=error,
                )
        finally:
            with self.lock:
                game = self.games.get(table_id)
                if game is not None and game.generation == generation:
                    game.in_flight = False

    def _request_action(
        self,
        table_id: int,
        action_count: int,
        generation: int,
        trace_context: dict[str, Any] | None = None,
    ) -> dict[str, Any]:
        last_error: EngineProcessError | None = None
        for attempt in range(1, 3):
            with self.lock:
                game = self.games.get(table_id)
                if game is None or game.generation != generation:
                    raise EngineProcessError("game session is no longer current")
                engine = game.engine
                if engine is None:
                    engine = self.engine_factory(
                        self._engine_command_for_game(game),
                        self.engine_timeout,
                    )
                    game.engine = engine
                    game.engine_initialized = False
                    game.synced_actions = 0
                initialized = bool(game.engine_initialized)
                synced = int(game.synced_actions)
                if initialized and synced <= action_count:
                    payload = {
                        "kind": "append",
                        "tableID": table_id,
                        "actions": game.actions[synced:action_count],
                    }
                else:
                    snapshot = game.snapshot(action_count)
                    payload = {"kind": "initialize", "snapshot": snapshot}
            if trace_context is not None:
                self._trace("record_request", trace_context, attempt, payload)
            try:
                response = engine.request(payload)
            except EngineProcessError as error:
                last_error = error
                if trace_context is not None:
                    self._trace(
                        "record_engine_error",
                        trace_context,
                        attempt,
                        error,
                    )
                with self.lock:
                    game = self.games.get(table_id)
                    if game is not None and game.engine is engine:
                        game.reset_engine()
                engine.close()
                continue
            if trace_context is not None:
                self._trace(
                    "record_response",
                    trace_context,
                    attempt,
                    response,
                )
            with self.lock:
                game = self.games.get(table_id)
                if game is None or game.generation != generation:
                    raise EngineProcessError("game session changed during planning")
                game.engine_initialized = True
                game.synced_actions = action_count
            return response
        assert last_error is not None
        raise last_error

    def _trace(self, method: str, *args: Any, **kwargs: Any) -> Any:
        if self.trace_recorder is None:
            return None
        try:
            return getattr(self.trace_recorder, method)(*args, **kwargs)
        except OSError as error:
            log(f"Could not write Hanabi Live trace data: {error}")
            return None

    @staticmethod
    def _close_game_engine(game: LiveGame | None) -> None:
        if game is not None:
            game.close_engine()

    @staticmethod
    def _command_option(command: list[str], option: str) -> str | None:
        try:
            index = command.index(option)
        except ValueError:
            return None
        if index + 1 >= len(command):
            return None
        return command[index + 1]

    def _engine_command_for_game(self, game: LiveGame) -> list[str]:
        command = self.engine_command.copy()
        level = game.h_group_level
        if level is None:
            return command
        try:
            index = command.index("--h-group-level")
        except ValueError:
            command.extend(["--h-group-level", str(level)])
        else:
            command[index + 1] = str(level)
        return command

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
    parser.add_argument(
        "--exact-world-limit",
        type=int,
        default=int(
            os.getenv("HANABI_ENGINE_EXACT_WORLD_LIMIT", DEFAULT_EXACT_WORLD_LIMIT)
        ),
        help=f"worlds allowed in exact endgame planning (default: {DEFAULT_EXACT_WORLD_LIMIT})",
    )
    parser.add_argument(
        "--exact-node-limit",
        type=int,
        default=int(
            os.getenv("HANABI_ENGINE_EXACT_NODE_LIMIT", DEFAULT_EXACT_NODE_LIMIT)
        ),
        help=f"nodes allowed in exact endgame planning (default: {DEFAULT_EXACT_NODE_LIMIT})",
    )
    parser.add_argument(
        "--objective",
        choices=("expected-score", "perfect-score"),
        default="perfect-score",
        help="planning objective (default: perfect-score)",
    )
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


def run_with_signal_handlers(bot: HanabiEngineBot) -> None:
    previous_handlers: dict[signal.Signals, Any] = {}

    def handle_shutdown(signum: int, _frame: Any) -> None:
        if bot.stop_event.is_set():
            return
        signal_name = signal.Signals(signum).name
        log(f"Received {signal_name}; shutting down.")
        bot.stop()

    shutdown_signals = [signal.SIGINT]
    if hasattr(signal, "SIGTERM"):
        shutdown_signals.append(signal.SIGTERM)
    for shutdown_signal in shutdown_signals:
        previous_handlers[shutdown_signal] = signal.signal(
            shutdown_signal,
            handle_shutdown,
        )
    try:
        bot.run()
    finally:
        for shutdown_signal, previous_handler in previous_handlers.items():
            signal.signal(shutdown_signal, previous_handler)


def main() -> int:
    arguments = parse_arguments()
    repository = Path(__file__).resolve().parents[1]
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
    try:
        validate_engine_binary(engine)
    except EngineProcessError as error:
        log(f"error: {error}")
        return 2
    if (
        arguments.exact_world_limit <= 0
        or arguments.exact_node_limit <= 0
    ):
        log("error: planning budgets must be positive")
        return 2
    if arguments.engine_timeout <= 0:
        log("error: the engine timeout must be positive")
        return 2
    engine_command = [
        str(engine),
        "live-session",
        "--exact-world-limit",
        str(arguments.exact_world_limit),
        "--exact-node-limit",
        str(arguments.exact_node_limit),
        "--convention",
        arguments.convention,
        "--objective",
        arguments.objective,
        "--include-planning-details",
    ]
    if arguments.convention == "h-group":
        engine_command.extend(["--h-group-level", arguments.h_group_level])

    profile = (
        f"H-Group {arguments.h_group_level}"
        if arguments.convention == "h-group"
        else "no convention"
    )
    trace_root = repository / "logs" / "hanabi-live"
    try:
        trace_recorder = TraceRecorder(
            trace_root,
            {
                "baseURL": arguments.base_url,
                "username": username,
                "profile": profile,
                "engineCommand": engine_command,
                "engineTimeoutSeconds": arguments.engine_timeout,
            },
        )
        configure_log_file(trace_recorder.run_directory / "bot.log")
    except OSError as error:
        log(f"error: could not create the default trace directory: {error}")
        return 2
    log(f"Connecting to {arguments.base_url} with {profile}.")
    log(f"Writing player-safe traces to {trace_recorder.run_directory}.")
    bot = HanabiEngineBot(
        base_url=arguments.base_url,
        engine_command=engine_command,
        username=username,
        password=password,
        debug=arguments.debug,
        engine_timeout=arguments.engine_timeout,
        trace_recorder=trace_recorder,
    )
    try:
        run_with_signal_handlers(bot)
    finally:
        close_log_file()
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
