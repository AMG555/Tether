# Limitations

A recovery tool that oversells itself is dangerous, because you stop making your own backups. Here is exactly where Tether stops helping.

## Security boundaries

**Root / admin / debugger on a live machine wins.** Tether protects data *at rest* and against *crashes*. If an attacker can read your process memory (`ptrace`, a kernel module, an admin-level memory dump), they can read your text before it is ever encrypted. There is no defence against a live privileged attacker in userland, and claiming otherwise would be a lie.

**Physical access to an unlocked session wins.** Anyone who can run `tether list` as you can read your rescues. There is no passphrase prompt in v1 (see below).

**The master key sits next to the vault.** `key.txt` is mode 0600 in the data directory, not in an OS keychain. This is a deliberate tradeoff: a passphrase prompt would block unattended capture at login, which is the whole point. Consequences:
- Anyone who gets *both* the vault and the key file gets everything. Back them up separately.
- FileVault / BitLocker / LUKS is therefore a real part of your defence, not optional.
- Moving to the OS keychain (`SecItemAdd`, DPAPI, `kernel keyring`) is the top item on the roadmap and would not change the file format.

**Metadata is not hidden.** An attacker with your disk learns: how many rescues exist, when they were created and last edited, how many characters each contains, and how many secrets were redacted. They do not learn text, app names, or window titles. Hiding volume and timing requires fixed-rate padding and decoy traffic, which is out of scope for a 2.0 MB tool ([ADR 0003](adr/0003-encryption.md)).

**Windows directory ACLs are not rewritten.** On Windows the data directory inherits your user-profile ACL, which excludes other non-admin users, but an explicit DACL rewrite (`SetNamedSecurityInfo`) is not implemented. Admin-level users can read the files — but they could also just read your process memory.

## Capture fidelity

**The caret is unknown in keystroke-replay mode.** Low-level hooks report keys, not cursor position. Replay assumes "typing at the end of the field", which is correct for the common case (composing something new) and *degrade-safe* for the rest: a mid-string edit produces a plausible draft containing all your characters, but possibly in a different arrangement than you saw. It never loses text.

`probe()` fixes this exactly, because it reads the field's real contents and caret via the accessibility API. It is enabled by default on macOS (`AXUIElement`) and wired for Windows (`UIA`) — but probe mode is a poll, so a fast typist's last few characters can be missing from a snapshot. Keystroke capture and probe mode are complementary: keystrokes give you *nothing lost*, probe gives you *exactly right*.

**Linux has no keystroke capture.** Wayland forbids global hooks by design and X11 global hooks are indistinguishable from malware. Tether uses the clipboard instead: a smaller, honest guarantee. A `Source` implementation using `evdev` (needs `input` group) or `XTEST` is possible and welcome.

**Multi-line and rich-text fields.** Newlines are captured. Rich text is captured as plain text — formatting is not preserved, which is the right tradeoff for recovery.

**Very large pastes are chunked.** Clipboard content longer than a ring slot (60 chars) is split across events, so a 50 KB paste consumes many slots and may evict earlier history. Drafts are flagged `truncated` when this happens.

**Terminal emulators.** Text typed into a TUI that uses raw mode / alternate screen is captured as keystrokes (including arrow-key escape sequences, which appear as control characters and are treated as non-text). Passwords typed at a `sudo` prompt are *not* detected as password fields on any platform — the blocking rule for terminals is by window title, so add your terminal's title pattern in config if you use one for secrets.

## Functional

- **No GUI.** CLI only in v1. A menu-bar/tray icon is the natural next step; the `Engine` API is already thread-safe and `Runtime::shutdown` is there for it.
- **No sync.** By design. `backup` produces a directory you can put on a USB stick or in an encrypted cloud folder; the key stays separate.
- **No passphrase.** See above.
- **Search is linear.** `search` decrypts candidate rows in-process. At 500 rescues this is ~10 ms; at 50,000 it would be slow. A blind-index token search (HMAC of stemmed words) is the upgrade path, but it leaks a little more metadata, so it's a deliberate non-feature for now.
- **Retention is manual.** `prune` exists but nothing runs it automatically. Wire it into cron / Task Scheduler if you care.
- **Drafts are per (app, session), not per field.** Two text fields in the same app during one continuous typing session share a draft. Probe mode narrows this to (app, window title).
- **No undo of `purge`.** It vacuums the database and removes the ring. That is the point.

## Explicitly out of scope

- Capturing audio, screen, or clipboard *images*.
- Working around OS permission models. If your OS says no, Tether says no.
- Cloud sync, accounts, team features, analytics.
- Any form of remote access. There is no network code in the binary, and a test enforces that.

## Desktop companion

- **Linux has no native pet window yet.** `tether ask` runs a terminal companion with the same
  reminder engine and ASCII art. The native window is `gui/`, next build target.
- **Quiet hours on Windows use UTC** in the terminal companion, because reading real local time
  without a dependency is only portable through `date +%z` on Unix. The GUI reads local time
  properly. Off-by-a-timezone quiet hours is annoying, not dangerous.
- **Pets are not sandboxed.** The format is data-only (a manifest and a PNG), and the loader never
  executes pet content, but a future richer format could grow an attack surface. Keep pets to
  `pet.json` + `sprite.png` and the validator stays meaningful.
- **The pet cannot read your rescues.** It shells out to the same CLI you use and shows a count
  and the most recent preview. It has no vault key and no database handle.
- **Reminders are not a health feature.** They are a nudge on a timer, not medical advice, and the
  engine has no idea whether you actually stood up.

## 3D avatars

**A procedural character is not photoreal, and never will be.** The built-in human, cat, dog and
blob are stylised figures assembled from primitives with a shared skeleton. They read as
intentional characters, not as realism. If you want realism, install a VRM made in VRoid Studio
or a GLB made in Blender — Tether is the runtime for those, and `tether ask --check` audits
them before use.

**WebGL is required, and is not always available.** Remote desktops, some VMs and locked-down
browsers have no WebGL. The companion then shows a clear message instead of a character, and the
2D sprite path still works. This is the "degrade visibly, never silently" rule.

**A VRM will not look identical to its authoring tool.** MToon/toon outlines, material blending
and expression ranges are renderer-dependent. The mesh, skeleton and expressions load; the exact
shading is three.js's interpretation.

**The audit is a security check, not a forensic one.** It reads headers and JSON. It will tell
you a model references an external URL, carries identifying metadata, or embeds a 40000×40000
image. It does not prove the mesh is harmless, because "harmless" for geometry is a rendering
question, not a file question — and decoding untrusted geometry to answer it would hand an
attacker the allocator.
