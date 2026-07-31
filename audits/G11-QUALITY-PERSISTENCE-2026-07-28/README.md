# G11 Quality inspection persistence slice — 2026-07-28

## Delivered

Quality inspections now have a versioned authoritative tenant aggregate with
scope-bound lifecycle commands. The SQLx write adapter persists the source
row, snapshot, complete outbox envelope, and command idempotency receipt in
one transaction. The disposable read model validates the authoritative event
identity and envelope, enforces contiguous versions, records checkpoints,
supports reconciliation and clear/rebuild, and is read through a separate
tenant read pool. The durable tenant projection scheduler can run the Quality
incremental or rebuild worker through `ERPX_TENANT_PROJECTION_KIND=quality`.

Authenticated `QualityService` transport provides create, start/result/release/
cancel, and read operations. Server authorization requires the Quality
entitlement plus `quality.write` or `quality.read`, explicit product scope,
tenant routing, customer-scope containment, and customer audit publication.
The target-neutral UI shell exposes authenticated read-only `/quality`
navigation; mutation authority remains server-owned.

The follow-on read surface now exposes `ListQualityInspections` with bounded
keyset pagination over `(projected_at, inspection_id)`. It validates holding,
company, and optional branch containment, uses only the tenant read pool,
publishes a list-read audit event, and returns typed identity/status/version
metadata alongside bounded inspection JSON. The Dioxus Quality route supplies
an explicit scope form, authoritative table, safe error states, and next-page
navigation without local fixtures or client mutation authority.

## Evidence

- `cargo test -p erpx-domain --test quality_inspection --offline` — 6 passed.
- `cargo test -p erpx-infrastructure --test quality_persistence --offline --no-run` — PASS; the live two-database test is ignored until disposable URLs are supplied.
- `ERPX_TEST_TENANT_WRITE_DATABASE_URL=... ERPX_TEST_TENANT_READ_DATABASE_URL=... cargo test -p erpx-infrastructure --test quality_persistence --offline -- --ignored --nocapture --test-threads=1` — 1 passed against fresh disposable PostgreSQL write/read databases; both were dropped afterward.
- `cargo test -p erpx-worker quality_projection --offline` — PASS.
- `cargo test -p erpx-migrator --test g11_product_context_acceptance --offline` — 8 passed.
- Quality read-list implementation checks — `cargo check -p erpx-server --offline`,
  `cargo check -p erpx-console --no-default-features --features desktop --offline`,
  and server cursor transport tests — PASS.
- The live two-database Quality acceptance now verifies the typed read page and
  scope filtering; fresh disposable databases passed and were dropped.
- `cargo check -p erpx-server --offline` — PASS.
- `cargo test -p erpx-migrator --test g13_parallel_acceptance --offline` — 9 passed.
- `cargo test -p erpx-migrator --test g14_local_compatibility --offline` — 5 passed.
- `ERPX_G14_LOCAL_DIRECTORY_DATABASE_URL=... cargo test -p erpx-identity --test directory_sqlx_acceptance --offline -- --ignored --nocapture --test-threads=1` — 3 passed against a fresh disposable PostgreSQL database; it was dropped afterward.
- `crates/migration/build.rs` now watches all four SQLx migration directories, preventing stale embedded migration sets on incremental builds; the live Quality run applied migration 0053 successfully.
- Console Dioxus web, desktop, host-mobile, iOS, and Android target checks — PASS.
- Folio `bun run lint` — PASS with four pre-existing n8n warnings; `bunx tsc --noEmit` — PASS.
- Folio port-3004 probes — `/` 200 and `/operations` 307.

## Queued acceptance

The ignored live test `quality_write_outbox_projection_read_reconcile_and_rebuild`
requires disposable tenant write/read PostgreSQL URLs. Vendor/device and
production cutover evidence remain outside this local packet. The mandated
ignored workspace sweep currently exits 101 with 71 environment-gated targets,
including missing disposable SQLx/MinIO/PITR resources, credentialed Generic
LDAP/RFC 4533, AD DS/DirSync, UCS Provisioning Service, operator-revocation,
physical mobile, and explicit production approval evidence. No customer
directory was activated, no command owner changed, and no Folio write was
retired.

## Rollback boundary

Rollback clears only the disposable Quality read rows/checkpoints and replays
the tenant write outbox. Authoritative inspection rows, snapshots, events, and
idempotency receipts are not deleted by projection rollback.
