# G13 migration/evidence audit

**Audit date:** 2026-07-27  
**Verdict:** **OPEN — G13 is not complete.**

This audit records the evidence actually present in the repository. It does
not authorize a command-owner switch, Folio write retirement, customer
cutover, or any production migration. No shared migration numbering, Cargo
manifest, protobuf, server/worker registration, `MASTER_PLAN.md`, or
`EXECUTION_LEDGER.md` file was changed for this audit.

## Inspected artifacts

| Artifact | Evidence found | Boundary |
| --- | --- | --- |
| `audits/G13-MIG-001/README.md` | Ordered cutover contract and unit-test record | Contract only; explicitly leaves durable runs, backfill, reconciliation, rollback rehearsal, and cutover open |
| `audits/G13-MIG-002/README.md` | Read-only Folio extractor and historical local smoke-test record | Aggregate snapshot only; explicitly leaves target import, projection, delta replay, rollback, and retirement open |
| `crates/migration/src/lib.rs` | `MigrationPhase`, `MigrationRun`, deterministic mapping, source snapshot, and reconciliation value objects | In-memory/transport-neutral behavior; no persistence adapter or runtime command path |
| `apps/migrator/src/main.rs` | Read-only `extract`, `map`, `reconcile`, and dry-run `plan` commands | `plan` emits `read_only: true`, `state_change: false`, and every step `performed: false` |
| `apps/migrator/tests/cli.rs` | CLI safety, deterministic mapping, explanation, and read-only plan tests | Does not exercise a target database, worker, API, UI, or cutover |
| `adr/ADR-003-no-long-term-dual-writes.md` | Required source freeze, shadow replay, final delta, owner enforcement, rollback, retention, and Folio-write rejection evidence | Design requirement, not execution evidence |

## Verified behavior

The following is authoritative at the code/test level:

- `MigrationRun::new` rejects a nil run ID, unsafe references, and a source
  contract hash that is not exactly 64 hexadecimal characters.
- `MigrationPhase::next` permits only the declared phase order. Command owner
  is derived from phase and cannot become ERPX before rollback rehearsal,
  mutation freeze, and final-delta phases.
- Folio read-only retention can be scheduled only after the Folio-read-only
  phase and cannot complete before its end time.
- `FolioSourceExtractor` uses a single SQLx PostgreSQL transaction, issues
  `SET TRANSACTION READ ONLY`, captures bounded aggregate-only data, and rolls
  the transaction back after capture. It does not emit source IDs or raw
  business payloads.
- Deterministic mapping and reconciliation are repeatable and report
  unexplained differences rather than silently treating them as reconciled.
- The CLI rejects unsafe paths, secret-looking input, oversized input, and
  unknown options without echoing sensitive values.

## Evidence matrix

| G13 requirement | Status | Evidence or gap |
| --- | --- | --- |
| Domain state machine and ordering | **PASS (contract)** | `MigrationRun`, `MigrationPhase`, and migration-crate tests |
| SQLx source extraction | **PASS (read-only source)** | `FolioSourceExtractor::extract`; no source mutation is performed |
| Durable SQLx migration-run persistence | **MISSING** | No G13 run/cohort/phase/owner/source-hash/retention store, table, query, or checkpoint was found. `MigrationRun` is only a serializable value object |
| Target write backfill and final-delta import | **MISSING** | No target import adapter, idempotency ledger, batch checkpoint, or live write evidence is wired to the migrator |
| Outbox/events for migration transitions | **MISSING** | No G13 phase, backfill, reconciliation, rollback, or owner-switch event is persisted to an outbox |
| Worker execution and projector | **MISSING** | No G13 worker handler, claim/retry/DLQ path, projector, or rebuild checkpoint is wired |
| Reconciliation | **PARTIAL** | Pure snapshot comparison and CLI output are tested. There is no persisted, context-by-context source/target report with difference explanations and approval evidence |
| Read model | **MISSING** | No migration status, cohort, reconciliation, rollback, or retention read model is present |
| Authorization and audit context | **MISSING** | No G13 API/command authorization, approver binding, SoD/MFA decision, or durable audit record is wired. The phase-derived owner is not runtime enforcement |
| API | **MISSING** | No G13 RPC/HTTP endpoint or durable command surface was found |
| UI | **MISSING** | No G13 migration/reconciliation/rollback/retention screen is wired. Generic `command_owner` display data is not proof of G13 enforcement |
| Rollback | **PARTIAL (transaction safety only)** | Source extraction rolls back its read transaction. `RollbackRehearsed` is only a phase label; there is no target restore/compensating action, rehearsal result, or post-rehearsal evidence |
| Shadow replay and workflow acceptance | **MISSING** | The dry-run plan names these steps, but all steps are unperformed and no replay/workflow evidence is persisted |
| Folio bridge and retirement | **MISSING** | The read-only extractor is not a bridge or command-owner adapter. No proof exists that post-cutover writes are rejected by Folio or that retention has elapsed |

