#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

EVIDENCE="${EVIDENCE:-.omo/evidence/surface-verify-perf/t4-cross-surface.txt}"
mkdir -p "$(dirname "$EVIDENCE")"
: >"$EVIDENCE"

log() { printf '%s\n' "$*" | tee -a "$EVIDENCE"; }
fail() {
  printf 'FAIL: %s\n' "$*" | tee -a "$EVIDENCE" >&2
  exit 1
}

WORK="$(mktemp -d)"
SERVER_PID=""
SERVER_LOG="$WORK/server.log"

cleanup() {
  if [ -n "$SERVER_PID" ] && kill -0 "$SERVER_PID" 2>/dev/null; then
    kill "$SERVER_PID" 2>/dev/null || true
    disown "$SERVER_PID" 2>/dev/null || true
    for _ in $(seq 1 50); do
      kill -0 "$SERVER_PID" 2>/dev/null || break
      sleep 0.1
    done
    if kill -0 "$SERVER_PID" 2>/dev/null; then
      kill -9 "$SERVER_PID" 2>/dev/null || true
    fi
    wait "$SERVER_PID" 2>/dev/null || true
  fi
  rm -rf "$WORK"
}
trap cleanup EXIT

if [ -n "${HORON_MESSENGER_CONFIG_DIR:-}" ]; then
  CONFIG_DIR="$HORON_MESSENGER_CONFIG_DIR"
else
  CONFIG_DIR="$WORK/config"
fi
if ! mkdir -p "$CONFIG_DIR" 2>/dev/null; then
  fail "cannot create the config directory: $CONFIG_DIR"
fi
[ -w "$CONFIG_DIR" ] || fail "the config directory is not writable: $CONFIG_DIR"
export HORON_MESSENGER_CONFIG_DIR="$CONFIG_DIR"

CLI="$ROOT/target/release/horon-messenger"
MCP="$ROOT/target/release/horon-messenger-mcp"
SERVER="$ROOT/target/release/horon-messenger-server"

log "# cross-surface QA"
log "# config dir: $CONFIG_DIR"
log ""

log "== build (release) =="
if ! cargo build --release --workspace >>"$EVIDENCE" 2>&1; then
  fail "cargo build --release --workspace failed"
fi
for bin in "$CLI" "$MCP" "$SERVER"; do
  [ -x "$bin" ] || fail "missing release binary: $bin"
done
log "build ok"
log ""

log "== CLI =="
cli_check() {
  local label="$1" file="$2"
  shift 2
  local out
  if ! out="$("$CLI" "$@" 2>"$WORK/cli.err")"; then
    cat "$WORK/cli.err" >>"$EVIDENCE" || true
    fail "CLI $label exited non-zero"
  fi
  printf '%s' "$out" >"$file"
  log "cli $label -> $(printf '%s' "$out" | head -c 200)"
}

cli_check "providers" "$WORK/cli-providers.json" providers
cli_check "chats-mock" "$WORK/cli-chats.json" chats mock
cli_check "send-mock" "$WORK/cli-send.json" send mock general hi

python3 - "$WORK/cli-providers.json" "$WORK/cli-chats.json" "$WORK/cli-send.json" <<'PY' || fail "CLI envelopes invalid"
import json
import sys

providers = json.load(open(sys.argv[1], encoding="utf-8"))
chats = json.load(open(sys.argv[2], encoding="utf-8"))
send = json.load(open(sys.argv[3], encoding="utf-8"))

assert providers["ok"] is True, providers
assert any(p["kind"] == "mock" for p in providers["data"]), providers
assert chats["ok"] is True, chats
assert len(chats["data"]) == 3, chats
assert send["ok"] is True, send
assert send["data"]["accepted"] is True, send
PY
log "CLI: providers/chats(3)/send all ok:true"
log ""

log "== SDK (cargo test -p horon-messenger-sdk) =="
if ! cargo test -p horon-messenger-sdk >>"$EVIDENCE" 2>&1; then
  fail "cargo test -p horon-messenger-sdk failed"
fi
log "SDK: cargo test passed"
log ""

log "== MCP (stdio) =="
cat >"$WORK/mcp_probe.py" <<'PY'
import json
import os
import select
import subprocess
import sys
import time

binary, config_dir, out_path = sys.argv[1], sys.argv[2], sys.argv[3]

env = dict(os.environ)
env["HORON_MESSENGER_CONFIG_DIR"] = config_dir
env["RUST_LOG"] = "error"

proc = subprocess.Popen(
    [binary],
    stdin=subprocess.PIPE,
    stdout=subprocess.PIPE,
    stderr=subprocess.DEVNULL,
    env=env,
)

frames = [
    {
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize",
        "params": {
            "protocolVersion": "2024-11-05",
            "capabilities": {},
            "clientInfo": {"name": "qa", "version": "0.0.0"},
        },
    },
    {"jsonrpc": "2.0", "method": "notifications/initialized"},
    {
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/call",
        "params": {"name": "list_providers", "arguments": {}},
    },
]
payload = "".join(json.dumps(frame) + "\n" for frame in frames).encode("utf-8")

try:
    proc.stdin.write(payload)
    proc.stdin.flush()
