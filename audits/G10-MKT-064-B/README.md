# G10-MKT-064-B — Durable Marketing abuse-window cleanup worker

## Delivered behavior

- The worker exposes `marketing_public_abuse_cleanup` as an explicit opt-in
  handler; the default registry never claims it implicitly.
- The handler accepts only a bounded batch (`1..=1000`), computes a fixed
  24-hour retention cutoff, calls only the control-plane purge port, and
  returns a cleanup count without client identity or persistence details.
- The executable composition root registers the handler only when
  `ERPX_WORKER_HANDLER=marketing_public_abuse_cleanup`. Batch configuration is
  bounded by `ERPX_MARKETING_PUBLIC_ABUSE_CLEANUP_BATCH_SIZE` and the worker
  runtime default.
- The handler rejects a nil worker ID and maps database failures to a generic
  redacted worker error. It never routes a holding database.

## Files and contracts

- Worker handler and injected purge port:
  `apps/worker/src/marketing_public_abuse.rs`.
- Worker module export and executable registration:
  `apps/worker/src/lib.rs` and `apps/worker/src/main.rs`.
- Authoritative bounded control-plane purge:
  `crates/infrastructure/src/repositories.rs`.

## Verification evidence

- Six focused worker tests — PASS.
- `cargo check -p erpx-worker` — PASS.
- `cargo clippy -p erpx-worker --all-targets -- -D warnings` — PASS.
- `cargo fmt --all --check` — PASS.
- Fresh disposable control-plane PostgreSQL abuse proof — PASS; quota
  allowance/limit/new-window behavior and one-row bounded cleanup passed, with
  zero table/holding residue before the database was dropped.

## Security and scope notes

The cleanup handler is control-plane-only and does not inspect or persist IPs,
contact values, request bodies, tokens, or fingerprints. It is a bounded
retention cleanup seam; durable scheduling, worker leases, and DLQ semantics
remain owned by the broader worker runtime and are not fabricated here.

## Rollback

Stop selecting the handler name or remove its registration branch. Existing
abuse counters remain intact for a later bounded cleanup; no holding write/read
state is changed.
