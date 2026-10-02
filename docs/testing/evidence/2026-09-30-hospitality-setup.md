# Hotel + Restaurant Setup verification — 2026-09-30

Milestone: development rehearsal readiness. Production activation gates remain in place.

The guide is `/holdings/{holdingSlug}/hospitality/setup`, with the permission-filtered read API at `/api/v1/holdings/{holdingSlug}/hospitality/setup`. POS and PMS settings link to it. Finance review, independent approval, room inspection and cash-session floats remain separate owner actions.

## Automated checks

- POS, PMS, hospitality grid and workspace-scope suites: 89 passing tests across the final focused runs. Coverage includes omitted-mode retail compatibility, named restaurant bootstrap/replay, duplicate table and room rows, rejected-versus-interrupted grid requests, non-Latin room labels, accounting review/approval/freeze, wrong currency, scope/permission rejection, recipe stock consumption, room-charge replay, payments, refunds, journals and day-close blockers.
- Typecheck passed. Lint passed with three pre-existing warnings in unrelated files.
- Isolated Node 24.20.0 standalone release build passed. No release was installed over production or staging.
- `ERPX_ENVIRONMENT=production bun run seed:hospitality:local` rejected with `HOSPITALITY_DEVELOPMENT_ONLY` before database connections. Unit cases also reject staging, remote hosts and retired PostgreSQL ports.

## Seed and authenticated API checks

The seed created two fictional companies and 24 staff accounts. The practice company has no PMS property or POS settings. The configured company has a clean rehearsal branch and a populated scenario branch; each hotel has eight rooms, two room types, six restaurant tables, prices, recipes and independently approved opening stock.

Successive seed runs preserved a deliberate practice-company name edit and unchanged operational counts: 41 journals, eight Finance payments, two imported fictional bank transactions, three posted stock counts (including separate linen), three registers, nine POS bills, three POS payments, one POS refund, two PMS properties and one linked charge correction. Every posted seed journal balances. The restaurant bill has exactly one PMS source charge. The seed journal contains 233 completed steps. Only the seed's specific Night Audit job ID is drained.

Normal email/password authentication against development verified:

| Persona / company | Setup API | Result |
| --- | --- | --- |
| Practice administrator | 200 | Empty setup; not ready |
| Configured administrator, rehearsal branch | 200 | Ready for rehearsal |
| Configured Finance approver | 200 | Finance/restaurant permissions; no guest records |
| Configured POS cashier | 403 | Setup access denied |

The POS staff scope list now matches the existing published IAM holding/branch scopes, allowing fictional cashiers and kitchen staff to select their workspace.

Credentials, the write-ahead seed journal and a scenario guide with direct folio/bill links are private, ignored files under `.scratch/dev/hospitality/`. Credentials are never included in this report. Fictional receipts/evidence use local workflows; external payments and messaging are excluded.

## Browser and environment checks

The actual guide was inspected through `https://console-dev-erpx.jesadakorn.com/` using the existing authenticated Chrome session. English desktop and Thai mobile presentation were verified. Switching branches replaced the hotel and prerequisite data. At mobile width, page width matched the viewport and wide entry grids scrolled inside their cards. The desktop viewport was restored. Existing company configuration was not changed by the browser checks.

After waiting for the separate benchmark as the user requested, port 3100 returned to the current workspace source. Final normal-session API verification passed for all four personas. The setup page was inspected again in the authenticated browser: English desktop and Thai mobile rendered correctly, with no page overflow; the desktop viewport and English were restored. Screenshots are private under `.scratch/dev/hospitality/`.

Final local development health (`http://127.0.0.1:3100/api/health`), public development health (`https://console-dev-erpx.jesadakorn.com/api/health`) and private production health (`http://127.0.0.1:3101/api/health`) all returned 200. The detached workspace development server uses pinned Node 24.20.0 and Bun 1.2.17, with the development jobs worker running. PID/log records remain under ignored `.scratch/dev/`. Staging was not accessed. The final isolated build is `.runtime/releases/2026-09-30T06-41-35-717Z`.

Shared-workspace verification also corrected a People CSS module selector by scoping it under its existing workspace class. An unscoped attribute selector had blocked both development compilation and the isolated build. The hospitality seed verification after the disk/database recovery still found unchanged counts, balanced journals and the preserved practice-company edit.

## Follow-up: existing Acme daily-work warnings

The existing `POS Acceptance Retail` and hotel `Teest` belong to Acme Corporation, rather than the seeded Riverside company. Teest remains in setup with no independent Finance approval. Acme already has 73 active products. The POS warning had incorrectly inferred missing products from the deliberately empty kitchen/shifts/history payload; daily readiness was actually blocked by the hotel's unopened business day. POS now reports product availability from its scoped count and displays the associated hotel approval/opening or day-close blocker. Finance approval and opening are still owner workflows. The regression verifies all four daily tabs, an opened hotel, a stale business date and archived products. POS passed 46 tests, typecheck and lint (three unrelated warnings); the updated isolated release is `.runtime/releases/2026-09-30T06-56-51-598Z`.

After the follow-up benchmark finished, the current-source Riverside rehearsal POS was verified using normal fictional-account sign-in in the browser. It renders the sale screen with register `Rehearsal kitchen`, six tables and the cash-shift opening prompt, without a setup-blocked message. Screenshot: `.scratch/dev/hospitality/pos-rehearsal-ready.png`. Final local/public development and private production health all returned 200. Teest remains awaiting actual independent approval; no approval was impersonated or bypassed.

## Authorized Acme addition

The user's subsequent request authorized adding PMS and POS rehearsal data to Acme. `seed:hospitality:acme:local` targets only existing Acme/Bangkok in guarded local development manifests. Teest's existing setup was approved by a separately provisioned fictional Finance approver through the PMS command, opened, and advanced from September 24 to September 30 through six specific seed-owned Night Audit jobs. Eight new rooms plus the original room are independently inspected. The restaurant has six tables, six menu/catalog entries and eight stocked ingredients/products in a dedicated kitchen warehouse. The opening stock count was assigned to its independent Finance reviewer through the existing flow assignment command, approved and posted; its journal balances.

Rerun verification preserved counts: nine rooms, 30 PMS records, 86 company products, 67 seed command receipts and eight kitchen stock rows. Acme's existing USD Commerce/POS chart and settings remain unchanged; Teest remains THB. Daily applications are ready separately, while room charging remains unavailable until currencies align. Typecheck, lint (three unrelated warnings) and isolated release build passed; release `.runtime/releases/2026-09-30T09-58-16-898Z`. Credentials/replay state are private under `.scratch/dev/hospitality-acme/`.

Normal fictional staff sign-in verified the Acme/Bangkok POS and PMS browser pages. POS selected Acme Practice Restaurant and displayed six tables plus its menu; PMS showed Teest, September 30/THB and nine vacant inspected rooms. Neither page displayed `Not ready for daily work`. Mobile PMS had no horizontal page overflow, and the desktop viewport was restored. Screenshots are private in `.scratch/dev/hospitality-acme/`. The command's production rejection was also exercised (`HOSPITALITY_DEVELOPMENT_ONLY`).
