# G6-PRV-011 — bounded holding projection status adapter

## Delivered behavior

Added a bounded control-plane projection-status read adapter to the durable
holding scheduler. It validates optional holding, holding, and projection
filters, rejects ambiguous holding-only requests, applies deterministic
keyset pagination, and returns only scheduler state/checkpoint metadata. Job
payloads, claim IDs, worker IDs, and credentials are not represented in the
status DTO. Persisted error text is passed through the scheduler redaction and
size bound before it is returned.

The adapter reads the authoritative scheduler coordination rows from the
control write pool. Platform authorization, customer-visible audit, and the
opaque transport encoding remain composition-root responsibilities and are
not bypassed by this lower-level repository method.

## Files changed

- `crates/infrastructure/src/holding_projection_scheduler.rs`
- `crates/infrastructure/src/lib.rs`
- `crates/infrastructure/tests/holding_projection_status.rs`
- `audits/G6-PRV-011/README.md`

No migration, holding database, Folio route, environment file, or runtime
service state changed.

## Verification

From `erpx/`:

```text
cargo fmt --all — PASS
cargo test -p erpx-infrastructure --test holding_projection_status — PASS: 4 tests
cargo clippy -p erpx-infrastructure --all-targets -- -D warnings — PASS
```

Focused tests cover cursor validation and bounded ordering metadata,
payload/claim-identity exclusion, and invalid page/filter rejection before any
database access.

## Security and rollback

The adapter does not accept credentials or arbitrary SQL fragments. Filter
values are validated, page size is capped at 100, holding scope cannot be
specified without its holding, and continuation state is limited to validated
ordering fields. The packet can be rolled back by removing the additive DTO,
repository method, export, focused test, and this audit; no schema rollback is
required.

## Unfinished work

Authenticated Platform Operations transport, control-read projection storage,
projection rebuild actions, freshness producers, and complete provisioning,
backup, integration, audit, and settings workflows remain open. This packet
does not close G6.
