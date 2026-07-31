# G10-MKT-074 — Deterministic multi-account channel selection

## Delivered

- Added control migration `0051_marketing_provider_account_priority.sql` with
  bounded priority values from 1 through 10,000.
- Added provider priority to the onboarding command, SQLx account lifecycle,
  safe receipt, and Tonic configuration transport. Zero on the wire uses the
  documented default of 100 for compatibility.
- Ordered active account discovery by ascending priority and then opaque
  provider-account reference, preserving deterministic tie-breaking.
- Included priority in safe account bindings and validated it at the worker
  boundary without exposing credentials.

## Evidence

- Fresh two-database control-plane onboarding proof passed. It configured and
  activated priority-10 and priority-100 accounts, then verified the ordered
  active-account binding list and route metadata.
- Existing idempotency, optimistic concurrency, suspension, reactivation,
  audit, and credential-redaction assertions remained passing.
- `cargo fmt --all --check`: passed.
- `cargo clippy --workspace --all-targets -- -D warnings`: passed.
- `cargo test --workspace`: passed; explicitly ignored disposable database
  tests remained ignored.
- `cargo check -p erpx-console --no-default-features --features web`: passed.
- `cargo check -p erpx-console --no-default-features --features desktop`: passed.
- `bun run lint`: passed with four pre-existing warnings in
  `infra/scripts/start-n8n.js` and zero errors.
- `bunx tsc --noEmit`: passed.
- PostgreSQL readiness passed and Folio `/operations` returned HTTP 307.

## Explicit non-claims

This packet does not claim live provider failover, weighted traffic policy,
vendor-native SDKs, complete Marketing/CRM, the remaining ERP modules,
migration cutover, or Folio write retirement.
