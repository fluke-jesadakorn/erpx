# G7-004 — Operations exception lifecycle transport

## Delivered

- Added `OperationsService.TransitionOperationsException` to the shared
  protobuf contract.
- Added authenticated server transport for `assign`, `reassign`,
  `acknowledge`, `escalate`, and `resolve`.
- Required a holding-scoped command context, matching authenticated actor,
  positive expected aggregate version, active holding route, and
  `operations.exception.manage` authorization.
- Loaded the exception from the routed holding write database and checked that
  its persisted scope is contained by the caller scope before constructing the
  command.
- Obtained half-open validity windows for actor, assignee, and manager from the
  authoritative control-plane role-assignment chain. Caller-supplied validity
  windows are not accepted.
- Required bounded SLA policy, due time, and escalation window for assignment;
  delegated parsing remains strict for all other actions.
- Called the existing atomic SQLx lifecycle helper, preserving optimistic
  concurrency, aggregate-bound idempotency, immutable event history, and
  read-side projection ownership.
- Added Operations read/manage permissions to the finite provisioning owner
  assignment so a newly initialized holding can operate its exception queue.

## Evidence

- `cargo check -p erpx-server` — PASS.
- `cargo fmt --all --check` — PASS.
- `cargo clippy -p erpx-server --all-targets -- -D warnings` — PASS.
- `cargo test -p erpx-server --all-targets` — PASS; 81 passed and one
  disposable owner-seed test remained ignored without database URLs.
- Current disposable PostgreSQL
  `exception_lifecycle_is_atomic_versioned_idempotent_and_holding_bound` —
  PASS; 1 passed, with the temporary database removed after the run.
- Full post-integration wave — PASS: workspace clippy/tests, Dioxus
  web/desktop checks, Folio lint/typecheck, `git diff --check`, PostgreSQL
  readiness, Folio `/operations` HTTP 307, and MinIO health HTTP 200. Folio
  dev server remained active on port 3004.

## Security boundary

The transport never accepts an arbitrary actor validity interval and never
writes source Sales, Procurement, Inventory, Finance, Maintenance, Support, or
Integration records. It also does not infer manager hierarchy or expired-user
replacement; those remain explicit authoritative role/workflow integrations.

## Remaining work

- Add policy publication/manager-authority evaluation, notification delivery,
  and external identity-expiration/manager-hierarchy integration.
- Add projection freshness/queue UI adapters and Folio reconciliation.
- Complete the G7/G14 exception recovery and end-to-end evidence gates.

The persistent ERPX goal remains active.

## Coordinator continuation — source outbox feed adapter — 2026-07-28

Added `SqlxOperationsStore::read_source_outbox_event_page`, a bounded,
holding-routed source feed for the Operations detection scheduler. It reads
only non-Operations holding outbox rows, requires the shared envelope's
holding-scope, actor, event-version, correlation, identity, and timestamp
fields, and maps only the closed Sales, Procurement, Inventory, Finance,
Maintenance, Support, and Integrations domain set. Unknown prefixes and legacy
rows without authoritative metadata fail closed. The adapter returns immutable
`OperationsSourceEvent` facts while leaving payload ownership with the source
context.

Focused infrastructure tests and warnings-denied infrastructure Clippy pass;
the worker crate compiles against the exported feed contract. No fabricated
source event, database fixture, Folio write, directory activation, or
production cutover was used. Automatic policy evaluation, SLA notification
dispatch, manager-chain resolution, and live source/vendor certification
remain open.

## Coordinator live local source-feed acceptance — 2026-07-28

Against fresh disposable PostgreSQL databases, the ignored worker acceptance
test `source_reconciliation_is_authoritative_and_scope_isolated` passed. It
inserted a complete source-owned holding outbox envelope, verified that the
Operations reconciliation cursor inspected exactly one in-scope event, and
verified that a same-shaped event routed to another holding was counted as a
source-scope violation and made the report unhealthy. The test databases were
removed after the run.

This is local SQLx and worker evidence for the feed-integrity fence. It does
not claim external source-provider certification, automatic detection policy
evaluation, SLA notification dispatch, manager-chain resolution, or a live
vendor feed.

## Coordinator source-outbox detection scheduler — 2026-07-28

The registered Operations completion worker now has an explicit bounded
detection scheduler. It accepts a coordinator-supplied policy rule set and
current authorization/assignee/manager-chain proof, pages the source-owned
holding outbox, matches only explicitly named source domains and source types,
fails closed on scope escape or queue/page exhaustion, and derives stable
exception/correlation/idempotency identities for replay. It never resolves
roles or invents an assignee locally. Scheduled requests then use the existing
SQLx aggregate, append-only Operations event, holding outbox, projector, and
read-model path.

