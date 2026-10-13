#!/usr/bin/env bash
set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "$HERE/../../../../.." && pwd)"
BIN="$ROOT/target/debug/horon-messenger"
STUB="$HERE/stub_line.py"
CFG="$(mktemp -d)"

chmod +x "$STUB"

cleanup() {
  rm -rf "$CFG"
  echo "cleanup: temp config $CFG removed; no background processes started"
}
trap cleanup EXIT

echo "# building the CLI with the line provider"
(cd "$ROOT" && cargo build -p horon-messenger-cli --features horon-messenger-sdk/all -q)

export HORON_MESSENGER_CONFIG_DIR="$CFG"
export HORON_MESSENGER_LINE_SIDECAR="$STUB"

fail=0

echo "# happy: auth login line (stub over HORON_MESSENGER_LINE_SIDECAR)"
"$BIN" auth login line --param token=QA_TOKEN | python3 -c 'import sys,json; d=json.load(sys.stdin); assert d["ok"] is True, d; assert d["data"]["authenticated"] is True, d; assert d["data"]["account_id"]=="u_self", d; print("ok:true account_id=u_self")' || fail=1

echo "# happy: chats line"
"$BIN" chats line | python3 -c 'import sys,json; d=json.load(sys.stdin); assert d["ok"] is True, d; c=[x for x in d["data"] if x["id"]=="u_friend"][0]; assert c["name"]=="Friend", c; assert c["kind"]=="direct", c; assert c["member_count"]==1, c; print("ok:true chats=2 first=Friend kind=direct")' || fail=1

echo "# happy: send line u_friend hi (asserts a real message id)"
"$BIN" send line u_friend hi | python3 -c 'import sys,json; d=json.load(sys.stdin); assert d["ok"] is True, d; assert d["data"]["message_id"]=="sent-1", d; assert d["data"]["accepted"] is True, d; assert d["data"]["delivery"]=="server_accepted", d; print("ok:true message_id=sent-1 accepted")' || fail=1

echo "# happy: members line g_crew"
"$BIN" members line g_crew | python3 -c 'import sys,json; d=json.load(sys.stdin); assert d["ok"] is True, d; assert d["data"][0]["id"]=="u_friend", d; assert d["data"][0]["display_name"]=="Friend", d; print("ok:true members=1")' || fail=1

echo "# adversarial: auth failure -> AUTH_FAILED, non-zero exit"
set +e
out="$(LINE_STUB_MODE=authfail "$BIN" chats line)"
code=$?
set -e
echo "$out"
echo "$out" | grep -q '"code":"AUTH_FAILED"' || { echo "ASSERT FAILED: AUTH_FAILED"; fail=1; }
[ "$code" -ne 0 ] || { echo "ASSERT FAILED: exit code should be non-zero"; fail=1; }

echo "# adversarial: malformed stub output -> PROTOCOL, non-zero exit"
set +e
out="$(LINE_STUB_MODE=malformed "$BIN" chats line)"
code=$?
set -e
echo "$out"
echo "$out" | grep -q '"code":"PROTOCOL"' || { echo "ASSERT FAILED: PROTOCOL"; fail=1; }
[ "$code" -ne 0 ] || { echo "ASSERT FAILED: exit code should be non-zero"; fail=1; }

echo "# failure: no credentials -> AUTH_REQUIRED, non-zero exit"
set +e
out="$(HORON_MESSENGER_CONFIG_DIR="$(mktemp -d)" "$BIN" chats line)"
code=$?
set -e
echo "$out"
echo "$out" | grep -q '"code":"AUTH_REQUIRED"' || { echo "ASSERT FAILED: AUTH_REQUIRED"; fail=1; }
[ "$code" -ne 0 ] || { echo "ASSERT FAILED: exit code should be non-zero"; fail=1; }

if [ "$fail" -ne 0 ]; then
  echo "QA FAILED"
  exit 1
fi
echo "QA PASSED"
