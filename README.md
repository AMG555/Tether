<div align="center">

<p align="center">
  <img src="assets/logo.svg" width="96" height="96" alt="Tether Vector Logo" />
</p>

# Tether

<p align="center">
  <a href="https://github.com/AMG555/Tether/actions/workflows/ci.yml"><img src="https://github.com/AMG555/Tether/actions/workflows/ci.yml/badge.svg" alt="CI Status"></a>
  <a href="LICENSE"><img src="https://img.shields.io/badge/license-MIT-blue.svg" alt="License: MIT"></a>
  <a href="https://blog.rust-lang.org/2023/12/28/Rust-1.75.0.html"><img src="https://img.shields.io/badge/rustc-1.75%2B-orange.svg" alt="Rust 1.75+"></a>
  <a href="#desktop-application--gui"><img src="https://img.shields.io/badge/gui-Tauri%202.0-24C8D8.svg" alt="Tauri 2"></a>
  <a href="#weight-measured"><img src="https://img.shields.io/badge/cli--binary-1.99--2.58_MB-success.svg" alt="Binary Size"></a>
  <a href="SECURITY.md"><img src="https://img.shields.io/badge/encryption-AES--256--GCM-blueviolet.svg" alt="AES-256-GCM"></a>
  <a href="#privacy-the-part-that-matters"><img src="https://img.shields.io/badge/network-zero--telemetry-brightgreen.svg" alt="Zero telemetry"></a>
</p>

**100% offline crash recovery, typing autosave, and desktop companion engine.**

Tether is a privacy-first, ultra-fast local crash recovery instrument written in Rust. While active, it captures typing into a sub-millisecond append-only encrypted ring buffer and flushes drafts into an AES-256-GCM SQLite vault. **Zero network calls. Zero cloud telemetry. Zero subscriptions.** Sensitive fields like passwords, crypto seed phrases, tokens, and credit cards are blocked or masked before disk.

Choose a featherlight native **Desktop GUI** (Tauri 2) with 32 downloadable companions, live state previews, and a floating desktop overlay — or an invisible **CLI daemon** for headless servers.

<br>

