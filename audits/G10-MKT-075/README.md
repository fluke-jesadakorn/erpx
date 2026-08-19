# G10-MKT-075 — Marketing preview and custom-domain command surface

Date: 2026-07-28

## Implemented boundary

The shared Dioxus Marketing/CRM console now exposes bounded authenticated
commands over the existing authoritative `MarketingContentService` RPCs:

- issue and revoke an expiring preview for an exact template revision;
- issue a custom-domain verification challenge;
- verify the supplied domain proof; and
- bind a verified domain to an exact published template revision.

The client accepts only explicit holding/holding/company/branch scope, UUID
identifiers, positive template versions, bounded RFC3339/proof text, and
server-issued command context. The client replaces command identity,
idempotency, correlation, actor, audit purpose, and scope through the typed
request builder. It renders only bounded receipt metadata and never displays
preview tokens, challenge material, raw submitted form values, persistence
payloads, or provider credentials.

The server remains authoritative for entitlement/RBAC, holding routing, domain
and template publication state, hash-only secret persistence, SQLx
transactions, outbox/events, idempotency, expiry worker behavior,
reconciliation, and rollback. No browser command is treated as proof of
publication or verification.

## Evidence

- `cargo test -p erpx-console --offline --no-fail-fast --quiet`: 104 passed.
- `cargo clippy -p erpx-console --all-targets --offline -- -D warnings`:
  passed.
- Console web, desktop, host-mobile, iOS, Android, shared UI iOS/Android,
  and Android bridge target checks: passed.
- Full workspace formatting, warnings-denied Clippy, and non-ignored tests:
  passed on the current tree.

The required ignored workspace sweep remains fail-closed at exactly 66
missing-resource/certification targets. Those are not claimed as passes.

## Rollback and ownership

This packet changes only the console adapter and its tests. Reverting the
console packet removes the client controls while leaving server ownership and
authoritative persistence unchanged. No provider account was activated, no
custom domain was bound, no Folio write path was changed, and no production
cutover was performed.
