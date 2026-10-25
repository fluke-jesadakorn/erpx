# PMS mock pilot rehearsal — 2026-10-09

The user explicitly requested mock inputs on 2026-10-09. This record supplies the fictional pilot assumptions and verification mapping; the [production plan](../production-plan.md) remains the sole active checklist. A fictional reviewer is an independent test identity, not an actual employee signoff. No live payment or production publication is authorized by this rehearsal.

## Inputs and independence

The executable operational fixture is `fixtures/hospitality/pilot-example.json`, seeded by `scripts/fixtures/hospitality-browser-demo.ts` in disposable PostgreSQL databases. Riverside Lantern Bangkok has eight deliberately varied rooms, separate inspected/occupied/maintenance states, restaurant stock and reviewed opening inventory. It is a workflow fixture, not a 30–200-room capacity claim. The separate capacity cohort must contain three properties of 200 rooms each and two years of synthetic history, with 20 concurrent authenticated users.

Front desk, hotel cashier, housekeeper, inspector, night auditor, POS cashier, kitchen, Finance preparer and Finance approver use separate fixture accounts and normal role selection. Finance approval must come from a different person than the preparer; room inspection must come from a different person than the cleaner. Housekeeping must not receive guest money or private identity documents. Console and Front authentication remain separate.

All sample photography is explicitly fictional, with source and room consistency recorded in `public/pms-demo/rooms/README.md`. Approved publication and revocation must be tested through the existing Files publication owner; a prototype image does not establish that this pipeline passed.

## Opening balances and two daily closes

`tests/pms.test.ts` contains a concrete opening-receivable rehearsal named “settles the original opening receivable and invoices only new hotel service across two business days”. Its fictional THB opening receivable is 200.00. A separate Finance reviewer posts the journal and approves the PMS import. Day one collects 100.00 and retains 100.00 on the original invoice; revenue remains zero before the room-night close. Day two collects 207.00, settles the remaining opening invoice and issues only the new-service invoice for 107.00 (100.00 room revenue plus 7.00 tax). The original imported amount is never invoiced again. Both shifts close and Business Date advances from 2026-09-01 to 2026-09-03. PMS–Finance differences must be zero for receivable, advance and deposit.

The adjacent “traces imported advance and deposit to the approved opening journal and reconciles partial refunds” case supplies an opening advance of 200.00 and security deposit of 50.00, including a 20.00 partial deposit refund. These are separate balances, not additional revenue. Other import cases exercise preview, independent approval, replay and preservation of original guest/stay IDs and evidence.

These assertions are executable engineering review. Acceptance remains pending until their final-candidate run is successful; narrative amounts alone are not a receipt.

## Required proof

| Review | Executable source | Required outcome |
| --- | --- | --- |
| Front desk and room readiness | `tests/e2e/thai-hotel-v1-hotel.spec.ts`, `tests/e2e/demo-hospitality-setup.spec.ts` | Normal login, booking/check-in, independent room preparation, settlement, checkout and reload |
| Housekeeping authority | `tests/pms.test.ts`, `tests/pms-read-model.test.ts` | DND, stale task cycle, independent inspection, overdue occupancy and cross-property denial |
| Finance opening and daily close | `tests/pms.test.ts` | Original opening receivable, advance/deposit funding, balanced new-service invoices and two consecutive closes |
| Guest booking and recovery | `tests/pms-booking.test.ts` and required guest browser journeys | Accepted child/rate/policy snapshots, partial cancellation/refund, signed sandbox webhooks, response loss and restart without duplicate money |
| Guest preferences and CRM history | `tests/crm-marketing-analytics.test.ts`, PMS application cases | Independent identity link, explicit sharing grant/revocation, fresh scoped reads and immutable evidence |
| Reports and pace | `tests/pms-reports.test.ts` | Revenue reconciles to Finance; historical quotes and net pickup use observed facts; unobserved history stays unknown |
| Migration and recovery | `tests/engineering-migration-upgrade.test.ts`, `tests/engineering-recovery.test.ts` | Same compiled candidate, unchanged original receipts/IDs/documents, additive migration replay and encrypted restore |
| Scale | Existing source diagnostic plus pending release HTTP cohort | 3 × 200 rooms, 730-day history, 20 users; read p95 below 1s and ordinary write p95 below 2s with raw samples |

