# G6-PRV-005-059 — Holding projection scheduling and claiming

## Delivered behavior

This isolated packet adds a durable control-plane contract for holding-routed
projection work:

- Typed holding/holding binding, incremental and rebuild jobs, policy, lease,
  claim, retry, stale-reclaim, dead-letter, and rebuild records.
- Bounded page size, batch size, retry attempts, lease duration, and
  exponential backoff. Invalid values fail closed.
- Schedule idempotency through a request hash and a scoped schedule key.
- Multi-worker claiming uses a transaction, `FOR UPDATE SKIP LOCKED`, active
  claim uniqueness, worker/claim ownership checks, and a sealed claim batch.
- Repeating the same claim key for the same worker and binding is a replay;
  another worker or scope receives a conflict.
- Lease expiry is exact: `now == lease_until` is expired. Stale claims are
  reclaimed at that boundary and either become retryable or terminal
  dead-letter records when the attempt budget is exhausted.
- Completion, renewal, and failure require the current worker, claim identity,
  scope, active lease, and non-terminal state.
- Error text is bounded and redacted before persistence.
- The repository includes a server-side route verification hook for the
  existing `HoldingDatabaseRouter`; holding credentials do not enter the
  scheduler contract.

## Files changed

- `erpx/crates/infrastructure/src/holding_projection_scheduler.rs`
- `erpx/crates/migration/migrations/0025_holding_projection_scheduler.sql`
- `erpx/audits/G6-PRV-005-059/README.md`

No shared exports, migration composition roots, application code, worker code,
server code, UI code, runtime files, or existing source authorities were
changed.

## Contract and migration changes

The additive migration creates:

- `erpx_control.holding_projection_jobs`
- `erpx_control.holding_projection_claim_batches`
- `erpx_control.holding_projection_claims`
- `erpx_control.holding_projection_rebuilds`

The schema enforces bounded numeric policies, valid state values, scope fields,
claim uniqueness, active-job claim uniqueness, lease ordering, and rebuild
state/kind consistency. It is control-plane coordination state only; holding
write/read aggregate and projection tables remain physically separate.

## Tests and exact results

Pure tests were authored in the new module for:

- fail-closed state transitions;
- exact lease expiry boundary;
- holding/holding binding isolation;
- page, batch, attempt, and backoff bounds;
- stale lease reclaim;
- terminal dead-letter state and redaction;
- idempotent, scope-bound claim keys;
- rebuild initial state.

Verification run for this isolated packet:

```text
rustfmt --edition 2024 --check erpx/crates/infrastructure/src/holding_projection_scheduler.rs
PASS

git diff --check -- erpx/crates/infrastructure/src/holding_projection_scheduler.rs erpx/crates/migration/migrations/0025_holding_projection_scheduler.sql
PASS
```

`cargo test` and crate-level compilation are intentionally pending. The packet
restriction forbids adding the module to
`erpx/crates/infrastructure/src/lib.rs` and forbids changing the migration
composition root, so Cargo cannot collect this new module's tests yet. No
completion claim is made for those deferred checks.

## Unfinished work

- Main integration must register the module and export the selected public
  types without exposing holding pools or credentials.
- Main integration must add migration 0025 to the migration verification
  assertions and run clean/upgrade migration tests.
- The composition root must route claims to each holding's write/read pools,
  invoke projection handlers, and independently verify projector checkpoints.
- SQL repository tests require a disposable PostgreSQL database after the
  migration is wired. This packet contains pure tests only.
- Rebuild execution still belongs to the worker/projector integration packet;
  the main integration now supplies a typed worker scheduler/executor seam,
  but production multi-holding projector composition remains open.

## Security concerns

- Main integration must require authoritative platform authorization before
  scheduling, claiming, replaying, or rebuilding jobs.
- `holding_id` and `data_profile_key` must be resolved through the control-plane
  registry/router; caller-provided strings alone are not authorization.
- Claim keys must be generated as request idempotency keys and must not contain
  secrets or bearer tokens.
- Diagnostic and dead-letter consumers must preserve the redaction boundary;
  the scheduler deliberately stores only bounded error text.
- Rebuild and dead-letter operations need separate high-risk authorization and
  audit events when exposed through an API.

## Integration order

1. Add `mod holding_projection_scheduler` and the intentionally selected
   re-exports in the infrastructure composition root.
2. Add migration 0025 to migration verification and run clean/upgrade tests.
3. Construct the repository from the control-plane write pool.
4. Verify the holding route and holding binding before opening holding write/read
   pools for each claimed job.
5. Wire the worker's bounded claim loop, lease renewal, projector completion,
   retry, stale reclaim, dead-letter inspection, and rebuild handler.
6. Independently run module tests, migration tests, strict Clippy, the full
   workspace suite, and affected end-to-end isolation tests.

## Rollback

Before integration, remove only the three files listed above. After migration
application, rollback must use the repository's normal migration/retention
procedure; do not drop scheduler tables while jobs are active. No existing
aggregate, outbox, checkpoint, holding database, or Folio source data is
modified by this packet.
