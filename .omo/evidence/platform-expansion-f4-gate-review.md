# F4 Gate Review — platform-expansion ideal-state fidelity

- Reviewer: omo-native-gate-reviewer (st_01a136f7), READ-ONLY
- Worktree: /Users/alpox/Desktop/dev/alpox/hyper-messenger-wt/w-final (branch ulw/w-final, HEAD 2482ef9)
- Plan: .omo/plans/platform-expansion.md (IS-1..IS-6, Success criteria table)
- Base ref used for diff: fdee558 (last commit before the platform-expansion series)
- Attempt dir: ulw-loop plan missing for this session (`ULW_LOOP_PLAN_MISSING`); fallback path per task rules: `.omo/evidence/platform-expansion-f4-gate-review.md`
- Date: 2026-10-13

## recommendation

APPROVE

## blockers

(none)

## originalIntent

The operator wanted nine more messengers (iMessage, Instagram, LINE, Slack user/bot, Telegram user/bot, WhatsApp user/bot) reachable through the same one CLI/SDK/MCP/HTTP contract already used for Discord and Channel Talk, each headless-authenticated, consistent in output shape, cheap to extend, and never auto-retrying a send whose outcome is unknown.

## desiredOutcome

Every listed platform selectable by name and returning the same `Envelope` for advertised ops (unadvertised ops `UNSUPPORTED`); headless credential provisioning; a single isolated subprocess/FFI boundary with a dependency-clean core; per-platform additions confined to crate + registration + feature; a clean build/test/clippy/fmt workspace with `NOT_IMPLEMENTED` for advertised-but-unbuilt ops; shared HTTP/retry reused by discord + channeltalk with no send retry on unknown outcome.

## userOutcomeReview

Delivered behavior satisfies every IS row. Independently reproduced from the worktree: `cargo build --workspace --all-features` exit 0, `cargo test --workspace --all-features` 145 passed / 0 failed, `cargo clippy --workspace --all-features --all-targets -- -D warnings` exit 0, `cargo fmt --all --check` exit 0. `target/debug/hyper-messenger providers` lists all 13 kinds; `chats whatsappbot` returns `UNSUPPORTED`; `chats bogus` returns `INVALID_INPUT`; `auth login mock --param token=abc` authenticates and `--param foo` exits 2. The change is confined to provider crates + core kinds + sdk registration/features + the one generic CLI auth-param plumbing; mcp/server/ui untouched. iMessage is the documented macOS/Full-Disk-Access exception. The user gets exactly the outcome the plan promised.

## IS row results

| IS | Verdict | Delivering evidence |
| --- | --- | --- |
| IS-1 | MATCH | `providers` lists 13 kinds (F3 `s1-cli-providers.txt`; my rerun); `chats whatsappbot` → `UNSUPPORTED`, exit 1 (my rerun); uniform `Envelope` from core. `crates/core/src/provider.rs:29-41` capability gate. |
| IS-2 | MATCH | `auth login --param` plumbing `crates/cli/src/main.rs:98,149-167`; my rerun: `--param token=abc` ok, malformed exit 2. Owner-only `<config>/<provider>/accounts/<account>.json` via `CredentialStore` → `write_private_atomic` (`crates/core/src/credential.rs:16,62`). iMessage macOS/FDA exception documented `docs/providers.md:62` + F3 `s2-imessage.log` (FDA-denial assert). |
| IS-3 | MATCH | imessage/whatsapp/line spawn the sidecar through `provider-common` `JsonRpcProcess`/`SidecarSpec` (`crates/providers/imessage/src/client.rs:6`, `whatsapp/src/sidecar.rs:7`, `line/src/client.rs:6`); telegram loads `libtdjson` at runtime via `libloading` (`crates/providers/telegram/src/lib.rs:10-17`). Core stays dependency-clean: `git diff fdee558..HEAD -- crates/core/Cargo.toml` empty. |
| IS-4 | MATCH | `git diff --stat fdee558..HEAD -- crates/cli crates/mcp crates/server crates/ui` shows only `crates/cli/{Cargo.toml,src/main.rs,tests/cli.rs}`; mcp/server/ui untouched. Per-provider commits touch only their crate; scaffold commits touch `crates/sdk/src/lib.rs` + workspace `Cargo.toml`. |
| IS-5 | MATCH | Independent run (/tmp/f4_verify.log): BUILD_EXIT=0, CLIPPY_EXIT=0, FMT_EXIT=0, 145 tests passed / 0 failed. Gate distinguishes `NotImplemented` vs `Unsupported` (`crates/core/src/provider.rs:29-41`; test `crates/core/tests/contracts.rs:45-68`). |
| IS-6 | MATCH | discord + channeltalk depend on and `use` `provider-common` `HttpClient`/`Request` (`.../Cargo.toml:13`, `.../src/client.rs:8-9`); no residual retry/backoff in their clients (grep empty). Non-idempotent send never retried and message states the outcome is unknown (`crates/providers/common/src/client.rs:170-181`); tests `client.rs:390-431`. Evidence `task-2.txt`. |

