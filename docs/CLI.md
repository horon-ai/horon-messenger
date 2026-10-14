# CLI

`horon-messenger` is the command line surface. It is a thin translation layer
over the [`Client`](SDK.md) in `horon-messenger-sdk`: every command turns argv
into one or more `Client` calls and prints the resulting [`Envelope`](CORE.md).

- Crate: `crates/cli` (`horon-messenger-cli`)
- Binary: `horon-messenger`
- npm: `@horon-ai/messenger-cli`, and `@horon-ai/messenger` (all-in-one)

## Run

```bash
cargo run -p horon-messenger-cli -- providers          # from the workspace
npx @horon-ai/messenger-cli providers                  # from npm, once published
npx @horon-ai/messenger providers                      # all-in-one package
horon-messenger providers                              # installed binary
```

## Output

Every command prints one JSON envelope on stdout and sets the exit code from it.

```json
{"ok": true,  "data": [ ... ], "meta": {"provider": "mock", "generated_at": "..."}}
{"ok": false, "error": {"code": "INVALID_INPUT", "message": "...", "retryable": false}, "meta": { ... }}
```

`--pretty` indents the envelope. The exit code is `0` when `ok` is true and `1`
when it is false; a usage error (unknown flag, missing argument) exits `2` with
clap's message on stderr. `--config-dir <DIR>` (env `HORON_MESSENGER_CONFIG_DIR`)
relocates all credentials and state for the run.

## Commands

### Providers and messages

| Command | Description |
|---------|-------------|
| `providers` | List the providers this build enabled, with each one's capabilities |
| `whoami <provider>` | Show the authenticated account for a provider |
| `chats <provider> [--search <text>] [--limit <n>]` | List chats |
| `messages <provider> <chat> [--search <text>] [--limit <n>]` | List or search messages in one chat |
| `send <provider> <chat> <text> [--reply-to <id>]` | Send a message to a chat |
| `members <provider> <chat>` | List the members of a chat |
| `snapshot <provider>` | Print a provider snapshot (profile + chats) |
| `chat <provider> <chat>` | Show one chat by id |
| `search <provider> <query> [--chat <id>] [--limit <n>]` | Search messages, in one chat or across every chat |
| `mark-read <provider> <chat>` | Mark a chat as read |
| `listen <provider> [--chat <id>] [--limit <n>]` | Stream events until interrupted (or `--limit` is reached) |

```bash
horon-messenger providers --pretty
horon-messenger chats mock --search standup
horon-messenger messages mock general --limit 20
horon-messenger send mock general "hello" --reply-to 42
horon-messenger chat mock general
horon-messenger search mock "standup" --chat team
horon-messenger mark-read mock general
horon-messenger listen mock --chat general --limit 5
```

`listen` prints one event per line (newline-delimited JSON) rather than a single
envelope, so a stream can be piped. Without `--limit` it runs until interrupted
(Ctrl-C); with `--limit <n>` it stops after `n` events and exits with the summary
envelope. An operation a provider does not advertise reports `Unsupported`; one it
advertises but has not built reports `NotImplemented`.

### Authentication

`auth` groups the sign-in flow. A provider that does not expose sign-in reports
`Unsupported` for these commands.

| Command | Description |
|---------|-------------|
| `auth status <provider>` | Show stored accounts and the current one |
| `auth login <provider> [--account <id>] [--param key=value]...` | Sign in |
| `auth logout <provider> [--account <id>]` | Sign out |

`--param` is repeatable and carries provider-specific credential fields
(`--param token=...`, `--param cookie=...`). A value without `=` is a usage
error. Providers also read the same fields from environment variables; see
[providers.md](providers.md).

```bash
horon-messenger auth login mock --param token=abc
horon-messenger auth status mock
```

### Daemon (HTTP API server)

`daemon` runs the separate `horon-messenger-server` binary as a background
process and manages it. It looks for that binary next to the CLI, on `PATH`, or
at `HORON_MESSENGER_SERVER`. State is a `server.pid` file and a `server.log`
under the config dir; liveness is a TCP probe against the bind address.

| Command | Description |
|---------|-------------|
| `daemon status` | Report whether the server is running and on which url |
| `daemon start [--host <ip>] [--port <n>]` | Start the server in the background (defaults `127.0.0.1:8787`) |
| `daemon stop` | Stop the background server (sends a termination signal) |
| `daemon restart [--host <ip>] [--port <n>]` | Stop then start |

```bash
horon-messenger daemon start --port 8787
horon-messenger daemon status      # {"running": true, "url": "http://127.0.0.1:8787", "pid": 1234, ...}
horon-messenger daemon restart
horon-messenger daemon stop
```

`start` waits up to 10 seconds for the port to accept connections and fails with
the log path if the server exits first. `start` on an already-running server is
an `INVALID_INPUT` error naming the current pid.

### Setup

`setup` prepares the local configuration and reports provider readiness.

| Command | Description |
|---------|-------------|
| `setup` | Create the config dir and print provider readiness |
| `setup --check` | Report without creating anything |

```bash
horon-messenger setup
```

```json
{"ok": true, "data": {
  "version": "0.1.0",
  "config_dir": "/Users/you/.config/horon-messenger",
  "config_dir_created": true,
  "providers": [{"kind": "mock", "auth_supported": true, "authenticated": true, "accounts": 1}],
  "next_steps": ["sign in to a provider: horon-messenger auth login <provider>", "start the API server: horon-messenger daemon start"]
}}
```

### Update

`update` reads the npm registry for the installed package and installs the latest
with the package manager that ran the CLI (npm, bun, pnpm or yarn).

| Command | Description |
|---------|-------------|
| `update --check` | Report whether a newer release is published |
| `update` | Install the latest release |

```bash
horon-messenger update --check
# {"current": "0.1.0", "latest": "0.2.0", "update_available": true}
horon-messenger update
```

The npm launcher sets `HORON_MESSENGER_PACKAGE` (the package to update) and
`HORON_MESSENGER_INSTALL_MANAGER`. A build that was not installed from npm
(e.g. `cargo install`) does not overwrite itself: `update` reports the available
version and a hint instead. `HORON_MESSENGER_INSTALL_MANAGER` overrides the
detected manager.

## Status

Implemented: providers, whoami, chats, messages, send, members, snapshot, chat,
search, mark-read, listen, auth (status/login/logout), daemon
(status/start/stop/restart), setup, update.

Notes:

- `messages` accepts `--search` and `--limit`; the `before`/`after` cursor fields
  the model supports are not exposed as flags yet.
- The published npm binaries build with every provider; a build with a smaller
  provider set comes from `cargo install --path crates/cli --features ...`.

## Tests

`crates/cli/tests/cli.rs` runs the binary and asserts on the envelope: providers,
seeded chats and messages, send acceptance, an unknown provider as
`INVALID_INPUT`, an unauthenticated platform provider as `AUTH_REQUIRED`, the
mock login flow, and a `--param` usage error.
