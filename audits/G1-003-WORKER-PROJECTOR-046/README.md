# G1-003 worker/projector foundation — packet 046

## Status

Coherent packet handoff. This packet does not claim the G1 gate or production
worker completion. Shared plans and the execution ledger were intentionally not
edited.

## Delivered behavior

- Outbox claims reject nil worker identities, non-positive leases, and batches
  above `MAX_OUTBOX_CLAIM_BATCH_SIZE` (1,000).
- SQLx claims remain `FOR UPDATE SKIP LOCKED`, preserve per-aggregate ordering,
  reclaim stale or malformed leases, and atomically dead-letter exhausted
  attempts before claiming new work.
- Successful publication clears the persisted error and requires the current
  claim owner. Retry and dead-letter persistence sanitize error text at the
  database boundary.
- Dead-letter replay is bounded by the existing cumulative `attempts` and
  `max_attempts` columns. Replay does not reset the delivery budget, clamps
  availability to the database clock, and fails closed for non-dead-lettered or
  exhausted events.
- Projection checkpoints now distinguish `Apply`, `AlreadyApplied`, and
  `OutOfOrder`. Checkpoints advance only by one aggregate version, duplicate
  events are idempotent, and gaps are retried rather than acknowledged.
- Projection receipts validate event identity, enforce claim ownership on
  release, and reset receipts/checkpoints transactionally.
- Projection handlers can decline unrelated aggregate events before a receipt or
  checkpoint is created; the existing Customer 360 and incident handlers use
  that boundary.
- Retry delay arithmetic is normalized, positive, overflow-safe, and capped.

## Files changed

- `crates/infrastructure/src/outbox.rs`
- `crates/infrastructure/src/worker.rs`
- `crates/infrastructure/src/lib.rs`
- `apps/worker/src/lib.rs`
- `audits/G1-003-WORKER-PROJECTOR-046/README.md`

No migration, database schema, domain, application, proto, server, UI, Folio,
master-plan, or execution-ledger files were changed.

## Verification

- `rustfmt --edition 2024 --check crates/infrastructure/src/outbox.rs crates/infrastructure/src/worker.rs` — PASS.
- `rustfmt --edition 2024 --check apps/worker/src/lib.rs` — PASS.
- `cargo check -p erpx-infrastructure --lib` — PASS; three pre-existing
  warnings in `crates/infrastructure/src/commercial.rs`.
- `cargo check -p erpx-worker --lib` — PASS.
- `cargo test -p erpx-infrastructure --lib` — PASS: 54 passed, 4 ignored
  disposable-PostgreSQL tests.
- `cargo test -p erpx-worker --lib` — PASS: 15 passed, 1 ignored
  disposable-PostgreSQL test.
- Focused `cargo test -p erpx-infrastructure --lib outbox::tests` — PASS: 3.
- Focused `cargo test -p erpx-infrastructure --lib worker::tests` — PASS: 2.
- `cargo clippy -p erpx-worker --all-targets -- -D warnings` — PASS.
- `cargo clippy -p erpx-infrastructure --all-targets -- -D warnings` — PASS.
- `cargo fmt --all --check` — PASS.
- `git diff --check` — PASS for tracked changes; the ERPX tree is an existing
  untracked application tree, so packet evidence is recorded by path rather
  than a committed diff.

The disposable SQLx integration suite was not claimed as run in this handoff;
the existing integration test remains ignored unless explicit disposable
control write/read database URLs are supplied.

## Residuals

- External publishing still owns its provider-side idempotency and must use the
  stable event ID/idempotency key when acknowledging an external delivery.
- Projection rebuild still depends on a context-owned handler writing to the
  intended replacement read database. The existing
  `ProjectionReadDatabaseSwitcher` is an integration seam; a physical tenant
  adapter must bind preparation, target-pool rebuild, readiness verification,
  and activation before production use.
- Durable replay history is represented by cumulative outbox delivery attempts;
  a separate replay audit/history model is not part of this no-migration packet.
- Worker composition remains incomplete for the remaining ERPX contexts,
  migration jobs, notifications, and production external publishers.
- The original interrupted handoff blockers were superseded by the independent
  integrated workspace verification; this packet still does not close G1.

## Security notes

- Claim, release, publication, and projection completion require the owning
  worker identity; stale leases are the only reclaim path.
- Aggregate-version gaps and receipt identity mismatches fail closed.
- Persisted worker errors are bounded, control-character-free, and redact
  credential-bearing markers, URLs, tokens, and secrets.
- No payload, tenant credential, storage credential, or database URL is added
  to worker errors or diagnostics by this packet.
- Replay cannot reset the cumulative attempt budget or bypass a non-dead-letter
  state.

## Integration order

1. Repair and independently verify the unrelated commercial/domain formatting
   and clippy blockers.
2. Run the ignored SQLx worker tests against disposable, physically distinct
   control write/read databases, including claim lease recovery, replay budget,
   and projection gap cases.
3. Implement a tenant-scoped replacement-read adapter that rebuilds on the
   candidate pool, records verified source/checkpoint state, and only then
   activates the registry switch.
4. Integrate authenticated outbox publishers and remaining projector handlers
   with event-ID idempotency and bounded retry policies.
5. Re-run the full G1-003/G1-004 verification wave before accepting downstream
   identity and business-context packets.

## Rollback

Rollback is limited to the packet-specific additions in the four source files
listed above and removal of this evidence directory. No migration or runtime
database state was changed, so rollback requires no database downgrade. Existing
pre-packet handlers, receipts, checkpoints, and worker composition remain the
safe fallback if the packet changes are reverted by the owning integrator.