## Exact verification status

Executed against the current tree:

- `cargo test -p erpx-migration --lib` — **PASS**, 9 passed, 0 failed.
- `cargo test -p erpx-migrator --bin erpx-migrator --test cli` — **PASS**, 3
  binary/unit tests and 5 CLI integration tests.
- `cargo test -p erpx-migration -p erpx-migrator` — **NOT A COMPLETE PASS**.
  Migration tests and migrator unit/CLI tests passed, then the existing
  `g14_directory_acceptance` test launched a nested Cargo invocation while
  the outer invocation held Cargo's build lock. The command remained blocked
  at that nested test and was interrupted; no G13 test failure was observed.
- No live Folio extraction was re-run by this audit. The historical smoke
  result is recorded in `audits/G13-MIG-002/README.md`; it is not evidence of
  a target import or cutover.
- No audit-only test was added under this directory. Without changing a
  Cargo manifest, a Rust file here would not be part of an authoritative test
  target; a text-only test would provide no stronger evidence than this
  matrix.

## Required closure evidence

G13 can be closed only after all of the following are produced and verified
against disposable source and target databases:

1. A durable migration-run/cohort schema and SQLx store with transactional
   phase transitions, checkpoints, idempotency, source-contract hash, command
   owner, retention, and append-only audit/outbox records.
2. Real bounded backfill, projection, shadow replay, final-delta, and
   context-by-context reconciliation adapters with restart and duplicate
   evidence.
3. A real rollback rehearsal that proves the selected target state can be
   restored or safely compensated before owner switch, plus enforcement that
   Folio rejects writes after cutover.
4. Worker/projector/read-model wiring, authorized API commands/queries, and UI
   states for loading, denied, stale, read-only, reconciliation differences,
   rollback, retention, and failure.
5. Cohort evidence covering source freeze, workflow acceptance, rollback,
   final delta, ERPX ownership, Folio read-only retention, and the explicit
   approval boundary before retirement.

## Rollback of this audit

Rollback is limited to deleting or reverting this audit file. The audit made
no database, runtime, migration, command-owner, Folio, or customer state
change.

## Current-tree continuation update — 2026-07-27

The earlier matrix above is retained as historical audit context. The
coordinator has since added durable G13 control-write migration 0058 and
control-read migration 0011, including migration runs, transition events,
idempotency, bounded checkpoints/work items, reconciliation reports, rollback
rehearsal evidence, and rebuildable status/read-model tables. The SQLx adapter
in `crates/infrastructure/src/g13_migration.rs` enforces phase/version fences,
request hashes, append-only evidence, MFA, local-only break-glass, and explicit
cutover approval. `g13_migration_persistence` passed against fresh disposable
control-write/control-read PostgreSQL databases.

The G13 worker projector is registered as `g13_migration`; the server exposes
only the audited read-only `MigrationService.ListMigrationStatuses` RPC; and
the console renders the authoritative status model under Platform Settings.
There is still no cutover or command-owner mutation RPC.

G13 remains open because the implementation does not yet provide a live
source/target backfill, shadow replay, final-delta adapter, or rollback
rehearsal against a real Folio source and ERPX target. Approved protobuf
descriptor comparison, credentialed source environments, Folio post-cutover
write rejection, retention expiry, and production ownership/retirement remain
queued and unauthorized. No customer directory was activated and Folio
remains the operational write owner.

## Final coordinator verification confirmation — 2026-07-27

The current tree now has live disposable SQLx evidence for the durable G13
run/checkpoint/evidence/read-model path and the complete ignored workspace
suite. The final workspace format, warnings-denied Clippy, non-ignored tests,
ignored tests, protobuf compilation, Dioxus target checks, Folio lint/typecheck,
and port-3004 probe all passed. The ignored suite's first run found and fixed a
commercial snapshot casing defect; the clean rerun passed and removed all test
databases.

