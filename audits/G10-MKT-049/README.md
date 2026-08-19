# G10-MKT-049 — Marketing consent and suppression delivery enforcement

## Scope

This packet adds independent acceptance coverage for the final Marketing
delivery claim boundary. The existing authoritative implementation loads
consent and suppression rows from the holding write database inside the claim
transaction before `DeliveryDispatch::begin_attempt` can reserve a provider
attempt.

The live proof covers:

- a late active suppression cancelling queued work before claim;
- revoked consent preventing a provider claim;
- the exact consent-expiry boundary preventing a provider claim;
- no claim owner or provider authorization being persisted for blocked work;
- forced RLS and append-only consent migration contracts.

Consent records remain append-only. Fixture cleanup temporarily disables the
append-only trigger inside one cleanup transaction, deletes only the unique
test holding rows, restores the trigger, and commits.

## Changed paths

- `crates/infrastructure/tests/marketing_delivery_consent.rs`

No production, migration, worker, transport, or Folio behavior changed.

## Evidence

- Contract test — PASS.
- Disposable PostgreSQL test `late_consent_and_suppression_changes_block_provider_claims` — PASS.
- Strict infrastructure Clippy — PASS.
- Focused infrastructure test target — PASS.
- Fixture queries confirmed zero `marketing-consent-*` delivery and consent rows.

## Open dependencies

This packet does not claim completion of provider adapters, campaign/journey
providers, Marketing UI, CRM attribution, remaining ERP contexts, migration
cutover, or Folio write retirement.
