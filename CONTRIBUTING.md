# Contributing

Thanks for being here. This is a small tool with a sharp promise, so the bar for changes is
"does this keep the promise or break it?"

## The two rules

1. **Never lose text.** If a change can silently drop, truncate or reorder recovered text, it
   needs a test that proves the opposite, and it needs to fail loudly if it can't.
2. **Never store a secret.** Redaction happens *before* the ring write. Any code path that
   touches disk before `Policy::sanitize` is a bug, not a design choice.

Both are enforced by tests in `tests/crash_recovery.rs` and `tests/privacy.rs`. Add to them.

## Getting set up

```bash
git clone https://github.com/USER/tether && cd tether
cargo test                 # 132 tests, no external services
cargo clippy --all-targets -- -D warnings
cargo fmt
```

Linux is the only platform where the whole thing builds without extra permissions, which is
why CI runs all three.

## Where help is most useful

| Area | Why it matters | Where to start |
|---|---|---|
| **Linux keystroke backend** | Wayland can't, but X11 (`XRecord`/`XInput2`) and `evdev` can, with the user's consent | implement `engine::Source`; `capture/passive.rs` is the smallest example |
| **Windows clipboard events** | Keystrokes are captured, but `Ctrl+V` text isn't yet | `AddClipboardFormatListener` needs a message-only window; `capture/windows.rs` |
| **OS keychain for the master key** | removes `key.txt` from the threat model; format-compatible | `crypto::SecretKey` load path, `vault::open` |
| **Native pet window** | the companion is a rendering job now: engine, format, validation and studio are done | `gui/`, driven by `remind::ReminderEngine` + `pet::Pet` |
| **Tray/menu-bar UI** | the biggest usability gap; `Engine` is already thread-safe | `engine::Runtime::start` / `shutdown` |
| **Blind-index search** | `search` currently decrypts every row | `vault::search`, ADR 0003 for the metadata tradeoff |

## Adding a capture backend

Implement three things:

```rust
pub struct MyBackend;
impl Source for MyBackend {
    fn name(&self) -> &'static str { "my-backend" }
    fn probe(&mut self) -> Option<FieldSnapshot> { None }   // optional, high fidelity
    fn run(&mut self, sink: &dyn Fn(Event), stop: &AtomicBool) { ... }
}
```

Rules for `run`: return within microseconds, never block on I/O, never panic on hostile OS data,
and honour `stop` so shutdown commits the ring. Then wire it into `capture::default_source()`
behind `#[cfg]` and add it to the platform table in the README — including the permission it
needs. If a backend cannot capture keystrokes, say so; do not silently degrade.

## Style

- Comments explain **why**, not what. If the reason is subtle (the ring's cursor-vs-timestamp
  decision, the tail re-seeding), the comment should say what breaks without it.
- Prefer a deep module: small interface, all the messiness inside (`mattpocock/skills`
  vocabulary — module, interface, implementation, seam, depth, leverage, locality).
- No new dependency without justifying it in the PR. `Cargo.toml` currently has five direct
  deps and CI fails if a network-capable one appears.
- Public items get doc comments. Private ones get them when the code would otherwise puzzle
  a reviewer.

## PR checklist

- [ ] `cargo test` passes, and new behaviour has a test that fails without the change
- [ ] `cargo clippy --all-targets -- -D warnings` is clean
- [ ] If capture changed: a crash test still passes (`tether simulate --crash` then `recover`)
- [ ] If storage changed: `tests/privacy.rs` still passes (no plaintext on disk)
- [ ] If a decision was non-obvious: a short ADR in `docs/adr/`
- [ ] README numbers updated if performance moved materially

## Security issues

Please don't open a public issue for a vulnerability. Email the maintainer, or open a draft
security advisory on GitHub. Anything that causes plaintext of a redacted secret to reach disk
is treated as a release blocker.
