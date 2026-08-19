# G7 Operations completion

## Scope delivered in this packet

This packet closes the Operations control-loop seam on the existing holding
write/read model. It does not alter shared authorization, protobuf, migration
numbering, server registration, worker registration, console shell, or Folio
Next.js UI files. The Dioxus-facing model is kept in the Operations UI crate
and is read-only by construction.

Authoritative behavior now includes:

- policy-bound source detection with immutable source provenance and bounded
  SLA metadata;
- MFA and contained-scope authorization proofs, with only an explicit,
  expiring, local-only break-glass proof accepted as fallback;
- active-window assignee/manager proofs and delegated manager-chain
  validation using the existing `RoleAssignment` contract;
- versioned expired-principal reassignment and due-SLA escalation through the
  existing exception lifecycle command;
- append-only holding outbox parity for every Operations event, with exact
  event-ID replay checks and a bounded repair pass for legacy Operations rows;
- write-stream/read-model/checkpoint reconciliation, projection freshness, and
  bounded Operations queue UI/API state.

Authoritative event/lifecycle writes use the existing
`operations_aggregate_snapshots`, `operations_events`,
`operations_source_references`, and `outbox_events` tables in one write
transaction. The rebuildable projection uses `operations_read_rows` and
`operations_projection_checkpoints` in the routed read database. No new schema
or migration is introduced by this packet.

## Changed paths

- `crates/domain/src/operations.rs`
- `crates/domain/src/operations/completion.rs`
- `crates/domain/src/operations_completion.rs`
- `crates/infrastructure/src/operations_persistence.rs`
- `crates/infrastructure/tests/operations_persistence.rs`
- `apps/worker/src/operations_completion.rs`
- `apps/worker/tests/operations_projection.rs`
- `crates/ui/src/operations.rs`
- `audits/G7-COMPLETION/README.md`

## Verification

Focused commands:

```text
cargo fmt --all --check
cargo test -p erpx-domain
cargo test -p erpx-infrastructure --lib
cargo test -p erpx-infrastructure --test operations_persistence
cargo test -p erpx-worker --lib
cargo test -p erpx-ui --lib
cargo test -p erpx-worker --test operations_projection
```

The current owned-file evidence is: `cargo test -p erpx-domain --all-targets`
passes (209 unit tests plus existing integration targets),
`cargo test -p erpx-infrastructure --test operations_persistence` passes its
three non-database tests and compiles three ignored PostgreSQL tests,
`cargo test -p erpx-worker --test operations_projection` compiles its one
ignored PostgreSQL route test, and `cargo test -p erpx-ui --lib` passes (71
tests). `cargo check` passes for the domain, infrastructure, worker, and UI
packages, and rustfmt check passes for every owned Rust path. The ignored
database tests require the repository's configured disposable holding/control
databases and remain coordinator-run evidence; no live database result is
claimed here.

## Coordinator integration hooks

The coordinator owns worker registration and shared module exports. Wire
`apps/worker/src/operations_completion.rs` as `operations_completion` in the
worker composition root, then schedule `OperationsCompletionWorker::run_once`
with authority-evaluator-provided manager-chain requests. The worker drains
the already registered `OperationsProjectionRunner` and returns a
`OperationsCompletionRunReport` containing reconciliation evidence; fail the
scheduler job when `reconciliation.healthy` is false. The coordinator must
provide authority-evaluator proofs; no local role lookup is permitted.

The coordinator-owned API adapter can call `SqlxOperationsStore::queue_state`
with the authenticated requested scope and `OperationsAuthorizationProof`.
The existing exception lifecycle transport remains the only mutation API;
the new completion methods are its worker/domain handoff for detection,
reassignment, escalation, and reconciliation.

## Rollback

Stop the completion worker handler and leave the existing projector running,
or disable the new handler registration. `prepare_projection_rebuild` is the
only destructive helper and is limited to the disposable routed read
projection; the write-side events, snapshots, source references, and outbox
rows remain untouched and replay restores the read model. The completion
repair operation is append-only and never rewrites an existing event. Reverting
this packet therefore affects only completion orchestration, read projection
rebuilds, and outbox parity checks; it does not activate customer directories,
switch production command ownership, retire Folio writes, or perform cutover.

## External certification

No external LDAP/AD/UCS dependency is required for this G7 packet. Manager
chain and principal validity inputs remain an explicit handoff from the
authoritative identity/RBAC evaluator. The coordinator must provide those
proofs in integration certification; this packet does not substitute local
role resolution.

Folio verification for this handoff: `bun run lint` completed with 0 errors
(four existing warnings in `infra/scripts/start-n8n.js`) and `bunx tsc --noEmit`
passed. The configured Folio development server remains running on port 3004;
no production build was used.

## Coordinator continuation — authoritative Operations SQLx revalidation — 2026-07-28

Fresh disposable holding write/read PostgreSQL pairs passed the four ignored
Operations persistence tests and the two ignored authoritative completion
tests. The persistence packet covered versioned/idempotent write/read
boundaries, source-referenced aggregate replay, exception lifecycle, and
scheduled detection with source event/outbox/read-model evidence. The worker
packet covered authoritative source reconciliation with scope isolation and
stable authority-bound scheduler requests. All temporary databases were
removed.

This is local Operations evidence only. Full source-domain breadth, runtime
restart/lease telemetry, Folio reconciliation, credentialed directory/vendor
certification, measured external scale/revocation, and production approval
remain queued. No holding activation, command-owner switch, Folio write
retirement, or cutover occurred.
