# G3-006 Customer 360 detail/read model — handoff

## Status

Implementation was stopped at a coherent, reviewable point at the user's
request. The packet adds a transport-neutral typed detail aggregate and a
customer-specific SQLx read seam, but it is not independently verified or
ready for integration.

## Changed files

- `erpx/crates/domain/src/customer.rs`
  - `Customer360DetailReadContext` with authenticated global-read context
    validation and fail-closed customer scope binding checks.
  - Typed `Customer360DetailReadModel` covering organization, contacts,
    commercial, entitlements, onboarding, health, support, access-audit, and
    notes.
  - Section completeness/read metadata preservation, absent/unsupported
    section states, deterministic notes cursor/page types, and bounded page
    sizing.
  - Detail-read free-text redaction for common bearer/basic credentials,
    provider secrets, API keys, passwords, and token forms.
  - Domain unit tests for scope binding, audit-context validation, section
    completeness, missing-section behavior, paging, and redaction.
- `erpx/crates/infrastructure/src/repositories.rs`
  - Customer-specific `SqlxControlPlane::get_customer_360_detail` read seam.
  - Authoritative write-pool customer-scope check before read-pool detail
    assembly.
  - Deterministic, bounded, freshness-aware notes page query with body
    redaction.
  - Existing notes read now uses the bounded/redacted customer notes seam.

No migration, protobuf, server, UI, worker, support, commercial, operations,
master-plan, or execution-ledger files were changed.

## Verification

Commands were run from `/Users/fluke/Desktop/Work/folio/erpx` after repairing
the packet's generic section construction, scope match, and test ownership:

```text
`cargo fmt --all --check` — PASS.

`cargo test -p erpx-domain --lib` — PASS; 73 tests.

`cargo test -p erpx-infrastructure --lib` — PASS; 54 tests, 4 disposable-DB
tests explicitly ignored.

`cargo clippy -p erpx-domain -p erpx-application -p erpx-infrastructure
-p erpx-worker --all-targets -- -D warnings` — PASS.

The original interrupted handoff results are retained only as history; they
are superseded by this independent integrated verification.
```

No focused test or clippy result is claimed for this interrupted packet.

## Unfinished work

- Format the two changed Rust files.
- Repair or receive the unrelated `access.rs` parse fix, then run focused
  domain and infrastructure tests and clippy with `-D warnings`.
- Add a disposable PostgreSQL test for the new composite detail read, scope
  denial, notes pagination, freshness, and redaction behavior.
- Decide whether the new repository method should be promoted through the
  application port and then wired into authenticated server/protobuf
  transport; this packet deliberately did not touch those forbidden paths.
- Record audit events through the existing authenticated global-read
  application contract at the eventual transport boundary.
- Validate notes pagination against the live read schema and verify
  read/write pool separation in a disposable two-database run.

## Security concerns

- The new repository method checks customer scope against the authoritative
  control write pool, but it does not itself record the global-read audit; the
  caller must do so through the authenticated application boundary.
- Platform scope is intentionally accepted only when the caller supplies a
  validated global-read context. Platform role/entitlement authorization is
  outside this customer-specific read seam.
- The redactor is deterministic and defensive for common credential/token
  forms, but it is not a substitute for structured secret classification or
  server-side response policy review.
- No signed object URLs, holding credentials, storage keys, or raw audit
  payloads are added to the detail model.

## Integration order

1. Resolve the unrelated `access.rs` parse failure and format this packet.
2. Run domain tests and clippy for the changed customer contracts.
3. Run the disposable two-database infrastructure test, including an
   unauthorized scope and notes-cursor replay case.
4. Independently review the detail model and redaction output.
5. Promote the typed read seam through application/server/protobuf only after
   authenticated global-read auditing and transport authorization are wired.

## Rollback

Remove the two customer Rust changes and this evidence directory. No database
migration or external runtime state was changed by this packet.
