# G6 scheduler composite binding hardening — 2026-07-28

## Scope

This packet closes a control-plane persistence hardening slice for the durable
holding projection scheduler. It does not activate holdings, change command
ownership, retire Folio writes, or perform a production cutover.

## Delivered behavior

- Control migration `0064_holding_projection_scheduler_binding.sql` adds
  composite `(holding_id, data_profile_key)` references from scheduler jobs, claim
  batches, claims, and rebuilds to the authoritative active-registration
  identity.
- Claim rows additionally reference the exact scoped job and the exact
  scoped worker-owned claim batch. Direct SQL cannot pair a valid claim ID,
  job, holding, and worker from different scheduler scopes.
- Existing repository checks, registry row locks, authorization facts, and
  transactional claim/job/rebuild transitions remain authoritative; the
  migration is defense-in-depth for recovery and out-of-band writers.
- The scheduler integration acceptance asserts all seven named composite
  constraints after a fresh control write/read migration and still exercises
  unregistered, suspended, active, rebuild, stale-claim, and audit paths.

## Evidence

```text
cargo fmt --all --check                                      PASS
cargo test -p erpx-migration --offline                       PASS: 12 passed
cargo test -p erpx-infrastructure --test holding_projection_scheduler \
  --offline -- --ignored --nocapture --test-threads=1          PASS: 1 passed
```

The ignored acceptance used fresh local PostgreSQL control write/read
databases and dropped both databases after the run. No customer or external
directory environment was used.

## Remaining G6 evidence

This is one persistence-integrity slice, not G6 completion. Multi-holding
runtime/provider/device acceptance, full crash/retry/idempotency evidence,
external directory certification, approved descriptors, and production
migration/cutover approval remain queued. The scheduler continues to fail
closed when a holding registration or external route is unavailable.

## Rollback boundary

The migration is additive and has no application-data deletion path. A failed
upgrade must stop before scheduler activation; existing authoritative jobs,
claims, outbox events, and audit history remain intact. Read models remain
rebuildable from their source streams.