except BrokenPipeError:
    pass

fd = proc.stdout.fileno()
os.set_blocking(fd, False)
buf = b""
result = None
deadline = time.time() + 30
while time.time() < deadline:
    ready, _, _ = select.select([fd], [], [], 0.5)
    if not ready:
        continue
    try:
        chunk = os.read(fd, 65536)
    except BlockingIOError:
        continue
    if not chunk:
        break
    buf += chunk
    while b"\n" in buf:
        line, buf = buf.split(b"\n", 1)
        line = line.strip()
        if not line:
            continue
        try:
            obj = json.loads(line)
        except json.JSONDecodeError:
            continue
        if obj.get("id") == 2:
            result = obj
            break
    if result is not None:
        break

proc.terminate()
try:
    proc.wait(timeout=5)
except subprocess.TimeoutExpired:
    proc.kill()
    proc.wait()

if result is None:
    print("MCP: no tools/call response", file=sys.stderr)
    sys.exit(1)

structured = result.get("result", {}).get("structuredContent")
if structured is None:
    print(f"MCP: response has no structuredContent: {result}", file=sys.stderr)
    sys.exit(1)

with open(out_path, "w", encoding="utf-8") as handle:
    json.dump(structured, handle, sort_keys=True)
    handle.write("\n")
PY
if ! python3 "$WORK/mcp_probe.py" "$MCP" "$CONFIG_DIR" "$WORK/mcp-envelope.json" >>"$EVIDENCE" 2>&1; then
  fail "MCP stdio probe failed"
fi
python3 - "$WORK/mcp-envelope.json" <<'PY' || fail "MCP list_providers envelope invalid"
import json
import sys

env = json.load(open(sys.argv[1], encoding="utf-8"))
assert env["ok"] is True, env
assert any(p["kind"] == "mock" for p in env["data"]), env
PY
log "MCP: list_providers names mock"
log ""

log "== HTTP server =="
free_port() {
  python3 -c 'import socket; s = socket.socket(); s.bind(("127.0.0.1", 0)); print(s.getsockname()[1]); s.close()'
}
PORT="$(free_port)"
BASE="http://127.0.0.1:$PORT"

"$SERVER" --host 127.0.0.1 --port "$PORT" >"$SERVER_LOG" 2>&1 &
SERVER_PID=$!

ready=0
for _ in $(seq 1 50); do
  code="$(curl -s -o /dev/null -w '%{http_code}' "$BASE/health" 2>/dev/null || echo 000)"
  if [ "$code" = "200" ]; then
    ready=1
    break
  fi
  sleep 0.1
done
if [ "$ready" -ne 1 ]; then
  cat "$SERVER_LOG" >>"$EVIDENCE" || true
  fail "server did not become ready at $BASE/health"
fi

health_code="$(curl -s -o "$WORK/http-health.json" -w '%{http_code}' "$BASE/health")"
[ "$health_code" = "200" ] || fail "/health returned $health_code"
chats_code="$(curl -s -o "$WORK/http-chats.json" -w '%{http_code}' "$BASE/v1/providers/mock/chats")"
[ "$chats_code" = "200" ] || fail "/v1/providers/mock/chats returned $chats_code"

python3 - "$WORK/http-health.json" "$WORK/http-chats.json" <<'PY' || fail "HTTP envelopes invalid"
import json
import sys

health = json.load(open(sys.argv[1], encoding="utf-8"))
chats = json.load(open(sys.argv[2], encoding="utf-8"))
assert health["ok"] is True, health
assert chats["ok"] is True, chats
assert len(chats["data"]) == 3, chats
PY
log "health 200, /v1/providers/mock/chats 200 with 3 chats"

kill "$SERVER_PID" 2>/dev/null || true
disown "$SERVER_PID" 2>/dev/null || true
for _ in $(seq 1 50); do
  kill -0 "$SERVER_PID" 2>/dev/null || break
  sleep 0.1
done
if kill -0 "$SERVER_PID" 2>/dev/null; then
  kill -9 "$SERVER_PID" 2>/dev/null || true
fi
wait "$SERVER_PID" 2>/dev/null || true
kill -0 "$SERVER_PID" 2>/dev/null && fail "server $SERVER_PID is still running"
SERVER_PID=""
log "server stopped"
log ""

log "== Envelope shape (identical top-level keys, meta.generated_at normalized) =="
python3 - "$WORK/cli-providers.json" "$WORK/mcp-envelope.json" "$WORK/http-chats.json" <<'PY' || fail "Envelope shapes differ"
import json
import sys

top_level = {}
for path in sys.argv[1:]:
    data = json.load(open(path, encoding="utf-8"))
    meta = data.get("meta")
    if isinstance(meta, dict):
        meta.pop("generated_at", None)
    top_level[path] = sorted(data.keys())
    print(f"  {path}: top-level={top_level[path]} meta={sorted((meta or {}).keys())}")

expected = ["data", "meta", "ok"]
for path, keys in top_level.items():
    assert keys == expected, f"{path}: top-level keys {keys} != {expected}"
PY
log "shape: all surfaces share top-level keys ['data', 'meta', 'ok']"
log ""

log "PASS: cross-surface QA complete"
