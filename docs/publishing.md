# Publishing this on GitHub

Everything in this repo is ready to be public except one thing: your username. Ten minutes,
start to finish.

## 1. Claim the name and the identity

```sh
sh scripts/rename-owner.sh your-github-username
```

That rewrites the `USER` placeholder in `Cargo.toml`, `README.md`, `CONTRIBUTING.md`,
`scripts/install.sh` and `scripts/install.ps1`. It is idempotent, so run it again if you change
your mind about the name.

Check nothing is left:

```sh
grep -rn 'USER/savepoint' . --include='*.md' --include='*.sh' --include='*.ps1'
```

Then push the repository. It is already a git repository with history:

```sh
git remote add origin git@github.com:your-github-username/savepoint.git
git push -u origin main
```

## 2. Fill in the repo page

GitHub shows the description and topics before anyone reads a word of the README.

- **Description:** `Crash-proof, offline, encrypted typing recovery. One 2 MB binary, zero network calls — plus a desktop companion you author yourself.`
- **Website:** leave empty until there is one.
- **Topics:** `rust` `privacy` `encryption` `crash-recovery` `keystroke` `desktop-companion` `offline` `single-binary` `sqlite` `aes-gcm`
- **Social preview:** upload `shots/companion.png`. It is the image that explains the product
  fastest.

Under Settings → General, turn on **Discussions** if you want somewhere for "how do I make a
character" questions that is not the issue tracker.

## 3. Cut the first release

The install one-liner resolves `releases/latest`, so it does nothing until a release exists.

```sh
git tag -a v0.1.0 -m "v0.1.0 — first public release"
git push origin v0.1.0
```

`.github/workflows/release.yml` then builds five targets — Linux x86_64 and aarch64, macOS Intel
and Apple Silicon, Windows x86_64 — names the assets `savepoint-<platform>-<cpu>`, generates
`checksums.txt`, and publishes. Watch the Actions tab.

**Verify the release before you tell anyone about it:**

```sh
# from a machine that is not the build one
curl -fsSL https://raw.githubusercontent.com/your-github-username/savepoint/main/scripts/install.sh | sh
savepoint setup
savepoint version
```

Confirm `checksum  ok` appears, and that the asset list has a `checksums.txt`.

## 4. Before the first public link

```sh
cargo test                      # 211
node --test gui/*.test.js       # 55
cargo clippy --all-targets -- -D warnings
sh scripts/test-install.sh      # the installer refuses bad checksums
python3 shots/capture.py        # regenerate screenshots from the real binary
```

If `capture.py` reports console errors, a page is broken — do not ship with a screenshot that
hides it.

## 5. Things worth knowing before people use it

These are not blockers, but they are the questions you will be asked:

- **Linux is clipboard-only.** Say it first. `savepoint setup` already does, and the platform table in
  the README does. Anyone who expected system-wide keystrokes on Wayland should find out from the
  README, not from a bug report.
- **The key sits next to the vault.** That is a deliberate trade — a passphrase prompt breaks
  foreground capture — and both `init` and `setup` say so loudly. Expect this to be the most
  discussed design decision in the repo. `docs/adr/0003-encryption.md` has the reasoning.
- **macOS needs two permissions.** Input Monitoring plus Accessibility. Screen recordings of
  onboarding get judged on how gracefully this is handled.
- **There is no image generator.** The studio writes prompts and validates clips; the art is
  yours. Being clear about this up front prevents "where do I download characters" issues.
- **`savepoint ask` on Linux is a terminal companion today.** The native window is the next build
  target; the engine, format and validation are done. Say "today" and mean it.

## 6. After publishing — the first three issues you will get

Worth writing the answers now, while the reasoning is fresh:

1. *"Why not just use a system clipboard manager?"* Because those are per-app, unencrypted,
   usually unlimited, and they do not survive a reboot. Savepoint is bounded, encrypted, and has a
   retention policy.
2. *"Can I sync the vault between machines?"* Not the vault itself — `savepoint backup` produces an
   encrypted archive that is safe to copy, but it is a copy, not a merge. A merge is a design
   problem, not an implementation one.
3. *"Can you add X11 keystroke capture on Linux?"* Yes, and the `Source` trait is ~80 lines.
   Someone will send the PR; review it for the same crash-safety properties as the ring.

## What is deliberately not in this repo

- **No binaries checked in.** Releases are built by CI, so a download is always traceable to a
  commit.
- **No `gui/preview.html`.** It is an early 3D experiment that does not follow the design system;
  `tests/design.rs` covers the ship set only. Delete it or re-skin it before making it
  discoverable — a page in the repo is a page someone will find.
- **No telemetry, no analytics, no issue templates that ask for your logs.** A privacy tool that
  asks for logs on first contact has already lost the argument.
