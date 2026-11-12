# Weight: what this costs, measured

"Ultra lightweight" is a claim, so it gets measurements, a budget, and a list of the
optimisations that were *rejected* because they were not worth what they cost elsewhere.

Everything here is reproducible with the commands shown. Numbers were taken on Linux x86-64,
`rustc` release profile, 2026-11.

## The shipped binary

| Measure | Value | How |
| --- | --- | --- |
| On disk, stripped | **2,089,960 bytes (1.99 MB)** | `ls -l target/release/tether` |
| Resident memory, long-running capture | **4,188 kB (4.1 MB)** | `VmRSS` after 2 s of `tether run` |
| Peak resident | 4,192 kB | `VmHWM` |
| Text section | 1.45 MB | `size -A` |
| Keystroke ingest | ~11 µs | `tests/performance.rs` |
| Flush to encrypted ring | ~1.3 ms | `tests/performance.rs` |

There is no runtime dependency: SQLite is compiled in (`rusqlite` with `bundled`), the crypto is
pure Rust, and the whole thing is one file you can copy onto a machine.

## What the 1.95 MB is made of

| Part | Rough share | Could it go? |
| --- | --- | --- |
| SQLite (bundled, amalgamation) | ~1.1 MB | Only by writing a database. WAL gives crash safety for free; that is the product. |
| `clap` (argument parsing) | ~0.2 MB | Only by hand-rolling parsing, which trades 200 KB for a worse `--help` and more bugs. |
| AES-GCM + SHA-2 | ~0.15 MB | No. This is the security claim. |
| Our own code | the rest | — |

The two things that dominate are the two things that make it a trustworthy tool rather than a
small one.

## Optimisations that were measured and taken

| Change | Effect | Kept? |
| --- | --- | --- |
| `opt-level = "z"` instead of `"s"` | 2,048,840 vs **2,162,088** (+113 KB for `s`) | yes, `z` |
| `panic = "abort"` instead of unwind | 2,048,840 vs **2,178,152** (+129 KB to unwind) | yes, `abort` |
| `lto = true`, `codegen-units = 1` | significant; measured indirectly above | yes |
| `strip = true` | removes ~0.6 MB of symbols | yes |
| `clap` without colour/error-context/suggestions | −24 KB net (see below) | yes |

### The `clap` change, including the bug it caused

`clap`'s default features pull in terminal colour handling, typo suggestions and help wrapping.

- `default-features = false` alone: **1,998,984 bytes** — and `--help` stopped existing.
  The binary got 74 KB smaller, every test passed, and the tool could no longer explain itself.
- `default-features = false` + `std, help, usage`: **2,048,840 bytes**.
- Net saving: **24 KB**, with `--help` intact.

The regression is now guarded by `tests/cli_contract.rs`, which asserts that `--help` exits
successfully, lists every command, and that each subcommand prints usage. A footprint win that
removes the manual is not a win.

## Optimisations rejected, with reasons

| Idea | Saving | Why not |
| --- | --- | --- |
| Drop SQLite, use a hand-rolled append-only file | ~1.1 MB | Loses WAL atomicity, transactions and `PRAGMA quick_check`. The crash guarantee is the product; 1 MB is not worth re-deriving it. |
| Hand-roll CLI parsing | ~0.2 MB | Doubles the surface where a typo silently does the wrong thing. |
| `no_std` / musl static | varies | Not needed: there is no dynamic dependency to remove. |
| Compress the binary (UPX) | ~0.7 MB on disk | Decompresses at exec, needs a writable temp dir, and trips antivirus heuristics. It optimises the wrong number — 2 MB was never the problem. |
| Ship the 3D renderer | — | **Not shipped at all.** See below. |

## The renderer: 74× the weight for the thing people actually asked for

This is the most important number on the page, and it is the one that decided the architecture.

| Path | Code the user's machine must run | Gzipped |
| --- | --- | --- |
| Procedural **clip pack** (animated WebP) | `gui/clip-player.js` + math | **2.8 KB** |
| Procedural **3D** (three.js r187 + loaders) | `gui/vendor/three-bundle.js` | **210 KB** |

**74×.** And the clip route produces better-looking characters, because the art is
AI-generated full frames rather than primitives a program arranged.

So the renderer is *optional and separate*: it is never part of the capture binary, it is never
loaded unless a pet declares a 3D `model`, and the default realistic path costs less than the
CSS on a marketing page. That is what "keep the capture core small" actually bought.

## Budget

Enforced in CI so the number cannot drift upward silently:

- **binary ≤ 2.75 MiB** (currently **1.99 MB** / 2 089 960 bytes)
- **idle RSS ≤ 12 MB** (currently **4.3 MB**, VmHWM 4 304 kB)
- **ingest ≤ 200 µs per keystroke** (currently **4.1 µs**)
- **vault commit ≤ 50 ms** (currently **1.2 ms**)

## Re-measured 2026-11-13, after the clip-mutation work

Every number below came from the release binary in this commit, not from an earlier
measurement carried forward:

| Measure | Value | How |
|---|---|---|
| Binary | 2 089 960 B (1.99 MB) | `ls -l target/release/tether` |
| Startup (`tether status`, 12 runs) | min 4.1 ms, median **4.4 ms**, max 5.1 ms | subprocess timing in a loop |
| Idle RSS, capture running | **4 248 kB**, flat across 4 s of sampling | `/proc/$PID/status` VmRSS |
| Ingest | **4.13 µs**/keystroke | 50 000 `ingest()` calls after a 2 000-key warmup |
| Vault commit | **1.23 ms** | one `flush()` of a 50 000-event backlog |
| Clip path (gzipped) | 2 885 B | `gzip -9c gui/clip-player.js` |
| three.js path (gzipped) | 214 734 B | `gzip -9c gui/vendor/three-bundle.js` |

The ingest figure is the one that matters and it is **4× better than the 11 µs this document
previously claimed** — the earlier number was taken before the ring write was reduced to a single
fixed-size write per keystroke, and was never re-measured. Recording the correction rather than
quietly editing the old line.

RSS did not move when clip packs and the mutation engine were added: those allocate on the
authoring path, not the capture path, and the capture core is untouched by both.

## Hostile input, measured by hand

The clip-import path is where downloaded content enters the process, so it was attacked
directly. Every case below leaves the pet byte-identical:

| Input | Result |
|---|---|
| A text file renamed `.webp` | refused: "is not a WebP, PNG/APNG or GIF" |
| A missing path | refused, names the path |
| `--set-clip ../../etc/passwd` | refused: not one of the ten states, and they are listed |
| A still PNG for `water` | refused: "needs an animation with frames to play" |
| A 45-byte PNG declaring 60000×60000 | refused by **header**, before any allocation: 3600 Mpx vs 16.8 Mpx limit |

## Reproduce

```bash
cargo build --release
ls -l target/release/tether
TETHER_HOME=/tmp/x ./target/release/tether run &   # then read /proc/$!/status VmRSS
gzip -9c gui/vendor/three-bundle.js | wc -c        # 214734
gzip -9c gui/clip-player.js | wc -c                # 2885
sh scripts/test-install.sh                         # the installer's 9 assertions
python3 shots/capture.py                           # all screenshots, from the real binary
```
