# G13 target rollback-readiness hardening

**Audit date:** 2026-07-28  
**Verdict:** **IMPLEMENTED — bounded local invariant; focused Rust execution is blocked by an unrelated dependency compile error.**

## Owned scope

This packet covers only:

- `apps/worker/src/g13_target_runtime.rs`
- the focused unit test in that module
- this audit

No infrastructure tests, server main, protobuf, console shell, manifest,
migration, `MASTER_PLAN.md`, or `EXECUTION_LEDGER.md` file was changed.

## Delivered invariant

The target worker may expire retained rehearsal rows only when the newest
durable rollback rehearsal for the run reports all of the following:

- `state = 'succeeded'`;
- `source_preserved = true`;
- `read_model_rebuilt = true`; and
- `error_present = false`.

Missing rehearsal rows fail closed through the persisted default values. The
existing durable reconciliation and unsettled-work gates remain in force.
This hardening does not advance a migration phase, switch command ownership,
or mutate Folio.

## Focused evidence

The edited runtime passes `rustfmt --edition 2024 --check`.

`cargo test -p erpx-worker --lib g13_target_runtime --offline` did not reach
the target runtime tests. Compilation stops in the pre-existing dependency
`crates/domain/src/quality.rs:373` because `Option<Uuid>::is_some_and` is passed
`Uuid::is_nil` with a borrowed-UUID signature. This file is outside the
allowed scope and was not changed.

No disposable target/rollback database, worker restart, retention expiry,
external service, or production deployment was claimed or performed by this
packet.

## Rollback boundary

Rollback is limited to reverting the predicate/query and its focused test,
then removing this audit. No database or deployment rollback is required.
