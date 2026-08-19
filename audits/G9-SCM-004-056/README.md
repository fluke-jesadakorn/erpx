# G9-SCM-004 standalone SCM holding projector runner

## Delivered behavior

- Added a standalone holding-routed SCM projector module.
- Validated holding/holding references and bounded page sizes before routing.
- Routes both the holding write and read databases through
  `HoldingDatabaseRouter`; holding database handles are closed on every exit
  path after routing.
- Applies bounded source-event pages through `SqlxScmStore::project_event`.
  The infrastructure boundary supplies aggregate identity/version validation,
  contiguous projection checks, and idempotent replay behavior.
- Provides a catch-up method that reads and applies all available pages while
  retaining the infrastructure cursor internally.
- Clears only the requested holding's SCM projection and rebuilds it from the
  authoritative SCM source outbox through `clear_projection` and
  `replay_event`.
- Emits only redacted worker-facing errors and never formats database
  connection details.
- SCM remains the source owner; no Operations or Finance source tables are
  touched.

## Files changed

- `apps/worker/src/scm_projection.rs`
- `audits/G9-SCM-004-056/README.md`

## Contract and migration changes

- No domain, infrastructure, migration, protobuf, server, UI, or shared-file
  changes.
- The infrastructure crate currently does not publicly re-export the SCM
  cursor/event/page types. This packet therefore uses the source page/event
  fields through the public store API and keeps cursors internal to catch-up
  and rebuild. The main integration packet must publish a continuation DTO
  when external cursor paging is required.

## Main integration verification

The main agent exported the module from `apps/worker/src/lib.rs` and
independently reran the worker suite with the module compiled and linted as
part of the crate:

- `cargo fmt --all --check` — PASS.
- `cargo test -p erpx-worker --lib` — PASS: 22 passed, 1 documented
  disposable-PostgreSQL test ignored.
- `cargo clippy -p erpx-worker --all-targets -- -D warnings` — PASS.

## Packet verification

Run from `erpx/`:

- `rustfmt --edition 2024 --check apps/worker/src/scm_projection.rs` — PASS.
- Isolated `rustc --edition 2024 --test ... -D warnings` against the built
  infrastructure crate — PASS: 3 tests passed.
- `cargo check -p erpx-worker` — PASS; this file is intentionally not exported
  until the main integration packet.
- `cargo test -p erpx-worker --lib` — PASS: 19 passed, 1 documented
  disposable-PostgreSQL test ignored; the three isolated tests above cover
  this unexported file.
- `cargo clippy -p erpx-worker --all-targets -- -D warnings` — PASS for the
  current worker composition; this file will also be linted when exported.
- `cargo fmt --all --check` — BLOCKED by pre-existing formatting differences
  in forbidden `apps/worker/src/lib.rs` (lines 617 and 693); direct formatting
  of this packet passes and no forbidden file was edited.

No production database URL or credential was used in this packet or its
evidence.

## Unfinished work

- Main must export and integrate this module from the worker crate.
- Main must decide whether to publicly re-export the SCM cursor/event/page
  types and add continuation-aware scheduling/claiming.
- Durable per-holding scheduling/claiming, transport registration, source
  event production, downstream Operations/Finance handoffs, and full
  Procure-to-Pay reconciliation remain open.

## Security concerns

- Routing is fail-closed through the control-plane holding registry and holding
  binding; holding credentials remain server-side.
- Inputs reject whitespace/control/path separator characters and unbounded
  page sizes.
- Projection failures are redacted at the worker boundary.
- The runner has no authority to mutate Operations or Finance source state.
- Production integration must preserve authoritative write-side RBAC,
  entitlement, billing, audit, and support-capability checks before invoking
  any source command.

## Integration order

1. Main exports this module from `apps/worker/src/lib.rs`.
2. Main publishes the SCM source cursor/event/page types or supplies a
   continuation adapter.
3. Main registers the runner with durable holding scheduling/claiming.
4. Main adds authenticated transport and downstream event consumers.
5. Main independently runs the worker, infrastructure, migration, and full
   ERPX verification waves.

## Rollback

Remove `apps/worker/src/scm_projection.rs` and this evidence file. No shared
source rows, migrations, or database schema changes are introduced by this
packet, so existing SCM source/outbox data remains available for replay.
