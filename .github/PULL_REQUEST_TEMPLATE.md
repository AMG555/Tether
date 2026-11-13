## Description

<!-- Briefly describe the changes in this pull request and the problem they solve. -->

## Type of Change

- [ ] Bug fix (non-breaking change fixing an issue)
- [ ] New feature (non-breaking change adding functionality)
- [ ] Performance optimization
- [ ] Documentation update
- [ ] Security hardening

## Invariant & Quality Checklist

Please ensure the following guarantees remain intact before requesting review:

- [ ] **100% Offline Invariant:** No network calls, socket connections, or external HTTP dependencies introduced.
- [ ] **Zero-Telemetry:** No analytics, crash reporting, tracking tokens, or phone-home mechanisms.
- [ ] **Encryption at Rest:** Sensitive data encrypted with AES-256-GCM; blind indices preserved.
- [ ] **Secret Redaction:** Passwords, API tokens, card numbers, and blocked apps filtered before disk writes.
- [ ] **Binary Size Budget:** Release CLI binary stays under 2.75 MiB; desktop binary under 3.0 MiB.
- [ ] **Tests Pass:** `cargo test --all-targets` passes on MSRV Rust 1.75+.
- [ ] **Frontend & GUI Tests:** `npm test` and `node --test gui/*.test.js` pass with zero failures.
- [ ] **Accessibility:** `npm --prefix desktop run check:a11y` passes with 0 violations.
- [ ] **Architecture Budget:** `npm --prefix desktop run check:architecture` passes clean.
- [ ] **No Dead Code / Linter Clean:** `cargo clippy --all-targets -- -D warnings` and `cargo fmt --check` pass.

## Testing Performed

<!-- Describe manual tests, simulated crashes, or platform-specific verifications (Windows / macOS / Linux). -->
