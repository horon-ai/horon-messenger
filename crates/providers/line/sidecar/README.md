# LINE sidecar

A Node program that wraps the linejs client and speaks the horon-messenger
line-delimited JSON-RPC protocol on stdin/stdout. The Rust provider
(`horon-messenger-provider-line`) spawns it and calls the operations below.

## Install

```bash
cd crates/providers/line/sidecar
npm install          # resolves @jsr/evex__linejs from the JSR npm mirror
```

Then either put `index.js` on `PATH` as `horon-messenger-line-sidecar`, or point
the provider at it directly:

```bash
export HORON_MESSENGER_LINE_SIDECAR="$PWD/crates/providers/line/sidecar/index.js"
```

`HORON_MESSENGER_CONFIG_DIR` (the same variable the CLI reads) selects where the
per-account linejs storage file lives under `line-storage/`.

## Protocol

One JSON-RPC 2.0 object per line. Methods:

| Method | Params | Result |
|--------|--------|--------|
| `auth.login` | `{ token }` or `{ email, password }`, optional `device`, `account_id` | `{ authenticated, account_id, display_name, device, auth_token }` |
| `auth.status` | `{}` | `{ authenticated }` |
| `auth.logout` | `{}` | `{}` |
| `whoami` | `{}` | `{ mid, display_name, status_message?, picture_url? }` |
| `chats.list` | `{ limit }` | `{ chats: [{ chat_id, type, display_name, member_count? }] }` |
| `messages.list` | `{ chat_id, count }` | `{ messages: [{ message_id, chat_id, author_id, author_name?, text, content_type, sent_at }] }` |
| `members.list` | `{ chat_id }` | `{ members: [{ mid, display_name? }] }` |
| `message.send` | `{ chat_id, text }` | `{ success, chat_id, message_id, sent_at }` |

An error response carries `error.data` with a code the Rust side classifies:
`AUTH_FAILED`, `AUTH_REQUIRED`, `NOT_FOUND`, `E2EE_REQUIRED`, or `INTERNAL`.

## Out of scope

End-to-end encryption (Letter Sealing) and the long-poll listener are not
implemented. Messages that arrive as encrypted chunks are reported with `text:
null`; a send to a chat that forces Letter Sealing fails with `E2EE_REQUIRED`.
