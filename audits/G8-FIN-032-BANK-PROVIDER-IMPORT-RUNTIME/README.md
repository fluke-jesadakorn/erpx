# G8-FIN-032 — Bank-provider import runtime boundary

## Delivered local path

The Finance provider import path is now end-to-end reachable locally:

1. The console submits an explicit tenant/holding scope, validated bank
   account JSON, opaque provider/account and secret-manager references, date
   window, import identity, retry bound, and fresh MFA code.
2. The Finance gRPC server authenticates the actor, checks effective
   directory-managed or local `finance.write` access and commercial scope,
   verifies fresh MFA, evaluates the high-risk Finance authorization contract,
   binds the account to the exact active control-plane route, and captures the
   route version as the source fence.
3. SQLx enqueues the durable job and read-model status. The same transaction
   appends the audit fact and `finance.bank_provider_import.queued.v1` outbox
   intent. No credentials or provider response payloads are persisted.
4. The executable worker root registers `finance_bank_provider_import` with
   explicit authorizer, credential resolver, adapter factory, tenant router,
   batch bound, and lease. The worker rechecks directory/local authorization,
   route version, MFA-derived authorization facts, provider credential, and
   provider adapter before the tenant write. Retry, dead-letter,
   reconciliation, and rollback evidence remain append-only and source-fenced.

## Verification

- `cargo check -p erpx-server -p erpx-worker -p erpx-infrastructure --offline`
- `cargo test -p erpx-worker --lib finance_banking_import --offline` — 7/7
- `cargo test -p erpx-server --bin erpx-server finance_bank_provider_import_status_tests --offline` — 1/1
- `cargo test -p erpx-console --bin erpx-console finance_bank --offline` — 3/3
- Warnings-denied focused Clippy for server, worker, and console passed.

## Queued certification

No credentialed bank-provider secret manager, concrete provider adapter, or
live bank environment is available in this workspace. The runtime therefore
uses an explicit unconfigured resolver and empty exact-match adapter registry;
claims fail closed and do not simulate provider success. Live route activation,
provider certification, high-volume import rehearsal, and production approval
remain queued. No Folio write ownership, directory activation, or cutover was
performed.
