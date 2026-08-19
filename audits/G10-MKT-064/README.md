# G10-MKT-064 — Anonymous Marketing abuse controls

## Delivered behavior

- The domain crate exports a pure, deterministic abuse policy with exact
  holding/form/domain binding, bounded opaque fingerprints, sustained and burst
  limits, honeypot rejection, explicit challenge decisions, and redacted
  `Debug`/error output.
- Public Marketing form and preview ingress derives a keyed HMAC fingerprint
  from the configured ingress key and bounded proxy/client headers. The raw
  header values and fingerprint source are never persisted.
- The ingress validates the active holding registration, hostname, resource
  kind, and opaque resource UUID before consuming the quota. Missing or empty
  `ERPX_MARKETING_ABUSE_FINGERPRINT_KEY` fails closed with a generic service
  error.
- Control-plane migration 0047 adds an atomic minute-window counter keyed by
  holding, holding, normalized hostname, resource kind, resource ID, and window
  start. It stores no IP address, email, contact value, token, or request body.
- The atomic control-plane counter is consumed before holding database routing;
  an exhausted window returns `429 Too Many Requests`. The holding registry is
  rechecked inside the same control-plane transaction.
- Form honeypot fields are rejected before holding writes. Explicit challenge
  adapter signals remain bounded to deterministic reject/challenge responses;
  a passed challenge never bypasses durable limits.

## Files and contracts

- Pure policy: `crates/domain/src/marketing_abuse.rs`.
- Durable migration and control repository: `crates/migration/migrations/0047_marketing_public_abuse.sql`
  and `crates/infrastructure/src/repositories.rs`.
- Public HTTP ingress: `apps/server/src/main.rs`.
- Static and disposable PostgreSQL proof:
  `crates/infrastructure/tests/marketing_public_abuse.rs`.

## Verification evidence

- Domain abuse policy: 7 focused tests and strict Clippy — PASS.
- Server abuse signal/binding tests: 2 focused tests; server check and strict
  Clippy — PASS.
- Infrastructure migration/static test: PASS.
- Fresh disposable PostgreSQL control-plane proof: PASS; atomic limits accepted
  two requests, rejected the third, opened a new minute window, and the
  disposable database was dropped after the run.
- Full wave: workspace format, strict Clippy, workspace tests, Dioxus web and
  desktop checks, Folio lint/typecheck, `/operations` HTTP 307, PostgreSQL
  readiness, and diff checks — PASS. Folio lint retained four pre-existing
  warnings and zero errors.

## Scope notes

This packet establishes the policy and durable coarse resource fence. A
production trusted-proxy configuration, external challenge provider, and
scheduled worker purge/reconciliation remain follow-up integration work. The
control counter is intentionally not an identity or contact-tracking store.

## Rollback

Disable the public Marketing ingress route while preserving the additive
control-plane table and policy code. Do not delete counter history outside its
bounded retention cleanup; holding write state and existing form idempotency
records remain authoritative.
