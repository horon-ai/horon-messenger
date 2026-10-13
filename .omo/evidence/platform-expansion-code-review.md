# F2 Code-Quality Review — platform-expansion (new platform providers)

- Reviewer: omo-native-code-reviewer (st_01a136f5), READ-ONLY (only write: this report)
- Worktree: /Users/alpox/Desktop/dev/alpox/hyper-messenger-wt/w-final (branch ulw/w-final, HEAD 2482ef9)
- Base ref for the diff: fdee558 (last commit before the platform-expansion series)
- Report path note: no ulw-loop plan exists for this session (SDK `currentAttemptDir`/`goalId` returned `undefined`, i.e. `ULW_LOOP_PLAN_MISSING`), so the `.omo/evidence/<goal>-code-review.md` fallback was used, matching F1/F4.
- Date: 2026-10-13

## codeQualityStatus: WATCH
## recommendation: APPROVE
## blockers: (none)

---

## Skill-perspective check (required)

Both skills were loaded and read directly before judging:
- `/Users/alpox/.bun/install/global/node_modules/omo-ai/plugin/skills/remove-ai-slops/SKILL.md`
- `/Users/alpox/.bun/install/global/node_modules/omo-ai/plugin/skills/programming/SKILL.md`

Result: the check RAN. Neither skill perspective is violated at a level that blocks this goal
(no CRITICAL/HIGH). The diff does trip two documented criteria at MEDIUM/LOW: tautological
capability tests (remove-ai-slops category 9 / programming "test restates the change") and
unsafe-FFI blocks without SAFETY comments (programming Rust iron list). See findings.

---

## Per-area results

| Area | Result | Evidence |
|------|--------|----------|
| Build / clippy `-D warnings` | PASS | `cargo clippy --workspace --all-features --all-targets -- -D warnings` -> exit 0 (`/tmp/f2-clippy.log`, `CLIPPY_EXIT=0`) |
| Formatting | PASS | `cargo fmt --all --check` -> exit 0 (`/tmp/f2-fmt.log`, `FMT_EXIT=0`) |
| Workspace tests | PASS | `cargo test --workspace --all-features` -> exit 0, 145 passed / 0 failed (`/tmp/f2-test.log`, `TEST_EXIT=0`) |
| Typed errors, no `anyhow` in libs | PASS | `grep -rn anyhow crates/providers crates/core/src crates/sdk/src crates/cli/src` -> empty |
| No panics/unwrap in non-test code | PASS | every `unwrap`/`expect`/`panic!` hit is inside a `#[cfg(test)]` module (e.g. `instagram/src/client.rs:225` is inside `#[cfg(test)] impl`, `telegram/src/lib.rs:366` inside `#[cfg(test)]`, `line/src/lib.rs:187` inside `#[cfg(test)]`) |
| Send-retry policy | PASS | `common/src/client.rs:213-236`; non-idempotent POST (`client.rs:76-78`) is not retried on transport/body failure or 5xx; proven by `non_idempotent_send_is_not_retried_on_transport_failure` / `..._on_truncated_body` |
| Resource leaks (sidecar children) | PASS | `JsonRpcProcess` sets `kill_on_drop(true)` (`common/src/sidecar.rs:150`) plus explicit `shutdown` (`sidecar.rs:213`); callers kill+reap (`whatsapp/src/sidecar.rs:84`, `line/src/lib.rs:216..408`, `imessage/src/lib.rs:215..322`) |
| `provider-common` actually reused | PASS | all HTTP providers build `Request`/use `HttpClient` (`common/src/lib.rs:1-11`); JSON-RPC only in `common/src/sidecar.rs` (no per-provider copy) |
| Naming, one-line public docs | PASS (nit) | matches the existing discord/channeltalk pattern; see LOW-1 |
| No narration comments | PASS | `grep` for non-doc `//` comments across all new provider `src` -> empty |
| No dead/duplicated code | PASS | clippy `-D warnings` clean; no duplicated retry logic (only `telegrambot/src/client.rs:168-172` maps `retry_after` into a typed error) |
| Evidence integrity | PASS | F3 manual-QA artifacts under `.omo/evidence/ulw/<session>/F3/` and `ulw-execute/platform-expansion/verify.log` (`CLIPPY_EXIT=0`, `FMT_EXIT=0`, `VERIFY_DONE`); provider registry shows all 13 kinds |

---

## Defects

### CRITICAL
None.

### HIGH
None.

### MEDIUM

- **M1 — TDLib client is never closed/destroyed; the shared library handle is `dlclose`d while TDLib
  threads may still run.** `crates/providers/telegram/src/tdlib.rs:90-101` creates a TDLib client via
  `create_client()` (`tdlib.rs:96`) but there is no `Drop`, no `close`, and no `td_destroy_client`
  anywhere (`grep` for `impl Drop`/`td_destroy`/`close` in `crates/providers/telegram/src` finds only
  the error string at `tdlib.rs:153`). On drop, `TdlibLibrary._handle: Option<libloading::Library>`
  (`tdlib_library.rs:23`) is unloaded via `dlclose` while TDLib's background threads may still hold code
  pointers into it. This is a leaked client handle and a potential crash/UB for any surface that drops
  and recreates the backend in-process. Not a goal blocker (CLI processes are short-lived and the
  no-library path degrades to a typed `Error::Config`), but it is a real resource-leak defect.

