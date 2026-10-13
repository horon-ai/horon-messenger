# platform-expansion - Work Plan

## TL;DR (For humans)

**Who this is for and what changes for them:** You (and any AI agent driving the tools) will be able to reach nine more messengers — iMessage, Instagram, LINE, Slack (as you and as a bot), Telegram (as you and as a bot), WhatsApp (as you and as a bot) — through the same one command and the same output shape you already use for Discord and Channel Talk, instead of a different tool per app.

**What you'll get:** Nine working platform connectors added to the project, each able to do the operations its platform actually supports (list chats, read messages, send a message), each set up headlessly with a credential you provide, plus the shared plumbing that keeps them consistent and the documentation to run them.

**Why this approach:** The project already has the right shape — one connector per platform behind one shared contract — so this is mostly "add connectors". The load-bearing decisions are: build the pure-HTTP connectors first and the ones needing a helper program last; use a maintained Rust library where one exists (Telegram via `tdlib-rs`, Slack via `slack-morphism`) and one clearly-marked bridge (a helper process or a native library) only where none does; and never auto-retry a send whose outcome is unknown.

**What it will NOT do:** No live realtime message streaming yet (read and send only). No reading credentials out of your desktop apps or browsers — you provide them. No emoji, sticker, reaction or thread management.

**Effort:** XL
**Risk:** High - several connectors depend on unofficial APIs or native/JS helper programs (LINE, Instagram, TDLib, WhatsApp).
**Decisions to sanity-check:** (1) each platform is its own named connector (Slack vs SlackBot, WhatsApp vs WhatsAppBot stay separate); (2) a provider that cannot list chats returns "unsupported" for that operation rather than a fake empty list (WhatsApp Bot is the case); (3) realtime listening is out of scope this round; (4) iMessage is the one platform that must run on a Mac with Full Disk Access.

Your next move: run `/ulw-execute platform-expansion` in this session or a new one. A high-accuracy plan review and a codex review have been run and folded in; execution detail follows below.

---

> TL;DR (machine): XL effort, High risk; 14 implementation todos across 4 waves + F1-F4; adds 9 provider crates + core kinds + provider-common + a sidecar boundary + a generic CLI auth-param passthrough, wired into the sdk and proven by committed mock-double QA per provider.

## Scope
### Affected user and ideal state
**Affected user:** the operator (you) driving messengers through one CLI / MCP server / HTTP server; secondarily an AI agent consuming the MCP tools and the SDK `Client`; tertiarily a maintainer adding platform #10.

| Row | Statement | Reason |
| --- | --- | --- |
| IS-1 | Every listed platform is selectable by name and returns the same `Envelope` shape for the operations it advertises: a provider that can list chats/messages returns them uniformly, and a provider whose platform cannot (e.g. WhatsApp Bot) returns `UNSUPPORTED` for that operation rather than a fake empty list. | One mental model and one parseable output; the advertised capability set is the truth, not a per-platform surprise. |
| IS-2 | Each platform's credentials are provisioned without GUI/browser extraction — via an env var or `auth login --param k=v` with explicit credentials, stored owner-only under `<config>/<provider>/`. iMessage is the one documented exception: macOS-only, no credentials, relies on Full Disk Access and reports a clear error when it is missing. | Reproducible, headless, testable; the one platform that cannot be headless is named and handled explicitly. |
| IS-3 | Platforms with no viable Rust library still work through one declared, isolated boundary (HTTP, a subprocess sidecar, or FFI), so the core and every other provider stay pure Rust and portable. | Keeps the workspace a Rust project without pretending a library exists. |
| IS-4 | Adding a platform touches only its provider crate plus one registration line and one feature flag — never a surface crate beyond the generic plumbing added once here. | The architecture's core promise; a 10th platform must stay cheap. |
| IS-5 | The whole workspace builds/tests/clippy/fmt clean; any advertised-but-unimplemented operation returns `NOT_IMPLEMENTED`, never panics. | The project must never half-work silently. |
| IS-6 | HTTP, retry/backoff and rate-limit behavior is defined once and reused, and a send whose outcome is unknown is never auto-retried. | Nine providers duplicating it will drift, and a duplicated send is worse than a failed one. |

| Row | Statement | Reason |
| --- | --- | --- |
| GAP-1 | Only mock/discord/channeltalk implement operations; kakaotalk/telegram are skeletons; the other eight platforms are absent. | The operator cannot use them at all. |
| GAP-2 | No auth flow exists beyond a token/cookie field; the CLI `auth login` cannot pass credentials (it sends only an account name), and no `auth login` populates credentials for the new platforms. | IS-2: setup must be headless. |
| GAP-3 | No sidecar/subprocess/FFI boundary exists in the codebase. | IS-3: some platforms need one. |
| GAP-4 | `ProviderKind` lacks the new variants; sdk features and workspace deps are not wired. | IS-1/IS-4. |
| GAP-5 | Retry/rate-limit logic is duplicated in discord and channeltalk clients; no send-retry policy exists. | IS-6. |

