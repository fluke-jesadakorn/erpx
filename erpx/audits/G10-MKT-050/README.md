# G10-MKT-050 — Account-bound Marketing provider delivery

## Delivered behavior

- Worker provider routes validate exact HTTPS endpoints without embedded
  credentials, query strings, fragments, or unsafe redirects.
- Account-bound requests carry only non-secret tenant, holding, provider
  account, and binding-version metadata. Credential bytes and verifier
  references never cross the provider request or error boundary.
- Delivery authorization must match the dispatch ID and idempotency key; the
  idempotency key is sent both as provider request metadata and in the bounded
  envelope.
- The account-bound wrapper re-resolves and compares the exact active
  control-plane binding, including account, tenant/holding identity, opaque
  credential reference, and version. Changes fail closed before submission or
  reconciliation.
- Provider response bodies are streamed under a fixed bound and mapped to
  redacted classifications. Existing retry and ambiguous-outcome semantics
  remain intact; unbound providers fail closed for account-bound calls.

## Files and verification

- `apps/worker/src/marketing_provider.rs`.
- `apps/worker/src/marketing_delivery.rs`.
- `cargo fmt --all --check` — PASS.
- `cargo check -p erpx-worker` — PASS.
- `cargo clippy -p erpx-worker --all-targets -- -D warnings` — PASS.
- Focused provider tests — 7 passed; full worker tests — 116 passed with 1
  ignored.

## Scope notes

The existing environment resolver remains explicitly host-native pilot
composition. Production secret-manager resolution, vendor-specific provider
adapters, durable callback account routing, and full end-to-end provider
reconciliation remain open.

## Rollback

Disable account-bound worker composition without deleting queued deliveries or
callback inbox state. Preserve idempotency keys and binding versions for safe
replay after forward configuration repair.
