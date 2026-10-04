# Local development data and acceptance workflows

Run `bun run seed:local` (or its alias `bun run db:seed:local`) to apply local migrations, reconcile the international fixtures, repair known legacy identity links, and configure connected commerce. The command uses the configured development PostgreSQL databases and private files, preserves existing business records, and does not reset passwords. Configure `.env.local` from `.env.example` with absolute data/manifest paths and the development databases on the shared PostgreSQL 5432 service. Production and staging use separate databases on that service. `scripts/seed-d1.ts --remote` rejects development fixtures; these credentials must never be used for a production tenant.

### Acme Position pay and Level leave defaults

This fixture is separate from `seed:local` and is restricted to the `erpx_dev_*` databases on loopback port 5432 and Company `acme-co`. Run `bun --conditions=react-server scripts/seed-acme-staff-defaults.ts --preview` first. `--defaults` installs a USD monthly pay recommendation for every active Acme Position and tracked annual/sick policy recommendations for Levels 1–6, leaving employee records untouched. After testing one fictional employee through People, run `--apply` to add draft compensation only where an Engagement has no compensation history and to replace only legacy untracked leave assignments. `--verify` checks that the default catalog is complete. Repeating `--apply` preserves HR-edited default versions, approved compensation and custom leave assignments.

Fixture monthly pay by Level is USD 12,000 / 9,000 / 6,500 / 4,800 / 3,600 / 2,800 for Levels 1–6. Annual leave is 18 / 15 / 12 / 10 / 8 / 6 days; sick leave is 15 days at every Level. These numbers are fictional workflow data, not employment policy, legal entitlement or Thai statutory payroll. The seed reports Engagements missing Position, Level or pay group for HR review instead of guessing a placement.

The base roster contains **60 distinct fictional workers and 19 development login accounts** across Acme Corporation, Beacon Labs and Northstar Supply. The optional [Acme organization expansion](testing/acme-organization-data.md) adds **92 staff and login accounts to Acme only**, bringing Acme to 112 profiles with every numbered level and nine department types in both branches. It was applied to this development workspace on 2026-09-14. They are fictional development examples. Additional browser/API acceptance records are preserved with their audit history. Raw table counts also include archived and merged records, which the current directory filters out.

Each of the 60 base Engagements receives an explicit backoffice Position from its fixture title only when its Position is still unset. Re-running the seed preserves HR edits. The Acme expansion marks only Bangkok Operations and Warehouse roster example staff as roster-required; managers and New York remain backoffice. The Riverside browser fixture gives its front desk a separate roster-required Position. The roster seed publishes the current and next weeks when future shifts remain; it reports any current-week gap instead of adding past shifts.

The data has six branches: Bangkok, New York, London, Singapore, Rotterdam and Toronto. Operations ledgers use USD, GBP and EUR respectively. Other historical fixture ledgers also exist; select a chart before reading a financial report. Currency amounts from separate charts are never combined in a trial balance.

## Login accounts

### Acme credential update — 2026-09-15

The user standardized all 112 current Acme People emails to lowercase `firstname.lastname@erpx.dev` and updated the 102 existing linked login accounts to match. All 102 Acme login passwords are now `12345678` for local testing. The other 10 profiles have no login account. Roles and identity links were preserved. Examples: `mali.srisai@erpx.dev`, `anya.bennett@erpx.dev`, and `elena.suwan@erpx.dev`. This overrides the older short-login examples below for Acme; other companies and the separate all-access account are unchanged. Existing acceptance scripts that hard-code the old Acme credentials must use these updated values when targeting this development database. Fresh database seed defaults remain as defined in the seed scripts.

Most addresses use `@erpx.dev`. Initial passwords follow `<FirstName>-ERPX-2026!`, such as `Henry-ERPX-2026!`. The all-access dev account below uses the listed Gmail address and password. Existing passwords are preserved when seeding again.

| Login | Full name | Main responsibility |
| --- | --- | --- |
| mali | Mali Srisai | Acme owner and People management; local platform administration |
| nisa | Nisa Chen | Acme financial controller; use the holding finance approver assignment |
| ava | Ava Williams | Bangkok HR administration |
| noah | Noah Patel | New York employee self-service |
| henry | Henry Lawson | Prepare quotations and sales orders |
| sofia | Sofia Ramirez | Independently approve sales |
| leila | Leila Haddad | Prepare purchase orders |
| priya | Priya Shah | Independently approve purchasing |
| owen | Owen Brooks | Receive and deliver stock |
| daniel | Daniel Kim | Prepare journals, invoices and record customer receipts |
| eli | Eli Morgan | Beacon payroll review |
| zoe | Zoe Laurent | Beacon treasury |
| sam | Sam Rivera | Northstar holding owner |
| iris | Iris Johnson | Northstar finance preparation |
| luca | Luca Rossi | Northstar payroll preparation |
| mia | Mia Okafor | Northstar People management |
| jun | Jun Park | Northstar audit review |
| olivia | Olivia Grant | Toronto employee self-service |
| jesadakorn.kirtnu@gmail.com | Jesadakorn Kirtnu | All seeded platform and business roles; dev-only password `12345678` |

