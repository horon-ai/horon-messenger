#!/usr/bin/env python3
"""Mock WhatsApp Cloud (Graph) API for hyper-messenger QA on a fixed port."""
import json
import sys
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

TOKEN = "mock-token"
PHONE_ID = "123456789012345"
MESSAGE_ID = "wamid.mock123"


class Handler(BaseHTTPRequestHandler):
    protocol_version = "HTTP/1.1"

    def log_message(self, *args):
        pass

    def _send(self, code, body):
        payload = body.encode()
        self.send_response(code)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(payload)))
        self.end_headers()
        self.wfile.write(payload)

    def _authed(self):
        if self.headers.get("Authorization") != f"Bearer {TOKEN}":
            self._send(401, json.dumps({"error": {"message": "Invalid OAuth access token", "type": "OAuthException", "code": 190}}))
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
        path = self.path.split("?")[0]
        if not self._authed():
            return
        if path == f"/{PHONE_ID}":
            return self._send(200, json.dumps({"id": PHONE_ID, "verified_name": "Mock Business"}))
        return self._send(404, json.dumps({"error": {"message": "Unsupported get request", "code": 100}}))

    def do_POST(self):
        path = self.path.split("?")[0]
        if not self._authed():
            return
        if path == f"/{PHONE_ID}/messages":
            body = self._body()
            if body.get("text", {}).get("body") == "__MALFORMED__":
                return self._send(200, '{"messaging_product":"whatsapp","messages":[')
            to = body.get("to", "")
            return self._send(200, json.dumps({
                "messaging_product": "whatsapp",
                "contacts": [{"input": to, "wa_id": to}],
                "messages": [{"id": MESSAGE_ID}],
            }))
        return self._send(404, json.dumps({"error": {"message": "Unsupported post request", "code": 100}}))


def main():
    port = int(sys.argv[1]) if len(sys.argv) > 1 else 8812
    server = ThreadingHTTPServer(("127.0.0.1", port), Handler)
    print(f"mock graph on {port}", flush=True)
    server.serve_forever()


if __name__ == "__main__":
    main()
