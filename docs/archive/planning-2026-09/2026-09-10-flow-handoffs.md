# Named workflow handoffs

> Historical plan/report. Superseded for delivery scope and priorities by [the current production plan](../../production-plan.md). Runtime and implementation statements below describe their original date; see [the retention index](README.md) for current requirements.


Implemented adapters: expense, leave, staff task, employee request (service/equipment/purchase), stock count, warehouse transfer, commerce order, invoice, journal, payroll preview and generic workspace record.

The owner service remains the source of document state. `resolveFlow` projects current and next actions into document pages and My Work. The common command boundary validates the named primary against current staff, role, scope and separation-of-duty evidence. Existing domain authorization also remains in effect. Multiple role assignments never make one user two independent people.

## Assignment

An eligible single candidate is selected automatically. Ambiguous teams stop until a company administrator opens **กำหนด / เปลี่ยนคนหลัก** in the document Flow, selects an eligible existing person and records a reason. Saving the selection as a default creates a new revision for that company/branch/department. Documents snapshot the default revision when their workflow starts; later default changes do not rewrite that route. Explicit reassignment has optimistic version checks and an audit record. Native leave/task approval rerouting continues through its existing policy and synchronizes the route.

No login accounts were created by this implementation. Unit/integration suites use isolated database fixtures; browser checks reuse the existing verification accounts.

## Accounting boundaries

- Expense accounting approval ends at `gl_posting`, without a journal. A separate explicit command checks the approved accounting date/period and account mappings, creates balanced GL and the payable, and opens Treasury work. The accounting preparer cannot post their own preparation. Treasury excludes the claimant, business approvers, accounting preparer and poster.
- Stock counts first reach `approved`; posting then applies the checked stock snapshot and its journal atomically. Legacy posted counts remain posted.
- Goods fulfilment creates a draft goods-movement journal. Accounting posts that journal before the fulfilment can produce a draft invoice. An independent accountant posts the invoice; Treasury records receipts/payments and allocations. Partial settlement leaves the outstanding action open.
- Expense bank-payment drafts retain their existing mandatory slip upload and explicit payment confirmation. General invoice settlements retain their bank reference on the actual payment, journal and allocation records.
- Leave, staff tasks, employee requests and generic status transitions do not create accounting entries.
- Payroll is still the existing preview engine. Its approved stage explicitly reports that a validated statutory payroll adapter is missing; it cannot invent production payroll liabilities or bank payments.

Source-specific reversals/credit notes retain existing domain restrictions. The workflow layer does not invent reversal capabilities for source types that do not support them.

## Persistence and compatibility

Migrations `0007`–`0009` add versioned defaults, document route snapshots, handoff evidence and the separate stock-count approval state. Owner updates, route guards, audit evidence, next-recipient evidence, notifications and command receipts share the existing serializable command transaction. Existing idempotency receipts and owner version assertions protect retries/concurrent commands. Migrating does not fabricate old approval events or repost old journals/payments.

## Verification

- TypeScript and ESLint passed.
- Isolated PostgreSQL integration: 28 files, 228 tests passed, including assignment ambiguity, invalid candidates, revision freezing, stale reassignment, returned GL preparation, revoked Treasury, posting retries, partial settlement, stock approval/posting and purchase receipt → draft GL → draft payable.
- Browser E2E: six selected desktop/mobile cases passed for incomplete drafts, independent expense approval/GL/two slip-backed payments, and staff project/task handoffs. Existing service-request browser cases also passed in the staff suite.
- The full expense browser file additionally contains AI-provider/AI-extraction scenarios. Those unrelated scenarios failed during the initial broad run; the targeted workflow acceptance suite excludes them. They are not evidence of successful AI functionality.
- Public authenticated stock-count page inspected: real historical journal lines and no pending posting action. Public expense workflow E2E passed end to end using separate existing authenticated sessions, explicit GL posting and two slip-backed payments (`.scratch/flow-e2e-public.log`). Initial attempts encountered transient 502/timeouts; sequential login and allowing the PDF preview response a longer timeout completed the same assertions. Final local/public health and the intact production health endpoint returned HTTP 200. No Tunnel configuration was changed.

This adapter list and verification list are explicit. It does not constitute browser acceptance of every combination of department, role, financial submodule or legacy document. Payroll production processing and broader source-specific accounting correction capabilities remain separate domain work.
