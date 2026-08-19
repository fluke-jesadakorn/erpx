# G6-PRV-018 — failure-safe replacement-read orchestration

## Delivered behavior

The worker replacement-read boundary now requires an explicit abort operation.
When a candidate projection rebuild fails, the orchestrator best-effort marks
the candidate replacement failed and leaves the active read database selected.
When activation fails, the same cleanup path is attempted after the switcher’s
own bounded failure handling. Projection names remain exact-bound, and all
errors returned by the SQLx switcher are replaced with safe fixed messages.

This is control-plane state-transition safety only. It does not create a
candidate database, select holding credentials, run a context projector against
the candidate pool, expose a server action, or claim that physical replacement
switching is complete.

## Files changed

- `apps/worker/src/lib.rs`
- `crates/infrastructure/src/lib.rs`
- `audits/G6-PRV-018/README.md`
- `MASTER_PLAN.md`
- `EXECUTION_LEDGER.md`

## Contract and migration effects

`ProjectionReadDatabaseSwitcher` now consumes `prepare_replacement`,
`abort_replacement`, and `activate_replacement`. The abort reason is a
compile-time static worker-selected message. No database migration or schema
change was made.

## Verification

```text
cargo fmt --all --check — PASS
cargo check -p erpx-worker — PASS
cargo clippy -p erpx-worker --all-targets -- -D warnings — PASS
cargo test -p erpx-worker replacement_switcher_is_bounded_and_fail_closed_on_projection_mismatch — PASS
```

## Security and unfinished work

The switcher validates holding, candidate database, source-version, and
projection references; the candidate key cannot contain path, URL, whitespace,
or control characters. Database errors and connection details are not exposed
through the worker error boundary. Physical candidate provisioning, migration,
candidate-pool routing, projector execution on that pool, authenticated
rebuild/switch actions, and backup/restore integration remain open.

## Rollback

Remove only the additive abort method, orchestrator cleanup branch, export,
focused test assertion, and this audit. Retain the existing replacement state
machine until a reviewed migration/retention decision is made.
