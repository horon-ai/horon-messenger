#!/usr/bin/env python3
import json
import os
import sys
import time

MODE = os.environ.get("IMSG_STUB_MODE", "ok")
ARG = sys.argv[1] if len(sys.argv) > 1 else ""

CHATS = [
    {
        "id": 42,
        "name": "Jane",
        "identifier": "iMessage;-;+15551234567",
        "guid": "iMessage;-;+15551234567",
        "service": "iMessage",
        "is_group": False,
        "participants": ["+15551234567"],
        "last_message": {
            "id": 1,
            "chat_id": 42,
            "guid": "m1",
            "sender": "+15551234567",
            "is_from_me": False,
            "text": "hi",
            "created_at": "2026-06-24T00:00:00.000Z",
        },
    },
    {
        "id": 43,
        "name": "Crew",
        "identifier": "iMessage;+;chat99",
        "guid": "iMessage;+;chat99",
        "service": "iMessage",
        "is_group": True,
        "participants": ["+15551111111", "+15552222222"],
    },
]


def emit(obj):
    sys.stdout.write(json.dumps(obj) + "\n")
    sys.stdout.flush()


def reply(req, result):
    emit({"jsonrpc": "2.0", "id": req.get("id"), "result": result})


def fail(req, code, message, data=None):
    error = {"code": code, "message": message}
    if data is not None:
        error["data"] = data
    emit({"jsonrpc": "2.0", "id": req.get("id"), "error": error})


if ARG == "--version":
    sys.stdout.write("0.11.1\n")
    sys.exit(0)

if ARG != "rpc":
    sys.stderr.write("usage: imsg rpc\n")
    sys.exit(2)

if MODE == "hang":
    time.sleep(3600)

for line in sys.stdin:
    line = line.strip()
    if not line:
        continue
    if MODE == "malformed":
        sys.stdout.write("this is not json\n")
        sys.stdout.flush()
        continue
    req = json.loads(line)
    method = req.get("method")
    if method == "chats.list":
        if MODE == "fda":
            fail(req, -32603, "Internal error", "Permission Error: Cannot access Messages database")
        else:
            reply(req, {"chats": CHATS})
    elif method == "messages.history":
        chat_id = req.get("params", {}).get("chat_id")
        reply(
            req,
            {
                "messages": [
                    {
                        "id": 2,
                        "chat_id": chat_id,
                        "guid": "m2",
                        "sender": "",
                        "is_from_me": True,
                        "text": "yo",
                        "created_at": "2026-06-24T00:01:00.000Z",
                    },
                    {
                        "id": 1,
                        "chat_id": chat_id,
                        "guid": "m1",
                        "sender": "+15551234567",
                        "is_from_me": False,
                        "text": "hi",
                        "created_at": "2026-06-24T00:00:00.000Z",
                    },
                ]
            },
        )
    elif method == "send":
        reply(req, {"ok": True, "id": 99, "guid": "sent-guid", "service": "iMessage"})
    else:
        fail(req, -32601, "Method not found")
