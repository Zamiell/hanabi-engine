from __future__ import annotations

import json
import signal
import sys
import tempfile
import threading
import time
import types
import unittest
import urllib.parse
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path
from typing import Any
from unittest import mock


SCRIPTS = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(SCRIPTS))

import hanabi_live_bot as bridge  # noqa: E402
import hanabi_live_engine as engine_process  # noqa: E402


class FakeWebSocket:
    def __init__(self) -> None:
        self.messages: list[str] = []
        self.lock = threading.Lock()

    def send(self, message: str) -> None:
        with self.lock:
            self.messages.append(message)

    def close(self) -> None:
        pass

    def actions(self) -> list[dict[str, Any]]:
        with self.lock:
            return [
                json.loads(message.partition(" ")[2])
                for message in self.messages
                if message.startswith("action ")
            ]

    def private_messages(self) -> list[dict[str, Any]]:
        with self.lock:
            return [
                json.loads(message.partition(" ")[2])
                for message in self.messages
                if message.startswith("chatPM ")
            ]


class RecordingEngine:
    instances: list[RecordingEngine] = []

    def __init__(self, command: list[str], _timeout: float) -> None:
        self.command = command
        self.payloads: list[dict[str, Any]] = []
        self.closed = False
        self.__class__.instances.append(self)

    def request(self, payload: dict[str, Any]) -> dict[str, Any]:
        self.payloads.append(payload)
        table_id = payload.get("tableID")
        if table_id is None:
            table_id = payload["snapshot"]["tableID"]
        return {"tableID": table_id, "type": 0, "target": 0}

    def close(self) -> None:
        self.closed = True


class DetailedRecordingEngine(RecordingEngine):
    def request(self, payload: dict[str, Any]) -> dict[str, Any]:
        response = super().request(payload)
        return {
            "action": response,
            "logicalDeductions": {"ownCards": []},
            "conventionInferences": {"framework": "h-group"},
            "planning": {"phase": "symbolic", "rootActions": []},
        }


class BlockingEngine(RecordingEngine):
    started = threading.Barrier(3)
    release = threading.Event()

    def request(self, payload: dict[str, Any]) -> dict[str, Any]:
        self.payloads.append(payload)
        self.__class__.started.wait(timeout=2)
        self.__class__.release.wait(timeout=2)
        table_id = payload["snapshot"]["tableID"]
        return {"tableID": table_id, "type": 0, "target": 0}


class FailingEngine(RecordingEngine):
    failures_remaining = 1

    def request(self, payload: dict[str, Any]) -> dict[str, Any]:
        self.payloads.append(payload)
        if self.__class__.failures_remaining > 0:
            self.__class__.failures_remaining -= 1
            raise bridge.EngineProcessError("simulated engine crash")
        table_id = payload["snapshot"]["tableID"]
        return {"tableID": table_id, "type": 0, "target": 0}


def init_message(table_id: int) -> dict[str, Any]:
    return {
        "tableID": table_id,
        "playerNames": ["Bot", "Alice"],
        "ourPlayerIndex": 0,
        "spectating": False,
        "replay": False,
        "options": {"variantName": "No Variant"},
    }


def wait_until(predicate: Any, timeout: float = 2.0) -> None:
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        if predicate():
            return
        time.sleep(0.01)
    raise AssertionError("condition was not satisfied before timeout")


def make_bot(
    engine_factory: Any,
    trace_recorder: bridge.TraceRecorder | None = None,
    engine_command: list[str] | None = None,
) -> tuple[bridge.HanabiEngineBot, FakeWebSocket]:
    bot = bridge.HanabiEngineBot(
        base_url="http://example.invalid",
        engine_command=engine_command or ["fake-engine", "live-session"],
        username="Bot",
        password="secret",
        debug=False,
        engine_factory=engine_factory,
        trace_recorder=trace_recorder,
    )
    socket = FakeWebSocket()
    bot.ws = socket
    bot.connection_generation = 1
    return bot, socket


