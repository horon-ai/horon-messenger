# Architecture

## Layers

```
surfaces   cli ─┐
           mcp ─┤
         server ─┼──► sdk ──► core ◄── providers (one crate per platform)
            ui ─┘
```

The providers are mock, kakaotalk, discord, channeltalk, telegrambot,
whatsappbot, slackbot, slack, imessage, telegram, whatsapp, line and instagram.

- **core** defines the contracts and owns no platform logic: the `Provider` trait,
  the domain model, `Capabilities`, config and credential paths, and the `Envelope`
  wire shape.
- **providers** implement `Provider` for one platform each.
- **sdk** assembles a `ProviderRegistry` from the providers a build enabled and
  exposes `Client`, the one entry point every surface uses.
- **surfaces** translate their input (argv, MCP tool call, HTTP request, keypress)
  into `Client` calls and render the resulting `Envelope`.

Because every surface returns the same `Envelope`, a new surface is a translation
layer, not a second implementation.

## The provider contract

`Provider` is the operation set. Reading and sending are separate methods, and
each has a default that reports why it is unavailable:

```rust
async fn list_chats(&self, query: &ChatQuery) -> Result<Vec<Chat>> {
    Err(self.gate(Capabilities::LIST_CHATS))
}
```

`gate` returns `NotImplemented` when the provider advertises the capability in
`capabilities()` and `Unsupported` when it does not. That one rule lets a skeleton
provider (only `kind` and `capabilities`) report the truth for every operation
without a single stub body, and lets a caller branch on "not built yet" versus
"never supported".

A provider that also supports sign-in implements `AuthProvider` and returns
`Some(self)` from `Provider::auth()`. Auth is separate so a provider can read and
send while delegating sign-in to a CLI-only flow.

## Capabilities

`Capabilities` is a bit set. `whoami`, `list_chats`, `get_chat`, `list_messages`,
`search_messages`, `send_message`, `send_media`, `mark_read`, `list_members`,
`snapshot`, `listen`. Surfaces read it to describe what a provider can do before
anything is attempted (`horon-messenger providers`).

## Wire envelope

```json
{"ok": true,  "data": { }, "meta": {"provider": "mock", "generated_at": "..."}}
{"ok": false, "error": {"code": "NOT_IMPLEMENTED", "message": "...", "retryable": false, "provider": "kakaotalk"}, "meta": { }}
```

`ErrorCode` is a stable enum (`UNSUPPORTED`, `NOT_IMPLEMENTED`, `NOT_REGISTERED`,
`AUTH_REQUIRED`, `AUTH_FAILED`, `NOT_FOUND`, `AMBIGUOUS`, `RATE_LIMITED`,
`NETWORK`, `PROTOCOL`, `INVALID_INPUT`, `IO`, `CONFIG`, `INTERNAL`). `retryable`
is true only for the transient classes.

## Config and credentials

```
<config>/<provider>/accounts/<account>.json   # one file per account, 0600
<config>/<provider>/<name>.json               # provider state (sync cursors, caches)
```

`<config>` is `HORON_MESSENGER_CONFIG_DIR`, else the platform config dir plus
`horon-messenger`. Directories are created `0700`; files are written atomically
through `write_private_atomic`, so a reader never sees a partial file. Providers
choose their own on-disk schema; core owns only the location and the safe write.

## Timestamps and IDs

Every timestamp that crosses a surface boundary is produced by
`core::now_rfc3339` (RFC 3339, milliseconds, UTC), so surfaces cannot drift.
Identifiers that cross a boundary are strings, so a platform's 64-bit or
snowflake id survives JSON without precision loss.

## Performance model

The port exists because the JavaScript original paid a per-call cost behind a
long-running process. Three choices remove that cost, and one harness measures
whether they hold.

**One HTTP client per provider.** Every HTTP provider builds its client once, in
`new()`, from `HttpClient::shared()` in `horon-messenger-provider-common`. That
constructor returns a clone of one process-global `reqwest::Client`, so all
providers and every operation share one connection pool and one rate-limit
bucket map. A repeated operation reuses an open connection instead of paying a
fresh TLS handshake, and the clone itself is cheap because `reqwest::Client` is
reference counted internally.

**One sidecar process per sidecar provider.** iMessage, WhatsApp and LINE hold a
`SidecarSession`. The child is started lazily on the first call and kept alive
for later calls, and clones of the session share the same child. A call whose
request was already written is never retried unless the method is marked
idempotent, so a retried send cannot duplicate a message; a call that failed
before anything was written may restart the process once and retry. When the
child dies, the next call starts a fresh one, and the child is reaped when the
session drops.

**A current-thread runtime in the CLI.** A CLI invocation runs exactly one
command and awaits one operation, so `main` builds a `new_current_thread` tokio
runtime rather than a multi-threaded worker pool. The MCP server and the HTTP
server are long-lived, so they keep a multi-threaded runtime and reuse the
shared client and the sidecar sessions across requests.

### Measuring it

The harness lives in `scripts/perf/`. Build the release binaries first, then:

```bash
bash scripts/perf/measure.sh --label baseline          # writes target/perf/baseline.json
bash scripts/perf/compare.sh target/perf/before.json target/perf/after.json
```

`measure.sh` records CLI start-up plus operation latency for `providers`,
`chats mock` and `send mock general hi`, the bare process-spawn floor
(`/usr/bin/true`), operation latency through a locally started HTTP server, and
the byte size of the three release binaries. It writes the numbers as JSON to
`target/perf/<label>.json` and prints a table. `compare.sh` prints a side-by-side
delta of two runs. Neither script needs the network, a live platform or a fixed
port.
