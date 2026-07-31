# G6-G12-UI-013 — Authenticated Platform audit read surface

## Objective

Expose the safe Platform Operations audit route at `/platform/audit` while
keeping audit payloads and customer identifiers out of the transport and
Dioxus display model.

## Delivered behavior

- `SqlxControlPlane::list_platform_audit_page` reads only the allow-listed
  audit columns from the control-plane read pool.
- The Tonic method requires an active authenticated Platform scope and
  `platform.tenant.read`, validates the bounded page size and paired
  `(recorded_at, audit_id)` cursor, and records the read as
  `platform.audit.list`.
- The protobuf response contains only audit ID, actor, action, purpose,
  correlation, timestamp, and a customer-scoped flag. It has no payload or
  customer-account field.
- WebAssembly gRPC-Web and native Tonic clients follow the bounded cursor and
  map the response into a display-safe model.
- The Dioxus route renders explicit loading, denied, error, empty, and
  read-only states and never exposes mutation controls or raw backend errors.

## Verification

- `cargo check -p erpx-server` — PASS.
- `cargo test -p erpx-console` — PASS; 21 tests.
- `cargo check -p erpx-console --no-default-features --features web` — PASS.
- `cargo check -p erpx-console --no-default-features --features desktop` — PASS.

## Open boundaries

This packet does not claim completion of the remaining integrations/settings
Platform Operations routes, customer-security projection wiring beyond the
existing Customer Access Audit route, mobile target builds, or the G6/G12/G14
gates. A credentialed live database read proof remains environment-dependent;
the repository boundary is covered by compilation and the existing SQL
integration fixtures.

## Rollback

Revert the additive protobuf RPC, repository method, server handler, and
console adapter/component together. Existing Platform Operations routes and
the underlying append-only audit writer remain available.