### Must have
- Nine provider crates, each implementing `Provider` (whoami, list_chats, get_chat, list_messages, search_messages where the platform supports it, send_message, list_members, snapshot) and `AuthProvider` (status/login/logout), advertising only the operations the platform actually supports.
- New `ProviderKind` variants and their `as_str`/`display_name`/`description`/`from_str` mappings.
- A `hyper-messenger-provider-common` crate: HTTP client builder, retry/backoff, rate-limit gate, URL/query helpers, a **send-retry policy** (idempotent methods retryable; send is never auto-retried on an unknown outcome), and a subprocess JSON-RPC client.
- One declared sidecar/FFI boundary, reused by imessage (subprocess), whatsapp (Node sidecar) and telegram (`tdlib-rs`, which bundles libtdjson).
- A generic CLI `auth login --param key=value` (repeatable) that fills `LoginOptions.params`, so any provider can be authenticated headlessly without a per-provider CLI change.
- sdk registration + features + workspace members/deps for all nine.
- Per-provider unit tests and one committed, re-runnable mock-double QA script + transcript per provider.
- Docs: provider/auth table and sidecar prerequisites.

### Must NOT have (guardrails, anti-slop, scope boundaries)
- No realtime listeners / websocket event streams (Slack socket mode, Telegram getUpdates loop, WhatsApp socket, Instagram MQTT, LINE long-poll).
- No desktop-app or browser credential extraction (Chromium cookie decryption).
- No emoji/sticker/reaction/thread/pin/bookmark management.
- No live-account integration tests; mock doubles only.
- No new dependencies inside `crates/core` (it stays `serde`/`thiserror`/`chrono`/`dirs` only).
- No per-provider edits to surface crates (cli/mcp/server/ui); the ONE allowed surface change is the generic `auth login --param` passthrough (todo 14), which is provider-agnostic.
- No auto-retry of a send whose outcome is unknown (see IS-6).

## Verification strategy
> Zero human intervention - all verification is agent-executed.
- Test decision: tests-after per provider (Rust `#[test]`/`#[tokio::test]` unit tests for parsing, mapping, error mapping, auth precedence, send-retry policy) + one real-surface QA per provider.
- CLI QA harness: every CLI QA command first runs `cargo build --workspace --all-features` (which turns on the sdk `all` provider features) and then invokes the built binary `target/debug/hyper-messenger <args>`. Do NOT use `cargo run -p hyper-messenger-cli --all-features`, which enables only the CLI crate's own features and leaves the sdk on its default `mock` (so `providers` would list only mock).
- Environment matrix: CI is Ubuntu-only; the iMessage success path is macOS-only and is proven by the checked-in stub plus the crate's unit tests, not by CI. Record this limitation; do not gate on a macOS runner.
- **QA is a committed, re-runnable script per provider** at `crates/providers/<name>/tests/qa/`: a mock server (Python `http.server`, fixed port, seeded fixtures) or a stub sidecar, plus `run.sh` that (1) starts the mock/stub, (2) runs the exact CLI commands with the base-URL/binary env vars, (3) asserts the expected envelope fields with `python3 -c`/`jq`, (4) tears the mock down and prints a `cleanup:` line. The transcript is the evidence. Ports are fixed per provider: telegrambot 8811, whatsappbot 8812, slackbot 8813, slack 8814, imessage 8815, telegram 8816, whatsapp 8817, line 8818, instagram 8819.
- Evidence: `<attemptDir>/task-<N>-platform-expansion.txt` (attemptDir = `result.currentAttemptDir` from `await agentToolkit.status()` in a JS eval cell after importing `agentToolkit` with `await import(\`${env("OMO_AGENT_TOOLKIT_SDK_ROOT")}/sdk.js\`)`, `.omo/evidence/ulw/<session>/<goalId>/a<attempt>`; outside ulw-loop use `.omo/evidence/`).
- Per-todo acceptance is the exact cargo command that must exit 0; per-todo QA is the exact `run.sh` invocation + the asserted envelope field, captured to the evidence path.

## Execution strategy
### Parallel execution waves
- **Wave 0 - foundation (blocks all):** 1, 2, 3 (3 after 2), 14.
- **Wave 1 - pure-HTTP providers (independent):** 4, 5, 6, 7.
- **Wave 2 - subprocess/FFI user-account providers:** 8, 9, 10.
- **Wave 3 - hard protocols + docs:** 11, 12, 13.
- **Final verification wave:** F1-F4 after all todos.

### Dependency matrix
| Todo | Depends on | Blocks | Can parallelize with |
| --- | --- | --- | --- |
| 1 | - | 2,14,4..13 | - |
| 2 | 1 | 3,4,5,6,7,8,10,11,12 | 14 |
| 3 | 1,2 | 8,9,10 | 14 |
| 14 | 1 | 4,5,6,7,8,9,10,11,12 | 2,3 |
| 4 | 1,2,14 | - | 5,6,7 |
| 5 | 1,2,14 | - | 4,6,7 |
| 6 | 1,2,14 | - | 4,5,7 |
| 7 | 1,2,14 | - | 4,5,6 |
| 8 | 1,2,3,14 | - | 9,10 |
| 9 | 1,3,14 | - | 8,10 |
| 10 | 1,2,3,14 | - | 8,9 |
| 11 | 1,2,3,14 | - | 12,13 |
| 12 | 1,2,14 | - | 11,13 |
| 13 | 4..12,14 | - | - |

