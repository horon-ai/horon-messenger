# F1 gate review - platform-expansion (plan compliance audit)

- recommendation: **APPROVE**
- blockers: none
- reviewer role: final gate reviewer (omo-native-gate-reviewer, task st_01a136f4)
- worktree: /Users/alpox/Desktop/dev/alpox/hyper-messenger-wt/w-final (branch ulw/w-final, dev tree 2482ef9)
- report path note: no ulw-loop plan exists for this session (SDK returned `ULW_LOOP_PLAN_MISSING`), so the
  `.omo/evidence/<goal>-gate-review.md` fallback was used.

## originalIntent

Add nine more messenger connectors (iMessage, Instagram, LINE, Slack user, Slack bot, Telegram user,
Telegram bot, WhatsApp user, WhatsApp bot) to the existing Rust multi-messenger bridge behind the one
shared `Provider`/`Envelope` contract, plus the shared plumbing (core kinds, provider-common, a
sidecar/FFI boundary, a generic CLI `auth login --param`), wired into the sdk and documented - without
realtime listeners, without credential extraction, and without media/emoji/reaction/thread management
beyond send.

## desiredOutcome

Each of the 9 platforms is selectable by name and returns the same envelope shape for the operations it
advertises; credentials are provisioned headlessly; non-HTTP platforms run through one declared isolated
boundary; a 10th platform stays cheap (one crate + one registration + one feature); the workspace
builds/tests/clippy/fmt clean; and HTTP retry/rate-limit plus the never-retry-an-unknown-send policy live
in one place.

## userOutcomeReview

The shipped artifact matches the intent. All 13 providers register and are listed by `providers`
(kakaotalk, discord, telegram, channeltalk, mock, slack, slackbot, telegrambot, whatsapp, whatsappbot,
line, imessage, instagram). The whole workspace builds (`--all-features`), tests (all green), clippy
`-D warnings` clean, fmt clean. Each provider ships a committed, re-runnable mock/stub QA script under
`crates/providers/<name>/tests/qa/`; I ran all eleven (discord, channeltalk, telegrambot, whatsappbot,
slackbot, slack, imessage, telegram, whatsapp, line, instagram) and every one exited 0. The nine new
providers return real envelopes over their mocks/stubs, the advertised-but-unimplemented case is handled
by the framework gate, and the guardrails hold. This is a PASS; the residual items are notes, not
criterion failures.

## 16-checkbox audit

