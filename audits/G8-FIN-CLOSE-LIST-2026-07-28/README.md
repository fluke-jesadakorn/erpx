# G8 Finance close-review bounded list

## Delivered slice

The Finance close projection now exposes a bounded company-scoped list over
the existing holding-read model. `SqlxFinanceCloseReadAdapter::list_reviews`
uses the read pool only, joins the close projection checkpoint, validates
holding/company/period identity, source version and event identity, lifecycle
state, control totals, statement invariants, freshness, and a bounded
keyset cursor `(projected_at DESC, period_id DESC)`. It returns lifecycle and
report scalars only; close aggregate JSON, journal facts, command context,
credentials, and object references remain server-owned.

`ListFinanceCloseReviews` requires authenticated `finance.read`, explicit
holding/company/branch containment, customer scope membership, holding read
routing, bounded opaque cursors, and a customer audit event. The Finance
Dioxus route renders loading, safe error, empty, bounded table, pagination,
freshness, and read-only boundary states while retaining the existing
single-period detail and authoritative close commands.

## Verification

Passed:

- `cargo fmt --all --check`
- `cargo test -p erpx-infrastructure finance_close_read --offline` — 5/5
- `cargo test -p erpx-console finance_close --no-default-features --features desktop --offline` — 3/3
- `cargo clippy -p erpx-infrastructure -p erpx-server -p erpx-console --all-targets --offline -- -D warnings`
- `cargo check -p erpx-proto -p erpx-infrastructure -p erpx-server --offline`

The ignored disposable two-database acceptance was extended to assert one
scoped list row, source version/freshness, sibling-company isolation, and
clear/rebuild rollback. It remains queued behind the supplied
`ERPX_TEST_HOLDING_WRITE_DATABASE_URL` and `ERPX_TEST_HOLDING_READ_DATABASE_URL`.
No mock, schema-only fixture, or fabricated database result is claimed as
acceptance evidence.

The final mandated ignored workspace sweep after this packet exited 101 with
exactly 71 targets. Those failures remained explicit environment-gated
blockers; no new application assertion failure appeared.

## Rollback

Disable the additive RPC/UI adapter and rebuild the existing holding-read
projection from retained close outbox events. The authoritative Finance close
write tables, outbox history, audit rows, scheduler, and existing detail RPC
remain unchanged. No production state, Folio write ownership, directory
activation, or cutover changed.

## Residual gate status

This closes the bounded Finance close list-read implementation slice, not the
full G8 gate. Provider SDK/credential execution, AP/AR completion, posting,
SoD/approval expansion, consolidation, Thai tax/statements, external vendor
evidence, live disposable SQLx execution, and production migration/cutover
approval remain open and explicitly queued.
