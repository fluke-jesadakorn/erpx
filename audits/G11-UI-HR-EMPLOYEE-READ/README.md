# G11 authenticated HR employee lifecycle read

## Scope

The Dioxus HR & Payroll route now exposes the existing authenticated
HrService::ListHrEmployees projection for an explicit tenant, holding,
company, and optional branch UUID scope. The adapter is read-only and
deliberately excludes payroll amounts, credentials, identity-proof material,
and HR mutation controls.

## Authoritative path

- The console sends an authenticated/audited query context to the existing HR
  service.
- The server performs explicit company/branch containment, hr.read
  authorization, customer-scope containment, tenant-read routing, bounded
  pagination, SQLx projection access, and audit persistence.
- The client retains only employee UUID/reference, display name, lifecycle
  status, scoped company/branch, dates, source version, projection timestamp,
  and correlation metadata. Source event IDs and payload material are not
  displayed.
- The route renders explicit scope, loading, empty, denied, offline, error,
  and safe read states. Payroll, leave, expense, reassignment, and employee
  writes remain server-owned.

## Fail-closed boundary

HrEmployeeScope rejects invalid tenant/holding/company/branch references.
HrEmployeeReadModel::from_wire rejects oversized pages, malformed or
contradictory UUID cursors, scope escapes, unsafe values, unknown lifecycle
statuses, invalid dates, inconsistent terminated-state/date combinations,
zero source versions, and malformed projection timestamps.

## Evidence

- cargo check -p erpx-console --offline --no-default-features --features web:
  PASS.
- cargo test -p erpx-console --offline --no-default-features --features web:
  78/78 PASS, including company/branch containment, lifecycle consistency,
  safe date handling, cursor rejection, and source-payload redaction.
- Existing HR SQLx persistence, outbox/projector, reconciliation, and server
  acceptance suites remain the authoritative domain evidence.

This packet closes only the local HR employee read adapter. Full HR/payroll
workflows, leave/expense/payroll/reassignment UI, device journeys,
independent database acceptance, migration/cutover, and G0-G14 production
evidence remain open. No activation, command-owner switch, Folio write
retirement, or cutover occurred.