## gap list

(none — no stated IS criterion fails)

## checked artifact paths

- .omo/plans/platform-expansion.md
- .omo/evidence/ulw/01a12e84-9150-7a68-8410-b699d5ff978c/F3/F3-manual-qa.md
- .omo/evidence/ulw/01a12e84-9150-7a68-8410-b699d5ff978c/F3/s1-cli-providers.txt, s2-*.log, s3-http.txt, s3-mcp.txt, s3-server.log, s4-cleanup.txt
- .omo/evidence/ulw-execute/platform-expansion/{task-1..task-16}.txt, verify.log, manual-qa.log
- Worktree: crates/core/src/provider.rs, crates/core/src/credential.rs, crates/core/tests/contracts.rs, crates/cli/src/main.rs, crates/cli/tests/cli.rs, crates/sdk/src/lib.rs, crates/providers/common/src/client.rs, crates/providers/{imessage,whatsapp,line,telegram,discord,channeltalk}/src/*, docs/providers.md, README.md
- Live reruns: /tmp/f4_verify.log; `target/debug/hyper-messenger providers|chats whatsappbot|chats bogus|auth login mock --param`

## remove-ai-slops / programming overfit pass (direct)

- No deletion-only or removal-verifying tests: this is additive work; no test asserts a requested removal.
- No tautological/implementation-mirroring tests observed: tests assert fixtures→model mapping, error-code mapping (401→AUTH_FAILED, malformed→PROTOCOL, timeout→NETWORK), auth precedence, capability gate, and the send-retry policy — all behavioral, none re-stating the implementation.
- Test volume is proportionate (145 workspace tests for 9 providers + shared crate); no excessive/duplicate tests.
- No unnecessary production extraction/parsing/normalization; the sidecar/FFI boundary and `libloading` loader are the required IS-3 mechanism, not speculative scaffolding.
- Code review (F2) report was not present as a file in the evidence dirs to cross-check its skill-perspective coverage; this direct pass is recorded here. No maintenance-burden, false-confidence, or scope-drift finding rises to a blocker.

## notes (non-blocking)

- `docs/architecture.md` is also modified (docs only; not a surface crate) — outside IS-4's explicit surface list, no criterion violated.
- Plan checkboxes for todos 8–13 remain `[ ]` in the plan file although the work is committed (2482ef9) and verified; bookkeeping only, the IS rows are delivered. Not a delivered-behavior shortfall.

## evidence gaps

None that map to a stated criterion. Exact gap noted: the F2 code-review report artifact was not found under the checked evidence paths, so its explicit skill-perspective coverage could not be confirmed from a file; the reviewer's own overfit/slop pass above covers that requirement.

F4 VERDICT: PASS
