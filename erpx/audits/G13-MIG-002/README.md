# G13-MIG-002 — Read-only Folio source extractor

## Objective

Create the first executable G13 source-side tool: an aggregate-only, PII-free
Folio extractor that can be rerun deterministically and feed later mapping and
reconciliation work without mutating either database.

## Delivered behavior

- `FolioSourceExtractor` uses a single PostgreSQL transaction and issues
  `SET TRANSACTION READ ONLY` before every source query; it rolls the
  transaction back after capture.
- The extractor records bounded counts for customers, orders, expenses, users,
  leave, journals, inventory movements, legal documents, files, domain events,
  and audit/decision rows.
- It records allowlisted status counts, money totals in integer minor units,
  object-byte totals, and missing-table inventory without selecting names,
  addresses, contacts, document contents, credentials, or raw event payloads.
- Source IDs are never emitted. Each mapping key is a SHA-256 source
  fingerprint, and each value is a deterministic opaque UUID under the caller's
  namespace. Per-table mapping and aggregate-result bounds are enforced.
- The canonical source-contract hash is stable across capture timestamps and
  changes when aggregate content changes.
- `erpx-migrator extract` reads the URL only from a named environment variable,
  rejects unsafe variable names, emits an explicit `read_only` marker, and
  redacts connection/extraction failures.

## Files changed

- `crates/migration/src/lib.rs`
- `apps/migrator/Cargo.toml`
- `apps/migrator/src/main.rs`
- `apps/migrator/tests/cli.rs`
- `audits/G13-MIG-002/README.md`
- `MASTER_PLAN.md`

No schema, source row, ERPX database, Folio route, or external service state
was changed.

## Verification

- `cargo fmt --all --check` — PASS.
- `cargo test -p erpx-migration -p erpx-migrator` — PASS; 9 migration unit
  tests, 3 migrator unit tests, and 5 CLI integration tests.
- Live read-only smoke test against the local `folio_db` — PASS using the
  `extract` command; the snapshot contained the expected live aggregate
  counters, status distributions, money/file totals, and opaque mappings.
- The output contained no database URL or raw source business payload.

## Open requirements

This is source extraction only. It does not freeze Folio writes, backfill ERPX
write databases, build ERPX projections, replay deltas, compare target data,
explain every difference, persist migration runs, orchestrate cohorts, rehearse
rollback, switch command ownership, or retire Folio writes.

## Rollback

Revert only the extractor, CLI dependency/command, tests, and this audit. No
database or runtime rollback is needed.
