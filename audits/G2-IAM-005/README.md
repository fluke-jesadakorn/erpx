# G2-IAM-005 — Durable role-review notification delivery

## Objective

Consume persisted role-expiry notifications through a restart-safe worker
claim/reconcile loop without silently acknowledging delivery.

## Dependencies and ownership

- Owner: main agent; control persistence and worker integration.
- Depends on the existing role-review snapshot/notification migration and
  G2-IAM-004 role-chain evidence.
- Allowed paths: `crates/infrastructure/src/role_review.rs`,
  `apps/worker/src/role_review_notifications.rs`, and worker composition
  exports/registration.
- No external provider or secret-manager state is changed.

## Delivered behavior

- Expired notification leases are reclaimable by a later claim.
- Successful publisher results reconcile only notification keys bound to the
  current claim key.
- Failed and unprocessed notifications return to `pending` for retry.
- Claim identity, key bounds, horizon, and lease bounds fail closed.
- The worker reports claimed, published, processed, and retried counts.
- The default worker registry does not claim role-review rows without an
  explicitly injected publisher.

## Evidence

- `cargo test -p erpx-worker role_review_notifications` — 3 passed.
- Success and partial-failure tests cover reconciliation and retry release.
- `cargo check -p erpx-worker` — PASS.
- `cargo check -p erpx-infrastructure` — PASS.

## Nonclaims

This packet does not claim a production email/push provider, template
localization, recipient policy, provider reconciliation, or complete G2-G14
completion.
