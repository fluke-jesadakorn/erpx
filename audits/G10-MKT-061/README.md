# G10-MKT-061 — Marketing form semantics and privacy retention

## Delivered behavior

- Form submissions now validate field semantics at the domain boundary:
  deterministic email syntax, canonical international phone digits, bounded
  finite plain decimals, exact lowercase `true` consent, required blank-value
  rejection, and non-sensitive validation errors.
- Tenant-write submissions receive a bounded retention deadline. A holding-
  scoped privacy sweep irreversibly replaces expired `values_json` with `{}`,
  records `redacted_at`, and appends an attributed
  `marketing.content_form_submission_redacted.v1` event without submitted
  values.
- The replacement read rebuild copies retention/redaction metadata and the
  already-redacted value state. Rebuilding therefore cannot restore erased
  form values.
- The executable worker registers `marketing_content_privacy`, routes only
  tenant write state, validates a system-worker holding context and bounded
  batch, closes its route, and maps persistence failures to a generic error.

## Files and contracts

- Domain semantics: `crates/domain/src/marketing_content.rs`.
- Write privacy migration: `crates/migration/migrations_tenant_write/0048_marketing_content_privacy.sql`.
- Read privacy migration: `crates/migration/migrations_tenant_read/0038_marketing_content_privacy.sql`.
- Authoritative write/rebuild behavior: `crates/infrastructure/src/marketing_content_persistence.rs`.
- Worker runner: `apps/worker/src/marketing_content_privacy.rs`.
- Worker composition: `apps/worker/src/lib.rs` and `apps/worker/src/main.rs`.
- Live proof: `crates/infrastructure/tests/marketing_content_persistence.rs`.

## Verification evidence

- `cargo fmt --all --check` — PASS.
- `cargo clippy --workspace --all-targets -- -D warnings` — PASS.
- `cargo test --workspace --quiet` — PASS; domain targets passed, infrastructure
  161 passed with 4 ignored, server 79 passed, and worker 113 passed with 1
  ignored.
- `cargo test -p erpx-worker marketing_content_privacy --quiet` — PASS; 5
  focused tests.
- `cargo clippy -p erpx-worker --all-targets -- -D warnings` — PASS.
- `cargo test -p erpx-domain --quiet` — PASS; 178, 6, and 9 tests passed in
  the domain targets.
- `cargo check -p erpx-console --no-default-features --features web` — PASS.
- `cargo check -p erpx-console --no-default-features --features desktop` — PASS.
- Disposable two-database live test
  `content_lifecycle_is_atomic_idempotent_versioned_and_rebuildable` — PASS.
  It applied tenant write migration 48 and read migration 38, verified source
  redaction, non-sensitive outbox payload, idempotent empty replay, read-model
  rebuild redaction, forced RLS, and exact cleanup. Databases
  `erpx_mkt061_w_20260726_a` and `erpx_mkt061_r_20260726_a` were dropped; the
  residue query returned no rows.
- `bun run lint` — PASS with four pre-existing warnings in
  `infra/scripts/start-n8n.js`, zero errors.
- `bunx tsc --noEmit` — PASS.
- Folio `/operations` — HTTP 307 PASS; development server remains on port 3004.
- PostgreSQL readiness — PASS.
- `git diff --check` — PASS.

## Security and scope notes

The sweep retains submission identity, timestamps, scope, and audit/outbox
facts while deleting submitted values from both source and rebuildable read
state. It does not yet claim the complete G10 Marketing privacy program:
profile-level erasure, consent lifecycle, suppression, delivery enforcement,
SSR/UI, MinIO, remaining ERP contexts, migration cutover, and Folio write
retirement remain open.

## Integration order

1. Apply tenant write/read migrations.
2. Deploy infrastructure write/rebuild code.
3. Deploy worker module and executable registration.
4. Run focused domain/worker tests and the disposable two-database proof.
5. Run the complete workspace, Dioxus, and Folio verification wave.

## Rollback

The migration is additive. Stop registering the privacy worker to pause new
sweeps; existing redaction events remain authoritative and are not reversed.
Rolling back code must not restore or infer erased values from audit/outbox
facts.
