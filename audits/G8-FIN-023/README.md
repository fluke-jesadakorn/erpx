# G8-FIN-023 — Bank-match transport

## Contract

- **Owner:** main agent
- **Dependencies:** G8-FIN-016 authoritative bank-match persistence and
  G8-FIN-022 control-plane provider routing.
- **Allowed paths:** Finance protobuf/server transport, bounded transport
  tests, this audit, and plan/ledger records.
- **Forbidden paths:** holding banking persistence, provider SDKs, credential
  retrieval, worker scheduling, Finance UI, Folio, and unrelated ERP contexts.

## Delivered behavior

- Added `FinanceService.SaveBankMatchProposal` and
  `FinanceService.SaveBankMatchDecision` to the versioned Tonic/gRPC-Web
  contract.
- Commands accept bounded typed JSON representations of the already-validated
  domain proposal/decision, require an explicit holding/company/branch scope,
  and reject Platform/subtree scopes.
- The server requires an authenticated command context, actor equality with
  the embedded match context, the customer scope binding, the `finance.write`
  role, the Finance entitlement, and routed holding write access.
- Persistence reloads authoritative bank facts and posted-journal facts and
  remains responsible for atomic idempotency, proposal binding, and immutable
  decisions; matching never posts or mutates a journal.
- Responses return only bounded identifiers, deterministic idempotency keys,
  replay status, and correlation IDs. Persistence failures are redacted.

## Evidence

- `cargo fmt --all --check` — PASS.
- `cargo check -p erpx-server` — PASS.
- `cargo clippy -p erpx-server --all-targets -- -D warnings` — PASS.
- `cargo test -p erpx-server -- --nocapture` — 64 PASS after the transport
  bounds/redaction test was added.
- Existing G8-FIN-016 disposable persistence test remains the independent
  evidence for authoritative proposal/decision writes, exact replay, posted
  journal facts, database immutability, and concurrency.
- Existing G8-FIN-022 disposable control-plane test remains the independent
  evidence for provider route exactness and credential-reference redaction.

## Remaining boundary

Provider SDK and secret-manager calls, worker-driven bank imports, automatic
bank-to-GL proposal generation, journal posting/application, full Finance
read/UI workflows, approvals/SoD, AP/AR, consolidation, Thai tax, statements,
and migration reconciliation remain open. This packet does not close G8.

## Rollback

Remove the additive RPC methods and server handlers while retaining the
validated persistence boundary. No holding write facts, Folio state, or
provider credentials are changed by rollback.
