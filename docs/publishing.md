# GitHub publishing checklist

The repository URL and public metadata are configured for `AMG555/Tether`. Before sharing the
project, confirm that the repository contains the intended source and that every public install
instruction works. Do not announce a download until a verified release exists.

## 1. Publish the source

This project folder may not contain Git history. Check before using Git commands:

```sh
git status --short --branch
git remote -v
```

If there is no Git repository, initialize it only after reviewing the exact files to publish:

```sh
git init -b main
git add --all
git diff --cached --check
git diff --cached
git commit -m "chore: publish Tether source"
git remote add origin https://github.com/AMG555/Tether.git
git push -u origin main
```

Exclude build outputs, `node_modules`, local vaults, recovery keys, and captured text. The existing
`.gitignore` excludes the main generated and sensitive paths. Before sharing the link, verify the
GitHub page shows the README, MIT license, screenshots, and working links. The repository
description and topics are configured for unsaved-text recovery; the companion and pet
customization are secondary.

## 2. Verify the source build and supported behavior

Run the checks on the relevant native platforms; do not infer Windows or macOS capture support
from a Linux build:

```sh
cargo +1.75 test --all-targets
cargo fmt --check
cargo clippy --all-targets -- -D warnings
node --test gui/*.test.js
npm --prefix desktop test
npm --prefix desktop run check:a11y
npm --prefix desktop run check:architecture
sh scripts/test-install.sh
```

Be explicit about current limits: Linux capture is clipboard-only; Windows does not capture paste
contents; macOS requires Accessibility and Input Monitoring permissions. The desktop Tauri shell
is a development preview and does not yet provide recovery or pet customization.

## 3. Set the repository preview

- Keep the description and topics focused on local text recovery; avoid implying a finished GUI.
- Use `shots/terminal.png` as the social preview so the primary recovery workflow is clear.
- Add a project website only when a maintained public site exists.
- Enable Discussions only if someone is available to moderate them.

## 4. Publish and verify the first release

The installers resolve the latest GitHub release and require both a matching binary and
`checksums.txt`. After the source is pushed and release checks pass, create a version tag:

```sh
git tag -a v0.1.0 -m "v0.1.0 — first public release"
git push origin v0.1.0
```

The release workflow builds Linux x86_64 and aarch64, macOS Intel and Apple Silicon, and Windows
x86_64. Confirm all five artifacts and `checksums.txt` exist, then test the platform installer from
a clean machine. Verify the installed binary's version and run `tether setup` before announcing it.

## 5. Keep the public claims accurate

- Capture runs only when started or when the user has separately configured autostart. Secret
  filters are not infallible; link to [`docs/limitations.md`](limitations.md).
- The vault is encrypted locally and there is no network code in the CLI. This does not protect
  against a privileged attacker reading process memory, or someone using an unlocked account.
- Linux is clipboard-only and Windows paste contents are not captured.
- Companion customization works through the CLI; the native desktop shell remains incomplete.
- Do not imply that Tether generates character art. Character Studio creates prompts and validates
  local clip files; art generation happens outside the app.