The 2026-10-09T14-07-27-291Z whole verification attempt passed release build, migration upgrade and recovery, but failed integration (an outdated injected-clock fixture) and browser preparation (an incorrect POS approval command name). It is incomplete and must not be promoted. The corrected attempt is 2026-10-09T14-13-55-668Z; its final outcome was still pending when this record was written. Raw private evidence is retained under `.scratch/dev/engineering-completion/runs/`.

## Rollout boundary

Engineering mock acceptance does not approve real-hotel rates, real images, opening balances, external email recipients or live payment credentials. SiteMinder certification, OTA delivery and government/provider integrations follow the after-pilot phase recorded in the active plan and official market research. Their absence does not prevent this fictional engineering rehearsal.

## Compiled rehearsal checkpoint — 2026-10-10

Subsequent [complete guest lifecycle evidence](evidence/2026-10-10-pms-full-guest-journey.json) closes the focused booking/check-in/independent housekeeping and completed-date visual gaps on desktop/mobile. Separate normal mock roles complete booking through cash settlement, independently reviewed correction, checkout and actual compiled Night Audit jobs replacement. Both reloaded pages show the next Business Date; journals balance and the original paid invoice remains singular. This does not yet satisfy consecutive mock-pilot daily closes, paid online provider process recovery, aggregate acceptance or write latency.

The enhanced eight-case hotel/POS/Finance matrix also passed again on both viewport projects, with explicit completed-day reload and inspected screenshots: [16-case completed-day evidence](evidence/2026-10-10-pms-hospitality-completed-day.json). This closes the visual gap in the historical checkpoint below; its original receipt remains unchanged.

Candidate `2026-10-09T17-38-18-796Z` passed all 50 `tests/pms.test.ts` cases, including the opening receivable, advance/deposit, partial deposit refund and consecutive daily closes above. Additive migration and encrypted restore passed on the same candidate, preserving the original history digest across eight schema tracks and restoring eight databases / 2,375 tables. Compatible rollback succeeded; incompatible rollback and ciphertext tampering were rejected. The upgrade scanner is a declared interface fixture; real scanner evidence is recorded separately.

Fresh desktop/mobile cohorts each passed all eight hotel/POS/Finance browser journeys with normal independent mock roles. The Night Audit recovery journey replaced the compiled jobs process, first blocked a revoked originating assignment, then resumed through an authorized owner and completed the next Business Date. The original paid invoice remained singular and PMS posting debit/credit balanced. [Receipt](evidence/2026-10-10-pms-pilot-migration-restore-night.json) binds source/assertion fingerprints, report hashes and worker PIDs. Final screenshots retain the pending UI page; completed state is proven by the database and still needs post-completion visual reload. Complete booking/check-in/independent housekeeping, paid guest/provider process recovery, full final-candidate gates and write latency acceptance remain open.

## Current frozen source checkpoint — 2026-10-10, 03:44 UTC

Release `2026-10-10T03-44-29-508Z` binds runtime source `2322ee6ed91e5cfd610f67645e0fd25b0eaee19617ec6ae26c67e6788983b50c` and verification source `f82f0f5a82f95d9b38fb48bd0196e2d5aad65e20a36ff13833c65409a0dc1db3`. Its full PMS native selection passes 1,301 cases, including all 69 PMS operational cases, 37 direct-booking cases, additive migration and encrypted-recovery tests. The focused identity/security selection passes 44 cases separately; it is not included in the PMS module total. These native cases cover the fictional opening and consecutive-day assertions above; no real employee signoff is implied.

The unchanged compiled-release capacity workload completes 840 reads and 843 writes with no invalid samples or deadlocks. It preserves all 657,000 historical records/versions, reconciles original advance/deposit and refund funding, clears the 20 operational accounts, settles the 20 Finance invoices and advances all three Business Dates. Read p95 is 309.196 ms; core command p95 is 2,088.612 ms and therefore **fails** the strict 2,000 ms gate. Financial, operational and closing aggregates are below 2,000 ms. [Current source and raw-result receipt](evidence/2026-10-10-pms-auth-factory.json) retains the failure and cleanup; whole-plan acceptance remains open. The 178-case required PMS browser selection is running with explicit compiled-business/development-UI execution mapping, which must not be represented as an all-compiled receipt.

## Transactional cache scope candidate — 2026-10-10

