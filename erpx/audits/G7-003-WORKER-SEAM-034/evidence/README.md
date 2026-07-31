# G7-003-WORKER-SEAM-034 evidence

Date: 2026-07-24

## Delivered behavior

- Added `RoutedTenantDatabase::database_pool`, a server-side-only borrow of the already-routed `PgPool`; no URL or credential serialization is added.
- Added `OperationsEventStreamRunner` in `erpx-worker`.
- The runner validates tenant/holding references and the existing maximum Operations page size before routing.
- Event-page reads route both tenant write and read databases through `TenantDatabaseRouter`, construct `SqlxOperationsStore` only from those server-side pools, consume `read_event_page`, and close both routed pools on completion or read failure.
- Projection rebuild preparation follows the same tenant-bound routing and consumes `prepare_projection_rebuild`; it only clears the named, holding-scoped read projection through the existing infrastructure API.
- Underlying routing, SQLx, validation, and store-construction failures are logged by operation kind and returned as bounded, stable worker configuration errors. Database URLs, credentials, SQL errors, event payloads, and tenant secrets are not returned.
- The seam reports read-page/rebuild-preparation metadata only. It does not apply aggregate events, schedule work, reconcile projections, or claim projector completion.

## Files changed

- `erpx/crates/infrastructure/src/provisioning.rs`
- `erpx/apps/worker/src/lib.rs`
- `erpx/audits/G7-003-WORKER-SEAM-034/evidence/README.md`

## Contract and migration impact

- No domain, protobuf, application, or public database contract changes.
- No migration or schema changes.
- Database effects are limited to existing Operations event reads and the existing explicitly named projection-rebuild deletion.

## Verification

From `erpx/`:

```text
cargo fmt -p erpx-worker -- --check                 PASS
cargo test -p erpx-worker                           PASS: 14 passed, 1 ignored
cargo clippy -p erpx-worker --all-targets -- -D warnings  PASS
```

Focused tests cover page-size bounds, tenant/holding reference binding, projection-name validation, and worker-runner debug redaction. No live database test was added because the seam has no injectable router and the existing disposable routing harness is outside this packet's allowed paths.

## Remaining work and security concerns

- Worker scheduling/claiming, aggregate consumers, ordered projection application, checkpoints, retries/dead letters, reconciliation, and replacement-read activation remain open G7 work.
- The runner must receive a server-created `TenantDatabaseRouter`; callers must not construct routes from client-provided URLs or credentials.
- The current runner intentionally exposes no event payload through worker errors, but future projector diagnostics must preserve the same redaction boundary.

## Integration order

1. Keep this seam behind the existing tenant registry and readiness checks.
2. Add a dependency-ready Operations handler that supplies an aggregate consumer and uses `read_page` with the returned cursor.
3. Add worker-level checkpoint/idempotency/retry orchestration.
4. Add reconciliation and replacement-read activation only after independent tenant-isolation tests pass.

## Rollback

Remove only `database_pool`, the two worker request/report types, `OperationsEventStreamRunner`, its redaction/validation helpers, focused tests, and this evidence file. Existing provisioning, Operations persistence, migrations, and worker handlers remain unchanged.
