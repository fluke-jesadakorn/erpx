# G10-MKT-062 — Marketing profile erasure and suppression

## Delivered behavior

- `MarketingProfileErasure` is a typed, tenant-scoped, contact-free erasure
  command. It validates identity, scope, timestamp, and bounded reason data,
  and exposes `validate_for_scope` for the authoritative command boundary.
- The tenant-write SQLx store erases the profile in one transaction: contact
  fields and attributes are removed, the stable profile identity is retained,
  the profile becomes immutable, and a permanent profile-erasure suppression
  record is written.
- The erasure outbox event contains only the redacted tombstone. Existing
  profile outbox payloads are replaced in the same transaction with the same
  tombstone plus a privacy marker, preserving event IDs, versions, timestamps,
  and audit metadata while preventing replay from restoring contact data.
- Consent and suppression mutations reject erased profiles. Replaying an
  erasure idempotency key returns the original receipt.
- The tenant-read projector recognizes both the erasure event and scrubbed
  historical profile events. A replacement read projection can therefore be
  rebuilt from an empty database without a prior read-side profile row and
  cannot rehydrate contact or attribute values.

## Files and contracts

- Domain command and redacted projection:
  `crates/domain/src/marketing.rs`.
- Write/read schema changes:
  `crates/migration/migrations_tenant_write/0049_marketing_profile_erasure.sql`
  and `crates/migration/migrations_tenant_read/0039_marketing_profile_erasure.sql`.
- Authoritative persistence and projector:
  `crates/infrastructure/src/marketing_persistence.rs`.
- Disposable proof:
  `crates/infrastructure/tests/marketing_profile_erasure.rs`.

## Verification evidence

- `cargo fmt --all --check` — PASS.
- `cargo check -p erpx-infrastructure` — PASS.
- `cargo clippy -p erpx-infrastructure --all-targets -- -D warnings` — PASS.
- Static migration/privacy test — PASS.
- Disposable two-database live proof
  `profile_erasure_redacts_source_and_rebuildable_read_state` — PASS. It
  applied write migration 49 and read migration 39, verified idempotent
  erasure, source contact/attribute removal, permanent suppression, scrubbed
  source payload, empty-read projection rebuild, forced-RLS routing, and
  redacted read state.
- Direct post-test checks found no raw email in the write aggregate snapshot,
  no raw contact fields in the rebuilt read profile, and no disposable
  `erpx_mkt062_*` database residue after cleanup.

## Security and scope notes

The persistence path is privacy-authoritative and deliberately retains only
stable identity, scope, timestamps, reason, suppression, and audit metadata.
The command's actor authorization still belongs at the application/Tonic
boundary and must call `validate_for_scope`; this packet does not claim that
transport or UI work. Queued delivery payload scrubbing, public abuse controls,
SSR/UI, MinIO, remaining ERP contexts, migration cutover, and Folio write
retirement remain open.

## Integration order

1. Apply tenant write/read migrations 0049/0039.
2. Deploy domain and SQLx erasure/projector code.
3. Run the focused unit/static tests and disposable two-database proof.
4. Run the complete workspace, Dioxus, and Folio verification wave.
5. Add the application/transport authorization boundary before exposing the
   command to customer-facing workflows.

## Rollback

The schema changes are additive. Stop exposing new erasure commands if a
deployment must be halted; do not roll back by restoring erased values. The
redaction tombstone and permanent suppression remain authoritative and must be
preserved through any replacement read rebuild.
