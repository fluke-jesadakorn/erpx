# G6-PRV-021 — candidate source high-watermark verification

## Objective

Prevent a replacement-read candidate from being activated after replaying an
incomplete or moving source stream.

## Delivered behavior

- Operations and Finance Banking stores expose a holding-scoped immutable
  source-event count as a bounded high-watermark.
- Candidate projectors receive the expected source version explicitly.
- Each candidate rebuild compares the source watermark before replay and
  after replay. An intent mismatch is permanent; a source that changes during
  replay is retryable and cannot activate.
- The existing generation-fenced switch remains the only activation path.

## Files changed

- `crates/infrastructure/src/operations_persistence.rs`
- `crates/infrastructure/src/finance_banking_persistence.rs`
- `apps/worker/src/lib.rs`
- `apps/worker/src/finance_banking_projection.rs`
- `apps/worker/tests/operations_projection.rs`
- `erpx/audits/G6-PRV-021/README.md`

No migration, protobuf, server, Folio, credential, or runtime configuration
files were changed. The watermark is derived from append-only source rows and
is not persisted as a new schema object.

## Verification

- `cargo fmt --all --check` — PASS.
- `cargo clippy -p erpx-worker --all-targets -- -D warnings` — PASS.
- `cargo check -p erpx-infrastructure -p erpx-worker --tests` — PASS.
- `cargo test -p erpx-worker --test operations_projection -- --ignored
  --nocapture` — PASS against disposable control/write/read/candidate
  PostgreSQL databases; the candidate replay and activation completed with
  the expected one-event watermark.
- The disposable databases were explicitly dropped after the test.

## Security and reliability

The watermark is scoped through the same transaction-local holding setting as
source reads. Candidate completion is refused when the requested version does
not equal the observed source count, and source movement returns a retryable
failure before activation. No payload, credentials, URLs, or holding database
keys cross the worker error boundary.

## Unfinished work

Durable dispatch from the authenticated Platform Operations intent into this
candidate executor, source high-watermark selection for every future context,
external backup/restore execution, remaining Platform Operations actions, and
the broader G6/G7/G8 gates remain open.

## Rollback

Remove only the additive watermark methods, the candidate-projector argument,
and this evidence. No database rollback is required and no active read key is
changed by rollback.
