# Telegram provider QA

Re-runnable smoke test for `hyper-messenger-provider-telegram`. It builds the
`hyper-messenger` CLI twice and drives it with the `qa-fake` backend, which
replays fixture TDLib JSON through the same mapping functions the real TDLib path
uses. No `libtdjson` is needed at any point.

```bash
bash crates/providers/telegram/tests/qa/run.sh
```

The first build enables `hyper-messenger-provider-telegram/qa-fake` alongside
`hyper-messenger-sdk/all` and covers `whoami`, `chats` and `messages` over
`tests/fixtures`, asserting each `{ok,data,meta}` envelope with `python3 -c`.

Adversarial cases:

- `HYPER_MESSENGER_TELEGRAM_FAKE_STATE=closed` makes the fake report a closed
  authorization state, so `whoami` must return `error.code == AUTH_REQUIRED`.
- `HYPER_MESSENGER_TELEGRAM_FAKE_MALFORMED=1` makes the fake emit a fixture that
  is not valid JSON, so `whoami` must return `error.code == PROTOCOL`.

The second build has neither `qa-fake` nor `tdlib`, so the provider has no TDLib
backend. `whoami telegram` must exit non-zero with a typed `error.code == CONFIG`
message naming TDLib, and must not panic. The missing-library load path itself
(`TdlibLibrary::load_from`) is covered by the crate unit tests.
