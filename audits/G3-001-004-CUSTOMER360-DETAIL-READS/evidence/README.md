# G3-001-004 Customer 360 detail reads

> Historical note: the access-audit and notes `Unsupported` behavior described
> below was superseded by G3-005-006-READMODELS-002, which adds their read
> models and typed repository projections.

## Delivered

- Added domain read-view envelopes with explicit `Present`, `Absent`, and
  `Unsupported` states, five-minute freshness classification, and an explicit
  read-only marker.
- Added typed organization, contacts, commercial, entitlements, onboarding,
  health, support-summary, access-audit-summary, and notes DTOs.
- Added `Customer360DetailReadStore` application read contracts without
  changing the legacy `CustomerReadStore` list/detail port.
- Added `SqlxControlPlane` read-side methods backed only by the control read
  pool and the existing `erpx_control.customers` projection.
- Added enriched-payload parsing with legacy `Customer360` fallback and a
  projection-key/customer-ID consistency check.
- Access-audit and notes return `Unsupported` for an existing customer because
  the current control read migration has no corresponding read models; an
  unknown customer returns `Absent`.

## Verification

All commands ran from `erpx/`:

```text
rustfmt --edition 2024 crates/domain/src/customer.rs crates/application/src/lib.rs crates/infrastructure/src/repositories.rs crates/infrastructure/tests/customer360_detail_reads.rs
PASS

cargo check -p erpx-infrastructure --tests
PASS

cargo test -p erpx-domain --lib
PASS: 41 passed, 0 failed

cargo test -p erpx-application --lib
PASS: 4 passed, 0 failed

cargo test -p erpx-infrastructure --lib
PASS: 15 passed, 0 failed, 3 ignored (pre-existing disposable PostgreSQL tests)

cargo test -p erpx-infrastructure --tests --no-run
PASS: all infrastructure test targets compiled

cargo test -p erpx-infrastructure --test customer360_detail_reads
PASS: 0 passed, 0 failed, 1 ignored (requires disposable PostgreSQL databases)

cargo clippy -p erpx-domain -p erpx-application -p erpx-infrastructure --all-targets -- -D warnings
PASS
```

The ignored disposable read-database test is present at
`crates/infrastructure/tests/customer360_detail_reads.rs` but was not run in
this environment because `ERPX_TEST_CONTROL_WRITE_DATABASE_URL` and
`ERPX_TEST_CONTROL_READ_DATABASE_URL` were not configured.

## Independent rerun — 2026-07-24

After the commercial re-export and detail-read integration were repaired, the
disposable test was run against explicitly named local PostgreSQL databases:

```text
ERPX_TEST_CONTROL_WRITE_DATABASE_URL=postgres://localhost/erpx_c360_detail_w_20260724_a \
ERPX_TEST_CONTROL_READ_DATABASE_URL=postgres://localhost/erpx_c360_detail_r_20260724_a \
cargo test -p erpx-infrastructure --test customer360_detail_reads -- --ignored --nocapture
```

Result: PASS — 1 passed, 0 failed. Both databases were explicitly dropped
after the run.

## Security and isolation review

- Detail repository methods issue `SELECT payload, updated_at` only through
  `ControlPlanePools.read`; they do not query or mutate the control write pool,
  holding databases, scope bindings, support records, or audit records.
- No authorization decision is made in the read port. Authorization and global
  read auditing remain transport/application responsibilities.
- The payload's embedded customer ID must match the read row key, failing
  closed on inconsistent projection data.
- Legacy list/detail behavior remains available through `CustomerReadStore` and
  `Customer360Projection::to_legacy`.
- Existing `Customer360Projection::summary()` redaction remains unchanged and
  excludes registration/tax identifiers, billing address, domains, logos,
  contacts, payment-provider references, and financial totals. Commercial
  provider references are exposed only by the separately named commercial
  detail DTO and still require an authorized transport boundary.

## Migration effects and unsupported fields

No migration files were changed. The existing control read schema has only the
customer projection needed by these methods. Access-audit summaries and notes
have typed DTOs and explicit unsupported responses, but cannot be populated
until their read models are added by a future projection/migration packet.

## Integration order and rollback

1. Integrate domain DTOs and read-view semantics.
2. Integrate the application `Customer360DetailReadStore` port.
3. Integrate infrastructure read-pool methods and the focused disposable test.
4. Add transport handlers only after authorization and global-read audit wiring
   consumes the read-only envelopes.

Rollback removes only the added DTOs, read port, repository implementation,
focused test, and this evidence file. It does not remove or alter write-side
authority, existing projection state, legacy list/detail behavior, or any
migration.
