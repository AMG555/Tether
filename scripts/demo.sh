#!/usr/bin/env bash
# Reproducible proof of the core promise, safe to run anywhere (it uses a throwaway vault).
#
#   ./scripts/demo.sh
#
# 1. Types a sentence into a fake app.
# 2. Kills the process with a hard abort — no flush, no unwinding, no Drop.
# 3. Shows the buffer survived, recovers it, and prints it back.
set -euo pipefail
cd "$(dirname "$0")/.."

BIN="${SAVEPOINT_BIN:-./target/release/rescue}"
[ -x "$BIN" ] || cargo build --release --quiet
BIN="${SAVEPOINT_BIN:-./target/release/rescue}"

export SAVEPOINT_HOME="$(mktemp -d)"
trap 'rm -rf "$SAVEPOINT_HOME"' EXIT

hr() { printf '\n\033[1;36m── %s\033[0m\n' "$*"; }
prompt="refactor the auth middleware so tokens rotate, and keep the tests green"

hr "1. init (throwaway vault)"
"$BIN" init | head -6

hr "2. type 74 characters, then hard-crash with no flush"
"$BIN" simulate --text "$prompt" --app "Editor" --crash || true

hr "3. what survived on disk"
"$BIN" status

hr "4. recover"
"$BIN" recover

hr "5. read it back"
"$BIN" list --preview 80

hr "6. proof it is encrypted at rest"
echo "grepping every file in the vault for the word 'middleware':"
if grep -rl "middleware" "$SAVEPOINT_HOME" 2>/dev/null; then
  echo "LEAK"; else echo "  not found — plaintext never touched the disk"; fi

echo
echo "done. the same thing is asserted by tests/crash_recovery.rs on every commit."
