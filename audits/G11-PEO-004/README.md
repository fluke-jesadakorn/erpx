# G11-PEO-004 — Authenticated HR employee command

## Delivered behavior

- Added protobuf `SaveHrEmployee` and a dedicated `HrService`.
- Decodes only bounded typed `EmployeeRecord` data; raw JSON is never returned.
- Requires the shared command context, authenticated actor/session binding,
  explicit non-platform holding scope, matching employee scope, command ID,
  idempotency key, and expected-version field.
- Routes only to the holding write database through the existing router.
- Applies customer scope, active RBAC, commercial entitlement, billing,
  segregation-of-duties, and actor authorization before persistence.
- Emits a customer-auditable `hr.employee.saved` event and typed receipt.

## Evidence

- `cargo fmt --all --check` — PASS.
- `cargo check -p erpx-proto -p erpx-server` — PASS.
- `cargo clippy -p erpx-server --all-targets -- -D warnings` — PASS.
- `cargo test -p erpx-server -- --nocapture` — PASS; 65 tests.

## Open boundary and rollback

Leave, expense, payroll, and reassignment command transports, HR read
projection/projector integration, UI, and live database evidence remain open.
Rollback removes the additive HR proto/service/handler; existing holding-write
state, other services, and Folio remain unchanged.
