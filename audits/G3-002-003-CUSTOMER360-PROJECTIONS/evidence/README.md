# G3-002-003 Customer 360 projections

## Delivered behavior

- Legacy `Customer360` events remain accepted and are converted into the
  enriched `Customer360Projection` without inventing organization IDs.
- Enriched projections canonicalize unordered domains, contacts, organization
  units, modules, invoices, quotes, and discounts before read persistence.
- Organization bindings are derived only from active, structurally connected
  holding → company → branch units. Missing or foreign parents produce no
  authority binding.
- Holding, company, and branch bindings are deterministically sorted and
  deduplicated.
- Enriched Customer 360 payloads are stored on the control read database.
- Derived `customer_scope_bindings` are inserted on the control write database;
  read counts and payloads are never used as authorization authority.
- Existing list/detail compatibility returns the legacy `Customer360` shape
  from enriched read payloads.
- Customer summaries continue to exclude tax/registration data, billing
  addresses, domains, logos, contacts, provider references, and financial
  totals.
- Projection rebuilds are idempotent and produce byte-stable payloads.

## Files changed

- `crates/domain/src/customer.rs`
- `apps/worker/src/lib.rs`
- `apps/worker/tests/customer_360.rs`
- `crates/infrastructure/src/repositories.rs`
- `audits/G3-002-003-CUSTOMER360-PROJECTIONS/evidence/README.md`

## Contracts and migrations

- No protobuf or shared transport contract changes.
- Migration `0008_customer_scope_bindings.sql` was consumed unchanged.
- No source transaction tables or destructive operations were added.

## Verification

- Targeted rustfmt checks for all four Rust files — PASS.
- `cargo test -p erpx-domain -p erpx-infrastructure -p erpx-worker --lib` — PASS: 37 domain, 15 infrastructure, and 1 worker tests; 2 infrastructure tests remain intentionally ignored for disposable PostgreSQL.
- `cargo test -p erpx-worker --tests --no-run` — PASS.
- `cargo clippy -p erpx-domain -p erpx-infrastructure -p erpx-worker --all-targets -- -D warnings` — PASS.
- Clean two-database ignored worker test — PASS. It verified enriched read payloads, write-only bindings, holding/customer isolation, summary redaction, and a byte-stable second rebuild.

## Open work

- Legacy customer events cannot produce scope bindings because they contain
  counts but no holding/company/branch IDs. Organization source events or a
  migration backfill must provide those IDs.
- Binding removal/revocation when an organization unit is deactivated remains a
  write-side lifecycle packet; this packet intentionally performs additive
  upserts only and does not delete authority rows.
- Detailed enriched Customer 360 query APIs and platform pages remain later
  work; compatibility list/detail APIs still return the legacy shape.

## Security

- No credentials, storage namespaces, signed URLs, payment instruments, or
  secrets are copied into scope bindings.
- A holding binding intentionally authorizes descendants under the existing
  `customer_scope_contains` wildcard semantics. Isolation tests therefore use
  an unbound holding/customer boundary.
- Structurally disconnected or inactive organization units are fail-closed and
  do not grant access.

## Integration order

1. Emit organization source events with authoritative IDs.
2. Reconcile additive bindings against organization deactivation/revocation
   workflows on the write side.
3. Expose enriched Customer 360 detail queries without widening summaries.
4. Add migration backfill/reconciliation evidence for all holdings.

## Rollback

Remove only the projection conversion, read-persistence, binding-upsert, and
focused test changes. Raw control-plane snapshots and outbox events remain
untouched; existing legacy read compatibility can be restored independently.
