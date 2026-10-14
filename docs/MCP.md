# MCP server

`horon-messenger-mcp` exposes the messenger providers to any MCP client over
stdio. Each tool is a thin wrapper over the [`Client`](SDK.md): the arguments
become a `Client` call and the returned [`Envelope`](CORE.md) becomes the tool
result.

- Crate: `crates/mcp` (`horon-messenger-mcp`)
- Binary: `horon-messenger-mcp`
- Transport: stdio (JSON-RPC), no network port
- npm: `@horon-ai/messenger-mcp`, and `@horon-ai/messenger` (all-in-one)

## Run

```bash
cargo run -p horon-messenger-mcp          # from the workspace
npx @horon-ai/messenger-mcp               # from npm, once published
```

The server reads credentials from the same config dir the other surfaces use
(`HORON_MESSENGER_CONFIG_DIR`, else the platform config dir). Logs go to stderr
so they never corrupt the stdio protocol on stdout.

## Client configuration

Register it with an MCP client as a stdio server. For example:

```json
{
  "mcpServers": {
    "horon-messenger": {
      "command": "npx",
      "args": ["-y", "@horon-ai/messenger-mcp"],
      "env": { "HORON_MESSENGER_CONFIG_DIR": "/Users/you/.config/horon-messenger" }
    }
  }
}
```

The server reports its name as `horon-messenger` and its version from the crate.

## Tools

| Tool | Arguments | Returns |
|------|-----------|---------|
| `list_providers` | none | Providers this build enabled, with capabilities |
| `whoami` | `provider` | The authenticated account for a provider |
| `list_chats` | `provider`, `search?`, `limit?` | Chats, optionally filtered by a name substring |
| `list_messages` | `provider`, `chat_id`, `search?`, `limit?` | Messages in one chat, optionally filtered by a text substring |
| `send_message` | `provider`, `chat_id`, `text`, `reply_to?` | The send result (accepted, delivery, message id) |
| `list_members` | `provider`, `chat_id` | The members of one chat |
| `get_chat` | `provider`, `chat_id` | One chat by id |
| `search_messages` | `provider`, `chat_id?`, `search?`, `limit?` | Messages matching a text substring, in one chat or across every chat |
| `mark_read` | `provider`, `chat_id` | Marks a chat as read |
| `snapshot` | `provider` | The profile and every chat |
| `listen` | `provider`, `chat_id?`, `limit?` | Recent events (message, read, typing), up to `limit` (default 20) |

`provider` is one of the enabled provider kinds (`mock`, `discord`, `telegram`,
`channeltalk`, and so on). `chat_id` is a value returned by `list_chats`.

## Result shape

A successful call returns the envelope's `data` as structured content:

```json
{"ok": true, "data": [ ... ], "meta": {"provider": "mock", "generated_at": "..."}}
```

A failed call returns the same envelope as a structured error, so the client sees
the typed `error.code` (`NOT_REGISTERED`, `AUTH_REQUIRED`, `UNSUPPORTED`, ...)
instead of a transport failure.

## Status

Eleven tools are implemented: `list_providers`, `whoami`, `list_chats`,
`list_messages`, `send_message`, `list_members`, `get_chat`, `search_messages`,
`mark_read`, `snapshot`, `listen`. The server advertises the tools capability
only.

`listen` collects events from the provider stream up to `limit` (default 20) and
returns them as an array, because an MCP tool call is one request/response rather
than a long-lived stream. For a continuous stream, use the HTTP server's
`/events` route or the CLI's `listen` command.

## Provider selection

The npm binary builds with every provider. A build with a smaller set comes from
`cargo install --path crates/mcp --features discord,telegram`; the tools then
reach only the providers that build enabled, and an unenabled provider returns
`NOT_REGISTERED`.