This does not close G13's external acceptance criteria. No credentialed
Folio-to-ERPX source/target migration, shadow replay, final delta, rollback
restore, post-cutover Folio write rejection, retention expiry, approved
descriptor comparison, or production approval evidence exists. Those blockers
remain queued; Folio remains the operational owner and no cutover was done.

## Coordinator continuation — executable directory transport and G13 leases — 2026-07-27

The current tree adds a real rustls-only LDAP worker dependency and transport
execution for LDAPS/StartTLS paging, RFC 4533 Sync refresh-only controls, AD DS
DirSync BER controls, and the UCS Provisioning API consumer workflow. The
worker starts from the authoritative SQLx checkpoint, writes identity state,
append-only role facts, outbox, and projection atomically, and confirms UCS
messages only after commit. This closes the former transport-contract-only
implementation gap; it does not certify a vendor environment.

The G13 store now persists idempotent work enqueue, expired-lease reclaim,
bounded claims, completion, retry/dead-letter transitions, and work outbox
events. The registered runtime remains a projector until a real Folio
source/ERPX target executor is configured, so it reports no migration claims
and never marks unexecuted migration work succeeded.

Focused worker/infrastructure tests and formatting checks pass. External
LDAP/AD/UCS credentials, measured volume/revocation acceptance, live Folio
source/target/rollback/retention evidence, and explicit cutover approval are
still required for G13/G14 closure.

## Coordinator final verification addendum — 2026-07-27

The complete ignored workspace suite passed against fresh disposable
control/holding/commercial/scheduler/support databases, including the durable
G13 migration persistence test and the one-million-user directory volume
test; all disposable databases were removed. Final format, warnings-denied
Clippy, non-ignored tests, protobuf compilation, Dioxus target checks, Folio
lint/typecheck, and the port-3004 probe also passed.

This remains local evidence only. Credentialed LDAP/AD/UCS certification,
measured external import/revocation, physical mobile execution, approved
protobuf descriptor comparison, live Folio source/target/rollback/retention
evidence, post-cutover write rejection, and explicit production approval are
not available. G13/G14 are therefore not marked complete and no cutover or
write-owner change was performed.

## Coordinator continuation — durable Folio source bridge — 2026-07-27

The implementation inventory above is extended by control migrations 0059 and
0012 and the registered `g13_folio_bridge` worker. A bounded Folio aggregate
snapshot is now captured through the existing read-only SQLx extractor,
persisted idempotently with source hash/extractor/count evidence, emitted to
the migration outbox, projected into the migration status read model, and
expanded into deterministic bounded backfill work. The bridge requires a
TLS-only source URL and verified MFA; it does not mutate Folio business data,
claim target migration work, or fabricate success.

The live disposable G13 persistence acceptance passed, as did the complete
ignored SQLx suite and the final warnings-denied Clippy, workspace tests,
protobuf, Dioxus, Folio, and port-3004 checks. This updates the local
implementation evidence for the former extractor-only gap. It does not close
the audit: no credentialed Folio source/target run, shadow replay, final delta,
rollback restore, retention expiry, post-cutover Folio write rejection,
approved descriptor comparison, or production approval exists. G13 remains
OPEN and Folio remains the operational write owner.

## Coordinator continuation — executable target import and rollback boundary — 2026-07-27

Holding-write migration 0050 and `G13TargetImporter` now provide a real,
RLS-scoped target import ledger. The importer consumes retained source
identity mappings under the exact source contract hash/fence, persists
idempotent target rows and outbox events, saves control checkpoints through the
G13 store, and exposes source-versus-target reconciliation differences. The
registered target worker handles backfill and reconciliation work and only
performs rollback capture/delete/restore when an explicit authenticated write
fence is present; rollback replay events are append-only. It never changes
command ownership or advances a migration phase.

Disposable PostgreSQL acceptance
`g13_target_import_replay_reconcile_and_rollback_are_durable`: PASS. It used
separate control write/read, holding target, and rollback databases; verified
source snapshot reload, two-row import, zero-row replay, matched
reconciliation, two-row rollback restore, and two rollback-replay outbox
events.

G13 remains OPEN. This local target-boundary evidence does not replace a
credentialed Folio source/ERPX target run, full source-row migration, shadow
replay, final delta, retention expiry, post-cutover Folio write rejection,
vendor certification, approved descriptor comparison, or explicit production
approval. Folio remains the operational write owner and no cutover occurred.

