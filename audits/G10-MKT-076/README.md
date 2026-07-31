# G10-MKT-076 — Marketing provider account command surface

Date: 2026-07-28

## Implemented boundary

The Marketing Channels console route now exposes the existing authenticated
provider-account commands for a concrete holding scope:

- configure a provider account and its enabled channel routes;
- record an external verification reference and optionally request activation;
- suspend or reactivate the exact provider account.

The client validates UUIDs, holding scope, bounded channel lists, bounded JSON
route metadata, positive/empty-default priority, and opaque secret-manager
references. Only safe provider account state, onboarding state, version,
replay, and correlation metadata are rendered. Raw credentials, secret
material, provider payloads, and verification secrets do not cross the client
display boundary.

The server remains authoritative for provider-account scope, entitlement/RBAC,
channel/route validation, secret-manager references, verification gating,
optimistic state transitions, control-plane SQLx persistence, audit/outbox,
worker account routing, idempotency, reconciliation, and rollback. No account
was activated and no external provider was contacted by this packet.

## Evidence

- `cargo test -p erpx-console --offline --no-fail-fast --quiet`: 105 passed.
- `cargo clippy -p erpx-console --all-targets --offline -- -D warnings`:
  passed.
- Console web, desktop, host-mobile, iOS, Android, shared UI iOS/Android,
  and Android bridge target checks: passed.

The mandated ignored sweep remains fail-closed at exactly 66 queued
missing-resource/certification targets and is not completion evidence.

## Rollback and ownership

Reverting this packet removes only the console controls and tests. Server
provider-account ownership and persistence remain unchanged. No provider
activation, directory activation, Folio write retirement, or production
cutover occurred.
