# G13 deployment, recovery, and observability acceptance verifier

**Audit date:** 2026-07-27  
**Verdict:** **OPEN — this verifier is complete; G13 deployment/recovery acceptance is not.**

This packet owns exactly:

- `apps/migrator/tests/g13_deployment_recovery.rs`
- this README

It does not modify Cargo manifests or lockfiles, migration numbering or SQL,
protobufs, server or worker registration, shared authorization, the console,
`MASTER_PLAN.md`, `EXECUTION_LEDGER.md`, Folio source files, or production
state.

## What the verifier checks

The executable test reads the authoritative implementation surfaces. It does
not use mocks, a schema-only fixture, or a fake database.

1. **Migration and protobuf compatibility seams.** It checks the embedded
   control/read/holding migrator roots, idempotency/outbox/projection-receipt
   tables, ordered G13 phases, read-only source extraction, versioned protobuf
   packages, the actual `tonic-prost-build` inputs, generated-code inclusion,
   and reserved wire-compatibility fields.
2. **Restart-safe worker and observability seams.** It checks the real worker
   registry and shutdown loop, bounded handler reports, heartbeats/readiness,
   lease ownership, retry/dead-letter transitions, idempotent claim keys, and
   the server `/healthz`, `/readyz`, `/metrics`, correlation-trace, worker
   heartbeat, and projection-freshness surfaces.
3. **Rollback and Folio bridge boundary.** It checks that the existing
   migrator remains dry-run/read-only and contains no cutover command, that
   rollback rehearsal and Folio-retention phases remain explicit, and that the
   Folio ERPX platform page remains permission-gated and schema-aware. The
   Folio control-plane migration message explicitly preserves holding business
   records.
4. **Runtime blocker queue.** It reports missing database references and
   whether port 3004 is reachable. It also reports the absence of disposable
   source/target/rollback references. These are non-mutating diagnostics; a
   reachable port is not application, bridge, restart, or cutover evidence.

Run it with:

```text
cargo test -p erpx-migrator --test g13_deployment_recovery --offline -- --nocapture
```

The test intentionally remains green while it prints queued blockers. A
green structural test is not a green G13 gate.

## Evidence matrix

| Acceptance requirement | Current evidence | Status |
| --- | --- | --- |
| Authoritative migration set | `CONTROL_*_MIGRATOR` and holding migrators are embedded from their real SQL directories; control SQL has idempotency, outbox, checkpoints, and projection receipts | PASS — contract |
| Protobuf compatibility | Versioned `.proto` packages, build inputs, generated inclusion, and reserved fields are checked; no prior descriptor baseline is checked in | PARTIAL — source/build contract only |
| Worker restart and idempotency | Real registry, lease, claim, retry/DLQ, receipt/idempotency, heartbeat, and graceful Ctrl-C seams are checked | PASS — contract; live restart open |
| Rollback evidence | Ordered `RollbackRehearsed` phase and read-only dry-run plan are checked | PARTIAL — no disposable target restore/compensation rehearsal |
| Observability | Server health/readiness/metrics and correlation/readiness markers plus worker heartbeat/report fields are checked | PASS — contract; live alert/metric capture open |
| Folio bridge operational boundary | Existing permission-gated `/platform` route and schema-aware control-plane console are checked; port 3004 is probed without mutation | PARTIAL — no bridge ownership or post-cutover write rejection evidence |
| Production cutover | The verifier asserts no migrator cutover command exists and makes no owner change | OPEN BY DESIGN — explicit approval is still required |

## Required runtime evidence before G13 can close

The coordinator must run these against disposable, isolated source/target and
rollback resources, then attach the resulting redacted evidence to the
cohort record:

- apply every authoritative control/read/holding migration set twice and prove
  the second run is a no-op with compatible checksums/versions;
- generate a protobuf descriptor set from the checked-in versioned inputs and
  compare it with the last approved descriptor baseline, including removed or
  reused field/service numbers;
- start two worker instances against the same claim stream, kill one while it
  holds a lease, restart it, and prove that the surviving or reclaimed work is
  processed once through idempotency keys/receipts and does not duplicate
  outbox publication;
- capture `/healthz`, `/readyz`, `/metrics`, correlation IDs, worker heartbeat,
  retry/dead-letter, projection freshness, and restart/recovery timestamps;
- rehearse target rollback after backfill, shadow replay, and final-delta
  preparation, preserving immutable mappings, outbox history, and audit
  evidence; verify reconciliation after restore;
- exercise the Folio bridge in its current owner state and prove that no
  Folio write is retired or redirected until the explicit owner-switch and
  retention approvals are recorded;
- run the relevant vendor/external acceptance suites when their environment
  is in scope. This verifier does not activate LDAP, AD DS, UCS, or any
  customer directory.

## Current blockers

At verifier time, the following evidence is not claimed:

- no disposable G13 source, target, and rollback database references are
  configured under `ERPX_G13_SOURCE_DATABASE_URL`,
  `ERPX_G13_TARGET_DATABASE_URL`, or `ERPX_G13_ROLLBACK_DATABASE_URL`;
- live migration reruns, target import, projection rebuild, shadow replay,
  worker kill/restart, rollback restoration, and post-restore reconciliation
  have not been executed by this lane;
- no approved protobuf descriptor baseline or automated compatibility-diff
  artifact is present in this lane;
- a local Folio port probe, if reachable, proves only that something listens
  on port 3004. It does not prove the Next.js application, ERPX bridge, or
  command boundary is healthy; if unreachable, the test reports that runtime
  blocker explicitly;
- external directory vendor certification remains separately queued. No
  customer connector is activated by this verifier.

These blockers do not authorize changing production command ownership,
activating customer directories, retiring Folio writes, or performing final
cutover.

## Verification record

- `cargo fmt --all --check` — PASS.
- `cargo check -p erpx-proto --offline` — PASS; the authoritative protobuf
  build inputs compile.
- `cargo test -p erpx-migration --lib --offline` — PASS; 9 tests passed.
- `cargo test -p erpx-migrator --test g13_deployment_recovery --offline
  -- --nocapture` — PASS; 4 tests passed. The test printed the missing
  database/disposable-runtime and port-3004 blockers listed above.
- `git diff --check` — PASS for both owned paths.
- No database, process configuration, migration state, command owner,
  customer directory, or Folio write state was changed.

Rollback for this packet is limited to reverting the new test and README. It
has no database or runtime side effect.
