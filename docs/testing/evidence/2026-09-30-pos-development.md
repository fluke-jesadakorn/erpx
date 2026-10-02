# POS development acceptance — 2026-09-30

Scope: the authorized development POS expansion, THB company `acme-pos-thb` under Acme, restaurant and retail branches, linked fictional hotel, CRM, images, receipts, cash sessions and daily closing. Production was not deployed and staging was not accessed. This records development rehearsal, not production/provider/hardware signoff.

## Persisted data and reconciliation

The development-only preview/apply/verify seed uses normal business commands, approved Finance setup, approved stock counts, chronological business dates and stable command receipts. Replay resumes completed steps and preserves user edits. It creates 24 menus, 40 retail SKUs, 12 tables, 20 initial customers, recipes/modifiers, costed stock and 60 completed bills during 2026-09-24–30. Manual provider examples are fictional; no Stripe success is seeded.

Forty-four generated product photographs supply 64 enabled catalog bindings, private original files, 480 px thumbnails, checksums and metadata. Ingredient products do not appear in the storefront catalog.

After browser acceptance, `seed:pos:verify` reports:

| Invariant | Result |
| --- | --- |
| Enabled catalog entries / valid image bindings | 64 / 64 |
| Ingredient entries exposed as storefront products | 0 |
| Tables / CRM customers | 12 / 21 |
| Completed sales / frozen receipt snapshots | 62 / 62 |
| Business dates / confirmed branch-day reports | 7 / 14 |
| Unbalanced journals | 0 |
| Stock value / inventory GL | 70,311,235 / 70,311,235 THB minor units |
| Historical close blockers | 0 |

The stock reconciliation is **THB 703,112.35** on both sides. Read-only verification after reprinting confirms the completed bills retain their payment, invoice and stock source counts. Closing days produces no additional revenue journal.

Tracked sanitized proof: [counts, source invariants, receipt checksums and account-event evidence](2026-09-30-pos-development.json). Private detailed proofs: `.scratch/dev/pos-seed/verification.json` and `.scratch/dev/pos-browser-acceptance/verification.json`. These files contain fictional customer snapshots and source IDs. Credentials, API keys, webhook secrets and onboarding tokens stay outside tracked evidence.

## Authenticated browser journeys

Browsers used the existing authenticated Front user and console user. Separate fictional manager/Finance users reviewed cash sessions and confirmed days through the actual business services; this does not claim four separate browser sign-in journeys.

| Journey | Observed result |
| --- | --- |
| Restaurant `POS-20260930-32B037FA` | Table 01, CRM customer, basil chicken with egg modifier and kitchen note; queued → preparing → ready → delivered; THB 104.00 + 7.28 tax = 111.28 |
| Charge to room 101 | PMS owns folio charge/revenue; no POS payment or invoice; four ingredient movements total THB 33.40; frozen receipt shows room charge separately |
| Retail `POS-20260930-5F3B7639` | Keyboard barcode `885990000003`, oat milk SKU, newly created fictional CRM customer; cash THB 100.00 received for 63.13, change 36.87; one settled payment, one invoice, one stock movement |
| Offline draft | Network disabled; tissue draft persisted, money/dispatch/close disabled; reload and reconnect retained draft; explicit original-bill selection and validation flushed once; rehearsal bill then canceled without a payment/invoice/stock movement |
| Cash counts | Restaurant 2,000.00 expected/counted, zero difference; retail 2,063.13 expected versus 2,063.25 denomination count, difference +0.12; different manager reviewed |
| Day close | Console manager prepared each branch; separate Finance user confirmed immutable digest; no duplicate revenue journal |
| Receipt history/reprint | Reopened restaurant receipt from history and inspected Chrome Print Preview; one page with correct text/tax/modifiers/tender; canceled preview without printing to physical hardware |
| Tablet/mobile/themes | Actual Chrome emulation at 1024×768 and 390×844, light/dark; header wraps and no horizontal overflow |

September 30 branch reports are confirmed. Further sales/provider rehearsal must use a later writable business date; confirmed reports must not be reopened. PMS business date must also match for a subsequent room charge or correction.

Current-day amounts in THB:

| Total | Restaurant | Retail |
| --- | ---: | ---: |
| Gross / discount / sales before tax | 722.00 / 1.00 / 721.00 | 1,123.00 / 3.00 / 1,120.00 |
| Gross tax / refund tax / net tax | 50.47 / 0.00 / 50.47 | 78.40 / 11.06 / 67.34 |
| Refund / net sales | 0.00 / 771.47 | 169.06 / 1,029.34 |
| Cost / waste | 231.70 / 28.40 | 530.75 / 0.00 |
| Cash variance including seeded session | +1.00 | +1.12 |
| Room charge, separate from POS receipts | 111.28 | 0.00 |

