# PMS — internal hotel operations

Implemented for the THB pilot on 2026-09-17. SiteMinder is a separate second phase and is not required to open a Property. No PMS command queues outbound channel-manager traffic.

## Start a Property

Open `/holdings/{holdingSlug}/pms` in a Company or Branch workspace. Create the hotel against an active Branch; the Branch identifies its owning ledger Company. One Property per Branch is enforced. Add room types, rooms, rate plans and the cancellation policy. Country, timezone and check-in/out times are configurable; the pilot permits THB with two minor digits matching the Finance chart.

A finance preparer maps the fourteen control accounts, tax and service-charge policy. A different person approves that revision. The readiness gate prevents opening without rooms, rates and approved accounting. Accounting configuration is frozen after the first financial entry; this pilot does not migrate existing liabilities or booked prices between account/tax configurations. Set the correct configuration before taking real receipts.

Reservation, Front Desk, Cashier, Housekeeping, Supervisor, Maintenance, Night Auditor and PMS Manager roles are available through the existing IAM assignment workflow. Finance preparer/approver remain separate from operating roles. Company/Branch scope, the active role, IAM revisions and work-context authority apply to every mutation; department workspaces and beta are excluded. Housekeeping reads hide guest identity and financial entries.

## Operating surfaces

The twelve PMS pages cover dashboard, reservations, Front Office, rooms, housekeeping, folios, cashier shifts, minibar/linen, guests, reports, Night Audit and settings. Navigation, Work and Inbox use the same scoped work source for assigned room tasks, independent refund approvals and blocked Night Audits.

- Reservations accept direct, telephone, walk-in and staff-entered OTA sources/reference numbers. The UI supports several room-type/date groups per reservation, each with a quantity. Every room produces a separate Stay and Folio; room assignment may follow later. Staff can identify guests per room, route future nights to another folio, change dates, handle individual no-shows, or cancel the entire unarrived reservation.
- Rate plans hold inclusive/exclusive tax snapshots, included guests, additional adult prices, minimum/maximum nights, package description, company/agent party, stop-sell and arrival/departure restrictions. Daily overrides never silently reprice an existing Stay.
- Group allotments reserve capacity until their release date. Pickup consumes the held quota once. Property locks serialize reservations, capacity changes, room moves and maintenance; overbooking is not supported.
- Check-in requires an independently inspected room without occupancy or maintenance conflicts. Move-out and checkout mark the old room dirty. Cleaning completion means clean; independent inspection makes it ready. DND is a separate request. New cleaning cycles supersede old pending tasks so an old inspection cannot open a newly dirtied room. Maintenance remains closed until inspected release, even if its estimated completion date passes.
- Cashier shifts record opening float, cash receipts, transfer/EDC references, refunds, cash movements, counted cash, variance and handover. Variances need independent review before Night Audit.
- Folios support extra services, whole-line transfers of unpaid charges, merging within a reservation, payer-specific approved credit and manual OTA gross/net/commission/fee reconciliation. Use adjustments/refunds for charges already settled or invoiced. Issued documents retain their original snapshot; adding new charges requires a new open folio.
- Minibar and consumables use existing products, branch warehouses, stock balances and the Inventory ledger. Linen issuance from inventory records consumption once; issue/return/laundry/clean/loss track the housekeeping custody cycle without repeatedly reducing warehouse stock. Lost property records storage and return/disposal evidence.

Visible pages refresh every eight seconds while online; network failures keep the last snapshot and display a notice. This is polling, not an offline synchronization system. A changed Property version requires the user to review before retrying a stale form.

## Accounting ownership

Amounts are integer minor units. Percent calculations and stock valuation use integer arithmetic. Event timestamps are UTC milliseconds; stay dates and the explicit Property Business Date are calendar dates in the hotel's operating timezone. Midnight does not close the hotel day.

| Event | Journal ownership |
| --- | --- |
| Confirm reservation | No revenue or cash journal |
| Receive stay advance | Dr cash/bank/EDC, Cr stay advance; optional advance-tax clearing under the approved policy |
| Receive damage deposit | Dr cash/bank/EDC, Cr damage deposit |
| Room night/service | Dr guest receivable, Cr net revenue/service charge/output tax |
| Apply advance | Dr advance, Cr receivable; reverse the corresponding advance-tax clearing |
| Apply damage deposit | Dr damage deposit, Cr receivable, preserving receipt attribution |
| Refund | Reverse the selected liability and advance-tax clearing, with independently approved request and original receipt references |
| Charge correction | Reversing journal linked to the original entry; excess paid credit becomes a refundable advance |
| Minibar/material consumption | Dr consumption, Cr inventory at the existing stock value |
| OTA/company settlement | Dr bank net and commission/fees expense, Cr receivable gross |
| Checkout/invoice | Post missing nights, allocate receipts, issue a snapshot; no second revenue posting |

