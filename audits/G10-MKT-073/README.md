# G10-MKT-073 — Channel-scoped Marketing delivery claims

## Delivered

- Added a channel-filtered tenant delivery-store constructor.
- Applied the provider account's normalized channel capability list directly
  in the authoritative `claim_due` SQL before `FOR UPDATE SKIP LOCKED` leases
  are created.
- Wired the multi-tenant worker runtime to build each account source with its
  control-plane channel set.
- Preserved the unfiltered repository constructor for explicitly composed
  single-source tests and non-provider callers; production multi-account
  delivery uses the filtered constructor.
- Canonicalized and bounded filters to email, SMS, social, push, and webhook.

## Evidence

- Infrastructure channel-filter unit coverage: 3 passed.
- Disposable two-database tenant proof: 1 passed. Fresh tenant migrations
  seeded queued email and SMS dispatches plus valid consent; an email-scoped
  source claimed only email, and an SMS-scoped source claimed only SMS.
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

This packet does not claim provider priority policy for multiple accounts with
the same channel, vendor-native SDKs, live external-provider evidence,
complete Marketing/CRM, the remaining ERP modules, migration cutover, or Folio
write retirement.
