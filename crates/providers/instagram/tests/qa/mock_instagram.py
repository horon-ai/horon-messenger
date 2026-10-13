#!/usr/bin/env python3
"""Mock Instagram mobile API for horon-messenger QA.

Every request must carry a `Cookie` header whose `sessionid` equals the seeded
value; any other credential returns HTTP 401 with {"status": "fail", "message":
"login_required"} so the client maps it to AUTH_FAILED. Serves the endpoints the
Instagram provider calls: accounts/current_user, direct_v2/inbox,
direct_v2/threads/<id> and direct_v2/threads/broadcast/text.
"""
import json
import sys
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from urllib.parse import parse_qs, urlparse

VALID_SESSION = "mock-session-id"
SELF_ID = 4242
OTHER_ID = 7
THREAD_ID = "340282366841710300949128130000000000001"
OLDER_ID = "ig-older-1"
NEWER_ID = "ig-newer-2"
SENT_ID = "ig-sent-1"

CURRENT_USER = {
    "status": "ok",
    "user": {
        "pk": SELF_ID,
        "username": "alice",
        "full_name": "Alice Doe",
        "profile_pic_url": "https://cdn.example/alice.jpg",
    },
}
THREAD = {
    "thread_id": THREAD_ID,
    "thread_title": "",
    "thread_type": "private",
    "read_state": 2,
    "users": [{"pk": OTHER_ID, "username": "bob", "full_name": "Bob Lee"}],
    "items": [
        {
            "item_id": NEWER_ID,
            "user_id": SELF_ID,
            "timestamp": 1700000060000000,
            "item_type": "text",
            "text": "second seeded message",
        },
        {
            "item_id": OLDER_ID,
            "user_id": OTHER_ID,
            "timestamp": 1700000000000000,
            "item_type": "text",
            "text": "seeded hello from mock instagram",
        },
    ],
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
        cookie = self.headers.get("Cookie") or ""
        sessionid = ""
        for pair in cookie.split(";"):
            name, _, value = pair.strip().partition("=")
            if name == "sessionid":
                sessionid = value
        if sessionid != VALID_SESSION:
            self._send(401, {"status": "fail", "message": "login_required"})
            return False
        return True

    def _read_form(self):
        length = int(self.headers.get("Content-Length") or 0)
        if not length:
            return {}
        raw = self.rfile.read(length).decode("utf-8", "replace")
        try:
            data = json.loads(raw)
        except json.JSONDecodeError:
            data = raw
        if isinstance(data, str):
            return parse_qs(data)
        if isinstance(data, dict):
            return {key: (value if isinstance(value, list) else [value]) for key, value in data.items()}
        return {}

    def do_GET(self):
        if not self._authed():
            return
        path = urlparse(self.path).path
        if path.endswith("/accounts/current_user/"):
            return self._send(200, CURRENT_USER)
        if path.endswith("/direct_v2/inbox/"):
            return self._send(200, {"status": "ok", "inbox": {"threads": [THREAD]}})
        if "/direct_v2/threads/" in path and path.endswith("/"):
            return self._send(200, {"status": "ok", "thread": THREAD})
        return self._send(200, {"status": "fail", "message": "unknown_endpoint"})

    def do_POST(self):
        if not self._authed():
            return
        path = urlparse(self.path).path
        form = self._read_form()
        if path.endswith("/direct_v2/threads/broadcast/text/"):
            text = (form.get("text") or [""])[0]
            return self._send(
                200,
                {
                    "status": "ok",
                    "payload": {
                        "thread_id": THREAD_ID,
                        "items": [
                            {
                                "item_id": SENT_ID,
                                "user_id": SELF_ID,
                                "timestamp": 1700000120000000,
                                "item_type": "text",
                                "text": text,
                            }
                        ],
                    },
                },
            )
        return self._send(200, {"status": "fail", "message": "unknown_endpoint"})


def main():
    port = int(sys.argv[1]) if len(sys.argv) > 1 else 8831
    server = ThreadingHTTPServer(("127.0.0.1", port), Handler)
    print("mock instagram on " + str(port), flush=True)
    server.serve_forever()


if __name__ == "__main__":
    main()