The configured advance-tax policy is an explicit finance decision, not an automatic assertion of country-specific tax compliance. Automatic government filings and online payment collection are outside this delivery. Guest-facing pay-at-hotel booking is described below.

PMS writes use existing Finance journals, invoices, payments and allocations. `pms_postings` links each recognition/correction source to a journal. `pms_documents` retains the issued guest, folio, entries, amount and accounting revision. Receipt allocation excludes refunded money, includes applied damage deposits and traces replacement receipts. PMS control balances can be compared to GL in the report page.

Database guards prevent generic Finance updates, deletion or allocation from modifying PMS-owned journals/invoices/payments outside the PMS transaction. Generic journal reversal is hidden for PMS sources; attempts through older APIs receive `PMS_OWNER_WORKFLOW_REQUIRED`.

## Command and persistence contract

`GET /api/v1/holdings/{holdingSlug}/pms?property={propertyId}` returns scoped properties, the selected snapshot, role-filtered selectors, readiness, document links and PMS/GL control totals. Responses are private/no-store.

`POST` to the same path requires the existing work-context header/session and `Idempotency-Key`. The JSON shape is:

```json
{
  "operation": "stay.check_in",
  "propertyId": "property-id",
  "expectedVersion": 12,
  "input": { "id": "stay-id" }
}
```

`property.create` omits property/version and takes `name`, `branchId`, `businessDate`. Monetary inputs are integer minor units. Read-after-write returns `{id, propertyId, version}`. Replaying the same key/body/actor returns the same receipt; changed input with the same key conflicts. Financial commands revalidate the open accounting period and approved chart.

Command groups are defined by `executePms` in `src/lib/pms-engine.ts`; validation lives beside each operation. Groups: `property.*`, `accounting.*`, `room_type.save`, `room.*`, `rate.*`, `guest.save`, `reservation.*`, `block.*`, `stay.*`, `folio.*`, `charge.*`, `payment.*`, `refund.*`, `credit.*`, `ota.settle`, `shift.*`, `cash.move`, `task.*`, `maintenance.*`, `item.save`, `minibar.charge`, `supply.consume`, `linen.*`, `lost.*`, `night.*`.

Property commands execute in a serializable PostgreSQL transaction holding the Property row lock. Typed domain records are stored in `pms_records`, with changed-record versions; journals, stock moves, source uniqueness, audit, outbox and command receipts commit together. This favors a small multi-property pilot and simple atomic invariants; the workspace currently loads a whole Property snapshot. Benchmark and introduce paged read models before very large property histories.

Migration `holding-write/0031_pms.sql` follows `0030_pos.sql`, adds the PMS schema, Inventory view contribution, finance guards and immutable IAM revisions. Never reset an existing database or edit a migration already applied. Later POS migrations are preserved. `0034_pms_finance_ownership.sql` extends the guard to journal lines, allocation deletion and both sides of reassigned links.

## Night Audit and recovery

The Night Auditor explicitly starts closure. Readiness, arrivals/no-shows, due departures, open shifts and unresolved cash variance are checked; the ledger period and accounts must be usable. The Property becomes `closing`; guest/financial/room mutations are frozen.

`pms_jobs` is durable. The local job runner pulses every three seconds, processes at most 25 stays per step, and relies on unique `night:{stayId}:{date}` sources. Successful chunks remain committed if the process stops. The Business Date advances only after all nights and the next day's housekeeping tasks complete. Closed-day occupancy capacity is frozen with the run so adding rooms later does not rewrite historical Occupancy/RevPAR. Room-charge corrections affect the open posting date. Repeated or competing workers cannot post the same source twice.

Originating user authority is retained. Revoked/expired authority or a closed ledger period blocks the job instead of inheriting system access. Correct the external cause and let a current Night Auditor resume. Inspect the blocked run under Night Audit or Work. After closure, corrections post on the currently open Business Date. Checkout before Night Audit explicitly posts missing eligible nights; early departure omits future nights and late checkout permits an explicit extra charge.

## POS contract and phase two