## Todos
> Implementation + Test = ONE todo. Never separate.
<!-- APPEND TASK BATCHES BELOW THIS LINE WITH edit/apply_patch - never rewrite the headers above. -->
- [x] 1. Add the eight new ProviderKind variants
  What to do / Must NOT do: In `crates/core/src/kind.rs`, add variants `Slack`, `SlackBot`, `TelegramBot`, `WhatsApp`, `WhatsAppBot`, `Line`, `IMessage`, `Instagram` to `ProviderKind` (keep the existing `KakaoTalk, Discord, Telegram, ChannelTalk, Mock`). Extend `ALL` (now 13), `as_str` (`slack`, `slackbot`, `telegrambot`, `whatsapp`, `whatsappbot`, `line`, `imessage`, `instagram`), `display_name`, `description`, and the `FromStr` match (accept aliases: `slack`, `slackbot`/`slack-bot`, `telegrambot`/`telegram-bot`, `whatsapp`, `whatsappbot`/`whatsapp-bot`, `line`, `imessage`/`i-message`, `instagram`/`ig`). Must NOT add dependencies to core; must NOT change existing variants' string forms.
  Closes: GAP-4
  Parallelization: Wave 0 | Blocked by: none | Blocks: 2,14,4..13
  References: `crates/core/src/kind.rs:1-90` (existing enum, `ALL`, `as_str`, `FromStr`).
  Acceptance criteria (agent-executable): `cargo test -p hyper-messenger-core` passes including a test asserting round-trip `k.as_str().parse::<ProviderKind>() == k` for all 13 `ProviderKind::ALL` and that each alias parses; `cargo build --workspace --all-features` exits 0.
  QA scenarios (name the exact tool + invocation): happy — `cargo test -p hyper-messenger-core` passes the enum round-trip/alias test, and after `cargo build --workspace --all-features` the binary `target/debug/hyper-messenger chats bogus` exits 1 with `error.code == "INVALID_INPUT"`; the 13-provider registry listing is NOT checked here (the CLI prints SDK-registered providers, which do not exist yet) — it is validated in F3 after every provider registers. Evidence `<attemptDir>/task-1-platform-expansion.txt`
  Commit: Y | feat(core): add provider kinds for the expanded platform set
  Recommended task executor category: quick - single-file enum + tests.

- [x] 2. Add the provider-common crate (HTTP/retry/rate-limit + send-retry policy) and adopt it in discord + channeltalk
  What to do / Must NOT do: Create `crates/providers/common` (`hyper-messenger-provider-common`), added to workspace `members` and `[workspace.dependencies]`. Provide: a `HttpClient` wrapper over `reqwest::Client` with (a) retry with exponential backoff for 5xx/transport on **idempotent methods only**, (b) 429 `Retry-After` handling, (c) a rate-limit gate keyed by a caller-supplied bucket key, (d) `urlencode`/query helpers, (e) `map_http_status(provider, status, message) -> Error` (401/403→AuthFailed, 404→NotFound, 429→RateLimited, 5xx→Network, else Protocol), and (f) an explicit **send-retry policy**: a `send`/non-idempotent call is NEVER auto-retried after the request is transmitted — on a transport failure it returns a typed error whose message says the outcome is unknown. Refactor `crates/providers/discord/src/client.rs` and `crates/providers/channeltalk/src/client.rs` onto it, deleting their duplicated retry/rate-limit code, WITHOUT changing observable behaviour. Must NOT auto-retry sends; must NOT change existing error codes/paths.
  Closes: GAP-5, IS-6
  Parallelization: Wave 0 | Blocked by: 1 | Blocks: 3,4,5,6,7,8,10,11,12
  References: `crates/providers/discord/src/client.rs:38-190`, `crates/providers/channeltalk/src/client.rs:80-230`, root `Cargo.toml:9-20` (members), `Cargo.toml:29-40` (workspace deps); anti-pattern source `~/Desktop/dev/alpox/kakaotalk-bridge/AGENTS.md` ("never auto-resubmit an unknown send outcome").
  Acceptance criteria (agent-executable): `cargo build --workspace --all-features` exits 0; `cargo clippy --workspace --all-features --all-targets -- -D warnings` clean; `cargo test --workspace --all-features` passes; a unit test proves a non-idempotent call is NOT retried on transport failure while a GET is.
  QA scenarios (name the exact tool + invocation): this todo also commits `crates/providers/discord/tests/qa/run.sh` and `crates/providers/channeltalk/tests/qa/run.sh` (mock servers + assertions). happy — after `cargo build --workspace --all-features`, `bash crates/providers/discord/tests/qa/run.sh` prints `ok:true` with the same seeded envelope as before; failure — a mock that accepts a send then closes the connection yields a typed "unknown outcome" error and is NOT retried (the mock asserts exactly one request arrived). Evidence `<attemptDir>/task-2-platform-expansion.txt`
  Commit: Y | refactor(providers): add provider-common with a send-retry policy and reuse it in discord and channeltalk
  Recommended task executor category: unspecified-high - cross-crate shared contract with a behaviour-preserving refactor.

- [x] 3. Add the subprocess JSON-RPC boundary to provider-common
  What to do / Must NOT do: In `hyper-messenger-provider-common`, add a `sidecar` module: a `JsonRpcProcess` that spawns a child (`tokio::process::Command`), writes line-delimited JSON requests to stdin, reads line-delimited JSON responses from stdout, correlates by `id`, supports notifications, applies a per-request timeout, and a `SidecarSpec` (binary path from env, default name, args). Returns a typed `Error` (Protocol/Network) when the binary is missing, never panics. Add a test spawning a tiny stub (a checked-in `python3` one-liner or a test helper) asserting request/response + timeout. Must NOT hardcode any platform; must NOT add platform deps.
  Closes: GAP-3
  Parallelization: Wave 0 | Blocked by: 1,2 | Blocks: 8,9,10
  References: `crates/core/src/error.rs`; reference RPC shape `/tmp/agent-messenger-ref/src/platforms/imessage/rpc.ts:1-40`.
  Acceptance criteria (agent-executable): `cargo test -p hyper-messenger-provider-common` passes, including (a) a request/response round-trip and (b) a timeout on a non-responding stub; `cargo clippy -p hyper-messenger-provider-common --all-targets -- -D warnings` clean.
  QA scenarios (name the exact tool + invocation): happy — the unit test's stub round-trips a request; failure — a stub that never replies produces a timeout `Error`, asserted. Evidence `<attemptDir>/task-3-platform-expansion.txt`
  Commit: Y | feat(providers-common): add a subprocess JSON-RPC sidecar boundary
  Recommended task executor category: deep-low - subprocess protocol + async I/O.

