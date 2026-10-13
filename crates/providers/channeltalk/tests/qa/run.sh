#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../../../.." && pwd)"
QA_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PORT=8772
CHAT="ws1/group/g1"
BIN="$ROOT/target/debug/horon-messenger"
CFG="$(mktemp -d)"
MOCK_PID=""

cleanup() {
  if [ -n "$MOCK_PID" ]; then
    kill "$MOCK_PID" 2>/dev/null || true
  fi
  wait 2>/dev/null || true
  rm -rf "$CFG"
  echo "cleanup: mock_channeltalk pid=${MOCK_PID:-none} killed; temp config $CFG removed"
}
trap cleanup EXIT

echo "# building the CLI with all features"
if ! (cd "$ROOT" && cargo build --workspace --all-features -q); then
  echo "build failed"
  exit 1
fi

python3 "$QA_DIR/mock_channeltalk.py" "$PORT" >/dev/null 2>&1 &
MOCK_PID=$!
for _ in $(seq 1 60); do
  if curl -s -o /dev/null "http://127.0.0.1:$PORT/__stats"; then
    break
  fi
  sleep 0.1
done

export HORON_MESSENGER_CONFIG_DIR="$CFG"
export HORON_MESSENGER_CHANNELTALK_API_BASE="http://127.0.0.1:$PORT"
export HORON_MESSENGER_CHANNELTALK_ACCOUNT_COOKIE="mock-account"

echo "# whoami channeltalk"
"$BIN" whoami channeltalk | python3 -c 'import sys,json; d=json.load(sys.stdin); assert d["ok"] is True, d; assert d["data"]["display_name"]=="Operator", d; print("ok:true whoami Operator")'

echo "# chats channeltalk"
"$BIN" chats channeltalk | python3 -c 'import sys,json; d=json.load(sys.stdin); assert d["ok"] is True, d; assert d["data"][0]["name"]=="Test Workspace", d; print("ok:true chats Test Workspace")'

echo "# messages channeltalk"
"$BIN" messages channeltalk "$CHAT" | python3 -c 'import sys,json; d=json.load(sys.stdin); assert d["ok"] is True, d; assert d["data"]["messages"][0]["text"]=="seeded hello from mock channel talk", d; print("ok:true messages seeded")'

echo "# send channeltalk"
"$BIN" send channeltalk "$CHAT" "qa hello ct" | python3 -c 'import sys,json; d=json.load(sys.stdin); assert d["ok"] is True, d; assert d["data"]["accepted"] is True, d; print("ok:true send accepted")'

echo "# adversarial: mid-response disconnect on send must not be retried"
curl -s "http://127.0.0.1:$PORT/__reset" >/dev/null
drop_out="$("$BIN" send channeltalk "$CHAT" "__DROP__" || true)"
echo "$drop_out" | python3 -c 'import sys,json; d=json.load(sys.stdin); assert d["ok"] is False, d; assert d["error"]["code"]=="NETWORK", d; assert "unknown" in d["error"]["message"], d; print("ok:true drop not retried, outcome unknown")'
STATS_KEY="POST /desk/channels/ws1/groups/g1/messages" python3 -c 'import sys,json,os; d=json.load(sys.stdin); n=d.get(os.environ["STATS_KEY"],0); assert n==1, (n,d); print("ok:true exactly one POST after mid-response disconnect")' < <(curl -s "http://127.0.0.1:$PORT/__stats")

echo "channeltalk qa done"
