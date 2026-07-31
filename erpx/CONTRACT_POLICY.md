# ERPX shared-contract policy

G0-003 freezes the first contract version at `1` (`erpx_domain::contracts::CONTRACT_VERSION`). Domain contracts and protobuf contracts are versioned independently but must remain semantically compatible.

## Compatibility rules

1. Protobuf packages use a versioned namespace (`erpx.<context>.v1` or `erpx.common.v1`).
2. Existing field numbers and meanings are never reused. Removed fields are marked `reserved` in the next revision.
3. Additive fields, messages, and RPCs are backward-compatible when they have safe defaults and do not change authorization meaning.
4. A change to required authorization, tenant scope, event meaning, aggregate-version semantics, error-code meaning, or idempotency behavior requires a new package/contract version and a compatibility adapter.
5. Events are identified by `event_type` plus `event_version`; handlers must retain old versions through the migration retention window.
6. Commands must carry an idempotency key and may carry an expected aggregate version. Retrying the same key must not repeat a mutation.
7. Queries must carry tenant scope, actor attribution, audit purpose, correlation ID, and an explicit consistency requirement. Read responses must expose projection freshness where eventual consistency applies.
8. Error codes are stable machine values; human messages may change. Correlation IDs are returned/logged for every failure without leaking secrets.
9. Support and impersonation contexts are explicit and cannot be inferred from an ordinary actor string.
10. Every contract change adds a compatibility test, a migration/rollback note, and an entry to `EXECUTION_LEDGER.md` before integration.

## Current shared envelopes

The Rust domain contracts and `erpx.common.v1` protobuf message set define:

- `TenantScope` and containment semantics.
- `ActorAttribution` and represented-user/support-capability attribution.
- `CommandContext` with command, idempotency, correlation/causation, actor, scope, audit, and expected aggregate version.
- `QueryContext`, `QueryConsistency`, and `ProjectionFreshness`.
- `EventEnvelope` with event/aggregate versions, tenant scope, actor, causation/correlation, idempotency, and JSON payload.
- Stable `ErrorCode`/`ErrorContext`.
