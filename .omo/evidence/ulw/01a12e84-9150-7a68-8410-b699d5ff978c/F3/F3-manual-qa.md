# F3 Manual QA Matrix — final verification of delivered surfaces

- Worktree: `/Users/alpox/Desktop/dev/alpox/hyper-messenger-wt/w-final` (branch `ulw/w-final`, HEAD `2482ef9`)
- Attempt dir: `/Users/alpox/Desktop/dev/alpox/hyper-messenger/.omo/evidence/ulw/01a12e84-9150-7a68-8410-b699d5ff978c/F3`
- Date: 2026-10-13 (UTC)
- Build gate: `cargo build --workspace --all-features` exit 0; `cargo build -p hyper-messenger-cli --features hyper-messenger-sdk/all` exit 0.

## surfaceEvidence

| Scenario | Criterion | Surface | Exact invocation | Observed | Verdict | artifactRefs |
|---|---|---|---|---|---|---|
| S1 | CLI lists all 13 provider kinds | CLI | `HYPER_MESSENGER_CONFIG_DIR=$(mktemp -d) target/debug/hyper-messenger providers` | `ok:true`, 13 kinds = kakaotalk, discord, telegram, channeltalk, mock, slack, slackbot, telegrambot, whatsapp, whatsappbot, line, imessage, instagram; exit 0 | PASS | A1 |
| S2-telegrambot | Committed mock QA asserts seeded values | CLI+mock HTTP | `bash crates/providers/telegrambot/tests/qa/run.sh` | exit 0; whoami account 123456789 / mock_bot, chats "Ada Lovelace", messages seeded text, send message_id 9001 | PASS | A2 |
| S2-whatsappbot | Committed mock QA asserts seeded values | CLI+mock HTTP | `bash crates/providers/whatsappbot/tests/qa/run.sh` | exit 0; send message_id wamid.mock123, account_id 123456789012345 | PASS | A3 |
| S2-slackbot | Committed mock QA asserts seeded values | CLI+mock HTTP | `bash crates/providers/slackbot/tests/qa/run.sh` | exit 0; `QA PASSED`; whoami UBOT, chats "general", send message_id 1700000000.000200 | PASS | A4 |
| S2-slack | Committed mock QA asserts seeded values | CLI+mock HTTP | `bash crates/providers/slack/tests/qa/run.sh` | exit 0; whoami alice, chats general, messages seeded | PASS | A5 |
| S2-imessage | Committed mock QA asserts seeded values | CLI+imsg stub | `bash crates/providers/imessage/tests/qa/run.sh` | exit 0; `QA PASSED`; chats=2 first Jane, messages m1,m2, auth authenticated | PASS | A6 |
| S2-telegram | Committed mock QA asserts seeded values | CLI+qa-fake backend | `bash crates/providers/telegram/tests/qa/run.sh` | exit 0; whoami Ada Lovelace 777000 @ada, chats 1001/1002, messages with photo | PASS | A7 |
| S2-whatsapp | Committed mock QA asserts seeded values | CLI+stub sidecar | `bash crates/providers/whatsapp/tests/qa/run.sh` | exit 0; chats Alice/Team Standup, send sent-1, real sidecar auth.status authenticated false (no skip) | PASS | A8 |
| S2-line | Committed mock QA asserts seeded values | CLI+line stub | `bash crates/providers/line/tests/qa/run.sh` | exit 0; `QA PASSED`; account u_self, chats Friend, send sent-1, members=1 | PASS | A9 |
| S2-instagram | Committed mock QA asserts seeded values | CLI+mock HTTP | `bash crates/providers/instagram/tests/qa/run.sh` | exit 0; whoami 4242, chats "Bob Lee", send ig-sent-1 | PASS | A10 |
| S3a | HTTP surface reaches a provider through its mock | HTTP (axum server) | `hyper-messenger-server --port 8799` + `curl -i http://127.0.0.1:8799/v1/providers/discord/chats` | `HTTP/1.1 200 OK`, `ok:true`, data[0].name "Test Guild", provider discord | PASS | A11, A13 |
| S3b | MCP stdio surface drives a provider tool | MCP (rmcp stdio) | `hyper-messenger-mcp` stdin: initialize + `tools/call list_chats {provider: mock}` | initialize serverInfo "hyper-messenger"; list_chats isError false, ok true, count 3 (#general, Team Standup, Alice) | PASS | A12 |
| S4 | No leftover mock processes/listeners | process/port state | `lsof -iTCP:<port> -sTCP:LISTEN` for 8771,8799,8811-8814,8831 + `pgrep` mock/server/mcp/sidecar | all ports 0 listeners; no stray processes | PASS | A14 |

## adversarialCases

| Scenario | Criterion | Adversarial class | Expected behavior | Observed | Verdict | artifactRefs |
|---|---|---|---|---|---|---|
| S2-telegrambot | Error mapping | Malformed non-JSON success body | PROTOCOL, ok:false | `ok:true malformed body -> PROTOCOL` | PASS | A2 |
| S2-telegrambot | Error mapping | Wrong token (401) | AUTH_FAILED, ok:false | `ok:true wrong token -> AUTH_FAILED` | PASS | A2 |
| S2-whatsappbot | Error mapping | Malformed JSON body | PROTOCOL | `ok:true malformed -> PROTOCOL` | PASS | A3 |
| S2-whatsappbot | Error mapping | Bad token (401) | AUTH_FAILED | `ok:true bad token -> AUTH_FAILED` | PASS | A3 |
| S2-whatsappbot | Capability contract | Unadvertised op `chats` | UNSUPPORTED | `ok:true chats -> UNSUPPORTED` | PASS | A3 |
| S2-slackbot | Error mapping | ok:true envelope missing `messages` | PROTOCOL + non-zero exit | `QA PASSED` incl. PROTOCOL + non-zero exit asserts | PASS | A4 |
| S2-slackbot | Error mapping | HTTP 200 with `invalid_auth` | AUTH_FAILED + non-zero exit | `QA PASSED` incl. AUTH_FAILED + non-zero exit asserts | PASS | A4 |
| S2-slack | Error mapping | Missing cookie -> HTTP 200 invalid_auth | AUTH_FAILED | `ok:true missing cookie mapped to AUTH_FAILED` | PASS | A5 |
| S2-imessage | Error mapping | Malformed stub output | PROTOCOL + non-zero exit | `QA PASSED` incl. PROTOCOL assert | PASS | A6 |
| S2-imessage | Permission boundary | Full Disk Access denied | AUTH_FAILED naming "Full Disk Access" | `QA PASSED` incl. FDA assert | PASS | A6 |
| S2-imessage | Config boundary | Invalid binary path | CONFIG + non-zero exit | `QA PASSED` incl. CONFIG assert | PASS | A6 |
| S2-imessage | Config boundary | Unset binary, none on PATH | CONFIG + non-zero exit | `QA PASSED` incl. CONFIG assert | PASS | A6 |
| S2-telegram | Error mapping | Malformed fixture | PROTOCOL | `ok:true malformed json -> PROTOCOL` | PASS | A7 |
| S2-telegram | Auth state | Closed session | AUTH_REQUIRED | `ok:true closed state -> AUTH_REQUIRED` | PASS | A7 |
| S2-telegram | Dependency boundary | No qa-fake, no TDLib lib | CONFIG, non-zero exit, no panic | `ok:true no library -> CONFIG, non-zero exit, no panic` | PASS | A7 |
| S2-whatsapp | Config boundary | Missing sidecar script | CONFIG + non-zero exit | `ok:true missing sidecar -> CONFIG` / `exit code 1` | PASS | A8 |
| S2-whatsapp | Network boundary | Hung sidecar never replies | NETWORK timeout | `ok:true hung -> ... timed out after 2s` | PASS | A8 |
| S2-whatsapp | Error mapping | Malformed sidecar output | PROTOCOL | `ok:true malformed -> PROTOCOL` | PASS | A8 |
| S2-whatsapp | Network boundary | Stale sidecar exits before reply | NETWORK | `ok:true stale -> NETWORK` | PASS | A8 |
| S2-line | Error mapping | Auth failure from stub | AUTH_FAILED + non-zero exit | `QA PASSED` incl. AUTH_FAILED assert | PASS | A9 |
| S2-line | Error mapping | Malformed stub output | PROTOCOL + non-zero exit | `QA PASSED` incl. PROTOCOL assert | PASS | A9 |
| S2-line | Auth state | No credentials | AUTH_REQUIRED + non-zero exit | `QA PASSED` incl. AUTH_REQUIRED assert | PASS | A9 |
| S2-instagram | Error mapping | Wrong session (401) | AUTH_FAILED | `ok:true 401 mapped to AUTH_FAILED` | PASS | A10 |

Adversarial classes not applicable to S1 (CLI listing has no external double), S3a (happy-path reachability), S3b (happy-path tool call): the change under test for those surfaces is presence/reachability, so no fault class is triggered; recorded here rather than as a fake PASS.

## artifactRefs

| id | kind | description | path |
|---|---|---|---|
| A1 | cli-transcript | `hyper-messenger providers` JSON output + exit code | .omo/evidence/ulw/01a12e84-9150-7a68-8410-b699d5ff978c/F3/s1-cli-providers.txt |
| A2 | qa-log | telegrambot run.sh full transcript + exit code | .omo/evidence/ulw/01a12e84-9150-7a68-8410-b699d5ff978c/F3/s2-telegrambot.log |
| A3 | qa-log | whatsappbot run.sh full transcript + exit code | .omo/evidence/ulw/01a12e84-9150-7a68-8410-b699d5ff978c/F3/s2-whatsappbot.log |
| A4 | qa-log | slackbot run.sh full transcript + exit code | .omo/evidence/ulw/01a12e84-9150-7a68-8410-b699d5ff978c/F3/s2-slackbot.log |
| A5 | qa-log | slack run.sh full transcript + exit code | .omo/evidence/ulw/01a12e84-9150-7a68-8410-b699d5ff978c/F3/s2-slack.log |
| A6 | qa-log | imessage run.sh full transcript + exit code | .omo/evidence/ulw/01a12e84-9150-7a68-8410-b699d5ff978c/F3/s2-imessage.log |
| A7 | qa-log | telegram run.sh full transcript + exit code | .omo/evidence/ulw/01a12e84-9150-7a68-8410-b699d5ff978c/F3/s2-telegram.log |
| A8 | qa-log | whatsapp run.sh full transcript + exit code | .omo/evidence/ulw/01a12e84-9150-7a68-8410-b699d5ff978c/F3/s2-whatsapp.log |
| A9 | qa-log | line run.sh full transcript + exit code | .omo/evidence/ulw/01a12e84-9150-7a68-8410-b699d5ff978c/F3/s2-line.log |
| A10 | qa-log | instagram run.sh full transcript + exit code | .omo/evidence/ulw/01a12e84-9150-7a68-8410-b699d5ff978c/F3/s2-instagram.log |
| A11 | http-dump | `curl -i` discord chats + health, with response headers | .omo/evidence/ulw/01a12e84-9150-7a68-8410-b699d5ff978c/F3/s3-http.txt |
| A12 | mcp-transcript | MCP stdio initialize + tools/call list_chats mock | .omo/evidence/ulw/01a12e84-9150-7a68-8410-b699d5ff978c/F3/s3-mcp.txt |
| A13 | server-log | hyper-messenger-server startup log | .omo/evidence/ulw/01a12e84-9150-7a68-8410-b699d5ff978c/F3/s3-server.log |
| A14 | cleanup-receipt | port/process sweep after all runs | .omo/evidence/ulw/01a12e84-9150-7a68-8410-b699d5ff978c/F3/s4-cleanup.txt |
| A15 | runner-scripts | exact harness used (temp files) | /tmp/f3_qa_runner.sh, /tmp/f3_scenario3.sh, /tmp/f3_mcp_driver.py, /tmp/f3_mock_discord.py |

## Summary

All 13 surface evidence rows PASS; all 23 triggered adversarial rows PASS. No skips: the whatsapp real sidecar step ran (node+npm present). No leftover listeners or mock processes after the runs.
