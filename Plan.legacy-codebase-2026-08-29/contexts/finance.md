# Context — finance

**Crate:** `crates/contexts/finance` · **Proto:** `crates/proto/proto/erpx/finance/service.proto` · **Adapters:** `crates/adapters/finance-sqlx` (where present) · **UI:** `apps/console/src/features/finance` / `crates/ui-domains/*` / `crates/console-features/finance`

## Services & RPCs

CreateVendorInvoice, PostVendorInvoice, CreatePayment, ReleasePayment, GetVendorInvoice, GetPayment, CreateChart, AddAccount, ChangeAccountStatus, CreatePeriod, BeginPeriodClose, ClosePeriod, ReopenPeriod, CreateJournal, PostJournal, ApplySubledgerSettlement, GetChart, ListCharts, GetPeriod, GetJournal, ListJournals, GetSubledger, ListSubledger, GetTrialBalance, ListBankTransactions, GetStatutoryTaxReport

Total RPCs: 26.

## Domain (sample)

Aggregates / types: —

Source: `src/domain.rs` (and `src/application.rs` —).

## Ports & adapters

- Ports: `src/ports.rs`
- Application: `src/application.rs`
- Persistence: `crates/adapters/finance-sqlx` (SQLx, CQRS read/write split where applicable)

## Invariants

Enforced in `crates/domain` and `crates/contexts/finance/src/domain.rs` — validate via `cargo xtask test-affected` and `crates/contexts/finance` unit tests. Cross-context rules travel only through versioned proto contracts, never direct domain imports.
