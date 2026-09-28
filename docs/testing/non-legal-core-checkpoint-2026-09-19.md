# Non-Legal core delivery checkpoint — 2026-09-19

> Historical report and scoped evidence. Current priorities, payroll scope and launch criteria are governed by [the production plan](../production-plan.md); remaining work below is retained as history, not a competing delivery plan.

This checkpoint records the first implementation slice of the seven-stage non-Legal delivery plan. It is not a claim that all stages are complete. Legal remains owned by its separate task and was not changed by this work.

## Delivered in this checkpoint

- Rebuilt the non-Legal capability matrix from the current catalog and mapped all 178 capabilities to delivery stage, UI, API, service, schema, test evidence, status, and remaining acceptance gap.
- Added preview-bound, idempotent opening-balance imports with balanced-entry validation, open-period enforcement, audit history, and a ledger UI.
- Added independent payment reversal for eligible commerce payments, including allocation restoration, reversing journal evidence, and bank-reconciliation protection.
- Added customer and supplier return lifecycles with independent approval, stock reversal, credit/debit-note journals, invoice adjustment, and over-return protection. Paid-invoice refund handoff remains deferred.
- Added approved-order cancellation before stock movement and atomic customer credit-limit enforcement at approval.
- Added configurable payroll-core proration and overtime policy, immutable versioned calculation inputs, recurring deductions, and checksum-backed review exports. Statutory country packs, filings, bank payments, advances, final pay, and post-approval correction remain deferred.

## Verification evidence

- Targeted integration regression: 9 files and 37 tests passed.
- TypeScript typecheck passed.
- Lint passed for the implementation slice.
- Isolated release build passed and produced `.runtime/releases/2026-09-19T04-11-13-461Z`.
- Authenticated browser verification through `https://erpx.jesadakorn.com` confirmed:
  - Finance ledger renders the Opening balances widget and import action.
  - Payroll renders pay-group configuration, explicit non-statutory messaging, calculation policy, and reviewed-revision export.
  - Fulfilled sales-order detail renders Returns and adjustments alongside delivery, invoice, and goods-movement GL evidence.
- Local and public health endpoints returned HTTP 200 after migration and verification.

## Baseline limitations

The broader integration run recorded 72 passing files, 2 skipped files, and 3 failing files before the final targeted regression. The capability-widget failure belonged to this checkpoint and was fixed. Two pre-existing/concurrent failures remain outside this slice:

- `tests/directory-operations.test.ts`: expected one-to-one directory validation did not throw.
- `tests/expense-v2.test.ts`: PostgreSQL trigger references missing `OLD.attachment_id`.

These failures prevent claiming full-regression acceptance for the whole workspace. They do not change the targeted evidence above.

## Next delivery boundary

Continue Stage 1 with reconciliation completion, price/discount policy, paid-invoice refunds, procurement requisition/RFQ and variance review, reservations/in-transit/partial receiving, lot/serial/landed cost, schedules, FX precision, consolidation, and close/reopen acceptance. Then finish the remaining Stage 2 lifecycle, Work/Inbox/Calendar, mentions, moderation, and action-card gaps before starting Stage 3.

