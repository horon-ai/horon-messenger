# WhatsApp Bot provider QA

Re-runnable smoke test for `hyper-messenger-provider-whatsappbot`. It boots a mock
WhatsApp Cloud (Graph) API (`mock_graph.py`, `127.0.0.1:8812`), drives the real
`hyper-messenger` CLI over it with `HYPER_MESSENGER_WHATSAPPBOT_API_BASE` plus the
phone-number-id and access-token env vars, asserts each `{ok,data,meta}` envelope
with `python3 -c`, then kills the mock and prints a `cleanup:` line.

```bash
bash crates/providers/whatsappbot/tests/qa/run.sh
```

Covered: `send` returns `accepted:true` with the mock message id `wamid.mock123`;
`whoami` returns the phone number id as `account_id`; a bad token yields
`AUTH_FAILED`; `chats` yields `UNSUPPORTED` because the Cloud API cannot list
chats; and a malformed JSON body yields `PROTOCOL`.
