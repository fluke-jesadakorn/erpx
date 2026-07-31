# G10-MKT-071 — Marketing provider onboarding and channel capability lifecycle

Status: integrated and independently verified on 2026-07-26.

## Delivered behavior

- Added control migration `0049_marketing_provider_onboarding.sql`.
- Provider accounts now carry a bounded allow-list of Marketing channels,
  `pending`/`verified` onboarding state, verification timestamp, and a
  hash-only verification reference.
- New configuration always starts `suspended` and `pending`; changing a
  credential reference or channel set invalidates prior verification.
- Verification is a separate authenticated command. Activation is rejected
  until the account is verified; state changes use optimistic versions.
- Suspension removes the account from callback and outbound resolution.
- Tonic exposes configure, acknowledge-verification, and state commands under
  `MarketingContentService`.
- Every mutation is idempotent and commits the account row, outbox event,
  audit event, and receipt atomically in the control write database.
- Provider delivery validates the shared secret-reference contract and
  rejects dispatches outside the account's onboarded channel set.
- Event payloads and receipts contain no secret values or credential
  references. Only opaque secret-manager locators are accepted at the command
  boundary and persisted in the routing row.

## Ownership and boundaries

- Control-plane PostgreSQL is authoritative for provider account routing and
  onboarding state.
- Tenant databases are not touched by onboarding commands.
- The worker still resolves active accounts from control state and uses the
  existing provider-neutral HTTP adapter. Vendor-native SDKs, external
  provider verification, and live provider evidence remain open work.

## Evidence

- Application onboarding unit test: 1 PASS.
- Worker provider suite: 11 PASS.
- Disposable two-database PostgreSQL test
  `provider_onboarding_is_idempotent_verified_before_activation_and_audited`:
  1 PASS. It proved replay, verification-gated activation, suspension,
  stale-version rejection, reactivation, outbox/audit counts, and absence of
  credential references from event payloads.
- `cargo fmt --all --check`: PASS.
- `cargo clippy --workspace --all-targets -- -D warnings`: PASS.
- `cargo test --workspace`: PASS; only pre-existing explicitly ignored
  disposable-database tests remained ignored.
- Dioxus web and desktop checks: PASS.
- `bun run lint`: PASS with four pre-existing warnings in
  `infra/scripts/start-n8n.js`, zero errors.
- `bunx tsc --noEmit`: PASS.
- Folio `/operations`: HTTP 307; Folio dev server remains on port 3004.
- PostgreSQL readiness: accepting connections.
- `git diff --check`: PASS.

## Remaining scope

This packet does not claim vendor-native provider SDKs, live external provider
verification, channel delivery completion, the wider G10 gate, remaining ERP
contexts, migration/cutover, or Folio write retirement.
