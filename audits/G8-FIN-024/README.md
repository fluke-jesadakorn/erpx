# G8-FIN-024 — bounded bank-provider import coordinator

## Task contract

- **Task ID:** G8-FIN-024
- **Owner:** AG-FIN
- **Dependencies:** the typed `BankProviderAdapter`,
  `BankStatementPageRequest`, `BankStatementPage`, `BankStatementFacts`,
  `ProviderBankTransaction`, `BankProviderDomainRecords`, and
  `CommandContext` contracts already present in the infrastructure/domain
  crates.
- **Allowed paths:** the additive infrastructure facts mapper, the worker
  coordinator/tests, worker module export, this audit, and plan/ledger
  evidence.
- **Forbidden paths:** SQLx repositories/migrations, server/protobuf, scheduler
  registration, credentials, provider SDKs, UI, Marketing, Finance close,
  Folio, and runtime environment state.

## Delivered behavior

`BankProviderImportCoordinator::import_statement` accepts an injected
`BankProviderAdapter`, an initial `BankStatementPageRequest`, an import UUID,
and a `CommandContext`, returning either `BankProviderDomainRecords` or the
existing redacted `BankProviderError`.

- Every page is fetched through
  `BankProviderAdapter::fetch_validated_statement_page`.
- Follow-up requests are rebuilt only from the original account/date/currency/
  page-size binding and the preceding validated opaque cursor.
- The coordinator rejects missing or repeated cursors, changed statement
  identity, account identity, request facts, duplicate or globally
  non-increasing transaction references, invalid final-page metadata, and
  fixed-limit violations.
- Adjacent pages must also have matching closing/opening balances, preventing
  a provider from silently splicing unrelated statement segments.
- The fixed bounds are 256 pages and 100,000 transactions per import.
- Opening balance comes from the first page; closing balance comes from the
  final page; transactions are combined deterministically and mapped only
  after the complete statement is validated.
- The infrastructure provider boundary now has an additive complete-facts
  mapper that permits the bounded aggregate to exceed one provider page while
  preserving the same typed, credential-free domain conversion.

No SQLx write, outbox event, scheduler registration, command-owner change,
credential lookup, network SDK, gRPC/protobuf, UI, or Folio change was made.

## Files changed

- `crates/infrastructure/src/finance_banking_provider.rs`
- `apps/worker/src/finance_banking_provider.rs`
- `apps/worker/src/lib.rs`
- `audits/G8-FIN-024/README.md`
- `MASTER_PLAN.md`
- `EXECUTION_LEDGER.md`

## Public API

- `BankProviderImportCoordinator`
- `BankProviderImportCoordinator::new`
- `BankProviderImportCoordinator::import_statement`
- `MAX_BANK_PROVIDER_IMPORT_PAGES`
- `MAX_BANK_PROVIDER_IMPORT_TRANSACTIONS`

The method is generic over the existing injected `BankProviderAdapter`; the
coordinator itself has no database, network, credential, or scheduler
dependency.

## Verification

Focused packet verification:

- `cargo fmt --all --check` — PASS.
- `cargo check -p erpx-worker` — PASS.
- `cargo clippy -p erpx-worker --all-targets -- -D warnings` — PASS.
- `cargo test -p erpx-worker finance_banking_provider -- --nocapture` — PASS:
  5 tests, including single-page and multi-page imports, first/final balance
  preservation, cursor/request binding, repeated cursors, changed facts,
  duplicate and non-increasing references, adjacent balance continuity,
  page/transaction bounds, invalid final metadata, and provider-error
  redaction.

The limit test builds only typed mock pages. It creates no database and emits
no provider payload or secret material.

## Security review

The coordinator stores only typed facts and opaque validated cursors. It does
not accept or log credentials, access tokens, raw provider responses, URLs,
payment instruments, or secret bytes. Provider failures remain the existing
closed enum and therefore cannot carry provider payloads. Request scope and
account identity stay authoritative in the initial request; provider output
cannot replace either. No persistent or command-owner state was changed.

## Remaining boundary and integration order

