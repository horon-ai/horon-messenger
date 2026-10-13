#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../../../.." && pwd)"
QA_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PORT=8811
CHAT="42"
BIN="$ROOT/target/debug/hyper-messenger"
CFG="$(mktemp -d)"
MOCK_PID=""

cleanup() {
  if [ -n "$MOCK_PID" ]; then
    kill "$MOCK_PID" 2>/dev/null || true
  fi
  wait 2>/dev/null || true
  rm -rf "$CFG"
  echo "cleanup: mock_telegrambot pid=${MOCK_PID:-none} killed; temp config $CFG removed"
}
trap cleanup EXIT

echo "# building the CLI with all features"
if ! (cd "$ROOT" && cargo build -p hyper-messenger-cli --features hyper-messenger-sdk/all -q); then
  echo "build failed"
  exit 1
fi

python3 "$QA_DIR/mock_telegrambot.py" "$PORT" >/dev/null 2>&1 &
MOCK_PID=$!
for _ in $(seq 1 60); do
  if curl -s -o /dev/null "http://127.0.0.1:$PORT/__stats"; then
    break
  fi
  sleep 0.1
done

export HYPER_MESSENGER_CONFIG_DIR="$CFG"
export HYPER_MESSENGER_TELEGRAMBOT_API_BASE="http://127.0.0.1:$PORT"
export HYPER_MESSENGER_TELEGRAMBOT_TOKEN="mock"

echo "# whoami telegrambot"
"$BIN" whoami telegrambot | python3 -c 'import sys,json; d=json.load(sys.stdin); assert d["ok"] is True, d; assert d["data"]["account_id"]=="123456789", d; assert d["data"]["username"]=="mock_bot", d; print("ok:true whoami mock_bot 123456789")'

echo "# chats telegrambot"
"$BIN" chats telegrambot | python3 -c 'import sys,json; d=json.load(sys.stdin); assert d["ok"] is True, d; assert d["data"][0]["id"]=="42", d; assert d["data"][0]["name"]=="Ada Lovelace", d; print("ok:true chats Ada Lovelace")'

echo "# messages telegrambot"
"$BIN" messages telegrambot "$CHAT" | python3 -c 'import sys,json; d=json.load(sys.stdin); assert d["ok"] is True, d; assert d["data"]["messages"][0]["id"]=="500", d; assert d["data"]["messages"][0]["text"]=="seeded hello from mock telegram bot", d; print("ok:true messages seeded")'

echo "# members telegrambot"
"$BIN" members telegrambot "$CHAT" | python3 -c 'import sys,json; d=json.load(sys.stdin); assert d["ok"] is True, d; assert d["data"][0]["display_name"]=="Ada Lovelace", d; print("ok:true members Ada Lovelace")'

echo "# send telegrambot"
"$BIN" send telegrambot "$CHAT" "qa hello from mock" | python3 -c 'import sys,json; d=json.load(sys.stdin); assert d["ok"] is True, d; assert d["data"]["accepted"] is True, d; assert d["data"]["message_id"]=="9001", d; print("ok:true send created message_id 9001")'

echo "# adversarial: non-JSON success body maps to PROTOCOL"
bad_out="$("$BIN" send telegrambot "$CHAT" "__BADJSON__" || true)"
echo "$bad_out" | python3 -c 'import sys,json; d=json.load(sys.stdin); assert d["ok"] is False, d; assert d["error"]["code"]=="PROTOCOL", d; print("ok:true malformed body -> PROTOCOL")'

echo "# adversarial: wrong token maps ok:false 401 to AUTH_FAILED"
wrong_out="$(HYPER_MESSENGER_TELEGRAMBOT_TOKEN="wrong" "$BIN" whoami telegrambot || true)"
echo "$wrong_out" | python3 -c 'import sys,json; d=json.load(sys.stdin); assert d["ok"] is False, d; assert d["error"]["code"]=="AUTH_FAILED", d; print("ok:true wrong token -> AUTH_FAILED")'

echo "telegrambot qa done"
