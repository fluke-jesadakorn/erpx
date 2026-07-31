# G6-PRV-024 — Authenticated tenant provisioning transport

## Delivered

`PlatformOperationsService` now exposes two bounded lifecycle commands:

- `BeginTenantProvisioning` creates the opaque control-plane tenant registry
  and provisioning row through `TenantProvisioner::begin`.
- `ProvisionTenantResources` uses server-only PostgreSQL routing configuration
  to create/migrate the separate tenant write/read databases and uses the
  existing server MinIO boundary to create and verify the registered storage
  namespace.

Both commands require an authenticated session, an explicit platform scope,
an actor matching the authenticated principal, and the dedicated
`platform.tenant.provision` role permission. Both write a bounded platform
audit event. Responses contain only the opaque tenant identifier, state,
boolean readiness results, registry version, and correlation metadata; no
database key, storage namespace, credential reference, backup reference, URL,
or persistence detail crosses the transport.

## Verification

- `cargo check -p erpx-proto -p erpx-server` — PASS.
- `cargo test -p erpx-server --bin erpx-server` — PASS; 81 passed.
- `bash audits/G2-IAM-008/verify_authorization_routes.sh` — PASS, including
  both new lifecycle methods.
- `cargo fmt --all --check` — PASS.

No live tenant was provisioned by these checks. The commands are fail-closed
when the server-only database routing configuration or storage configuration
is absent.

## Open requirements

This packet does not implement tenant activation/suspension/recovery,
organization-owner seeding, backup/PITR scheduling/history, executable worker
backup/restore processing, integration-health/settings/audit browsing, or the
full Platform Operations UI. The G6 gate and Folio command ownership remain
open.

## Rollback

Revert the additive protobuf messages/RPCs, server handlers, authorization
route inventory, and this audit. No live database or external storage state is
modified by the verification commands.
