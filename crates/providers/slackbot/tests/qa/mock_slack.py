#!/usr/bin/env python3
import json
import sys
from http.server import BaseHTTPRequestHandler, HTTPServer

FAIL_AUTH = {"value": False}
MALFORMED = {"value": False}


class Handler(BaseHTTPRequestHandler):
    def _reply(self, obj):
        body = json.dumps(obj).encode()
        self.send_response(200)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)

    def _handle(self):
        length = int(self.headers.get("Content-Length") or 0)
        if length:
            self.rfile.read(length)
        path = self.path.split("?")[0]
        if path == "/__mode/ping":
            self._reply({"ok": True})
            return
        if path == "/__mode/fail_auth":
            FAIL_AUTH["value"] = True
            self._reply({"ok": True})
            return
        if path == "/__mode/malformed":
            MALFORMED["value"] = True
            self._reply({"ok": True})
            return
        if FAIL_AUTH["value"]:
            self._reply({"ok": False, "error": "invalid_auth"})
            return
        method = path.rsplit("/", 1)[-1]
        if method == "auth.test":
            self._reply(
                {
                    "ok": True,
                    "user_id": "UBOT",
                    "team_id": "T1",
                    "bot_id": "B1",
                    "user": "botuser",
                    "team": "TestWorkspace",
                }
            )
        elif method == "conversations.list":
            self._reply(
                {
                    "ok": True,
                    "channels": [{"id": "C1", "name": "general", "num_members": 3}],
                }
            )
        elif method == "conversations.info":
            self._reply({"ok": True, "channel": {"id": "C1", "name": "general"}})
        elif method == "conversations.history":
            if MALFORMED["value"]:
                self._reply({"ok": True})
            else:
                self._reply(
                    {
                        "ok": True,
                        "messages": [
                            {"ts": "1700000000.000100", "user": "U1", "text": "hello"}
                        ],
                        "has_more": False,
                    }
                )
        elif method == "conversations.members":
            self._reply({"ok": True, "members": ["U1"]})
        elif method == "users.info":
            self._reply(
                {"ok": True, "user": {"id": "U1", "name": "alice", "real_name": "Alice"}}
            )
        elif method == "chat.postMessage":
            self._reply({"ok": True, "ts": "1700000000.000200"})
        else:
            self._reply({"ok": False, "error": "unknown_method"})

    def do_POST(self):
        self._handle()

    def do_GET(self):
        self._handle()

    def log_message(self, *args):
        pass


if __name__ == "__main__":
    port = int(sys.argv[1]) if len(sys.argv) > 1 else 8813
    HTTPServer(("127.0.0.1", port), Handler).serve_forever()
