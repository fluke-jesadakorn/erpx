# G1-004-WORKER-DURABLE-CONTEXT — durable sanitized failure context

## Delivered

- Added control migration `0061_worker_error_context.sql`, which stores a
  bounded JSON object beside the legacy outbox `last_error` text.
- Worker failure context now carries event version, aggregate identity and
  version, tenant scope, actor, correlation, causation, idempotency key,
  worker identity, retryability, and a sanitized message.
- The worker validates that the context still binds to the exact claimed
  event before retry/dead-letter persistence; tenant, actor, correlation, and
  causation mismatches fail closed.
- Scope/actor metadata containing secrets, credentials, tokens, private keys,
  raw bodies, signatures, or endpoint URLs is omitted rather than persisted.
- Published and replayed rows clear transient failure context; business event
  payloads and source aggregates are unchanged.

## Evidence

- `cargo test -p erpx-infrastructure --lib worker::tests --offline` — PASS;
  4/4, including tenant/correlation mismatch and secret-bearing metadata
  rejection.
- `cargo test -p erpx-infrastructure --lib outbox::tests --offline` — PASS;
  3/3.
- `cargo check -p erpx-worker --offline` — PASS.
- Fresh disposable control write/read SQLx acceptance for durable context —
  PASS; 1/1. The row retained safe tenant/correlation/causation context,
  omitted the secret-bearing actor, and persisted only the generic display
  error. Both databases were removed afterward.
- Current-tree serialized workspace verification — PASS for every local
  ignored database target, including the new worker-context acceptance. The
  eight-database matrix removed its disposable databases; the only nonzero
  ignored target was the intentional credentialless G14 vendor preflight.
- Current-tree dedicated identity SQLx checks — PASS; directory RBAC scope
  authority 1/1 and local directory persistence 2/2 on fresh disposable
  databases. These are local persistence checks, not external vendor
  certification.

## Security and residual boundary

The context is diagnostic metadata, not authorization. Dead-letter replay is
still an operator/application boundary that needs an authenticated, audited
replay command before production use; this packet does not activate or expose
that operation. Stripe-specific webhook metadata and provider replay policy
remain separate contracts.

## Rollback

Remove migration `0061`, the `last_error_context` field and persistence paths,
and this audit directory. Existing outbox rows and business event payloads
remain valid because the new column is nullable and additive.
