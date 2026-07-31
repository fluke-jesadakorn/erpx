# G12 mobile verification probe

Date: 2026-07-27  
Mode: focused, read-only certification probe

## Verdict

This probe does **not** certify G12 mobile completion. Real Apple Keychain
adapters are now wired at source level and the iOS target compiles, but no Android
or iOS application was packaged, launched, or exercised on a simulator or
physical device. Android Keystore, notifications, offline/conflict recovery,
camera/scanner flows, accessibility, and mobile approval journeys therefore
remain unverified.

## Configuration inspected

- `erpx/apps/console/Cargo.toml` defines Dioxus `web`, `desktop`, and `mobile`
  features. The workspace pins the Rust `dioxus` dependency to `0.7.3`.
- `erpx/crates/ui/src/session_target.rs` selects Web, Desktop, Android, or iOS
  from the compilation target. iOS and macOS use the system Keychain; Web,
  Windows/Linux desktop, and Android remain fail-closed until their target-owned
  protected adapters exist.
- `erpx/apps/console/src/product_completion.rs` exposes a target-neutral mobile
  handoff that reports native certification as required and does not claim
  certification.
- `.github/workflows/erpx-targets.yml` has CI jobs for
  `aarch64-apple-ios` and `aarch64-linux-android`, each running the same
  mobile-feature `cargo check` used below. A workflow definition is not local
  device evidence.

## Commands and results

Cargo commands were run from `/Users/fluke/Desktop/Work/folio/erpx`.

| Command | Result |
|---|---|
| `rustc --version` | `rustc 1.91.1 (ed61e7d7e 2025-11-07)` |
| `cargo --version` | `cargo 1.91.1 (ea2d97820 2025-10-10)` |
| `rustup show active-toolchain` | `stable-aarch64-apple-darwin (default)` |
| `rustup target list --installed` | `aarch64-apple-darwin`, `aarch64-apple-ios`, `aarch64-linux-android`, `wasm32-unknown-unknown` |
| `cargo check -p erpx-console --no-default-features --features web` | PASS; dev profile finished in 37.47s |
| `cargo check -p erpx-console --no-default-features --features desktop` | PASS; dev profile finished in 13.81s |
| `cargo check -p erpx-console --no-default-features --features mobile` | PASS; dev profile finished in 39.29s |
| `cargo check -p erpx-console --target aarch64-apple-ios --no-default-features --features mobile` | PASS; dev profile finished in 13.96s |
| `cargo check -p erpx-console --target aarch64-linux-android --no-default-features --features mobile` | PASS; dev profile finished in 12.56s |
| `cargo test -p erpx-console --bin erpx-console qa_completion::tests` | PASS; 11 passed, 0 failed |
| `cargo test -p erpx-ui --lib session_target` | PASS; 5 passed, 0 failed |
| `cargo check -p erpx-ui --target aarch64-apple-ios --offline --no-default-features` | PASS; real iOS Keychain adapter type-checks |
| host macOS `cargo check -p erpx-ui --offline --no-default-features` | PASS; real macOS Keychain adapter type-checks |
| `cargo check -p erpx-console --target aarch64-apple-ios --no-default-features --features mobile --offline` | PASS; console compiles with the iOS adapter |

The focused tests are target-neutral contract tests. They do not execute an
Android or iOS binary.

## Local native tooling and device findings

Read-only tool discovery from the Folio root produced:

- `dx --version`: `dioxus 0.7.9 (bfcc111)`.
- `trunk --version`: `trunk 0.21.14`.
- `xcode-select -p`: `/Applications/Xcode.app/Contents/Developer`.
- `xcodebuild -version`: Xcode `26.6`, build `17F113`.
- `xcrun --sdk iphoneos --show-sdk-path` resolved to
  `/Applications/Xcode.app/Contents/Developer/Platforms/iPhoneOS.platform/Developer/SDKs/iPhoneOS26.5.sdk`.
- `xcrun --sdk iphonesimulator --show-sdk-path` resolved to
  `/Applications/Xcode.app/Contents/Developer/Platforms/iPhoneSimulator.platform/Developer/SDKs/iPhoneSimulator26.5.sdk`.
