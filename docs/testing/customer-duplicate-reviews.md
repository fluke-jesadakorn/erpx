# Customer identity — duplicate review

> Recorded report — findings, counts, gaps and hostname observations below belong to the original run. Current workflows: [owner catalogue](../contracts/next/catalogue.md); latest recorded engineering/provider/release boundaries: [verification inventory](../status/capability-delivery-matrix.md). This report is retained evidence, not an active plan.

The duplicate-review queue compares two active customer masters in one Company. It orders the pair so the same records cannot be submitted twice in reverse order, and it requires a different, currently eligible Company sales manager to make the decision.

- `distinct` records that both masters remain valid separate customers.
- `duplicate` records which existing master is canonical. An eligible manager can then run a transactional merge that repoints safe orders, invoices, payments, revenue contracts, refunds, carts, opportunities, expenses and POS sales, while preserving the losing master as blocked lineage.
- `merge` rechecks company, currency, active status, reviewer version and immutable-owner blockers in the same transaction. Notifications, customer access bindings, Field Service agreements/releases and owner-controlled source evidence block the merge with an actionable error instead of losing scope.
- `reopen` returns a prior decision to independent review when new evidence appears.
- Work and Inbox route the pending pair to eligible reviewers; revoked or ended staff cannot decide it.
- Pair identity and all decision events are immutable. Optimistic version and idempotency checks protect concurrent decisions.
- The detail screen compares each record's customer code, tax registration and status beside the submitted evidence so the independent reviewer can make the decision from the same page. It hides the decision card when the current role has no available action.
- The merge stores invoice, payment and refund-obligation counts, total amounts and outstanding amounts across both customer IDs before safe source references are repointed.

The [2026-09-23 browser evidence](evidence/2026-09-23-customer-duplicate-review-browser.txt) verifies a real two-role request, comparison, canonical decision, merge, retained history, blocked lineage and quotation transfer to Customer 360 in an isolated Demo. This is partial browser acceptance, not Company business-owner sign-off.

Remaining gaps include full Finance ledger/AR/AP balance reconciliation, address/contact identity resolution, external registry checks, reversal/correction workflows and owning-department correction acceptance for blocked records. Provider and production acceptance remain separate gates.
