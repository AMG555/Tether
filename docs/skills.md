# How this project was built

Tether was built by deliberately applying three public skill sets — Addy Osmani's
`agent-skills`, Matt Pocock's `skills`, and Garry Tan's `gstack` — rather than by prompting
"build me an app" and accepting the first output. This page records **which method produced
which artifact**, including the parts that changed the design.

The mapping is not decorative. Several of the strongest decisions in this repo exist because a
checklist asked a question that had not been asked yet.

---

## Addy Osmani — `agent-skills`

Phase model: **DEFINE → PLAN → BUILD → VERIFY → REVIEW → SHIP**, with `/constraints` as a
cross-cutting rule: *decide it once, enforce it everywhere*.

### `/constraints` → `deny.toml`, the no-network test, and the capture-core rule

The constraint pass produced the four invariants the whole repo is measured against:

| Constraint | Enforced by |
| --- | --- |
| No network code, ever | `test no_network_crates` in `tests/privacy.rs`; `deny.toml` |
| Capture core stays small | CI fails above the binary-size budget; the pet/3D renderer lives in `gui/` |
| Secrets never reach disk | `src/policy.rs` + `sanitize` applied at *write* time, not read time |
| Untrusted content is untrusted | `src/pet.rs::validate`, `src/avatar/audit.rs` |

Writing these down once is what made the later 3D request tractable: "the renderer is a separate
optional component" was already a rule, so adding a 824 KB three.js bundle did not require
renegotiating the architecture — it required checking it against a constraint that already
existed.

### Threat-model-first + STRIDE → the adversarial review in `docs/adr/0004`

Before writing the capture loop, the threat model listed the assets (typed text, the key, the
vault), the boundaries (process ↔ OS input APIs ↔ disk), and the attackers (another local user, a
malicious pet download, a stolen laptop). STRIDE turned that into concrete tests:

- **Spoofing** → the AAD in `crypto::seal` binds each ciphertext to its ring slot.
- **Tampering** → `CrashRing::salvage` and the `header_corrupt` path; see `tests/crash_recovery.rs`.
- **Information disclosure** → redaction tests that assert a typed password never appears in the
  ring, the vault, or the recovery output.
- **Denial of service** → header-only file size checks before any decoder runs.

### The adversarial pass that actually found a bug

The `REVIEW` phase instruction — *think like an attacker* — produced the char-by-char secret
bypass: typing a password one key at a time slipped past per-event redaction because no single
event looked like a secret. The fix (authoritative `sanitize` at commit, recovery and
`live_text`) came with two regression tests. This is the single most valuable bug the method
found.

### VERIFY as a gate, not a vibe

Every phase ends with a command, not an opinion:

```bash
cargo fmt --check && cargo clippy --all-targets -- -D warnings \
  && cargo test && node --test gui/avatar-math.test.js
```

"Done" is the exit code.

---

## Matt Pocock — `skills`

### `grill-me` → the four questions that set the scope

Before any code, the request was interrogated. The answers became non-negotiables:

1. Which OSes? → **all three**, with downloadable artifacts, not "Linux works".
2. What is captured? → **typing in inputs, automatically**; no manual copy step.
3. How private? → **strict**: never store secrets, skip password fields, password managers,
   banking and payment apps.
4. What kind of repo? → **publish-ready**, a portfolio piece.

Those four answers are why the project has a CI matrix, a two-tier privacy model, a
`BLOCKED_APPS`/`BLOCKED_TITLES` list, and release automation on day one rather than as polish.

### `CONTEXT.md` → `docs/architecture.md`

A single written description of the data flow (keystroke → policy → ring → vault → recovery)
that every later decision could be checked against. When the companion feature arrived, the
question "where does this live?" was already answered by the document: it is a *consumer* of the
reminder engine, not part of capture.

### `/tdd` red → green → refactor

Used for the parts where correctness is not visually obvious:

