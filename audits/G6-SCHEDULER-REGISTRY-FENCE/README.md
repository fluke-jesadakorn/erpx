# G6 scheduler registry lifecycle fence

Date: 2026-07-28

This packet closes the local scheduler race identified by the G6 follow-up: a
holding registry lifecycle change could otherwise occur after a scheduler
preflight check but before the scheduler mutation transaction completed.

## Authoritative behavior

- Schedule, claim, renew, complete, fail, terminalize-stale-rebuild, and stale
  reclaim now begin their SQLx transaction and lock the matching
  `erpx_control.holding_registry` row with `FOR UPDATE`.
- The locked row is revalidated for `state = active` and the exact
  `data_profile_key` before scheduler rows, leases, claims, rebuild state, or
  failure history are mutated.
- Holding lifecycle and replacement-read activation already use the same
  registry-row transaction lock. A lifecycle transition therefore waits for,
  or rejects before, the scheduler side effect rather than racing it.
- Existing permission/audit preflight remains in place. The row lock is a
  transaction fence, not a replacement for authorization.
- Schedule idempotency and claim-batch idempotency remain transactional;
  credentials, payloads, worker identity, and lease material are not added to
  public read models.

## Verification

```text
cargo fmt --all --check                       PASS
cargo check -p erpx-infrastructure --offline  PASS
cargo test -p erpx-infrastructure --offline   PASS
```

The infrastructure unit run completed with 219 tests and no failures. A fresh
eight-database disposable PostgreSQL workspace run also passed every local
ignored target, including the scheduler persistence tests. The sole nonzero
target was the intentionally credentialless `erpx-migrator` G14 external
readiness preflight, which recorded the exact Generic LDAP/RFC4533, AD
DS/DirSync, and UCS Provisioning Service references still required for vendor
certification. All disposable databases were removed after the run.

This is a local G6 continuation, not a claim of MinIO, credentialed external,
PITR/WAL, production, or full G0–G14 acceptance.
