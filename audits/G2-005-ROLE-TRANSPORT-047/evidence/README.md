# G2-005 role review and final recovery transport evidence

## Scope

Added additive `RoleLifecycleService.ListRoleReview` and
`RoleLifecycleService.RecoverFinalRole` RPCs. Review reads require an
authenticated query context, `role.assignment.review`, bounded horizon (1–365
days), and scope filtering against the authoritative role assignment scope.

Recovery requires an authenticated command context, actor/requester equality,
`role.assignment.recover`, target-scope containment, RFC3339 finite expiry, and
the domain's two-person, final-target, permission, reason, and no-parent
invariants. The application sets `valid_from` from its authoritative clock;
clients cannot backdate recovery. The response contains only the safe role
assignment DTO.

## Changed files

- `erpx/crates/proto/proto/erpx/platform/v1/platform.proto`
- `erpx/crates/application/src/lib.rs`
- `erpx/apps/server/src/main.rs`

## Verification

- `cargo fmt --all` — PASS.
- `cargo test -p erpx-server` — PASS; 51 tests.
- `cargo clippy -p erpx-server --all-targets -- -D warnings` — PASS.

## Residual work

Persistent review queue/notification delivery, authoritative approver identity
and session/SoD/entitlement resolution for recovery, durable recovery replay
receipts, full universal mutation adoption, and UI/native role-review surfaces
remain open. This packet does not close G2.

## Security and rollback

Role review is scope-filtered and recovery is fail-closed on actor, permission,
target scope, and domain invariants. Recovery start time is server-authority
derived. Rollback is limited to the three changed source files and this
evidence directory.
