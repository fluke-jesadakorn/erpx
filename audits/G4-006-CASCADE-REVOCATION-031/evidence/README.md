# G4-006 bounded support capability cascade evidence

## Delivered behavior

The control repository now provides bounded, active-only, retry-safe cascade
revocation by customer, agent, parent-role agent reference, and ticket. Each
batch uses `FOR UPDATE SKIP LOCKED`, increments the capability version once,
records the supplied revocation reason and timestamp, and returns redacted
metadata. Scope and batch-size validation fail closed.

## Independent verification

| Command | Result |
|---|---|
| `ERPX_SUPPORT_TEST_DATABASE_URL='postgres://fluke@127.0.0.1:5432/erpx_support_20260724_08' cargo test -p erpx-infrastructure --test support_persistence -- --ignored --nocapture` | PASS; 4 disposable PostgreSQL tests |
| `cargo test -p erpx-infrastructure --lib` | PASS; 42 tests, 3 ignored |
| `cargo clippy -p erpx-infrastructure --all-targets -- -D warnings` | PASS |
| `cargo fmt -p erpx-infrastructure -- --check` | PASS |

The database-backed tests use unique UUID fixtures and a shared in-process
guard because the production expiry sweep is intentionally global to its
control database. The disposable database was dropped afterward.

## Remaining boundary

Production triggers/workers still need to invoke customer-suspension,
agent-role-expiry, and parent-role-expiry cascades. Ticket closure currently
calls its bounded drain after the authoritative command/outbox transaction;
session revocation has the same bounded-drain property. Customer-bound
diagnostics and complete support workflows remain open.

## Rollback

Remove the additive cascade repository methods and this evidence file. No
command-owner or Folio state changes were made.
