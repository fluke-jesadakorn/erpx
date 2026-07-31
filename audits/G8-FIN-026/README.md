# G8-FIN-026 — authorized worker bank-provider import workflow

## Contract

- **Owner:** main agent
- **Dependencies:** G8-FIN-022 exact control-plane provider routing,
  G8-FIN-024 bounded pagination, and G8-FIN-025 credential resolution.
- **Allowed paths:** worker-side import composition, worker tests, this audit,
  and plan/ledger records.
- **Forbidden paths:** worker scheduler registration, HTTP/gRPC transport,
  provider SDK implementation, database migrations, Folio, UI, and unrelated
  ERP contexts.

## Delivered behavior

- Added `FinanceBankProviderImportWorkflow`, which requires an injected
  authoritative authorizer and an injected provider adapter factory.
- Validates command context, non-platform scope containment, provider/account
  identity, bounded dates/page size, and a non-nil import ID before external
  calls.
- Resolves the exact active control-plane route using provider account,
  provider key, tenant, holding, and credential-key reference; every returned
  route field is checked again against the requested account.
- Resolves credentials only through the `BankProviderCredentialResolver`, then
  passes them to the provider factory as a secret-manager object. No raw
  credential is accepted in the request or persisted.
- Runs the bounded `BankProviderImportCoordinator`, then opens physically
  separate routed tenant write and read connections and persists the account
  registration and statement import through the existing SQLx write boundary.
- Returns only typed account/import receipts. Persistence, route, provider,
  authorization, and credential failures are mapped to fixed redacted error
  categories.
- Does not register a scheduler, expose a transport command, select a command
  owner, or alter Folio.

## Evidence

- `cargo fmt --all` — PASS.
- `cargo test -p erpx-worker finance_banking_import -- --nocapture` — 2 PASS.
- `cargo clippy -p erpx-worker --all-targets -- -D warnings` — PASS.

The executable workflow remains intentionally injection-based; a disposable
database integration test is still required after a production worker command
and secret-manager/provider adapter are composed.

## Remaining boundary

Deployment-specific secret-manager and provider SDK adapters, authorized
transport/scheduler registration, database-backed import evidence, automatic
bank-to-GL proposal generation, journal application, reconciliation, Finance
UI, and the broader G8 gate remain open.

## Rollback

Remove the additive worker workflow/module export. G8-FIN-024 pagination,
G8-FIN-025 credential resolution, control-plane routing, tenant persistence,
and Folio state remain unchanged.