| # | Deliverable | Verdict | Evidence |
|---|---|---|---|
| 1 | core kinds + aliases | PASS | `crates/core/src/kind.rs:14-26` (13 variants), `:29` (`ALL` len 13), `:111-136` (FromStr, `-`/`_`/` ` normalized → `slack-bot`/`i-message`/`ig` aliases); `cargo test -p hyper-messenger-core` (13 passed); `chats bogus` → `INVALID_INPUT` exit 1 (task-1.txt) |
| 2 | provider-common + send-retry, adopted by discord/channeltalk | PASS | `crates/providers/common/src/client.rs` (HttpClient, backoff, 429 Retry-After, bucket gate, `send()` never retried, `status.rs` map_http_status); `client.rs:319-380` tests prove non-idempotent not retried on Drop/truncated body while GET is; `discord/src/client.rs:8`, `channeltalk/src/client.rs:9` import common |
| 3 | subprocess JSON-RPC boundary | PASS | `crates/providers/common/src/sidecar.rs` (`SidecarSpec`, `JsonRpcProcess`, timeout, typed errors); tests: round-trip, timeout, missing binary, non-JSON, dead sidecar |
| 4 | telegrambot | PASS | `crates/providers/telegrambot/src/{lib,client,models}.rs`, 7 ops, feature `telegrambot`; QA port 8811 exit 0 (wrong token → AUTH_FAILED) |
| 5 | whatsappbot | PASS | `crates/providers/whatsappbot/src/lib.rs:114-115` capabilities exactly WHOAMI\|SEND_MESSAGE; test `capabilities_are_exactly_whoami_and_send_message`; QA 8812 exit 0 (`chats` → UNSUPPORTED, bad token → AUTH_FAILED) |
| 6 | slackbot (thin reqwest) | PASS | `crates/providers/slackbot/src/{lib,client}.rs`, 7 ops (no SEARCH), `client.rs:32` ok:false→AuthFailed, token-prefix validation; QA 8813 exit 0 |
| 7 | slack user | PASS | `crates/providers/slack/src/{lib,client,mappers}.rs`, 8 ops incl SEARCH_MESSAGES; `client.rs:21,286-309` cookie+token header and ok:false mapping tests; QA 8814 exit 0 (missing cookie → AUTH_FAILED) |
| 8 | imessage (imsg sidecar, macOS) | PASS | `crates/providers/imessage/src/lib.rs`, 9 ops; `:166-172` `ensure_macos` → Unsupported off-macOS; missing-binary (CONFIG) and Full-Disk-Access (AUTH_FAILED naming the permission) tests; QA 8815 exit 0 |
| 9 | telegram (TDLib) | PASS | `crates/providers/telegram/src/{lib,mapping,transport,tdlib,tdlib_library,qa_fake}.rs`, 9 ops; missing-library test → typed CONFIG, no panic; QA 8816 exit 0 (qa-fake fixtures). See N2/N3 |
| 10 | whatsapp (baileys sidecar) | PASS | `crates/providers/whatsapp/src/{lib,sidecar,models}.rs` + `sidecar/index.mjs`+`package-lock.json`; `node --check` OK; sidecar-path resolution test (env then CARGO_MANIFEST_DIR); QA 8817 exit 0 + real-sidecar `auth.status` smoke |
| 11 | line (linejs sidecar) | PASS | `crates/providers/line/src/{lib,client,models}.rs` + `sidecar/index.js`; crate doc records path (b); QA 8818 exit 0 (happy + auth failure + malformed → PROTOCOL + no-credentials → AUTH_REQUIRED) |
| 12 | instagram | PASS | `crates/providers/instagram/src/{lib,client,mappers,session}.rs`, 8 ops; `client.rs:247-302` header + timeout→NETWORK tests; QA 8819 exit 0 (401 → AUTH_FAILED). See N4 |
| 13 | docs | PASS | `docs/providers.md` platform table covers all 9 (auth/transport/prereqs) + "Registering a provider"; `README.md:25-33,57-65` provider table/status; QA: grep of all 9 names passes (task-13.txt) |
| 14 | CLI `auth login --param` | PASS | `crates/cli/src/main.rs:98-99,149-166,264-282` (repeatable `--param key=value` → `LoginOptions.params`, merged with `--account`); test `login_with_params_authenticates_the_mock_provider` + `..._without_an_equals_is_a_usage_error` |
| 15 | cli per-feature test fix | PASS | `crates/cli/tests/cli.rs:66-76` discord test gated `#[cfg(feature = "discord")]`; `crates/cli/Cargo.toml` adds `discord = ["hyper-messenger-sdk/discord"]`; `cargo test -p hyper-messenger-cli` → 8 passed, exit 0 |
| 16 | slackbot QA + thin-client doc | PASS | `crates/providers/slackbot/tests/qa/run.sh` asserts seeded `"name":"general"` and `"message_id":"1700000000.000200"`; `crates/providers/slackbot/src/lib.rs:3` thin-client note; QA exit 0 |

## Must-NOT guardrail checks

- No realtime listeners / websocket / socket-mode / getUpdates-loop / MQTT / long-poll: **respected**. No
  listener operation exists in the framework (`crates/core/src/provider.rs` has no `listen`), and no
  provider implements one; the only `getUpdates` use is a single `list_chats` call
  (`telegrambot/src/client.rs:105`), matching todo 4. See N1 for a cosmetic capability flag.
- No desktop-app / browser credential extraction: **respected** (grep for chromium/keychain/cookie-decrypt
  is empty).
- No emoji/sticker/reaction/thread/pin/bookmark management beyond send: **respected** (only `thread_ts`
  used as `reply_to` mapping in slackbot, not thread management).
- No new dependencies inside `crates/core`: **respected** (`git diff 434f8a8..HEAD -- crates/core/Cargo.toml`
  is empty).
- Only surface change is the generic auth login --param: **respected**. `git diff --stat 434f8a8..HEAD --
  crates/cli crates/mcp crates/server crates/ui` shows only `cli/src/main.rs` (todo 14), `cli/Cargo.toml`
  (feature for todo 15) and `cli/tests/cli.rs` (todo 15 test-only). mcp/server/ui untouched.
