# G10-MKT-006 — Marketing provider callback contract

Status: delivered as a pure domain contract. No provider authentication,
HTTP ingress, SQLx, migration, worker composition, or external provider call
is implemented by this packet.

## Delivered behavior

`marketing_callback.rs` adds a transport-independent callback boundary with:

- an explicit `MarketingProviderCallbackBinding` for provider account, opaque
  holding ID, holding ID, and exact `HoldingScope`;
- `MarketingProviderCallbackRequest` and `MarketingProviderCallback`, carrying
  delivery ID, provider message reference, provider event ID, provider event
  timestamp, and the typed `delivered`, `failed`, or `bounced` kind;
- `ProviderAuthenticationDecision::Verified`, which requires an adapter-provided
  verification scheme, verifier key reference, and SHA-256 digest of the exact
  raw body. `NotVerified` is rejected;
- bounded opaque references and safe errors that do not echo provider values;
- the fixed half-open timestamp policy
  `[received_at - 24 hours, received_at + 5 minutes)`;
- deterministic inbox and delivery-transition keys plus an event fingerprint;
- replay classification as `New`, `InOrder`, `OutOfOrder`, `Duplicate`, or
  `ConflictingDuplicate`, using provider timestamp and event ID as the stable
  order tuple;
- `bind_to_delivery`, requiring the exact delivery ID, scope, and provider
  message reference stored by the accepted delivery aggregate.

The event-order comparison uses the watermark’s own typed tuple
`(provider_event_at, provider_event_id)`; it does not pass a watermark to a
callback-only helper.

## Files changed

- `erpx/crates/domain/src/marketing_callback.rs`
- `erpx/crates/domain/src/lib.rs`
- `erpx/audits/G10-MKT-006/README.md`

No other source, migration, application, worker, Folio, or existing audit file
was changed by this packet.

## Verification evidence

From `erpx/`:

- `cargo fmt --all` — passed.
- `cargo fmt --all --check` — passed.
- `cargo clippy -p erpx-domain --all-targets -- -D warnings` — passed.
- `cargo test -p erpx-domain --lib` — passed, 135 passed, 0 failed.

The callback module contributes seven unit tests covering authentication
decisions, provider-account/holding/holding/scope binding, deterministic
duplicate identity, lower-inclusive/upper-exclusive timestamps, delivery
provider-reference binding, bounded/redacted errors, and deterministic
out-of-order classification.

## Open integration points

The main agent must integrate this contract in this order:

1. Verify the raw request body and construct `VerifiedProviderAuthentication`
   from the provider adapter result; never pass a raw signature or secret into
   the domain value.
2. Resolve the provider account to an active control-plane holding/holding
   binding and resolve the delivery through an authorized write-side lookup.
3. Construct and `verify` the request using that trusted binding and the
   ingress receipt time.
4. Insert the `inbox_key` with the event fingerprint atomically and treat an
   exact duplicate as a safe no-op. Reject a same-key fingerprint conflict.
5. Call `bind_to_delivery`, then authorize and apply the provider outcome
   through the Marketing aggregate with optimistic concurrency and an outbox
   event. Persist out-of-order callbacks for audit only; never regress the
   delivery state from them.

SQLx tables, migrations, raw-body HTTP routes, provider-specific parsers,
signature verification, event inbox processing, worker composition, and
provider callback response mapping remain open.

## Security concerns

- The domain contract proves only that an upstream verifier reported success;
  it does not verify signatures. The ingress must authenticate the exact raw
  bytes before parsing and must bind the supplied body digest to those bytes.
- Provider account and holding/holding values must come from trusted route and
  control-plane lookup data, not from untrusted callback JSON. `verify` rejects
  any mismatch with the expected binding.
- The fixed timestamp window is intentionally strict and half-open. The
  infrastructure inbox must verify authentication and apply this policy before
  accepting a new event; an old event must not bypass the inbox through a
  different route.
- A duplicate provider event is not a delivery-state transition. A conflicting
  duplicate must be rejected and alerted. An out-of-order event may be retained
  for audit but must not roll back a newer delivery outcome.
- `bind_to_delivery` is required before state mutation. Provider message lookup
  alone is insufficient because the callback must match the stored accepted
  reference and exact delivery scope.
- The callback values are opaque references only. Provider payloads, signatures,
  credentials, tokens, and failure bodies are deliberately outside this type.

## Database effects

None. This packet adds no SQLx dependency, migration, repository, or worker
behavior.

## Rollback

Because the contract is additive and no database effects occur, rollback is a
source-only revert of `marketing_callback.rs` and its `lib.rs` module export,
plus removal of this audit directory. Do not remove or alter existing
Marketing delivery tables or callback consumers without first disabling their
route and draining their inbox safely.
