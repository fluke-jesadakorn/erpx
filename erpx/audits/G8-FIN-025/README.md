# G8-FIN-025 — fail-closed bank-provider credential boundary

## Contract

- **Owner:** main agent
- **Dependencies:** G8-FIN-022 exact control-plane provider routing and
  G8-FIN-013 provider-neutral adapter boundary.
- **Allowed paths:** infrastructure secret-manager composition boundary,
  focused unit tests, this audit, and plan/ledger records.
- **Forbidden paths:** PostgreSQL credential storage, provider SDKs, tenant
  banking persistence, server authorization, worker scheduling, Finance UI,
  Folio, and unrelated ERP contexts.

## Delivered behavior

- Added a validated opaque `BankProviderCredentialKey` for secret-manager
  lookup references. It rejects empty, oversized, absolute, traversal, and
  control-bearing values without interpreting provider-specific key contents.
- Added `ResolvedBankProviderCredential`, backed by zeroizing memory and a
  redacted `Debug` implementation. Provider adapters can borrow material only
  through a short-lived closure; the value is never serializable or persisted
  by this boundary.
- Added the asynchronous `BankProviderCredentialResolver` seam and an
  explicit `UnconfiguredBankProviderCredentialResolver` that fails closed.
- Added bounded, redacted credential errors and tests proving secret material
  and key references do not appear in diagnostics.

## Evidence

- `cargo fmt --all --check` — PASS.
- `cargo test -p erpx-infrastructure finance_bank_credentials -- --nocapture`
  — 3 PASS.
- `cargo clippy -p erpx-infrastructure --all-targets -- -D warnings` — PASS.

## Remaining boundary

Deployment-specific KMS/Vault/host secret-manager adapters, provider SDKs,
worker composition, automatic imports, bank-to-GL proposal generation, and
the remaining Finance requirements remain open. No credential is read from or
written to PostgreSQL by this packet.

## Rollback

Remove the additive module and export. The existing opaque control-plane route
and provider DTO boundary remain unchanged; no tenant facts or credential
material are modified.
