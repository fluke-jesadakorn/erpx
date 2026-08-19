# G10-MKT-052 — end-to-end Marketing delivery proof

## Scope

This packet adds an independently runnable disposable PostgreSQL acceptance
flow for the authoritative Marketing delivery path:

`granted consent → queued dispatch → write-side claim → provider accepted →
authenticated callback → delivered state`.

The proof verifies the exact provider reference, claim authorization, delivery
version progression, callback inbox deduplication boundary, callback outbox
event, forced-RLS holding scope, physically distinct write/read database
migrations, and cleanup that temporarily disables only the append-only consent
trigger inside one transaction before restoring it.

## Evidence

- `marketing_delivery_provider_flow.rs` compiles under strict infrastructure
  Clippy.
- The disposable two-database live test passed; both exact databases were
  dropped after the test.
- The redirect policy on the provider HTTP client is disabled, so provider
  calls cannot follow an untrusted redirect.

## Explicit limits

This is an acceptance proof, not the final Marketing gate. Vendor-specific
adapters, secret-manager credential resolution, provider-account endpoint
onboarding, ambiguous timeout reconciliation, journey channel effects,
campaign scheduling, CRM/attribution/Finance reconciliation, content and
landing-page workflows, UI, and the remaining ERPX requirements remain open.

## Rollback

Remove only the test and disable the delivery/callback handlers through
configuration if required. No production rows or Folio writes are removed.
