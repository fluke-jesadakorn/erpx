# G10-MKT-003-058 — Marketing holding read projector runner

## Delivered behavior

`apps/worker/src/marketing_projection.rs` defines the standalone worker seam
for the Marketing disposable read model:

- Bounded holding/holding requests with a maximum page size of 500.
- Stable source cursor containing aggregate type, aggregate ID, and aggregate
  version.
- Holding write/read routing through `HoldingDatabaseRouter`.
- One-page projection, bounded catch-up, and clear/replay rebuild operations.
- Idempotent source-event application delegated to the persistence adapter;
  the worker counts only applications reported as new.
- Missing continuation cursors fail closed instead of allowing an unbounded or
  repeated loop.
- Persistence and routing errors are logged with an operation label but exposed
  only as redacted worker errors.
- Both routed database handles are explicitly closed after every normal result
  path; a failed read-route closes the already-open write handle immediately.
- Pure tests cover request bounds, holding/holding validation, cursor validity,
  and error redaction.

The worker deliberately does not choose holding credentials, database URLs, or
write-side business tables.

## Files changed

- `erpx/apps/worker/src/marketing_projection.rs`
- `erpx/audits/G10-MKT-003-058/README.md`

No other paths were changed.

## Contract consumed and produced

The module produces these worker-facing contracts:

- `MarketingProjectionRequest`
- `MarketingProjectionRebuildRequest`
- `MarketingProjectionRunReport`
- `MarketingProjectionRebuildReport`
- `MarketingProjectionWorkerError`
- `MarketingProjectionRunner<F>`

The persistence boundary is explicit in `MarketingProjectionStore` and
`MarketingProjectionStoreFactory`. The infrastructure implementation must
provide:

1. A bounded, stable source-event page from the write database.
2. Idempotent and contiguous-version projection application to the read
   database.
3. Holding-scoped disposable projection clearing.
4. A factory that opens the store from the two already-routed pools.

The current public `SqlxMarketingStore` does not expose these source-event,
projection-apply, or clear APIs. No private or unexported SQLx API was assumed.

## Migration changes

None. The runner consumes the existing Marketing write/read schemas. The main
agent must add or verify source-event and projection-checkpoint schema support
in the existing Marketing migrations when implementing the missing public
infrastructure adapter.

## Verification

- `rustfmt --edition 2024 --check erpx/apps/worker/src/marketing_projection.rs`
  — PASS.
- `cargo check -p erpx-worker` — PASS for the existing worker composition. The
  new module is not yet included by `lib.rs`, so this command does not claim
  compilation of the module’s adapter implementation.
- `cargo test -p erpx-worker --lib` — PASS: 24 passed, 1 ignored. These are the
  existing worker tests; the four new module tests are pending `lib.rs`
  inclusion and adapter implementation.

## Unfinished work / main-agent integration

The main agent must complete these dependency steps:

1. Add public Marketing infrastructure DTOs equivalent to the module’s source
   cursor, source event/page, projection application, and rebuild-preparation
   contracts, or provide lossless conversions to them.
2. Add public `SqlxMarketingStore` methods for source-page reads,
   version/idempotent projection application, and holding-scoped projection
   clearing. These methods must read source events from the write pool and
   write only the replaceable read model.
3. Implement `MarketingProjectionStore` for an adapter around
   `SqlxMarketingStore`, plus `MarketingProjectionStoreFactory` using the
   public `from_pools` constructor.
4. Add `mod marketing_projection;` and re-export the runner, reports, request
   types, handler name, and error from `apps/worker/src/lib.rs`.
5. Add the Marketing worker handler to the worker registry/scheduler and run
   worker tests with the module included.
6. Verify clean and upgrade Marketing migrations, RLS, projection rebuild, and
   end-to-end consent/suppression/retry behavior.

## Security concerns

- Routing is performed from the holding and holding request only; the module
  never accepts raw connection strings or credentials.
- Holding and holding references reject control characters and path/database
  separator characters and are length-bounded.
- Every adapter operation receives both routed pools and the holding reference;
  the adapter remains responsible for installing the scoped database setting
  and relying on forced RLS.
- Worker errors never expose SQLx, host, password, token, or connection
  details.
- Rebuild clears only the routed holding’s disposable read model. The source
  write database is never cleared or mutated by the worker.
- Marketing consent and suppression enforcement must remain on the authoritative
  command/delivery paths; a read projector must not make delivery decisions.

## Integration order

1. Publish the missing Marketing SQLx source/projection APIs and migrations.
2. Implement the two adapter traits and include this module in the worker.
3. Run focused worker tests and strict Clippy.
4. Register the handler, run projection rebuild tests, then run the full
   workspace verification wave.

## Main-agent integration

The public SQLx adapter and migration seam are now integrated:

- `SqlxMarketingStore` exposes bounded source paging, contiguous/idempotent
  projection application, and holding-scoped disposable read clearing.
- Read migration `0024_marketing_projection.sql` adds forced-RLS projection
  checkpoints.
- `apps/worker/src/lib.rs` includes the module and provides
  `SqlxMarketingProjectionFactory`/`SqlxMarketingProjectionStore` lossless
  adapters over already-routed pools.
- The worker module's four pure tests are collected by the worker crate.

Independent integration verification:

- `cargo fmt --all --check` — PASS.
- `cargo check -p erpx-infrastructure` — PASS.
- `cargo test -p erpx-infrastructure --lib` — PASS: 79 passed, 4 documented
  disposable-PostgreSQL tests ignored.
- `cargo clippy -p erpx-infrastructure --all-targets -- -D warnings` — PASS.
- `cargo test -p erpx-migration` — PASS: 6 passed.
- `cargo check -p erpx-worker` — PASS.
- `cargo test -p erpx-worker --lib` — PASS: 28 passed, 1 documented
  disposable-PostgreSQL test ignored.
- `cargo clippy -p erpx-worker --all-targets -- -D warnings` — PASS.

The adapter does not choose credentials or URLs, writes only the disposable
read model, and retains consent/suppression authorization on command and
delivery paths. Durable scheduler registration, channels, journeys, CRM,
attribution, transport/UI, and the G10 gate remain open.

## Rollback

Remove the worker module inclusion and handler registration, leaving the
Marketing write-side tables and existing audience reads unchanged. Because
the runner only writes disposable read state and does not perform long-term
dual writes, disabling it does not alter Marketing command authority. Any
partially rebuilt read model can be cleared and rebuilt after the adapter and
handler are restored.