The production composition root still needs a provider SDK adapter, exact
control-plane route resolution from G8-FIN-022, secret-manager retrieval, and
an explicitly authorized worker workflow that passes only validated pages into
this coordinator. Holding banking persistence, bank-to-GL proposal generation,
journal application, reconciliation, Finance UI, and the broader G8 gates
remain open. The next safe order is provider/credential composition, then an
authorized worker import command that invokes this coordinator, then the
existing holding write boundary and independent reconciliation evidence.

This packet does not claim G8 or ERPX completion.

## Coordinator continuation — durable SQLx import job lifecycle — 2026-07-28

The worker-side provider import seam now has a real SQLx control-plane source:

- migration `0063_finance_bank_provider_import_jobs.sql` persists bounded
  import envelopes, source fences, claim leases, retry/dead-letter state, and
  append-only reconciliation/rollback evidence;
- read migration `0014_finance_bank_provider_import_jobs.sql` projects only
  rebuildable status fields and never request, authorization, credential-key,
  or provider payload material;
- enqueue is idempotent and validates the typed request plus MFA-derived
  authorization before persistence; claim, retry, completion, and rollback
  operations are worker/source-fenced and lease-bound;
- completion requires evidence that the authoritative holding write count and
  the import outbox event count reconcile exactly; the source does not infer
  completion from provider success;
- the production worker registry remains opt-in and requires injected router,
  authorizer, credential resolver, and provider adapter boundaries. No provider
  SDK or secret is invented or activated by this packet.

The real disposable PostgreSQL acceptance
`bank_import_jobs_persist_claim_fence_retry_reconciliation_and_read_status`
passed on 2026-07-28. It exercised idempotent enqueue, read projection,
claim/retry/reclaim, append-only rollback evidence, reconciled completion, and
status-only read-schema inspection. Focused worker/infrastructure tests and
warnings-denied Clippy also passed.

Rollback is additive: remove migration 0063/0014, the source module/export,
and the explicit registry method. Existing provider-neutral page validation,
holding banking persistence, and Folio remain unchanged. No command-owner
switch, directory activation, Folio write retirement, or production cutover
occurred.

The provider SDK/credential-manager composition, live provider certification,
holding write/outbox integration under a credentialed adapter, and broader G8
Finance gate remain open.

## Coordinator continuation — Finance holding SQLx isolation and worker evidence — 2026-07-28

Fresh disposable PostgreSQL evidence passed the Finance bank-provider worker
job acceptance 1/1 and the holding banking persistence target 2/2. The worker
test covered claim fencing, retry/reclaim, reconciliation completion, rollback
evidence, and status-only projection. The holding target covered idempotent
scoped bank writes, source/outbox reconciliation, typed matching, and
read-projection rebuild/replay.

The holding acceptance fixture now gives each ignored test a unique holding
reference so a shared disposable database cannot contaminate source-page
counts across tests. This is test isolation only; production scope and
persistence behavior remain unchanged. All temporary databases were dropped.
Provider SDK/credential composition, live vendor certification, and broader
Finance workflows remain open.

## Coordinator continuation — authenticated job-status read — 2026-07-28

The durable job read model is now exposed through the additive
`FinanceService.GetFinanceBankProviderImportJobStatus` query. The server
requires an authenticated explicit holding/holding scope and `finance.read`,
checks the returned job against both opaque route references, records an audit
event, and returns only bounded job identity, state, source-fence, attempt,
lease, failure-class, and timestamp metadata. Request JSON, authorization
facts, credential-key references, provider payloads, and secrets are not in the
protobuf response. The default worker registration remains opt-in.

The server status mapper has unit coverage for bounds, unknown-state rejection,
and payload-free output; server strict Clippy and 85/85 non-ignored tests pass.
The visible console status panel and credentialed/live provider acceptance are
still queued. Rollback is additive removal of the RPC, mapper, and existing
status read path; no source rows or Folio ownership state are changed.

## Rollback

Remove the additive worker module/export and complete-facts mapper. Existing
provider-neutral page validation, control-plane routing, and holding banking
persistence remain unchanged. No Folio state or runtime secret is affected.
