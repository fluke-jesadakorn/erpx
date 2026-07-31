# G5-003-004 Stripe semantic reconciliation

## Scope

This packet advances the durable Stripe inbox to a typed semantic boundary. It
does not add a live Stripe SDK/client, change the webhook raw-body/signature
boundary, or transfer Folio command ownership.

## Exact semantic mapping

| Stripe event | Semantic result |
|---|---|
| `invoice.paid` | `PaymentRecovered` |
| `invoice.payment_succeeded` | `PaymentRecovered` |
| `invoice.payment_failed` | `PaymentFailed` |
| `customer.subscription.deleted` | `SubscriptionCanceled` |
| `customer.subscription.created` / `.updated` with `status=active` | `TrialEnded`, or recovery when the stored state is past-due/grace/read-only |
| `customer.subscription.created` / `.updated` with `status=past_due` or `unpaid` | `PaymentFailed` |
| `customer.subscription.created` / `.updated` with `status=canceled` | `SubscriptionCanceled` |
| `customer.subscription.created` / `.updated` with `status=trialing` | typed no-op |
| `entitlements.active_entitlement_summary.updated` | typed entitlement capacity/activation update |
| `checkout.session.completed` | typed no-op; checkout is not billing authority |
| `customer.subscription.trial_will_end` | typed no-op reminder |
| `invoice.finalized` | typed no-op; no lifecycle transition |
| any other type/status | permanent semantic error → dead letter |

Internal customer/subscription/module references are read from the explicit
`erpx_*` metadata contract. Raw Stripe payloads are not copied into semantic
events or error messages.

## Delivered behavior

- `StripeSemanticDispatcher` decodes inbox claims into typed billing,
  entitlement, or no-op events and accepts an injected `StripeSemanticApplier`.
- `SqlxCommercialStore` applies billing transitions and entitlement updates
  through optimistic SQLx persistence interfaces.
- Provider event timestamps and IDs are stored additively in existing JSON
  payloads with serde defaults; no migration was added.
- Duplicate event IDs remain inbox duplicates. Same/older provider timestamps
  return `DuplicateOrOutOfOrder` without changing state.
- Repeated newer status observations are idempotent at the lifecycle level;
  e.g. active after past-due is treated as recovery, while active while already
  active only advances the provider watermark.
- The worker exposes `ERPX_WORKER_HANDLER=stripe_semantic`, claims the inbox,
  dispatches typed events, marks successful/no-op/stale events processed, sends
  retryable application failures through bounded retry, and dead-letters
  permanent parse/application failures.
- Existing raw-body persistence, signature handling, lease ownership, retry,
  dead-letter, replay, and debug redaction paths remain intact.

## Verification evidence

- Targeted rustfmt checks: **PASS**.
- `cargo clippy -p erpx-domain -p erpx-infrastructure -p erpx-worker --all-targets -- -D warnings`: **PASS**.
- `cargo test -p erpx-domain -p erpx-infrastructure -p erpx-worker`: **PASS** — domain 37, infrastructure 15, worker 1 non-ignored tests; expected disposable tests ignored.
- Disposable semantic SQLx test: **PASS** — billing duplicate/stale ordering and entitlement duplicate behavior; temporary databases dropped.
- Disposable worker test: **PASS** — valid event processed/applied and unknown event dead-lettered; temporary databases dropped.
- Existing disposable inbox transition test: **PASS** — lease ownership, retry, dead-letter, replay, duplicate handling, and payload redaction; temporary databases dropped.
- `cargo fmt --all --check`: **BLOCKED by an unrelated pre-existing unformatted diff in `apps/server/src/main.rs`**, which was outside this packet and was not changed.

## Residual gaps

- No pinned official Stripe OpenAPI client or API-version publication adapter.
- No live Stripe catalog/Product/Price/Entitlement/Meter synchronization.
- The metadata-to-internal-ID resolver remains an explicit contract; production
  onboarding must populate and validate it before accepting semantic events.
- Billing reconciliation scheduling and payment recovery authorization still
  require the later worker/application integration gate.

## Security and rollback

Raw request bytes remain confined to the inbox persistence boundary. Semantic
debug output and errors exclude payload values, payment instruments, tokens,
and raw bodies. Permanent malformed/unsupported events cannot mutate billing or
entitlements. Retryable storage/concurrency failures do not acknowledge the
inbox claim.

Rollback is limited to reverting the typed dispatcher, worker handler, and
serde-defaulted provider watermark fields. Existing billing/entitlement state
and inbox rows remain readable by the prior code; unknown JSON watermark fields
are ignored by older deserializers.
