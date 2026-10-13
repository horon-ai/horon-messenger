#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../../../.." && pwd)"
QA_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PORT=8812
PHONE_ID="123456789012345"
RECIPIENT="+15551234567"
BIN="$ROOT/target/debug/horon-messenger"
CFG="$(mktemp -d)"
MOCK_PID=""

cleanup() {
  if [ -n "$MOCK_PID" ]; then
    kill "$MOCK_PID" 2>/dev/null || true
  fi
  wait 2>/dev/null || true
  rm -rf "$CFG"
  echo "cleanup: mock_graph pid=${MOCK_PID:-none} killed; temp config $CFG removed"
}
trap cleanup EXIT

echo "# building the CLI with the whatsappbot provider"
if ! (cd "$ROOT" && cargo build -p horon-messenger-cli --features horon-messenger-sdk/all -q); then
  echo "build failed"
  exit 1
fi

python3 "$QA_DIR/mock_graph.py" "$PORT" >/dev/null 2>&1 &
MOCK_PID=$!
for _ in $(seq 1 60); do
  if curl -s -o /dev/null "http://127.0.0.1:$PORT/$PHONE_ID" -H "Authorization: Bearer mock-token"; then
    break
  fi
  sleep 0.1
done

export HORON_MESSENGER_CONFIG_DIR="$CFG"
export HORON_MESSENGER_WHATSAPPBOT_API_BASE="http://127.0.0.1:$PORT"
export HORON_MESSENGER_WHATSAPPBOT_PHONE_NUMBER_ID="$PHONE_ID"
export HORON_MESSENGER_WHATSAPPBOT_ACCESS_TOKEN="mock-token"

echo "# send whatsappbot <phone> hi (happy): asserts the returned message id"
"$BIN" send whatsappbot "$RECIPIENT" "hi" | python3 -c 'import sys,json; d=json.load(sys.stdin); assert d["ok"] is True, d; assert d["data"]["accepted"] is True, d; assert d["data"]["message_id"]=="wamid.mock123", d; print("ok:true message_id=wamid.mock123")'

echo "# whoami whatsappbot (happy): returns the phone number id"
"$BIN" whoami whatsappbot | python3 -c 'import sys,json,os; d=json.load(sys.stdin); assert d["ok"] is True, d; assert d["data"]["account_id"]==os.environ["HORON_MESSENGER_WHATSAPPBOT_PHONE_NUMBER_ID"], d; print("ok:true account_id="+d["data"]["account_id"])'

echo "# adversarial: malformed JSON body -> PROTOCOL"
malformed="$("$BIN" send whatsappbot "$RECIPIENT" "__MALFORMED__" || true)"
echo "$malformed" | python3 -c 'import sys,json; d=json.load(sys.stdin); assert d["ok"] is False, d; assert d["error"]["code"]=="PROTOCOL", d; print("ok:true malformed -> PROTOCOL")'

echo "# failure: bad token -> 401 -> AUTH_FAILED"
bad="$(HORON_MESSENGER_WHATSAPPBOT_ACCESS_TOKEN="wrong-token" "$BIN" send whatsappbot "$RECIPIENT" "hi" || true)"
echo "$bad" | python3 -c 'import sys,json; d=json.load(sys.stdin); assert d["ok"] is False, d; assert d["error"]["code"]=="AUTH_FAILED", d; print("ok:true bad token -> AUTH_FAILED")'

echo "# failure: chats whatsappbot -> UNSUPPORTED (capability not advertised)"
chats="$("$BIN" chats whatsappbot || true)"
echo "$chats" | python3 -c 'import sys,json; d=json.load(sys.stdin); assert d["ok"] is False, d; assert d["error"]["code"]=="UNSUPPORTED", d; print("ok:true chats -> UNSUPPORTED")'

echo "whatsappbot qa done"
