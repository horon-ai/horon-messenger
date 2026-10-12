# Gate review — Discord + Channel Talk provider ports

Reviewer: `category:deep-high` (independent verification worker)
Tree verified: `324249f` == HEAD `83dd9fb`
Verdict: **RECOMMENDATION: APPROVE** — no hard blockers.

## Findings

1. **Discord client — PASS.** Paths, headers and ack match the reference
   (client.rs:230-304 ↔ ref client.ts:278-341). Deviation: `Sec-Ch-Ua*` headers
   omitted; timezone fixed to UTC (client.rs:66-72 ↔ super-properties.ts:29-42).
2. **Channel Talk client — PASS.** Paths, cookie and payload unwrap match the
   reference (client.rs:248-325 ↔ ref client.ts:81-258); error `errors[0].message`
   matches.
3. **Provider/AuthProvider mapping — PASS.** Advertised capabilities equal the
   implemented overrides; no default method re-added (discord lib.rs:109-119,
   250-257; ct lib.rs:129-138).
4. **Bugs / panics — PASS.** No `unwrap`/`panic` in library code (client.rs:347,
   356,374 are test-only).
5. **Wiring — PASS.** Registered in sdk lib.rs:144-161; reachable from server/mcp
   via the registry; http and mcp evidence confirmed.
6. **Evidence integrity — PASS (with caveat).** AUTH_FAILED/read/send reproduce.
   quality-gate.json evidence was a placeholder and gate-review.md was absent
   (both fixed in this finalization). search_messages and mark_read are not
   covered by any evidence artifact.
7. **Test gaps — FAIL (minor).** No automated HTTP-path test; only 3 unit tests
   per crate.
8. **Irreversible / risky change — none.**

## Non-blocking observations

- Rate-limit buckets are not shared across operations: `client()` builds a fresh
  `DiscordClient`/`ChannelTalkClient` per call, so bucket state does not carry
  between calls (discord client.rs:38-43,66-71; ct client.rs:80-85,89). The
  429 `Retry-After` path still protects within a call.
- Discord search index-not-ready (`110000`) retry from the reference is not
  ported (lib.rs:177-213).
