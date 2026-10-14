#!/usr/bin/env bash
set -euo pipefail

usage() {
  echo "usage: bash scripts/perf/measure.sh --label <name>" >&2
  echo "  runs per measurement default to 30; override with PERF_N." >&2
}

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

LABEL=""
while [ $# -gt 0 ]; do
  case "$1" in
    --label)
      [ $# -ge 2 ] || { echo "measure.sh: --label requires a value" >&2; exit 2; }
      LABEL="$2"
      shift 2
      ;;
    --label=*)
      LABEL="${1#*=}"
      shift
      ;;
    -h | --help)
      usage
      exit 0
      ;;
    *)
      echo "measure.sh: unknown argument: $1" >&2
      usage
      exit 2
      ;;
  esac
done

[ -n "$LABEL" ] || { echo "measure.sh: --label <name> is required" >&2; usage; exit 2; }

N="${PERF_N:-30}"
case "$N" in
  '' | *[!0-9]*)
    echo "measure.sh: PERF_N must be a positive integer" >&2
    exit 2
    ;;
esac
[ "$N" -ge 1 ] || { echo "measure.sh: PERF_N must be >= 1" >&2; exit 2; }

CLI="$ROOT/target/release/horon-messenger"
MCP="$ROOT/target/release/horon-messenger-mcp"
SERVER="$ROOT/target/release/horon-messenger-server"

missing=0
for bin in "$CLI" "$MCP" "$SERVER"; do
  if [ ! -x "$bin" ]; then
    echo "measure.sh: missing release binary: $bin" >&2
    missing=1
  fi
done
if [ "$missing" -ne 0 ]; then
  echo "measure.sh: build the release tree first: cargo build --release" >&2
  exit 1
fi

CONFIG_DIR="${HORON_MESSENGER_CONFIG_DIR:-$ROOT/target/perf/config}"
if ! mkdir -p "$CONFIG_DIR" 2>/dev/null; then
  echo "measure.sh: cannot create the config directory: $CONFIG_DIR" >&2
  echo "measure.sh: set HORON_MESSENGER_CONFIG_DIR to a writable path" >&2
  exit 1
fi
if [ ! -w "$CONFIG_DIR" ]; then
  echo "measure.sh: the config directory is not writable: $CONFIG_DIR" >&2
  exit 1
fi
export HORON_MESSENGER_CONFIG_DIR="$CONFIG_DIR"

OUT_DIR="$ROOT/target/perf"
mkdir -p "$OUT_DIR"
OUT="$OUT_DIR/$LABEL.json"

now_ns() { python3 -c 'import time; print(time.perf_counter_ns())'; }

measure_loop() {
  local expect="$1"
  shift
  local start end i out
  start="$(now_ns)"
  for ((i = 0; i < N; i++)); do
    if ! out="$("$@")"; then
      echo "measure.sh: command failed: $*" >&2
      exit 1
    fi
    if [ -n "$expect" ]; then
      case "$out" in
        *"$expect"*) ;;
        *)
          echo "measure.sh: output missing '$expect' from: $*" >&2
          echo "$out" >&2
          exit 1
          ;;
      esac
    fi
  done
  end="$(now_ns)"
  python3 -c "print(round(($end - $start) / $N / 1e6, 4))"
}

P_PROVIDERS="$(measure_loop '"ok":true' "$CLI" providers)"
P_CHATS="$(measure_loop '"ok":true' "$CLI" chats mock)"
P_SEND="$(measure_loop '"ok":true' "$CLI" send mock general hi)"
FLOOR="$(measure_loop '' /usr/bin/true)"

free_port() {
  python3 -c 'import socket; s = socket.socket(); s.bind(("127.0.0.1", 0)); print(s.getsockname()[1]); s.close()'
}

PORT="$(free_port)"
BASE="http://127.0.0.1:$PORT"
SERVER_LOG="$OUT_DIR/server-$LABEL.log"
SERVER_PID=""

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
  SERVER_PID=""
}
trap cleanup EXIT

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
  echo "measure.sh: server did not become ready at $BASE/health" >&2
  cat "$SERVER_LOG" >&2 || true
  exit 1
fi

HTTP_URL="$BASE/v1/providers/mock/chats"
http_start="$(now_ns)"
for ((i = 0; i < N; i++)); do
  if ! body="$(curl -sf "$HTTP_URL")"; then
    echo "measure.sh: HTTP request failed (non-2xx): $HTTP_URL" >&2
    exit 1
  fi
  case "$body" in
    *'"ok":true'*) ;;
    *)
      echo "measure.sh: HTTP response is not ok:true: $HTTP_URL" >&2
      echo "$body" >&2
      exit 1
      ;;
  esac
done
http_end="$(now_ns)"
HTTP_MS="$(python3 -c "print(round(($http_end - $http_start) / $N / 1e6, 4))")"

kill "$SERVER_PID" 2>/dev/null || true
disown "$SERVER_PID" 2>/dev/null || true
for _ in $(seq 1 50); do
  kill -0 "$SERVER_PID" 2>/dev/null || break
  sleep 0.1
done
if kill -0 "$SERVER_PID" 2>/dev/null; then
  echo "measure.sh: server $SERVER_PID did not exit" >&2
  kill -9 "$SERVER_PID" 2>/dev/null || true
  exit 1
fi
wait "$SERVER_PID" 2>/dev/null || true
SERVER_PID=""

size_of() { wc -c < "$1" | tr -d ' '; }
SZ_CLI="$(size_of "$CLI")"
SZ_MCP="$(size_of "$MCP")"
SZ_SERVER="$(size_of "$SERVER")"

python3 - "$OUT" "$LABEL" "$N" "$P_PROVIDERS" "$P_CHATS" "$P_SEND" "$FLOOR" "$HTTP_MS" \
  "$SZ_CLI" "$SZ_MCP" "$SZ_SERVER" <<'PY'
import json
import sys

(out, label, n, providers, chats, send, floor, http,
 cli, mcp, server) = sys.argv[1:12]

data = {
    "label": label,
    "n": int(n),
    "startup_ms": {
        "providers": float(providers),
        "chats": float(chats),
        "send": float(send),
    },
    "spawn_floor_ms": float(floor),
    "http_ms": float(http),
    "binaries": {
        "horon-messenger": int(cli),
        "horon-messenger-mcp": int(mcp),
        "horon-messenger-server": int(server),
    },
}
with open(out, "w", encoding="utf-8") as handle:
    json.dump(data, handle, indent=2, sort_keys=True)
    handle.write("\n")

rows = [
    ("startup_ms.providers", data["startup_ms"]["providers"]),
    ("startup_ms.chats", data["startup_ms"]["chats"]),
    ("startup_ms.send", data["startup_ms"]["send"]),
    ("spawn_floor_ms", data["spawn_floor_ms"]),
    ("http_ms", data["http_ms"]),
    ("binaries.horon-messenger", data["binaries"]["horon-messenger"]),
    ("binaries.horon-messenger-mcp", data["binaries"]["horon-messenger-mcp"]),
    ("binaries.horon-messenger-server", data["binaries"]["horon-messenger-server"]),
]
width = max(len(name) for name, _ in rows)
print(f"# perf measurement: {label} (n={data['n']})")
print(f"# written to {out}")
print(f"{'metric':<{width}}  {'value':>14}")
print("-" * (width + 16))
for name, value in rows:
    print(f"{name:<{width}}  {value:>14}")
PY