## Coordinator continuation — authenticated G13 work queue — 2026-07-28

The coordinator added the first authoritative execution-command boundary for
G13. `MigrationService.QueueMigrationWork` authenticates the session, requires
platform scope, directory-aware `migration.execute` authorization, a fresh
MFA code, bounded work kind/scope/fence fields, and a caller idempotency key.
It deliberately has no command-owner, cutover, or Folio-write-retirement
operation.

`SqlxG13MigrationStore::enqueue_work_command` persists the idempotency hash,
deterministic work item, and `migration.work_enqueued` outbox event in one
transaction. Replays return the same work identity without a second queue
item; a changed request under the same key is rejected. The registered worker
can claim and complete the queued item, while the console exposes the bounded
MFA-bound form and no secrets or cutover controls.

Focused verification passed: disposable PostgreSQL queue/idempotency/claim/
completion acceptance, disposable source/target/import/reconciliation/
rollback acceptance, server/infrastructure/proto compilation, and console
web/test verification. This is local execution evidence only. Credentialed
LDAP/AD/UCS certification, approved descriptor comparison, live Folio source/
target shadow/final-delta/rollback/retention evidence, and explicit production
approval remain open; G13 is not closed and no cutover occurred.

## Coordinator continuation — authenticated G13 lifecycle commands — 2026-07-28

The migration service now exposes additive `BeginMigrationRun` and
`AdvanceMigrationPhase` commands over the existing platform protobuf. Both
commands require an authenticated platform-scoped session, directory-aware
`migration.execute` authorization, a fresh MFA step-up, authenticated actor
matching, bounded request fields, and caller idempotency. They invoke the
existing SQLx run aggregate, ordered phase transition, append-only transition
event, and idempotency persistence rather than maintaining a transport-only
state.

The server rejects requests at or beyond `final_delta_imported`, so the
command-owner approval boundary remains external and no cutover, Folio write
retirement, directory activation, or approval reference can be submitted.
The console exposes begin/advance controls with an allowlisted pre-approval
phase list and no source rows, credentials, or cutover fields.

Focused server, console, protobuf, and warnings-denied Clippy checks passed;
the structural G13 acceptance was updated to require both lifecycle RPC
markers. This improves the local G13 command boundary but does not provide
credentialed Folio migration, live worker restart, approved descriptor,
vendor-directory, retention, rollback, or production-approval evidence. G13
remains open.

## Coordinator verification — lifecycle command integration — 2026-07-28

The full non-ignored workspace verification passed after the lifecycle RPC
integration, including format, warnings-denied Clippy, workspace tests,
protobuf checks, G13/G14 acceptance, and all Dioxus target checks. Folio lint
and TypeScript verification passed, with four pre-existing n8n lint warnings;
the protected Folio route returned HTTP 307 and port 3004 remains active.

The aggregate ignored workspace run was executed and failed closed at 64
targets because its explicit disposable PostgreSQL, descriptor, and
credentialed directory/Folio runtime references are unavailable. This is
recorded as queued external certification evidence, not a claimed G13 pass.

## Coordinator continuation — disposable source/target rehearsal — 2026-07-28

The real ignored acceptance
`g13_disposable_postgres_acceptance_is_real_and_cutover_free` passed against
fresh local PostgreSQL databases. It exercised the bounded source snapshot,
control-plane run/read projection, immutable target import, target outbox,
separate read projection, shadow replay, exact reconciliation, rollback
restore, and post-cutover Folio-write fence. The harness dropped all
temporary databases after the run.

This closes the local disposable rehearsal boundary only. It does not close
G13: credentialed Folio source/target execution, two-worker kill/restart and
lease-reclaim evidence, approved descriptor comparison, retention, command
owner approval, Folio write retirement, and production cutover remain open.

## Coordinator continuation — worker lease/restart rehearsal — 2026-07-28

The ignored `g13_migration_persistence` acceptance passed against fresh
control write/read PostgreSQL databases. It verified idempotent migration-work
enqueue, first-worker claim, simulated crashed-worker lease expiry,
restarted-worker reclaim at attempt two, stale-claim fencing, bounded retry
exhaustion into durable dead-letter state, and read-model projection of the
pending/failed counts. Temporary databases were removed.

This is disposable persistence evidence, not live process telemetry or
production migration evidence. Credentialed Folio execution, descriptor
approval, retention, command-owner approval, and write retirement remain open.
