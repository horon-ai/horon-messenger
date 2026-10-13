---
slug: platform-expansion
status: approved-plan-written
intent: clear
review_required: true
plan_path: .omo/plans/platform-expansion.md
plan_sha256: c69fc3fefde69f86f16c8fad827237b615701fd651a67fb40e6c9ffbade40cf4
review_round_id: r3-platform-expansion
review_round_limit: 5
pending-action: review .omo/plans/platform-expansion.md (round 3)
review:
  plan_reviewer:
    status: approved
    workspace_root: /Users/alpox/Desktop/dev/alpox/hyper-messenger
    runtime_home: null
    target: .omo/plans/platform-expansion.md
    round_id: r3-platform-expansion
    plan_sha256: c69fc3fefde69f86f16c8fad827237b615701fd651a67fb40e6c9ffbade40cf4
    launch_id: launch-r3
    session: st_01a1364f
    result: "[OKAY] round 3 - approved, no blockers. Notes (non-blocking): (a) todo 11 dependency cell lists 1,2,3,14 in the matrix vs 1,2,14 on the todo line; (b) todo 14 QA uses cargo run --all-features which is fine because mock is a default sdk feature."
  history:
    r1_plan_reviewer: "[OKAY] notes: todo1 QA --all-features; auth login param passthrough; matrix 8<-2"
    r1_codex: "FAIL 6 findings - folded into r2"
    r2_plan_reviewer: "[OKAY] approved, no blockers"
    r2_codex: "4 PASS / 2 FAIL (whatsapp sidecar deploy, reproducible QA) + 4 new (todo1 acceptance, CLI feature flags, LINE path/dep, mock-only evidence) - folded into r3"
  codex_chatgpt_web:
    status: done
    target: .omo/plans/platform-expansion.md
    result: "r1 FAIL, r2 partial; codex call budget (2) exhausted, no r3 codex call"
approach: Add nine platform providers to the existing per-crate provider model, tiered by transport difficulty, behind a shared HTTP/retry helper and a declared sidecar boundary for platforms with no viable Rust library; wire each into the sdk and prove each with a mock-HTTP or subprocess-double real-surface QA.
---

# Draft: platform-expansion

## Affected user and ideal state

**Affected users**
- Primary: 민열님 (the operator) — drives every messenger through one CLI / MCP server / HTTP server instead of a different tool per platform.
- Secondary: an AI agent consuming the MCP tools and the SDK `Client` — needs one uniform request/response shape across all platforms.
- Tertiary: a future maintainer adding platform #10 — needs "add a crate + register it" to be the whole cost.

**Ideal state (IS rows)**

| Row | Statement | Reason |
| --- | --- | --- |
| IS-1 | Every listed platform (imessage, instagram, line, slack, slackbot, telegram, telegrambot, whatsapp, whatsappbot) is selectable by name and returns the same `Envelope` shape (profile, chats, messages, send) as mock/discord/channeltalk. | One mental model and one parseable output for the operator and the agent. |
| IS-2 | Each platform's credentials are provisioned without GUI/browser extraction — via an env var or `auth login` with explicit credentials, stored owner-only under `<config>/<provider>/`. | Reproducible, headless, testable; no macOS-only or Chromium dependency for setup. |
| IS-3 | Platforms with no viable Rust library still work through one declared, isolated boundary (HTTP, a subprocess sidecar, or FFI), so the core and every other provider stay pure Rust and portable. | Keeps the workspace a Rust project without pretending a library exists. |
| IS-4 | Adding a platform touches only its provider crate plus one registration line and one feature flag — never a surface. | The architecture's core promise; a 10th platform must stay cheap. |
| IS-5 | The whole workspace builds/tests/clippy/fmt clean; any advertised-but-unimplemented operation returns `NOT_IMPLEMENTED`, never panics. | The project must never half-work silently. |
| IS-6 | HTTP, retry/backoff and rate-limit behavior is defined once and reused, not copy-pasted per provider. | Nine providers duplicating it will drift. |

**Gap rows**