Release `2026-10-10T05-09-34-266Z` binds runtime source `1d646f0338b5b79ac34e872be80b32e1c315ffc518fd61d17bfebaa220197ac0` and verification source `7e2eaf3afc25a48ed71b197312381de1c30c7c2a5c2314a36c239b151fcae4e8`. [Current cache scope receipt](evidence/2026-10-10-pms-cache-shards.json) preserves every previous failure. Its original full native command has one unchanged 180-second scale timeout; the entire 18-case file subsequently passes alone. Every one of the 1,305 required native cases therefore has passing evidence on this source, plus 47 separate security cases, but the original command is still recorded as failed. This includes the mock opening balances, two daily closes, additive populated upgrade and encrypted recovery assertions.

The full HTTP cohort reconciles 840 reads and 843 writes, original money and all 657,000 historical records/versions, with zero invalid samples or database deadlocks. Read p95 is 474.892 ms. Core aggregate p95 is 1,998.351 ms, but reservation.create independently measures 2,032.681 ms and fails the strict 2,000 ms gate. Aggregate harness success does not close this requirement. An identical fresh cohort is ready for confirmation with existing lock/serialization telemetry retained; performance acceptance remains pending.

Required browser verification is running on the same frozen source, with explicit compiled business / isolated development shared-UI mapping. Desktop and mobile provider-response-loss/jobs-restart recovery and all sixteen hotel/POS/Finance cases passed. The remaining baseline cases, 38 PMS route UAT cases and final mock-review evidence must complete before this candidate can close engineering acceptance.

## Current native mock rehearsal checkpoint — 2026-10-11 ICT

The current runtime passes the refreshed 84-case PMS file, including the added linen, DND and early-departure assertions, plus the previously verified populated upgrade and compiled encrypted recovery. The [native acceptance extension](evidence/2026-10-11-pms-operational-acceptance-extension.json) distinguishes the original 1,339-case selection from this refreshed file; no fresh combined 1,342-case command is claimed. The opening-receivable case settles THB 200.00 across two Business Dates, issues only the new THB 107.00 service invoice and reconciles all three PMS–Finance balances at zero. The separate approved advance/deposit case records THB 200.00/50.00 and a THB 20.00 partial deposit refund with independent review and zero reconciliation differences. [Native rehearsal evidence](evidence/2026-10-11-pms-mock-native-rehearsal.json) includes source/log hashes and exact assertions. Capacity now passes its strict unchanged release cohort. The fresh 178-case browser run and 38-route matrix remain open; actual staff and live-money acceptance are not claimed.

Delivery alignment checkpoint (11 Oct): the [new candidate matches current main](evidence/2026-10-11-pms-main-alignment.json). The preceding native/recovery checkpoints describe 61d. New build, native, security, full browser/route and strict capacity evidence must qualify dae9/d338 before mock engineering acceptance; original evidence is retained.

Fresh main-aligned checkpoint (11 Oct): [complete native/security evidence](evidence/2026-10-11-pms-main-aligned-native.json) passes 1,342 native cases (781 unit + 561 PostgreSQL integration) and 47 focused security cases on release `2026-10-10T21-33-53-720Z`, runtime `dae9ff8f263604e859a87b37c3a9b10a1a3ec35f25e7669ca81c53c5d34ae405`. This includes the three operational acceptance additions and fresh migration/recovery execution. Required 178 browser, 38 route and strict release-capacity receipts remain pending; all in-pilot delivery statuses stay awaiting combined acceptance.

The [same-candidate supplemental CRM/privacy receipt](evidence/2026-10-11-pms-main-aligned-crm-privacy.json) passes nine existing PostgreSQL cases for reviewed identity links, explicit hotel-service-history sharing and revocation, foreign Branch refusal and append-only evidence. These nine cases remain separate from the required 1,342-case PMS selection; they are native application proofs, not actual guest consent or staff signoff. The [real Stripe test receipt](evidence/2026-10-11-pms-main-aligned-provider-sandbox.json) separately covers seven hosted Checkout/native-ledger scenarios, including three independently approved partial card refunds. It does not establish live money, real signed-webhook/guest-return E2E or external email delivery.

The [67-capability proof map](evidence/2026-10-11-pms-main-aligned-feature-proof-map.json) binds the current assertions and evidence to each pilot feature. Normal-password compiled HTTP rooming-list checks (10) and pricing/report-download/independent-maintenance-release checks (30) are prepared and unexecuted. The sequential supervisor waits for the original 178-browser/38-route driver before those checks, owned-cohort cleanup and fresh strict capacity. All 62 pilot rows remain awaiting combined engineering acceptance; the five partner families remain explicitly after pilot.
