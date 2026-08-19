# G7 Operations authenticated read surface

## Scope

This packet wires the existing authoritative Operations read boundary into the
Dioxus console for `/operations`. It is deliberately bounded to a holding and
holding read scope selected by the operator; the console never guesses a
holding from platform metadata and never renders a platform-scope result as a
holding result.

## Authoritative path

- `OperationsService::GetOperationsSummary` reads the persisted Operations
  summary through the server holding router and requires `operations.read`.
- `OperationsService::ListOperationsExceptions` reads the persisted active
  exception projection with a bounded page size and the same scope/role
  checks.
- The existing Operations aggregate, holding outbox, worker projector, read
  model, reconciliation, and exception command path remain server-owned. The
  console does not introduce a cache, fixture, local fallback, or mutation
  path.
- Each request carries an authenticated session, holding holding query scope,
  actor metadata, and an audited Operations purpose. Server categorical errors
  are mapped to safe UI states without transport/backend details.

## Display boundary

`OperationsReadModel::from_wire` rejects scope mismatches, an unrecognized
projection, missing correlation IDs, unsafe/unbounded scalar fields, oversized
exception pages, and cursors that contradict `has_more`. It renders only
bounded summary counters, source-domain counts, exception category/severity/
status/assignment/SLA timestamps, and correlation identifiers. Credentials,
database routes, outbox payloads, worker failures, and local commands are not
displayable.

The UI exposes an explicit scope form, loading/empty/denied/offline/error
states, a bounded 25-row exception page, and persistence/reconciliation copy.
The lifecycle form requires a UUID exception, allow-listed action, positive
expected aggregate version, and the additional assignment/SLA fields required
by the selected action. It sends a holding-scoped `CommandContext` with a
fresh idempotency key to `TransitionOperationsException`; the server performs
the permission, active-principal, aggregate-version, domain, SQLx event/outbox,
worker, and audit work. A successful receipt triggers a fresh read. Stale
versions and rejected commands are rendered as safe categories; no client-side
fallback or optimistic local mutation is enabled.

## Evidence

- `cargo test -p erpx-console --offline`: 67/67 passed, including scope
  validation, wire-boundary redaction, malformed-projection rejection, and
  safe error-state and holding-scoped command-context tests.
- `cargo check -p erpx-console --offline`: passed.
- Existing Operations SQLx persistence/projector/reconciliation and server
  acceptance suites remain the authoritative behavior evidence; this packet
  adds only the console adapter and its fail-closed tests.

No production activation, command-owner switch, Folio write retirement, or
cutover occurred. Credentialed vendor certification, measured external
one-million-user/five-minute evidence, and live production migration evidence
remain queued under the coordinator ledger.
