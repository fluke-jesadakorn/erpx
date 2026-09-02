# Finance — Finance-First Production Monthly Close

**Purpose:** Owns chart, subledgers, banking, budget, tax, close — the ledger. Finance is the **first production-real vertical** (not mockup) for Holding/Group customers; V1 closes a real month end-to-end with audit trail.

**Group:** `03-finance-sales-pos` · **Code:** `crates/contexts/finance` · **Proto:** `crates/proto/proto/erpx/finance/service.proto` · **Adapters:** `crates/adapters/finance-sqlx` · **UI:** `apps/console/src/features/finance` / `crates/console-features/finance` / `crates/ui-domains/finance`

**Actors & scopes:** `AuthorityScope::{Company, Branch}` exact via `crates/kernel`; Holding isolation + `AuthorityScope` validation (ADR-001/004/005); expiring `role_assignments` (ADR-004); periods are **per-Company** (`Company`/`Branch` scope).

**Services & RPCs (31):** FinancePayablesCommandService, FinancePayablesQueryService, FinanceCommandService, FinanceQueryService
- Payables: CreateVendorInvoice, PostVendorInvoice, CreatePayment, ReleasePayment, GetVendorInvoice, GetPayment
- Chart/Period: CreateChart, AddAccount, ChangeAccountStatus, CreatePeriod, BeginPeriodClose, ClosePeriod, ReopenPeriod
- Journals & multi-currency: CreateJournal (with `fx_rate_micros` / `base_currency_amount`), PostJournal, RevalueSubledgers, ApplySubledgerSettlement
- Migration toolkit: ImportChartCsv, ImportOpeningTrialBalance (single balancing `is_opening` journal, verifier `SUM(debit)==SUM(credit)` + hash)
- Reports: GetChart, ListCharts, GetPeriod, GetJournal, ListJournals, GetSubledger, ListSubledger, GetTrialBalance, GetFinancialStatements (P&L/BS from `AccountCategory`), ListAging (AP/AR aging from projection), ListBankTransactions, GetStatutoryTaxReport (empty/generic in v1 intl-only)

**Invariants:**
- **Double-entry & period gate:** Balanced journals (`UnbalancedJournal`); `JournalCurrencyMismatch` / `FxRateRequired` / `InvalidFxRate` enforced; `PostJournal`, `PostVendorInvoice`, `PostCustomerInvoice`, `ApplySubledgerSettlement`, `RecognizeRevenue` reject `entry_date` in `Closed` period; `TrialBalance`/`SubledgerList` filters by `period_id`; `ClosePeriod`/`ReopenPeriod` require single-human `HumanDecisionProof` with separation-of-duties, idempotent via `CommandId`/`idempotency_key`.
- **Hybrid docs:** `CreateVendorInvoice` accepts optional `ThreeWayMatchEvidence` (worker populates when `source=PO`); direct finance invoices bypass match but enforce subledger linkage. Symmetric for `CustomerInvoice`/`RevenueSchedule` via `PostingValidationSnapshot`.
- **Multi-currency (required):** `Chart.currency` is base; `Journal.currency != chart.currency` allowed only with stored `fx_rate` (`FxRate` micros scale 1_000_000, MIN 1 MAX 1e12, overflow-checked) + `base_minor` on lines; `RevalueOpenSubledgers(period_id, fx_snapshot)` produces unrealized reval `Journal`; `validate_currency` 3-letter uppercase.
- **Banking CSV-only in v1:** staging `finance_bank_transactions` → `SaveBankMatchProposal` (system) → `SaveBankMatchDecision(decision_ref: HumanDecisionProof)` → `SaveBankReconciliation(expected_version)` idempotent; `FINANCE_BANK_PROVIDER_ENABLED=false` gates `finance_bank_provider_import_jobs`.
- **International-only:** TH VAT 7%/WHT/RD/e-Tax/DBD deferred — `StatutoryTaxFact` retained but `GetStatutoryTaxReport` returns empty/generic.
- **Cutover:** parallel run + TB-match gate (legacy authoritative 1–2 closes; verifier `TB(legacy)==TB(erpx)` + `aging(legacy)==aging(erpx)` for closing date; period 0 `opening` locked after verification). Deferrals locked: budget vs actual, fixed-asset depreciation, inventory costing/variance, intercompany eliminations, bank provider live jobs.

**Data owned:** `finance_*` (chart/account/period/journal/subledger/customer_invoice/revenue_schedule) + `finance_bank_provider_import_jobs` (disabled in v1) + `fx_rates`/`reval_journal_link` in holding data pair; no new Topo tables beyond fx/reval link. Worker projections `finance_banking_projection` + `finance_tax_projection` for reval/aging.

**Depends on / depended-by:** depends on `organization`/`identity_access` for scope; collaborators link via `ReferenceQueryService` + Typed Mentions (no direct table joins). No TH statutory dependency in v1.

**Referenceable:** yes — `@` + ctx + `/<id>` → `/reference/item/:id` (typed mention). ID example: `@finance/example-28-01-2026-0001`. `@finance/<id>` backlinks on every host via `ReferenceQueryService.search` + `ListBacklinks`.

**Console routes under `/finance`:** `Overview` (KPI strip, TB health), `Chart`, `Journals`, `Subledgers`, `AP` (`/finance/ap`), `AR` (`/finance/ar`), `Cash`, `Bank → Import CSV → Match → Reconcile`, `Periods → Close/Reopen` (human-proof dialog), `Reports → TB/BS/PL/Aging`, `Budgets` (deferred). Reuses `FinanceShellStatus` (Fresh/Stale/Offline/Denied) + typed `@` chips.

**Verification:** `cargo xtask check-affected --dry-run` → `cargo xtask check-affected`; `cargo xtask test-affected` (owns `crates/contexts/finance` + console feature); final `cargo xtask verify` (format, clippy warnings-denied, workspace tests, Dioxus build, SQLx acceptance, runtime smoke). Acceptance: 2-company holding, import chart CSV (50 accounts), post 20 mixed-currency journals, hybrid vendor/customer invoices (with/without PO), CSV bank import (100 tx) → match → reconcile, run TB/BS/PL/aging, `BeginPeriodClose` → `ClosePeriod` with proof, assert closed-period posting fails, `ReopenPeriod` succeeds, parallel TB-match report equals fixture legacy TB.

ASCII: see `docs/plan/03-cross-cutting/@-and-ai/finance.md`. Traceability: `docs/plan/traceability.md`. Cutover: `CUTOVER.md` + `ops/`.

