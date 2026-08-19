# G1-004 readiness follow-up audit

## Scope and result

Read-only audit of the ERPX source snapshot observed on 2026-07-24. No source,
migration, ledger, runtime, database, secret, or Folio file was changed. The
only write for this packet is this report.

The earlier correlation and HTTP 5xx findings are fixed. The `RenewRole`
commercial/SoD bypass is fixed in the current server policy, but its full
server integration coverage is still missing. The earlier worker-context
finding is only partially fixed: structured redacted logging exists, while
durable outbox and Stripe semantic failure paths still receive raw strings.
Readiness remains materially incomplete.

## Prior HIGH findings

| Prior finding | Current evidence | Verdict |
|---|---|---|
| Correlation aliases could diverge | `boundary.rs:44-85` resolves all three aliases and rejects conflicts; `boundary.rs:326-347` requires header/context equality. Tests are `boundary.rs:453-475`. HTTP uses the same aliases at `main.rs:1754-1777`; gRPC failures are enriched by `main.rs:69-74`. | **Fixed at the common boundary.** Malformed HTTP correlation returns before `HttpRequestTrace` starts (`main.rs:1761-1777`), so observability is still incomplete. |
| `RenewRole` bypassed billing, entitlement, and SoD | Customer-scoped renewal now enters `authorize_entitled_command` through `authorize_role_lifecycle_command` (`main.rs:397-479`); the handler calls it before loading and renewing the assignment (`main.rs:1298-1348`). Platform renewal has an explicit identity-policy branch with role-chain and SoD checks (`main.rs:414-457`). | **Fixed in code.** The only server regression test is scope direction (`main.rs:2607-2624`); no handler test proves missing subscription, read-only billing, missing entitlement, failed SoD, or the allowed platform/customer branches. The assignment itself has no observed persisted customer ID; caller-supplied `customer_id` is the commercial anchor, so resource-binding needs an explicit acceptance test. |
| Worker failures lacked structured context and redaction | `WorkerErrorContext` carries worker/event/aggregate/correlation identifiers and sanitizes log context (`crates/infrastructure/src/worker.rs:19-67`); tests cover correlation/redaction at `:446-481`. | **Partially fixed.** The sanitized context is logged, but `run_once` passes the original error to durable outbox retry/DLQ (`worker.rs:205-228`, `:247-252`; persistence binds it at `outbox.rs:348-420`). Stripe semantic failures similarly use `error.to_string()` in `apps/worker/src/lib.rs:444-480`. |
| HTTP mapped all 5xx to unavailable | `errors.rs:167-180` maps 500–599 to `internal` except 502–504; regression test `errors.rs:305-318`. | **Fixed.** |

## Current boundary and error matrix

The proto currently exposes ten typed RPC methods: three Customer, four
Support, two Role, and one Entitlement method (`crates/proto/proto/erpx/platform/v1/platform.proto:155-175`). All current implementations use
`grpc_handler!` (`apps/server/src/main.rs:763-1369`), and the wrapper adds a
correlation ID and stable error metadata. `AuthContext::authorize` performs
write-side token/session authorization and transport actor/session matching
(`main.rs:199-229`); the Tonic interceptor repeats transport attribution and
enriches failures (`main.rs:1612-1645`). Customer global reads validate purpose
and record actor, session, customer, scope, purpose, and correlation
(`main.rs:899-1004`).

Remaining common gaps:

| Area | Evidence | Severity |
|---|---|---|
| Error context wire contract | `errors::enrich` emits JSON in `x-erpx-error-context`, but `ErrorContext.details` is always an empty map (`errors.rs:33-62`); no protobuf `Status::details` payload is emitted. Dynamic permission, conflict, assignment, and support text remains public through `errors.rs:86-106` and `safe_message` only forces fallback for internal/unavailable/deadline statuses (`:65-83`). | Medium |
| HTTP tracing | `HttpRequestTrace` records method, path, correlation, and latency only (`errors.rs:182-212`), with no status/outcome. Parser failures and malformed correlation return before the trace starts (`main.rs:1752-1777`, `read_http_request` at `:2359+`). | Medium |
| Identity error parity | gRPC identity database failures map to `internal` (`main.rs:630-644`), while HTTP identity database failures map to 503 (`main.rs:1695-1708`). | Medium |
| Object authorization | Signed upload/download HTTP handlers now exist (`main.rs:2064-2156`), but authorization checks only an active registry row, namespace equality, and a Holding-scope role (`main.rs:2158-2218`). It does not evaluate customer/resource binding, branch scope, billing, entitlement, SoD, or support capability. It also parses the registry’s opaque `data_profile_key` as a UUID (`:2209-2215`), which is a compatibility/functionality risk for non-UUID opaque references. There are storage seam tests, but no handler-level HTTP tests. | High |

