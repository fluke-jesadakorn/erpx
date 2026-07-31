# G10-MKT-070 — Marketing provider idempotency reconciliation

## Delivered behavior

Provider routes may now declare a separate bounded HTTPS reconciliation
endpoint. When a provider submission is ambiguous, the account-bound adapter
queries that endpoint with only the dispatch ID, idempotency key, channel, and
non-secret account context; it never resends the dispatch record. Redirects,
unsafe URLs, oversized responses, malformed responses, and unbounded status
values fail closed. `accepted`, `pending`, `not_found`, `retryable`, and
`permanent` are the only meaningful protocol states, and accepted responses
require a bounded provider message reference.

## Evidence

- Focused provider suite — 11 PASS, including local reconciliation HTTP proof.
- `cargo fmt --all --check` — PASS.
- `cargo clippy --workspace --all-targets -- -D warnings` — PASS.
- `cargo test --workspace` — PASS; all workspace targets passed, with only
  existing explicitly ignored disposable-database tests remaining ignored.
- Worker library target — 129 PASS and 1 existing ignored database test;
  worker binary target — 1 PASS.
- `cargo check -p erpx-server` — PASS.
- Dioxus web and desktop checks — PASS.
- `bun run lint` — PASS with four pre-existing warnings in
  `infra/scripts/start-n8n.js`, zero errors.
- `bunx tsc --noEmit` — PASS.
- Folio `/operations` — HTTP 307 PASS; development server remains on port
  3004.
- PostgreSQL readiness and `git diff --check` — PASS.

## Scope and remaining work

This packet closes the provider-neutral reconciliation boundary needed to
avoid duplicate submissions after ambiguous outcomes. Vendor-native provider
SDKs, account onboarding, live external-provider TLS/e2e evidence, channel
delivery completion, the broader Marketing/ERP gates, migration/cutover, and
Folio write retirement remain open.
