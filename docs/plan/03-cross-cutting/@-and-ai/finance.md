# Finance — Deep `@` + AI (Production — Finance-First Monthly Close)

**Group:** `03-finance-sales-pos` · **Code:** `crates/contexts/finance` · **Proto:** `crates/proto/proto/erpx/finance/service.proto` (`FinancePayablesCommandService, FinancePayablesQueryService, FinanceCommandService, FinanceQueryService` · 31 RPCs) · **Adapters:** `crates/adapters/finance-sqlx` · **UI:** `apps/console/src/features/finance`

**Scope:** Extends `docs/plan/02-capabilities/03-finance-sales-pos/finance.md`. Central polymorphic `@` contract applies verbatim — see `_shared-contract.md` for shared @/AI/Forms/Verify/Spine/ASCII frame.

## Services & RPCs (31) — per-RPC table (differentiated, unique to finance)

- **Services:** FinancePayablesCommandService (6), FinanceCommandService (chart/journal/period), FinancePayablesQueryService, FinanceQueryService
- **RPCs:** CreateVendorInvoice, PostVendorInvoice, CreatePayment, ReleasePayment, GetVendorInvoice, GetPayment, CreateChart, AddAccount, ChangeAccountStatus, CreatePeriod, BeginPeriodClose, ClosePeriod, ReopenPeriod, CreateJournal(with fx_rate_micros), PostJournal, RevalueSubledgers, ApplySubledgerSettlement, GetChart, ListCharts, GetPeriod, GetJournal, ListJournals, GetSubledger, ListSubledger, GetTrialBalance, ListBankTransactions, GetStatutoryTaxReport(empty/generic v1), ImportChartCsv, ImportOpeningTrialBalance(is_opening journal), GetFinancialStatements(P&L/BS), ListAging(AP/AR)

| RPC | Data touch |
|-----|------------|
| `CreateVendorInvoice` | `finance` aggregate + outbox |
| `PostVendorInvoice` | `finance` aggregate + outbox |
| `CreatePayment` | `finance` aggregate + outbox |
| `ReleasePayment` | `finance` aggregate + outbox |
| `CreateChart` | chart CSV→Chart |
| `CreateJournal` | `finance` aggregate + outbox (with fx_rate_micros) |
| `RevalueSubledgers` | `finance_subledger` projection (fx_snapshot) |
| `ImportChartCsv` | chart CSV→Chart (50 accts) |
| `ImportOpeningTrialBalance` | TB CSV→balancing opening Journal (SUM debit==credit) |
| `GetTrialBalance` | TB projection |
| `GetFinancialStatements` | P&L/BS from AccountCategory |
| `ListAging` | AP/AR buckets |
| `ClosePeriod` | HumanDecisionProof, period gate |
| `ReopenPeriod` | HumanDecisionProof |

## Invariants (domain — keep all 7, not generic)

- Double-entry & period gate: balanced journals (`UnbalancedJournal`/`JournalCurrencyMismatch/FxRateRequired/InvalidFxRate`); Post* rejects `entry_date` in Closed period; TB filters by `period_id`; Close/Reopen require `HumanDecisionProof`+ separation-of-duties idempotent `CommandId/idempotency_key`.
- Multi-currency: `Chart.currency` base `FxRate micros 1_000_000 MIN 1 MAX 1e12` + `base_minor`; `RevalueOpenSubledgers(period_id,fx_snapshot)` → unrealized reval Journal.
- Banking CSV-only v1: `finance_bank_transactions→SaveBankMatchProposal(system)→SaveBankMatchDecision(decision_ref:HumanDecisionProof)→SaveBankReconciliation(expected_version)` idempotent `FINANCE_BANK_PROVIDER_ENABLED=false`.
- International-only TH deferred; Cutover parallel TB-match gate `TB(legacy)==TB(erpx)` period 0 opening locked; Deferrals: budget/fixed-asset/inventory costing/intercompany.
- Generic 3 (holding-isolation/time-bounded/append-only) — see `_shared-contract.md`.

## Data owned (delta — precise, light is canonical)

- Write: `finance_* (chart/account/period/journal/subledger/customer_invoice/revenue_schedule)+finance_bank_provider_import_jobs(disabled)+fx_rates/reval_journal_link` in holding data pair; no new Topo beyond fx/reval
- Read: worker projections `finance_banking_projection+finance_tax_projection` rebuildable for search
- Reference graph: see `_shared-contract.md`

## Editable fields

- **This module:** `OwnerUnsupported (spec-ahead)` — no owner-published schema (contrast `sales: SALES_QUOTE_FIELDS`)
- Binding: see `_shared-contract.md`

## Business flows (unique — Finance-First monthly close)

1. **Happy:** 2-company holding → ImportChartCsv 50 → 20 mixed-currency CreateJournal/PostJournal (fx micros) + hybrid invoices with/without PO (`ThreeWayMatchEvidence`) → CSV ImportBankStatement 100 → SaveBankMatchProposal→SaveBankMatchDecision(human)→SaveBankReconciliation → GetTrialBalance/GetFinancialStatements/ListAging → BeginPeriodClose→Close(decision)→assert closed Post fails→Reopen succeeds + parallel TB==TB gate.
2. **Exception:** Posting to Closed rejected `InvalidPeriodTransition`/`CloseApprovalSeparation`/`HumanDecisionRequired`.
3. **AI-assisted:** see `_shared-contract.md` (pins ≥3 refs → ghost → Apply/Conflicted).

## Feed mapping (paradigm: `docs/plan/04-social-paradigm.md`)

- **Emit:** `CreateJournal`→draft post; `PostJournal`→post update; `ClosePeriod`→`#month-close` + finance lane only; bank CSV import→System section.
- **Comments:** finance roles only; amounts are record truth, never feed-computed.

## Route & UI delta

- **Route:** `/finance` Overview(KPI/TB health), Chart, Journals, Subledgers, AP `/finance/ap`, AR `/finance/ar`, Cash, Bank Import CSV→Match→Reconcile, Periods Close/Reopen (human-proof dialog), Reports TB/BS/PL/Aging, Budgets(deferred) + `/work + /reference/item/:id` (light `finance.md:30` canonical, not generic `/finance`)
- **Spec row:** `journal.memo | Text | OwnerUnsupported (spec-ahead) | no runtime schema`

## Traceability

- **Proto:** `crates/proto/proto/erpx/finance/service.proto` + `reference/service.proto`
- **Domain:** `crates/contexts/finance/src/domain.rs`
- **Contracts:** `editable_fields.rs`, `catalog.rs`, `suggestion.rs`
- **Adapters:** `finance-sqlx`, `migrations 000*`
- **UI:** `apps/console/src/features/finance`
- **Verify:** see `_shared-contract.md` + light `finance.md:32` acceptance (2-company, 50 accts, 20 journals, 100 bank tx, TB/BS/PL, Close/Reopen)

## Revision

- Code SHA: fill at PR time (Q18=A). See `docs/plan/MAP.md`.
