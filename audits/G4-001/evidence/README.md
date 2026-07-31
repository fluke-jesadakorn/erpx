# G4-001 / G4-003 / G4-005 support persistence evidence

## Delivered packet

- Added SQLx write-side persistence for support tickets, customer-approved
  access requests, hash-only support capabilities, and incidents.
- Added bounded idempotency records and optimistic aggregate versions for
  inserts, approvals, revocations, and incident transitions.
- Added database checks and triggers that keep approved request and capability
  bindings immutable, cap capability expiry to the approved request, require
  approved requests before capability issuance, and retain revocation state.
- Added `SupportPersistence` to the infrastructure public surface. The
  control-plane SQLx migrator applies `0005_support_incidents.sql` on the
  write database only.

## Independent verification

```text
cargo fmt --all --check                                      PASS
cargo clippy --workspace --all-targets -- -D warnings       PASS
cargo test --workspace                                      PASS
cargo test -p erpx-infrastructure --test support_persistence -- --ignored --nocapture  PASS
```

The ignored integration test ran on a named disposable PostgreSQL database
and verified idempotent ticket/incident writes, customer approval, hash-only
capability storage, secret non-disclosure, immutable bindings, and incident
optimistic versioning. The database was dropped after the run.

## Remaining integration

Authoritative server support authorization still needs the full intersection
of agent role, capability, scope, module entitlement, billing state, policy,
segregation of duties, and represented-user authority. Worker expiry and
revocation projection, diagnostics collection, SLA/escalation, and support UI
remain open.