class PersistentEngineTests(unittest.TestCase):
    def test_closed_engine_cannot_restart(self) -> None:
        engine = bridge.PersistentEngine(["unused-engine"], 1)
        engine.close()
        with self.assertRaisesRegex(bridge.EngineProcessError, "session is closed"):
            engine.request({"kind": "initialize"})

    def test_rejects_a_stale_engine_before_connecting(self) -> None:
        stale = mock.Mock()
        stale.returncode = 0
        stale.stdout = "hanabi-engine live-action [options] < live-snapshot.json\n"
        stale.stderr = ""
        with (
            mock.patch.object(engine_process.subprocess, "run", return_value=stale) as run,
            mock.patch.dict(
                bridge.os.environ,
                {"HANABI_USERNAME": "Bot", "HANABI_PASSWORD": "secret"},
            ),
            self.assertRaisesRegex(bridge.EngineProcessError, "cargo build --release"),
        ):
            bridge.validate_engine_binary(Path("/tmp/hanabi-engine"))

        environment = run.call_args.kwargs["env"]
        self.assertNotIn("HANABI_USERNAME", environment)
        self.assertNotIn("HANABI_PASSWORD", environment)

    def test_reuses_one_child_process_for_multiple_requests(self) -> None:
        program = (
            "import json,sys\n"
            "for line in sys.stdin:\n"
            " request=json.loads(line)\n"
            " table=request.get('tableID', request.get('snapshot',{}).get('tableID'))\n"
            " print(json.dumps({'tableID':table,'type':0,'target':0}), flush=True)\n"
        )
        engine = bridge.PersistentEngine([sys.executable, "-u", "-c", program], 2)
        try:
            first = engine.request(
                {"kind": "initialize", "snapshot": {"tableID": 7}}
            )
            assert engine.process is not None
            process_id = engine.process.pid
            second = engine.request({"kind": "append", "tableID": 7, "actions": []})
            self.assertEqual(first["tableID"], 7)
            self.assertEqual(second["tableID"], 7)
            assert engine.process is not None
            self.assertEqual(engine.process.pid, process_id)
        finally:
            engine.close()

    def test_reconnect_delay_is_bounded_and_resets_after_stability(self) -> None:
        self.assertEqual(bridge.next_reconnect_delay(1, 0), 2)
        self.assertEqual(bridge.next_reconnect_delay(30, 0), 30)
        self.assertEqual(bridge.next_reconnect_delay(16, 30), 1)

    def test_authenticates_again_before_each_reconnect(self) -> None:
        authentications: list[str] = []

        def authenticator(_base_url: str, _username: str, _password: str) -> tuple[str, str]:
            cookie = f"session={len(authentications) + 1}"
            authentications.append(cookie)
            return "ws://local.test/ws", cookie

        class ReconnectingWebSocket:
            instances: list[ReconnectingWebSocket] = []
            bot: bridge.HanabiEngineBot

            def __init__(self, _url: str, **callbacks: Any) -> None:
                self.cookie = callbacks["cookie"]
                self.on_open = callbacks["on_open"]
                self.__class__.instances.append(self)

            def run_forever(self) -> None:
                self.on_open(self)
                if len(self.__class__.instances) >= 2:
                    self.__class__.bot.stop_event.set()

            def close(self) -> None:
                pass

        bot = bridge.HanabiEngineBot(
            base_url="http://local.test",
            engine_command=["fake-engine"],
            username="Bot",
            password="secret",
            debug=False,
            authenticator=authenticator,
        )
        ReconnectingWebSocket.bot = bot
        fake_websocket = types.SimpleNamespace(WebSocketApp=ReconnectingWebSocket)
        with (
            mock.patch.object(bridge, "websocket", fake_websocket),
            mock.patch.object(bridge, "INITIAL_RECONNECT_DELAY", 0.01),
        ):
            bot.run()

        self.assertEqual(authentications, ["session=1", "session=2"])
        self.assertEqual(
            [instance.cookie for instance in ReconnectingWebSocket.instances],
            authentications,
        )

    def test_sigint_stops_instead_of_reconnecting(self) -> None:
        authentications = 0

        def authenticator(_base_url: str, _username: str, _password: str) -> tuple[str, str]:
            nonlocal authentications
            authentications += 1
            return "ws://local.test/ws", "session=1"

        class InterruptedWebSocket:
            closed = False

            def __init__(self, _url: str, **callbacks: Any) -> None:
                self.on_open = callbacks["on_open"]

            def run_forever(self) -> None:
                self.on_open(self)
                signal.raise_signal(signal.SIGINT)

            def close(self) -> None:
                self.__class__.closed = True

        bot = bridge.HanabiEngineBot(
            base_url="http://local.test",
            engine_command=["fake-engine"],
            username="Bot",
            password="secret",
            debug=False,
            authenticator=authenticator,
        )
        fake_websocket = types.SimpleNamespace(WebSocketApp=InterruptedWebSocket)
        with mock.patch.object(bridge, "websocket", fake_websocket):
            bridge.run_with_signal_handlers(bot)

        self.assertTrue(bot.stop_event.is_set())
        self.assertTrue(InterruptedWebSocket.closed)
        self.assertEqual(authentications, 1)

    def test_authenticate_works_with_a_local_http_server(self) -> None:
        received: dict[str, str] = {}

        class LoginHandler(BaseHTTPRequestHandler):
            def do_POST(self) -> None:  # noqa: N802
                length = int(self.headers["Content-Length"])
                received["path"] = self.path
                received["body"] = self.rfile.read(length).decode()
                self.send_response(200)
                self.send_header("Set-Cookie", "session=test-cookie; Path=/")
                self.end_headers()

            def log_message(self, _format: str, *_args: Any) -> None:
                pass

        server = ThreadingHTTPServer(("127.0.0.1", 0), LoginHandler)
        thread = threading.Thread(target=server.serve_forever, daemon=True)
        thread.start()
        try:
            host, port = server.server_address
            ws_url, cookie = bridge.authenticate(
                f"http://{host}:{port}",
                "Bot User",
                "correct horse",
            )
        finally:
            server.shutdown()
            server.server_close()
            thread.join(timeout=2)

        self.assertEqual(ws_url, f"ws://{host}:{port}/ws")
        self.assertEqual(cookie, "session=test-cookie")
        self.assertEqual(received["path"], "/login")
        form = urllib.parse.parse_qs(received["body"])
        self.assertEqual(form["username"], ["Bot User"])
        self.assertEqual(form["password"], ["correct horse"])
        self.assertEqual(form["version"], ["bot"])


