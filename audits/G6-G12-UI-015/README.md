# G6-G12-UI-015 — Mobile target compilation evidence

## Objective

Provide direct cross-target compilation evidence for the shared Dioxus client
and preserve the checks in repository CI.

## Delivered behavior

- The shared `erpx-console` mobile feature compiles for
  `aarch64-apple-ios` and `aarch64-linux-android`.
- A GitHub Actions matrix preserves both target checks on macOS and Ubuntu.
- The host mobile feature check remains available for fast local feedback.

## Verification

- `cargo check -p erpx-console --no-default-features --features mobile` — PASS.
- `cargo check -p erpx-console --target aarch64-apple-ios --no-default-features --features mobile` — PASS.
- `cargo check -p erpx-console --target aarch64-linux-android --no-default-features --features mobile` — PASS.
- `rustup target list --installed` includes both cross targets after setup.

## Open boundaries

Cross-target compilation does not prove device execution, secure Android
keystore/iOS Keychain persistence, camera/scanner integration, notifications,
offline draft synchronization, or every mobile role journey. Those remain
explicit G12/G14 work.

## Rollback

Remove `.github/workflows/erpx-targets.yml` and this evidence record. No runtime
or persistence behavior depends on the CI-only target matrix.
