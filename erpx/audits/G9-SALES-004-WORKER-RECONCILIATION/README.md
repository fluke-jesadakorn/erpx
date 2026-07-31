# G9 Sales worker catch-up and reconciliation gate

**Audit date:** 2026-07-28

## Gap addressed

The Sales SQLx store already owns the authoritative write aggregate, shared
outbox, disposable read model, checkpoint validation, and source/read
reconciliation. The Sales worker previously applied only one source page and
discarded the returned cursor at scheduler completion. The scheduler payload
does not receive product-specific cursor updates, so a holding with more than
one page could repeatedly reprocess its first page without reaching a
reconciliation checkpoint.

## Delivered behavior

- `SalesProjectionRunner::project_page` now drains the routed Sales source
  outbox to a terminal page in one bounded worker invocation.
- Each page is applied through `SqlxSalesStore::project_event`, preserving
  authoritative outbox verification, contiguous aggregate versions, and
  idempotent read projection writes.
- A missing continuation cursor is rejected before acknowledgement.
- A 100-page invocation budget prevents an unbounded stream from keeping a
  worker claim alive indefinitely.
- Terminal incremental projection and rebuild both run
  `SqlxSalesStore::reconcile_projection` and fail closed on any mismatch.
- A separate routed `reconcile` method returns the typed Sales reconciliation
  report for an operational caller without mutating source or read state.
- All database routes are closed on success and error paths already owned by
  the Sales runner.

## Files changed

- `apps/worker/src/lib.rs` — Sales runner catch-up, reconciliation gate, and
  focused continuation/acknowledgement tests.
- `audits/G9-SALES-004-WORKER-RECONCILIATION/README.md` — this bounded
  implementation and rollback evidence.

No server, console, protobuf, manifest, migration, scheduler, main-worker,
MASTER_PLAN, or EXECUTION_LEDGER file was changed.

## Verification

Focused source formatting passed:

- `rustfmt --edition 2024 --check apps/worker/src/lib.rs` — passed.
- `bun run lint` — passed with four pre-existing warnings in
  `infra/scripts/start-n8n.js`.
- `bunx tsc --noEmit` — passed.

The focused Rust test/build commands were attempted but did not reach the
Sales tests because the workspace has an unrelated compile error in
`crates/domain/src/quality.rs:373`:

- `cargo test -p erpx-worker --lib sales_projection` — blocked before test
  execution by `Option::is_some_and(Uuid::is_nil)` signature mismatch.
- `cargo test -p erpx-worker --lib` — same blocker.
- `cargo check -p erpx-worker` — same blocker.
- `cargo test -p erpx-infrastructure --test sales_persistence --no-run` —
  same blocker.

The focused test code covers terminal/continued page handling, missing cursor
rejection, the page budget, and fail-closed mismatch acknowledgement, but the
workspace blocker prevents claiming execution. `cargo fmt --all --check` also
reports pre-existing formatting drift outside this Sales lane.

No external vendor, live production database, or live certification is
claimed by this packet. Disposable SQLx acceptance remains dependent on the
routed test database URLs being supplied.

## Rollback

Remove the Sales worker changes in `apps/worker/src/lib.rs` and this evidence
file. The authoritative Sales source aggregates, shared outbox history,
idempotency receipts, and disposable read rows are not deleted or rewritten.
If a deployed read model is damaged, the existing controlled path remains:
stop the Sales worker claim, inspect the reconciliation report, clear only the
Sales read projection, and replay the immutable source outbox before resuming
acknowledgement.
