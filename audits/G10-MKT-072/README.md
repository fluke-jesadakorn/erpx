# G10-MKT-072 — Provider account-bound channel routes

## Delivered

- Added control migration `0050_marketing_provider_channel_routes.sql` with a
  bounded JSON route array on each Marketing provider account.
- Added the transport-neutral `MarketingProviderChannelRoute` contract and
  required a unique route for every configured channel.
- Added server transport mapping for provider-neutral endpoint and optional
  reconciliation endpoint metadata.
- Added SQLx persistence and binding decoding for routes. Route values are
  validated as credential-free HTTPS URLs; query strings, fragments, userinfo,
  and non-HTTPS schemes fail closed.
- Updated the worker account-bound HTTP adapter to construct routes from the
  authoritative control-plane binding. The deployment environment remains
  responsible only for the provider-key equality check and secret-manager
  resolution; it no longer supplies per-account endpoint URLs.
- Kept receipts and outbox payloads free of route credential references and
  secret values. Account binding debug output exposes only route channel names.

## Evidence

- Application route contract test: 1 passed.
- Worker Marketing provider suite: 12 passed, including account-bound route
  construction.
- Disposable PostgreSQL two-database onboarding proof: 1 passed after fresh
  control migrations, including route persistence, replay, verification,
  activation, suspension, stale-version rejection, audit, and credential
  redaction.
- `cargo fmt --all --check`: passed.
- `cargo clippy --workspace --all-targets -- -D warnings`: passed.
- `cargo test --workspace`: passed; explicitly ignored disposable database
  tests remained ignored.
- `cargo check -p erpx-console --no-default-features --features web`: passed.
- `cargo check -p erpx-console --no-default-features --features desktop`: passed.
- `bun run lint`: passed with four pre-existing warnings in
  `infra/scripts/start-n8n.js` and zero errors.
- `bunx tsc --noEmit`: passed.
- `git diff --check`: passed.
- PostgreSQL readiness passed and Folio `/operations` returned HTTP 307 while
  the development server remained on port 3004.

## Explicit non-claims

This packet does not claim vendor-native SDKs, live external-provider
verification, production secret-manager deployment, complete Marketing/CRM,
the remaining ERP modules, migration cutover, or Folio write retirement.
