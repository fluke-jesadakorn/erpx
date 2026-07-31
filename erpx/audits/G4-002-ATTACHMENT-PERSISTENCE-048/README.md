# G4-002 attachment persistence completion

## Delivered

`SupportPersistence` now provides a bounded SQLx metadata boundary for
`SupportAttachment`:

- transactional `insert_attachment` with ticket row locking, customer/ticket
  binding, typed scope containment, domain attachment-count validation, and
  ticket plus attachment version semantics;
- bounded ticket metadata listing with authorized-scope filtering;
- customer-visible listing that excludes internal, revoked, and expired rows
  and projects through the domain safe DTO;
- optimistic, idempotent `revoke_attachment`;
- clock-guarded `expire_attachment` and bounded `sweep_expired_attachments`;
- idempotency replay and request-hash/command/resource conflict handling for
  insert, revoke, and explicit expiry;
- metadata-only SQL: object bytes, signed URLs, bearer credentials, and
  capability secrets are neither accepted nor persisted.

The support persistence migration boundary now applies the existing
`0016_support_attachments.sql` after the existing support aggregate schema.
The public infrastructure exports include the attachment page/result types
and page-size bound.

## Files

- `crates/infrastructure/src/support_persistence.rs`
- `crates/infrastructure/src/lib.rs`
- `audits/G4-002-ATTACHMENT-PERSISTENCE-048/README.md`

## Focused tests

Unit tests in `support_persistence.rs` cover visibility/status parsing, page
and expiry bounds, typed scope containment, and safe domain-error mapping.
Disposable PostgreSQL tests remain in the existing infrastructure integration
test suite and require `ERPX_SUPPORT_TEST_DATABASE_URL` (or the configured
control write URL); this packet did not change that test harness.

## Verification

- `rustfmt --edition 2024 --check crates/infrastructure/src/support_persistence.rs` — PASS.
- `cargo check -p erpx-infrastructure` — blocked by pre-existing compile errors in `crates/domain/src/operations.rs` (`ExceptionLifecycleCommand` references fields belonging to `ExceptionCase`). No attachment-source error was reached.
- `cargo test -p erpx-infrastructure --lib` — blocked by the same pre-existing domain compile errors.
- `cargo clippy -p erpx-infrastructure --lib -- -D warnings` — blocked by the same pre-existing domain compile errors.
- Workspace `cargo fmt --all --check` — blocked by pre-existing unformatted changes in finance and operations files outside this packet; the attachment file itself passes rustfmt.

## Limitations and integration order

The control-plane migration composition root still needs to register migration
`0016` in the normal migration set, or invoke `SupportPersistence::apply_migration`
before composing this repository. That composition change is outside this
packet's allowed paths. Object-storage upload and signed download authorization,
support transport, expiry worker wiring, and UI remain later integration work.

## Security and rollback

Ticket rows are locked before attachment count and mutation checks. Reads use a
half-open `[uploaded_at, expires_at)` active window and filter scopes before
returning data. Customer reads never return internal attachment rows. Database
constraints and the binding trigger reject customer/ticket rebinding and
closed-ticket mutations. Idempotency reservations are transactional and a
hash, command, or resource mismatch fails closed.

Before migration deployment, remove only the packet's source changes. After
`0016` has been deployed, use a reviewed migration-specific rollback only
after confirming the sidecar is empty or backed up; it must remove the
attachment table, indexes, trigger/function, and restore the prior support
idempotency constraint.
