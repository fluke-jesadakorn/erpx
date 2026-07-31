# G6-G12-UI-012 — Authenticated Platform backup-history surface

## Objective and dependency contract

Connect `/platform/backups` to the existing authenticated
`PlatformOperationsService::ListTenantBackupJobs` read boundary. The client
consumes the already audited tenant registry and performs a bounded fan-out of
at most 100 tenant histories, with at most 100 rows per tenant. No new write
authority or database access is introduced in the Dioxus client.

## Delivered

- WebAssembly gRPC-Web and native Tonic adapters carry the access token,
  authenticated actor/session metadata, platform query context, and bounded
  request size.
- The display model allow-lists tenant/job identity, kind, state, availability,
  attempt counters, verification state, error-presence state, and timestamps.
- Backup object references, restore targets, physical database keys,
  credentials, and raw error text are not represented by the client model or
  rendered by the surface.
- Loading, authentication-denied, error, empty, and read-only states are
  explicit. The surface does not expose scheduling, restore, deletion, or
  activation controls.

## Evidence

- `cargo test -p erpx-console` — PASS; 20 tests, including the backup display
  redaction test.
- `cargo fmt --all --check` — PASS.
- `cargo clippy --workspace --all-targets -- -D warnings` — PASS.
- `cargo test --workspace` — PASS; the explicitly disposable PostgreSQL tests
  remain ignored without their required databases.
- `cargo check -p erpx-console --no-default-features --features web` — PASS.
- `cargo check -p erpx-console --no-default-features --features desktop` — PASS.
- Folio `bun run lint` — PASS with four pre-existing warnings in
  `infra/scripts/start-n8n.js`; zero errors.
- Folio `bunx tsc --noEmit` — PASS.
- PostgreSQL readiness — PASS; Folio `/operations` returned HTTP 307.
- `git diff --check` — PASS.

## Open boundary

Backup scheduling/restore commands, physical MinIO/PITR evidence, server-side
backup-history pagination, filtering, native secure storage, mobile target
evidence, and the remaining Platform Operations routes remain open.

## Rollback

Revert the backup-history client model, bounded adapters, resource, surface,
test, and this audit record. Preserve the existing authenticated server RPC
and shared Platform Operations route registry.
