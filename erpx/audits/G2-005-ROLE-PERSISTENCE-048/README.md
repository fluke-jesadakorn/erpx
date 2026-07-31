# G2-005 durable role-review persistence

## Delivered

The control-plane infrastructure now has a bounded `RoleReviewPersistence`
repository for immutable review snapshots, derived expiry notifications, and
recovery approval receipts. Snapshot writes are idempotent and hash-bound;
items preserve assignment scope, parent/version, permissions, risk tier, and
finite validity. Notification claims are exact-scope, bounded, leased, and
`SKIP LOCKED`. Recovery receipts hash reason/ticket/request material and never
store secrets.

## Evidence

- `crates/infrastructure/src/role_review.rs` — typed SQLx repository and safe
  scope/risk/notification mappings.
- `crates/migration/migrations/0015_role_review_persistence.sql` — control
  write schema for snapshots, notifications, claims, and recovery receipts.
- `crates/infrastructure/src/lib.rs` — public repository exports.
- Domain role-review and recovery tests remain authoritative for exact clock,
  parent, approval, and finite-window rules.

## Independent verification

- `cargo fmt --all` — PASS.
- `cargo check --workspace` — PASS.
- `cargo test -p erpx-domain` — PASS: 87 tests.
- `cargo clippy -p erpx-infrastructure --lib -- -D warnings` — PASS.

Disposable PostgreSQL execution is still environment-gated and was not
claimed by this packet; migration execution, worker notification delivery,
approver/session/SoD resolution, recovery command receipt integration, and
universal mutation adoption remain open.

## Security and rollback

Scope is stored structurally and claims are exact-scope; request hashes and
recovery reasons are persisted as digests. This packet stores no credentials,
tokens, or recovery secrets. Roll back only with a reviewed migration after
stopping review workers and checking that no claimed notification is active;
never remove authoritative role assignments.
