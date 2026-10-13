#!/usr/bin/env python3
"""Mock Discord REST API for horon-messenger QA.

401s on a wrong Authorization header. Serves the routes the Discord provider
calls, plus two QA-only endpoints: GET /__stats returns per-route request counts
and GET /__reset clears them. A POST message whose content is __DROP__ writes a
partial body and closes the connection mid-response, so the client must treat the
outcome as unknown and must not retry the send.
"""
import json
import sys
import threading
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

TOKEN = "mock-token"

GUILD_ID = "200000000000000001"
CHANNEL_ID = "300000000000000001"
USER_ID = "100000000000000001"

GUILD = {"id": GUILD_ID, "name": "Test Guild", "owner": True}
CHANNEL = {"id": CHANNEL_ID, "guild_id": GUILD_ID, "name": "general", "type": 0}
USER = {"id": USER_ID, "username": "alice", "global_name": "Alice", "bot": False}
SEED_MESSAGE = {
    "id": "400000000000000001",
    "channel_id": CHANNEL_ID,
    "author": {"id": USER_ID, "username": "alice"},
    "content": "seeded hello from mock discord",
    "timestamp": "2026-01-01T00:00:00.000000+00:00",
    "type": 0,
}

LOCK = threading.Lock()
COUNTS = {}


def bump(key):
    with LOCK:
        COUNTS[key] = COUNTS.get(key, 0) + 1


def snapshot():
    with LOCK:
        return dict(COUNTS)


class Handler(BaseHTTPRequestHandler):
    protocol_version = "HTTP/1.1"

    def log_message(self, *args):
        pass

    def _send(self, code, body):
        payload = json.dumps(body).encode()
        self.send_response(code)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(payload)))
        self.send_header("X-RateLimit-Remaining", "10")
        self.send_header("X-RateLimit-Reset", "9999999999")
        self.end_headers()
        self.wfile.write(payload)

    def _authed(self):
        if self.headers.get("Authorization") != TOKEN:
            self._send(401, {"message": "401: Unauthorized", "code": 0})
            return False
        return True

    def _body(self):
        length = int(self.headers.get("Content-Length") or 0)
        if not length:
            return {}
        try:
            return json.loads(self.rfile.read(length))
        except json.JSONDecodeError:
            return {}

    def _drop_mid_response(self):
        self.send_response(200)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", "500")
        self.send_header("X-RateLimit-Remaining", "10")
        self.send_header("X-RateLimit-Reset", "9999999999")
        self.end_headers()
        self.wfile.write(b'{"id": "400000000000000')
        self.wfile.flush()
        self.close_connection = True

    def do_GET(self):
        path = self.path.split("?")[0]
        if path == "/__stats":
            return self._send(200, snapshot())
        if path == "/__reset":
            with LOCK:
                COUNTS.clear()
            return self._send(200, {"reset": True})
        if not self._authed():
            return
        bump(f"GET {path}")
        if path == "/users/@me":
            return self._send(200, USER)
        if path == "/users/@me/guilds":
            return self._send(200, [GUILD])
        if path == f"/guilds/{GUILD_ID}":
            return self._send(200, GUILD)
        if path == f"/guilds/{GUILD_ID}/channels":
            return self._send(200, [CHANNEL])
        if path == f"/guilds/{GUILD_ID}/members":
            return self._send(200, [{"user": USER, "roles": [], "joined_at": "2026-01-01T00:00:00Z", "deaf": False, "mute": False, "flags": 0}])
        if path == f"/channels/{CHANNEL_ID}/messages":
            return self._send(200, [SEED_MESSAGE])
        if path == f"/guilds/{GUILD_ID}/messages/search":
            hit = dict(SEED_MESSAGE, hit=True, guild_id=GUILD_ID)
            return self._send(200, {"total_results": 1, "messages": [[hit]]})
        return self._send(404, {"message": "404: Not Found", "code": 10003})

    def do_POST(self):
        path = self.path.split("?")[0]
        if not self._authed():
            return
        bump(f"POST {path}")
        body = self._body()
        if path == f"/channels/{CHANNEL_ID}/messages":
            if body.get("content") == "__DROP__":
                return self._drop_mid_response()
            created = dict(SEED_MESSAGE, id="400000000000000999", content=body.get("content", ""))
            return self._send(200, created)
        if path.startswith(f"/channels/{CHANNEL_ID}/messages/") and path.endswith("/ack"):
            return self._send(204, {})
        return self._send(404, {"message": "404: Not Found", "code": 10003})


def main():
    port = int(sys.argv[1]) if len(sys.argv) > 1 else 8771
    server = ThreadingHTTPServer(("127.0.0.1", port), Handler)
    print(f"mock discord on {port}", flush=True)
    server.serve_forever()


if __name__ == "__main__":
    main()