- [x] 4. Implement the Telegram Bot provider (telegrambot)
  What to do / Must NOT do: New crate `crates/providers/telegrambot` using `provider-common` + `reqwest`. Base URL `https://api.telegram.org` (env override `HYPER_MESSENGER_TELEGRAMBOT_API_BASE`). Bot-token auth via env `HYPER_MESSENGER_TELEGRAMBOT_TOKEN` or config account `default` (`token`). Map: `whoami`→`getMe`; `list_chats`→ distinct chats from `getUpdates`; `get_chat`→`getChat`; `list_messages`→ best-effort from updates (Bot API has no history — document the limitation in the code doc); `send_message`→`sendMessage`; `list_members`→`getChatAdministrators`; `snapshot`→getMe+chats. `AuthProvider` (token). Register behind feature `telegrambot`; add to `all`. Must NOT implement a getUpdates polling loop.
  Closes: GAP-1, GAP-2
  Parallelization: Wave 1 | Blocked by: 1,2,14 | Blocks: none
  References: `/tmp/agent-messenger-ref/src/platforms/telegrambot/client.ts:9-11`, `.../telegrambot/types.ts`, `.../telegrambot/commands/message.ts`; repo pattern `crates/providers/discord/src/{client.rs,lib.rs}`; registration `crates/sdk/src/lib.rs:133-161`.
  Acceptance criteria (agent-executable): `cargo build -p hyper-messenger-provider-telegrambot --all-features` exits 0; clippy clean; unit tests for error mapping + auth precedence pass.
  QA scenarios (name the exact tool + invocation): happy — `crates/providers/telegrambot/tests/qa/run.sh` starts a Python mock on 127.0.0.1:8811 and runs `HYPER_MESSENGER_TELEGRAMBOT_API_BASE=http://127.0.0.1:8811 HYPER_MESSENGER_TELEGRAMBOT_TOKEN=mock hyper-messenger whoami telegrambot` and `send telegrambot <chat> hi`, asserting `ok:true` and seeded fields; failure — wrong token → mock 401 → assert `error.code == "AUTH_FAILED"`. Evidence `<attemptDir>/task-4-platform-expansion.txt`
  Commit: Y | feat(telegrambot): implement the Telegram Bot provider
  Recommended task executor category: unspecified-high - standard multi-file provider feature.

- [x] 5. Implement the WhatsApp Bot provider (whatsappbot)
  What to do / Must NOT do: New crate `crates/providers/whatsappbot` using `provider-common`. Base URL `https://graph.facebook.com/v23.0` (env override `HYPER_MESSENGER_WHATSAPPBOT_API_BASE`). Auth: `phone_number_id` + `access_token` via env or config account `default`. Map: `whoami`→ phone number id; `send_message`→`POST /{phone_number_id}/messages` with a text body. Advertise ONLY `WHOAMI` + `SEND_MESSAGE` (the Cloud API has no chat listing/history); do NOT advertise LIST_CHATS/LIST_MESSAGES/LIST_MEMBERS/GET_CHAT/SEARCH_MESSAGES/SNAPSHOT. Register behind feature `whatsappbot`; add to `all`.
  Closes: GAP-1, GAP-2
  Parallelization: Wave 1 | Blocked by: 1,2,14 | Blocks: none
  References: `/tmp/agent-messenger-ref/src/platforms/whatsappbot/client.ts:4-40`, `.../whatsappbot/types.ts`; capability pattern `crates/providers/channeltalk/src/lib.rs:129-138`.
  Acceptance criteria (agent-executable): `cargo build -p hyper-messenger-provider-whatsappbot --all-features` exits 0; clippy clean; unit tests for auth precedence + error mapping; a test asserts `capabilities()` == WHOAMI|SEND_MESSAGE.
  QA scenarios (name the exact tool + invocation): happy — `crates/providers/whatsappbot/tests/qa/run.sh` (mock Graph API on 8812) runs `send whatsappbot <phone> hi` → `ok:true` with a message id, and `whoami whatsappbot` → the phone number id; failure — bad token → 401 → `AUTH_FAILED`, AND `chats whatsappbot` → `error.code == "UNSUPPORTED"` (because the capability is not advertised). Evidence `<attemptDir>/task-5-platform-expansion.txt`
  Commit: Y | feat(whatsappbot): implement the WhatsApp Cloud API provider
  Recommended task executor category: unspecified-high - standard provider feature.

