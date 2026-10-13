#!/usr/bin/env python3
import json
import os
import sys
import time

MODE = os.environ.get("LINE_STUB_MODE", "ok")

CHATS = [
    {"chat_id": "u_friend", "type": "user", "display_name": "Friend", "member_count": 1},
    {"chat_id": "g_crew", "type": "group", "display_name": "Crew", "member_count": 3},
]


def emit(obj):
    sys.stdout.write(json.dumps(obj) + "\n")
    sys.stdout.flush()


def reply(req, result):
    emit({"jsonrpc": "2.0", "id": req.get("id"), "result": result})


def fail(req, message, data):
    emit({"jsonrpc": "2.0", "id": req.get("id"), "error": {"code": -32000, "message": message, "data": data}})


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

    if MODE == "authfail":
        fail(req, "login failed", "AUTH_FAILED: invalid token")
        continue

    if method == "auth.login":
        params = req.get("params", {})
        reply(
            req,
            {
                "authenticated": True,
                "account_id": "u_self",
                "display_name": "Self",
                "device": params.get("device", "ANDROIDSECONDARY"),
                "auth_token": "returned-token",
            },
        )
    elif method == "whoami":
        reply(req, {"mid": "u_self", "display_name": "Self", "status_message": "hi"})
    elif method == "chats.list":
        reply(req, {"chats": CHATS})
    elif method == "messages.list":
        cid = req.get("params", {}).get("chat_id")
        reply(
            req,
            {
                "messages": [
                    {
                        "message_id": "m1",
                        "chat_id": cid,
                        "author_id": "u_friend",
                        "author_name": "Friend",
                        "text": "hi",
                        "content_type": "NONE",
                        "sent_at": "2026-06-24T00:00:00.000Z",
                    }
                ]
            },
        )
    elif method == "members.list":
        reply(req, {"members": [{"mid": "u_friend", "display_name": "Friend"}]})
    elif method == "message.send":
        reply(
            req,
            {
                "success": True,
                "chat_id": req.get("params", {}).get("chat_id"),
                "message_id": "sent-1",
                "sent_at": "2026-06-24T00:02:00.000Z",
            },
        )
    else:
        fail(req, "Method not found", "INTERNAL")
