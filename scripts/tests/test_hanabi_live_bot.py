from __future__ import annotations

import json
import sys
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


class RecordingEngine:
    instances: list[RecordingEngine] = []

    def __init__(self, _command: list[str], _timeout: float) -> None:
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


def make_bot(engine_factory: Any) -> tuple[bridge.HanabiEngineBot, FakeWebSocket]:
    bot = bridge.HanabiEngineBot(
        base_url="http://example.invalid",
        engine_command=["fake-engine", "live-session"],
        username="Bot",
        password="secret",
        debug=False,
        engine_factory=engine_factory,
    )
    socket = FakeWebSocket()
    bot.ws = socket
    bot.connection_generation = 1
    return bot, socket


class PersistentEngineTests(unittest.TestCase):
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

    def test_searches_on_two_tables_without_blocking_callbacks(self) -> None:
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


if __name__ == "__main__":
    unittest.main()
