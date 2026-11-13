# Implementation Plan: Tether audit and cleanup

## Overview
This session audited the active project in `savepoint/`, verified whether the app was complete and working, fixed the thread-safety issue in the capture backend contract, and removed stale workspace artifacts that were not part of the active project.

## Architecture decisions
- The live project is the Rust crate under `savepoint/`; the other directories under the workspace root are historical or duplicate artifacts.
- The capture engine uses platform-specific `Source` backends; those backends must own their sink callback safely across threads.
- The release binary and full test suite are the correct verification gate for whether the project is working.

## Task list
- [x] Confirm the actual project root and repository status
- [x] Validate the Rust toolchain and run the project test suite
- [x] Fix the thread-safe sink contract in the capture backend
- [x] Verify the release build succeeds
- [x] Remove stale files that are not part of the active project
- [x] Write project instructions for future agent sessions

## Verification
- [x] `cargo test --quiet` passed (175 tests in the core suite, plus additional module tests)
- [x] `cargo build --release --quiet` passed
- [x] The active code under `savepoint/` remains the only live project tree

## Risks and mitigations
| Risk | Impact | Mitigation |
|---|---|---|
| Duplicate project trees in the workspace | Confusing active vs stale code | Keep the project under `savepoint/` and remove archive/history copies |
| Hidden compile break in platform hook layer | Runtime failure / no capture | Re-run cargo test and release build after the sync fix |

## Open questions
- None at this point; the project is in a working state in this workspace.
