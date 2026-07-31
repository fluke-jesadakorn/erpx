# G4-005 support capability binding evidence

This packet tightens the support approval/capability boundary and verifies it
against disposable PostgreSQL state.

## Delivered behavior

- Support requests carry an explicit bounded module, defaulting to `support`.
- Repeated approval of the same support aggregate is rejected.
- The server issues the one-time opaque capability secret only after customer
  approval and returns its identifier/secret without persisting the raw value.
- The control repository stores only the 32-byte secret hash and binds the
  capability to request, customer, agent, device reference, ticket, mode,
  permissions, scope, module, and expiry.
- The support migration trigger rejects a capability module that differs from
  the immutable request module.
- A bounded `FOR UPDATE SKIP LOCKED` sweep revokes active capabilities at the
  injected expiry boundary, records `expired`, increments version, and is safe
  to retry without changing already-revoked rows.
- Session and terminal-ticket revocation now drain active capability rows in
  bounded `SKIP LOCKED` batches in the control repository; repeated delivery
  is idempotent.
- The named worker registry includes a bounded expiry handler that drains
  expired capabilities through the repository without exposing persistence
  errors or bearer material.
- The disposable persistence test covers hash-only storage, approval
  immutability, wrong-secret failure, module binding, and revocation.

## Independent verification

Run from `erpx/` with a disposable database:

```text
ERPX_SUPPORT_TEST_DATABASE_URL='postgres://fluke@127.0.0.1:5432/erpx_support_20260724_08' cargo test -p erpx-infrastructure --test support_persistence -- --ignored --nocapture
```

Result: PASS; 4 tests. The disposable database was dropped after the run. The
database-backed tests serialize their shared disposable-database fixture so
the global expiry sweep cannot consume another test's active rows.

Additional checks:

| Command | Result |
|---|---|
| `cargo fmt --all --check` | PASS |
| `cargo test -p erpx-domain` | PASS; 56 tests |
| `cargo test -p erpx-server` | PASS; 42 tests |
| `cargo test -p erpx-infrastructure --test support_persistence -- --ignored --nocapture` | PASS; 4 disposable PostgreSQL tests |
| `cargo test -p erpx-worker` | PASS; 12 tests, 1 ignored |
| `cargo clippy --workspace --all-targets -- -D warnings` | PASS |
| `cargo test --workspace` | PASS |

## Security gaps and rollback

The mirrored approval and capability rows now commit together, but that
transaction still follows the authoritative support aggregate/outbox commit.
The original packet accepted a bounded device reference without resolving the
identity device; a subsequent hardening packet now resolves the active session
device at approval and rechecks it at signed-object access. Worker
Production triggers/workers for role/ticket/customer cascade revocation,
capability-bound customer diagnostics, incidents, and server/native support UI
remain open. Rollback is limited to the
additive module/revocation fields, persistence methods, trigger, server
response wiring, and worker registration; Folio remains the write owner.
