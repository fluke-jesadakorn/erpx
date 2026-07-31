# G4-002 — Support attachments and SLA notification policy

## Scope

This packet adds transport-neutral write-side contracts in
`crates/domain/src/support.rs` for:

- bounded ticket/customer-bound attachment metadata;
- opaque, path-safe object references without signed URLs or credentials;
- content type, size, canonical SHA-256, visibility, uploader, tenant scope,
  upload time, and expiry;
- maximum attachment count and size;
- customer-visible projection that excludes internal and expired attachments;
- redacted diagnostic attachment metadata with the object reference omitted;
- bounded SLA warning/breach policy;
- deterministic audience-specific notification keys;
- ticket-terminal and notification-expiry delivery checks.

No transport, worker, migration, UI, plan, ledger, or Folio files were changed.
No SQLx persistence implementation was added before the requested stop point;
the next integration packet must place attachment arrays in the existing
ticket JSON payload through the established atomic ticket update seam.

## Exact verification

From `erpx/`:

```text
rustfmt --edition 2024 --check crates/domain/src/support.rs — PASS
cargo test -p erpx-domain support_attachment --lib — PASS (2 passed)
cargo test -p erpx-domain support_sla_notifications --lib — PASS (1 passed)
cargo clippy -p erpx-domain --lib -- -D warnings — PASS
cargo test -p erpx-domain --lib — PASS (80 passed, 0 failed)
```

The changed domain file was formatted with `rustfmt --edition 2024` before the
checks above. No infrastructure test was run because the infrastructure
persistence packet was intentionally not started.

## Residual work

- Add the `SupportPersistence` atomic attachment append/read seam, preserving
  the JSON sidecar across all existing ticket updates and enforcing optimistic
  version plus idempotency replay.
- Add disposable PostgreSQL coverage for append, duplicate replay, request-hash
  conflict, closed-ticket rejection, count limits, and cross-customer binding.
- Compose authenticated support transport and object-storage signed URL
  issuance; never expose raw credentials or unsigned object access.
- Compose worker scheduling, durable notification inbox/outbox, retries,
  suppression after ticket close, and expiry sweeps.
- Add customer/platform UI projections and notification delivery adapters.
- Integrate attachment events/audit records into the broader G4 evidence and
  independently verify global support authorization and tenant isolation.

## Security concerns

- The domain contract rejects traversal segments, URL-like references,
  controls, whitespace, invalid MIME tokens, invalid hashes, oversize content,
  invalid scope, invalid expiry, duplicate references, and binding mismatch.
- Internal attachments are excluded from the customer projection by an
  explicit visibility value; callers must not serialize the durable aggregate
  directly as a customer response.
- Notification values contain no message body or secret and are bound to both
  ticket and customer IDs. Delivery must re-check ticket status and expiry at
  the authenticated worker boundary.
- Signed upload/download URLs, object existence checks, malware scanning, and
  authorization are intentionally not claimed by this packet.

## Integration order

1. Add the SQLx attachment sidecar append/read implementation through the
   existing ticket transaction and idempotency reservation.
2. Preserve the sidecar during existing ticket update/command persistence.
3. Add worker notification claiming and expiry/close suppression.
4. Add authenticated transport and object-access issuance.
5. Add customer/platform projections and end-to-end security tests.

## Rollback

The domain additions are additive and have no schema or runtime side effects.
Rollback is a source revert of the packet files. Before persistence is
integrated, no database cleanup is required. After the future JSON sidecar
integration, rollback must first stop attachment writes, retain the opaque
objects under their retention policy, and remove only the sidecar through a
versioned migration or a separately reviewed repair job; do not delete MinIO
objects as part of application rollback.
