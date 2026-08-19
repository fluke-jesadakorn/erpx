# G6-001 provisioning metadata evidence

This packet is a control-plane provisioning foundation, not a complete G6
gate. It records the state needed to provision and recover a holding without
placing holding credentials in the client:

- `holding_provisioning` tracks provisioning, recovery, and activation state.
- `holding_database_health` requires independently reported write and read
  reachability and schema versions.
- `holding_storage_health` records the holding-bound storage namespace health.
- `holding_backup_runs` records full, PITR, and restore-test metadata; restore
  tests require a target reference and verification is persisted.
- `holding_projection_switches` supports rebuild → ready → activated read-model
  switching while retaining the active read key until the replacement is
  ready.
- `HoldingDatabaseProvisioner` creates validated opaque PostgreSQL database
  names through a server-side admin connection, applies distinct holding write
  and read SQLx migration sets, and records the resulting schema health.
- Holding write and read migrations create separate schemas and force
  row-level-security policies keyed by the server-set `erpx.data_profile_key`
  session setting.
- `HoldingDatabaseRouter` resolves only active registry entries, rejects a
  mismatched holding scope before connecting, and exposes only transaction
  setup that applies a transaction-local holding setting.
- `HoldingDatabaseProvisioner::provision_storage_namespace` records a pending
  unhealthy state, invokes an injected namespace verifier, records healthy
  only after success, and leaves activation blocked on failure.
- `MinioObjectStore` signs namespace and object requests with SigV4, bounds
  presigned upload/download lifetimes, and never exposes the configured
  access or secret key in debug output.
- `HoldingProvisioner::execute_backup` validates backup kind/status/target
  invariants and persists started-to-completed/failed outcomes around an
  injected executor. It intentionally does not claim to run `pg_dump`,
  `pg_restore`, shell commands, or destructive database operations.
- `erpx-server` now validates the optional/required MinIO configuration at
  startup, exposes redacted storage health on `/healthz` and `/readyz`, and
  provides a server-only seam that invokes the provisioning namespace-health
  gate without returning credentials or object-store handles.

## Independent verification

Command:

```text
ERPX_TEST_CONTROL_WRITE_DATABASE_URL=postgres://localhost/erpx_provision_write_20260724 \
ERPX_TEST_CONTROL_READ_DATABASE_URL=postgres://localhost/erpx_provision_read_20260724 \
cargo test -p erpx-infrastructure --test provisioning -- --ignored --nocapture
```

Result: PASS — one ignored integration test passed against two physically
distinct disposable PostgreSQL databases. The test proved activation is
blocked until write/read/storage health is present, activation persists, full
and restore-test backup metadata is accepted, and a replacement read key is
switched atomically after rebuild completion. Both databases were explicitly
dropped by the test cleanup trap.

Server composition evidence:

```text
cargo check -p erpx-server --tests
cargo test -p erpx-server -- --nocapture
cargo clippy -p erpx-server --all-targets -- -D warnings
```

Result: PASS — storage configuration validation, credential redaction,
required/optional readiness behavior, injected namespace verification, and
identity HTTP routing all passed. Health deliberately reports live MinIO as
`not_probed` until a holding-bound verification runs.

Physical provisioning command:

```text
ERPX_TEST_CONTROL_WRITE_DATABASE_URL=postgres://localhost/erpx_physical_control_w_20260724 \
ERPX_TEST_CONTROL_READ_DATABASE_URL=postgres://localhost/erpx_physical_control_r_20260724 \
ERPX_TEST_ADMIN_DATABASE_URL=postgres://localhost/postgres \
ERPX_TEST_HOLDING_DATABASE_URL_TEMPLATE=postgres://localhost/{database} \
cargo test -p erpx-infrastructure --test provisioning physical_provisioning_creates_separate_rls_backed_holding_databases -- --ignored --nocapture
```

Result: PASS — the factory created and migrated distinct holding write/read
databases, verified write-side forced RLS metadata, verified read-side
projection/source-table separation, routed a scoped transaction, rejected a
mismatched holding, recorded storage health, and activated the holding. The two
control and two holding databases were explicitly dropped by the test cleanup
trap.

## Open G6 requirements

Live MinIO credential/configuration composition, PITR execution, restore
usability checks, router and object-access integration into all holding RPC
handlers, storage-health freshness, and platform operations UI remain open.
The injected storage verification and activation gate are integrated, but the
factory's admin URL and database URL template
are server-side configuration boundaries and are never returned in a
provisioning receipt.
No G6 completion claim or ERPX command-owner switch is made.

## Independent rerun — 2026-07-24

The two provisioning integration tests were rerun together against explicitly
named disposable control databases:

```text
ERPX_TEST_CONTROL_WRITE_DATABASE_URL=postgres://localhost/erpx_prov_w_20260724_a \
ERPX_TEST_CONTROL_READ_DATABASE_URL=postgres://localhost/erpx_prov_r_20260724_a \
cargo test -p erpx-infrastructure --test provisioning -- --ignored --nocapture
```

Result: PASS — 2 passed, 0 failed. The run created and verified distinct
holding databases `erpx_holding_w_20260724` and `erpx_holding_r_20260724`; all
four explicitly named databases were dropped after verification.
