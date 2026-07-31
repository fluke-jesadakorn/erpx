# G3-001 / G3-005 Customer 360 domain foundation

## Packet scope

This bounded packet adds the control-plane domain projection layer in
`crates/domain/src/customer.rs` without changing the existing `Customer360`,
`CustomerAccount`, or `CustomerHealth` serialized shapes. The enriched
projection is defaultable so older projection documents can be read while the
Customer 360 migration is staged.

Delivered types cover:

- trading/legal identity, registration and tax identifiers;
- industry, country, locale, billing address, MinIO logo references, and
  domains;
- account and subscription state;
- responsibility contacts and customer-success ownership;
- organization, commercial, onboarding, and expanded health projections;
- a redacted `Customer360Summary` for list/search surfaces; and
- `GlobalReadAuditRecord`, requiring actor, customer, module, scope, purpose,
  timestamp, and correlation ID.

Logo values are object references only. Signed URLs and credentials are not
part of the domain model.

## Evidence

- `cargo test -p erpx-domain`: **PASS** — 23 tests, including three focused
  enriched-projection/audit tests.
- `rustfmt --edition 2024 --check crates/domain/src/customer.rs`: **PASS**.
- `cargo fmt --all --check`: **BLOCKED by pre-existing unrelated formatting
  differences in `crates/domain/src/commercial.rs`**; that forbidden file was
  not changed in this packet.
- `cargo clippy -p erpx-domain --all-targets -- -D warnings`: **BLOCKED by
  three pre-existing diagnostics in `crates/domain/src/commercial.rs`**
  (`collapsible_if` twice and `too_many_arguments` once); no diagnostic points
  to the packet file.

## Security assertions

`Customer360Projection::summary()` excludes registration and tax identifiers,
billing address, domains, logos, responsibility contacts, Stripe references,
and lifetime value. The audit constructor rejects blank actor, module, scope,
or purpose values. Global-read audit persistence and customer-visible querying
remain application/infrastructure work.

## Integration order

1. Add application read models and serializers that consume
   `Customer360Projection`.
2. Persist/project the enriched payload in the control-plane read database.
3. Emit `GlobalReadAuditRecord` at the global-read gateway and expose it to the
   customer security audit view.
4. Add UI pages only after the read gateway and audit persistence are wired.

## Rollback

Remove the new projection/audit definitions and this evidence file. Existing
legacy customer domain types and their serialized fields remain unchanged.

## Unfinished work

No database, repository, transport, UI, or global-read gateway changes are in
scope for this packet. Sensitive projection fields must not be exposed through
summary/list endpoints when those adapters are implemented.
