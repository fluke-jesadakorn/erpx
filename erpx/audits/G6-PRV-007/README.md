# G6-PRV-007 — Physical provisioning and replacement-read evidence

## Delivered behavior

- Added an ignored real PostgreSQL integration harness for the existing `TenantDatabaseProvisioner` and `TenantProvisioner` path.
- Creates uniquely named disposable control-write, control-read, tenant-write, tenant-read, failed-candidate-read, and successful-candidate-read databases.
- Applies the embedded control and tenant migrations and verifies the tenant write/read databases are physically distinct.
- Repeats tenant provisioning to verify migration/provisioning idempotency.
- Verifies every table in the `erpx_tenant` schema has both row-level security and forced row-level security.
- Verifies inactive tenants cannot route, activation requires storage verification, failed storage verification cannot activate a tenant, and successful verification permits activation.
- Verifies active routing is holding-scoped and that the write transaction receives the transaction-local `erpx.holding_ref` setting.
- Exercises failed restore verification and proves the durable backup run is failed, the prior read database remains selected, and no projection switch is recorded.
- Exercises verified restore plus replacement-read activation against a physically migrated candidate read database, then probes the newly selected read route.
- Cleanup attempts every created database in reverse order and terminates active connections before dropping it. Database URLs and credentials are not printed.

## Exact test commands and results

The focused harness is ignored by default:

```text
cargo test -p erpx-infrastructure --test provisioning_replacement -- --ignored real_physical_provisioning_and_replacement_read_path_is_safe
```

Required environment:

- `ERPX_TEST_ADMIN_DATABASE_URL`: server-side PostgreSQL URL with permission to create/drop databases; defaults in the harness to `postgres://localhost/postgres` for local development.
- `ERPX_TEST_TENANT_DATABASE_URL_TEMPLATE`: tenant URL template containing exactly one `{database}` token; defaults to `postgres://localhost/{database}`.

The test does not print either value. The control and tenant databases are generated inside the test with a UUID suffix and are dropped before the test returns on both success and ordinary assertion/error paths.

Required verification:

```text
cargo fmt --all --check
cargo clippy -p erpx-infrastructure --all-targets -- -D warnings
cargo test -p erpx-infrastructure --test provisioning
```

Observed results in the local PostgreSQL environment:

- `cargo test -p erpx-infrastructure --test provisioning_replacement -- --ignored real_physical_provisioning_and_replacement_read_path_is_safe`: 1 passed; all generated databases were dropped.
- `rustfmt --check --edition 2024 crates/infrastructure/tests/provisioning_replacement.rs`: passed.
- `cargo clippy -p erpx-infrastructure --all-targets -- -D warnings`: passed.
- `cargo test -p erpx-infrastructure --test provisioning`: 2 passed, 2 ignored, 0 failed.
- Cleanup proof: `psql -d postgres -Atqc "SELECT datname FROM pg_database WHERE datname LIKE 'erpx_prv007_%' ORDER BY datname;"` returned no rows.
- `cargo fmt --all --check`: reports an existing formatting difference in `apps/console/src/main.rs:2169`, outside this packet’s allowed paths; the packet did not retain that incidental formatter change.

## Contract and migration changes

None. Production source, migration directories, server composition, and shared contracts are unchanged.

## Security and remaining limitations

- Database names are generated and validated by the harness; interpolated SQL is restricted to those generated identifier values.
- All route access remains server-side; no URL or credential is exposed by the test output or the production provisioning types.
- The injected restore executor remains deterministic test infrastructure. This packet does not claim a backup/PITR provider, physical WAL restore, or external object-storage implementation.
- The production replacement flow still relies on the injected executor’s verification result and the existing control-plane rebuild/activation records; this packet proves the database separation and activation gate, not an external backup system.

## Integration order and rollback

Integrate after the existing G6 provisioning, routing, and scheduler contracts. There are no production changes to roll back; remove `crates/infrastructure/tests/provisioning_replacement.rs` and this audit directory to revert the packet. Any databases created by the ignored test are disposable and are explicitly dropped by the harness.
