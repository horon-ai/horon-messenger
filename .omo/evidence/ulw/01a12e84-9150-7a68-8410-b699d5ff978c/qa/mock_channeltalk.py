#!/usr/bin/env python3
"""Mock Channel Talk desk API for hyper-messenger QA; 401s on a missing account cookie."""
import json
import sys
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


class Handler(BaseHTTPRequestHandler):
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

    def do_GET(self):
        if not self._authed():
            return
        path = self.path.split("?")[0]
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
        if not self._authed():
            return
        path = self.path.split("?")[0]
        body = self._body()
        text = ""
        for block in body.get("blocks", []):
            if isinstance(block, dict) and block.get("value"):
                text = block["value"]
        if path == f"/desk/channels/{WORKSPACE_ID}/groups/{GROUP_ID}/messages":
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