## Worker, holding, commercial, and readiness gaps

| Boundary | Delivered evidence | Concrete remaining gap |
|---|---|---|
| Worker executable | Startup selects only `customer_360` or `stripe_semantic` and initializes basic formatting (`apps/worker/src/main.rs:5-54`). Runtime connects and migrates only control write/read pools (`apps/worker/src/lib.rs:163-180`). | No worker heartbeat/readiness contract, worker lease health, holding-database routing, commercial inbox readiness, or correlation span for successful passes. `WorkerErrorContext` omits holding scope, actor, and causation even though the envelope contains them (`crates/infrastructure/src/worker.rs:23-44`; `outbox.rs:30-52`). |
| Outbox/projector | Retry/DLQ tracing now includes sanitized context (`crates/infrastructure/src/worker.rs:191-253`); projection claim/checkpoint ordering remains present (`:331-405`). | Durable `last_error` receives unsanitized handler text (`outbox.rs:348-420`). Projection/rebuild converts errors back to `error.to_string()` (`worker.rs:337-402`), and customer projection trusts event payload/scope while writing bindings (`apps/worker/src/lib.rs:302-327`) without an explicit holding/resource consistency check. `replay_dead_letter` is a storage/runtime method with no authenticated, audited operator boundary (`apps/worker/src/lib.rs:265-273`; `outbox.rs:380-401`). |
| Holding routing/provisioning | The infrastructure router validates opaque references, active state, holding match, and selects server-side write/read pools (`crates/infrastructure/src/provisioning.rs:132-184`). Provisioning creates/migrates both databases and records health (`:216-272`); activation is health-gated (`:813-858`); replacement-read state transitions exist (`:861-1001`). | The server composition creates only `SqlxHoldingStoreRegistry` for object authorization (`apps/server/src/main.rs:2286-2295`); no `HoldingDatabaseRouter` or `HoldingDatabaseProvisioner` is composed into `AppState` or any typed holding command/query. Backup/restore is still an injected executor seam and explicitly does not invoke real pg backup/restore (`provisioning.rs:537-541`). No server readiness probe aggregates holding registry, physical write/read health, or projection switch state. |
| `/readyz` | It checks control write/read `SELECT 1` (`repositories.rs:268-272`), identity readiness, and storage configuration (`main.rs:1828-1857`). | `registry` is passed to the HTTP listener but is not used in the readiness calculation. Commercial inbox/worker health, holding physical databases, projection lag/checkpoints, and live MinIO connectivity are absent. Storage reports `live_connectivity: "not_probed"` and readiness checks configuration only (`storage.rs:249-274`). A configured-but-unreachable dependency can therefore coexist with HTTP 200 readiness. |
| Commercial/Stripe | Raw bodies are persisted and duplicate event IDs are idempotent (`commercial.rs:1337-1369`); claims intentionally exclude raw body (`:1371-1440`). Provider adapter errors have deterministic redaction tests (`commercial.rs:1982+`). | `StripeInboxEvent` stores event/type/time/payload/raw body only (`commercial.rs:16-23`); the HTTP digest is computed and discarded (`main.rs:1753`), and no correlation ID, signature metadata, provider delivery metadata, or audit record is retained (`main.rs:2260-2290`). `last_error` is only truncated to 4096 characters (`commercial.rs:1759-1761`). Semantic persistence classification still stores `PersistenceError::to_string()` (`:1736-1750`), and dead-letter replay is a direct store method (`:1585-1607`) without an authenticated/audited platform command boundary. |

## Verification evidence

