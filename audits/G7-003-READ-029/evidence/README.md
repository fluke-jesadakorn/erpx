# G7-003 Operations event page and rebuild evidence

## Delivered behavior

`SqlxOperationsStore` now exposes a holding-bound, typed event page with a
maximum page size of 500 and an exclusive aggregate/version cursor. It validates
the routed holding and returns deterministic ordering with `has_more` and a
resume cursor. Projection rebuild preparation clears only the selected
projection and holding from the holding read database; write-side events,
snapshots, and other projections are untouched.

## Independent verification

| Command | Result |
|---|---|
| `cargo test -p erpx-infrastructure --test operations_persistence operations_persistence_write_read_boundary_is_versioned_and_idempotent -- --ignored --nocapture` | PASS; 1 disposable PostgreSQL write/read test |
| `cargo test -p erpx-infrastructure` | PASS; 42 unit tests, 2 local integration tests, disposable tests ignored |
| `cargo clippy -p erpx-infrastructure --all-targets -- -D warnings` | PASS |
| `cargo fmt -p erpx-infrastructure -- --check` | PASS |

The disposable write and read databases were created separately and dropped
after the test.

## Remaining boundary

The worker has not yet consumed these APIs for production Operations
projection orchestration, rebuild switching, exception handling, or Folio
reconciliation. No command-owner change was made.

## Rollback

Remove the additive Operations page/rebuild types and methods, their tests,
and this evidence file. Preserve the existing Operations write/read contract
and all unrelated user changes.
