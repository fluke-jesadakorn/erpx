# G1-005 — Authenticated dead-letter replay

Status: local boundary complete; external and production certification remain queued.

## Authoritative path

- `crates/proto/proto/erpx/platform/v1/platform.proto` defines the versioned
  `ReplayDeadLetterOutbox` command and safe receipt. The MFA code is a
  transport-only field.
- `apps/server/src/main.rs` requires an active access token/session, exact
  actor attribution, platform scope, the directory-managed
  `platform.outbox.replay` permission, and fresh one-time TOTP verification.
- `crates/infrastructure/src/outbox.rs` commits idempotency reservation,
  dead-letter eligibility, outbox requeue, stale error-context clearing, and
  append-only audit insertion in one SQLx transaction. The audit payload is
  metadata-only and excludes the event payload, worker failure details, and
  MFA value.
- `apps/console/src/main.rs` provides the guarded Platform Operations form;
  it clears the MFA input after submission and renders only safe receipt
  metadata.

## Evidence

- Fresh disposable PostgreSQL acceptance:
  `audited_dead_letter_replay_is_atomic_idempotent_and_redacted` — PASS 1/1.
  It proves requeue state, claim/error cleanup, duplicate idempotency, one
  audit row, and payload redaction.
- `cargo fmt --all --check` — PASS.
- Workspace Clippy with warnings denied — PASS.
- Serialized workspace tests and doc-tests — PASS.
- G13 compatibility 9/9 and deployment recovery 4/4 — PASS.
- Dioxus web/desktop/mobile/iOS/Android checks — PASS.

## Queued evidence

Credentialed Generic LDAP/RFC4533, AD DS/DirSync, and UCS certification;
measured one-million-user imports and five-minute external revocation;
approved protobuf descriptor comparison; physical mobile execution; live
Folio migration/rollback/retention/post-cutover evidence; MinIO round-trip;
and explicit production approval. No directory activation, command-owner
switch, Folio write retirement, or cutover was performed.
