# G2-002 evidence — universal immutable expiring role assignments

## Scope

This packet implements the role-assignment aggregate and application lifecycle
invariants in `erpx-domain` and `erpx-application`. It does not change shared
contracts, protobuf definitions, persistence migrations, transport code, or
Folio.

## Delivered behavior

- Role windows are finite, risk-tier bounded, and half-open:
  `valid_from <= at < valid_until`.
- The maximum `DateTime<Utc>` sentinel is rejected as a permanent assignment.
- Delegated assignments must reference the supplied parent and remain within
  the parent’s start/end window, scope, authority level, and permissions.
- High-risk grants and renewals require a non-empty approver distinct from the
  grantor.
- Renewal validates the new window, cannot backdate before the source version,
  creates a new UUID, increments the immutable assignment version with
  overflow protection, and leaves the source value unchanged.
- `RoleLifecycleService` validates grant and renewal invariants, including
  parent lookup/revalidation.
- `AuthoritativeRoleLifecycleService` now supports command-backed grants and
  renewals. Parent assignments are loaded and revalidated before a command is
  emitted.
- Effective access continues to require parent activity and parent scope
  containment at the evaluation instant.

## Contract and migration impact

No files under `erpx/crates/domain/src/contracts.rs` or
`erpx/crates/proto` changed. No database migrations changed or were added.

New domain surface is limited to role validation methods and typed
`AssignmentError` variants. The application surface adds
`AuthoritativeRoleLifecycleService::grant`; the existing renewal signature is
preserved.

## Verification

Run from `erpx/`:

```text
cargo fmt --all
cargo test -p erpx-domain -p erpx-application
cargo clippy -p erpx-domain -p erpx-application --all-targets -- -D warnings
```

Results:

- `erpx-domain`: 20 tests passed.
- `erpx-application`: 4 tests passed.
- Doc-tests: passed for both packages.
- Clippy for both packages and all targets: passed with `-D warnings`.

Focused tests cover exact boundaries, finite expiry, parent scope/window,
authority and permission containment, immutable renewal, parent revalidation,
and two-person high-risk approval.

## Unfinished work and security boundary

This packet does not implement persistent role-assignment repositories,
revocation storage, recursive parent-chain loading, approver identity/role
resolution, or server interceptor wiring. Callers must use the validated
application lifecycle or the domain validation methods; a future persistence
adapter must not expose an unchecked mutation path. Universal access evaluation
still composes identity/session, entitlement, billing, support capability, and
segregation-of-duties evidence at the authoritative command boundary.

## Integration order and rollback

Integrate after the existing identity primitives and before server-backed RBAC
mutation adoption and G2-003 effective-access enforcement. Independently rerun
the focused package tests, then the workspace checks and persistence tests.
Rollback is limited to reverting the changes in `erpx/crates/domain/src/access.rs`,
`erpx/crates/application/src/lib.rs`, and this evidence file; no migration or
runtime state is affected.
