# Hotel room-charge recovery — isolated browser evidence

On 2026-09-23, run `8fedb1c94cfe` used the disposable Riverside Lantern holding on port 3102 and its isolated `erpx_browser_8fedb1c94cfe_h002_db` database. The cashier created a Bottled water bill in the actual POS browser UI. The room selector offered the checked-in Room 202 and did not offer inspected Room 101 or maintenance-blocked Room 104.

With the cashier's active work context, the browser test sent three separate room-charge commands to the real POS API: a nonexistent stay, a stale sale version, and a stale hotel property version. They returned `PMS_CHARGE_STAY`, `POS_VERSION_CONFLICT`, and `PMS_VERSION_CONFLICT` respectively. After each rejection the bill was still open at its original version, no linked PMS source existed, and aggregate warehouse stock quantity was unchanged. After reload, the cashier could still use **Charge to room**.

The test then revoked that cashier's assignment in the disposable database and retried with the previously opened session. The API returned HTTP 409 `WORK_ROLE_UNAVAILABLE`; the bill version, linked PMS source count, and stock were unchanged. The assignment was restored in a `finally` block so the rest of the isolated suite could continue. This checks immediate command denial, not every navigation or background-job path after revocation.

Verification: TypeScript and targeted ESLint passed. The wrong-room/stale-version case first passed on run `8fedb1c94cfe` with all six desktop scenarios. Revocation was then added and a fresh run `8927c97ebc5e` passed all six desktop scenarios together (15.1 seconds). The suite also covered role isolation, successful kitchen-to-folio charge, pending transfer recovery, independent external tax-document review, and an early Night Audit close blocker. Both disposable runs were destroyed after verification.

This is isolated browser evidence with fictional data. It does not establish a real property's business acceptance, provider acceptance, production readiness, or the full refund/checkout reconciliation journey.
