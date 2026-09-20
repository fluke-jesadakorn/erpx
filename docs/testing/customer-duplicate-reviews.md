# Customer identity — duplicate review

The duplicate-review queue compares two active customer masters in one Company. It orders the pair so the same records cannot be submitted twice in reverse order, and it requires a different, currently eligible Company sales manager to make the decision.

- `distinct` records that both masters remain valid separate customers.
- `duplicate` records which existing master is canonical, without silently moving orders, invoices, payments or access bindings.
- `reopen` returns a prior decision to independent review when new evidence appears.
- Work and Inbox route the pending pair to eligible reviewers; revoked or ended staff cannot decide it.
- Pair identity and all decision events are immutable. Optimistic version and idempotency checks protect concurrent decisions.

The actual transactional merge remains separate work. It must inventory every source reference, preserve old identifiers, re-check customer authority, reconcile balances and support reversal/correction before any master is blocked or redirected.
