# G1-005-AUTH-SESSION-UI evidence

## Delivered behavior

- Added typed `SessionState` values for signed out, authenticating,
  authenticated, refreshing, revoking, expired, revoked, offline, and error
  states.
- Added a `SessionTransport` seam for login, rotating refresh, and revoke.
  HTTP/Web/native adapters remain application-owned.
- Added `SessionController`, which keeps access and refresh tokens only in
  process memory. The controller, issued-session DTO, and token wrappers have
  redacted debug output; safe shell metadata contains no bearer, refresh token,
  password, cookie, or private key.
- Added refresh metadata preservation so read-only and customer-approved
  support banners survive token rotation.
- Replaced the console's hard-coded actor/session transport metadata with
  controller-derived access-token, actor, and session metadata. Unauthenticated
  console routes are now gated by the shared route boundary instead of sending
  provisional credentials.
- Existing `AppShell`/`RouteStateBoundary` surfaces persist read-only, support,
  impersonation, sign-in-required, expiry, revoked, and offline states.

## Secure storage strategy

Tokens are not serializable client state and are never written to localStorage,
sessionStorage, IndexedDB, URL parameters, Dioxus route state, or UI metadata.
The only retained copies are the controller's in-memory token wrappers and the
short-lived transport request value. A future platform adapter must preserve
this memory-only policy and use platform secure storage only if an explicit
threat-model decision approves it.

## Verification

- `cargo fmt --manifest-path erpx/Cargo.toml --all` — PASS.
- `cargo test --manifest-path erpx/Cargo.toml -p erpx-ui` — PASS; 15 tests.
- `cargo check --manifest-path erpx/Cargo.toml -p erpx-console --no-default-features --features web` — PASS.
- `cargo check --manifest-path erpx/Cargo.toml -p erpx-console --no-default-features --features desktop` — PASS.
- `cargo clippy --manifest-path erpx/Cargo.toml -p erpx-ui -p erpx-console --all-targets -- -D warnings` — PASS.

The UI tests cover redacted token/debug output, explicit login/refresh/revoke
transitions, unauthorized expiry, safe metadata, read-only/support banner
state, and preservation across refresh.

## Residual gaps

- No concrete browser/desktop/mobile HTTP adapter is installed yet; the
  existing server exposes `/auth/login`, `/auth/refresh`, and `/auth/revoke`,
  but target-specific networking and login UI remain integration work.
- Android/iOS secure keystore/keychain integration, biometric/passkey UX,
  background refresh scheduling, and deep-link reauthentication handoff remain
  open.
- Server authorization, entitlement, scope, support-capability, and billing
  policy remain authoritative backend concerns and are not duplicated here.

## Rollback

Remove `crates/ui/src/auth.rs`, its exports, the console boundary/header
integration, and this evidence file. No server, database, protobuf, Folio, or
secret state was changed.
