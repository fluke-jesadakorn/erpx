# G10-MKT-057 — Authenticated Marketing form submission transport

Date: 2026-07-26

## Scope delivered

ERPX now exposes an authenticated Tonic `MarketingContentService.SubmitMarketingForm` command. The transport:

- requires the existing authenticated session, command context, holding scope, and authoritative entitlement/RBAC check;
- rejects platform and subtree scopes and validates holding, holding, form, and submission references;
- bounds the request body to 2 MiB and accepts only a JSON object whose values are strings;
- routes the write through the holding write/read database router and SQLx Marketing content store;
- preserves idempotent write receipts and the existing published-form, scope, and forced-RLS rules;
- records a customer-visible audit event without logging submitted values; and
- returns stable aggregate/version/idempotency/event metadata plus correlation identity.

The service is registered in the single Tonic server process. Persistence failures are mapped to redacted gRPC statuses carrying correlation metadata; database details are not exposed to callers.

## Evidence

- `cargo fmt --all --check` — PASS.
- `cargo check -p erpx-server` — PASS.
- `cargo clippy -p erpx-server --all-targets -- -D warnings` — PASS.
- `cargo test -p erpx-server --quiet` — PASS; 77 tests.
- `cargo clippy --workspace --all-targets -- -D warnings` — PASS.
- `cargo test --workspace --quiet` — PASS; 173 domain, 158 infrastructure with 4 ignored, 77 server, and 108 worker tests passed; worker has 1 ignored.
- `cargo check -p erpx-console --no-default-features --features web` — PASS.
- `cargo check -p erpx-console --no-default-features --features desktop` — PASS.
- `bun run lint` — PASS with four pre-existing warnings in `infra/scripts/start-n8n.js`, zero errors.
- `bunx tsc --noEmit` — PASS.
- Folio `/operations` — HTTP 307; development server remains on port 3004.
- `pg_isready -h /tmp -p 5432` — accepting connections.
- `git diff --check` — PASS.

## Explicitly open

This packet does not claim completion of the Marketing context. Anonymous/public HTTP submission and active-domain lookup are still open, as are preview issue/serve/authorize/revoke transport, domain verification/binding RPCs, executable worker registration/scheduling, an application-layer Marketing port, semantic typed-field validation, retention/erasure, live MinIO and signed-object integration, SSR/UI, vendor adapters and secret management, journeys/campaigns/CRM/attribution, the remaining ERP contexts, migration/cutover, and Folio write retirement.

No Folio behavior or Folio writes changed.
