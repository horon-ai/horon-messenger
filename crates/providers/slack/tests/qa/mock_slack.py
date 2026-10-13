#!/usr/bin/env python3
"""Mock Slack Web API for hyper-messenger QA.

Requires `Authorization: Bearer xoxc-mock-token` plus the `Cookie: d=mock-cookie`
header. Any other credential returns HTTP 200 with {"ok": false, "error":
"invalid_auth"}, so the client must treat a misleading success body as an auth
failure. Serves the methods the Slack provider calls: auth.test,
conversations.list/info/history/members, users.info, chat.postMessage and
search.messages.
"""
import json
import sys
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from urllib.parse import parse_qs, urlparse

TOKEN = "xoxc-mock-token"
COOKIE = "d=mock-cookie"

USER_ID = "U1"
OTHER_ID = "U2"
CHANNEL_ID = "C1"
SEED_TS = "1700000000.000100"

WORKSPACE = {
    "ok": True,
    "url": "https://mock.slack.com/",
    "team": "Test Workspace",
    "user": "alice",
    "team_id": "T1",
    "user_id": USER_ID,
}
CHANNEL = {
    "id": CHANNEL_ID,
    "name": "general",
    "is_private": False,
    "is_archived": False,
    "is_channel": True,
    "is_im": False,
    "is_mpim": False,
    "num_members": 2,
}
SEED_MESSAGE = {
    "type": "message",
    "user": OTHER_ID,
    "text": "seeded hello from mock slack",
    "ts": SEED_TS,
}
SELF_USER = {
    "id": USER_ID,
    "name": "alice",
    "real_name": "Alice",
    "is_bot": False,
    "profile": {"display_name": "alice", "real_name": "Alice"},
}
OTHER_USER = {
    "id": OTHER_ID,
    "name": "bob",
    "real_name": "Bob Smith",
    "is_bot": False,
    "profile": {"display_name": "Bob", "real_name": "Bob Smith"},
}


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

    def _authed(self):
        token_ok = self.headers.get("Authorization") == f"Bearer {TOKEN}"
        cookie_ok = self.headers.get("Cookie") == COOKIE
        if not (token_ok and cookie_ok):
            self._send(200, {"ok": False, "error": "invalid_auth"})
            return False
        return True

    def do_GET(self):
        if not self._authed():
            return
        parsed = urlparse(self.path)
        path = parsed.path
        query = parse_qs(parsed.query)
        if path == "/auth.test":
            return self._send(200, WORKSPACE)
        if path == "/conversations.list":
            return self._send(
                200,
                {
                    "ok": True,
                    "channels": [CHANNEL],
                    "response_metadata": {"next_cursor": ""},
                },
            )
        if path == "/conversations.info":
            return self._send(200, {"ok": True, "channel": CHANNEL})
        if path == "/conversations.history":
            return self._send(
                200,
                {"ok": True, "messages": [SEED_MESSAGE], "has_more": False},
            )
        if path == "/conversations.members":
            return self._send(
                200,
                {
                    "ok": True,
                    "members": [USER_ID, OTHER_ID],
                    "response_metadata": {"next_cursor": ""},
                },
            )
        if path == "/users.info":
            user_id = (query.get("user") or [""])[0]
            if user_id == USER_ID:
                return self._send(200, {"ok": True, "user": SELF_USER})
            return self._send(200, {"ok": True, "user": OTHER_USER})
        if path == "/search.messages":
            return self._send(
                200,
                {
                    "ok": True,
                    "messages": {
                        "total": 1,
                        "matches": [
                            {
                                "ts": SEED_TS,
                                "text": "seeded hello from mock slack",
                                "user": OTHER_ID,
                                "username": "bob",
                                "channel": {"id": CHANNEL_ID, "name": "general"},
                                "permalink": (
                                    "https://mock.slack.com/archives/"
                                    f"{CHANNEL_ID}/p1700000000000100"
                                ),
                            }
                        ],
                    },
                },
            )
        return self._send(200, {"ok": False, "error": "unknown_method"})

    def do_POST(self):
        if not self._authed():
            return
        path = urlparse(self.path).path
        length = int(self.headers.get("Content-Length") or 0)
        body = {}
        if length:
            try:
                body = json.loads(self.rfile.read(length))
            except json.JSONDecodeError:
                body = {}
        if path == "/chat.postMessage":
            ts = "1700000000.000999"
            created = {"type": "message", "text": body.get("text", ""), "ts": ts}
            return self._send(
                200,
                {
                    "ok": True,
                    "channel": body.get("channel", CHANNEL_ID),
                    "ts": ts,
                    "message": created,
                },
            )
        return self._send(200, {"ok": False, "error": "unknown_method"})


def main():
    port = int(sys.argv[1]) if len(sys.argv) > 1 else 8814
    server = ThreadingHTTPServer(("127.0.0.1", port), Handler)
    print(f"mock slack on {port}", flush=True)
    server.serve_forever()


if __name__ == "__main__":
    main()