- [x] 6. Implement the Slack Bot provider (slackbot) using slack-morphism
  What to do / Must NOT do: New crate `crates/providers/slackbot` using `slack-morphism` (v2.x) for Web API calls. Base URL override `HYPER_MESSENGER_SLACKBOT_API_BASE` (slack-morphism supports a custom endpoint; if not, fall back to a thin reqwest client over `https://slack.com/api` — state which you used in the crate doc). Auth: bot token `xoxb-...` via env `HYPER_MESSENGER_SLACKBOT_TOKEN` or config `default` (validate prefix, else `Error::InvalidInput`). Map: `whoami`→`auth.test`; `list_chats`→`conversations.list`; `get_chat`→`conversations.info`; `list_messages`→`conversations.history`; `send_message`→`chat.postMessage`; `list_members`→`conversations.members`+`users.info`; `snapshot`. Do NOT advertise `SEARCH_MESSAGES` (bot tokens cannot search). Slack returns HTTP 200 with `{ok:false,error}` — map `invalid_auth`→AuthFailed. Register behind feature `slackbot`; add to `all`. Must NOT implement Socket Mode.
  Closes: GAP-1, GAP-2
  Parallelization: Wave 1 | Blocked by: 1,2,14 | Blocks: none
  References: `/tmp/agent-messenger-ref/src/platforms/slackbot/client.ts:1-40`, `.../slackbot/message-mapper.ts`; library `slack-morphism` (crates.io v2.31.0).
  Acceptance criteria (agent-executable): `cargo build -p hyper-messenger-provider-slackbot --all-features` exits 0; clippy clean; unit tests cover the `ok:false`→Error mapping and token-prefix validation.
  QA scenarios (name the exact tool + invocation): happy — `crates/providers/slackbot/tests/qa/run.sh` (mock Slack on 8813) runs `whoami`/`chats`/`send slackbot <chan> hi` → `ok:true`; failure — mock returns `{"ok":false,"error":"invalid_auth"}` at HTTP 200 → `error.code == "AUTH_FAILED"`. Evidence `<attemptDir>/task-6-platform-expansion.txt`
  Commit: Y | feat(slackbot): implement the Slack Bot provider
  Recommended task executor category: unspecified-high - standard provider feature with one non-obvious mapping.

- [x] 7. Implement the Slack user provider (slack)
  What to do / Must NOT do: New crate `crates/providers/slack` using the same client approach as todo 6. Auth: user token `xoxc-...` + cookie `d=...` (env `HYPER_MESSENGER_SLACK_TOKEN` + `HYPER_MESSENGER_SLACK_COOKIE`, or config `default`); send the cookie header alongside the token. Map as slackbot plus `search_messages`→`search.messages` (advertise `SEARCH_MESSAGES`). Same `ok:false` handling. Register behind feature `slack`; add to `all`. Must NOT implement RTM/QR login.
  Closes: GAP-1, GAP-2
  Parallelization: Wave 1 | Blocked by: 1,2,14 | Blocks: none
  References: `/tmp/agent-messenger-ref/src/platforms/slack/client.ts`, `.../slack/client-mappers.ts`, `.../slack/types.ts`; the cookie header is the key edge.
  Acceptance criteria (agent-executable): `cargo build -p hyper-messenger-provider-slack --all-features` exits 0; clippy clean; unit tests for cookie+token header assembly and `ok:false` mapping.
  QA scenarios (name the exact tool + invocation): happy — `crates/providers/slack/tests/qa/run.sh` (mock Slack on 8814) runs `whoami slack`, `chats slack`, `messages slack <chan>`, `send slack <chan> hi` → `ok:true`; failure — missing cookie → mock 401/`invalid_auth` → `AUTH_FAILED`. Evidence `<attemptDir>/task-7-platform-expansion.txt`
  Commit: Y | feat(slack): implement the Slack user provider
  Recommended task executor category: unspecified-high - standard provider feature.

- [x] 8. Implement the iMessage provider (imessage)
  What to do / Must NOT do: New crate `crates/providers/imessage` using `provider-common`'s sidecar client to spawn `imsg rpc` (binary env `HYPER_MESSENGER_IMSG_BIN`, default `imsg`). Map the reference JSON-RPC methods to the Provider ops. **macOS-only**: on non-macOS, operations return `Error::Unsupported`; on macOS without Full Disk Access the `imsg` process fails and the provider surfaces a typed error naming the missing permission (do NOT panic, do NOT silently return empty). No credentials; `AuthProvider::status` reports "no login needed". Register behind feature `imessage`; add to `all`. Must NOT reimplement the imsg protocol; must NOT require the binary at build time.
  Closes: GAP-1, GAP-2, GAP-3
  Parallelization: Wave 2 | Blocked by: 1,2,3,14 | Blocks: none
  References: `/tmp/agent-messenger-ref/src/platforms/imessage/{client.ts,rpc.ts,types.ts}`; `crates/providers/common` sidecar module (todo 3).
  Acceptance criteria (agent-executable): `cargo build -p hyper-messenger-provider-imessage --all-features` exits 0; clippy clean; unit tests map reference RPC payloads → `Chat`/`Message` (fixtures), and cover the missing-binary AND the permission-denied error paths (both typed, no panic).
  QA scenarios (name the exact tool + invocation): happy — `crates/providers/imessage/tests/qa/run.sh` uses a checked-in stub `imsg` script (port-n/a) via `HYPER_MESSENGER_IMSG_BIN`, runs `chats imessage`/`messages imessage <id>` → `ok:true`; failure — unset/invalid binary → typed error, non-zero exit; on a non-macOS host → `error.code == "UNSUPPORTED"`. Evidence `<attemptDir>/task-8-platform-expansion.txt`
  Commit: Y | feat(imessage): implement the iMessage provider over the imsg sidecar
  Recommended task executor category: unspecified-high - provider + subprocess integration with fixtures.