- `src/ring.rs` — fixed-slot encryption has off-by-one hazards; tests for `salvage` and
  wraparound came first and caught real offset errors.
- `gui/avatar-math.js` — 24 `node:test` cases written against the math before it was wired to a
  renderer. Two of them (`twoBoneIK` with unreachable targets, `damp` frame-rate independence)
  describe properties that would be nearly impossible to verify by looking at a screenshot.
- `src/pet.rs` — every validation rule has a test that asserts the *message*, not just the error,
  because a user who cannot fix the file has been failed twice.

### Deep modules

`crypto`, `ring`, `policy` and `remind` each hide a lot behind a small surface. `remind` in
particular exposes `tick(now, any_input, hour, minute)` — three arguments, and none of them is
content. That signature *is* the privacy claim.

---

## Garry Tan — `gstack`

### `/review` with an adversarial pass

Two separate reviews were run against the finished code:

1. **Attacker**: can a downloaded pet neutralise the UI? (Bidi overrides in `pet.json` — yes,
   before the fix; `U+202E` reverses displayed text.) Can an avatar make the app phone home?
   (An external `uri` in glTF JSON — blocked by `src/avatar/audit.rs`.)
2. **Chaos engineer**: what happens if the process dies *here*? `simulate` runs a real
   `process::abort()` mid-typing; 51 events were replayed and recovered. What if the ring header
   is corrupted? What if the file is truncated mid-slot?

Findings were fixed, not filed.

### "Done means" gates

Each feature was given an explicit, checkable definition of done. For the companion:

- A pet can be created, installed and listed with no network and no extra dependencies.
- The reminder engine cannot read what was typed — asserted by a test that compares a code-heavy
  day with a prose-heavy day and demands identical reminders.
- The capture core binary size does not change.

### `/ship`

`git tag`, CHANGELOG-style release notes, and a `release.yml` matrix that produces macOS,
Windows and Linux binaries. Shipping is a workflow, not an announcement.

---

## The 3D avatar pass, applied end to end

The request — "the pet must look like a realistic 3D human or animal" — went through the same
sequence, and the sequence is why the answer is what it is.

**DEFINE.** Realism is an *asset* problem, not a rendering problem. A believable human avatar is
a 5–80 MB rigged mesh with textures, usually under a licence that forbids redistribution. No
amount of Rust makes a procedural sphere look like a person. So the honest scope: Tether is
the **runtime** for user-supplied avatars (VRM for humans, glTF/GLB for animals), with
procedurally generated stand-ins that look *intentional*, not photoreal.

**PLAN.** Four decisions, recorded rather than assumed:

| Decision | Why |
| --- | --- |
| Vendor three.js r187 (824 KB, MIT) | A tool whose selling point is "no network code" cannot fetch its renderer from a CDN |
| VRM/glTF at runtime, no baked art | Licences, repo size, and the user's right to their own character |
| Built-ins are *parameters*, not meshes | A few hundred bytes, infinitely customisable, no licence mess |
| Avatars are untrusted input | Downloaded files get audited before they are decoded |

**BUILD.** Math first (`gui/avatar-math.js`, 24 tests), then the renderer, then the wiring —
`Model` in `pet.json`, `src/avatar/audit.rs` for security, `src/avatar/builtin.rs` for presets.

**VERIFY.** Rust: 162 tests, clippy clean. JS: 24 tests. Visual: a headless browser renders each
species and the screenshots are inspected — because a pose bug that produces `NaN` renders as
*nothing at all*, and no unit test would have caught it. Three separate bugs of exactly that
shape were found this way.

**REVIEW.** The audit module exists because of the attacker pass: an avatar is a zip bomb, a
path traversal, a tracking beacon and a metadata leak wearing a cute hat.

**SHIP.** Everything above is committed; the renderer is optional, lazy, and absent from the
capture binary.

---

## What the methods did *not* do

Being explicit about the limits of the process is part of using it honestly:

