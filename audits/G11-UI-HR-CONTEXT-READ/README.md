# G11 authenticated HR context read bundle

## Scope

The authenticated Dioxus HR & Payroll route now consumes the existing typed
HrService read RPCs for employees, leave requests, expense claims, payroll
runs/lines, and expired-role reassignments under one explicit tenant, holding,
company, and optional branch scope.

## Authoritative path

- Each request carries an authenticated query context and platform metadata.
- The server remains authoritative for hr.read authorization, customer-scope
  audit, tenant-read routing, SQLx projection state, source versions, and
  rebuildable outbox history.
- The console retains only bounded scalar fields, opaque identifiers, lifecycle
  statuses, reconciled payroll amounts, and safe projection metadata.
- No receipt, account, tax, bank, payment-instrument, credential, attachment,
  raw payload, or identity-proof material is rendered.

## Fail-closed boundary

HrReadBundle::from_wire rejects page overflow, invalid cursors, scope or
branch escapes, malformed UUIDs, unknown lifecycle statuses, reversed dates,
negative or unreconciled amounts, invalid currencies, payroll line/aggregate
drift, missing source versions, unsafe source metadata, and malformed
projection timestamps. The route keeps loading, empty, denied, offline, error,
and explicit read-only states visible.

## Evidence

- cargo test -p erpx-console --offline: 100/100 PASS.
- Workspace Clippy with warnings denied: PASS.
- Workspace non-ignored tests: PASS.
- Console web, desktop, host-mobile, iOS, and Android target checks: PASS.
- Shared UI iOS and Android target checks: PASS.
- The required ignored workspace sweep remains a 66-target queued
  infrastructure/vendor set because disposable PostgreSQL URLs, MinIO/PITR
  credentials, local directory SQLx references, and external certification
  inputs are unavailable.

## Rollback

Remove the bundle adapter, its route rendering, focused test, and this audit
directory. No database migration, server registration, worker registration, or
runtime state changed in this presentation/read-adapter packet; authoritative
HR source and read services remain unchanged.

This packet closes the local HR leave/expense/payroll/reassignment read
adapter only. Live database acceptance, native device execution, broader HR
workflow completion, migration/cutover, and G0-G14 closure remain open.
