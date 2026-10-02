# VAT correction lifecycle — 2026-09-30

This implements one bounded development goal in the [sole production plan](../../production-plan.md#บัญชีไทยตั้งแต่เอกสารถึงปิดปี--2026-09-30) and [Finance contract](../../contracts/next/finance.md#source-derived-vat-and-filing-preparation). The stop point is reviewed replacement of source-derived THB VAT facts, retained original/exported/filed evidence, stale-package and close controls, Thai UI, integration/browser verification and environment availability. WHT, Thai document issuance, official filing and year close are outside this goal. The complete Thai accounting plan and business acceptance remain unfinished.

## Implemented behavior

- Corrections append frozen replacement Tax Entries with predecessor/group links; old approved rows keep their original evidence, status, review and amounts. The current register reads approved leaves and ordinary initial drafts. Correction drafts stay out of operational totals until joint review.
- Only the current approved entry may be replaced. An invoice correction also replaces all reviewed linked returns, carrying references to the replacement original and its deduction policy. Sales/purchase returns preserve their signed amounts and their own tax/claim dates. An original cannot move its tax point after a referenced return.
- There is one pending correction per invoice family, at most 100 replacement entries per group and 100 pending groups per Company. Unreviewed family members block preparation. A source/party/family change after preparation requires rejection and re-preparation. All decisions, members, audit/outbox and command receipts commit together; database constraints reject partial group approval and frozen-body edits.
- An independent, currently authorized Company Finance reviewer approves or rejects the whole group. Repeat/concurrent commands recover the same outcome; self review, revoked assignment and cross-Company records fail without partial effects.
- Pending corrections block package and close preparation. Changed reviewed entries invalidate unfiled package approval/export. Complete VAT evidence is included in close support, so metadata-only corrections invalidate a prepared close even when GL totals are unchanged. Cancelling and preparing close again retains the old support.
- Filed packages retain their original register, workpaper bytes/hash and receipt facts. Additional workpapers link to the filed original and show both retained prior figures and corrected current figures. Stale unfiled preparations can be retained as history. A real filing receipt can still be recorded against the exact exported version if the register subsequently changed.
- Thai pages expose current/history details, before/after group review, preparation and stale-package warnings. The correction disclosure is disabled until hydration, avoiding an observed pre-hydration native-details mismatch.

Taxable/VAT amounts and the posted Financial Event/journal are unchanged by this metadata/deduction workflow. Any accounting reclassification needs a separately authorized adjustment. The existing whole-document/single-tax-point/THB restriction remains. Additional workpapers do not calculate the official additional-form delta, prior credits, penalties or tax payment.

## Development migration

`0914_vat_correction_history.sql` adds frozen correction groups, replacement links, current-entry view, replacement validity and deferred all-member decision constraints, cache invalidation and application grants. SHA-256: `5ce4913b37cf7bc2a46faf5b8380a2d9ce133c45dfb64b99887810da6368d5a5`.

Applied only to the three loopback `5432` development holding write databases. The restricted helper subsequently reported verified checksums and **0 applied/pending** on H001, H002 and H003. Already-applied migration bytes were preserved. Existing unfiled workpapers/close preparations whose source shape changes during migration must be prepared again; retained snapshots are not rewritten. No production or staging migration was performed.

## Automated verification

Pinned Bun **1.2.17**, Node **24.20.0**, Next.js **16.3.4**. Final targeted suite: **18 files / 133 tests passed**, 52.27 seconds, started **2026-09-30 22:38:25 Asia/Bangkok**:

```sh
bun run test -- tests/finance-*.test.ts tests/commerce-returns.test.ts tests/thai-accounting-foundation.test.ts tests/payroll-accounting.test.ts tests/workplace-navigation.test.ts tests/general-company-demo.test.ts
bun run typecheck
bun run lint
bun run deploy:build
```

The VAT suite now has 10 cases. New meaningful checks cover concurrent replay, exact old-row retention, retained export bytes/hash, partial-decision rollback, independent/current-authority checks, immutable drafts, scoped history, month movement, stale rejection/re-preparation, pending package/close blocking and metadata-only close invalidation with successful independent re-preparation. Commerce's 9-case suite exercises linked sales/purchase return replacement and full rollback when one family member is omitted. A changed purchase-return revision stales the entire family before a fresh correction succeeds.

Typecheck passed. Lint passed with **0 errors / 4 pre-existing warnings** (service-navigation, PMS and two existing E2E files). The final isolated build passed at `.runtime/releases/2026-09-30T15-43-30-061Z`; no service was deployed or switched. Concurrent unrelated workspace edits were preserved. These are developer fixtures, not accountant-approved golden expectations or an all-repository integration claim.

## Authenticated public-development browser rehearsal

Used the existing Chrome session at <https://console-dev-erpx.jesadakorn.com/holdings/acme/finance/tax>. A dedicated fictional Company, `expense-3c95885d-d9da-48de-94cc-a10c1741efc5`, is labelled **ซ้อม VAT Correction · ไม่มีการยื่นจริง**. A temporary Finance assignment exposed only this fictional Company to the existing session. No new login credential/session was created and authentication was not bypassed. Other Company policies, journals and tax records were not changed.

- The existing authenticated reviewer independently approved a correction prepared by a fictional Finance actor: source VAT **70 THB** remained **70 THB**, while deductible input changed **70 → 35 THB**. The page showed approved version 2 and retained before/after evidence.
- Opening the old entry showed its original **70 THB / 100%** facts with an explicit history label and link to the current **35 THB / 50%** replacement.
- The browser prepared a second correction moving a **140 THB** input claim from September to October. Its detail showed both months and required another reviewer; no self-approval action appeared. A separate fictional service actor supplied the independent review, and the browser confirmed approved version 2.
- The original fictional filed package still showed **210 THB** input/deduction, the old entry references and simulated receipt. It displayed the current-register-change warning. The retained export GET returned **HTTP 200** in dev logs; the browser download-event bridge timed out and did not provide a saved local file, so browser file-save completion is not claimed.
- A stale unfiled additional preparation retained **210 THB**. Its approval remained disabled after entering a valid reason; the browser retained it as history, version 2.
- The September current register contained only the replacement, with VAT/GL cumulative gross amounts **210 THB / 210 THB**, difference zero. The browser prepared a fresh additional workpaper showing current input **70 THB**, deductible **35 THB**, linked prior input/deduction **210 THB**. It awaited another reviewer.
- The updated correction disclosure opened with the current 50% default after hydration. No new mismatch was observed on this check.

Read-only service verification of the same fictional browser data passed: September deductible **3,500 minor units**, October **14,000**, original filed **21,000**, both current registers reconciled, original entries unchanged, stale preparation superseded and corrected preparation retaining prior figures. Original workpaper remained **1,285 UTF-8 bytes**, SHA-256 `ef0d22199a82691611f7d3accd0a3b984249902a53191ce43c406b6081267fab`. Integration also compares retained content byte-for-byte.

Local browser/fixture evidence is under ignored `.scratch/dev/`: `vat-correction-approved.png`, `vat-correction-workpaper.png`, and `vat-correction-rehearsal/{state,verification}.json`. It is not a committed business acceptance artifact. The fictional filing receipt is expressly **not an official filing**. Actual multi-user accountant acceptance, official RD validation and the broader tax/year-close flow remain pending.

## Final availability and cleanup

The browser was restored to **Acme ร้านตัวอย่าง THB / Holding Owner** using its normal Company switcher. The temporary fictional-Company Finance assignment was revoked; a read-only database check confirmed it is inactive. Fictional evidence is retained without a real filing or payment. No other browser tab or Company records were changed.

Final health checks returned **HTTP 200** from `http://127.0.0.1:3100/api/health`, `https://console-dev-erpx.jesadakorn.com/api/health`, and `http://127.0.0.1:3101/api/health`. PostgreSQL remained on loopback `5432`. The existing development process was reused. Concurrent POS work subsequently refreshed the shared dev launcher; the last observed Next listener was PID 58128 with workspace cwd, pinned Node 24.20.0 parent PID 58127 and Bun dev script PID 58108. Its final log/PID files are `.scratch/dev/pos-final-dev.{log,pid}`. It was rechecked after the checks completed. No staging environment was accessed; production received only a read-only health probe.

Late verification briefly hit the command tool's `Too many open files` error. Commands succeeded through its PTY interface; after the existing development process refreshed, a normal browser reload restored interactive controls. We did not stop/restart another service or alter security settings. The bounded development goal is complete; the broader production-plan acceptance remains open.
