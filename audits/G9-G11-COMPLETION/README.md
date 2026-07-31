# G9–G11 product-context completion handoff

## Delivered slice

The product-owned write/read boundaries now expose typed, holding-scoped
projection reconciliation for Sales, Procurement/Inventory (SCM), POS,
Asset/Maintenance, HR/Payroll, Legal/Documents, and Marketing/CRM.

Each reconciliation check reads the authoritative tenant outbox on the write
route and compares its latest aggregate versions with the disposable read
projection/checkpoints on the read route. It reports source event count,
source/projected aggregate counts, missing aggregates, stale versions, and
orphan projected aggregates. A non-zero mismatch is never repaired in place;
the report is a rebuild/incident signal. Worker projector seams for HR, Legal,
SCM, and POS expose the check with tenant/holding validation, routed pools,
bounded redacted errors, and guaranteed route closure.

This preserves source ownership, append-only outbox history, forced-RLS
boundaries, and read-model rebuild semantics. No schema, migration, shared
registration, shared authorization, protobuf, API transport, console shell, or
Folio write behavior was changed.

## SCM scoped read and reconciliation handoff

The highest-value product-owned follow-up is implemented for the
Procurement/Inventory path. `ScmAggregate::scope` exposes only the validated
aggregate scope, and `SqlxScmStore::get_for_scope` rejects a read-model row
outside the caller's company/branch containment before an API or UI adapter
can render it. Shared authorization remains responsible for roles and
permissions; this product guard is the second, fail-closed boundary.

`ScmProjectionRunner::project_to_caught_up_and_reconcile` performs bounded
source-outbox catch-up and reconciliation against the same routed holding
before returning a success handoff. A projection run with missing, stale, or
orphan rows remains an incident/rebuild result rather than a false healthy
status.

The ignored acceptance test
`crates/infrastructure/tests/scm_persistence.rs` exercises two real
PostgreSQL databases: source write, idempotent command replay, append-only
outbox delivery, idempotent projection, authorized and sibling-company read
denial, mismatch detection after a disposable read clear, and replay-based
recovery. It is not a mock or schema-only test.

## Changed paths

- `crates/domain/src/asset_maintenance.rs`
- `crates/domain/src/hr.rs`
- `crates/domain/src/legal.rs`
- `crates/domain/src/marketing.rs`
- `crates/domain/src/pos.rs`
- `crates/domain/src/procurement_inventory.rs`
- `crates/domain/src/sales.rs`
- `crates/infrastructure/src/asset_maintenance_persistence.rs`
- `crates/infrastructure/src/hr_projection.rs`
- `crates/infrastructure/src/legal_projection.rs`
- `crates/infrastructure/src/marketing_persistence.rs`
- `crates/infrastructure/src/pos_projection.rs`
- `crates/infrastructure/src/sales_persistence.rs`
- `crates/infrastructure/src/scm_persistence.rs`
- `apps/worker/src/hr_projection.rs`
- `apps/worker/src/legal_projection.rs`
- `apps/worker/src/pos_projection.rs`
- `apps/worker/src/scm_projection.rs`
- `audits/G9-G11-COMPLETION/README.md`
- `crates/infrastructure/tests/scm_persistence.rs`

## Verification

Focused verification completed for this packet:

- `cargo test -p erpx-domain --lib`: 209 passed, 0 failed, 0 ignored.
- `cargo test -p erpx-infrastructure --lib`: 183 passed, 0 failed, 4 ignored.
- `cargo test -p erpx-worker --lib`: 136 passed, 0 failed, 1 ignored. The
  target emitted two existing Finance worker dead-code warnings; no Finance
  files were changed.
- `cargo check --workspace --all-targets --keep-going` completed without
  errors. Its only reported warnings were eight existing Finance worker
  dead-code warnings.
- `cargo fmt --all --check` is currently blocked by unrelated in-flight
  formatting in `crates/identity/src/directory/sync.rs`; the changed product
  Rust paths pass the focused rustfmt check below.
- `cargo clippy -p erpx-domain --all-targets -- -D warnings` is blocked by
  the pre-existing out-of-scope `crates/domain/src/finance.rs:278`
  `collapsible_if` lint. The infrastructure and worker strict-Clippy runs
  are likewise blocked before reaching this slice by existing workspace
  errors in Finance and/or identity directory files.
- `rustfmt --edition 2024 --check` passed on every changed Rust path listed
  above. Workspace-wide `cargo fmt --all --check` remains coordinator-owned.
- `cargo test -p erpx-infrastructure --lib scm_persistence`: 4 passed, 0
  failed.
- `cargo test -p erpx-worker --lib scm_projection`: 3 passed, 0 failed.
- `cargo test -p erpx-infrastructure --test scm_persistence -- --ignored` is
  queued for the coordinator's disposable two-database verification wave.

Folio verification also completed: `bun run lint` has 0 errors and 4 existing
warnings in `infra/scripts/start-n8n.js`; `bunx tsc --noEmit` passed. No
production Next.js build was run.

Local PostgreSQL is accepting connections on `/tmp:5432`, but no routed
disposable `ERPX_*DATABASE_URL` pair was supplied, so no live
reconciliation-specific database test was claimed. No mock or schema-only
result is used as completion evidence.

## Blockers

- Certification of the new SQLx reconciliation queries against disposable
  routed write/read databases is queued until the coordinator supplies the
  test URL pair.
- Workspace strict Clippy and workspace formatting remain blocked by
  unrelated Finance and identity-directory changes named above.
- LDAP, Active Directory Domain Services, and UCS vendor certification is not
  part of this product-context packet; the identity lane must record its own
  external-environment blocker if those systems are unavailable.

## Coordinator hooks

1. Export the seven typed report structs through the shared infrastructure
   composition root if transport or scheduler code needs named report types.
2. Register the existing worker reconciliation methods in the coordinator's
   product scheduler/authorization path; this packet does not edit worker
   registration or shared authorization.
3. Add product API/UI read models that render the report as fresh, stale,
   rebuild-required, or unavailable; no command may authorize from the read
   model.
4. Run disposable two-database tests that seed a real source aggregate/outbox,
   project it, verify a consistent report, then exercise missing/stale/orphan
   detection and rebuild rollback. Preserve authoritative source rows on any
   rollback.

## Rollback

Revert the listed source/report and worker seam changes only. No migration or
source rows are removed; existing authoritative outbox events remain
replayable, and existing product projector/rebuild behavior remains the
rollback path.