Assignments have company/branch/department scope and expiry. The account menu shows available assignments. Branch assignments cannot access company-wide payroll; a holding owner can grant a suitable assignment through the employee's detail page. Being listed in People does not grant login or business permissions.

## Run and verify

Use Bun 1.2.17. Reuse existing ERPX processes where available.

```bash
bun run seed:local
bun run dev                         # http://localhost:3100
bun run test:business:local          # HTTP journeys against 3100; creates persisted acceptance records
bun run seed:verify:local            # read-only PostgreSQL/files integrity checks
bun run test:integration             # isolated PostgreSQL tests at 5434
bun run lint
bun run deploy:build
```

Use `ERPX_TEST_BASE_URL` to point HTTP journeys at an isolated native preview with development fixture data. Never target the public beta or its production database. Ports 3000, 3001 and 8080 are reserved for other projects.

## Connected examples

- Purchasing: Leila drafts an order to Summit Furniture Manufacturing. Priya approves it. Owen receives 12 desks, then the remaining 8. Each receipt updates quantity, stock value and inventory/GRNI journal lines. An invoice for each receipt clears GRNI; an independent finance approver records supplier settlement.
- Sales: Henry prepares a Harbor Office Solutions quote. Sofia approves it. Conversion creates a linked sales order, delivery consumes five desks and posts cost of goods sold, and invoicing creates the receivable and tax lines. Receipts of USD 1,000 and USD 872.50 clear that invoice. The scenario leaves 15 desks worth USD 3,000 in stock.
- People: Mia creates and activates an employee with a legal name, email and placement. Sam issues a scoped invitation. Activation creates password credentials and links the account to that employee. Self-service leave flows to a different manager; revocation immediately prevents further business access with that session.
- Pay inputs: Sophie Vermeer's EUR 3,200 base, EUR 150 allowance and EUR 50 deduction pass independent compensation review. The October gross-pay preview is EUR 3,350 gross and EUR 3,300 net. Time entries can be returned, corrected and resubmitted; approved time is locked.
- Warehouse transfers: Owen prepares mounting-kit movement from Bangkok to a receiving warehouse. The receiving reviewer confirms it independently. Both warehouse balances retain the same total quantity/value; concurrent transfers and pending physical counts cannot overwrite the committed movement. Three companies now each have a main and an installation-staging warehouse.
- Physical stock counts: Owen counts installation kits; Nisa independently posts quantity/value differences to inventory and stock-count variance. The journey returns and recounts a stale concurrent count, delivers kits through a sales order, checks COGS, counts to zero and reconciles the final stock value to the general ledger. The browser monitor example uses Mali as preparer and Nisa as reviewer.
- Expenses: Olivia submits a rail receipt and business purpose. Mia returns it for correction, then independently approves it. Sam posts EUR 100 expense, EUR 9 recoverable tax and EUR 109 employee payable, records EUR 50 and EUR 59 reimbursements, and matches both bank statement rows. A separate unpaid example is reversed with its original journal retained. Claims are available from People, Finance and Your work.
- Accounting: Daniel creates a balanced journal; Nisa posts it. A reversing journal preserves the original history. A USD 107 service invoice separates USD 100 revenue and USD 7 tax, then links to its receipt and matched bank-statement row.

The HTTP verification scripts provision their required fixtures if absent and use stable command keys for repeatable business transitions. Identity activation tests use a fresh address and archive the employee afterward. Stock-count and transfer verification use separate SKUs per run and retains count, sale and ledger history. Expense verification uses a fresh receipt reference per run and retains paid, cancelled and reversed cases; replay checks reuse keys within that run. Failed or historical test records remain auditable.

Expense commands live under `/api/v1/holdings/{holdingSlug}/expenses`. The old People/Finance expense-create endpoints return `410 EXPENSE_API_MOVED`; new expense payment batches must use the posted payable. All three company ledgers seed travel, office-supply and professional-development and stock-count-variance expense accounts. Reimbursement currently uses the company operations currency; cross-currency claims and employee advances remain open work.

## Current release boundary

This is a working local global-core implementation, **not a completed production ERP**. Payroll is an explicitly labelled gross-pay preview; it cannot post or execute payments without an implemented statutory adapter. Seeded payroll/export files are historical test fixtures, not certified calculations or executable bank instructions. No real employer data has been imported, no emails have been sent, and no production deployment has been performed.

See [implementation status](production-plan.md) for the complete 31-module inventory, gaps and verification evidence, and [ERP workflow research](production-plan.md) for the primary-source workflow references.

Physical counts are available from Stock & movements and Inventory module actions, with APIs under `/api/v1/holdings/{holdingSlug}/stock-counts`. Counts require an inventory operator and an independent holding finance reviewer. Subsequent stock movement invalidates an unposted snapshot. Migrations 0024–0025 add counts, decisions and the combined inventory ledger. `bun run seed:verify:local` now checks that stock quantity/value equals the combined movement ledger.

Direct warehouse transfers are available from Stock & movements and Inventory module actions, with APIs under `/api/v1/holdings/{holdingSlug}/stock-transfers`. Migration 0026 extends the movement ledger. `scripts/verify-inventory-transfers.ts` exercises source/receiver branch scopes, independent receipt review, return/correction, concurrent stock changes and preservation of company valuation.
