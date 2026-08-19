# G11 QualityInspection domain integration tests

**Audit date:** 2026-07-28  
**Status:** Focused regression found; source correction remains outside this
test-only packet.

## Ownership and scope

This packet owns only:

- `crates/domain/tests/quality_inspection.rs`
- this audit

The integration test uses only public `erpx-domain` APIs. It contains no
mocks, infrastructure, server, protobuf, console, manifest, migration, or
ledger changes.

## Invariants covered

- Company and branch scope binding, company-to-branch containment, sibling
  denial, branch-to-company denial, and invalid record scopes.
- Ordered Draft/InProgress/Failed/Passed/Released/Cancelled lifecycle paths,
  retry from Failed, and terminal-state closure.
- Stable `QI-<UUID>` references and trimmed, namespaced idempotency keys across
  equivalent replays, with distinct command identity and changed-key behavior.
- Whole-aggregate equality after rejected scope, quantity, text-validation, and
  lifecycle commands.

## Exact focused evidence

Commands were run from `erpx/`:

| Check | Result |
|---|---|
| `rustfmt --edition 2024 --check crates/domain/tests/quality_inspection.rs` | **PASS** |
| `cargo test --offline -p erpx-domain --test quality_inspection -- --nocapture` | **FAIL — 3 passed, 1 failed** |

Passing tests:

- `scope_binding_and_command_containment_are_fail_closed`
- `lifecycle_is_ordered_and_terminal_states_remain_closed`
- `identity_is_deterministic_for_replays_and_distinguishes_commands`

Failing test:

- `rejected_public_commands_leave_the_whole_aggregate_unchanged`

The failing case calls the public `QualityInspection::record_result` API with
the complete quantity and an oversized disposition note. The API returns the
expected `QualityError::TooLong { field: "disposition_note", max: 255 }`, but
the aggregate is no longer equal to its pre-command snapshot: it has changed
to `Passed`, set `passed_quantity` to `10`, stored the oversized note, and
advanced `updated_at`. The focused test therefore correctly prevents a false
rollback-safe claim. No source fix was made in this packet.

## Explicitly queued G11 work

This packet does not claim G11 completion. The following remain queued for
separate bounded ownership:

- Quality SQLx write/read persistence, outbox/idempotency receipts, holding RLS,
  projection, reconciliation, clear/replay, and live rollback evidence.
- Quality worker registration, claiming, retry/restart behavior,
  idempotent projection, and replay/dead-letter evidence.
- Quality server/transport and UI/read-surface authorization, loading,
  denied, stale, empty, unavailable, and read-only journeys.
- Real provider/device or manufacturing/inventory integration acceptance;
  no provider certification is claimed.

## Rollback boundary

Rollback is removal of this integration test and audit only. No database,
runtime, worker, provider, or customer data was changed. The source-side
rollback defect must be corrected and re-run by an owner authorized to change
`crates/domain/src/quality.rs` before this focused gate can be marked passing.