- [x] 9. Implement the Telegram user provider (telegram) using tdlib-rs
  What to do / Must NOT do: Replace the `crates/providers/telegram` skeleton with a real provider backed by **`tdlib-rs`** (v1.x; it bundles libtdjson via its `download-tdlib` feature) rather than a hand-rolled FFI loop. Map TDLib chats/messages to `Chat`/`Message`; auth via the TDLib authorization state (phone/code/password through `LoginOptions.params`). If the TDLib library cannot be loaded, return a clear typed `Error` (do NOT panic). Keep `ProviderKind::Telegram`. Register behind feature `telegram` (already in `all`). Must NOT vendor or hand-build TDLib; must NOT block on a real network at test time; if `tdlib-rs` proves unbuildable in this environment, fall back to `grammers-client` (pure Rust) and state the choice.
  Closes: GAP-1, GAP-2, GAP-3
  Parallelization: Wave 2 | Blocked by: 1,3,14 | Blocks: none
  References: `/tmp/agent-messenger-ref/src/platforms/telegram/{tdlib.ts,client.ts,types.ts}`; library `tdlib-rs` (crates.io v1.4.0) / `grammers-client` (v0.10.0).
  Acceptance criteria (agent-executable): `cargo build -p hyper-messenger-provider-telegram --all-features` exits 0; clippy clean; unit tests for the TDLib JSON→model mapping (fixtures) and the missing-library error path.
  QA scenarios (name the exact tool + invocation): decision — the QA uses a `#[cfg(feature = "qa-fake")]` in-crate fake that replays fixture TDLib JSON through the same mapping (no native library needed); the real `tdlib-rs` load path is covered by the missing-library test. happy — `bash crates/providers/telegram/tests/qa/run.sh` builds with `--features qa-fake` and runs `whoami telegram`/`chats telegram` over fixtures → `ok:true`; failure — with `qa-fake` off and no library present → typed `Error` (library unavailable), non-zero exit, no panic. Evidence `<attemptDir>/task-9-platform-expansion.txt`
  Commit: Y | feat(telegram): implement the Telegram user provider via tdlib-rs
  Recommended task executor category: deep-high - FFI/native-library contract with no in-repo precedent.

- [x] 10. Implement the WhatsApp user provider (whatsapp) via a Node/baileys sidecar
  What to do / Must NOT do: New crate `crates/providers/whatsapp`. Add a sidecar under `crates/providers/whatsapp/sidecar/` (a Node script using `@whiskeysockets/baileys`, pinned version in its `package.json` + committed lockfile) speaking the `provider-common` line-delimited JSON-RPC protocol (methods: `chats.list`, `messages.list`, `message.send`, `auth.status`, `auth.login`). The Rust provider spawns it via the sidecar client (node env `HYPER_MESSENGER_NODE_BIN`; sidecar path resolved from the crate via `CARGO_MANIFEST_DIR` at build time, falling back to an env override `HYPER_MESSENGER_WHATSAPP_SIDECAR` for installed layouts). Document the install step (`npm ci` in `sidecar/`) and the pinned baileys version. If node or the sidecar is missing, return a clear typed `Error`. Register behind feature `whatsapp`; add to `all`. Must NOT port the baileys protocol to Rust; must NOT require node at build time. (Note: `whatsapp-rust` exists as a pure-Rust alternative but is young; the sidecar is the stable default.)
  Closes: GAP-1, GAP-2, GAP-3
  Parallelization: Wave 2 | Blocked by: 1,2,3,14 | Blocks: none
  References: `/tmp/agent-messenger-ref/src/platforms/whatsapp/{client.ts,types.ts}`; `crates/providers/common` sidecar module (todo 3).
  Acceptance criteria (agent-executable): `cargo build -p hyper-messenger-provider-whatsapp --all-features` exits 0; clippy clean; `node --check crates/providers/whatsapp/sidecar/index.mjs` exits 0; unit tests map reference message shapes → `Message`; the sidecar path resolution is covered by a test using `HYPER_MESSENGER_WHATSAPP_SIDECAR` pointing outside the source tree. Distribution/packaging (bundling the sidecar into a release artifact) is OUT of scope; `HYPER_MESSENGER_WHATSAPP_SIDECAR` is the installed-layout mechanism.
  QA scenarios (name the exact tool + invocation): happy — `bash crates/providers/whatsapp/tests/qa/run.sh` runs a deterministic stub sidecar (env `HYPER_MESSENGER_WHATSAPP_SIDECAR` → the stub, port 8817) → `chats whatsapp`/`send whatsapp <chat> hi` `ok:true`; additionally, when `node` is present, the script also starts the REAL sidecar (`npm ci` in `sidecar/` then run it) and asserts it answers `auth.status`, so a real subprocess start is exercised, not only a stub; failure — sidecar missing → typed `Error`, non-zero exit. Evidence `<attemptDir>/task-10-platform-expansion.txt`
  Commit: Y | feat(whatsapp): implement the WhatsApp provider over a baileys sidecar
  Recommended task executor category: deep-high - cross-language sidecar contract + protocol boundary.

