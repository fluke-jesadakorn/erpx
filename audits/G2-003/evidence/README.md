# G2-003 evidence — write-side role authorization slice

## Delivered behavior

`SqlxControlPlane::principal_has_role` loads role payloads and command snapshots
from the control write database and fails closed when a role is missing,
expired, outside the requested scope, missing the required permission, has a
missing parent, has a cyclic parent chain, or violates parent scope, authority,
or permission containment. Support and role mutation RPCs invoke this check
after session authentication and before application command execution.

## Independent evidence

The disposable `sqlx_control_plane` integration test verifies an active role
allows its permission and denies an unrelated permission. Domain/application
tests verify exact role boundaries, parent validation, and high-risk approval.

## Remaining boundary

This is not the complete universal evaluator. Billing state, commercial module
entitlement, approval limits, segregation of duties, support capabilities,
represented-user authority, tenant routing, and revocation review workflows
must be joined before G2 is accepted.
