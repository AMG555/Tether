# ADR 0005 — Ask for the smallest permission that delivers the promise

**Status:** accepted · **Date:** 2026-11-13

## Context

The user asked for capture "on all platforms". The three platforms have very different
answers, and the temptation is to paper over that difference.

## Decision

Be explicit per platform, and degrade to a smaller honest guarantee rather than asking for more
privilege than necessary.

| Platform | Mechanism | Permission | Guarantee |
|---|---|---|---|
| Windows | `WH_KEYBOARD_LL` | none | exact keystrokes |
| macOS | `CGEventTap` + AX probe | Accessibility + Input Monitoring | exact keystrokes, exact field reads |
| Linux X11/Wayland | clipboard watch | none | everything you copy is saved |

## Why

- **Never root.** A keystroke recorder asking for root is indistinguishable from malware, and
  a privileged helper is a persistent attack surface for a tool whose job is to be boring.
- **Wayland's refusal is a feature.** It exists because global keylogging is dangerous. Fighting
  it (libinput group, `LD_PRELOAD`, uinput) would make this tool a rootkit with good manners.
  Clipboard capture rescues the most common disaster (copy the half-prompt, then crash) and is
  the honest maximum.
- **macOS prompts, and that's fine.** Both permissions are user-granted in System Settings and
  visible in the menu bar's privacy indicators. `tether doctor` reports precise state instead
  of failing silently.
- **Windows needs nothing**, which makes it the smoothest install.

## Consequences

- Feature parity across platforms is explicitly *not* a goal; documented in the README table so
  nobody is surprised.
- On macOS without permissions, `run` prints the exact System Settings panes and continues in
  clipboard mode — it never pretends to be capturing.
