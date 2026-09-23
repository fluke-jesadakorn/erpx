# POS implementation and operations

POS is served at `/holdings/{holdingSlug}/pos`. Commands use `/api/v1/holdings/{holdingSlug}/pos/{resource}/{action}`, an active workspace assignment, same-origin requests and `Idempotency-Key`. Existing records require their current `expectedVersion`; create commands use a unique key instead. Company and branch come from trusted work context, never a body-supplied company. Department scope is rejected.

## Application flows

Retail: configure company accounts → create a register using a Commerce warehouse → open cash session → create bill → scan/select products → cash, transfer, external EDC or split tender → complete → print 80 mm receipt. A typed bank-transfer reference now remains pending until a different Finance user matches an imported bank row with the exact account, reference, date, currency and amount; only then does it create a settled receipt. The first production pilot enables cash and verified transfer tender creation only. Held retail bills do not reserve stock. Checkout reserves, completion consumes stock at shared moving-average cost. Money and quantities use safe integer minor units and thousandths of the base unit.

Restaurants: create tables and menu recipes; ingredient quantities support an explicit unit label and conversion to the Commerce base unit. Menu modifiers carry price and ingredients. Each bill item snapshots the recipe and modifier definition. Kitchen send reserves ingredients; start consumes into preparation cost; delivery transfers preparation cost into COGS. Cancellation before start releases reservations; after start records waste. Completed items require an approved refund before cancellation. A dispatched kitchen fulfillment persists across split/merge, while draft items can be divided before dispatch.

An unpaid, full restaurant bill can be charged to an eligible in-house hotel stay. POS consumes stock and completes the sale while PMS creates the folio charge and owns the revenue journal in the same PostgreSQL transaction. Retries return the same source result. A POS refund is blocked for this tender because there is no POS payment; linked PMS/POS correction and reversal are still required before production activation. The production room-charge command is disabled by default.

Refunds require an independent approver. Return quantities, cumulative discount/tax rounding and payment refund capacity are checked again under transaction locks at approval. Only delivered resalable stock can return to inventory, at original fulfillment cost. Prepared meals never restore ingredients. Manual refund approval includes proof that money was returned; cash refunds identify an open payout cash session. Stripe refunds stay pending until verified provider success. Cash-session close compares receipts, completed cash refunds and approved drawer movements; independent review posts only the difference.

Finance receives POS-source invoices, receipts, allocations and posted journals. Cashiers need no manual-ledger permission. Existing Finance source restrictions prevent manual reversal of POS postings. Sales and receipts are separate from ingredient/WIP/delivery cost movements. A closed period rolls back local effects. After a provider receipt, Finance can supply an open accounting date and reason in the exception queue while retaining original event evidence.

The company stop switch rejects new bills/payments while existing refunds and reconciliation continue. Company-owned Connect settings and account/mode pinned on each payment keep older refunds on the original Stripe account.

## Stripe deployment

Web and jobs processes need the dedicated `POS_STRIPE_TEST_*` / `POS_STRIPE_LIVE_*` variables shown in `.env.example`. Existing subscription `STRIPE_*` variables are intentionally not reused. `POS_STRIPE_LIVE_ENABLED` defaults false; enable only after sandbox acceptance. No platform application fee is added.

Configure a Connect webhook at `https://erpx.jesadakorn.com/api/pos/stripe/webhook` for connected-account events in each mode. Subscribe to `account.updated`, `account.application.deauthorized`, `checkout.session.*`, `payment_intent.*`, `refund.*`, `charge.succeeded`, `charge.dispute.*`, and `payout.*`. Signatures must match the event mode. The control database resolves account/mode to a holding and company; historical bindings remain routable.

OAuth redirect: `https://erpx.jesadakorn.com/api/v1/holdings/{holdingSlug}/pos/connect/callback`. The state is company/user/mode-bound, expiring and single-use. Hosted onboarding uses Accounts v1 controller properties, Full Dashboard and company-paid fees. Required capabilities must be active before checkout. The cashier displays a local QR of the Stripe-hosted checkout link; PromptPay's payment QR is shown by Stripe. Browser return URLs never establish payment success. See [PromptPay](https://docs.stripe.com/payments/promptpay) for asynchronous refund requirements.

The jobs service polls POS independently every three seconds, leases durable jobs, renews leases and checks provider state. Unknown results retain the original provider idempotency key. A confirmed failed refund requires explicit review before incrementing its provider-attempt key. Provider receipt evidence is retained even if local accounting is blocked. Duplicate/out-of-order events retrieve current provider state rather than replaying old payload status.