Commands were run from `/Users/fluke/Desktop/Work/folio/erpx`; no disposable
database test was enabled and no runtime/database mutation was performed.

| Command | Exact result |
|---|---|
| `cargo fmt --all --check` | PASS |
| `cargo test -p erpx-server -- --nocapture` | PASS: 29 passed, 0 failed, 0 ignored |
| `cargo test -p erpx-worker --lib -- --nocapture` | PASS: 1 passed, 0 failed, 1 ignored (PostgreSQL integration test) |
| `cargo test -p erpx-infrastructure --lib worker::tests -- --nocapture` | PASS: 2 passed, 0 failed, 23 filtered |
| `cargo test -p erpx-infrastructure --lib commercial::tests -- --nocapture` | PASS: 11 passed, 0 failed, 3 ignored (PostgreSQL tests) |
| `cargo test -p erpx-infrastructure --lib provisioning::tests -- --nocapture` | PASS: 2 passed, 0 failed, 23 filtered |
| `cargo test -p erpx-domain access::tests -- --nocapture` | PASS: 10 passed, 0 failed, 31 filtered |
| `cargo test -p erpx-application role_lifecycle_tests -- --nocapture` | PASS: 3 passed, 0 failed, 1 filtered |
| `cargo check -p erpx-server && cargo check -p erpx-worker` | PASS |
| `cargo clippy -p erpx-worker --all-targets -- -D warnings` | PASS |
| `cargo clippy -p erpx-infrastructure --all-targets -- -D warnings` | PASS |
| `cargo clippy -p erpx-server --all-targets -- -D warnings` | **FAIL**: `clippy::too_many_arguments` for `handle_object_http` at `apps/server/src/main.rs:2064` (8 arguments; limit 7). |

## Dependency-ready next packets

1. **G1-004-WORKER-DURABLE-CONTEXT (AG-FND):** make sanitized structured
   context the durable outbox/Stripe failure contract; carry holding scope,
   causation, actor, and correlation; add holding-mismatch, replay authorization,
   and no-secret persistence tests. Depends on the existing envelope and worker
   context primitives.
2. **G1-004-READINESS-COMPOSITION (AG-FND + AG-PRV):** define server/worker
   readiness providers for worker heartbeat, commercial inbox, holding registry
   and physical write/read probes, projection freshness, and live MinIO; wire
   them into `/readyz`; test each dependency failing independently.
3. **G6-OBJECT-AUTHORITY-BOUNDARY (main + AG-PRV):** bind object access to the
   opaque holding registry, customer/branch resource scope, billing/entitlement,
   SoD, and support capability where applicable; add handler tests and fix the
   clippy failure. Keep this separate from worker/readiness changes.
4. **G5-WEBHOOK-AUDIT-RETENTION (AG-BIL):** retain correlation, raw-body
   digest, signature/provider delivery metadata, and an auditable replay command;
   sanitize all semantic persistence/DLQ errors. Do not expose raw bodies to
   workers.
5. **G1-004-ERROR-CONTEXT-WIRE (main/AG-FND):** complete `details`/Tonic wire
   compatibility, central dynamic-message redaction, identity error parity, and
   status/outcome tracing, including parser failures.
6. **AG-QA regression:** rerun this matrix only after packets 1–5; passing
   domain, seam, or unit tests must not be treated as proof that production
   readiness or handler authorization is complete.

## Security and false-completion risks

- Current passing server tests cover boundary helpers, storage seams, signatures,
  and scope direction, not the full `RenewRole` commercial-policy matrix or
  object HTTP handler.
- Passing infrastructure provisioning tests use injected executors; they do not
  prove real physical holding creation, restore, or server routing.
- Provider-error redaction tests do not cover raw `PersistenceError` strings
  written by worker and Stripe semantic paths.
- `/readyz` can report ready while worker, commercial processing, holding
  databases, projection state, or MinIO is unavailable.
- `ErrorContext` metadata presence does not prove complete wire compatibility or
  safe dynamic messages.

## Rollback

No source, migration, database, runtime, Folio, or ledger changes were made.
Rollback is limited to removing this report:

`erpx/audits/G1-004-QA-READINESS-FOLLOWUP/evidence/README.md`
