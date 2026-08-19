# G10-MKT-051 — account-bound Marketing delivery and callback routing

## Scope

This packet extends the bounded provider adapter with authoritative
control-plane account routing and callback scope derivation.

Delivered behavior:

- control-plane migration `0046_marketing_provider_accounts_outbound.sql`
  stores only `provider_key` and an opaque `outbound_credential_ref`, with
  legacy rows explicitly marked unconfigured;
- active outbound resolution matches provider account, holding, and holding;
- active callback resolution derives holding and holding from the provider
  account identifier alone;
- the worker requires `ERPX_MARKETING_DELIVERY_PROVIDER_ACCOUNT_REF`, resolves
  the active account before registration, and re-resolves account/version/
  provider/credential identity before each provider submission;
- callback ingress loads the delivery from the holding write database and uses
  its stored scope, rather than company/branch/holding/holding callback
  headers;
- callback scope must belong to the provider account holding before the
  existing exact provider-reference and inbox/outbox transaction runs;
- provider requests carry the exact delivery idempotency key as an HTTP
  `Idempotency-Key` header.

## Evidence

- `cargo fmt --all --check` passed after integration edits.
- `cargo check -p erpx-worker` passed.
- `cargo clippy -p erpx-worker --all-targets -- -D warnings` passed.
- Worker provider tests passed, including local HTTP response mapping and
  idempotency-header assertion.
- `cargo check -p erpx-server` passed.
- `cargo clippy -p erpx-server --all-targets -- -D warnings` passed.
- Marketing callback signature tests passed.
- Control-plane migration/repository focused test compiles with active,
  outbound, callback, and suspended route assertions; the disposable live
  two-database proof passed and both databases were dropped.

## Explicit limits

This packet does not claim the final provider gate. The credential resolver is
still an environment-backed pilot seam, vendor-specific email/SMS/social/push
and n8n adapters are not complete, endpoint/account onboarding is not yet a
full catalog, ambiguous provider timeout reconciliation is not implemented,
and the end-to-end queued → accepted → callback → delivered disposable flow
remains open. Marketing journeys, consent scheduling, CRM effects,
attribution/Finance reconciliation, UI, and the wider G10 gate remain open.

## Rollback

Disable the explicit Marketing delivery handler and callback route through
configuration/code rollback. Do not delete delivery or callback inbox rows;
queued work remains replayable and Folio remains operational.
