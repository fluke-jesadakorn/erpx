# Customer identity — duplicate review

The duplicate-review queue compares two active customer masters in one Company. It orders the pair so the same records cannot be submitted twice in reverse order, and it requires a different, currently eligible Company sales manager to make the decision.

- `distinct` records that both masters remain valid separate customers.
- `duplicate` records which existing master is canonical. An eligible manager can then run a transactional merge that repoints safe orders, invoices, payments, revenue contracts, refunds, carts, opportunities, expenses and POS sales, while preserving the losing master as blocked lineage.
- `merge` rechecks company, currency, active status, reviewer version and immutable-owner blockers in the same transaction. Notifications, customer access bindings, Field Service agreements/releases and owner-controlled source evidence block the merge with an actionable error instead of losing scope.
- `reopen` returns a prior decision to independent review when new evidence appears.
- Work and Inbox route the pending pair to eligible reviewers; revoked or ended staff cannot decide it.
- Pair identity and all decision events are immutable. Optimistic version and idempotency checks protect concurrent decisions.

Merge coverage is intentionally conservative: balance reconciliation, address/contact identity, external registry checks, browser/business acceptance and reversal/correction workflows remain deferred. Blocked source workflows must be corrected by their owning department before retrying the merge.
