# G3-005-AUDIT-DETAIL-004 evidence

## Delivered behavior

- Added a transport-independent application query contract for customer-security-facing global-read detail.
- `CustomerGlobalReadAuditDetailQuery` requires a non-empty requester, purpose, correlation ID, customer ID, and an explicit `CustomerSecurityAdminScope` whose customer matches the query.
- `CustomerGlobalReadAuditDetailService` records the detailed read through the existing `AuditSink` before invoking the read port, preserving the existing `GetCustomerProjection` audit pattern.
- Added a typed read-only `CustomerGlobalReadAuditDetailStore` port and `SqlxControlPlane` implementation.
- The repository reads the customer projection from the control read pool and authoritative global-read audit rows from the control write pool. It returns `CustomerReadView` metadata, including projection freshness and `read_only` state.
- Detail pagination is deterministic by `(occurred_at DESC, id DESC)`, uses a bounded maximum page size of 100, and probes one extra row so the final page has no false cursor.
- The detail DTO selects only attribution fields: audit ID, actor/session reference, customer, module, scope, purpose, occurrence time, and correlation ID. The persisted `payload` is never selected or serialized.
- No notes mutation or note source was added; notes remain explicitly dependent on an authoritative note aggregate.

## Files changed

- `crates/domain/src/customer.rs`
- `crates/application/src/lib.rs`
- `crates/infrastructure/src/repositories.rs`
- `apps/worker/tests/customer_access_audit.rs` (physical integration coverage)
- `audits/G3-005-AUDIT-DETAIL-004/evidence/README.md`

No protobuf, migration, server, UI, identity, commercial, or Folio files were changed. No schema change was required.

## Contracts and transport hook

The main agent's transport adapter should authenticate the customer security administrator, authorize the requested `CustomerSecurityAdminScope`, construct `CustomerGlobalReadAuditDetailQuery`, and call `CustomerGlobalReadAuditDetailService::list`. The repository deliberately performs no authorization decision. An additive protobuf RPC was not added because server/main transport integration is reserved for the main agent; the application port and DTO are the compatibility seam.

## Verification

- `rustfmt --edition 2024 --check crates/domain/src/customer.rs crates/application/src/lib.rs crates/infrastructure/src/repositories.rs apps/worker/src/lib.rs apps/worker/tests/customer_access_audit.rs` — passed.
- `cargo test -p erpx-domain --lib` — 43 passed, 0 failed.
- `cargo test -p erpx-application --lib` — 6 passed, 0 failed.
- `cargo test -p erpx-infrastructure --lib` — 26 passed, 0 failed, 3 ignored (disposable database tests).
- `cargo test -p erpx-worker --lib` — 5 passed, 0 failed, 1 ignored.
- `cargo test -p erpx-worker --tests --no-run` — passed.
- `cargo clippy -p erpx-domain -p erpx-application -p erpx-infrastructure -p erpx-worker --all-targets -- -D warnings` — passed.
- `cargo check -p erpx-proto` — passed; protobuf sources were unchanged.
- Disposable physical test with separate control write/read databases:
  `ERPX_TEST_CONTROL_WRITE_DATABASE_URL='postgresql:///erpx_g3005006_write_20260724' ERPX_TEST_CONTROL_READ_DATABASE_URL='postgresql:///erpx_g3005006_read_20260724' cargo test -p erpx-worker --test customer_access_audit -- --ignored --nocapture` — 1 passed, 0 failed. Both temporary databases were dropped after the run.

## Security and isolation review

- Scope is explicit in the application contract, while authorization remains at the authoritative transport/application boundary.
- The source query is customer-keyed and checks the row customer ID before constructing DTOs.
- Read projection access uses the read pool; authoritative audit detail uses the write pool; no tenant credentials or secrets cross the DTO boundary.
- Raw `global_read_audits.payload` is excluded from SQL selection and serialization. The service audit itself contains only actor, customer, action, purpose, and correlation metadata.
- The repository is read-only with respect to global-read source rows and notes.

## Unfinished work and integration order

- Main agent: wire the authenticated customer-security-admin transport/RBAC check and additive RPC/HTTP surface if desired.
- Main agent: expose the `CustomerReadView` freshness, absent, unsupported, and read-only states in the client.
- Future support/domain work: provide an authoritative note aggregate before implementing notes mutation or a notes projector.
- Integrate after customer read models and `AuditSink` wiring are available, before the Customer 360 access-audit UI.

## Rollback

Remove only the added detail DTO, application port/service, SQLx read method, focused test additions, and this evidence file. Existing Customer 360 projections, access-audit summary projection, write-side global-read audit records, and notes behavior remain unchanged.