- **M2 — the `tdlib` feature is not reachable from any shipped surface, while `docs/providers.md`
  presents Telegram as working with only `libtdjson` as a prerequisite.** `crates/sdk/Cargo.toml:16`
  (`telegram = ["dep:hyper-messenger-provider-telegram"]`) and `:26` (`all = [..., "telegram", ...]`) do
  not forward `hyper-messenger-provider-telegram/tdlib`, so `--features hyper-messenger-sdk/all` yields a
  Telegram provider whose every operation returns `Error::Config` (`telegram/src/lib.rs:111-118`). The
  docs row (`docs/providers.md`, Telegram) lists only "libtdjson on the library path or `TDLIB_PATH`"
  and omits that the non-default cargo feature must also be enabled (as the QA does explicitly:
  `telegram/tests/qa/run.sh` builds `.../qa-fake`). Documentation/wiring gap; the backend is reachable
  only by naming the provider-crate feature directly.

- **M3 — tautological capability tests that merely mirror the implementation constant.**
  `whatsappbot/src/lib.rs:236`, `whatsapp/src/lib.rs:322`, `imessage/src/lib.rs:566`,
  `line/src/lib.rs:813` each `assert_eq!(provider().capabilities(), Capabilities::WHOAMI | ...)`, a
  verbatim copy of the impl. They cannot fail for any regression and must be edited in lockstep with the
  source (remove-ai-slops category 9; programming "test restates the change"). MEDIUM by policy; deleting
  them is a fix, not a coverage loss. The `capabilities` set is a contract, but pinning it here guards
  the diff, not behavior.

- **M4 — `unsafe` FFI blocks carry no `SAFETY` comments.** `crates/providers/telegram/src/tdlib_library.rs:71`,
  `:80`, `:86`, `:91`, `:99`, `:114` call raw `td_create_client_id` / `td_send` / `td_receive` /
  `CStr::from_ptr` with no invariant documented (programming Rust iron list: every `unsafe` block needs a
  SAFETY comment and, for FFI, a Miri/soundness note). The pointer-lifetime contract of `td_receive`
  (valid until the next call, must not be freed) is load-bearing and undocumented.

- **M5 — many new files exceed the 250 pure-LOC ceiling** (programming Smell 1). Measured with
  `awk '!/^\s*$/ && !/^\s*(\/\/|#|--)/' | wc -l`: `line/src/lib.rs` 777, `imessage/src/lib.rs` 601,
  `telegram/src/tdlib.rs` 499, `telegram/src/lib.rs` 482, `common/src/client.rs` 435,
  `common/src/sidecar.rs` 378, `telegrambot/src/lib.rs` 344, `instagram/src/mappers.rs` 321, plus ~10
  more between 250 and 320 (counts include in-file `#[cfg(test)]` modules). Maintenance-level concern;
  no size marker (`// allow: SIZE_OK`) present.

### LOW

- **L1 — public items missing the one-line doc comment AGENTS.md requires.** e.g.
  `whatsappbot/src/lib.rs:38,45,49`, `slackbot/src/lib.rs:32,39,43`, `slack/src/lib.rs:32,39,43`,
  `imessage/src/lib.rs:40,49,53`. Matches the pre-existing `discord/src/lib.rs:32,39,43` and
  `channeltalk/src/lib.rs:38,45,49` pattern, so it is a convention gap, not a regression.
- **L2 — a 429 retries a non-idempotent POST.** `common/src/client.rs:213` retries on `TOO_MANY_REQUESTS`
  regardless of `idempotent` (the test `too_many_requests_retries_then_succeeds` sends a POST and expects
  2 hits). A 429 means the server rejected the request, so this is standard and defensible, but the
  `send` doc "never retried after transmission" (`client.rs:283-286`) is imprecise — it is never retried
  after a *transport* failure.
- **L3 — `use tdlib_rs as _;`** (`telegram/src/tdlib.rs:13`) is a dead-looking import kept only to force
  the optional dependency into the link; intentional per the module doc, but reads as leftover.
- **L4 — EPOCH sentinel for missing timestamps** (`line/src/lib.rs:41`, `imessage/src/lib.rs:31`,
  `1970-01-01T00:00:00.000Z`) instead of `core::now_rfc3339`. Defensible (a historical message should not
  be stamped "now"), but deviates from the AGENTS.md timestamp convention; no test pins it.

---

## Verification performed this session
- `cargo clippy --workspace --all-features --all-targets -- -D warnings` -> exit 0.
- `cargo fmt --all --check` -> exit 0.
- `cargo test --workspace --all-features` -> exit 0 (145 passed, 0 failed).
- Inspected the full diff `fdee558..HEAD` (109 files, +14910/-357), the shared `provider-common`, every
  new provider client/lib, the TDLib FFI layer, the sdk/cli/core wiring, and the QA scripts.

## F2 VERDICT: PASS
