#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../../../.." && pwd)"
QA_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
CFG="$(mktemp -d)"
FAKE_BIN="$QA_DIR/.horon-messenger-fake"
NOLIB_BIN="$QA_DIR/.horon-messenger-nolib"

cleanup() {
  rm -f "$FAKE_BIN" "$NOLIB_BIN"
  rm -rf "$CFG"
  echo "cleanup: qa binaries and temp config $CFG removed"
}
trap cleanup EXIT

echo "# building the CLI with the qa-fake telegram backend (fixtures, no libtdjson)"
(cd "$ROOT" && cargo build --workspace \
  --features "horon-messenger-sdk/all horon-messenger-provider-telegram/qa-fake" -q)
cp "$ROOT/target/debug/horon-messenger" "$FAKE_BIN"

echo "# building the CLI with no telegram backend (no qa-fake, no tdlib)"
(cd "$ROOT" && cargo build -p horon-messenger-cli --features "horon-messenger-sdk/all" -q)
cp "$ROOT/target/debug/horon-messenger" "$NOLIB_BIN"

export HORON_MESSENGER_CONFIG_DIR="$CFG"

echo "# whoami telegram"
"$FAKE_BIN" whoami telegram | python3 -c 'import sys,json; d=json.load(sys.stdin); assert d["ok"] is True, d; assert d["data"]["account_id"]=="777000", d; assert d["data"]["display_name"]=="Ada Lovelace", d; assert d["data"]["username"]=="ada", d; print("ok:true whoami Ada Lovelace 777000 @ada")'

echo "# chats telegram"
"$FAKE_BIN" chats telegram | python3 -c 'import sys,json; d=json.load(sys.stdin); assert d["ok"] is True, d; assert d["data"][0]["id"]=="1001", d; assert d["data"][0]["name"]=="Ada Lovelace", d; assert d["data"][0]["kind"]=="direct", d; assert d["data"][1]["kind"]=="group", d; print("ok:true chats 1001 direct, 1002 group")'

echo "# messages telegram 1001"
"$FAKE_BIN" messages telegram 1001 | python3 -c 'import sys,json; d=json.load(sys.stdin); assert d["ok"] is True, d; assert d["data"]["messages"][0]["text"]=="look at this", d; assert d["data"]["messages"][0]["attachments"][0]["id"]=="900", d; print("ok:true messages look at this with photo attachment")'

echo "# adversarial: stale closed state -> AUTH_REQUIRED"
stale_out="$(HORON_MESSENGER_TELEGRAM_FAKE_STATE=closed "$FAKE_BIN" whoami telegram || true)"
echo "$stale_out" | python3 -c 'import sys,json; d=json.load(sys.stdin); assert d["ok"] is False, d; assert d["error"]["code"]=="AUTH_REQUIRED", d; print("ok:true closed state -> AUTH_REQUIRED")'

echo "# adversarial: malformed fixture -> PROTOCOL"
bad_out="$(HORON_MESSENGER_TELEGRAM_FAKE_MALFORMED=1 "$FAKE_BIN" whoami telegram || true)"
echo "$bad_out" | python3 -c 'import sys,json; d=json.load(sys.stdin); assert d["ok"] is False, d; assert d["error"]["code"]=="PROTOCOL", d; print("ok:true malformed json -> PROTOCOL")'

echo "# adversarial: no qa-fake and no TDLib library -> typed CONFIG error, non-zero exit, no panic"
set +e
nolib_out="$("$NOLIB_BIN" whoami telegram 2>"$CFG/nolib.err")"
nolib_code=$?
set -e
[ "$nolib_code" -ne 0 ] || { echo "expected a non-zero exit, got $nolib_code"; exit 1; }
[ "$nolib_code" -ne 101 ] || { echo "the provider panicked (exit 101)"; exit 1; }
if grep -qi panic "$CFG/nolib.err"; then
  echo "panic detected:"
  cat "$CFG/nolib.err"
  exit 1
fi
echo "$nolib_out" | python3 -c 'import sys,json; d=json.load(sys.stdin); assert d["ok"] is False, d; assert d["error"]["code"]=="CONFIG", d; assert "TDLib" in d["error"]["message"], d; print("ok:true no library -> CONFIG, non-zero exit, no panic")'

echo "telegram qa done"
