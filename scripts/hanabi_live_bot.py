#!/usr/bin/env python3

from __future__ import annotations

import argparse
import http.cookies
import json
import os
import subprocess
import sys
import urllib.error
import urllib.parse
import urllib.request
from pathlib import Path
from typing import Any

try:
    import websocket
except ImportError:
    websocket = None


DEFAULT_BASE_URL = "https://hanab.live"
DEFAULT_ITERATIONS = 1_000
DEFAULT_SEED = 0


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


class HanabiEngineBot:
    def __init__(
        self,
        ws_url: str,
        cookie: str,
        engine_command: list[str],
        username: str,
        debug: bool,
    ) -> None:
        self.username = username
        self.engine_command = engine_command
        self.debug = debug
        self.tables: dict[int, dict[str, Any]] = {}
        self.games: dict[int, dict[str, Any]] = {}
        self.last_decided_turn: dict[int, int] = {}
        self.deciding = False
        self.ws = websocket.WebSocketApp(
            ws_url,
            cookie=cookie,
            on_open=self.on_open,
            on_message=self.on_message,
            on_error=self.on_error,
            on_close=self.on_close,
        )

    def run(self) -> None:
        self.ws.run_forever()

    def on_open(self, _ws: websocket.WebSocketApp) -> None:
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
        table = next(
            (
                candidate
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
        self.tables[int(data["id"])] = data

    def handle_table_list(self, data: list[dict[str, Any]]) -> None:
        for table in data:
            self.handle_table(table)

    def handle_table_gone(self, data: dict[str, Any]) -> None:
        self.tables.pop(int(data["tableID"]), None)

    def handle_table_start(self, data: dict[str, Any]) -> None:
        self.send("getGameInfo1", {"tableID": int(data["tableID"])})

    def handle_init(self, data: dict[str, Any]) -> None:
        table_id = int(data["tableID"])
        self.games[table_id] = {
            "tableID": table_id,
            "playerNames": data["playerNames"],
            "ourPlayerIndex": data["ourPlayerIndex"],
            "spectating": data.get("spectating", False),
            "replay": data.get("replay", False),
            "options": data["options"],
            "actions": [],
        }
        self.send("getGameInfo2", {"tableID": table_id})

    def handle_game_action_list(self, data: dict[str, Any]) -> None:
        table_id = int(data["tableID"])
        snapshot = self.games[table_id]
        snapshot["actions"] = list(data["list"])
        self.send("loaded", {"tableID": table_id})
        self.maybe_move(table_id)

    def handle_game_action(self, data: dict[str, Any]) -> None:
        table_id = int(data["tableID"])
        snapshot = self.games.get(table_id)
        if snapshot is None:
            return
        snapshot["actions"].append(data["action"])
        self.maybe_move(table_id)

    def handle_game_finished(self, data: dict[str, Any]) -> None:
        table_id = int(data.get("tableID", -1))
        if table_id >= 0:
            self.send("tableUnattend", {"tableID": table_id})
            self.games.pop(table_id, None)
            self.last_decided_turn.pop(table_id, None)

    def maybe_move(self, table_id: int) -> None:
        if self.deciding:
            return
        snapshot = self.games[table_id]
        if snapshot.get("spectating") or snapshot.get("replay"):
            return

        turn = 0
        current_player = 0
        terminal = False
        for action in snapshot["actions"]:
            action_type = action.get("type")
            if action_type == "turn":
                turn = int(action["num"])
                current_player = int(action["currentPlayerIndex"])
            elif action_type == "gameOver":
                terminal = True
        if (
            terminal
            or current_player != int(snapshot["ourPlayerIndex"])
            or self.last_decided_turn.get(table_id) == turn
        ):
            return

        self.deciding = True
        try:
            result = subprocess.run(
                self.engine_command,
                input=json.dumps(snapshot),
                text=True,
                capture_output=True,
                timeout=180,
                check=False,
                env={
                    key: value
                    for key, value in os.environ.items()
                    if key not in {"HANABI_USERNAME", "HANABI_PASSWORD"}
                },
            )
            if result.returncode != 0:
                error = result.stderr.strip() or f"exit status {result.returncode}"
                log(f"Engine failed on table {table_id}, turn {turn}: {error}")
                return
            try:
                action = json.loads(result.stdout)
            except json.JSONDecodeError as error:
                log(f"Engine returned invalid action JSON: {error}")
                return
            self.last_decided_turn[table_id] = turn
            log(f"Table {table_id}, turn {turn}: sending {action}")
            self.send("action", action)
        except subprocess.TimeoutExpired:
            log(f"Engine timed out on table {table_id}, turn {turn}.")
        finally:
            self.deciding = False

    def reply(self, recipient: str, message: str) -> None:
        self.send(
            "chatPM",
            {"msg": message, "recipient": recipient, "room": "lobby"},
        )

    def send(self, command: str, data: dict[str, Any]) -> None:
        self.ws.send(f"{command} {json.dumps(data, separators=(',', ':'))}")
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
    if arguments.seed < 0:
        log("error: the search seed must be nonnegative")
        return 2
    if arguments.exploration is not None and arguments.exploration <= 0:
        log("error: the exploration coefficient must be positive")
        return 2

    engine_command = [
        str(engine),
        "live-action",
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

    try:
        ws_url, cookie = authenticate(
            arguments.base_url,
            username,
            password,
        )
    except (RuntimeError, ValueError) as error:
        log(f"error: {error}")
        return 1

    profile = (
        f"H-Group {arguments.h_group_level}"
        if arguments.convention == "h-group"
        else "no convention"
    )
    log(f"Connecting to {ws_url} with {profile}.")
    HanabiEngineBot(
        ws_url,
        cookie,
        engine_command,
        username,
        arguments.debug,
    ).run()
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
