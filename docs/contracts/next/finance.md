# Finance workspace contract

Finance is organized around work that must move, rather than a collection of setup forms. The Company ledger remains the accounting authority. Group reporting reads versioned Company balances and records separate consolidation snapshots; it never rewrites Company journals.

## Current workspace

- `/finance` is the role-aware workbench. Its queues are calculated from live invoices, journals, payments, bank transactions, and periods, and are filtered by the current assignment's permissions.
- `/finance/invoices?kind=receivable` and `?kind=payable` are the collection and payment obligation lists. Existing invoice and payment detail routes remain stable.
- A posted non-expense invoice can start a controlled receipt or payment. Company Finance prepares it, an independent Finance approver approves it, and a third Treasury user releases and confirms an exact imported bank row. Confirmation posts the balanced bank/control-account journal, allocates one or more compatible invoices, settles the payment, and reconciles the bank row atomically. Refunds continue through their return/refund obligation owner.
- A foreign-currency invoice records its transaction amount, recognition-date functional-currency rate and functional carrying amount. Bank confirmation records the settlement-date rate, preserves both sides of each allocation, and posts a separate realized gain/loss journal against configured functional-currency accounts. Foreign bank statements retain their transaction closing balance, closing rate and functional closing balance. At period end, Company Finance prepares closing rates and gain/loss mappings; an independent Finance approver revalidates the source hash and posts the unrealized revaluation. Open foreign items without a posted revaluation for that period block close.
- `/finance/accounting` owns manual journals and the journal list. `/finance/ledger` redirects here for compatibility.
- `/finance/close` shows period blockers plus account-level AR, AP, and bank-to-GL differences in functional currency. It also owns period-end foreign-currency revaluation preparation, independent approval, rejection and retained rate evidence. Close preparation stores an immutable versioned reconciliation snapshot; independent approval recomputes the same source evidence and rejects any stale or changed balance. Configured bank control accounts require a statement dated at period end; foreign statements use their retained closing rate.
- `/finance/reports` presents Company and chart context with the current posted trial balance and financial statement category totals.
- `/finance/group` reads intercompany transactions and immutable consolidation-run snapshots for the Holding.
- `/finance/setup` holds readiness checks and opening-balance import, separate from recurring accounting work.
- `/finance/engines` also contains recurring accrual schedules. Finance prepares a draft with two active same-currency accounts, an amount, date range and monthly/quarterly/annual frequency; an independent approver activates it. Each due period is prepared as a draft journal and must be posted by a different Finance approver. Posting advances the schedule cursor, marks the final period complete, and retains an immutable schedule/run event history.

All views use the selected Company from the active work context. Group Finance is visible only in a Holding workspace and still requires `finance.read`. Mutations continue to require the owner permission, idempotency key, current work authority, and optimistic version where applicable.

## Accounting and AI boundary

The existing expense AI can extract receipt evidence and prepare a proposal. A Finance Proposal is advisory: pinned source versions, evidence, provider revision, result, and human disposition are retained. AI never approves, posts, releases money, reconciles a bank transaction, closes a period, or publishes a consolidation run. Those actions stay on deterministic services with independent human authority.

The workspace assistant may explain only authorized data supplied to it. Report totals are calculated by finance queries rather than by a language model. If AI is unavailable, document preparation and every control action remain usable.

## Production boundary

The current implementation provides the operating workspace over the existing Company accounting, payment, reconciliation, and close services. Advanced baseline tables for tax, assets, budgets, leases, loans, revenue schedules, intercompany transactions, and consolidation runs remain disabled for production use until their lifecycle controls, calculations, filing/export validation, and acceptance fixtures are completed. Presence of a table or setup check is not production certification.

Opening a real ledger requires accountable review of the Company chart, periods, tax policy, bank accounts, approval assignments, source-module mappings, and opening trial balance. A pilot must complete two consecutive closes with subledger-to-GL and bank reconciliations before go-live.

## Calculation engines

`/finance/engines` prepares deterministic, source-hashed calculation snapshots for exact allocation, indirect cash flow, Thai VAT/withholding, NCI/equity-method/goodwill, moving-average manufacturing cost, and many-to-many reconciliation controls. Money enters and leaves an engine as decimal strings with an explicit currency scale. Runs require an independent review and approval; they do not silently post a Journal, file a return, reconcile a bank movement, or publish group statements.

The schema retains effective-dated calculation policies, statutory packages, group entities and ownership, acquisitions, bank match groups, close checks, manufacturing cost movements and overhead pools. Each downstream posting/export/publish adapter must revalidate the approved run's source hash and business-specific controls before changing its authority record.

Recurring accrual commands are `POST /finance/accrual-schedules`, `/finance/accrual-schedules/{id}/approve`, `/finance/accrual-schedules/{id}/cancel`, `/finance/accrual-schedules/{id}/prepare`, and `/finance/accrual-runs/{id}/post`. They require current Finance authority and an idempotency key; an open accounting period must cover the due period before preparation, and a posted schedule run is a normal balanced journal linked to its source schedule.
