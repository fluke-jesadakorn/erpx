# G6-PRV-008 — Platform Operations tenant registry read API

## Scope

This main-agent packet adds the first server-owned Platform Operations data
adapter. It is a bounded read slice and does not close the G6 gate.

## Delivered behavior

- Added `PlatformOperationsService.ListTenants` to the Tonic/gRPC and
  gRPC-Web contract.
- Added deterministic keyset pagination over all tenant registry states with a
  default page size of 25 and a hard maximum of 100.
- Added a server-side registry page query and safe readiness summary mapping
  for routing, projection, backup/PITR, and storage state.
- Requires an authenticated active session, platform query scope, and the
  authoritative `platform.tenant.read` role permission.
- Records a platform audit event containing actor, purpose, action, and
  correlation ID.
- Exposes only opaque tenant ID, registry state/version, and readiness state
  labels. Database keys, storage namespaces, credential references, backup
  references, and SQL errors never enter the protobuf response.

## Changed files

- `crates/proto/proto/erpx/platform/v1/platform.proto`
- `crates/infrastructure/src/repositories.rs`
- `crates/infrastructure/src/lib.rs`
- `apps/server/src/main.rs`

## Independent verification

- `cargo fmt --all --check` — PASS.
- `cargo clippy -p erpx-server --all-targets -- -D warnings` — PASS.
- `cargo check -p erpx-server` — PASS.
- `cargo test -p erpx-server --bin erpx-server` — PASS: 56 tests, including
  bounded Platform Operations page-size security coverage.

## Security and remaining work

The endpoint is platform-scoped and role-authorized on the write-side control
plane. Pagination is bounded and deterministic. Readiness is fail-closed at
the underlying metadata layer and is represented as a state label only.

The console currently renders the route surface in explicit not-connected and
read-only states; it does not yet consume this RPC. Tenant detail,
projection-rebuild, backup/restore, integrations, audit, settings, and
provisioning commands still require separate authoritative contracts and
authorization. This packet does not expose tenant credentials or claim live
external backup/storage operations.

## Rollback

Remove the additive protobuf method, registry page export, and server service
registration after stopping ERPX clients that may reference the method. No
tenant data, Folio state, or command ownership changed.
