# G5-BIL-048 — bounded billing-policy worker adoption

The existing authoritative `advance_billing_policy` command is now adopted by
a separately registered `stripe_billing_policy` worker handler. The handler
selects due `past_due` and expired-`grace` lifecycles with bounded
`FOR UPDATE SKIP LOCKED` batching, invokes the same domain and SQLx command
path, and commits the complete batch atomically. A deterministic row/time
idempotency key and command/correlation references preserve replay safety while
the existing snapshot, outbox, audit, and receipt writes remain in the same
transaction.

The existing Stripe semantic handler is intentionally unchanged: provider
events first establish provider-authoritative billing evidence, and the policy
worker advances the system-owned grace/read-only policy in a subsequent pass.
The worker does not recover payment, cancel a subscription, or accept a
client/provider-requested billing state.

Evidence:

- `cargo test -p erpx-infrastructure --offline --lib commercial -- --nocapture`
  — PASS: 29 active tests, 8 prerequisite-gated ignored tests.
- `cargo test -p erpx-worker --offline --lib
  registry_includes_support_expiry_handler -- --nocapture` — PASS: 1/1.
- `cargo clippy -p erpx-infrastructure -p erpx-worker --all-targets --offline
  -- -D warnings` — PASS.
- `billing_policy_transition_persists_worker_boundary_and_rolls_back_atomically`
  against fresh disposable PostgreSQL write/read databases — PASS: 1/1.
- `stripe_billing_policy_worker_advances_due_rows_and_is_idempotent` against
  fresh disposable PostgreSQL write/read databases — PASS: 1/1; first pass
  advanced one due row and the immediate replay processed zero rows.
- `cargo fmt --all` — PASS.

No UI or public API change is required for this system-owned policy pass. The
existing authorized billing reads and projection/outbox consumers remain the
read and event boundaries. Existing worker registration compatibility is
retained, and deployment can disable this handler by removing the
`stripe_billing_policy` registration; no source data or outbox history is
deleted, so replay/reconciliation remains available.

Live Stripe credentials/provider calls, catalog reconciliation, production
worker scheduling, and G5 vendor/production acceptance remain queued where
their external inputs are unavailable. This packet does not authorize
production activation, command-owner changes, Folio write retirement, or
cutover.
