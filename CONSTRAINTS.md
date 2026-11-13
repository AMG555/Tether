# Constraints

Last reviewed: 2026-11-13.

All selected quality gates block completion when they fail. Metrics without a baseline are measured first; no arbitrary threshold is invented. A missing checker is a release blocker, not a pass.

## Floor

- Never log or expose the recovery key or saved draft text except when the user explicitly views or copies a draft.
- No new skipped/deleted tests, suppression comments, empty error handlers, or unimplemented stubs to make checks pass.
- Preserve offline behavior, encryption, capture policy, and explicit user consent.
- Report build and validation failures; do not substitute a successful-looking fallback.

## Enforced dimensions

| Dimension | Rule | Check | Runs at |
|---|---|---|---|
| Test coverage | Measure the existing Rust/JavaScript baseline before implementation; add focused tests for new behavior; hold or improve measured coverage | `cargo llvm-cov` and the desktop package coverage script (to be wired before the first release) | Task end and release |
| Security scanning | Block high/critical dependency advisories; enforce existing Cargo license/ban/advisory policy; review new frontend dependencies | `cargo deny check` and `npm audit --audit-level=high` (desktop package) | Dependency changes and release |
| Performance | Measure installed package size, cold startup, and idle memory per OS; no regression from the first passing baseline without user approval | `npm --prefix desktop run check:perf` | Release |
| Accessibility | Zero critical or serious accessibility findings on app screens | `npm --prefix desktop run check:a11y` | UI changes and release |
| Architecture | Keep UI-to-Rust access behind the app API facade; enforce the approved capability dependency direction | `npm --prefix desktop run check:architecture` | Task end |

## Measured baselines

| Metric | Baseline | Direction |
|---|---|---|
| Test coverage | Core: 208 Rust tests (175 unit + 33 integration); GUI: 61 tests; Desktop: 19 tests | Must not regress |
| Desktop package size, cold startup, idle memory | Windows: 2.58 MB binary, <100ms startup, 7.77 MB private / 38.51 MB working set | Record per OS, then must not regress |

The fast, focused local checks should complete within 90 seconds. Full per-platform builds may exceed that budget and must be reported separately. Do not claim a gate is enforced until its command exists and has been verified against a failing case and a clean case.