- [x] 11. Implement the LINE provider (line)
  What to do / Must NOT do: New crate `crates/providers/line` using `provider-common`. **First decide the implementation path and record it in the crate doc:** (a) hand-roll the Thrift surface with the `thrift` crate (v0.25) over the LINE endpoints, if the read/send calls are reachable without E2EE; otherwise (b) use the todo-3 sidecar boundary with a Node sidecar wrapping the vendored linejs (the sidecar boundary is already a dependency of this todo). Then implement the read/send surface. Auth via `auth.login` (token/password in `LoginOptions.params`); store under `<config>/line/`. Register behind feature `line`; add to `all`. Must NOT implement E2EE letter-sealing or the long-poll listener.
  Closes: GAP-1, GAP-2
  Parallelization: Wave 3 | Blocked by: 1,2,14 | Blocks: none
  References: `/tmp/agent-messenger-ref/src/platforms/line/{client.ts,types.ts,credential-manager.ts}`; vendor `/tmp/agent-messenger-ref/src/vendor/linejs/`.
  Acceptance criteria (agent-executable): `cargo build -p hyper-messenger-provider-line --all-features` exits 0; clippy clean; unit tests for the reference payload → `Chat`/`Message` mapping.
  QA scenarios (name the exact tool + invocation): happy — `crates/providers/line/tests/qa/run.sh` (mock/stub on 8818) runs `chats line`/`send line <chat> hi` → `ok:true`; failure — auth failure → `AUTH_FAILED`. Evidence `<attemptDir>/task-11-platform-expansion.txt`
  Commit: Y | feat(line): implement the LINE provider
  Recommended task executor category: deep-high - Thrift/E2EE protocol with no in-repo precedent.

- [x] 12. Implement the Instagram provider (instagram)
  What to do / Must NOT do: New crate `crates/providers/instagram` using `provider-common`. Base URL `https://i.instagram.com/api/v1` (env override). Auth: session cookies/device id via env or config `default`. Map the reference read/send HTTP calls to the Provider ops; include a request timeout (flagged IPs hang silently). Register behind feature `instagram`; add to `all`. Must NOT implement MQTT realtime or browser token extraction.
  Closes: GAP-1, GAP-2
  Parallelization: Wave 3 | Blocked by: 1,2,14 | Blocks: none
  References: `/tmp/agent-messenger-ref/src/platforms/instagram/{client.ts,types.ts,credential-manager.ts}` (base URL, headers, timeouts).
  Acceptance criteria (agent-executable): `cargo build -p hyper-messenger-provider-instagram --all-features` exits 0; clippy clean; unit tests for header assembly + payload → model mapping + timeout mapping.
  QA scenarios (name the exact tool + invocation): happy — `crates/providers/instagram/tests/qa/run.sh` (mock IG on 8819) runs `chats instagram`/`messages instagram <chat>`/`send instagram <chat> hi` → `ok:true`; failure — 401 → `AUTH_FAILED`. Evidence `<attemptDir>/task-12-platform-expansion.txt`
  Commit: Y | feat(instagram): implement the Instagram DM provider
  Recommended task executor category: unspecified-high - HTTP provider with header/timeout edges.

- [x] 13. Document the expanded platform set and sidecar prerequisites
  What to do / Must NOT do: Update `docs/providers.md` (platform table: auth + transport per platform, marking which are sidecar/FFI, with the env vars/prerequisites: node+baileys, tdlib-rs/libtdjson, imsg, macOS+Full Disk Access for iMessage) and the `README.md` provider table/status. Add a "how to add platform #10" recap. Must NOT claim listeners or extraction exist.
  Closes: IS-4 (documentation), GAP-1..GAP-3 (documented)
  Parallelization: Wave 3 | Blocked by: 4..12,14 | Blocks: none
  References: `docs/providers.md`, `README.md`, `AGENTS.md`.
  Acceptance criteria (agent-executable): every provider from todos 4-12 appears in the docs table with auth + transport + prerequisites; `cargo build --workspace --all-features` still exits 0.
  QA scenarios (name the exact tool + invocation): happy — `for p in telegrambot whatsappbot slackbot slack imessage telegram whatsapp line instagram; do grep -q "$p" docs/providers.md || exit 1; done` passes; failure — a missing provider fails the grep assertion. Evidence `<attemptDir>/task-13-platform-expansion.txt`
  Commit: Y | docs(providers): document the expanded platform set and sidecar prerequisites
  Recommended task executor category: writing - docs only.

- [x] 14. Add a generic `auth login --param` passthrough to the CLI
  What to do / Must NOT do: In `crates/cli/src/main.rs`, extend the `auth login` subcommand with a repeatable `--param key=value` that populates `LoginOptions.params` (as a JSON object), merged with the existing `--account`. This is the ONE allowed surface change and it is provider-agnostic. Must NOT add per-provider flags; must NOT change other commands; must keep the existing `--account` behaviour.
  Closes: GAP-2
  Parallelization: Wave 0 | Blocked by: 1 | Blocks: 4,5,6,7,8,9,10,11,12
  References: `crates/cli/src/main.rs` (`AuthCommand::Login`, `LoginOptions`); `crates/core/src/auth.rs:44-50` (`LoginOptions.params`).
  Acceptance criteria (agent-executable): `cargo build -p hyper-messenger-cli --all-features` exits 0; clippy clean; a CLI test asserts `auth login <provider> --param token=abc` reaches the provider with `params.token == "abc"` (via the mock provider or a test double).
  QA scenarios (name the exact tool + invocation): happy — `cargo run -p hyper-messenger-cli --all-features -- auth login mock --param token=abc` exits 0 and `auth status mock` reports authenticated; failure — a malformed `--param foo` (no `=`) exits 2 with a clap usage error. Evidence `<attemptDir>/task-14-platform-expansion.txt`
  Commit: Y | feat(cli): add a generic auth login --param passthrough
  Recommended task executor category: unspecified-low - one command's argument plumbing + a test.

