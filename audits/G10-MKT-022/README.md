# G10-MKT-022 — Production Marketing/CRM projection scheduler composition

## Task contract

- **Objective:** wire the already-verified Marketing/CRM projection runner
  into the production worker composition through the established holding
  projection scheduler.
- **Dependencies:** G10-MKT-021 `SqlxMarketingStore` CRM read projection and
  event-receipt adapter; the existing holding projection scheduler,
  Operations/Finance composition, authorization, and claim interfaces.
- **Allowed paths:** `apps/worker/src/marketing_projection.rs`,
  `apps/worker/src/lib.rs`, `apps/worker/src/main.rs`, focused worker tests,
  and this audit.
- **Forbidden paths:** domain, infrastructure, migration, protobuf, server,
  console/UI, Finance, Folio, environment, and runtime files.
- **Database effects:** none. The packet uses existing routed pools and the
  existing holding write/read migrations.
- **Rollback:** disable the Marketing kind selection or revert only the
  worker/audit changes; no schema or authoritative write data is affected.

## Interfaces

Consumed:

- `MarketingProjectionRunner` and
  `SqlxMarketingProjectionFactory`.
- `HoldingDatabaseRouter`.
- `HoldingProjectionExecutor` and the existing
  `MultiHoldingProjectionSchedulerHandler` claim/lease/authorization path.

Produced:

- `MarketingHoldingProjectionExecutor`.
- `MARKETING_PROJECTION_NAME` (`marketing`).
- Explicit worker kind selection and bounded page-size configuration.
- A worker composition helper that registers the existing scheduler with a
  runner-owned executor.

## Delivered behavior

The verified G10-MKT-021 Marketing projection runner is now selectable from
the production worker executable through the established multi-holding holding
projection scheduler.

- `ERPX_WORKER_HANDLER=holding_projection_scheduler` remains the worker handler.
- `ERPX_HOLDING_PROJECTION_KIND=marketing` selects the Marketing/CRM executor.
- `ERPX_MARKETING_PROJECTION_PAGE_SIZE` is parsed as an integer and bounded to
  `1..=500`; the runner default is 500.
- The scheduler continues to provide active-holding enumeration, authorization
  context, claiming, lease renewal, completion, retry, and dead-letter behavior.
- The executor uses the existing `HoldingDatabaseRouter` and
  `MarketingProjectionRunner<SqlxMarketingProjectionFactory>`; it does not
  read credentials, construct database URLs, or create another routing path.
- Incremental claims apply one bounded source page and preserve the existing
  Marketing event-type allow-listing in `SqlxMarketingStore`.
- Rebuild claims are supported through the existing runner rebuild operation,
  which clears and replays only the routed holding's disposable Marketing/CRM
  read model.

The existing Operations and Finance Banking selection paths are unchanged.
Unknown projection kinds fail closed with an explicit configuration error.

## Exact files changed

- `apps/worker/src/marketing_projection.rs`
- `apps/worker/src/lib.rs`
- `apps/worker/src/main.rs`
- `audits/G10-MKT-022/README.md`

No infrastructure, migration, domain, proto, server, console, Finance,
Marketing source, Folio, environment, or runtime files were changed.

## Registration and interfaces

The registered handler kind remains
`HOLDING_PROJECTION_SCHEDULER_HANDLER_NAME` (`holding_projection_scheduler`).
The new worker composition helper registers the same
`MultiHoldingProjectionSchedulerHandler` with a direct
`HoldingProjectionExecutor`; it is not a second scheduler. The Marketing
executor validates the scheduler binding and the `marketing` projection name,
then delegates to the already-routed Marketing runner.

Rebuild payloads intentionally reject `source_version` and cursor fields. The
current Marketing runner does not expose a source high-watermark or
replacement-read candidate activation interface, so the scheduler's
Operations/Finance replacement-read wrapper cannot safely be reused for this
projection without an infrastructure contract change. The supported Marketing
rebuild is therefore an in-place disposable read-model clear/replay. Candidate
read-database activation and source-fenced rebuilds remain open concerns.

## Security and credential review

- Scheduler authorization and claim binding remain in the established control
  plane repository.
- Active holding enumeration still occurs on every scheduler pass.
- The executor requires a non-empty holding/holding binding and the exact
  Marketing projection kind before routing.
- Holding routes are opened only through `HoldingDatabaseRouter`; no holding
  credential or database URL reaches the worker composition code.
- Runner and persistence failures are mapped to bounded retry/permanent errors;
  error strings do not expose PostgreSQL details, credentials, or payloads.
- Existing Marketing event-type, aggregate identity, version, receipt, and RLS
  checks remain authoritative in the infrastructure adapter.

## Verification

- `cargo fmt --all --check` — PASS.
- `cargo check -p erpx-worker` — PASS.
- `cargo test -p erpx-worker marketing_projection -- --nocapture` — PASS,
  6 tests.
- `cargo clippy -p erpx-worker --all-targets -- -D warnings` — PASS.

The existing G10-MKT-021 disposable PostgreSQL projection test remains the
database evidence for routed Marketing/CRM source application, replay,
rebuild clearing, and forced-RLS metadata. A separate disposable scheduler
test was not added: the worker packet is composition-only and the scheduler's
control-plane claim path is already independently covered; adding another
database seam would violate the packet's no-infrastructure-change boundary.

## Unresolved boundaries

Marketing/CRM gRPC transport, UI, provider journeys, consent/delivery
producers, Finance attribution reconciliation, and mobile product surfaces
remain open. The in-place rebuild limitation above also remains until the
shared infrastructure scheduler contract grows a Marketing source watermark
and replacement-read projection variant.

## Rollback

Stop selecting `ERPX_HOLDING_PROJECTION_KIND=marketing` and continue running
Operations or Finance Banking with the unchanged scheduler. Reverting the
worker-only files is safe because no schema or authoritative write data was
changed. Keep already-applied migration history and disposable CRM read rows
from G10-MKT-021 intact.
