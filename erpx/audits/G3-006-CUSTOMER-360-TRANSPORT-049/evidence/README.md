# G3-006 Customer 360 detail transport evidence

## Scope

Added `PlatformCustomerService.GetCustomer360Detail`. The request requires an
authenticated query context, a bounded notes cursor/page size, a validated
purpose, role authorization, and explicit customer scope. The repository
checks non-platform scope against the authoritative write pool before reading
the control read projection. The response serializes only the typed,
redacted, freshness-aware Customer 360 detail model and records the global
read with actor, session, module, scope, purpose, time, and correlation.

Absent and unsupported projections retain distinct safe response states.
Customer detail reads never return credentials, signed object URLs, or raw
internal attachment/note data.

## Changed files

- `erpx/crates/proto/proto/erpx/platform/v1/platform.proto`
- `erpx/apps/server/src/main.rs`

## Verification

- `cargo fmt --all` — PASS.
- `cargo check -p erpx-server` — PASS.
- `cargo test -p erpx-server` — PASS; 51 tests.
- `cargo clippy -p erpx-server --all-targets -- -D warnings` — PASS.

## Residual work

Customer 360 application/client adapters, pagination UI, subtree/multi-holding
semantics beyond the bounded transport scope, notes/attachments worker
backfill, binding-revocation reconciliation, and native/deep-link surfaces
remain open.

## Security and rollback

Global reads are audited at the transport boundary and scope containment is
authoritative on the write side. The detail payload is domain-redacted before
serialization. Rollback is limited to the two source files and this evidence
directory.