`src/lib/pms-room-charge.ts` exports the strict POS charge-to-room envelope and `prepareRoomCharge`: Company/Branch/Property/currency and in-house Stay validation, immutable source line identity, duplicate/conflicting payload checks and an explicit PMS revenue owner. `sale.charge_to_room` now consumes the full unpaid restaurant bill and stock in the POS command transaction, then creates one PMS folio entry and PMS-owned revenue journal in that same transaction. POS revenue and receipt posting are suppressed. The production command remains disabled by default with `ERPX_POS_ROOM_CHARGE_PILOT_ENABLED=false` until linked corrections, reporting, multi-role browser and business acceptance pass. PMS staff can also enter services and minibar directly.

SiteMinder remains deferred: connection credentials and property/room-type/rate-plan mapping, inbound create/change/cancel, outbound rates/availability/restrictions, durable delivery/retry/reconciliation and provider certification. Before enabling inbound sync, match staff-entered OTA references instead of creating another booking. The originally supplied SiteMinder getting-started URL returned Page Not Found during verification; confirm the current process in the [provider developer documentation](https://developer.siteminder.com/) when phase two starts.

## Verification and rollout

Automated PostgreSQL cases in `tests/pms.test.ts` cover the full receipt/stay/audit/checkout path; last-room races; duplicate OTA references; stale versions; independent readiness; group pickup; advances versus damage deposits; refunds/replacement receipt allocation; split/merge/correction; manual OTA settlement; room move/extension; date pricing; Inventory atomicity; closed periods; revoked authority; worker recovery; Finance ownership and the POS contract.

Run with the pinned Node 24.20.0 and Bun 1.2.17:

```sh
bun --env-file=.env.local run db:migrate:local
bun run typecheck
bun run lint
bun --env-file=.env.local run test:integration
bun run deploy:build
```

Use only isolated test databases for financial test fixtures. For pilot rollout, choose an actual hotel name/Branch, rooms, approved rates and account mappings, assign independent staff, and pass the opening gate per Property. The implementation does not automatically approve financial configuration or assume a hotel's tax policy. Keep development on 3100 and installed production on 3101. Verify `/api/health` locally and at the public hostname and inspect the authenticated PMS desktop/mobile page before handing over.

### Recorded verification — 2026-09-17

- Full pinned-toolchain integration run: **472 passed, 2 skipped** (external provider suites require separately enabled environments).
- Final PMS suite after recovery/report/ownership hardening: **25 passed**. This includes return to an inspected room after checkout and a 26-room close resumed after its first 25-room chunk with competing workers.
- PMS + POS regression after migration 0034: **43 passed**; Finance/expense/business-command/IAM regression: **34 passed**. These counts overlap the full suite; they are not additive.
- `typecheck`, `lint`, and the isolated standalone release build passed. Final release: `.runtime/releases/2026-09-16T19-18-36-230Z`.
- PMS Playwright access tests: **6 passed** across desktop and mobile. They verify signed-out page and command protection. Authenticated public-browser verification separately covered creating a setup Property, a room type and room, the availability calendar, multiple reservation room sets, desktop layout, 390px housekeeping forms and rejection before opening readiness. The temporary setup-only Property and its two metadata records were removed; no test financial transactions were written to the live workspace.
- Financial journeys use isolated PostgreSQL fixtures, not a hotel's live money. Actual hotel pilot acceptance still requires the operator's room/rate data, independent finance approval and staff assignments.
- Final local `3100`, public `https://erpx.jesadakorn.com/api/health`, and unchanged installed production `3101` returned **HTTP 200**. Authenticated `/holdings/acme/pms` loaded successfully. Detached pinned-Node dev and the rebuilt job runner remain running; no PMS iteration errors were observed after restart.


## Direct room booking — 2026-09-17

The staff Reservations page now uses availability → room/rate selection → guest details → quote review → confirmation. Staff can create a guest inline or use an existing profile; manual OTA references and group pickup remain supported. The reservation details panel shows individual stays, policy, folio links and attributable history. Earlier records without a policy or history retain their original terms.

The guest portal is `/book/{holdingSlug}`, with individual hotel pages at `/book/{holdingSlug}/{propertyId}` and private management at `/book/{holdingSlug}/manage`. A Holding’s published hotels share a search; every reservation belongs to one Property. Website reservations confirm immediately and are payable at the hotel. Quotes do not hold inventory and expire after 15 minutes. The server rechecks inventory, rates and policy under the same Property row lock used by staff. A changed quote requires review. Confirmation creates the guest, reservation, stays, folios, idempotency receipt and email job atomically without cash or revenue postings.

### Publish and operate

1. Configure and open each actual hotel through the existing readiness/independent accounting approval process.
2. In PMS Settings → Guest booking website, enter public contact details, address, descriptions and a free-cancellation deadline; select the public rates and describe their room types. Publishing is explicit and requires configured `RESEND_API_KEY`/`RESEND_FROM`. Existing hotels are unpublished by default.
3. Configure child age bands under each rate. Each child consumes one occupancy place and adds that band’s nightly charge. Unconfigured ages cannot be quoted. Free child occupancy must be explicitly priced at zero; bands cannot overlap. Existing adult-only rates retain their pricing.
4. Review queued/failed delivery in PMS Settings. Failed jobs have an attributable Retry email command. Confirmed bookings survive delivery failures. The existing detached job runner sends emails with stable provider idempotency keys and exponential retries; after eight failures staff intervention is required.

Guest management links expire after seven days and can be requested again using the booking’s email and confirmation number. Recovery responses do not reveal whether a match exists. Tokens are hashed; email payloads are encrypted at rest using a key derived from the server secret. Keep the server secret stable or rotate it with a deliberate link/email recovery procedure. Management links use URL fragments, are removed from the address bar after loading, and are kept only in the browser session. Confirmation numbers alone never grant access.

Guest cancellation cancels the whole reservation, without fees, strictly before the snapshotted deadline and while all stays are reserved. The deadline is never later than scheduled check-in. Financial entries/refunds, partial arrival/cancellation and expired deadlines require staff. Cancellation and inventory release commit together; repeated cancellation is safe. Night Audit closure temporarily prevents booking/cancellation. Date changes, room changes, partial cancellation and no-shows remain staff operations.

### API and isolation

- Staff: `POST /api/v1/holdings/{holdingSlug}/pms/availability|quote`, `{propertyId,input}` with the existing work context and reservation permission. Existing PMS commands remain supported; `reservation.create` additionally accepts inline `guest`, `childAges` per room selection and an optional serialized `quote` for price/policy revalidation.
- Public: `/api/public/booking/{holdingSlug}/{action}`. `GET catalog`; `POST availability`, `quote`, `reservations`, `manage`, `cancel`, `recover`. Creation takes a signed quote token, guest contact data and an `Idempotency-Key` UUID. Manage/cancel use a bearer token; mutations are origin-checked and rate-limited.
- Public endpoints never return the staff snapshot, guest directory, financial entries or arbitrary commands. Catalogs contain only explicit public content; quote results use only selected public rates. Public operations use bounded capabilities rather than impersonating a staff member.
- Availability queries load room metadata, active blocks and overlapping live stays, without historical folios/entries. Confirmation and management currently load the selected Property snapshot; retain the existing small-property scaling caveat for very large histories.
- Migration `0036_pms_booking.sql` adds receipts, hashed tokens, encrypted email jobs, request limits and inventory indexes; `0037_pms_booking_receipt_snapshot.sql` freezes original confirmation responses for idempotent replay without exposing later guest edits. Existing records are not repriced, published or rewritten. Money and policy snapshots remain on confirmed stays/reservations.

The workflow benchmark is Oracle’s [Look to Book](https://docs.oracle.com/en/industries/hospitality/opera-cloud/25.4/ocsuh/t_booking_reservations_booking_a_reservation.htm), [deposit/cancellation policies](https://docs.oracle.com/en/industries/hospitality/opera-cloud/25.4/ocsuh/t_managing_reservation_deposit_and_cancellation.htm) and [booking-channel architecture](https://docs.oracle.com/en/industries/hospitality/integration-platform/dsbig/c_business_context.htm). This does not integrate with Oracle. OTA synchronization, online payments, loyalty, overbooking and cross-hotel baskets remain deferred.

### Implementation verification — 2026-09-17

Focused PMS, booking and migration tests: 42 passed. Desktop/mobile portal tests: 4 passed, including an actual family reservation in an isolated test database. Typecheck, lint and the isolated release build passed (release `2026-09-17T02-55-41-559Z`). The broader regression run recorded 504 passing and two skipped tests; its single outdated migration table-count assertion was corrected and passed in the focused rerun.

Browser checks covered the guest confirmation, secure management and cancellation, and authenticated staff reservation details/settings. Email delivery/retry was tested with a fake provider, not real customer emails. Temporary preview processes were stopped. Local and public health endpoints and `/book/acme` returned HTTP 200; the authenticated public PMS page loaded. Acme still has no configured hotel, so its guest portal correctly shows that online reservations are not open. Hotel configuration, deliberate publishing and a real email delivery check remain launch steps. Development remains on port 3100; production configuration was not changed.
