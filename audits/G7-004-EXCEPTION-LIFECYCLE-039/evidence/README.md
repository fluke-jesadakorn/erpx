# G7-004-EXCEPTION-LIFECYCLE-039 evidence

Status: implemented as a transport-neutral domain and holding write-persistence packet. No route, worker, protobuf, UI, or source-transaction ownership was added.

## Delivered behavior

- Added `ExceptionLifecycleCommand` and `ExceptionLifecycleAction` for assign, reassign, acknowledge, escalate, and resolve.
- Added versioned `ExceptionCase::apply_lifecycle_command` with the existing lifecycle graph and terminal-state rejection.
- Added bounded `ExceptionSlaMetadata` (`policy_ref`, resolution `due_at`, and bounded escalation minutes).
- Added manager references to `ExceptionCase`; existing `assigned_to` remains the assignee reference.
- Added explicit half-open validity proofs for actor, assignee, and manager references. `valid_from` is inclusive and `valid_until` is exclusive; expired or future principals fail closed.
- Reassignment is a versioned mutation that preserves lifecycle status and requires active assignee and manager proofs.
- Manager escalation requires an active manager proof before the command reaches persistence.
- Added `SqlxOperationsStore::apply_exception_lifecycle_command`, which locks the holding snapshot, checks the routed holding, rechecks idempotency, enforces optimistic versioning, and atomically inserts the Operations event and snapshot.
- Replays with the same aggregate-bound idempotency key return `applied = false`; keys used by another aggregate return an idempotency conflict.
- Cross-holding writes fail closed through holding routing/RLS metadata validation and do not emit events.
- Added a public infrastructure result type for applied/replayed transition evidence.

## Files changed

- `erpx/crates/domain/src/operations.rs`
- `erpx/crates/infrastructure/src/operations_persistence.rs`
- `erpx/crates/infrastructure/src/lib.rs`
- `erpx/crates/infrastructure/tests/operations_persistence.rs`
- `erpx/audits/G7-004-EXCEPTION-LIFECYCLE-039/evidence/README.md`

## Contract and migration changes

The domain contract gained additive JSON fields `manager_ref` and `sla`, both `serde(default)` for compatibility with older ExceptionCase snapshots. No holding, control-plane, or read-side migration was required; the existing JSONB snapshot/event schema is sufficient. The public infrastructure root also contains a compatibility `Clone` implementation for the pre-existing commercial request error so the existing publication-plan error's declared `Clone` bound compiles; no commercial module file was changed.

## Verification

- `cargo fmt -p erpx-domain -p erpx-infrastructure -- --check` — PASS.
- `cargo check -p erpx-domain -p erpx-infrastructure` — PASS.
- `cargo test -p erpx-domain` — PASS: 59 passed, 0 failed.
- `cargo test -p erpx-infrastructure --lib` — PASS: 47 passed, 0 failed, 4 ignored.
- `cargo test -p erpx-infrastructure --all-targets` — PASS: 53 passed, 0 failed, 17 ignored across library and integration targets.
- `cargo clippy -p erpx-domain -p erpx-infrastructure --all-targets -- -D warnings` — PASS.
- `cargo test -p erpx-infrastructure --test operations_persistence --no-run` — PASS.
- `ERPX_TEST_HOLDING_WRITE_DATABASE_URL=postgres://... cargo test -p erpx-infrastructure --test operations_persistence exception_lifecycle_is_atomic_versioned_idempotent_and_holding_bound -- --ignored --nocapture` — PASS: 1 passed, 0 failed against a fresh disposable PostgreSQL database. The database was dropped after the run.
- `git diff --check` — PASS.

`cargo fmt --all --check` still reports one pre-existing formatting difference in forbidden shared path `erpx/apps/worker/src/lib.rs:1346`; this packet did not alter that path.

## Residual gaps

- No authenticated route, worker scheduler, detection producer, SLA monitor, expired-user lookup, or manager hierarchy resolver is claimed here. The caller must obtain active principal proofs from the authoritative identity/RBAC evaluator.
- Support/Operations authorization, assignment workflow, SLA escalation execution, notifications, projection consumption, and UI remain separate packets.

## Security concerns

The domain proof type is deliberately not an identity store: accepting a command requires an upstream authoritative evaluator to supply the validity window and matching actor attribution. The write helper never accepts a holding override, uses the routed holding transaction setting, validates persisted metadata, and performs idempotency/version checks inside the write transaction. Superuser/bypass-RLS test roles are handled fail closed by the persisted holding-reference check; production holding roles must retain forced RLS.

## Integration order

Integrate after the existing G7-003 Operations write/read seam. Server or worker adapters may construct commands only after authoritative identity/RBAC evaluation and may call the new write helper. Projection, queue, SLA worker, route, and UI packets must consume the emitted event and must not create a second Operations source of truth.

## Rollback

Revert only the five files listed under “Files changed.” No migration rollback is necessary and no control-plane/read-side data was changed. The disposable verification database was removed after testing.