- No method can make a low-poly procedural figure look photoreal. The docs say so; the ADR says
  so; the preview shows the real thing rather than a concept render.
- The Linux capture path is clipboard-only. That is a documented security property, not a gap
  the checklist failed to notice.
- Cross-platform binaries are produced by CI from a matrix; they were not built or run locally,
  and the docs do not pretend otherwise.

---

## Round three: design, weight, and the Flow question

The skills were applied again to three problems, and in each case the method changed the answer.

### `/constraints` → the design system is a test, not a style guide

Addy's rule is *"decide it once, enforce it everywhere"*. A visual identity written only in a CSS
file is not a decision, it is a suggestion: within two commits someone pastes a gradient and the
product looks generated again.

So the design language lives in `gui/theme.css` **and** in `tests/design.rs`, which fails the build
if any shipped page contains a decorative gradient, a frosted-glass panel, an AI-violet
(`#a78bfa`, `#8b5cf6`, `#7c3aed`), a pill radius, or a network scheme. The test is not a
formality — it failed three times while the pages were being rewritten, which is exactly the
job.

### Adversarial review applied to the *interface*

The same "think like a critic" pass that found security bugs was turned on the UI. The finding:
the interface had every fingerprint of a generated product — near-black plum, violet-to-pink
gradients, 12–16 px cards on blurred glass, emoji standing in for icons, everything centred and
aligned to nothing. Replaced with a language borrowed from things made before AI existed: Swiss
technical manuals, oscilloscope panels, PCB legends, train timetables. Paper, one signal red,
hairlines, monospace for numbers.

### `grill-me` on the credits claim

The user asked: "50 credits a day is enough, so why not just use Flow?" The honest answer needed
research, not agreement. Grilling it produced the actual arithmetic:

| Question | Answer |
| --- | --- |
| Is 50/day real? | Yes, for accounts without a Google AI plan. |
| Is one character one generation? | No — eight clips, plus retries. |
| What does that cost? | ~12 generations → 120 credits at Veo Lite → **3 days**. |
| Can Quality be used free? | No: 100 credits per generation is more than a whole day's allowance. |
| What breaks a plan? | No rollover; the clock starts on first generation; video pauses ~14:00–17:00 UTC; cost is per *generation*, not per click; visible watermark in India, South Korea, Vietnam. |

That became the Studio's **first tab** — a planner with a live meter — because a user who cannot
see the cost will waste a day's credits learning it. The answer is "yes, for free, over three to
five days", and the tool now says so with numbers.

### VERIFY on the word "optimised"

"Ultra lightweight" was a claim without a measurement. So it was measured, and the measurements
were written down with the commands that produce them (`docs/weight.md`), including the
optimisations that were **rejected**.

The number that settled the architecture:

| Path | JavaScript the user must run | Gzipped |
| --- | --- | --- |
| Clip pack (the realistic path) | `gui/clip-player.js` | **2.8 KB** |
| Procedural 3D (three.js) | `gui/vendor/three-bundle.js` | **210 KB** |

74×, for characters that look worse. That is why the renderer is optional and separate, and it is
a fact rather than an opinion because someone ran `gzip`.

### The bug the checklist caught that the tests did not

Chasing size, `clap` was built with `default-features = false`. The binary shrank 74 KB, all 187
tests passed, and **`--help` had stopped existing**. The tool could no longer explain itself.

Fixed (`std, help, usage`; net saving 24 KB), and then the gap that allowed it was closed:
`tests/cli_contract.rs` asserts `--help` exits successfully, lists every command, and that each
subcommand prints usage. A footprint win that deletes the manual is not a win, and the difference
between noticing and not noticing was running the binary rather than trusting the suite.

### `/ship` discipline

Each round ends with a commit that states what was measured, what was rejected, and what broke.
The commit messages are part of the deliverable: they are the only place a future reader learns
*why* `opt-level = "z"` beat `"s"` by 113 KB, or why the clip path exists at all.
