#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../../../.." && pwd)"
QA_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
BIN="$ROOT/target/debug/horon-messenger"
STUB="$QA_DIR/stub_sidecar.mjs"
SIDECAR_DIR="$ROOT/crates/providers/whatsapp/sidecar"
CFG="$(mktemp -d)"

cleanup() {
  rm -rf "$CFG"
  echo "cleanup: temp config $CFG removed; stub sidecars stopped by process exit"
}
trap cleanup EXIT

echo "# building the CLI with every sdk feature"
(cd "$ROOT" && cargo build -p horon-messenger-cli --features horon-messenger-sdk/all -q)

export HORON_MESSENGER_CONFIG_DIR="$CFG"

stub_run() {
  local mode="$1"
  shift
  WHATSAPP_STUB_MODE="$mode" \
    HORON_MESSENGER_WHATSAPP_SIDECAR="$STUB" \
    HORON_MESSENGER_WHATSAPP_TIMEOUT_MS=2000 \
    "$@"
}

echo "# chats whatsapp against the stub sidecar"
stub_run normal "$BIN" chats whatsapp | python3 -c 'import sys,json; d=json.load(sys.stdin); assert d["ok"] is True, d; names=[c["name"] for c in d["data"]]; assert "Alice" in names, names; print("ok:true chats ->", names)'

echo "# send whatsapp against the stub sidecar"
stub_run normal "$BIN" send whatsapp 12025551234@s.whatsapp.net "hi" | python3 -c 'import sys,json; d=json.load(sys.stdin); assert d["ok"] is True, d; assert d["data"]["accepted"] is True, d; assert d["data"]["message_id"]=="sent-1", d; print("ok:true send -> message_id", d["data"]["message_id"])'

echo "# failure: a missing sidecar script is a typed CONFIG error"
set +e
missing_out="$(HORON_MESSENGER_WHATSAPP_SIDECAR="$CFG/absent.mjs" "$BIN" chats whatsapp 2>/dev/null)"
missing_code=$?
set -e
echo "$missing_out" | python3 -c 'import sys,json; d=json.load(sys.stdin); assert d["ok"] is False, d; assert d["error"]["code"]=="CONFIG", d; print("ok:true missing sidecar ->", d["error"]["code"])'
if [ "$missing_code" -eq 0 ]; then
  echo "FAIL: expected a non-zero exit for a missing sidecar"
  exit 1
fi
echo "ok:true missing sidecar exit code $missing_code"

echo "# adversarial: hung sidecar never replies -> NETWORK timeout"
set +e
hung_out="$(stub_run hung "$BIN" chats whatsapp 2>/dev/null)"
hung_code=$?
set -e
echo "$hung_out" | python3 -c 'import sys,json; d=json.load(sys.stdin); assert d["ok"] is False, d; assert d["error"]["code"]=="NETWORK", d; assert "timed out" in d["error"]["message"], d; print("ok:true hung ->", d["error"]["message"])'
if [ "$hung_code" -eq 0 ]; then
  echo "FAIL: expected a non-zero exit for a hung sidecar"
  exit 1
fi

echo "# adversarial: malformed sidecar output -> PROTOCOL"
set +e
malformed_out="$(stub_run malformed "$BIN" chats whatsapp 2>/dev/null)"
set -e
echo "$malformed_out" | python3 -c 'import sys,json; d=json.load(sys.stdin); assert d["ok"] is False, d; assert d["error"]["code"]=="PROTOCOL", d; print("ok:true malformed ->", d["error"]["code"])'

echo "# adversarial: stale sidecar exits before replying -> NETWORK"
set +e
stale_out="$(stub_run stale "$BIN" chats whatsapp 2>/dev/null)"
set -e
echo "$stale_out" | python3 -c 'import sys,json; d=json.load(sys.stdin); assert d["ok"] is False, d; assert d["error"]["code"]=="NETWORK", d; print("ok:true stale ->", d["error"]["code"])'

echo "# real sidecar: auth.status over the JSON-RPC bridge (needs node + npm)"
if command -v node >/dev/null 2>&1 && command -v npm >/dev/null 2>&1; then
  if (cd "$SIDECAR_DIR" && npm ci --silent >/dev/null 2>&1); then
    auth_line='{"jsonrpc":"2.0","id":1,"method":"auth.status","params":{"auth_dir":"'"$CFG"'/whatsapp/accounts/default"}}'
    set +o pipefail
    auth_out="$(printf '%s\n' "$auth_line" | node "$SIDECAR_DIR/index.mjs" 2>/dev/null)"
    set -o pipefail
    printf '%s\n' "$auth_out" | python3 -c 'import sys,json; d=json.loads(sys.stdin.readline()); assert d.get("id")==1, d; assert d["result"]["authenticated"] is False, d; print("ok:true real sidecar auth.status -> authenticated false")'
  else
    echo "skip: npm ci failed (offline?); the real sidecar was not exercised"
  fi
else
  echo "skip: node or npm not present; the real sidecar was not exercised"
fi

echo "whatsapp qa done"