Screenshot artifacts under ignored `.scratch/dev/`: `pos-restaurant-room-charge.png`, `pos-retail-cash-receipt.png`, `pos-offline-draft.png`, `pos-tablet-light.png`, `pos-mobile-light.png`, `pos-mobile-dark-count.png`, `pos-day-close-console.png`, `pos-reprint-preview.png`. Print Preview verifies browser rendering; it does not establish 80 mm hardware support.

## Concurrency and accounting checks

The real PostgreSQL POS suite covers command replay/lost responses, concurrent duplicate requests, stale versions/prices/catalog, simultaneous last-unit reservations, cross-company access, closed accounting periods, split after kitchen dispatch, cumulative partial refunds at original cost, independent approval, prepared/frozen days and PMS-owned room correction dates. Daily confirmation and receipt reprinting do not repost revenue. Refund tax and company-wide Stripe clearing balances are explicit.

Full integration completed: **223 test files passed, 2 skipped; 1,661 tests passed, 5 skipped**. After the final room-correction/business-date and legacy report fixes, focused POS integration passed **55/55 tests**. These provider simulations remain separate from genuine Stripe evidence. Logs: `.scratch/dev/pos-final-integration.log`, `.scratch/dev/pos-tests-final.log`. Final typecheck, lint and isolated release-build results are recorded in the active plan.

## Genuine Stripe boundary

The dedicated POS test key was verified against Stripe. The user accepted the Connect Platform Agreement; Connect was activated with direct seller payments, Stripe-hosted onboarding and Full Dashboard. A real Thailand test merchant was created: `acct_1ULOIw3k1m2RY52p`, platform `acct_1QiH8AKLj1zd9Nnv`. Fees belong to the account and no platform application fee is added.

A real connected-account webhook destination `we_1ULP9mKLj1zd9NnvYA8ULFtT` was configured at the development console `/api/pos/stripe/webhook`, test mode, API version `2025-02-24.acacia`, 22 subscribed events. A real API metadata update emitted `account.updated` event `evt_1ULPBC3k1m2RY52pEaPvxzX9`. Initial deliveries returned 400 because the existing Bun launcher inherited the old signing secret. Restarting only workspace development loaded the new secret; Stripe's own resend returned **HTTP 200, received:true**, and the durable worker applied the event. A second resend returned 200 again at 23:02:19 Asia/Bangkok, with one persisted event and one completed job; the original `applied_at` remained unchanged. This verifies account-event duplicate handling only. No synthetic webhook was used for this observation. Proof is `.scratch/dev/pos-stripe-sandbox/verification.json` and `webhook-delivery.png`.

The merchant account still has charges/payouts disabled and inactive card/PromptPay capabilities. A fresh hosted onboarding link has been opened for fictional `merchant@pos-thb-v1.example`. Password entry, confirmation and submission require user handoff under the computer-use policy, including for a development account. No new password was entered by the agent.

Pending genuine-provider acceptance: finish merchant onboarding, card and PromptPay checkout, failure/expiry, partial refunds, payment/refund webhook duplicates and reordered delivery, provider success with failed local posting and recovery, real fees/payout constituents and bank reconciliation. Account-update delivery does not satisfy those payment gates. Real scanners/printers and separate browser journeys for all approval roles also remain pending.

## Final development availability and build

`bun run typecheck` passed. `bun run lint` passed with zero errors and four unrelated existing unused-variable warnings. The final verifier file also passed its focused lint check. Isolated `bun run deploy:build` passed at `.runtime/releases/2026-09-30T16-02-47-465Z`; it was not installed over the production service.

Final local development, public development and private production `/api/health` checks returned **200 / 200 / 200**. The actual authenticated public console retail report shows `confirmed`, its independent preparer/Finance reviewer and the recorded tax/refund/cash totals. Development remains detached on 3100 with pinned Node 24.20.0/Bun 1.2.17; the existing detached jobs worker applied the authentic Stripe event. PID/log information is under ignored `.scratch/dev/`. PostgreSQL 5432 and the existing private production service remain available. No staging checks or changes were made.

## Continuation — 2026-10-01

The later [October 1 checkpoint](2026-10-01-pos-development.md) supersedes the earlier OAuth-disabled and missing-Legal-import observations. OAuth test and the development callback are now saved, merchant review was prepared without a new password, setup recovery and current compiler/build gates passed. Final merchant agreement and genuine payment acceptance remain pending. September 30 counts and checks above remain their dated baseline.