class BotConcurrencyTests(unittest.TestCase):
    def setUp(self) -> None:
        RecordingEngine.instances = []

    def test_welcome_reattends_an_ongoing_game(self) -> None:
        bot, socket = make_bot(RecordingEngine)
        try:
            bot.handle_welcome(
                {
                    "username": "hanabi-engine",
                    "playingAtTables": [27337],
                }
            )
            self.assertIn(
                'tableReattend {"tableID":27337}',
                socket.messages,
            )
        finally:
            bot.shutdown()

    def test_welcome_waits_for_an_invitation_without_an_ongoing_game(self) -> None:
        bot, socket = make_bot(RecordingEngine)
        try:
            bot.handle_welcome(
                {
                    "username": "hanabi-engine",
                    "playingAtTables": [],
                }
            )
            self.assertFalse(
                any(message.startswith("tableReattend ") for message in socket.messages)
            )
        finally:
            bot.shutdown()

    def test_private_level_message_configures_the_game_engine(self) -> None:
        bot, socket = make_bot(
            RecordingEngine,
            engine_command=[
                "fake-engine",
                "live-session",
                "--convention",
                "h-group",
                "--h-group-level",
                "max",
            ],
        )
        try:
            bot.handle_init(init_message(7))
            bot.handle_chat(
                {"recipient": "Bot", "who": "Alice", "msg": "/level 3"}
            )
            self.assertEqual(bot.games[7].h_group_level, "3")
            self.assertEqual(
                socket.private_messages()[-1]["msg"],
                "H-Group level 3 selected for this game.",
            )
            bot.handle_chat({"recipient": "Bot", "who": "Alice", "msg": "/level"})
            self.assertEqual(
                socket.private_messages()[-1]["msg"],
                "Current H-Group level: level 3.",
            )

            bot.handle_game_action_list(
                {
                    "tableID": 7,
                    "list": [{"type": "turn", "num": 0, "currentPlayerIndex": 0}],
                }
            )
            wait_until(lambda: len(socket.actions()) == 1)
            command = RecordingEngine.instances[0].command
            self.assertEqual(command[command.index("--h-group-level") + 1], "3")
        finally:
            bot.shutdown()

    def test_changing_level_restarts_only_that_games_engine(self) -> None:
        bot, socket = make_bot(
            RecordingEngine,
            engine_command=[
                "fake-engine",
                "live-session",
                "--convention",
                "h-group",
                "--h-group-level",
                "max",
            ],
        )
        try:
            bot.handle_init(init_message(7))
            bot.handle_game_action_list(
                {
                    "tableID": 7,
                    "list": [{"type": "turn", "num": 0, "currentPlayerIndex": 0}],
                }
            )
            wait_until(lambda: len(socket.actions()) == 1)
            original = RecordingEngine.instances[0]

            bot.handle_chat(
                {"recipient": "Bot", "who": "Alice", "msg": "/level 3"}
            )
            self.assertTrue(original.closed)
            bot.handle_game_action(
                {
                    "tableID": 7,
                    "action": {"type": "turn", "num": 1, "currentPlayerIndex": 1},
                }
            )
            bot.handle_game_action(
                {
                    "tableID": 7,
                    "action": {"type": "turn", "num": 2, "currentPlayerIndex": 0},
                }
            )
            wait_until(lambda: len(socket.actions()) == 2)
            self.assertEqual(len(RecordingEngine.instances), 2)
            replacement = RecordingEngine.instances[1]
            self.assertEqual(
                replacement.command[replacement.command.index("--h-group-level") + 1],
                "3",
            )
            self.assertEqual(replacement.payloads[0]["kind"], "initialize")
        finally:
            bot.shutdown()

    def test_level_message_validates_sender_value_and_convention(self) -> None:
        bot, socket = make_bot(RecordingEngine)
        try:
            bot.handle_init(init_message(7))
            bot.handle_chat(
                {"recipient": "Bot", "who": "Alice", "msg": "/level 26"}
            )
            self.assertEqual(
                socket.private_messages()[-1]["msg"],
                "Level must be 1 through 25, or max.",
            )
            bot.handle_chat(
                {"recipient": "Bot", "who": "Alice", "msg": "/level 3"}
            )
            self.assertEqual(
                socket.private_messages()[-1]["msg"],
                "This bot is not running the H-Group convention.",
            )
        finally:
            bot.shutdown()

    def test_bare_level_reports_the_default_without_restarting(self) -> None:
        bot, socket = make_bot(
            RecordingEngine,
            engine_command=[
                "fake-engine",
                "live-session",
                "--convention",
                "h-group",
                "--h-group-level",
                "max",
            ],
        )
        try:
            bot.handle_init(init_message(7))
            bot.handle_chat({"recipient": "Bot", "who": "Alice", "msg": "/level"})
            self.assertEqual(
                socket.private_messages()[-1]["msg"],
                "Current H-Group level: max.",
            )
            self.assertEqual(RecordingEngine.instances, [])
        finally:
            bot.shutdown()

    def test_uses_initial_snapshot_then_only_new_actions(self) -> None:
        bot, socket = make_bot(RecordingEngine)
        try:
            bot.handle_init(init_message(7))
            bot.handle_game_action_list(
                {
                    "tableID": 7,
                    "list": [
                        {"type": "turn", "num": 0, "currentPlayerIndex": 0}
                    ],
                }
            )
            wait_until(lambda: len(socket.actions()) == 1)

            bot.handle_game_action(
                {
                    "tableID": 7,
                    "action": {
                        "type": "turn",
                        "num": 1,
                        "currentPlayerIndex": 1,
                    },
                }
            )
            bot.handle_game_action(
                {
                    "tableID": 7,
                    "action": {
                        "type": "turn",
                        "num": 2,
                        "currentPlayerIndex": 0,
                    },
                }
            )
            wait_until(lambda: len(socket.actions()) == 2)

            self.assertEqual(len(RecordingEngine.instances), 1)
            payloads = RecordingEngine.instances[0].payloads
            self.assertEqual(payloads[0]["kind"], "initialize")
            self.assertEqual(len(payloads[0]["snapshot"]["actions"]), 1)
            self.assertEqual(payloads[1]["kind"], "append")
            self.assertEqual(len(payloads[1]["actions"]), 2)
        finally:
            bot.shutdown()

    def test_writes_player_safe_decision_trace(self) -> None:
        with tempfile.TemporaryDirectory() as temporary_directory:
            recorder = bridge.TraceRecorder(
                Path(temporary_directory),
                {
                    "username": "Bot",
                    "engineCommand": ["fake-engine", "live-session"],
                },
            )
            bot, socket = make_bot(DetailedRecordingEngine, recorder)
            try:
                bot.handle_init(init_message(7))
                actions = [
                    {
                        "type": "draw",
                        "playerIndex": 0,
                        "order": 0,
                        "suitIndex": -1,
                        "rank": -1,
                    },
                    {"type": "turn", "num": 0, "currentPlayerIndex": 0},
                ]
                bot.handle_game_action_list({"tableID": 7, "list": actions})
                wait_until(lambda: len(socket.actions()) == 1)

                table_directory = recorder.run_directory / "tables" / "7"
                wait_until(lambda: len(list(table_directory.glob("*.result.json"))) == 1)
                snapshot_path = next(table_directory.glob("*.snapshot.json"))
                request_path = next(table_directory.glob("*.request.json"))
                response_path = next(table_directory.glob("*.response.json"))
                result_path = next(table_directory.glob("*.result.json"))

                snapshot = json.loads(snapshot_path.read_text(encoding="utf-8"))
                request = json.loads(request_path.read_text(encoding="utf-8"))
                response = json.loads(response_path.read_text(encoding="utf-8"))
                result = json.loads(result_path.read_text(encoding="utf-8"))
                events = [
                    json.loads(line)
                    for line in (recorder.run_directory / "events.jsonl")
                    .read_text(encoding="utf-8")
                    .splitlines()
                ]

                self.assertEqual(snapshot["actions"], actions)
                self.assertEqual(snapshot["actions"][0]["suitIndex"], -1)
                self.assertEqual(request["kind"], "initialize")
                self.assertEqual(response["action"]["tableID"], 7)
                self.assertEqual(response["planning"]["phase"], "symbolic")
                self.assertEqual(result["status"], "sent")
                self.assertEqual(
                    [event["kind"] for event in events],
                    [
                        "decisionStarted",
                        "engineRequest",
                        "engineResponse",
                        "decisionFinished",
                    ],
                )
                trace_text = "\n".join(
                    path.read_text(encoding="utf-8")
                    for path in recorder.run_directory.rglob("*")
                    if path.is_file()
                )
                self.assertNotIn("secret", trace_text)
            finally:
                bot.shutdown()

    def test_plans_on_two_tables_without_blocking_callbacks(self) -> None:
        BlockingEngine.instances = []
        BlockingEngine.started = threading.Barrier(3)
        BlockingEngine.release = threading.Event()
        bot, socket = make_bot(BlockingEngine)
        try:
            started = time.monotonic()
            for table_id in (7, 8):
                bot.handle_init(init_message(table_id))
                bot.handle_game_action_list(
                    {
                        "tableID": table_id,
                        "list": [
                            {
                                "type": "turn",
                                "num": 0,
                                "currentPlayerIndex": 0,
                            }
                        ],
                    }
                )
            elapsed = time.monotonic() - started
            self.assertLess(elapsed, 0.5)
            BlockingEngine.started.wait(timeout=2)
            self.assertEqual(len(BlockingEngine.instances), 2)
            BlockingEngine.release.set()
            wait_until(lambda: len(socket.actions()) == 2)
        finally:
            BlockingEngine.release.set()
            bot.shutdown()

    def test_restarts_and_resynchronizes_after_an_engine_failure(self) -> None:
        FailingEngine.instances = []
        FailingEngine.failures_remaining = 1
        bot, socket = make_bot(FailingEngine)
        try:
            bot.handle_init(init_message(7))
            bot.handle_game_action_list(
                {
                    "tableID": 7,
                    "list": [
                        {"type": "turn", "num": 0, "currentPlayerIndex": 0}
                    ],
                }
            )
            wait_until(lambda: len(socket.actions()) == 1)
            self.assertEqual(len(FailingEngine.instances), 2)
            self.assertEqual(
                [engine.payloads[0]["kind"] for engine in FailingEngine.instances],
                ["initialize", "initialize"],
            )
            self.assertTrue(FailingEngine.instances[0].closed)
        finally:
            bot.shutdown()

    def test_trace_records_failed_engine_attempt_and_retry(self) -> None:
        FailingEngine.instances = []
        FailingEngine.failures_remaining = 1
        with tempfile.TemporaryDirectory() as temporary_directory:
            recorder = bridge.TraceRecorder(Path(temporary_directory), {})
            bot, socket = make_bot(FailingEngine, recorder)
            try:
                bot.handle_init(init_message(7))
                bot.handle_game_action_list(
                    {
                        "tableID": 7,
                        "list": [
                            {"type": "turn", "num": 0, "currentPlayerIndex": 0}
                        ],
                    }
                )
                wait_until(lambda: len(socket.actions()) == 1)

                table_directory = recorder.run_directory / "tables" / "7"
                wait_until(
                    lambda: len(list(table_directory.glob("*.result.json"))) == 1
                )
                self.assertEqual(len(list(table_directory.glob("*.request.json"))), 2)
                self.assertEqual(len(list(table_directory.glob("*.error.json"))), 1)
                self.assertEqual(len(list(table_directory.glob("*.response.json"))), 1)
                result_path = next(table_directory.glob("*.result.json"))
                result = json.loads(result_path.read_text(encoding="utf-8"))
                self.assertEqual(result["status"], "sent")
            finally:
                bot.shutdown()


if __name__ == "__main__":
    unittest.main()