- [x] 15. Fix the cli test's provider-feature assumption (pre-existing failure surfaced in wave 0)
  What to do / Must NOT do: `crates/cli/tests/cli.rs` has `platform_provider_without_credentials_requires_auth`, which expects `AUTH_REQUIRED` from `chats discord` but gets `NOT_REGISTERED` under the default sdk feature set (discord not enabled); it passes only under `--all-features`. Make the test correct under BOTH: either gate the discord expectation on the sdk `discord` feature, or assert the actual per-feature behaviour (default → `NOT_REGISTERED`, `--all-features` → `AUTH_REQUIRED`). This is a real failing test in code touched by todo 14; do not weaken it — assert the true behaviour. Must NOT change production behaviour; test-only change.
  Closes: pre-existing test fragility (wave-0 verification finding, `failure_class: product`)
  Parallelization: Wave 1 | Blocked by: none | Blocks: none
  References: `crates/cli/tests/cli.rs:70-90`; `crates/sdk/Cargo.toml` (`default = ["mock"]`).
  Acceptance criteria (agent-executable): `cargo test -p hyper-messenger-cli` passes (exit 0) AND `cargo test --workspace --all-features` still passes.
  QA scenarios (name the exact tool + invocation): happy — `cargo test -p hyper-messenger-cli` exits 0; failure — under default features `chats discord` still returns `NOT_REGISTERED` (asserted), and under `--all-features` `AUTH_REQUIRED`. Evidence `<attemptDir>/task-15-platform-expansion.txt`
  Commit: Y | test(cli): assert per-feature provider behaviour instead of assuming discord is enabled
  Recommended task executor category: quick - single test file.

- [x] 16. Strengthen slackbot QA assertions and document the client choice (wave-1 verification follow-up)
  What to do / Must NOT do: (a) `crates/providers/slackbot/tests/qa/run.sh` happy path asserts only `"ok":true`; make it assert seeded values (the seeded channel name and message id/content from the mock), matching the other providers' QA. (b) The plan said use slack-morphism or a thin reqwest client and STATE the choice in the crate doc; the crate uses the reqwest thin client but does not document why — add a one-line note to the crate doc (`crates/providers/slackbot/src/lib.rs`) explaining the thin-client choice (workspace dependency-light, same endpoints). Test + doc only; do NOT change provider behaviour.
  Closes: wave-1 verification non-blocking observations (todo 6 quality)
  Parallelization: Wave 3 | Blocked by: none | Blocks: none
  References: `crates/providers/slackbot/tests/qa/run.sh`, `crates/providers/slackbot/src/lib.rs`; sibling QA `crates/providers/telegrambot/tests/qa/run.sh` (asserts seeded values).
  Acceptance criteria (agent-executable): `bash crates/providers/slackbot/tests/qa/run.sh` passes AND its happy-path lines assert a seeded channel/message value (grep for the seeded string); `cargo build --workspace --all-features` still exits 0.
  QA scenarios (name the exact tool + invocation): happy — the strengthened `run.sh` exits 0 and its output includes the seeded value; failure — a regression that returns an empty chat list fails the assertion. Evidence `<attemptDir>/task-16-platform-expansion.txt`
  Commit: Y | test(slackbot): assert seeded QA values and document the thin-client choice
  Recommended task executor category: quick - one QA script + one doc line.

## Final verification wave
> Runs in parallel after ALL todos. ALL must APPROVE. Surface results and wait for the user's explicit okay before declaring complete.
- [x] F1. Plan compliance audit
- [x] F2. Code quality review
- [x] F3. Real manual QA
- [x] F4. Ideal-state fidelity

## Commit strategy
One atomic commit per todo, in the repo's observed Conventional Commit style (`feat(<scope>): ...`), staging only that todo's files. Wave-0 foundation commits first (core kinds, provider-common + send-retry policy, sidecar boundary, CLI auth param), then each provider, then docs. Never carry a verified provider's files into another commit. Run `cargo fmt --all`, `cargo clippy --workspace --all-features --all-targets -- -D warnings`, and `cargo test --workspace --all-features` before each commit; a red gate blocks the commit.

## Success criteria
> One row per IS row. The plan is complete only when every IS row has a delivering todo and a proving QA scenario; F4 checks the delivered behavior against these rows 1:1, and a shortfall becomes new `- [ ] N.` rows, never a note.
| IS | Delivering todo(s) | Proving QA scenario | Evidence |
| --- | --- | --- | --- |
| IS-1 | 1,4,5,6,7,8,9,10,11,12 | each provider's advertised ops return `ok:true` on its mock, an unadvertised op (e.g. `chats whatsappbot`) returns `UNSUPPORTED`, and after all registrations `target/debug/hyper-messenger providers` lists all 13 kinds (checked in F3) | `<attemptDir>/task-<N>-platform-expansion.txt` |
| IS-2 | 14,4,5,6,7,8,9,10,11,12 | `auth login <p> --param k=v` or env vars populate credentials headlessly; iMessage reports the macOS/Full-Disk-Access requirement as a typed error | `<attemptDir>/task-<N>-platform-expansion.txt` |
| IS-3 | 2,3,8,9,10 | imessage/whatsapp/telegram run through the subprocess/FFI boundary with a stub double; core stays dependency-clean | `<attemptDir>/task-{3,8,9,10}-platform-expansion.txt` |
| IS-4 | 1,2,14,4..13 | adding a platform changes only its crate + `register_builtin` + a feature; the only surface change is the generic todo 14 | `git diff --stat` shows no `crates/{cli,mcp,server,ui}` changes beyond todo 14 |
| IS-5 | all | `cargo build/test/clippy/fmt` all green; advertised-but-unimplemented ops return `NOT_IMPLEMENTED` | `cargo test --workspace --all-features` output |
| IS-6 | 2 | discord + channeltalk use `provider-common`; no duplicated retry code remains; a send with an unknown outcome is not retried | `<attemptDir>/task-2-platform-expansion.txt` |
