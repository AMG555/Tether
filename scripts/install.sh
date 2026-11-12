#!/bin/sh
# Savepoint installer — one command, no dependencies, no root.
#
#   curl -fsSL https://raw.githubusercontent.com/USER/savepoint/main/scripts/install.sh | sh
#
# What it does, in order, and nothing else:
#   1. Works out your OS and CPU.
#   2. Downloads the matching binary from the latest GitHub release.
#   3. Verifies its SHA-256 against the published checksum.
#   4. Installs to ~/.local/bin (or $SAVEPOINT_BIN_DIR) and tells you if that is not on PATH.
#
# Overrides, all optional: SAVEPOINT_VERSION, SAVEPOINT_BIN_DIR, SAVEPOINT_REPO, SAVEPOINT_BASE_URL.
#
# It does not use sudo, does not touch /usr/local, does not write a shell profile without
# asking, and does not phone home. Re-running it upgrades in place.

set -eu

REPO="${SAVEPOINT_REPO:-USER/savepoint}"
BIN_DIR="${SAVEPOINT_BIN_DIR:-$HOME/.local/bin}"
VERSION="${SAVEPOINT_VERSION:-latest}"

say()  { printf '%s\n' "$*"; }
fail() { printf '\nsavepoint installer: %s\n' "$*" >&2; exit 1; }

# ---------------------------------------------------------------- 1. platform
os=$(uname -s 2>/dev/null || echo unknown)
arch=$(uname -m 2>/dev/null || echo unknown)

case "$os" in
  Linux)  platform=linux ;;
  Darwin) platform=macos ;;
  *) fail "unsupported OS: $os (this script handles Linux and macOS; on Windows use the .zip from the release page)" ;;
esac

case "$arch" in
  x86_64|amd64)  cpu=x86_64 ;;
  arm64|aarch64) cpu=aarch64 ;;
  *) fail "unsupported CPU: $arch" ;;
esac

asset="savepoint-$platform-$cpu.tar.gz"
say "savepoint installer"
say "  platform  $platform / $cpu"
say "  target    $BIN_DIR"

# ---------------------------------------------------------------- 2. download
command -v curl >/dev/null 2>&1 || fail "curl is required"
command -v tar  >/dev/null 2>&1 || fail "tar is required"

tmp=$(mktemp -d 2>/dev/null || mktemp -d -t savepoint)
trap 'rm -rf "$tmp"' EXIT INT TERM

if [ -n "${SAVEPOINT_BASE_URL:-}" ]; then
  base=$SAVEPOINT_BASE_URL          # mirrors, air-gapped installs, and the test suite
elif [ "$VERSION" = "latest" ]; then
  base="https://github.com/$REPO/releases/latest/download"
else
  base="https://github.com/$REPO/releases/download/$VERSION"
fi

say "  fetching  $asset"
if ! curl -fsSL --retry 3 --connect-timeout 15 -o "$tmp/$asset" "$base/$asset"; then
  fail "download failed. Check your connection, or grab the file by hand from
  https://github.com/$REPO/releases"
fi

# ---------------------------------------------------------------- 3. verify
# A binary you piped from the internet is a binary you should check. If the release
# publishes checksums we insist on a match; if it does not, we say so rather than pretend.
if curl -fsSL --retry 2 --connect-timeout 10 -o "$tmp/checksums.txt" "$base/checksums.txt" 2>/dev/null; then
  # The checksum file exists, so it must vouch for this asset. A missing entry is not
  # "no checksum available" — it means the release is malformed or the file was tampered
  # with. Either way we stop rather than install something unverified.
  want=$(grep -E "[[:space:]]\*?$asset\$" "$tmp/checksums.txt" | awk '{print $1}' | head -n1)
  [ -n "$want" ] || fail "checksums.txt has no entry for $asset. Refusing to install an unverified binary."
  if [ -n "$want" ]; then
    if command -v sha256sum >/dev/null 2>&1; then
      got=$(sha256sum "$tmp/$asset" | awk '{print $1}')
    elif command -v shasum >/dev/null 2>&1; then
      got=$(shasum -a 256 "$tmp/$asset" | awk '{print $1}')
    else
      fail "no sha256sum or shasum on this machine, so the download cannot be verified.
  Install one (coreutils) or check the tarball by hand."
    fi
    if [ "$got" != "$want" ]; then
      fail "checksum mismatch. Expected $want
  got      $got
Refusing to install. Download the release manually and check it yourself."
    fi
    say "  checksum  ok"
  fi
else
  fail "this release publishes no checksums.txt, so the download cannot be verified.
  Install by hand from https://github.com/$REPO/releases and check the hash yourself."
fi

# ---------------------------------------------------------------- 4. install
mkdir -p "$BIN_DIR"
tar -xzf "$tmp/$asset" -C "$tmp"
# Accept either a bare binary or a folder containing one.
src="$tmp/savepoint"
[ -f "$src" ] || src=$(find "$tmp" -maxdepth 2 -type f -name savepoint | head -n1)
[ -n "${src:-}" ] && [ -f "$src" ] || fail "the archive did not contain a savepoint binary"

if [ -f "$BIN_DIR/savepoint" ]; then
  say "  upgrade   replacing the existing install"
fi
mv "$src" "$BIN_DIR/savepoint"
chmod +x "$BIN_DIR/savepoint"

say ""
say "installed: $BIN_DIR/savepoint"
if ! "$BIN_DIR/savepoint" --version >/dev/null 2>&1; then
  fail "the binary will not run on this machine. Please open an issue with your platform details."
fi
say "version:   $("$BIN_DIR/savepoint" --version)"

# ---------------------------------------------------------------- 5. PATH
case ":$PATH:" in
  *":$BIN_DIR:"*)
    say ""
    say "next:"
    say "  savepoint setup"
    ;;
  *)
    say ""
    say "$BIN_DIR is not on your PATH."
    say ""
    say "Add it for this shell:"
    say "  export PATH=\"$BIN_DIR:\$PATH\""
    say ""
    say "…and for future shells, add that same line to:"
    case "$(basename "${SHELL:-sh}")" in
      zsh)  say "  ~/.zshrc" ;;
      bash) say "  ~/.bashrc  (or ~/.bash_profile on macOS)" ;;
      fish) say "  ~/.config/fish/config.fish   →  set -gx PATH $BIN_DIR \$PATH" ;;
      *)    say "  your shell's startup file" ;;
    esac
    say ""
    say "Then run:  $BIN_DIR/savepoint setup"
    ;;
esac
