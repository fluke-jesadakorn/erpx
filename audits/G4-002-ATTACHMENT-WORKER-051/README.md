# G4-002 attachment expiry and SLA worker seam

## Delivered

`support_attachments.rs` adds a bounded attachment expiry handler backed by
the real `SupportPersistence::sweep_expired_attachments` method, plus an
injected SLA notification handler seam. Both enforce bounded batches, redact
all persistence error detail, and report retry-safe processed work. SLA
notification timing uses the domain's half-open delivery window; the default
worker registry now includes attachment expiry, while SLA delivery requires an
explicit source composition.

## Independent verification

- `cargo fmt --all` — PASS.
- `cargo test -p erpx-worker --lib` — PASS: 17 tests, 1 disposable Stripe
  test ignored.
- `cargo clippy -p erpx-worker --all-targets -- -D warnings` — PASS.

## Residuals

Durable SLA notification rows/claims, notification outbox delivery, customer
and internal channel adapters, support attachment object bytes, signed access,
transport/UI integration, and production scheduler configuration remain open.

## Security and rollback

The worker consumes only metadata and typed notification records; it never
handles capability secrets or object credentials. Revert the module and remove
the default registry entry after draining attachment expiry work; do not undo
already-recorded attachment status transitions without a reviewed data repair.
