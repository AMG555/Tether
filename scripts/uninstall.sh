#!/bin/sh
# Tether Uninstaller for Linux and macOS
#
#   curl -fsSL https://raw.githubusercontent.com/AMG555/Tether/main/scripts/uninstall.sh | sh
#   Or locally: sh scripts/uninstall.sh [--purge]
#
# Removes autostart services/agents, desktop launchers, and binaries.
# Passing --purge also wipes local encrypted vault records and encryption keys.

set -eu

PURGE=0
for arg in "$@"; do
  case "$arg" in
    --purge) PURGE=1 ;;
  esac
done

say() { printf '%s\n' "$*"; }

say "tether uninstaller"

# 1. Stop background daemon
pkill -f "tether run" 2>/dev/null || true
pkill -f "tether-desktop" 2>/dev/null || true

# 2. Remove autostart
os=$(uname -s 2>/dev/null || echo unknown)
if [ "$os" = "Linux" ]; then
  config_dir="${XDG_CONFIG_HOME:-$HOME/.config}"
  if [ -f "$config_dir/systemd/user/tether.service" ]; then
    systemctl --user stop tether.service 2>/dev/null || true
    systemctl --user disable tether.service 2>/dev/null || true
    rm -f "$config_dir/systemd/user/tether.service"
    say "  ok  removed systemd user service"
  fi
  rm -f "$config_dir/autostart/tether.desktop"
  rm -f "${XDG_DATA_HOME:-$HOME/.local/share}/applications/tether.desktop"
elif [ "$os" = "Darwin" ]; then
  plist="$HOME/Library/LaunchAgents/dev.tether.agent.plist"
  if [ -f "$plist" ]; then
    launchctl unload "$plist" 2>/dev/null || true
    rm -f "$plist"
    say "  ok  removed LaunchAgent"
  fi
  rm -f "$HOME/Library/LaunchAgents/dev.tether.plist"
fi

# 3. Remove binary
bin_dir="${TETHER_BIN_DIR:-$HOME/.local/bin}"
if [ -f "$bin_dir/tether" ]; then
  rm -f "$bin_dir/tether"
  say "  ok  removed binary from $bin_dir/tether"
fi
if [ -f "$bin_dir/tether-desktop" ]; then
  rm -f "$bin_dir/tether-desktop"
  say "  ok  removed desktop binary from $bin_dir/tether-desktop"
fi

# 4. Optional Purge
if [ "$PURGE" -eq 1 ]; then
  say "  purging local vault data and keys..."
  if [ "$os" = "Darwin" ]; then
    rm -rf "$HOME/Library/Application Support/dev.tether.tether" 2>/dev/null || true
  else
    rm -rf "${XDG_DATA_HOME:-$HOME/.local/share}/tether" 2>/dev/null || true
  fi
  say "  ok  vault and encryption keys purged"
else
  say "  vault data preserved (use --purge to wipe stored texts and keys)"
fi

say ""
say "Tether uninstallation complete."
