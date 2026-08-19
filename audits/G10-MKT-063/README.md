# G10-MKT-063 — Marketing profile-erasure application and Tonic authorization

## Delivered behavior

- The application layer exposes a typed, object-safe `MarketingProfilePrivacyPort`
  and `EraseMarketingProfileCommand`.
- The command validates erasure scope containment, UUIDs, bounded idempotency,
  correlation, and causation metadata before any implementation is called.
- The SQLx adapter routes the command to the holding write store, preserves the
  transaction/idempotency/redaction behavior from G10-MKT-062, and maps
  persistence details to stable redacted application failures.
- Tonic exposes `EraseMarketingProfile`, requiring authenticated identity,
  explicit holding scope, entitlement, the dedicated
  `marketing.profile.erase` permission, current billing/SoD/RBAC state, and a
  valid command context. The server records a customer-visible audit action
  and returns only a redacted profile receipt.

## Files and contracts

- Application command/port: `crates/application/src/marketing_privacy.rs`.
- SQLx adapter: `crates/infrastructure/src/marketing_privacy_application.rs`.
- Protobuf contract: `crates/proto/proto/erpx/platform/v1/platform.proto`.
- Tonic routing and authorization: `apps/server/src/main.rs`.
- Authoritative write/rebuild proof: `crates/infrastructure/tests/marketing_profile_erasure.rs`.

## Verification evidence

- Application packet: strict check/Clippy, 6 focused privacy tests, and 17
  full application tests — PASS.
- Infrastructure adapter: format, check, strict Clippy, and 2 focused adapter
  redaction/conflict tests — PASS.
- Server: check, strict Clippy, and 4 Marketing transport tests — PASS.
- Fresh disposable two-database proof — PASS; the application privacy port
  performed erasure, replay returned the original result, source/outbox/
  snapshot data remained contact-free, read rebuild from empty state remained
  redacted, and `erpx_mkt063_*` cleanup left no residue.

## Security and scope notes

No holding credentials, contact fields, persistence errors, or erasure reason
are returned through the application or Tonic receipt/error boundary. The
dedicated permission and current authoritative access evaluator are required;
platform and subtree scopes are rejected for this holding-routed mutation.
Queued delivery payload scrubbing, public abuse controls, SSR/UI, MinIO,
remaining ERP contexts, migration cutover, and Folio write retirement remain
open.

## Rollback

Disable the RPC route and retain the additive application adapter. Never
reverse a completed erasure or restore a redacted payload; the write-side
tombstone and permanent suppression remain authoritative.
