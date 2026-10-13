#!/usr/bin/env bash
set -euo pipefail

HERE="$(cd "$(dirname "$0")" && pwd)"
ROOT="$(cd "$HERE/../../../../.." && pwd)"
BIN="$ROOT/target/debug/horon-messenger"
PORT=8813
BASE="http://127.0.0.1:$PORT"

export HORON_MESSENGER_SLACKBOT_API_BASE="$BASE"
export HORON_MESSENGER_SLACKBOT_TOKEN="xoxb-test-token"
export HORON_MESSENGER_CONFIG_DIR="$(mktemp -d)"

echo "== build cli =="
cargo build -p horon-messenger-cli --features horon-messenger-sdk/all >/dev/null

echo "== start mock slack on $PORT =="
python3 "$HERE/mock_slack.py" "$PORT" &
MOCK_PID=$!
cleanup() {
  kill "$MOCK_PID" 2>/dev/null || true
  wait "$MOCK_PID" 2>/dev/null || true
  rm -rf "$HORON_MESSENGER_CONFIG_DIR"
  echo "cleanup: mock pid $MOCK_PID stopped, config dir removed"
}
trap cleanup EXIT

for _ in $(seq 1 50); do
  if curl -sf -X POST "$BASE/__mode/ping" >/dev/null 2>&1; then break; fi
  sleep 0.1
done

fail=0

echo "== whoami slackbot =="
out="$("$BIN" whoami slackbot)"
echo "$out"
echo "$out" | grep -q '"ok":true' || { echo "ASSERT FAILED: whoami ok:true"; fail=1; }

echo "== chats slackbot =="
out="$("$BIN" chats slackbot)"
echo "$out"
echo "$out" | grep -q '"ok":true' || { echo "ASSERT FAILED: chats ok:true"; fail=1; }
echo "$out" | grep -q '"name":"general"' || { echo "ASSERT FAILED: chats seeded channel name general"; fail=1; }

echo "== send slackbot C1 hi =="
out="$("$BIN" send slackbot C1 hi)"
echo "$out"
echo "$out" | grep -q '"ok":true' || { echo "ASSERT FAILED: send ok:true"; fail=1; }
echo "$out" | grep -q '"message_id":"1700000000.000200"' || { echo "ASSERT FAILED: send seeded message_id"; fail=1; }

echo "== malformed: ok:true with missing messages -> PROTOCOL =="
curl -sf -X POST "$BASE/__mode/malformed" >/dev/null
set +e
out="$("$BIN" messages slackbot C1)"
code=$?
set -e
echo "$out"
echo "$out" | grep -q '"code":"PROTOCOL"' || { echo "ASSERT FAILED: PROTOCOL"; fail=1; }
[ "$code" -ne 0 ] || { echo "ASSERT FAILED: exit code should be non-zero"; fail=1; }

echo "== auth failure: ok:false invalid_auth at HTTP 200 =="
curl -sf -X POST "$BASE/__mode/fail_auth" >/dev/null
set +e
out="$("$BIN" whoami slackbot)"
code=$?
set -e
echo "$out"
echo "$out" | grep -q '"code":"AUTH_FAILED"' || { echo "ASSERT FAILED: AUTH_FAILED"; fail=1; }
[ "$code" -ne 0 ] || { echo "ASSERT FAILED: exit code should be non-zero"; fail=1; }

if [ "$fail" -ne 0 ]; then
  echo "QA FAILED"
  exit 1
fi
echo "QA PASSED"
