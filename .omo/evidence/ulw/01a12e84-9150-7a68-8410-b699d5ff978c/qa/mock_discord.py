#!/usr/bin/env python3
"""Mock Discord REST API for hyper-messenger QA; 401s on a wrong Authorization header."""
import json
import sys
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


class Handler(BaseHTTPRequestHandler):
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

    def do_GET(self):
        if not self._authed():
            return
        path = self.path.split("?")[0]
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
        if not self._authed():
            return
        path = self.path.split("?")[0]
        body = self._body()
        if path == f"/channels/{CHANNEL_ID}/messages":
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
