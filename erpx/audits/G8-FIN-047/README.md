# G8-FIN-047 — Authenticated Finance period and journal posting transport

## Objective

Expose the existing Finance write-side period and immutable journal-posting
rules through authoritative, entitlement-checked Tonic commands.

## Delivered behavior

- `SaveFinancePeriod` validates bounded JSON, explicit scope, optimistic
  version context, and Finance entitlement/RBAC before tenant-write routing.
- `PostFinanceJournal` validates chart, period, journal balance, identity,
  command context, company/holding/period bindings, and explicit scope before
  tenant-write routing.
- Both commands use only the tenant write pool and record customer-scoped
  audit events.
- Journal posting reuses the atomic `SqlxFinanceStore::post_journal` boundary:
  authoritative chart/account loading, period locking, immutable posted state,
  account-usage facts, aggregate snapshot, outbox event, and idempotency replay.
- Responses contain only bounded scalar identifiers, versions, timestamps,
  and replay state; no raw database or credential material crosses transport.

## Evidence

- `cargo fmt --all --check` — PASS.
- `cargo check -p erpx-proto -p erpx-server` — PASS.
- G2 authorization route inventory — PASS; the new Finance mutation methods
  also reach `authorize_entitled_command`.

## Nonclaims

This packet does not claim AP/AR completion, bank/cash reconciliation,
period-close transport/workers/UI, consolidation, Thai tax, migration
reconciliation, or G0-G14 completion.