| Row | Difference from today | Reason | Closed by |
| --- | --- | --- | --- |
| GAP-1 | Only mock/discord/channeltalk implement operations; kakaotalk/telegram are skeletons; the other eight platforms are absent. | The operator cannot use them at all. | T1-T9 |
| GAP-2 | No auth flow exists beyond a token/cookie field; no `auth login` populates credentials for the new platforms. | IS-2: setup must be headless. | T2-T9 (auth), T10 |
| GAP-3 | No sidecar/subprocess/FFI boundary exists in the codebase. | IS-3: some platforms need one. | T0, T6, T7, T9 |
| GAP-4 | `ProviderKind` lacks the new variants; sdk features and workspace deps are not wired. | IS-1/IS-4. | T0 |
| GAP-5 | Retry/rate-limit logic is duplicated in discord and channeltalk clients. | IS-6. | T0 |

## Components (topology ledger)

| id | outcome (one line) | status | evidence path |
| --- | --- | --- | --- |
| C1 | core gains the new `ProviderKind` variants (+ any shared auth param type) | active | crates/core/src/kind.rs |
| C2 | a shared `hyper-messenger-provider-common` crate: HTTP client builder, retry/backoff, rate-limit gate, url/query helpers | active | crates/providers/common/src/lib.rs |
| C3 | one declared sidecar/FFI boundary (subprocess JSON-RPC or native lib) reused by the platforms that need it | active | crates/providers/common (sidecar module) |
| C4 | nine provider crates, one per platform | active | crates/providers/<name>/ |
| C5 | sdk registration + features + workspace members/deps | active | crates/sdk/src/lib.rs, Cargo.toml |
| C6 | docs: provider table, per-platform auth, sidecar prerequisites | active | docs/providers.md, README.md |

## Open assumptions (announced defaults)

| assumption | adopted default | rationale | reversible? |
| --- | --- | --- | --- |
| ProviderKind granularity | One variant per reference CLI: Slack, SlackBot, TelegramBot, WhatsApp, WhatsAppBot, Line, IMessage, Instagram (Telegram already exists). | Matches the reference's credential/operation split; keeps each provider's auth and ops distinct. | yes |
| Listeners / realtime | OUT of this plan; read/send only, consistent with the current project where no provider (incl. discord) has a listener. | Keeps the 9 ports uniform and comparable; realtime is a separate capability. | yes |
| Token/cookie extraction | OUT; credentials via env or `auth login`. | Consistent with the prior scope; headless and testable. | yes |
| Sidecar language | Node only where unavoidable (whatsapp/baileys); otherwise subprocess (imsg) or FFI (tdjson). | No viable Rust lib for some; isolate rather than reimplement. | yes |
| Shared helper | Introduce `provider-common` rather than copy retry logic nine times. | IS-6. | yes |
| Testing | tests-after per provider + a mock-HTTP/subprocess-double real-surface QA (the discord/channeltalk pattern). | Proven pattern in this repo. | yes |

## Findings (cited - path:lines)

### External library research (source-backed, crates.io + GitHub API, 2026-10)

| Platform | Best option | Status | Verdict |
| --- | --- | --- | --- |
| Telegram (user) | `tdlib-rs` 1.4.0 (bundles libtdjson via `download-tdlib`); or pure-Rust `grammers-client` 0.10.0 | active 2026-04 / 2026-07 | feasible — prefer `tdlib-rs` over hand-rolled libloading FFI |
| WhatsApp (user) | `whatsapp-rust` 0.7.0 (pure Rust, young) or Node `baileys` sidecar | active but new / mature JS | feasible; sidecar is the stable default |
| LINE | no crate; `thrift` 0.25.0 hand-roll or Go sidecar (beeper/line) | none | hand-roll or sidecar |
| Instagram | no crate; Python/Node sidecar (instagrapi) | unofficial, undocumented | sidecar |
| Slack | `slack-morphism` 2.31.0 (Web API + Socket Mode + user tokens; RTM is dead) | active 2026-10 | feasible — prefer over hand-written REST |
| iMessage | `imsg` CLI (JSON-RPC over stdio) | active 2026-10 | feasible — sidecar confirmed |
| Generic | `tokio-tungstenite` 0.30.0, `reqwest` 0.13.5 | active | standards |

