# G2-001 evidence — persisted identity and session enforcement

## Delivered behavior

- `erpx-identity` provides Argon2id passwords, hashed recovery codes,
  encrypted TOTP envelopes, bounded WebAuthn challenge/passkey seams, five-minute
  half-open access tokens, and rotating hashed refresh tokens.
- `SqlxIdentityStore` persists principals, password credentials, devices, and
  sessions only on the control write database.
- Recovery-code issuance replaces prior hashes atomically; consumption locks
  all candidate rows and uses a `consumed_at IS NULL` guard for one-time use.
- Device listing returns metadata only, and device revocation atomically
  revokes the device and all attached sessions.
- Login creates a device and session, refresh rotates the hash in a row lock,
  refresh-token reuse revokes the whole session, and access authorization checks
  principal status, device revocation, session revocation, and refresh lifetime.
- Passkeys are persisted with principal/RP binding, credential-ID digest and
  raw provider credential data, idempotent revocation, and an atomic
  compare-and-set sign counter that rejects stale or rollback updates.
- The server exposes `/auth/login`, `/auth/refresh`, `/auth/revoke`,
  `/auth/devices`, `/auth/devices/{id}/revoke`, `/auth/recovery-codes`, and
  `/auth/recovery-codes/consume`. Tonic transport verifies token
  signature/expiry/actor/session attribution and handlers perform the
  asynchronous write-side session lookup.
- The recovery handoff also exposes `/auth/recovery-sessions/reset-password`.
  It replaces the Argon2id password, revokes earlier sessions, creates a fresh
  device/session, and consumes the recovery bearer in one write-side
  transaction.

## Independent evidence

Run from `erpx/`:

```text
cargo test -p erpx-identity
cargo clippy -p erpx-identity --all-targets -- -D warnings
cargo test -p erpx-identity --test sqlx_identity -- --ignored --nocapture
cargo check -p erpx-server
cargo clippy -p erpx-server --all-targets -- -D warnings
cargo test -p erpx-server -- --nocapture
```

The ignored tests were run against explicitly named disposable PostgreSQL
databases and those databases were dropped afterward. A disposable server boot
also returned 200 for `/healthz` and `/readyz`, and exercised login, refresh,
and revoke without printing bearer or refresh values.

### Independent passkey rerun — 2026-07-24

```text
ERPX_TEST_CONTROL_WRITE_DATABASE_URL=postgres://localhost/erpx_identity_w_20260724_a \
ERPX_TEST_CONTROL_READ_DATABASE_URL=postgres://localhost/erpx_identity_r_20260724_a \
cargo test -p erpx-identity --test sqlx_identity -- --ignored --nocapture
```

Result: PASS — 5 passed, 0 failed, covering write-side-only schema,
sessions, recovery/device revocation, durable rate limits, and passkey
binding/counter atomicity. Both databases were explicitly dropped afterward.

## Remaining boundary

No password registration endpoint is exposed. Recovery issuance now returns
newly generated plaintext codes exactly once in its authenticated issuance
response; persisted rows contain only Argon2id hashes. A client-side
no-store/display integration and recovery-code rotation UX remain required.
Recovery/device HTTP workflows and one-time recovery password replacement are
integrated, but rate limits, lockout/audit policy, a production WebAuthn
ceremony provider, secure platform-local token storage, and full
role/entitlement/support-capability intersection remain open.
