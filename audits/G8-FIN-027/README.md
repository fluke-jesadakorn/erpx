# G8-FIN-027 — fail-closed provider adapter registry

## Contract

- **Owner:** AG-FIN, independently integrated by the main agent
- **Dependencies:** G8-FIN-026 injected import workflow and the typed
  `FinanceBankProviderAdapterFactory` seam.
- **Allowed paths:** one worker registry module, its export, focused tests,
  this audit, and plan/ledger records.
- **Forbidden paths:** provider SDKs, credentials, network calls, scheduler or
  transport registration, persistence, UI, Folio, and unrelated contexts.

## Delivered behavior

- Added `BankProviderAdapterRegistry` with a fixed bound of 16 explicitly
  injected factories.
- Provider keys use the existing restricted lowercase identifier alphabet and
  reject empty, unsafe, overlong, duplicate, or excessive registrations.
- Dispatch is exact-key only, verifies the returned adapter reports the same
  provider key, and returns a fixed redacted error for missing providers.
- Empty registries fail closed; no default, fallback, in-memory, or network
  provider is selected.
- Registry debug output contains only a provider count. Construction and
  dispatch errors do not echo keys, URLs, credentials, or provider payloads.

## Evidence

- `cargo fmt --all` — PASS.
- `cargo test -p erpx-worker finance_bank_provider_registry -- --nocapture`
  — 5 PASS.
- `cargo clippy -p erpx-worker --all-targets -- -D warnings` — PASS.

## Remaining boundary

Real provider SDK factories, secret-manager deployment adapters, authorized
transport/scheduler registration, disposable database workflow evidence,
bank-to-GL generation, reconciliation, Finance UI, and the broader G8 gate
remain open. The registry is intentionally unconfigured in the production
composition until those dependencies are supplied.

## Rollback

Remove the additive registry module and export. The generic worker import
workflow and provider-neutral DTOs remain unchanged, and no runtime provider
or Folio state is affected.