The scheduler acceptance test against fresh disposable PostgreSQL write/read
databases passed. It read one source-owned outbox envelope, produced one
policy-matched request, reproduced identical IDs on a second scan, and
persisted the assigned exception through the authoritative Operations path.
The test databases were removed after the run.

The source/application identity boundary was also corrected: source outbox
event IDs remain immutable provenance/causation, while Operations application
events use deterministic derived IDs. This prevents a source feed row from
colliding with the Operations outbox row during real scheduled detection.

Authenticated route/UI integration, automatic policy publication, manager
authority evaluation, SLA notifications, Folio parity, and external provider
certification remain open.

## Coordinator continuation — G7 due-SLA escalation scheduling — 2026-07-28

The Operations completion worker now scans the bounded active-exception read
model after projection and derives escalation requests only when a persisted
exception is assigned, due at or before the sweep time, and not already
escalated. The coordinator supplies the assigned-principal rule, manager
reference, manager-chain proof, MFA-backed authorization, and actor
attribution. Duplicate rules, scope escape, inactive proofs, missing due
times, pagination/queue exhaustion, and unsafe identities fail closed.
Escalation idempotency and correlation keys are bound to the exception,
aggregate version, and persisted due time; SQLx expected-version and due-time
checks remain authoritative before the append-only event/outbox transition.

The ignored fresh-PostgreSQL Operations acceptance projects the assignment
and reassignment events, reproduces the escalation request on two scans,
persists the escalation, and verifies the aggregate status. This is local
scheduler/persistence evidence only; policy publication, notification
delivery, authenticated UI integration, Folio parity, and live certification
remain open.

## Coordinator post-escalation verification — 2026-07-28

The full fresh-database ignored workspace matrix was rerun after this slice.
The Operations detection/source reconciliation acceptance and the extended
projection-backed reassignment/escalation acceptance passed. A flaky public
abuse fixture was corrected to anchor its test timestamp away from the
minute-window boundary; its focused SQLx acceptance and Clippy pass, and the
full matrix now has no local target failures. The sole nonzero target is the
credentialless G14 external-certification preflight, which continues to fail
closed with the exact Generic LDAP, AD DS/DirSync, and UCS references.

## Coordinator continuation — transactional SLA notification delivery — 2026-07-28

Escalation now enqueues a bounded holding-scoped SLA notification in the same
SQLx transaction as the append-only Operations lifecycle event, aggregate
snapshot, and holding outbox row. The notification inbox has forced RLS,
deterministic keys, expiry, worker claim tokens and leases, exact-worker
acknowledgement, retry release, and terminal delivered/expired states. Its
payload is allow-listed exception metadata; no provider credentials or message
body crosses the boundary.

The registered `operations_sla_notifications` worker claims due rows, emits a
deterministic `operations.sla_notification_delivered.v1` holding-outbox handoff,
and acknowledges only the exact claim. Provider delivery remains an injected
downstream boundary; an unavailable provider cannot be reported as delivered.

Focused unit tests and warnings-denied infrastructure/worker Clippy pass. The
ignored fresh two-database PostgreSQL Operations acceptance passed after
projecting detection, reassignment, and escalation: one notification was
enqueued transactionally, claimed, handed off once, acknowledged, and absent
from a replay run. This closes the local durable notification handoff only;
external provider/device delivery and live vendor certification remain queued.

## Bounded continuation — notification handoff reconciliation — 2026-07-28

The Operations reconciliation report now distinguishes the append-only
aggregate event stream from the intentional SLA notification delivery handoff.
An outbox-only `operations.sla_notification_delivered.v1` row is healthy only
when its deterministic event ID, aggregate identity/version, payload,
timestamp, envelope metadata, worker actor, and idempotency/correlation keys
match the forced-RLS `operations_sla_notifications` inbox. A generic orphan or
tampered handoff remains unhealthy and is not repaired by reconciliation.

The worker acceptance now drives the real SQLx notification claim/publish/
acknowledgement path, projects the escalation read row, and asserts that the
valid delivery handoff leaves the final Operations reconciliation healthy.
The existing projection rebuild rollback remains read-side-only: it clears
the disposable Operations read rows/checkpoints (including the detection
cursor) and replays immutable Operations events; it does not delete the
notification inbox or holding outbox handoff.

Verification for this continuation: owned-file rustfmt check passed; Folio
`bun run lint` exited 0 with four pre-existing warnings in
`infra/scripts/start-n8n.js`; `bunx tsc --noEmit` exited 0. The focused Rust
targets could not compile because the pre-existing, non-Operations
`crates/domain/src/quality.rs:373` uses `Option::is_some_and(Uuid::is_nil)`
with an incompatible function signature under the current toolchain. That
file was not changed, and no live PostgreSQL acceptance result is claimed for
this continuation.
