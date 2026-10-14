# HTTP API server

`horon-messenger-server` exposes the messenger providers over HTTP. Every route
calls the [`Client`](SDK.md) and returns the same [`Envelope`](CORE.md) the CLI
and MCP server speak, so a client parses one shape everywhere.

- Crate: `crates/server` (`horon-messenger-server`)
- Binary: `horon-messenger-server`
- Framework: axum
- npm: `@horon-ai/messenger-server`, and the bundles (see [npm/README](../npm/README.md))

## Run

```bash
cargo run -p horon-messenger-server -- --host 127.0.0.1 --port 8787
npx @horon-ai/messenger-server --port 8787
```

| Flag | Default | Description |
|------|---------|-------------|
| `--host` | `127.0.0.1` | Bind address |
| `--port` | `8787` | Bind port |
| `--config-dir` | platform default | Config dir (env `HORON_MESSENGER_CONFIG_DIR`) |

The server binds to loopback by default. Run it in the background with the CLI's
[`daemon`](CLI.md) commands (`horon-messenger daemon start`), which writes
`server.pid` and `server.log` under the config dir and probes the port.

## Routes

| Method | Path | Description |
|--------|------|-------------|
| GET | `/health` | Liveness: `{"status": "ok"}` |
| GET | `/v1/providers` | Providers this build enabled, with capabilities |
| GET | `/v1/providers/{kind}/whoami` | The authenticated account for a provider |
| GET | `/v1/providers/{kind}/chats` | Chats (`?search=<text>&limit=<n>`) |
| GET | `/v1/providers/{kind}/chats/{chat_id}` | One chat by id |
| GET | `/v1/providers/{kind}/chats/{chat_id}/messages` | Messages (`?search=<text>&limit=<n>`) |
| POST | `/v1/providers/{kind}/chats/{chat_id}/messages` | Send a message |
| GET | `/v1/providers/{kind}/chats/{chat_id}/members` | Members of one chat |
| POST | `/v1/providers/{kind}/chats/{chat_id}/read` | Mark a chat as read |
| GET | `/v1/providers/{kind}/messages` | Search messages (`?chat_id=<id>&search=<text>&limit=<n>`) |
| GET | `/v1/providers/{kind}/events` | Event stream as newline-delimited JSON (`?chat_id=<id>&limit=<n>`) |
| GET | `/v1/providers/{kind}/snapshot` | Profile + chats for a provider |

`{kind}` is a provider kind (`mock`, `discord`, `telegram`, ...). A `POST` body
is JSON:

```json
{"text": "hello", "reply_to": "42"}
```

`reply_to` is optional.

## Examples

```bash
curl localhost:8787/health
curl localhost:8787/v1/providers
curl 'localhost:8787/v1/providers/mock/chats?search=standup'
curl localhost:8787/v1/providers/mock/chats/general
curl 'localhost:8787/v1/providers/mock/messages?search=standup'
curl -X POST localhost:8787/v1/providers/mock/chats/general/messages \
  -H 'content-type: application/json' -d '{"text":"hello"}'
curl -X POST localhost:8787/v1/providers/mock/chats/general/read
curl 'localhost:8787/v1/providers/mock/events?limit=5'   # newline-delimited JSON
```

Responses:

```json
{"ok": true,  "data": { ... }, "meta": {"provider": "mock", "generated_at": "..."}}
{"ok": false, "error": {"code": "AUTH_REQUIRED", "message": "...", "retryable": false, "provider": "discord"}, "meta": { ... }}
```

## Status

All routes above are implemented. The server returns the envelope with HTTP 200
even when `ok` is false; clients branch on the body's `ok`/`error.code`, not the
status line (matching the CLI, which also encodes success in the envelope).

`/events` streams newline-delimited JSON (`application/x-ndjson`): one `Event`
per line, or `{"error": "..."}` if the stream yields an error. `?limit=<n>` stops
the stream after `n` events; without it the stream runs until the provider ends
it or the client disconnects. A provider that does not advertise `listen` returns
the failure envelope instead of a stream.

Not exposed yet: `send_media`, and any provider-specific realtime beyond
`listen`. They exist on the [`Provider`](CORE.md) contract.

## Provider selection

The npm binary builds with every provider. A build with a smaller set comes from
`cargo install --path crates/server --features discord,telegram`; an unenabled
provider then returns `NOT_REGISTERED`.