**[📥 Install](#-direct-download--instant-setup) · [🗑️ Uninstall](#️-clean-uninstallation) · [Highlights](#highlights) · [Desktop App](#desktop-application--gui) · [32 Companions](#32-companion-characters--on-demand-packs) · [Studio](#character-studio--state-engine) · [Privacy](#privacy-the-part-that-matters) · [Architecture](#architecture)**

<br>

![Tether Desktop Recover](shots/desktop-recover.png)

</div>

---

## 📥 Direct Download & Instant Setup

### Option 1: Standalone Installer (Windows)

**[⬇️ Download Tether_0.1.0_x64-setup.exe (~1.5 MB)](https://github.com/AMG555/Tether/releases/latest/download/Tether_0.1.0_x64-setup.exe)** — Windows 10/11 · no account · no internet required after install

Double-click the installer. Done. Tether appears in your Start Menu and runs at login.

> **Windows SmartScreen notice:** Because Tether does not carry a paid Authenticode certificate, SmartScreen may show "Unrecognized app" on first run. Click **More info → Run anyway** to proceed. Tether is fully open-source and built transparently on GitHub Actions.

> **WebView2:** Included with Windows 11 and most up-to-date Windows 10. If the app doesn't open, install [WebView2 Runtime](https://developer.microsoft.com/en-us/microsoft-edge/webview2/) (free, 2 MB).

### Option 2: ⚡ 1-Line PowerShell Installer

```powershell
irm https://raw.githubusercontent.com/AMG555/Tether/main/scripts/install.ps1 | iex
```

### Option 3: 1-Line Shell Installer (Linux / macOS)

```sh
curl -fsSL https://raw.githubusercontent.com/AMG555/Tether/main/scripts/install.sh | sh
```

---

## 🗑️ Clean Uninstallation

Tether respects your machine. When you uninstall, everything is cleaned up completely.

### Option 1: From the Desktop App (1-click)

Open **Settings** tab (`Ctrl+4`) → scroll to **Danger Zone** → click **Uninstall Tether**.  
Check **Purge all encrypted vault data** to also wipe saved texts, keys, and companion files.

### Option 2: 1-Line PowerShell Uninstall

```powershell
irm https://raw.githubusercontent.com/AMG555/Tether/main/scripts/uninstall.ps1 | iex
```

### Option 3: Windows Settings

**Windows Settings → Apps → Installed apps → Tether → Uninstall**

The uninstaller removes the startup shortcut, unhooks background capture, and optionally purges all encrypted local records.

---

## Highlights

- 🛡️ **Minimal Network Footprint** — Zero telemetry, zero external APIs, zero background phone-home. The CLI has no network code path whatsoever (enforced by `tests/privacy.rs`). The desktop shell makes one optional, checksum-verified HTTPS request when installing a companion pack not already on disk.
- ⚡ **Instant Crash Recovery** — Sub-millisecond ring buffer writes (~4 µs/key). Recovers text even after sudden `SIGKILL`, kernel panic, or power cut.
- 🔐 **Military-Grade Local Vault** — AES-256-GCM encryption at rest with HKDF-derived keys and SQLite WAL mode.
- 🕒 **14-Day Rolling Retention** — Saved texts auto-prune after 14 days by default. Full control over retention windows.
- 🖥️ **Tauri 2 Native Desktop App** — Modern, accessible desktop client with instant search, revision timeline, and one-click copy.
- 🐾 **Floating Companion Overlay** — After activating a companion, a transparent always-on-top animated character appears on your desktop — draggable, state-cycling, and dismissable with one click.
- 🎭 **32 Desktop Companions** — On-demand downloadable packs (CapCut-style asset streaming) keeping the initial installer under 4 MB.
- 🎨 **Live Preview & State Engine** — Full control over `idle`, `walk`, `wave`, `water`, `stretch`, `happy`, and `sad` states with custom speech bubbles, scale, and playback speed.
- ⏰ **Circadian & Battery Awareness** — Companions adapt mood and reminders to time of day and battery health.
- ⌨️ **Global Hotkeys** — Instant access via `Ctrl+Alt+T`, with full in-app navigation (`Ctrl+1..4`, `Ctrl+F`, `Ctrl+R`, `?`).
- 🌙 **Dark/Light Theme** — Defaults to a professional dark charcoal palette; toggle or follow OS preference.
- 🧹 **Complete Clean Uninstall** — 1-click in Settings, 1-line PowerShell, or Windows Settings → Apps.

---

## Desktop Application & GUI

Tether includes a native desktop application powered by **Tauri 2** and system webviews — accessible, responsive on high-DPI displays, and zero resource waste.

````carousel
![Desktop Text Recovery](shots/desktop-recover.png)
<!-- slide -->
![Companions Catalog](shots/desktop-companions.png)
<!-- slide -->
![Companion Preview & Customizer](shots/desktop-preview.png)
<!-- slide -->
![Character Studio & State Engine](shots/desktop-studio.png)
<!-- slide -->
![System Settings & Clean Uninstall](shots/desktop-settings.png)
````

### Key Desktop Capabilities

1. **Saved Drafts & Revision History** — Search drafts across all applications, inspect revisions, see word counts, and copy back in one click (`Ctrl+C`).
2. **Floating Companion Overlay** — Activate any companion to spawn a transparent, always-on-top overlay window on your desktop. The companion walks, waves, and reacts. Draggable by mouse. Dismissable from Settings.
3. **On-Demand Companion Downloads** — Browse 32 curated animated companions. Download only what you want directly to your local machine.
4. **Interactive Character Preview** — Test every animation state, customize speech lines, adjust scale (80%–150%) and speed (0.7x–1.3x) before activating.
5. **Studio Customizer** — Author bespoke characters from animated WebP clips or build procedural rigs with custom color palettes, proportions, and personality.
6. **System Settings & Diagnostics** — Toggle autostart, configure rolling retention (1–90 days), inspect vault encryption status, and run one-click uninstallation.

---

## 32 Companion Characters & On-Demand Packs

To keep the application download under 4 MB, Tether uses a lightweight on-demand asset model. Companions download to `%LOCALAPPDATA%\Tether\companions` or `~/.local/share/tether/companions` only when requested.

| Category | Companions | Personality & Style |
|---|---|---|
| **Classics** | Grace, Rory, Finn, Spike | Balanced productivity, gentle focus, hydration pacing |
| **Energetic** | Blaze, Bolt, Zippy, Axel | High-tempo sprints, coding velocity, victory celebrations |
| **Cozy & Calm** | Bao, Coco, Daisy, Marmalade | Soft pauses, posture alignment, tea & breathing reminders |
| **Tech & Cyber** | Pixel, Quest, Doc, Shadow | Monospace vibes, terminal aesthetics, debugging encouragement |
| **Nature & Zen** | Nimbus, Splash, Zen, Sage | Ambient tranquility, deep thinking, eye relief prompts |
| **Whimsical** | Captain Dew, Chip, Ember, Iris, Luna, Nova, Ollie, Peanut, Penny, Theo, Twirl, Waddle | Playful deskside companions with custom dialogue lines |

Every companion includes animation clips: `idle`, `walk`, `wave`, `water`, `stretch`, `happy`, and `sad`.

---

## Character Studio & State Engine

Manage each companion's emotional states and presence right inside the desktop app:

```
[Character State Engine]
  ├── Idle      ──> Gentle ambient loop while working
  ├── Walk      ──> Patrols across the overlay window
  ├── Wave      ──> Friendly greeting upon startup or app switch
  ├── Water     ──> Hydration nudge at programmed intervals
  ├── Stretch   ──> Posture and eye-strain micro-break cue
  ├── Happy     ──> Triggered when long typing drafts save safely
  └── Sad       ──> Soft alert on unexpected application exit
```

- **Live Speed & Scale Controls** — Tune playback speed from `0.7x` to `1.3x` and scale from `80%` to `150%`.
- **Custom Quote Dialogues** — Assign unique reminder lines to each character state.
- **Trigger Now Testing** — Manually trigger any state on demand without waiting for timers.

---

## Dynamic Timing & System Status

Tether monitors system health and circadian time passively:

- 🌅 **Morning Flow (06:00 – 12:00)** — Energized greetings, focusing on morning momentum.
- 💧 **Hydration & Posture (12:00 – 17:00)** — Water reminders and ergonomics checks.
- 🧘 **Evening Wind-Down (17:00 – 22:00)** — Soft stretch prompts as the workday ends.
- 🌙 **Night Owl Mode (22:00 – 06:00)** — Subdued animations and sleep reminders.
- 🔋 **Battery Awareness** — Companions adapt energy levels when battery drops below 20%.

---

## Build from Source

Requires [Rust 1.75+](https://rustup.rs/) and [Node.js 22+](https://nodejs.org/).

```sh
git clone https://github.com/AMG555/Tether.git && cd Tether

# CLI only (~2 MB binary)
cargo build --release

# Desktop GUI installer
cd desktop && npm install
node node_modules/@tauri-apps/cli/tauri.js build
# → desktop/src-tauri/target/release/bundle/nsis/Tether_0.1.0_x64-setup.exe
```

---

## Keyboard Shortcuts

| Scope | Shortcut | Action |
|---|---|---|
| **Global OS** | `Ctrl + Alt + T` | Launch or focus Tether desktop window |
| **Navigation** | `Ctrl + 1` | Switch to **Recover** tab |
| | `Ctrl + 2` | Switch to **Companions** tab |
| | `Ctrl + 3` | Switch to **Studio** tab |
| | `Ctrl + 4` | Switch to **Settings** tab |
| **Actions** | `Ctrl + F` or `/` | Focus search bar |
| | `Ctrl + R` | Refresh drafts from vault |
| | `Ctrl + C` | Copy selected draft to clipboard |
| | `?` or `Ctrl + /` | Open keyboard shortcuts cheatsheet |
| | `Escape` | Dismiss open modals |

---

## How Capture Works

![Crash Recovery Mechanics](shots/clips.png)

| Storage Layer | Durability & Scope | Latency |
|---|---|---|
| **Crash Ring** | Fixed-slot append-only file. Survives `SIGKILL`, OOM kills, and power cuts. | **~4 µs** per keystroke |
| **Encrypted Vault** | SQLite WAL mode with AES-256-GCM. Stores structured drafts and revisions. | **~1.2 ms** per flush (~1s) |

```
   User Typing
       │
       ▼
 ┌─────────────┐    Sanitizes secrets     ┌──────────────┐    Every ~1s    ┌─────────────┐
 │  OS Capture ├─────────────────────────►│  Crash Ring  ├────────────────►│   Vault     │
 │  (Hook/Tap) │    Blocks password apps  │  (Encrypted) │    Encrypted    │ (AES-256-GCM│
 └─────────────┘    Masks payment cards   └──────┬───────┘    SQLite WAL   │  Revisions) │
                                                 │                         └──────┬──────┘
                                          Process Dies ──┐                        │
                                                         ▼                        ▼
                                                `tether recover`         `tether list`
```

### Platform Support

| Platform | Keystroke Hook | Clipboard Fallback | Elevation Required |
|---|---|---|---|
| **Windows 10 / 11** | ✅ `WH_KEYBOARD_LL` + `ToUnicodeEx` | ❌ Excluded | None (user space) |
| **macOS 13+** | ✅ `CGEventTap` + Accessibility probe | ❌ Excluded | Accessibility permissions |
| **Linux (X11 / Wayland)** | ⚠️ Clipboard-only | ✅ `xclip` / `wl-clipboard` | None (user space) |

---

## Privacy: The Part That Matters

1. **Pre-Write Filtering** — 1Password, Bitwarden, KeePassXC, MetaMask, Phantom, and 20+ other password/crypto apps are always blocked. Window titles containing "sign in", "2FA", "CVV", "seed phrase", and similar patterns are pattern-blocked. Luhn-valid card numbers, JWT tokens, PEM private keys, and AWS/GitHub/OpenAI tokens are masked with `█` before touching disk.
2. **Encrypted at Rest** — AES-256-GCM with per-purpose HKDF key derivation. Files created with `0600` permissions on Unix; protected by user SID ACLs on Windows.
3. **Network Traffic** — CLI has zero network code (enforced by `tests/privacy.rs`). Desktop makes one optional, checksum-verified HTTPS request to download a companion pack; nothing else.

---

## CLI Command Reference

```
tether setup [--autostart]          Guided first run: creates vault, permissions, checks
tether run [--mode ...]             Start foreground capture engine (Ctrl-C commits)
tether recover                      Replay pending crash ring buffer into encrypted vault
tether list [-n 20] [--preview N]   Display recent typing drafts with timestamps
tether show <id> [--bare]           Output full recovered draft
tether copy <id>                    Place recovered text directly on system clipboard
tether search <query>               Fast in-memory decrypt-and-search across drafts
tether revisions <id>               View past revisions of a specific draft
tether prune [--max-days 14]        Prune drafts older than retention window
tether purge --yes                  Wipe all vault data, vacuum SQLite, clear crash ring
tether uninstall [--purge]          Cleanly uninstall autostart and delete binaries/vault
tether ask                          Launch desktop companion
tether ask --character-studio       Open offline Character Studio in local browser
tether ask --clips                  Inspect installed clip states for current character
tether doctor                       Verify DB integrity, permissions, and buffer state
```

---

## Weight, Measured

| Metric | Value | Notes |
|---|---|---|
| **CLI Binary** | **1.99 MB** | `opt-level="z"`, LTO, abort on panic, stripped |
| **Desktop App Binary** | **3.71 MB** | Full Tauri 2 GUI client |
| **Idle Memory (RSS)** | **4.3 MB** | Resident set size while capturing |
| **Capture Latency** | **4.1 µs** | Average per keystroke |
| **Commit Latency** | **1.2 ms** | Ingest, sanitize, encrypt, WAL commit |
| **Cold Startup** | **4.4 ms** | `tether status` execution time |
| **Runtime Dependencies** | **None** | SQLite compiled into binary |

---

## Architecture

| Module | Responsibility |
|---|---|
| [`src/crypto.rs`](src/crypto.rs) | AES-256-GCM encryption, HKDF key derivation, nonces |
| [`src/policy.rs`](src/policy.rs) | Secret scanning, sensitive app detection, masking |
| [`src/ring.rs`](src/ring.rs) | Encrypted append-only crash buffer with torn-write prevention |
| [`src/recover.rs`](src/recover.rs) | Event reconstruction, grapheme-aware backspace handling |
| [`src/vault.rs`](src/vault.rs) | SQLite WAL encrypted storage, blind indexing, revisions |
| [`src/engine.rs`](src/engine.rs) | Orchestrates capture pipeline: Policy → Ring → Vault |
| [`src/remind.rs`](src/remind.rs) | Circadian break timer (presence-aware, content-blind) |
| [`src/avatar/`](src/avatar/) | Character manifest validation, animated WebP/APNG auditor |
| [`src/setup.rs`](src/setup.rs) | Guided system check, autostart installer, and uninstaller |
| [`desktop/`](desktop/) | Tauri 2 native desktop app frontend & IPC commands |

---

## License

Released under the **MIT License**. Created and maintained by **AMG555**.  
See [LICENSE](LICENSE) for details.