Stripe account activity unrelated to POS is recorded as an unmapped balance entry. Finance prepares and independently posts its journal using the normal Finance workflow, then maps the exact Stripe-clearing net amount to that entry. The payout waits until every constituent is recognized and its net total matches. This permits an existing Stripe account without silently treating its other sales as POS sales. The same approved journal cannot map twice.

## Storage and migration

- Control: `0010_pos_stripe.sql` provides account routing.
- Holding write: `0030_pos.sql`, `0032_pos_recovery.sql`, `0033_pos_provider_evidence.sql` provide POS aggregates, reservations, journal source keys, leases and recovery evidence.
- Run the normal provision/migration workflow to grant application and jobs roles access to new tables. Applying SQL alone is insufficient for those scoped roles.
- The shared `commerce_stock` trigger rejects quantities below live reservations, including writes by Sales, transfers and physical counts. Resolve affected checkout/kitchen reservations before posting a conflicting count; do not bypass this constraint.
- Immutable recipe/account snapshots, POS audit and stock moves link bills, preparation, refunds and waste. `inventory_ledger` includes POS movements.

## Acceptance and rollout

Automated tests live in `tests/pos.test.ts`; they exercise real disposable PostgreSQL databases and simulated Stripe responses. They cover cash/split tender, replay, concurrent last-unit reservations, legacy stock-write protection, kitchen WIP/waste, splitting dispatched work, original-cost returns, independent approval, scope/period rejection, rounding, provider account changes, revoked cashiers, closed-period recovery, signature/mode validation and duplicate events. Simulated-provider tests are not live Stripe sandbox acceptance.

Before enabling live payments: complete Connect onboarding in the intended sandbox, then exercise card and PromptPay checkout, partial refund, payment failure/expiry, repeated/reordered webhooks, failed local posting after provider receipt, payout constituents, fees, disputes and bank matching. Test both a retail and restaurant pilot branch with real scanners and 80 mm printers. Monitor pending provider receipts, `pos_jobs.state='exception'`, refund failures and unmatched payouts. Production remains on its existing service; the public development hostname continues to use port 3100.

Environment-dependent acceptance (real connected-account sandbox, physical peripherals and pilot-store signoff) is pending until those accounts and devices are available. Full tax invoices/e-Tax and offline selling are outside this release.

## Verified in this workspace — 2026-09-17

- `bun run typecheck`, `bun run lint`, and the isolated `bun run deploy:build` passed. The final verified build was `.runtime/releases/2026-09-16T18-58-29-114Z` (not installed over production).
- Full integration run: 452 passed, 9 failed, 2 external-service tests skipped. The nine failures were obsolete navigation expectations, a baseline table count, and hard-coded built-in role revisions after the additive migrations. Those fixtures were corrected; the five affected/relevant suites then passed all 41 tests.
- After the final Stripe recovery changes, POS + business-command + finance-money suites passed all 24 tests, including 18 POS tests. Real Stripe sandbox acceptance remains pending: dedicated POS test key, webhook secret and OAuth client ID are absent.
- Public Chrome session: POS loads, SKU search filters correctly, kitchen station view renders, account configuration lists the selected chart, and creating `POS Acceptance Retail` succeeds through the authenticated command API. The register uses the existing Bangkok warehouse. Company posting settings were intentionally left for Finance to choose; there are no POS sales or payment-provider transactions created by this browser check.
- Both `http://127.0.0.1:3100/api/health` and `https://erpx.jesadakorn.com/api/health` returned HTTP 200. The detached development server and refreshed jobs worker remain running; PID and logs are in ignored `.scratch/dev/`.
- Existing Company demo Commerce uses USD; POS preserves that shared currency for manual methods. Stripe POS requires THB. New THB companies need a matching Commerce currency and chart base currency before enabling POS.

If a provider create response remains unknown for over 23 hours, POS will not reuse an expired provider idempotency window blindly. Finance can attach the existing `re_...` or `cs_...` identifier after investigating Stripe; the worker independently verifies the original account, metadata and amount before posting. Confirmed failed refunds may start a new attempt after explicit review.

The user-facing name is **Cash sessions**. Existing `pos?tab=shifts`, `shift.*` commands, and `pos_shifts` records remain compatible. Planned staff work belongs to People → Roster.
