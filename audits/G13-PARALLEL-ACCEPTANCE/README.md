# G13 parallel deployment and recovery acceptance

**Audit date:** 2026-07-27
**Verdict:** **OPEN — structural acceptance is strengthened; runtime acceptance remains queued.**

This packet owns exactly:

- `apps/migrator/tests/g13_parallel_acceptance.rs`
- this README

It deliberately does not modify manifests or lockfiles, migrations, the
migrator binary, protobuf sources, server or worker registration, shared
authorization, the console, `MASTER_PLAN.md`, `EXECUTION_LEDGER.md`, Folio
source files, database state, process state, customer directories, or command
ownership.

## Scope and safety boundary

The test is a read-only structural verifier. It uses `include_str!` to inspect
the authoritative checked-in source surfaces and reads environment-variable
presence without printing values. It does not connect to PostgreSQL, invoke
the migrator, start or stop workers, contact a service, apply or roll back a
migration, run a descriptor tool, probe Folio, activate a directory, switch a
command owner, retire Folio writes, or perform a production cutover.

A passing test is therefore evidence that the required seams remain present;
it is not evidence that a deployment or recovery rehearsal has run.

## Structural checks

`g13_parallel_acceptance.rs` independently checks:

1. The four authoritative migration roots are present, G13 phases are ordered
   from rollback rehearsal through final delta, ERPX ownership, Folio
   read-only retention, and eventual retirement, while the migrator remains
   read-only/dry-run and has no production cutover command.
2. Control-plane idempotency, outbox, checkpoint, and projection-receipt
   persistence seams are present.
3. Versioned protobuf packages, the actual `tonic-prost-build` input list,
   generated-code inclusion, reserved wire numbers, and the repository
   compatibility policy are bound to the checked-in sources.
4. Worker registry and shutdown/restart seams, heartbeat and readiness
   reporting, lease/claim keys, retry/dead-letter transitions, and the
   idempotent claim conflict path are present in the worker and scheduler
   sources.
5. The Folio bridge is still permission-gated and schema-aware, reads the
   control plane through its read boundary, records global reads, and retains
   Folio as the legacy business-data owner. No ownership switch is inferred.

Run the structural packet with:

```text
cargo test -p erpx-migrator --test g13_parallel_acceptance -- --nocapture
```

## Explicit queued runtime acceptance

The following acceptance records are intentionally queued. They require
disposable, isolated resources and an approved operator runbook; none is
claimed by this packet.

| Record | Required evidence | Current status |
| --- | --- | --- |
| G13-RUN-DB-001 | Non-secret references for `ERPX_G13_SOURCE_DATABASE_URL`, `ERPX_G13_TARGET_DATABASE_URL`, and `ERPX_G13_ROLLBACK_DATABASE_URL`; apply the authoritative migration sets twice, prove idempotent checksums/versions, and retain redacted run evidence | **QUEUED — not connected or executed** |
| G13-RUN-PROTO-001 | Approved descriptor baseline plus a compatibility comparison covering removed/reused field numbers, services, reserved fields, and additive changes | **QUEUED — no comparison executed** |
| G13-RUN-WORKER-001 | Two workers on one disposable claim stream; kill/restart during a lease; prove reclaim, one receipt/outbox publication, bounded retry, DLQ/replay, heartbeat, and recovery timestamps | **QUEUED — no worker rehearsal executed** |
| G13-RUN-ROLLBACK-001 | After disposable backfill/shadow/final-delta preparation, restore or roll back the target, preserve immutable mappings/outbox/audit history, and reconcile source versus target | **QUEUED — no rollback or restore executed** |
| G13-RUN-FOLIO-001 | Exercise the bridge in its current Folio-owner state and prove no Folio write retirement, redirect, or ERPX command-owner switch without the explicit approval record | **QUEUED — no bridge or cutover action executed** |

If the three database references or the optional descriptor reference are
present when the test runs, the test still leaves the corresponding record
queued. Presence is not connectivity, authorization, compatibility, restart,
rollback, or reconciliation evidence, and values are never emitted.

## Evidence and rollback

This packet has no live deployment evidence and makes no G13 completion claim.
Its rollback is limited to removing the new test and this audit directory;
there is no database, process, migration, Folio, or customer-directory state
to restore.

The remaining queued records must be completed by the coordinator against
disposable resources, with redacted artifacts attached to the appropriate
G13 evidence record. They must not be used to authorize production cutover,
directory activation, command-owner switching, or Folio write retirement.

## Verification record

- Scoped Rust test: to be run by the coordinator after integration.
- `cargo fmt --all --check`: to be run in the serialized full verification.
- `git diff --check`: to be run for these two new paths.
- No live database, network, worker, migration, rollback, descriptor, or Folio
  bridge operation was performed by this packet.

## Coordinator revalidation — current tree — 2026-07-27

The coordinator has since executed the local disposable SQLx acceptance
targets and the real descriptor-comparison test. The descriptor build emits a
current file-descriptor set; when an approved baseline is configured, the
comparison rejects removed or incompatible message fields, enum values, and
RPC contracts while permitting compatible additive changes. The baseline is
absent in this environment, so the comparison remains an explicit queued
blocker rather than a claimed pass.

The local ignored database-backed G13 targets, bridge persistence, target
import, rollback/replay, projection, and reconciliation scenarios configured
in the workspace passed with unique disposable databases. Live Folio
source/target migration rehearsal, two-worker restart/lease evidence, approved
cutover, post-cutover write rejection, and production retention remain
unexecuted and unauthorized.

## Coordinator continuation — command boundary revalidation — 2026-07-28

The current tree now includes the additive `QueueMigrationWork` protobuf RPC,
server authorization/MFA enforcement, SQLx idempotency plus outbox persistence,
worker-consumable durable work, and a bounded console form. The command queues
only execution work; it cannot switch command ownership or advance production
cutover. Repeated requests with the same idempotency key return the original
work item, while conflicting payloads fail closed.

The disposable G13 migration persistence, target import/reconciliation/
rollback, and console/server focused tests passed. This does not satisfy the
queued live worker restart, credentialed Folio migration, approved descriptor,
or production approval records above.

## Coordinator continuation — lifecycle command revalidation — 2026-07-28

The structural server markers now include the authenticated
`BeginMigrationRun` and `AdvanceMigrationPhase` RPCs in addition to durable
work queueing. The additive protobuf surface is backed by SQLx run creation
and ordered transition persistence, with platform scope, MFA, optimistic
version/phase fencing, append-only events, and an explicit rejection before
command-owner approval.

This remains structural/local evidence. It does not claim live source,
target, worker restart, rollback/retention, descriptor, vendor, or production
cutover acceptance.

## Coordinator revalidation — disposable migration rehearsal — 2026-07-28

The coordinator additionally ran the real ignored
`g13_disposable_acceptance` target against fresh local PostgreSQL databases.
It passed source extraction, durable control/read state, immutable target
backfill, outbox publication, separate read projection, shadow replay,
reconciliation, rollback restoration, and the guarded Folio-write rejection.
Temporary databases were removed. This evidence is local and bounded; the
queued live worker restart, credentialed Folio migration, descriptor
baseline, retention, and production-approval records remain required.

The coordinator also executed the ignored `g13_migration_persistence` target
against fresh control write/read databases. Its restart-safe claim sequence
passed lease expiry/reclaim, stale-worker fencing, retry exhaustion,
dead-letter projection, and idempotent work behavior. This does not replace
live two-process kill/restart telemetry or authorize a cutover.
