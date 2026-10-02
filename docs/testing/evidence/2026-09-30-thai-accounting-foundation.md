# Thai accounting development foundation — 2026-09-30

This record supports the [sole active production plan](../../production-plan.md#บัญชีไทยตั้งแต่เอกสารถึงปิดปี--2026-09-30) and [Finance contract](../../contracts/next/finance.md#thai-accounting-development-foundation--2026-09-30). The complete Thai accounting plan is **not finished**. This is development implementation evidence; it changes no business release acceptance or production permission.

## Implemented behavior

- Independently approved NPAEs Accounting Profile identifies one THB statutory ledger, fiscal start month, effective date and reviewed Company policies. Empty setup can create a first chart/period. Referenced account imports retain historical meaning; same-currency books are not implicitly aggregated, including archived books.
- Posted Financial Events and journal/invoice financial evidence are immutable. Deferred guards permit the initial posted journal and lines in one transaction but reject subsequently appended, updated or deleted lines. Existing historical posts are explicitly migration snapshots.
- AR/AP cutoffs use dated recognition, allocation, return, refund, reversal and FX evidence. Later settlement/reversal leaves earlier balances intact. Paid returns retain signed refund-control balances for close. Legacy missing-date evidence is displayed and blocks close even with a zero numerical difference.
- Manual/opening batch posting requires independent assigned review, exact versions/line snapshots, one Company/chart/period/transaction currency, at most 100 drafts/1,000 lines, all-or-nothing posting and same-transaction Financial Events/review/audit/receipts. Module-owned drafts await their adapters.
- Closing preparation can be cancelled with current version/reason by the preparer or Finance approver. Prior reconciliation/check snapshots remain; changed evidence invalidates approval. Draft and in-review work both block close. Effective statutory VAT review and VAT-to-GL snapshots are checked again inside the transaction.
- Source-derived VAT uses actual posted THB invoices and completed Commerce returns, independent tax-fact review and original-entry links for return adjustments. Separate document/accounting/VAT/claim/filing dates and reviewed standard/zero/exempt/prohibited/proportional input treatment are retained.
- ภ.พ.30 preparation freezes reviewed register/GL evidence, requires independent review, exports recoverable hash-verified Thai CSV workpapers, retains superseded preparations, links additional preparation to a filed original and records human filing-reference/receipt facts. Export text is protected against spreadsheet formulas. No external submission occurs.
- Reports separate interval P&L from cumulative B/S and include unclosed profit in equity. Workbench debt amounts remain grouped by transaction currency rather than using a cross-currency total labelled with the first book's currency.

## PostgreSQL development migrations

Applied and subsequently checksum-verified only on loopback `5432`, development holding write databases `erpx_dev_h001_write`, `erpx_dev_h002_write`, `erpx_dev_h003_write`, using their migration roles. The restricted helper under ignored `.scratch/dev/` rejects non-development database names, non-loopback connections and other migration tracks. Final verification reported **0 pending/applied** for each of the three databases. No staging or production database was migrated.

| Migration | SHA-256 |
| --- | --- |
| `0909_accounting_foundation.sql` | `1d5b1948de50d9124e37f97834309b0defe3bf0e5efbdb6d602fe7ce19e534f1` |
| `0910_accounting_review_evidence.sql` | `30507792043e7e2ce0d5c895beb6258907698fdd94bc805444c17b5f1551eed4` |
| `0911_vat_register_and_filing.sql` | `5f0bb9c2eda87ce72dcee4d5aa7ea37e6c1150ee82bca35def682aa6cd9ee487` |
| `0912_posted_document_evidence.sql` | `a53be6151df842fdab0697c1e51bcd9bc09b0955b60822974d5cb275ca3ea584` |
| `0913_vat_register_application_grants.sql` | `447fce0085c7d7e668cd58432e2f375058d91acf2606b6851e36244b72ebd9f7` |

`0913` is an additive grant correction for the installed `_migration`/`_app` role naming. Already-applied migration bytes were preserved; no checksum rewrite was used. Concurrent PMS/POS work also used this workspace and its development databases; unrelated edits/migrations were retained.

## Automated verification

Pinned Bun **1.2.17**, Node **24.20.0**, Next.js **16.3.4**. Final targeted suite: **18 files / 129 tests passed**, 63.95 seconds, started 2026-09-30 21:17:40 Asia/Bangkok:

```sh
bun run test -- tests/finance-*.test.ts tests/commerce-returns.test.ts tests/thai-accounting-foundation.test.ts tests/payroll-accounting.test.ts tests/workplace-navigation.test.ts tests/general-company-demo.test.ts
bun run typecheck
bun run lint
bun run deploy:build
```

Meaningful cases include concurrent idempotent posting/export, rollback when a later batch draft fails, stale versions/self review/cross-Company rejection, the successful 1,000-line boundary and rejection above it, posted-evidence mutation/append prevention, historical recognition and next-period settlement/reversal, quiet bank statement balance validation, account import protection, archived-book ambiguity, legacy source evidence blocking close despite matching GL, source-derived VAT dates/deductions/GL checks, stale package replacement, old export recovery, additional preparation retaining prior results, and mixed-currency queue amounts.

Commerce fixtures cover sales and purchase return VAT in a later month, preserving the original month, and paid return/refund control reconciliation. Close fixtures complete two consecutive simple independent monthly closes, and cancel/reprepare a later close after changed bank evidence. The VAT close test retains the original reviewed VAT check and rejects approval after an unmatched tax movement changes GL. These are developer fixtures, **not accountant-approved expected results or a complete business year rehearsal**.

Typecheck passed. Lint passed with **0 errors / 4 pre-existing warnings** in service-navigation, PMS and two existing E2E files. The isolated build passed at `.runtime/releases/2026-09-30T14-16-54-207Z`; it compiled all workspace code including concurrent changes and did not deploy or switch a service. Dev's separate output remained in use. This is not a frozen baseline attribution study or an all-repository integration-suite claim.

## Authenticated public development browser checks

Used the existing Chrome session as Holding Owner for Acme Corporation, with no authentication bypass and no posting, payment, profile approval or filing mutation of the Company's financial records.

- Setup displayed first-chart/period controls and the Thai Accounting Profile evidence form.
- Accounting displayed Thai batch review candidates. Selecting two drafts showed 2 drafts / 4 lines; review required a reason. Selection alone did not post them.
- Receivables aging at 2026-09-30 showed legacy-date warnings (15 AR and 10 AP items). Changing the visible date filter to 2026-09-02 updated the cutoff and results (10 AR / 5 AP legacy items); the ordinary document list remains current-state data.
- Close displayed live draft/in-review/bank blockers, account-level GL/source differences and missing dated-source evidence; the close button was disabled. Acme has no approved statutory profile, so its new statutory VAT reconciliation table was not exercised in this browser session; fixtures cover that calculation.
- Tax displayed the Thai prerequisite linking to independently reviewed statutory setup. The full VAT preparation/review/export/receipt browser cycle remains unverified because this Company does not have that approved profile.
- Workbench displayed debt amounts with transaction-currency labels; the mixed-currency case is covered by integration fixtures rather than inserting new Acme debt.
- Reports with the visible Operations ledger / USD selection and 2026-10-01 through 2026-10-31 retained assets **109,861.45**, liabilities **98,536.50**, equity **11,324.95**, and zero interval revenue/expenses. Assets equal liabilities plus equity. Cards rendered without text overlap on the desktop view. Read-only report/filter submission produced `?chart=commerce-chart-acme-co&periodId=&startDate=2026-10-01&endDate=2026-10-31`.

Public entry: <https://console-dev-erpx.jesadakorn.com/holdings/acme/finance>. Screenshot retained under ignored `.scratch/dev/thai-accounting-report.png` on the local workspace; it is not a committed production acceptance artifact.

## Availability and remaining acceptance

Final local development, public development and private production health probes returned **HTTP 200** at `http://127.0.0.1:3100/api/health`, `https://console-dev-erpx.jesadakorn.com/api/health`, and `http://127.0.0.1:3101/api/health`. PostgreSQL listened on `127.0.0.1:5432`. The healthy existing detached workspace development process was reused on 3100, with pinned Node, Bun launcher and PID/log files under ignored `.scratch/dev/`. No staging URL, port or database was accessed, and private production received only a read-only health probe.

Remaining implementation/acceptance is retained in the active plan: central effective-dated posting/tax rules; legal entity/branch/VAT registration; multi-line and issued Thai documents/PDFs; partial payment/advance/split tax points and foreign tax bases; WHT/50 ทวิ and official RD V2.0/SWC checks; prior credits and final additional-return/tax-payment lifecycles; bank exceptions committing tax/accounting together; complete source adapters/manufacturing WIP/payroll cost checks; assets and adjustments; full NPAEs statements/notes/comparatives/restatements; income-tax/audit/DBD workpapers; a complete year-close rehearsal; and accountant-reviewed, multi-role browser acceptance. The [official research note](../thai-accounting-research-2026-09-30.md) records the legal and official-tool boundaries separately.

Follow-up: the bounded approved-entry replacement workflow and its development browser rehearsal are now implemented; see [VAT correction lifecycle evidence](2026-09-30-vat-correction-lifecycle.md). The earlier results above remain the dated foundation verification, not the latest correction-flow check.