Sources: crates.io/GitHub for tdlib-rs, grammers, whatsapp-rust, baileys, thrift, slack-morphism, imsg, tokio-tungstenite, reqwest.

**Plan impact (to fold in on revision after the in-flight reviews):**
- todo 9 (telegram): use `tdlib-rs` (or `grammers-client`) instead of a hand-rolled `libloading` FFI loop.
- todos 6/7 (slackbot/slack): use `slack-morphism` instead of hand-written REST calls; RTM→Socket Mode is out of scope anyway.
- todo 10 (whatsapp): keep the sidecar default; note `whatsapp-rust` as an alternative.
- todos 11/12 (line/instagram): approach unchanged (no crate exists).

### Repo findings

- Current contract: `Provider` trait (read/send ops, `gate()` → NOT_IMPLEMENTED/UNSUPPORTED) at crates/core/src/provider.rs; model at crates/core/src/model.rs; `ProviderKind` at crates/core/src/kind.rs:16-24 (KakaoTalk, Discord, Telegram, ChannelTalk, Mock).
- Registration: `register_builtin` in crates/sdk/src/lib.rs:133-161; features `mock|kakaotalk|discord|telegram|channeltalk|all`.
- Reference transports (per platform, from /tmp/agent-messenger-ref/src/platforms/*):
  - imessage: spawns `imsg rpc` (JSON-RPC over stdio), mac-only (imessage/client.ts, rpc.ts).
  - instagram: HTTP `i.instagram.com/api/v1` + MQTT realtime + device fingerprint (instagram/client.ts:24-40, mqtt/).
  - line: Thrift protocol + E2EE, vendored linejs (line/client.ts:1-16).
  - slack: `@slack/web-api` + RTM websocket + QR login (slack/client.ts, listener.ts:67-73, qr-*).
  - slackbot: Web API + Socket Mode websocket (slackbot/client.ts, listener.ts).
  - telegram: TDLib via koffi FFI to tdjson (telegram/tdlib.ts).
  - telegrambot: Bot API HTTPS + getUpdates polling (telegrambot/client.ts:9-11).
  - whatsapp: baileys (JS multi-device protocol) (whatsapp/client.ts:1-14).
  - whatsappbot: Meta Graph API HTTPS (whatsappbot/client.ts:4-6).
- Reference sizes (LOC, non-test): slack 7404, instagram 3804, telegram 2709, slackbot 2595, telegrambot 1806, line 1626, whatsapp 1580, imessage 1384, whatsappbot 1043.

## Decisions (with rationale)

- **Flat ProviderKind variants** (see assumption) — simplest, matches reference.
- **Tiered waves** by transport: pure-HTTP first (telegrambot, whatsappbot), then HTTP+auth (slackbot, slack), then subprocess (imessage), then hard (telegram, whatsapp, line, instagram). Front-loads proven shapes and isolates the risky ports.
- **provider-common crate** for retry/rate-limit/url (GAP-5).
- **Sidecar boundary is a first-class component** (C3), not an afterthought, because three platforms (telegram, whatsapp, imessage) depend on it.
- **Each provider ships with its own mock-double QA** so completion is proven on a real surface without live accounts.

## Scope IN

- Nine provider crates implementing the standard read/send operations.
- core `ProviderKind` variants, sdk wiring, workspace deps.
- provider-common (HTTP/retry/rate-limit) + sidecar boundary.
- Auth via env / `auth login` for each provider.
- Docs + tests + mock-double QA per provider.

## Scope OUT (Must NOT have)

- No realtime listeners / websocket event streams (a separate capability).
- No desktop-app / browser token extraction (Chromium cookie decryption).
- No media/attachment upload beyond what a platform's send needs (no emoji/sticker/reaction/thread management).
- No live-account integration tests (mock doubles only).
- No changes to surface crates (cli/mcp/server/ui) beyond what registration requires.

## Open questions

- None blocking; the user asked for the plan immediately. Defaults above are announced for veto.

## Approval gate

status: approved (auto-passed per operating contract §8; user requested the plan be written immediately)
next workflow action: write `.omo/plans/platform-expansion.md`, then run plan-reviewer and the codex-chatgpt-web review.
