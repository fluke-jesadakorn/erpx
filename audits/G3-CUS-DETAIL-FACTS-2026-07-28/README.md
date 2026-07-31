# G3 Customer 360 bounded detail facts — 2026-07-28

## Scope

`GetCustomer360Detail` previously returned section states and notes but no
displayable projection facts. The console therefore could not show the
authoritative Customer 360 detail even when the existing control read
projection was present.

## Delivered behavior

- Added the additive `Customer360SectionFact` protobuf message and
  `section_facts` response field.
- The server derives facts only from the already-authorized
  `Customer360DetailReadModel`; no read model, write aggregate, migration, or
  client-supplied payload becomes authoritative.
- Facts are explicit scalar allow-lists for account, organization, contacts,
  commercial, entitlements, onboarding, health, support, access-audit, and
  notes. Sensitive provider references, object keys, credentials, raw JSON,
  contact contents, and audit payloads remain outside the client contract.
- The console validates section/key allow-lists, duplicate identity, bounds,
  control characters, and the supported/present fact requirement before
  rendering a read-only metric grid.
- Existing notes pagination, freshness, authorization, audit, and
  read-only semantics remain unchanged.

## Persistence and rollback

This is a read-transport/UI completion over the existing authoritative SQLx
Customer 360 projection and its existing worker/reconciliation path. No
database write occurred. Reverting the additive proto field plus the server
and console adapters is safe; persisted source and projection state is
unchanged, and older clients ignore the additive response field.

## Verification

- `cargo fmt --all --check` — PASS.
- `cargo check -p erpx-proto --offline` — PASS.
- `cargo test -p erpx-proto --offline` — PASS.
- `cargo check -p erpx-server --offline` — PASS.
- `cargo check -p erpx-console --offline --features web` — PASS.
- `cargo test -p erpx-console --offline --features web --no-fail-fast` — PASS,
  112 tests.
- `cargo clippy -p erpx-server -p erpx-console --all-targets --offline -- -D warnings`
  — PASS.

No directory activation, command-owner switch, Folio write retirement, or
production cutover occurred.
