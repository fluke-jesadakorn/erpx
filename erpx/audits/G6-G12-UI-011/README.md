# G6-G12-UI-011 — Authenticated Platform projection status surface

## Delivered

- The Dioxus console calls the authenticated Tonic
  `PlatformOperationsService::ListProjectionStatuses` endpoint for
  `/platform/projections` through WebAssembly gRPC-Web and native Tonic.
- The client follows the server's bounded opaque cursor, rejects empty cursors
  on continued pages, and caps pagination at 100 pages.
- Display models allow-list tenant/holding/projection identity, kind, state,
  bounded attempts, freshness, and timestamps. Job identifiers, raw error
  text, database keys, credentials, and mutation controls are not rendered.
- Authentication-required, loading, empty, error, and read-only states remain
  explicit.

## Evidence

- `cargo fmt --all --check` — PASS.
- `cargo clippy --workspace --all-targets -- -D warnings` — PASS.
- `cargo test --workspace` — PASS; disposable PostgreSQL integration tests remain
  explicitly ignored when their required databases are not provisioned.
- `cargo check -p erpx-console --no-default-features --features web` — PASS.
- `cargo check -p erpx-console --no-default-features --features desktop` — PASS.
- Folio `bun run lint` — PASS with four pre-existing warnings in
  `infra/scripts/start-n8n.js` and no errors.
- Folio `bunx tsc --noEmit` — PASS.
- PostgreSQL readiness — PASS; Folio `/operations` returned HTTP 307.
- `git diff --check` — PASS.

## Open boundary

Projection rebuild commands, status filtering controls, Android/iOS evidence,
and the remaining Platform Operations routes remain open. This is an audited
read-only adapter and does not authorize tenant mutation.

## Rollback

Revert only the projection status client model/surface changes and this audit
record. Preserve the shared Platform Operations route and server contract.
