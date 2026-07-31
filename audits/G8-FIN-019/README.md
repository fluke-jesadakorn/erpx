# G8-FIN-019 — Finance close review SQLx persistence

## Packet contract

- Objective: persist the already-integrated `FinanceCloseReview` aggregate in
  a holding-scoped tenant-write adapter with optimistic versions, idempotency,
  atomic snapshots/outbox events, and database immutability for closed input.
- Dependencies: `erpx_domain::finance_close`, tenant `begin_scoped`, and the
  Finance snapshot/outbox helper.
- Allowed paths: the new adapter, migration, focused integration test, and
  this audit file only.
- Forbidden paths: shared exports, migration assertions, server, worker, UI,
  existing Finance files, Folio routes, credentials, and unrelated modules.

## Delivered behavior

- `SqlxFinanceCloseStore` uses `period_id` as the aggregate identity because
  the pure close contract models one review per accounting period.
- Initial persistence accepts only a domain-created `Open` review.
- Review submission, review-input revision, and close are replayed against the
  stored aggregate using the domain transition methods. Cross-holding,
  cross-company, payload-column, stale-version, and unsupported transitions
  fail closed.
- The active mutation context supplies the bounded idempotency key and actor
  attribution. An exact retry returns a replay receipt without a second
  snapshot or outbox event.
- Each accepted mutation writes the tenant aggregate snapshot and outbox event
  in the same transaction.
- Migration `0034` adds forced-RLS storage, holding-bound keys, and a trigger
  that rejects every update or delete after a row reaches `closed`.
- Review and close candidates cannot manufacture accounting facts. Every
  `CloseJournalFact` is matched by ID to an immutable
  `finance_posted_journals` payload, including holding, company, period, date,
  and aggregated account debit/credit amounts. Malformed, missing, altered,
  or unbalanced source payloads fail closed.
- Resolved and unresolved control flags must exactly match the current
  holding-scoped `finance_close_control_evidence` rows. The evidence ledger is
  forced-RLS and its rows become immutable once the corresponding period is
  closed.

## Verification

- `rustfmt --edition 2021 --check` on the new adapter: PASS.
- `cargo fmt --all --check` and `git diff --check`: PASS.
- Disposable PostgreSQL acceptance
  `live_finance_close_review_schema_is_scoped_and_closed_rows_are_immutable`:
  PASS; forced RLS and closed-row trigger assertions passed.
- Disposable PostgreSQL acceptance
  `live_finance_close_persistence_requires_authoritative_facts_and_controls`:
  PASS; a genuine posted journal and matching control evidence can complete
  open → review → closed, while a synthetic fact is rejected by persistence.
- In a disposable source copy with only the packet module/reexports wired for
  compilation: `cargo check -p erpx-infrastructure`: PASS;
  `cargo test -p erpx-infrastructure finance_close_persistence --lib`: PASS
  (2/2); `cargo clippy -p erpx-infrastructure --all-targets -- -D warnings`:
  PASS.
- Repository migration checks: `cargo test -p erpx-migration`: PASS (6 unit
  tests and 0 doc tests); the new integration test compiled with
  `cargo test -p erpx-infrastructure --test finance_close_persistence
  --no-run`.
- The close-control evidence ledger is a source boundary: Finance banking,
  AP/AR, inventory, and other control producers still need to publish their
  authoritative evidence rows before their close controls can be used. The
  close adapter deliberately fails closed when evidence is absent or stale.

## Known integration order

1. Finance source contexts publish their control evidence through their own
   authoritative write paths.
2. Main agent adds transport, worker, and UI command/query integration.
3. Run focused adapter tests and both disposable close-review tests, then the
   required workspace verification.

## Security considerations

- RLS is forced and every adapter transaction sets `erpx.holding_ref`.
- The adapter cannot accept a caller-supplied idempotency key separate from the
  domain command context.
- Caller-provided journal facts and resolved control flags are not treated as
  authority; both are checked against tenant write-side evidence.
- Closed-row immutability is enforced twice: domain transition validation and a
  database trigger.
- The adapter never reads tenant credentials or emits secrets.