- `xcrun simctl list devices available` returned no available simulator
  devices.
- `ANDROID_HOME` is set to `/Users/fluke/Library/Android/sdk`, but that
  directory is absent. `ANDROID_SDK_ROOT` is unset.
- `adb`, `emulator`, `sdkmanager`, and `avdmanager` were not found on `PATH`.

The installed Rust targets are sufficient for `cargo check`, but they do not
provide Android SDK/NDK packaging or an iOS simulator/device. No device
execution evidence was available locally.

## Evidence boundary

The passing checks establish source-level target selection and Rust
cross-target compilation only. They do not establish:

- a Dioxus-produced Android APK or iOS app bundle;
- installation, launch, or navigation on Android, iOS, or a simulator;
- Android Keystore or iOS Keychain-backed session persistence on a real app
  install (the iOS source adapter is present, but device execution is not);
- native push notifications, deep-link return, camera/scanner, offline drafts,
  conflict recovery, or mobile MFA approval;
- performance, accessibility, or revocation behavior on a real mobile client.

The shared session boundary now uses a bounded internal envelope and the iOS
adapter stores that envelope through the Apple system Keychain. It never writes a
plaintext file, browser value, URL, or log. Android remains explicitly
unsupported rather than falling back to plaintext. Accordingly, G12 remains
open for mobile certification. No external target,
device, production directory, command owner, Folio write path, or cutover was
activated by this probe.

## Required follow-up evidence

Provide a credentialed Android SDK/NDK and an emulator or physical device, plus
an available iOS simulator or physical device. Android also needs the
target-owned Keystore bridge wired before certification. Then run the repository's
target-specific Dioxus packaging and launch flow, capture installation and
role-journey results for both platforms, and separately verify secure storage,
notifications, offline/conflict recovery, deep links, accessibility, and
mobile approvals. Those runs must be recorded as device evidence; successful
`cargo check` alone must not be promoted to certification.

## Ownership and rollback

This packet records source changes in `crates/ui/src/auth.rs`,
`crates/ui/src/session_target.rs`, and the iOS-only dependency declarations in
`crates/ui/Cargo.toml`. Rollback is a normal source revert; it does not touch
runtime state, Folio, directories, command ownership, or production cutover.

## Coordinator continuation — Android Keystore adapter — 2026-07-28

The Android session boundary is now implemented rather than merely fail-closed. A target-owned
JNI bridge obtains the live Dioxus/NDK Android context, generates or retrieves an AES key from
the `AndroidKeyStore` provider, encrypts the bounded internal session envelope with
`AES/GCM/NoPadding`, and synchronously commits only the base64 ciphertext to private
`SharedPreferences`. Load authenticates and revalidates the envelope before rebuilding a
`StoredSession`; tampered, malformed, missing-key, or unavailable-context cases fail closed.
Sign-out removes the protected ciphertext, and the console uses the adapter for supported-target
restore, login persistence, refresh rotation, and revoke cleanup. Web and unsupported desktop
targets still have no local fallback.

Source-level evidence:

- `cargo test -p erpx-ui --offline --no-default-features` — PASS; 84 UI tests plus target
  integration suites.
- `cargo check -p erpx-ui --target aarch64-linux-android --offline --no-default-features` — PASS.
- `cargo check -p erpx-console --target aarch64-linux-android --offline --no-default-features
  --features mobile` — PASS.
- `cargo check -p erpx-console --target aarch64-apple-ios --offline --no-default-features
  --features mobile` — PASS.
- host web/desktop console checks — PASS after the persistence wiring.

This is still source and cross-target compilation evidence, not device certification. The local
machine has no Android SDK/NDK, `adb`, emulator, or available iOS simulator. Real Android
Keystore round-trip, app installation, native notification/deep-link/offline/camera/accessibility
flows, and mobile MFA approval remain queued until device tooling is supplied. No production
directory, command owner, Folio write path, or cutover was activated.
