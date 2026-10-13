# Instagram provider QA

Re-runnable smoke test for `horon-messenger-provider-instagram`. It boots a mock
Instagram mobile API (`mock_instagram.py`, `127.0.0.1:8831`), drives the real
`horon-messenger` CLI over it with `HORON_MESSENGER_INSTAGRAM_API_BASE`,
`HORON_MESSENGER_INSTAGRAM_SESSIONID`, `HORON_MESSENGER_INSTAGRAM_DS_USER_ID`
and `HORON_MESSENGER_INSTAGRAM_DEVICE_ID`, asserts each `{ok,data,meta}` envelope
with `python3 -c`, then kills the mock and prints a `cleanup:` line.

```bash
bash crates/providers/instagram/tests/qa/run.sh
```

Covered: `whoami`, `chats`, `messages`, `send`, and an adversarial case where the
session cookie is wrong. The mock answers a missing or wrong `sessionid` with
HTTP 401, so the CLI must surface `error.code == AUTH_FAILED`. The seeded thread
returns items newest-first; the CLI must reverse them, so `messages[0]` is the
chronological first item.
