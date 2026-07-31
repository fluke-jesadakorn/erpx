# G10-MKT-060 — Application-to-SQLx Marketing write adapter

Date: 2026-07-26

## Scope delivered

`SqlxMarketingContentWriteAdapter` now implements the application-layer `MarketingContentWritePort` for published form submissions, preview issue/revoke/expiry, domain challenge issue/verify, and domain binding. It owns a clone of the existing SQLx store and converts persistence receipts into transport-neutral application receipts. Persistence errors are mapped to one redacted repository message; database URLs, SQL details, bound values, raw form values, idempotency keys, and secrets are not formatted into application errors.

The adapter is exported by infrastructure and composed into the authenticated Marketing Tonic server commands. The server constructs application commands after authoritative authentication, scope, entitlement, and RBAC checks; the adapter/store retain authoritative aggregate re-reads, forced-RLS transactions, idempotency, optimistic concurrency, audit/outbox persistence, and tenant database separation. A disposable two-database proof now exercises the application adapter on the form-submission path alongside the preview/domain/read-model lifecycle.

## Evidence

- `cargo fmt --all --check` — PASS.
- `cargo clippy -p erpx-application --all-targets -- -D warnings` — PASS.
- `cargo test -p erpx-application --quiet` — PASS; 11 tests.
- `cargo clippy -p erpx-infrastructure --all-targets -- -D warnings` — PASS.
- `cargo test -p erpx-infrastructure --quiet` — PASS; 161 tests passed, 4 ignored.
- `cargo clippy -p erpx-server --all-targets -- -D warnings` — PASS.
- `cargo test -p erpx-server --quiet` — PASS; 79 tests.
- Focused two-database Marketing live proof — PASS; application-port form submission, preview authorization/expiry, domain verification/binding, replacement read rebuild, forced-RLS inspection, and exact cleanup. Temporary databases were dropped and residue was checked.

## Explicitly open

The worker expiry path still uses its dedicated worker adapter rather than the application port; a live Tonic all-command authorization proof and complete server/worker composition audit remain open. Semantic form-field validation, privacy retention/erasure, public abuse controls, SSR/Dioxus UI, live MinIO rendering, host-only control-plane domain discovery, durable retry/dead-letter persistence, campaigns/journeys/CRM attribution, remaining ERP contexts, migration/cutover, and Folio write retirement remain open.

No Folio behavior or Folio writes changed.
