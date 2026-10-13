#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../../../.." && pwd)"
QA_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PORT=8831
CHAT="340282366841710300949128130000000000001"
BIN="$ROOT/target/debug/hyper-messenger"
CFG="$(mktemp -d)"
MOCK_PID=""

cleanup() {
  if [ -n "$MOCK_PID" ]; then
    kill "$MOCK_PID" 2>/dev/null || true
  fi
  wait 2>/dev/null || true
  rm -rf "$CFG"
  echo "cleanup: mock_instagram pid=${MOCK_PID:-none} killed; temp config $CFG removed"
}
trap cleanup EXIT

echo "# building the CLI with all features"
if ! (cd "$ROOT" && cargo build -p hyper-messenger-cli --features hyper-messenger-sdk/all -q); then
  echo "build failed"
  exit 1
fi

python3 "$QA_DIR/mock_instagram.py" "$PORT" >/dev/null 2>&1 &
MOCK_PID=$!
for _ in $(seq 1 60); do
  if curl -s -o /dev/null "http://127.0.0.1:$PORT/api/v1/direct_v2/inbox/?limit=1"; then
    break
  fi
  sleep 0.1
done

export HYPER_MESSENGER_CONFIG_DIR="$CFG"
export HYPER_MESSENGER_INSTAGRAM_API_BASE="http://127.0.0.1:$PORT/api/v1"
export HYPER_MESSENGER_INSTAGRAM_SESSIONID="mock-session-id"
export HYPER_MESSENGER_INSTAGRAM_DS_USER_ID="4242"
export HYPER_MESSENGER_INSTAGRAM_DEVICE_ID="android-mock-device"

echo "# whoami instagram"
"$BIN" whoami instagram | python3 -c 'import sys,json; d=json.load(sys.stdin); assert d["ok"] is True, d; assert d["data"]["account_id"]=="4242", d; print("ok:true whoami account_id=4242")'

echo "# chats instagram"
"$BIN" chats instagram | python3 -c 'import sys,json; d=json.load(sys.stdin); assert d["ok"] is True, d; assert d["data"][0]["name"]=="Bob Lee", d; print("ok:true chats name=Bob Lee")'

echo "# messages instagram"
"$BIN" messages instagram "$CHAT" | python3 -c 'import sys,json; d=json.load(sys.stdin); assert d["ok"] is True, d; assert d["data"]["messages"][0]["text"]=="seeded hello from mock instagram", d; print("ok:true messages seeded chronological first")'

echo "# send instagram"
"$BIN" send instagram "$CHAT" "hi" | python3 -c 'import sys,json; d=json.load(sys.stdin); assert d["ok"] is True, d; assert d["data"]["accepted"] is True, d; assert d["data"]["message_id"]=="ig-sent-1", d; print("ok:true send accepted id=ig-sent-1")'

echo "# adversarial: wrong session -> HTTP 401 -> AUTH_FAILED"
export HYPER_MESSENGER_INSTAGRAM_SESSIONID="wrong"
bad="$("$BIN" chats instagram || true)"
echo "$bad" | python3 -c 'import sys,json; d=json.load(sys.stdin); assert d["ok"] is False, d; assert d["error"]["code"]=="AUTH_FAILED", d; print("ok:true 401 mapped to AUTH_FAILED")'

echo "instagram qa done"
