# Hotel and restaurant pilot implementation — 2026-09-23

The first production target is one invite-only Thai hotel and its restaurant. Staff reservations, cash, and verified bank transfers are in scope. The production launch gate is **not passed**. The pilot property has not supplied real approved configuration, independent business acceptance, provider evidence, or an off-machine restore test. The Riverside Lantern data in [`fixtures/hospitality/pilot-example.json`](../../fixtures/hospitality/pilot-example.json) is fictional preparation data, not an activated resettable demo.

## Implemented in this slice

- POS `sale.charge_to_room` handles a full unpaid restaurant bill in the existing POS transaction. It requires a same-company, same-branch, same-currency open hotel, matching business date, in-house stay, open folio, current sale and hotel versions, active cashier assignment, approved PMS accounting, and an open period. Stock and POS cost consumption, the PMS folio entry, and the PMS revenue journal commit together. POS does not create a duplicate revenue journal or a POS payment. A repeated command key returns the original result. The production command is disabled by default until explicit pilot activation.
- A different Finance/POS/PMS approver can now reverse the full linked room charge from the POS bill. The command checks both record versions, reverses the PMS folio and revenue, classifies consumed cost as waste or restores selected resalable stock at original cost, and records one linked correction in a single transaction. Direct PMS reversal of a POS-sourced charge is blocked. The POS sale report excludes reversed room charges; receipt and bill detail identify the adjustment. Partial room-charge refunds and post-checkout settlement corrections still need business acceptance.
- A typed POS bank-transfer reference remains pending. Finance can settle it only by independently matching an imported bank row with the exact bank account, reference, date, currency, and amount. The match and receipt post in one transaction. An unverified reference can be cancelled and the bill recovered. Production POS tender creation allows cash and verified transfer; EDC, Stripe card, and Stripe PromptPay are gated.
- POS exposes limited checked-in stay details to the cashier and shows the linked folio entry after completion. An unconfigured POS now keeps the setup checklist visible without rendering the long disabled product catalogue.
- Public online booking pages and public booking API are disabled in production pending separate acceptance. The development and isolated demo routes remain available.
- The authenticated, searchable **Future features** page separates deferred items and prototypes from daily work, with a prerequisite for each. The static POS capability support map now reports partial pilot delivery rather than broad live status.

## Verification

The POS integration suite passed all 27 tests after the room-charge and bank-transfer changes. The new cases cover one PMS source and posting, no POS revenue/receipt posting, stock consumption, command replay, stale hotel version, closed folio, cashier revocation, pending transfer, independent Finance verification, bank amount mismatch, and bank match replay. The linked reversal case passed again after its final changes, covering direct-PMS rejection, folio/revenue correction, resalable-stock restoration, waste treatment and replay. The PMS integration suite passed all 25 tests in the earlier slice. TypeScript, targeted ESLint and the isolated `deploy:build` passed. The authenticated development browser displayed the unconfigured POS checklist without a disabled product wall and showed the searchable Future features page through `https://erpx.jesadakorn.com`. The reversal controls could not be checked in that browser because Acme POS has no configured pilot hotel and no room-charged bill.

These checks do **not** establish multi-role browser, provider, business, or production acceptance for the room-charge and transfer journey. The formal acceptance manifest remains pending.

## Remaining before production activation

1. Complete multi-role browser and business acceptance for the linked room-charge reversal, returns, folio/report reconciliation, and Night Audit blockers. Keep `ERPX_POS_ROOM_CHARGE_PILOT_ENABLED=false` until those pass.
2. Replace the static module support language with a company-scoped, versioned and audited rollout catalogue. Enforce it across pages, commands, scheduled jobs and AI proposals. All non-pilot production modules must default to Coming Soon while authorized historical records remain readable.
3. Provision a disposable hotel-and-restaurant demo from the fictional fixture through real owner commands, with independent accounts, reset isolation and successful/failed browser journeys.
4. Add scoped PMS/POS/inventory/Finance evidence adapters and reviewable AI suggestions. Provider opt-in, data minimization, budget, invocation evidence and AI-outage fallback need acceptance.
5. Capture an external tax-document reference against each relevant folio/POS sale, secure the accountant's VAT/service-charge/posting approval, and reconcile cash, bank, stock, folio and GL daily.
6. Verify encrypted off-machine backups, separate-environment restore to the one-hour RPO/four-hour RTO target, job recovery, role revocation, and operational monitoring.
7. Verify `dev-erpx.jesadakorn.com` against port 3100 before considering the production hostname. Keep `erpx.jesadakorn.com` on development until all launch gates and a rollback rehearsal pass. ERPX Finance remains parallel until two consecutive independently approved reconciled closes.

No real-property setup or hostname switch was performed in this slice.
