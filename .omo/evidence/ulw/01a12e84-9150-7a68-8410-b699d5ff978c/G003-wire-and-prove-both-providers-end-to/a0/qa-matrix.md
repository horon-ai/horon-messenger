# Manual QA matrix — Discord + Channel Talk providers (frozen tree 324249f, HEAD 83dd9fb)

All proofs were run by the main session against a local mock HTTP server; no live
credentials were used. Mock servers: `qa/mock_discord.py` (port 8771),
`qa/mock_channeltalk.py` (port 8772).

| Criterion | Surface | Invocation | Verdict | Artifact |
|-----------|---------|-----------|---------|----------|
| G001 C1 Discord read | cli | `whoami/chats/messages discord` (mock 8771) | passed | discord/discord-read.txt |
| G001 C2 Discord send | cli | `send discord <channel> "qa hello"` | passed | discord/discord-send.txt |
| G001 C3 Discord error | cli | wrong token -> `chats discord` exit 1 AUTH_FAILED | passed | discord/discord-auth-error.txt |
| G001 C3 regression | cli | `cargo test/clippy/fmt --all-features` | passed | discord/discord-regression.txt |
| G002 C1 CT read | cli | `whoami/chats/messages channeltalk` (mock 8772) | passed | channeltalk/channeltalk-read.txt |
| G002 C2 CT send | cli | `send channeltalk ws1/group/g1 "qa hello ct"` | passed | channeltalk/channeltalk-send.txt |
| G002 C3 CT error | cli | wrong cookie -> `chats channeltalk` exit 1 AUTH_FAILED | passed | channeltalk/channeltalk-auth-error.txt |
| G002 C3 regression | cli | `cargo test/clippy/fmt --all-features` | passed | channeltalk/channeltalk-regression.txt |
| G003 C1 HTTP | http | `GET /v1/providers/{discord,channeltalk}/chats` -> 200 | passed | cross-surface/http.txt |
| G003 C2 MCP | cli | `tools/call list_chats {provider: discord/channeltalk}` | passed | cross-surface/mcp.txt |
| G003 C3 mock regression | http+cli | `/v1/providers/mock/chats` + MCP mock | passed | cross-surface/{http,mcp}.txt |

## Adversarial classes

- malformed input: `ChatRef::parse` rejects `ws1/g1`, `ws1/bad/g1`, `//` (unit test
  `chat_ref_rejects_malformed_input`). Discord `search_messages` without a guild id
  returns INVALID_INPUT.
- stale state: not applicable (providers are stateless per call).
- hung/long commands: not applicable (mock responses are immediate).
- flaky tests: not applicable (no timing-dependent tests; unit tests are pure).

## Outcome

Both providers are implemented, wired through the sdk to the CLI, MCP server and
HTTP server, and proven on real surfaces against mock APIs. The default mock
provider still answers. Full suite (`cargo test --workspace --all-features`),
clippy (`-D warnings`) and `fmt --check` are green at the frozen tree.
