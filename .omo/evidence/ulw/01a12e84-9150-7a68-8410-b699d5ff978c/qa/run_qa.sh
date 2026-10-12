#!/usr/bin/env bash
set -uo pipefail

ROOT="${1:?usage: run_qa.sh <repo-root> <evidence-dir>}"
EVID="${2:?usage: run_qa.sh <repo-root> <evidence-dir>}"
QA_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
BIN="$ROOT/target/debug/hyper-messenger"
SERVER_BIN="$ROOT/target/debug/hyper-messenger-server"
CFG="$(mktemp -d)"
DISCORD_PORT=8771
CT_PORT=8772

mkdir -p "$EVID"

cleanup() {
  [ -n "${MD:-}" ] && kill "$MD" 2>/dev/null
  [ -n "${MC:-}" ] && kill "$MC" 2>/dev/null
  [ -n "${SRV:-}" ] && kill "$SRV" 2>/dev/null
  wait 2>/dev/null
  rm -rf "$CFG"
}
trap cleanup EXIT

python3 "$QA_DIR/mock_discord.py" "$DISCORD_PORT" >/dev/null 2>&1 &
MD=$!
python3 "$QA_DIR/mock_channeltalk.py" "$CT_PORT" >/dev/null 2>&1 &
MC=$!
for _ in $(seq 1 40); do
  curl -s -o /dev/null "localhost:$DISCORD_PORT/users/@me" && break
  sleep 0.1
done

export HYPER_MESSENGER_CONFIG_DIR="$CFG"
export HYPER_MESSENGER_DISCORD_API_BASE="http://127.0.0.1:$DISCORD_PORT"
export HYPER_MESSENGER_CHANNELTALK_API_BASE="http://127.0.0.1:$CT_PORT"
export HYPER_MESSENGER_DISCORD_TOKEN="mock-token"
export HYPER_MESSENGER_CHANNELTALK_ACCOUNT_COOKIE="mock-account"

CHANNEL_ID="300000000000000001"

run() {
  local label="$1" file="$2"; shift 2
  {
    echo "\$ hyper-messenger $*"
    "$BIN" "$@"
    echo "exit=$?"
  } >"$EVID/$file" 2>&1
  echo "[$label] -> $file"
}

run "discord-read" "discord-read.txt" chats discord
{
  echo "\$ hyper-messenger messages discord $CHANNEL_ID"
  "$BIN" messages discord "$CHANNEL_ID"
  echo "exit=$?"
} >>"$EVID/discord-read.txt" 2>&1

run "discord-send" "discord-send.txt" send discord "$CHANNEL_ID" "qa hello from mock"

{
  echo "\$ HYPER_MESSENGER_DISCORD_TOKEN=wrong hyper-messenger chats discord"
  HYPER_MESSENGER_DISCORD_TOKEN=wrong "$BIN" chats discord
  echo "exit=$?"
} >"$EVID/discord-auth-error.txt" 2>&1

run "channeltalk-whoami" "channeltalk-read.txt" whoami channeltalk
{
  echo "\$ hyper-messenger chats channeltalk"
  "$BIN" chats channeltalk
  echo "exit=$?"
  echo "\$ hyper-messenger messages channeltalk ws1/group/g1"
  "$BIN" messages channeltalk "ws1/group/g1"
  echo "exit=$?"
} >>"$EVID/channeltalk-read.txt" 2>&1

run "channeltalk-send" "channeltalk-send.txt" send channeltalk "ws1/group/g1" "qa hello ct"

{
  echo "\$ HYPER_MESSENGER_CHANNELTALK_ACCOUNT_COOKIE=wrong hyper-messenger chats channeltalk"
  HYPER_MESSENGER_CHANNELTALK_ACCOUNT_COOKIE=wrong "$BIN" chats channeltalk
  echo "exit=$?"
} >"$EVID/channeltalk-auth-error.txt" 2>&1

echo "qa done"