- No auto-retry of an unknown send outcome: **respected** (`common/src/client.rs` `send()` non-idempotent;
  tests assert exactly one request on transport failure).
- No live-account integration tests: **respected** (all QA is mock/stub doubles).

## Evidence gaps

- No separate code-review (F2) report artifact exists for this goal, so I could not cross-check its
  skill-perspective coverage. I ran the remove-ai-slops / programming pass directly instead (below).
- The non-macOS iMessage `UNSUPPORTED` branch is compile-time (`cfg!(target_os = "macos")`) and was not
  executed on this macOS host; it is covered by the crate gate + unit tests, per the plan's stated
  macOS-only limitation.

## remove-ai-slops / programming pass (run directly over diff, tests, production)

- Overfit/slop: no excessive or useless tests; no deletion-only tests (this is a feature-add expansion);
  no tautological or implementation-mirroring tests found. Test names are behavioral (mapping, auth
  precedence, retry policy, header assembly, error mapping). The two capability-equality tests
  (whatsappbot, imessage) are plan-required contract assertions, not mirrors.
- Unnecessary production extraction/parsing/normalization: none beyond what the protocol needs
  (e.g. `account_id_for` phone normalization is required for stable `auth login --param phone=`).
- Maintenance burden / false confidence: see N1-N4.

## Notes (non-blocking; each is a gap NOT tied to a failed criterion)

- N1. `crates/providers/telegram/src/lib.rs:167,171` advertise `Capabilities::SEND_MEDIA` and
  `Capabilities::LISTEN`, neither of which has a backing operation (no `send_media`/`listen` in
  `crates/core/src/provider.rs`), while `docs/providers.md` states no provider claims realtime. This is a
  cosmetic capability-flag inaccuracy; IS-5 explicitly permits advertised-but-unimplemented operations
  returning NOT_IMPLEMENTED, and no listener is implemented, so the guardrail holds. (SEND_MEDIA is also
  advertised by the pre-existing kakaotalk skeleton.)
- N2. Todo 9 specified `tdlib-rs`; the crate loads `libtdjson` at runtime via `libloading`
  (`crates/providers/telegram/src/tdlib_library.rs`) with `tdlib-rs` as an optional `tdlib` feature. The
  crate doc justifies it (all-features build must not download/link a native lib). Todo-9 acceptance
  criteria (build/clippy/unit tests/missing-library path) are met.
- N3. `crates/providers/telegram/src/lib.rs` doc-comment quality is high; the deviation is documented, so
  it is not an undisclosed scope change.
- N4. `crates/providers/instagram` sends the POST body as a JSON-encoded form string because the shared
  `HttpClient` always serializes via `reqwest::json()` (task-12 note); a real IG endpoint may reject the
  content-type. The mock accepts both, so this is a fidelity caveat, not a criterion failure.

## Artifact paths checked

- Plan: `.omo/plans/platform-expansion.md`
- Evidence: `.omo/evidence/ulw-execute/platform-expansion/{task-1..16.txt, verify.log, manual-qa.log}`
- Code: `crates/core/src/{kind.rs,capability.rs,provider.rs}`, `crates/sdk/{src/lib.rs,Cargo.toml}`,
  `crates/cli/{src/main.rs,tests/cli.rs,Cargo.toml}`, root `Cargo.toml`,
  `crates/providers/common/src/{client.rs,sidecar.rs,status.rs,url.rs}`,
  `crates/providers/{telegrambot,whatsappbot,slackbot,slack,imessage,telegram,whatsapp,line,instagram}/**`,
  `crates/providers/whatsapp/sidecar/{index.mjs,package-lock.json}`,
  `crates/providers/line/sidecar/{index.js,package.json}`, `docs/providers.md`, `README.md`
- Commands re-run in the worktree: `cargo build --workspace --all-features` (exit 0),
  `cargo test --workspace --all-features` (all `test result: ok`, exit 0),
  `cargo clippy --workspace --all-features --all-targets -- -D warnings` (exit 0),
  `cargo fmt --all --check` (exit 0), `hyper-messenger providers` (13 kinds),
  all 11 `crates/providers/*/tests/qa/run.sh` (exit 0).

F1 VERDICT: PASS
