# G1-004-READINESS-LIVE-OBSERVATIONS-012

## Handoff

Task: add production-safe, fail-closed live readiness observations for the
worker heartbeat, holding registry/routing, projection freshness, and
backup/PITR metadata. This packet does not close the G1 readiness gate.

Dependencies consumed:

- Existing `WorkerReadinessComposition` and fail-closed state classifiers.
- `holding_registry`, `holding_database_health`, `holding_storage_health`,
  `holding_projection_switches`, and `holding_backup_runs`.
- Existing physical holding database provisioner/router and separate holding
  write/read migrations.

## Delivered behavior

- Added infrastructure `ReadinessObservationState` and bounded observation
  policy with explicit database, storage, projection, and backup age limits.
- Added typed, sanitized durable metadata observations. Missing metadata is
  `not_probed`; stale, future, failed, mismatched, invalid, and non-activated
  metadata is `unavailable`.
- Added live holding route probing that connects to both physical databases,
  verifies `current_database()` against the server-side route, and verifies a
  side-specific holding relation. Probe failures return a bounded reason code;
  connection details are not returned.
- Composed durable database health with live probes. A live connection cannot
  promote missing or stale durable health to ready.
- Added worker-owned heartbeat state and typed observations. Heartbeat starts
  unprobed and becomes ready only after a recent scheduler-owned mark; future
  and stale timestamps fail closed.
- Added `WorkerRuntime::observe_live_readiness`, which asynchronously composes
  heartbeat, holding routing, projection freshness, and backup/PITR states into
  the existing `WorkerReadinessComposition`.
- Readiness models omit holding database keys, storage namespaces, credential
  key references, backup references, and persisted error text.

## Files changed

- `crates/infrastructure/src/provisioning.rs`
- `crates/infrastructure/src/lib.rs`
- `crates/infrastructure/tests/readiness_observations.rs`
- `apps/worker/src/lib.rs`
- `audits/G1-004-READINESS-LIVE-OBSERVATIONS-012/evidence/README.md`

No migrations, protobuf, server, identity, UI, commercial, or Folio files
were changed by this packet.

## Interfaces produced

- `ReadinessObservationPolicy`
- `HoldingProvisioner::observe_holding_readiness_metadata(...)`
- `HoldingDatabaseRouter::observe_holding_readiness(...)`
- `RoutedHoldingDatabase::probe(...)`
- `WorkerHeartbeat::{record_at,observe}`
- `WorkerRuntime::{heartbeat,record_heartbeat_at,observe_live_readiness}`
- `WorkerLiveReadinessPolicy` and `WorkerLiveReadinessObservation`

These are server-composition seams only. No transport or readiness endpoint
was changed.

## Database effects

Observation methods are read-only. They query only the control-plane registry,
durable health, projection-switch, and backup metadata tables, then perform
server-side `SELECT`/relation probes against the registered holding write/read
databases. They do not return or mutate credentials, registry keys, backup
references, or holding data.

## Verification

From `/Users/fluke/Desktop/Work/folio/erpx`:

- `cargo fmt --all --check` — PASS
- `cargo test -p erpx-infrastructure --all-targets -- --nocapture` — PASS:
  30 non-ignored tests, 0 failed; 8 additional tests ignored for disposable
  PostgreSQL.
- `cargo test -p erpx-worker --all-targets -- --nocapture` — PASS: 8
  non-ignored tests, 0 failed; 1 additional test ignored for disposable
  PostgreSQL.
- `cargo clippy -p erpx-infrastructure -p erpx-worker --all-targets -- -D warnings` — PASS
- Focused live integration with explicitly created disposable PostgreSQL
  databases — PASS: 1 passed, 0 failed. The control and holding databases were
  explicitly dropped afterward and verified absent.

The live integration exercised physical holding provisioning, both route
probes, activated projection metadata, verified PITR metadata, and reference
redaction.

## Unfinished work and security concerns

- The worker scheduler has not been changed to publish a durable heartbeat;
  the runtime loop marks successful registered-handler passes, while other
  scheduler paths still require explicit `record_heartbeat_at` integration.
- The main server does not yet call `observe_live_readiness` or compose it into
  `/readyz`; that is an integration responsibility outside the allowed paths.
- Storage readiness currently observes the existing recent durable storage
  health record. A live MinIO connectivity probe and scheduler publication are
  still required before claiming live object-storage readiness.
- The APIs deliberately return `Result` on control-plane query failure. The
  server must map that failure to unavailable, never to an empty/green
  composition.
- Backup readiness is metadata freshness/verification evidence only; real
  backup execution, restore testing, and PITR scheduling remain open.
- Projection freshness uses the durable holding projection-switch state and
  freshness age; holding-specific event lag/checkpoint publication remains open.

## Integration order and rollback

1. Main composes the typed worker observation with server readiness and maps
   observation/query failures to fail-closed unavailable responses.
2. Add live storage and scheduler publication behind the same observation
   policy.
3. Add independent endpoint and failure-injection tests for each dependency.
4. Re-run the full G1 verification wave before changing the readiness gate.

Rollback is limited to removing the new observation methods, worker heartbeat
state/composition seam, focused tests, and this evidence file. Durable schema,
holding data, credentials, and Folio runtime behavior are unaffected.

