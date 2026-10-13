# Slack provider QA

Re-runnable smoke test for `horon-messenger-provider-slack`. It boots a mock
Slack Web API (`mock_slack.py`, `127.0.0.1:8814`), drives the real
`horon-messenger` CLI over it with `HORON_MESSENGER_SLACK_API_BASE`,
`HORON_MESSENGER_SLACK_TOKEN` and `HORON_MESSENGER_SLACK_COOKIE`, asserts each
`{ok,data,meta}` envelope with `python3 -c`, then kills the mock and prints a
`cleanup:` line.

```bash
bash crates/providers/slack/tests/qa/run.sh
```

Covered: `whoami`, `chats`, `messages`, `send`, and an adversarial case where the
`d=` cookie is dropped. The mock answers a missing or wrong credential with HTTP
200 and `{"ok": false, "error": "invalid_auth"}`, so the CLI must surface
`error.code == AUTH_FAILED` rather than treating the 200 as success.
