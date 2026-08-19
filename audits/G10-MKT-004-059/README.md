# G10-MKT-004-059 Marketing durable channel delivery contract

## Delivered behavior

- Added a pure Rust delivery contract for email, SMS, social, push, and
  webhook dispatch records.
- Added send-time consent and suppression re-check decisions with explicit
  missing, pending-double-opt-in, revoked, expired, invalid-proof, clear,
  active, and expired states.
- Requires a non-empty double-opt-in proof for granted consent and fails
  closed for pending or invalid consent.
- Derives a deterministic, holding-scoped delivery idempotency key from the
  profile, campaign, purpose, channel, logical message reference, and
  template revision.
- Adds a registry that rejects a deterministic key being bound to another
  dispatch and a dispatch state transition that rejects a second in-flight
  attempt.
- Adds bounded exponential retry policy, exact due-time checks, maximum
  attempt enforcement, expiry checks, and terminal failure states.
- Cancels queued or retry-scheduled dispatches when an active suppression is
  rechecked. In-flight work is not falsely reported as cancelled.
- Distinguishes provider acceptance from delivery confirmation. Delivery is
  terminal only after an explicit delivery callback; suppression and terminal
  provider failure are also terminal.
- Contains pure tests for all channel records, consent/suppression checks,
  double-opt-in and proof gating, strict expiry boundaries, idempotency
  stability/conflict, retry bounds and due-time boundaries, duplicate attempt
  prevention, suppression cancellation, provider acceptance, delivery, and
  terminal states.

## Files changed

- `crates/domain/src/marketing_delivery.rs`
- `audits/G10-MKT-004-059/README.md`

## Contract and migration changes

- The main integration packet has exported `marketing_delivery` from
  `crates/domain/src/lib.rs`; this follow-up fixes the exported-module compile
  error without changing any shared path.
- No infrastructure, application, server, worker, protobuf, UI, migration,
  database, provider, or legacy Folio files were changed.
- No external provider delivery is claimed by this contract. Provider
  acceptance and delivery are represented only as explicit input results.

## Packet verification

Run from `erpx/`:

- `rustfmt --edition 2024 crates/domain/src/marketing_delivery.rs` — PASS.
- `rustfmt --edition 2024 --check crates/domain/src/marketing_delivery.rs` —
  PASS.
- `git diff --check -- erpx/crates/domain/src/marketing_delivery.rs` — PASS.
- `rg -n "marketing_delivery" crates/domain/src/lib.rs` — PASS; export present.
- `cargo test -p erpx-domain marketing_delivery` — PASS: 8 passed, 0 failed,
  0 ignored, 114 filtered out.

## Unfinished work

- Main must independently run the full domain suite and
  `cargo clippy --workspace --all-targets -- -D warnings` as part of the next
  integration wave.
- Persistence of dispatches, atomic idempotency reservation, durable worker
  claiming, provider adapters, consent re-check scheduling, suppression
  cancellation jobs, webhook verification, delivery callbacks, and channel
  observability remain outside this packet.
- Provider-specific rate limits, templates, journey orchestration, campaign
  policy, Finance attribution, and holding routing remain open.

## Security concerns

- Delivery authorization must be persisted with an atomic version check so
  two workers cannot reserve the same dispatch concurrently.
- The idempotency key must be passed to provider adapters without allowing a
  caller to replace it with a non-deterministic key.
- Destination references are opaque identifiers in this contract; provider
  credentials, tokens, webhook secrets, and message bodies must remain in
  secret-safe infrastructure boundaries.
- Consent and suppression must be rechecked immediately before every attempt,
  and an active suppression must cancel queued work before provider dispatch.
- A provider-accepted result is not a delivery claim; only an authenticated,
  holding-scoped callback may transition to delivered.

## Integration order

1. Main exports `marketing_delivery` from the domain crate and runs its pure
   tests and strict Clippy.
2. Infrastructure persists the dispatch aggregate, idempotency registry,
   attempt/version transitions, and terminal outcomes atomically.
3. The worker rechecks consent/suppression, claims one attempt, applies bounded
   retries, and emits redacted audit/outbox events.
4. Channel adapters implement provider-specific submission and authenticated
   acceptance/delivery callbacks without bypassing the domain state machine.
5. Marketing campaign/journey, CRM handoff, attribution, audit, and UI packets
   consume the stable delivery contract.

## Rollback

Delete only `crates/domain/src/marketing_delivery.rs` and this evidence file.
No source rows, migrations, provider calls, or legacy Folio writes are
introduced by this packet.
