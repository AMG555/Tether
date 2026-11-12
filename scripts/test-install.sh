#!/bin/sh
# Proves the installer does the three things that matter, without touching GitHub.
#
#   1. a good release installs, and the binary runs
#   2. a tampered tarball is refused, and nothing is left installed
#   3. a checksums.txt that does not mention the asset is refused
#
# Run: sh scripts/test-install.sh   (needs curl, tar, sha256sum/shasum, python3)
set -eu

here=$(cd "$(dirname "$0")" && pwd)
repo=$(dirname "$here")
bin=${TETHER_TEST_BIN:-$repo/target/release/tether}
[ -x "$bin" ] || { echo "build first: cargo build --release"; exit 1; }

work=$(mktemp -d)
port=${TETHER_TEST_PORT:-8761}
cleanup() {
  [ -n "${srv:-}" ] && kill "$srv" 2>/dev/null || true
  rm -rf "$work"
}
trap cleanup EXIT INT TERM

pass=0; failed=0
ok()   { pass=$((pass + 1));   printf '  ok    %s\n' "$1"; }
bad()  { failed=$((failed + 1)); printf '  FAIL  %s\n' "$1"; }

# --- a stand-in release ------------------------------------------------------
mkdir -p "$work/rel/pkg" "$work/home/.local/bin"
cp "$bin" "$work/rel/pkg/tether"
asset=tether-linux-x86_64.tar.gz
( cd "$work/rel" && tar czf "$asset" -C pkg tether && sha256sum "$asset" > checksums.txt )

( cd "$work/rel" && exec python3 -m http.server "$port" >/dev/null 2>&1 ) &
srv=$!
# wait for the port rather than sleeping a fixed amount
i=0
while [ $i -lt 50 ]; do
  curl -fsS "http://127.0.0.1:$port/checksums.txt" >/dev/null 2>&1 && break
  i=$((i + 1)); sleep 0.1
done

export HOME="$work/home"
export TETHER_BASE_URL="http://127.0.0.1:$port"
unset TETHER_BIN_DIR TETHER_VERSION || true

echo "installer tests"

# --- 1. the happy path -------------------------------------------------------
if sh "$here/install.sh" >"$work/1.log" 2>&1; then
  ok "a good release installs"
else
  bad "a good release failed to install"; sed 's/^/        /' "$work/1.log"
fi
if [ -x "$HOME/.local/bin/tether" ] && "$HOME/.local/bin/tether" --version >/dev/null 2>&1; then
  ok "the installed binary runs"
else
  bad "the installed binary is missing or does not run"
fi
grep -q 'checksum  ok' "$work/1.log" && ok "it verifies the checksum" || bad "no checksum line"

# --- 2. tampering ------------------------------------------------------------
printf '0000000000000000000000000000000000000000000000000000000000000000  %s\n' "$asset" \
  > "$work/rel/checksums.txt"
rm -f "$HOME/.local/bin/tether"
if sh "$here/install.sh" >"$work/2.log" 2>&1; then
  bad "a tampered tarball installed anyway"
else
  ok "a tampered tarball is refused"
fi
[ -f "$HOME/.local/bin/tether" ] && bad "a failed install left a binary behind" \
  || ok "a failed install leaves nothing behind"
grep -qi 'mismatch' "$work/2.log" && ok "it says why, and names both hashes" \
  || bad "the refusal did not mention a mismatch"

# --- 3. a checksum file that skips the asset ---------------------------------
echo "deadbeef  some-other-file.tar.gz" > "$work/rel/checksums.txt"
if sh "$here/install.sh" >"$work/3.log" 2>&1; then
  bad "installed with no checksum for the asset"
else
  ok "a checksums.txt with no entry for the asset is refused"
fi

# --- 4. no checksum file at all ----------------------------------------------
rm -f "$work/rel/checksums.txt"
if sh "$here/install.sh" >"$work/4.log" 2>&1; then
  bad "installed with no checksums.txt at all"
else
  ok "a release with no checksums.txt is refused"
fi

# --- 5. upgrade in place -----------------------------------------------------
( cd "$work/rel" && sha256sum "$asset" > checksums.txt )
sh "$here/install.sh" >"$work/5.log" 2>&1 || true   # first re-install after cleanup
if sh "$here/install.sh" >"$work/6.log" 2>&1 && grep -q 'upgrade   replacing' "$work/6.log"; then
  ok "re-running upgrades in place and says so"
else
  bad "re-running did not report an upgrade"
fi

echo ""
if [ "$failed" -eq 0 ]; then
  echo "$pass passed"
else
  echo "$failed failed, $pass passed"; exit 1
fi
