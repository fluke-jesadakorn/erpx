# G10 Marketing provider response identity boundary

## Gap addressed

The provider acceptance boundary now requires the response to echo the exact
local `dispatch_id`, current attempt number, and idempotency key authorized for
the delivery call. A provider reference without that identity binding cannot
be converted into `ProviderResult::Accepted`.

## Delivered behavior

- Provider response bodies are bounded at 16 KiB before parsing.
- Missing, malformed, stale, or cross-delivery identity is rejected.
- Provider message references are bounded, non-empty, and control-character
  free.
- Boundary errors retain no response body, provider reference, request
  identity, or credential material.

## Scope

Only `crates/infrastructure/src/marketing_provider.rs` and the focused
`crates/infrastructure/tests/marketing_provider_boundary.rs` test are part of
the implementation. No worker, delivery, shared transport, migration,
manifest, console, or ledger file is changed. This packet does not certify any
external provider or claim live/mock provider completion.

