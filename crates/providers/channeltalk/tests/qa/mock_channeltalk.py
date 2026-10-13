#!/usr/bin/env python3
"""Mock Channel Talk desk API for hyper-messenger QA.

401s on a missing account cookie. Serves the routes the Channel Talk provider
calls, plus two QA-only endpoints: GET /__stats returns per-route request counts
and GET /__reset clears them. A POST message whose text is __DROP__ writes a
partial body and closes the connection mid-response, so the client must treat the
outcome as unknown and must not retry the send.
"""
import json
import sys
import threading
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

ACCOUNT_COOKIE = "mock-account"
WORKSPACE_ID = "ws1"
GROUP_ID = "g1"

ACCOUNT = {
    "id": "acc1",
    "name": "Operator",
    "email": "op@example.com",
    "emailVerified": True,
    "language": "ko",
    "country": "KR",
    "createdAt": 1,
}
CHANNEL = {"id": WORKSPACE_ID, "name": "Test Workspace", "state": "opened"}
SEED_MESSAGE = {
    "id": "msg1",
    "chatId": GROUP_ID,
    "chatType": "group",
    "personType": "manager",
    "createdAt": 1700000000000,
    "blocks": [{"type": "text", "value": "seeded hello from mock channel talk"}],
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
        self.end_headers()
        self.wfile.write(payload)

    def _authed(self):
        cookie = self.headers.get("Cookie") or ""
        if f"x-account={ACCOUNT_COOKIE}" not in cookie:
            self._send(401, {"type": "unauthorized", "status": 401, "errors": [{"message": "unauthorized"}]})
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
        self.end_headers()
        self.wfile.write(b'{"message": {"id": "msg')
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
        if path == "/desk/account":
            return self._send(200, {"account": ACCOUNT})
        if path == "/desk/channels":
            return self._send(200, {"channels": [CHANNEL]})
        if path == f"/desk/channels/{WORKSPACE_ID}":
            return self._send(200, {"channel": CHANNEL})
        if path == f"/desk/channels/{WORKSPACE_ID}/managers":
            return self._send(200, {"managers": [{"id": "m1", "channelId": WORKSPACE_ID, "accountId": "acc1", "name": "Operator"}]})
        if path == f"/desk/channels/{WORKSPACE_ID}/groups/{GROUP_ID}/messages":
            return self._send(200, {"messages": [SEED_MESSAGE]})
        if path == f"/desk/channels/{WORKSPACE_ID}/team-chat/message/search":
            hit = {"index": "i", "score": "1.0", "source": SEED_MESSAGE, "highlight": {}, "searchAfter": [1, "x"]}
            return self._send(200, {"hits": [hit], "bots": [], "sessions": []})
        return self._send(404, {"type": "not_found", "status": 404, "errors": [{"message": "not found"}]})

    def do_POST(self):
        path = self.path.split("?")[0]
        if not self._authed():
            return
        bump(f"POST {path}")
        body = self._body()
        text = ""
        for block in body.get("blocks", []):
            if isinstance(block, dict) and block.get("value"):
                text = block["value"]
        if path == f"/desk/channels/{WORKSPACE_ID}/groups/{GROUP_ID}/messages":
            if text == "__DROP__":
                return self._drop_mid_response()
            created = dict(SEED_MESSAGE, id="msg2", blocks=[{"type": "text", "value": text}])
            return self._send(200, {"message": created})
        return self._send(404, {"type": "not_found", "status": 404, "errors": [{"message": "not found"}]})


def main():
    port = int(sys.argv[1]) if len(sys.argv) > 1 else 8772
    server = ThreadingHTTPServer(("127.0.0.1", port), Handler)
    print(f"mock channeltalk on {port}", flush=True)
    server.serve_forever()


if __name__ == "__main__":
    main()
