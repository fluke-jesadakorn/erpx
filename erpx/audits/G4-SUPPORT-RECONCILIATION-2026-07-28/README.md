# G4 Support capability reconciliation — 2026-07-28

## Scope

Support reconciliation now compares bounded authoritative capability
bindings with the tenant read projection and detects tampered, missing,
orphaned, stale, and oversized binding sets. The recovery assertion rebuilds
the read projection from the authoritative SQLx state through the existing
outbox/projector path, with tenant and holding scope checks retained.

## Evidence

- Focused support workflow tests and integration compilation passed.
- Strict infrastructure Clippy and formatting passed after integration.
- Real disposable PostgreSQL support workflow acceptance: 1 passed.
- Real disposable PostgreSQL support worker projector acceptance: 1 passed.
- The focused acceptance also exposed and verified two fixes: non-
  impersonation sessions reject represented-user bindings before persistence,
  and append-only revocation transitions capabilities before revoking their
  access request so the database binding trigger remains satisfiable.

## Safety and rollback

Reconciliation is read-only until an explicitly authorized rebuild is invoked.
The authoritative capability bindings and append-only events remain the
source of truth; a failed rebuild can be retried from its checkpoint. No
customer directory activation, command-owner switch, or production cutover
occurred.
