#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../../../.." && pwd)"
QA_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PORT=8814
CHANNEL="C1"
BIN="$ROOT/target/debug/horon-messenger"
CFG="$(mktemp -d)"
MOCK_PID=""

cleanup() {
  if [ -n "$MOCK_PID" ]; then
    kill "$MOCK_PID" 2>/dev/null || true
  fi
  wait 2>/dev/null || true
  rm -rf "$CFG"
  echo "cleanup: mock_slack pid=${MOCK_PID:-none} killed; temp config $CFG removed"
}
trap cleanup EXIT

echo "# building the CLI with all features"
if ! (cd "$ROOT" && cargo build -p horon-messenger-cli --features horon-messenger-sdk/all -q); then
  echo "build failed"
  exit 1
fi

python3 "$QA_DIR/mock_slack.py" "$PORT" >/dev/null 2>&1 &
MOCK_PID=$!
for _ in $(seq 1 60); do
  if curl -s -o /dev/null "http://127.0.0.1:$PORT/auth.test"; then
    break
  fi
  sleep 0.1
done

export HORON_MESSENGER_CONFIG_DIR="$CFG"
export HORON_MESSENGER_SLACK_API_BASE="http://127.0.0.1:$PORT"
export HORON_MESSENGER_SLACK_TOKEN="xoxc-mock-token"
export HORON_MESSENGER_SLACK_COOKIE="mock-cookie"

echo "# whoami slack"
"$BIN" whoami slack | python3 -c 'import sys,json; d=json.load(sys.stdin); assert d["ok"] is True, d; assert d["data"]["username"]=="alice", d; print("ok:true whoami alice")'

echo "# chats slack"
"$BIN" chats slack | python3 -c 'import sys,json; d=json.load(sys.stdin); assert d["ok"] is True, d; assert d["data"][0]["name"]=="general", d; print("ok:true chats general")'

echo "# messages slack"
"$BIN" messages slack "$CHANNEL" | python3 -c 'import sys,json; d=json.load(sys.stdin); assert d["ok"] is True, d; assert d["data"]["messages"][0]["text"]=="seeded hello from mock slack", d; print("ok:true messages seeded")'

echo "# send slack"
"$BIN" send slack "$CHANNEL" "hi" | python3 -c 'import sys,json; d=json.load(sys.stdin); assert d["ok"] is True, d; assert d["data"]["accepted"] is True, d; print("ok:true send accepted")'

echo "# adversarial: missing cookie -> ok:false invalid_auth at HTTP 200 -> AUTH_FAILED"
unset HORON_MESSENGER_SLACK_COOKIE
missing_out="$("$BIN" whoami slack || true)"
echo "$missing_out" | python3 -c 'import sys,json; d=json.load(sys.stdin); assert d["ok"] is False, d; assert d["error"]["code"]=="AUTH_FAILED", d; print("ok:true missing cookie mapped to AUTH_FAILED")'

echo "slack qa done"
