# G10-MKT-059 — Marketing administration transport and application port

Date: 2026-07-26

## Scope delivered

The authenticated Tonic Marketing content service now exposes administrative commands for issuing and revoking expiring previews, issuing a random 256-bit domain challenge and storing only its SHA-256 digest, verifying a supplied domain proof by digest, and binding a verified domain to an explicitly scoped template revision.

Every command uses the existing authentication interceptor, command-context conversion, explicit holding scope, entitlement/RBAC authorization, server-side holding write/read routing, SQLx persistence, customer-visible audit events, and redacted correlation-linked errors. Preview tokens and domain challenges are returned only at issuance and never cross the persistence boundary in raw form. The protobuf service remains additive and backward compatible.

An application-layer `MarketingContentWritePort` and transport-neutral command/receipt DTOs now define the orchestration seam for form submission, preview lifecycle, domain verification, and domain binding. The port validates shape, context, scope, bounded values, and digest format while leaving authoritative authorization, re-reads, routing, transactionality, idempotency, audit, and outbox behavior to the adapter.

## Evidence

- `cargo fmt --all --check` — PASS.
- `cargo clippy -p erpx-application --all-targets -- -D warnings` — PASS.
- `cargo test -p erpx-application --quiet` — PASS; 11 tests.
- `cargo clippy -p erpx-server --all-targets -- -D warnings` — PASS.
- `cargo test -p erpx-server --quiet` — PASS; 79 tests.
- `cargo check -p erpx-proto` — PASS.
- Secret-boundary unit proof — PASS; an issued secret is exactly 32 random bytes represented as 64 hex characters and its persistence value is a 64-character digest.

## Explicitly open

The SQLx store is not yet implemented behind the new application port; the current server adapter still calls the store directly. A live Tonic authorization/transport proof for all five new commands, public rate limiting/abuse controls, semantic form-field validation, SSR/Dioxus UI, MinIO rendering, host-only control-plane domain discovery, durable retry/dead-letter persistence, campaigns/journeys/CRM attribution, remaining ERP contexts, migration/cutover, and Folio write retirement remain open.

No Folio behavior or Folio writes changed.
