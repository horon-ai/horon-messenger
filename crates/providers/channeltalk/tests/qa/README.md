# Channel Talk provider QA

Re-runnable smoke test for `hyper-messenger-provider-channeltalk`. It boots a mock
Channel Talk desk API (`mock_channeltalk.py`, `127.0.0.1:8772`), drives the real
`hyper-messenger` CLI over it with `HYPER_MESSENGER_CHANNELTALK_API_BASE` and
`HYPER_MESSENGER_CHANNELTALK_ACCOUNT_COOKIE`, asserts each `{ok,data,meta}`
envelope with `python3 -c`, then kills the mock and prints a `cleanup:` line.

```bash
bash crates/providers/channeltalk/tests/qa/run.sh
```

Covered: `whoami`, `chats`, `messages`, `send`, and an adversarial send whose
response is truncated mid-body. The truncated send must surface
`error.code == NETWORK` with an "unknown" outcome message and must be sent
exactly once (the mock asserts the POST count is 1), proving the non-idempotent
send is never auto-retried.
