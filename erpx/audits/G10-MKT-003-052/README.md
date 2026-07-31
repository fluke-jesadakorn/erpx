# G10-MKT-003 — Marketing SQLx persistence

## Delivered behavior

- Added tenant-local SQLx write persistence in `marketing_persistence.rs` for:
  - Marketing profile upsert with domain validation, canonical deterministic dedupe-key verification, and optimistic aggregate versioning.
  - Immutable consent append and revoked-consent append, both protected by a transaction-local idempotency reservation and SHA-256 request-hash binding.
  - Suppression upsert with profile existence checking, immutable identity binding, version increments, payload hashing, and idempotent replay.
  - Bounded audience membership replacement/upsert with profile existence checking, aggregate versioning, membership hashing, and atomic replacement of normalized membership rows.
  - Keyset-paginated audience membership reads from the tenant read pool, capped at 500 IDs per page.
- Every SQLx transaction installs `erpx.holding_ref` with `set_config(..., true)` before queries.
- Domain payloads are validated before serialization. Platform scope is rejected at this tenant-local boundary.
- The command idempotency ledger reserves and completes in the same write transaction; a reused key with a different operation, resource, or payload hash fails closed.
- Read tables contain projection metadata such as `source_version` and are never used by the write methods.

## Files changed

- `erpx/crates/infrastructure/src/marketing_persistence.rs`
- `erpx/crates/migration/migrations_tenant_write/0018_marketing.sql`
- `erpx/crates/migration/migrations_tenant_read/0018_marketing.sql`

The main integration export is in `crates/infrastructure/src/lib.rs`; no
domain, worker, server, application, or UI behavior is claimed by this packet.

## SQL migration details

- Existing highest tenant migration was `0017_finance.sql` on both tracks; `0018_marketing.sql` is therefore the next available version with no duplicate version in either directory.
- Write side creates profiles, immutable consents, suppressions, audience aggregate/membership state, and the scoped command-idempotency ledger.
- Read side creates separate projection tables for profiles, consents, suppressions, audiences, and audience memberships. Read rows carry `source_version` and projection timestamps.
- All new tables have `ENABLE ROW LEVEL SECURITY`, `FORCE ROW LEVEL SECURITY`, and policies whose `USING` and `WITH CHECK` clauses compare `holding_ref` to `current_setting('erpx.holding_ref', true)`.

## Tests and exact results

- `rustfmt --edition 2024 --check crates/infrastructure/src/marketing_persistence.rs` — PASS.
- `cargo clippy -p erpx-infrastructure --all-targets -- -D warnings` — PASS.
- `cargo test -p erpx-infrastructure --lib` — PASS after integration: 67 passed,
  4 disposable PostgreSQL tests ignored, including the three Marketing
  persistence pure tests.
- Migration version inspection — PASS: write versions are `0001`, `0012`, `0017`, `0018`; read versions are `0001`, `0002`, `0017`, `0018`; no duplicates.
- `cargo test -p erpx-migration --lib` — PASS after the owning migration test
  was made version-aware: 6 passed, including the Finance and Marketing
  migration/RLS assertions.

## Unfinished integration

- Infrastructure export, format, clippy, and pure tests are complete.
- Apply and verify both migrations against disposable write/read PostgreSQL databases.
- Add outbox events and worker projectors from write rows to read rows.
- Add application/server transport methods and Marketing channels, journeys, CRM, attribution, and delivery workflows.
- Update the migration count assertion in the owning migration integration packet.

## Security concerns

- The persistence boundary rejects `Platform` scope but cannot derive an opaque `holding_ref` from a domain UUID; the authoritative tenant router must supply the correct holding reference. RLS is the database enforcement boundary.
- No credentials, provider tokens, or payment data are persisted by this packet. Consent proof is stored only as the domain-provided reference.
- Audience replacement is atomic and profile IDs are checked against the same holding-scoped write database before membership rows are written.
- Read queries use only the read pool and cannot become command authority.

## Integration order

1. Main agent exports the module and runs the focused infrastructure checks.
2. Apply `0018` to disposable tenant write/read databases and verify RLS with two holding references.
3. Implement the worker projection from validated write payloads and source versions.
4. Add application/server adapters and UI read consumers.
5. Re-run the migration suite after its stale count assertion is updated by the owning integration work.

## Rollback

Before applying `0018`, revert these three packet files together. After application, rollback must use the tenant backup/restore procedure or a separately reviewed forward migration; no destructive down migration is included here.
