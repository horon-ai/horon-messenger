#!/usr/bin/env python3
"""Mock Telegram Bot API for horon-messenger QA.

Answers HTTP 200 for every request, as the real Bot API does: a token other than
"mock" yields {ok:false, error_code:401, description:"Unauthorized"} so the client
must map the in-body error to a typed AuthFailed. Serves getMe, getChat,
getUpdates, getChatAdministrators and sendMessage, plus GET /__stats for a
readiness probe.
"""
import json
import sys
import threading
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from urllib.parse import parse_qs, urlparse

TOKEN = "mock"
BOT_ID = 123456789
CHAT_ID = 42
USER_ID = 7

BOT = {
    "id": BOT_ID,
    "is_bot": True,
    "first_name": "MockBot",
    "username": "mock_bot",
    "can_join_groups": True,
    "can_read_all_group_messages": False,
    "supports_inline_queries": False,
}
USER = {"id": USER_ID, "is_bot": False, "first_name": "Ada", "last_name": "Lovelace", "username": "ada"}
CHAT = {"id": CHAT_ID, "type": "private", "first_name": "Ada", "last_name": "Lovelace", "username": "ada"}
SEED_MESSAGE = {
    "message_id": 500,
    "date": 1700000000,
    "chat": CHAT,
    "from": USER,
    "text": "seeded hello from mock telegram bot",
}

LOCK = threading.Lock()
COUNTS = {}


def bump(key):
    with LOCK:
        COUNTS[key] = COUNTS.get(key, 0) + 1


def snapshot():
    with LOCK:
        return dict(COUNTS)


def as_chat_id(value):
    text = str(value)
    return int(text) if text.lstrip("-").isdigit() else text


class Handler(BaseHTTPRequestHandler):
    protocol_version = "HTTP/1.1"

    def log_message(self, *args):
        pass

    def _send(self, code, body):
        payload = json.dumps(body).encode()
        self.send_response(code)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(payload)))
        self.end_headers()
        self.wfile.write(payload)

    def _raw(self, code, payload):
        self.send_response(code)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(payload)))
        self.end_headers()
        self.wfile.write(payload)

    def _ok(self, result):
        self._send(200, {"ok": True, "result": result})

    def _body(self):
        length = int(self.headers.get("Content-Length") or 0)
        if not length:
            return {}
        try:
            return json.loads(self.rfile.read(length))
        except json.JSONDecodeError:
            return {}

    def _dispatch(self):
        parsed = urlparse(self.path)
        if parsed.path == "/__stats":
            return self._send(200, snapshot())
        if parsed.path == "/__reset":
            with LOCK:
                COUNTS.clear()
            return self._send(200, {"reset": True})

        parts = parsed.path.strip("/").split("/")
        token = parts[0][3:] if len(parts) >= 1 and parts[0].startswith("bot") else ""
        method = parts[1] if len(parts) >= 2 else ""
        if token != TOKEN:
            return self._send(200, {"ok": False, "error_code": 401, "description": "Unauthorized"})

        query = parse_qs(parsed.query)
        bump(f"{method}")
        if method == "getMe":
            return self._ok(BOT)
        if method == "getUpdates":
            return self._ok([{"update_id": 1, "message": SEED_MESSAGE}])
        if method == "getChat":
            chat_id = as_chat_id(query.get("chat_id", [CHAT_ID])[0])
            return self._ok(dict(CHAT, id=chat_id))
        if method == "getChatAdministrators":
            return self._ok([{"user": USER, "status": "creator"}])
        if method == "sendMessage":
            body = self._body()
            if body.get("text") == "__BADJSON__":
                return self._raw(200, b"<html>not json</html>")
            created = {
                "message_id": 9001,
                "date": 1700000001,
                "chat": {"id": as_chat_id(body.get("chat_id", CHAT_ID)), "type": "private"},
                "from": BOT,
                "text": body.get("text", ""),
            }
            return self._ok(created)
        return self._send(200, {"ok": False, "error_code": 404, "description": f"Not Found: {method}"})

    def do_GET(self):
        self._dispatch()

    def do_POST(self):
        self._dispatch()


def main():
    port = int(sys.argv[1]) if len(sys.argv) > 1 else 8811
    server = ThreadingHTTPServer(("127.0.0.1", port), Handler)
    print(f"mock telegram bot on {port}", flush=True)
    server.serve_forever()


if __name__ == "__main__":
    main()
