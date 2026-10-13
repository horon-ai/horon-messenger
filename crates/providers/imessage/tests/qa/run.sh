#!/usr/bin/env bash
set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "$HERE/../../../../.." && pwd)"
BIN="$ROOT/target/debug/hyper-messenger"
STUB="$HERE/stub_imsg.py"
CFG="$(mktemp -d)"

chmod +x "$STUB"

cleanup() {
  rm -rf "$CFG"
  echo "cleanup: temp config $CFG removed; no background processes started"
}
trap cleanup EXIT

echo "# building the CLI with the imessage provider"
(cd "$ROOT" && cargo build -p hyper-messenger-cli --features hyper-messenger-sdk/all -q)

export HYPER_MESSENGER_CONFIG_DIR="$CFG"
export HYPER_MESSENGER_IMSG_BIN="$STUB"

fail=0

echo "# happy: chats imessage (stub over HYPER_MESSENGER_IMSG_BIN)"
"$BIN" chats imessage | python3 -c 'import sys,json; d=json.load(sys.stdin); assert d["ok"] is True, d; c=[x for x in d["data"] if x["id"]=="42"][0]; assert c["name"]=="Jane", c; assert c["kind"]=="direct", c; assert c["member_count"]==1, c; print("ok:true chats=2 first=Jane kind=direct")' || fail=1

echo "# happy: messages imessage 42 (oldest first)"
"$BIN" messages imessage 42 | python3 -c 'import sys,json; d=json.load(sys.stdin); assert d["ok"] is True, d; m=d["data"]["messages"]; assert len(m)==2, d; assert [x["id"] for x in m]==["m1","m2"], m; assert d["data"]["newest_first"] is False, d; print("ok:true messages=2 ordered=m1,m2")' || fail=1

echo "# happy: members imessage 43"
"$BIN" members imessage 43 | python3 -c 'import sys,json; d=json.load(sys.stdin); assert d["ok"] is True, d; assert len(d["data"])==2, d; print("ok:true members=2")' || fail=1

echo "# happy: auth status imessage (no login needed)"
"$BIN" auth status imessage | python3 -c 'import sys,json; d=json.load(sys.stdin); assert d["ok"] is True, d; assert d["data"]["authenticated"] is True, d; print("ok:true auth status authenticated")' || fail=1

echo "# adversarial: malformed stub output -> PROTOCOL, non-zero exit"
set +e
out="$(IMSG_STUB_MODE=malformed "$BIN" chats imessage)"
code=$?
set -e
echo "$out"
echo "$out" | grep -q '"code":"PROTOCOL"' || { echo "ASSERT FAILED: PROTOCOL"; fail=1; }
[ "$code" -ne 0 ] || { echo "ASSERT FAILED: exit code should be non-zero"; fail=1; }

echo "# adversarial: Full Disk Access denied -> AUTH_FAILED naming the permission, non-zero exit"
set +e
out="$(IMSG_STUB_MODE=fda "$BIN" chats imessage)"
code=$?
set -e
echo "$out"
echo "$out" | grep -q '"code":"AUTH_FAILED"' || { echo "ASSERT FAILED: AUTH_FAILED"; fail=1; }
echo "$out" | grep -q 'Full Disk Access' || { echo "ASSERT FAILED: message must name Full Disk Access"; fail=1; }
[ "$code" -ne 0 ] || { echo "ASSERT FAILED: exit code should be non-zero"; fail=1; }

echo "# failure: invalid binary (env points nowhere, PATH has no imsg) -> CONFIG, non-zero exit"
set +e
out="$(PATH=/nonexistent HYPER_MESSENGER_IMSG_BIN=/nonexistent/imsg "$BIN" chats imessage)"
code=$?
set -e
echo "$out"
echo "$out" | grep -q '"code":"CONFIG"' || { echo "ASSERT FAILED: CONFIG"; fail=1; }
[ "$code" -ne 0 ] || { echo "ASSERT FAILED: exit code should be non-zero"; fail=1; }

echo "# failure: unset binary and no imsg on PATH -> CONFIG, non-zero exit"
set +e
out="$(env -u HYPER_MESSENGER_IMSG_BIN PATH=/nonexistent "$BIN" chats imessage)"
code=$?
set -e
echo "$out"
echo "$out" | grep -q '"code":"CONFIG"' || { echo "ASSERT FAILED: CONFIG"; fail=1; }
[ "$code" -ne 0 ] || { echo "ASSERT FAILED: exit code should be non-zero"; fail=1; }

if [ "$fail" -ne 0 ]; then
  echo "QA FAILED"
  exit 1
fi
echo "QA PASSED"
