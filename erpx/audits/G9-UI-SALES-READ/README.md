# G9 Sales authenticated aggregate read surface

## Scope

The Dioxus `/sales` product route now consumes the existing
`SalesService::GetSalesAggregate` read boundary. It requires explicit tenant,
holding route, holding UUID, aggregate type, and aggregate UUID inputs. The
console never infers a holding or renders a platform-scope response as tenant
data.

## Authoritative path

- The client sends an authenticated, audited holding-scoped `QueryContext`.
- The server owns `sales.read` authorization, tenant read routing, immutable
  source/read persistence, outbox/projector/reconciliation behavior, and
  missing-row semantics.
- The adapter validates response aggregate identity, correlation identity,
  presence/version consistency, and the one-megabyte transport bound, then
  discards `aggregate_json` before creating the display model.

The UI renders explicit scope, loading, empty, denied, offline, and error
states, plus only aggregate type, UUID, holding reference, source version, and
correlation metadata. Save, approval, fulfillment, payment, and other Sales
commands remain server-owned and audited.

## Evidence

- `cargo test -p erpx-console --offline`: 68/68 PASS, including Sales
  identity/scope validation, raw aggregate-byte redaction, missing-response
  consistency, and safe error-state coverage.
- `cargo fmt --all --check`: PASS.
- Existing Sales SQLx persistence, projector, reconciliation, server, and
  worker acceptance suites remain the authoritative domain/event/read evidence.

This closes the local G9 Sales read adapter only. Full Lead-to-Cash,
Procure-to-Pay, SCM, Finance reconciliation, UI workflow, rollback, and
external certification acceptance remain open under the coordinator matrix.
No activation, owner switch, Folio write retirement, or cutover occurred.

## Coordinator continuation — Sales lifecycle command — 2026-07-28

The Sales surface now includes an authenticated lifecycle command for an
already loaded aggregate. `TransitionSalesAggregate` accepts only bounded
allow-listed transitions, requires `sales.write` and the current source
version, and returns a bounded receipt. The SQLx source writer applies the
domain transition against authoritative write state and commits the source,
snapshot, outbox, and idempotency record atomically; the console refreshes its
read model after success and never renders the raw aggregate payload.

The ignored Sales persistence acceptance passed with a real disposable
PostgreSQL write/read pair, including the transition, projection,
reconciliation, clear-and-replay rebuild, and scope rejection. This closes only
the local Sales lifecycle command seam. Full Lead-to-Cash handoffs/payment,
worker restart, accounting/Operations reconciliation, vendor/device, and
production acceptance remain open.
