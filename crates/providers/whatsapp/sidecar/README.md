# WhatsApp sidecar

A Node sidecar that bridges the Baileys WhatsApp Web library to the
`horon-messenger-provider-whatsapp` crate over the line-delimited JSON-RPC 2.0
protocol defined by `horon-messenger-provider-common`.

## Install

```bash
npm ci
```

The pinned dependency is `@whiskeysockets/baileys` `7.0.0-rc14` (see
`package.json`); `package-lock.json` is committed so installs are reproducible.
Node.js 20 or newer is required at run time. The Rust build never invokes Node.

## Launch

The Rust provider spawns the sidecar as `node index.mjs`. The Node binary is
taken from `HORON_MESSENGER_NODE_BIN` when set, otherwise `node` on `PATH`. The
script path is `crates/providers/whatsapp/sidecar/index.mjs` at build time, or the
path in `HORON_MESSENGER_WHATSAPP_SIDECAR` for an installed layout.

## Protocol

Requests and responses are one JSON object per line on stdin and stdout. Every
request carries `jsonrpc: "2.0"`, an `id`, a `method`, and `params`. Every
response echoes the `id` and carries either `result` or `error`. All diagnostic
output goes to stderr so stdout stays reserved for JSON-RPC.

Every method receives `auth_dir`, the directory holding the Baileys multi-file
auth state (default `<config>/whatsapp/accounts/default`).

| Method | Params | Result |
| --- | --- | --- |
| `auth.status` | `auth_dir` | `{ authenticated, account: { id, name, phone_number } \| null }` |
| `auth.login` | `auth_dir`, `method?`, `phone_number?` | `{ authenticated, account_id, next_action, message }` |
| `chats.list` | `auth_dir`, `search?`, `limit?`, `chat_id?` | array of `{ id, name, type, unread_count, last_message?, members? }` |
| `messages.list` | `auth_dir`, `chat_id`, `limit?`, `search?` | array of `{ id, chat_id, from, from_name?, timestamp, is_outgoing, type, text? }` |
| `message.send` | `auth_dir`, `chat_id`, `text`, `reply_to?` | one message summary in the same shape as `messages.list` |

`chats.list` returns group members in `members` so the provider can answer
`get_chat` and `list_members` from the same call.

Interactive device linking is performed by running the sidecar's socket flow;
`auth.login` prepares the auth directory and reports `next_action: "scan_qr"` when
the account is not yet linked. Real network pairing is outside the offline QA.
