# Hotel room-charge recovery — isolated browser evidence

On 2026-09-23, run `8fedb1c94cfe` used the disposable Riverside Lantern holding on port 3102 and its isolated `erpx_browser_8fedb1c94cfe_h002_db` database. The cashier created a Bottled water bill in the actual POS browser UI. The room selector offered the checked-in Room 202 and did not offer inspected Room 101 or maintenance-blocked Room 104.

With the cashier's active work context, the browser test sent three separate room-charge commands to the real POS API: a nonexistent stay, a stale sale version, and a stale hotel property version. They returned `PMS_CHARGE_STAY`, `POS_VERSION_CONFLICT`, and `PMS_VERSION_CONFLICT` respectively. After each rejection the bill was still open at its original version, no linked PMS source existed, and aggregate warehouse stock quantity was unchanged. After reload, the cashier could still use **Charge to room**.

Verification: TypeScript and targeted ESLint passed. The targeted browser case passed, followed by all six desktop scenarios in `tests/e2e/demo-hospitality-setup.spec.ts` (6 passed, 20.6 seconds). The run also covered role isolation, successful kitchen-to-folio charge, pending transfer recovery, independent external tax-document review, and an early Night Audit close blocker.

This is isolated browser evidence with fictional data. It does not establish a real property's business acceptance, provider acceptance, production readiness, or the full refund/checkout reconciliation journey.
