# G2-005 role review and final-authority recovery

## Scope

This packet implements transport-neutral domain/application behavior for
expiry review queues, expiry notifications, safe reviewed renewal, and final
Platform Root/Holding Owner recovery. It does not claim the G2 identity/RBAC
gate, persistent review queues, worker notification delivery, approver
identity resolution, or transport adoption.

## Delivered behavior

- `build_role_review_queue` uses the exact half-open role window and an
  inclusive expiry horizon. It classifies expired, expiring-soon, orphaned,
  excessive, conflicting maker/checker, and future/inactive assignments.
- Review items and expiry notifications carry only typed, non-secret role
  metadata. Notification keys and queue serialization are deterministic after
  permission/item normalization.
- `RoleAssignment::renew_with_reviewer` records a non-empty reviewer and
  rejects reviewer self-approval or reviewer/approver reuse. Existing finite,
  immutable renewal behavior remains unchanged.
- `RoleRecoveryRequest::materialize` can create only a target-specific,
  version-one, finite Platform Root or Holding Owner assignment. It rejects
  permanent windows, backdated/non-immediate recovery, missing actors,
  requester self-approval, reviewer reuse, parent assignments, and any active
  final target assignment.
- `RoleLifecycleService::review_queue` and
  `AuthoritativeRoleLifecycleService::{review_queue,recover_final_role,
  renew_with_reviewer}` expose the domain behavior through application ports.
  Final recovery additionally requires the request actor to match the command
  context actor and emits `role_assignment.recovered` through the existing
  command unit of work.

## Files changed

- `erpx/crates/domain/src/access.rs`
- `erpx/crates/application/src/lib.rs`
- `erpx/audits/G2-005-ROLE-REVIEW-045/README.md`

No migration or infrastructure file was changed. No permanent-role creation
path was added.

## Tests and exact verification

Added focused domain tests for:

- exact expiry and half-open boundary behavior;
- expired, expiring-soon, orphaned, excessive, conflicting, and inactive
  classifications;
- two-person final-root recovery and self-approval rejection;
- final-owner recovery blocked by an active owner and allowed after exact
  expiry;
- permanent-assignment rejection;
- reviewer/approver constraints;
- canonical recovery, queue, and notification serialization.

Results after independent integration repair, from `erpx/`:

| Command | Result |
|---|---|
| `cargo fmt --all --check` | PASS |
| `cargo test -p erpx-domain --lib` | PASS; 73 tests |
| `cargo test -p erpx-application --lib` | PASS; 8 tests |
| `cargo clippy -p erpx-domain -p erpx-application -p erpx-infrastructure -p erpx-worker --all-targets -- -D warnings` | PASS |

The Customer 360 and backup/restore packets were repaired and verified in the
same integration wave; their original blocked handoff results are retained in
their audit notes but are not completion evidence.

The blocked results are recorded rather than treated as passing evidence.

## Residuals and security concerns

- Review queues are computed from the existing `RoleAssignmentStore` and are
  not yet persisted, checkpointed, deduplicated, or delivered by the worker.
- Expiry notifications have deterministic typed payloads but no scheduler,
  channel, retry, suppression, or audit producer.
- The application store still exposes a bounded-list abstraction; this packet
  requests `u32::MAX`, but a production repository must provide an explicit
  complete snapshot/read contract before final-authority recovery is used as a
  production command boundary.
- Approver/reviewer strings are command metadata. Authenticated approver role,
  identity, session, entitlement, SoD, and holding/scope checks remain required
  at the server/write boundary.
- Recovery is command-backed but has no transport, persistence-specific
  idempotency/replay response, audit-read projection, or worker escalation.
- No G2 gate, ERPX command-owner switch, or Folio retirement is claimed.

## Integration order

1. Repair the unrelated Customer 360 compile errors and crate-wide formatting
   drift.
2. Add a complete authoritative role snapshot/review repository contract and
   persist review/notification receipts without changing the finite domain
   invariants.
3. Resolve approver/reviewer identities through the authoritative identity and
   RBAC services; enforce entitlement, SoD, scope, and session conditions.
4. Integrate worker expiry notifications and final-root/owner recovery command
   transport with idempotent command receipts and customer/platform audit.
5. Independently verify exact clock boundaries, parent cascade, recovery
   concurrency, replay behavior, and security isolation before any G2 gate
   decision.

## Rollback

Rollback is limited to reverting the additions in `access.rs`,
`application/src/lib.rs`, and this audit directory. No database, runtime,
Folio, migration, transport, worker, or UI state was changed by this packet.
