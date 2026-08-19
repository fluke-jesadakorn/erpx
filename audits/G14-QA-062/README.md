# G14-QA-062 — Holding scheduler and Marketing seam audit

Date: 2026-07-25  
Owner: AG-QA  
Mode: independent, read-only

## Scope and constraints

This packet audits the newly integrated holding projection scheduler registry-binding enforcement and the existing Marketing delivery/scheduler seams. No production code, migration, documentation, or existing audit was modified. The only file created by this packet is this README.

Inspected paths:

- `crates/infrastructure/src/holding_projection_scheduler.rs`
- `crates/infrastructure/src/repositories.rs`
- `crates/infrastructure/src/provisioning.rs`
- `crates/infrastructure/src/marketing_delivery_persistence.rs`
- `crates/domain/src/marketing_delivery.rs`
- `apps/worker/src/holding_projection_scheduler.rs`
- `apps/worker/src/marketing_delivery.rs`
- `apps/worker/src/lib.rs`
- `apps/worker/src/main.rs`
- `apps/server/src/main.rs`
- `crates/migration/migrations/0025_holding_projection_scheduler.sql`
- `crates/migration/migrations_holding_write/0026_marketing_delivery.sql`
- `crates/migration/migrations_holding_read/0026_marketing_delivery_projection.sql`
- `crates/migration/src/lib.rs`

Database effects: none. The targeted tests are unit/contract tests; no disposable PostgreSQL integration test was invoked by this packet.

## Verified delivered behavior

The registry-binding change is present and independently verified:

- `HoldingProjectionSchedulerRepository::new` requires both the scheduler pool and an `Arc<SqlxHoldingStoreRegistry>` (`crates/infrastructure/src/holding_projection_scheduler.rs:693-704`).
- `schedule`, `claim_due`, `renew_lease`, `complete`, `fail`, and `reclaim_stale` call `require_registered_binding` before touching scheduler rows (`:720-726`, `:858-864`, `:1015-1026`, `:1069-1079`, `:1129-1140`, `:1218-1226`).
- The check requires a registry row in `active` state and an exact `data_profile_key` match (`:1316-1335`). Missing, suspended, provisioning, recovery, or mismatched bindings fail closed.
- Sealed claim batches return replay-marked claims, and the worker skips replay execution (`apps/worker/src/holding_projection_scheduler.rs:192-195`).
- Lease renewal, completion, failure, and stale-claim history updates are performed in transactions and require the expected claim/worker ownership (`crates/infrastructure/src/holding_projection_scheduler.rs:1028-1060`, `:1079-1125`, `:1140-1215`, `:1231-1289`).
- Marketing delivery has separate write/read migrations with forced RLS, write-side consent/suppression rechecks, claim UUID ownership, bounded provider-result classification, and provider-reference matching for `mark_delivered`.

These are meaningful improvements. They do not establish the complete ERPX authorization, routing, callback, or runtime-composition contract.

## Findings

### P0 — none proven in this packet

No confirmed P0 credential disclosure or direct cross-holding write bypass was observed. The exact holding filters, physical pool parameters, and forced-RLS policies provide partial isolation. The P1 findings below still prevent acceptance of the universal security and production-runtime requirements.

### P1-01 — Registry binding is not universal authorization

The new check authenticates only a holding lifecycle registration and exact holding binding. It does not evaluate a principal, active session, role/permission, functional domain, scope containment below the holding, entitlement/module activation, billing state, approval limit, segregation of duties, support capability, or audit context. The scheduler request models carry a holding/holding binding and worker UUID, but no authoritative command context (`crates/infrastructure/src/holding_projection_scheduler.rs:64-100`, `:520-565`). `schedule` therefore gates on registration only (`:720-727`, `:1316-1335`).

Impact: a caller that can reach this repository or a composed scheduler source can schedule or claim work for any active registered holding without a universal RBAC/entitlement decision or an attributable authorization record. The same conclusion applies to the Marketing delivery worker source: `SqlxMarketingDeliveryStore` is constructed from pools and a caller-provided `data_profile_key` (`crates/infrastructure/src/marketing_delivery_persistence.rs:41-68`) and its mutating methods have no command/audit context.

Required follow-up: make scheduler and delivery commands consume an authoritative service/command context, enforce the frozen effective-access and entitlement checks at the write boundary, bind the holding route from control-plane state, and emit an immutable audit event containing actor/service, scope, purpose, correlation, and outcome. The explicit registry-binding checks are **not sufficient for universal authorization**.

### P1-02 — Scheduler routing and production multi-holding composition are still absent

`verify_holding_route` is only a helper (`crates/infrastructure/src/holding_projection_scheduler.rs:706-718`). The scheduler handler does not receive a `HoldingDatabaseRouter`; it passes the claim directly to an unconstrained `HoldingProjectionExecutor` (`apps/worker/src/holding_projection_scheduler.rs:97-103`, `:192-196`). The executor contract does not require write/read route resolution, RLS scope setup, replacement-read selection, or projection checkpoint handling.

The worker executable constructs only `runtime.handler_registry()` (`apps/worker/src/main.rs:8-14`). That default registry contains no holding projection scheduler or Marketing delivery handler (`apps/worker/src/lib.rs:1531-1560`); the explicit helper constructors are library APIs only (`:1581-1627`). Source search found no production call that composes either helper. Consequently, setting `ERPX_WORKER_HANDLER=holding_projection_scheduler` or `marketing_delivery` in the shipped worker does not run these seams—the default registry rejects the handler as unknown.

The Marketing store also accepts arbitrary already-created write/read pools and a holding reference; it does not resolve those pools through `HoldingDatabaseRouter` or `SqlxHoldingStoreRegistry` (`crates/infrastructure/src/marketing_delivery_persistence.rs:47-68`).

Impact: the registry check prevents unregistered scheduler rows in the repository path, but it does not prove that the executor or Marketing store writes to the registered holding's physical write database and reads from its matching replacement/read database. The required production multi-holding worker composition is not delivered.

Required follow-up: provide one production composition root that enumerates active holding registrations, resolves server-side write/read routes, installs holding-scoped RLS, constructs the correct per-holding executor/store, enforces readiness/projection-switch state, and registers these handlers in the actual worker binary. Add route-mismatch and suspended-holding integration tests.

### P1-03 — Marketing provider callback ingress is missing and `mark_delivered` is not authenticated

The domain now checks that a delivery callback supplies the previously stored provider message reference (`crates/domain/src/marketing_delivery.rs:573-597`), and the SQLx store forwards that reference (`crates/infrastructure/src/marketing_delivery_persistence.rs:274-310`). There is, however, no Marketing callback operation in `MarketingDeliverySource` (`apps/worker/src/marketing_delivery.rs:39-58`), no Marketing callback route in the server HTTP dispatch, and no provider signature/raw-body verification, provider-account binding, event ID inbox, replay window, or callback holding resolution. The server route search found Stripe webhook handling only (`apps/server/src/main.rs:4367-4397`, `:5165-5184`).

`mark_delivered` accepts a caller-supplied timestamp and provider reference at an infrastructure boundary. Without an authenticated provider ingress, any future caller that can obtain the scoped store and reference can forge a delivered state; repeated callback events have no durable event-id deduplication. Provider-reference matching narrows the attack but is not authentication.

Required follow-up: add a raw-body provider ingress with a pinned provider/account binding, signature verification, bounded timestamp validation, holding/delivery resolution, durable inbox idempotency, redacted errors, and a worker-owned state transition. Add forged-reference, duplicate-event, out-of-window, cross-holding, and out-of-order tests.

### P1-04 — Marketing read projection and delivery state events are not runtime-connected

`project_read_model` writes the new disposable read table and `get` reads only that table (`crates/infrastructure/src/marketing_delivery_persistence.rs:313-385`). No worker handler or source invokes `project_read_model`; source search found only its definition. `record_provider_result` and `mark_delivered` update the authoritative delivery row but do not write a corresponding snapshot/outbox event (`:218-310`). The snapshot/outbox helper is called by enqueue only (`:70-130`, `:480-517`).

Impact: after enqueue, provider acceptance, retry, terminal failure, or delivery, the read model can be missing or stale. A replacement read database cannot rebuild the complete delivery state from a delivery-specific event stream because the state transitions are not emitted and no production projector is composed. This violates the claimed CQRS/read-rebuild behavior even though the read migration and projection helper compile.

Required follow-up: emit versioned state-transition events atomically with every authoritative transition, add an idempotent Marketing delivery projector with checkpoints and rebuild support, register it in the worker composition, and test out-of-order/replayed versions against a disposable read database.

### P1-05 — Scheduler retry replay is only claim-key scoped; executor idempotency is not enforced

The scheduler handler creates a new claim key containing a new UUID on every run (`apps/worker/src/holding_projection_scheduler.rs:173-180`). A sealed batch is protected against replay under the same key, but a lease-expiry/retry pass receives a new key and can invoke the executor again. `HoldingProjectionExecutor::execute` has no idempotency token or required durable projection receipt (`:97-103`), and the scheduler repository records claims but not the executor's side-effect identity.

Impact: a projector or future executor with an external/non-idempotent side effect can run twice when the first worker loses its lease after performing the side effect. The pure test `sealed_claim_replay_is_not_executed_again` proves only same-key replay handling; it does not prove lease-loss/retry idempotency.

Required follow-up: bind execution to a stable job/event/version identity, require an idempotent projector transaction or durable receipt/checkpoint in the target read database, and test crash-after-side-effect-before-complete and cross-worker retry scenarios.

### P2-01 — Registry binding enforcement has a check/use race and no database referential constraint

`require_registered_binding` reads the registry using one operation and the subsequent scheduler mutation uses a separate pool operation (`crates/infrastructure/src/holding_projection_scheduler.rs:1316-1335`, `:733-769`, `:864-1012`). A registration can be suspended or rebound between the check and the insert/claim. Migration `0025_holding_projection_scheduler.sql` has holding/holding text fields but no foreign key or database-side join to `erpx_control.holding_registry`.

Impact: a narrow suspension/rebinding race can permit one scheduler operation after the lifecycle state changed. This is defense-in-depth rather than evidence of a present cross-holding leak, but it is insufficient for immediate revocation semantics.

Required follow-up: serialize lifecycle state with the scheduler operation or include a registry version/state predicate in the same transaction, and add a schema/application invariant that scheduler rows cannot outlive their registered holding binding.

### P2-02 — The passing evidence does not exercise the new SQLx registry check

The infrastructure scheduler tests are pure model tests; no `#[sqlx::test]` or scheduler repository integration test was found. The Marketing persistence tests likewise do not exercise the new read projection, callback transition, or route composition. The targeted command results below therefore establish compilation and pure transition behavior, not the claimed database authorization behavior.

Required follow-up: add disposable control-plane and holding write/read integration tests for unregistered, inactive, mismatched, suspended-during-operation, and cross-holding cases, plus Marketing write/read RLS and projection tests. Keep ignored external-database tests explicitly reported in CI.

## Exact verification evidence

All commands were run from `/Users/fluke/Desktop/Work/folio/erpx` and completed with exit code 0:

| Command | Result |
|---|---|
| `cargo fmt --all --check` | pass |
| `cargo clippy -p erpx-infrastructure --all-targets -- -D warnings` | pass |
| `cargo test -p erpx-infrastructure --lib` | 90 passed, 0 failed, 4 ignored |
| `cargo test -p erpx-migration` | 6 passed, 0 failed; 0 doc tests |
| `cargo clippy -p erpx-worker --all-targets -- -D warnings` | pass |
| `cargo test -p erpx-worker --lib` | 35 passed, 0 failed, 1 ignored |
| `cargo clippy -p erpx-domain --all-targets -- -D warnings` | pass |
| `cargo test -p erpx-domain --lib` | 128 passed, 0 failed |
| `cargo check -p erpx-console --no-default-features --features web` | pass |
| `cargo check -p erpx-console --no-default-features --features desktop` | pass |
| `cargo test -p erpx-worker --lib holding_projection_scheduler` | 4 passed, 0 failed; 32 filtered out |
| `cargo test -p erpx-infrastructure --lib holding_projection_scheduler` | 9 passed, 0 failed; 85 filtered out |

Read-only searches also verified:

- `rg -n "HoldingProjectionSchedulerRepository::new|MarketingDeliveryHandler|handler_registry_with_marketing|handler_registry_with_holding_projection" erpx --glob '*.rs'` found the library seams but no production composition call for the scheduler repository or Marketing store.
- `rg -n "#\[sqlx::test|HoldingProjectionSchedulerRepository|holding_projection_scheduler" erpx/crates/infrastructure/src/holding_projection_scheduler.rs erpx/crates/infrastructure/tests erpx/crates/infrastructure/src --glob '*.rs'` found no SQLx scheduler repository test.
- `rg -n "marketing delivery|delivery.*callback|provider.*callback|webhook|raw body|signature|inbox|marketing_delivery" erpx/apps/server/src erpx/crates/proto erpx/crates/infrastructure/src erpx/apps/worker/src --glob '*.rs' --glob '*.proto'` found Stripe webhook infrastructure but no Marketing delivery callback ingress.

No database, server, or client runtime was mutated by this packet.

## Handoff

Delivered behavior: independently verified the active-state/exact-holding registry gate, sealed-claim replay skip, transactional claim transitions, Marketing claim/provider-reference checks, separate read migration, and all listed compile/test evidence.

Files changed: `erpx/audits/G14-QA-062/README.md` only.

Unfinished work: all P1/P2 findings above remain open; this packet does not approve G6, G10, or G14 completion and does not establish final Folio write retirement.

Security concerns: universal RBAC/entitlement/audit is still absent at these seams; physical route selection is not required by the scheduler executor or Marketing store; provider callbacks are unauthenticated; read-side state can be stale; and retry execution lacks a mandatory durable idempotency contract.

Integration order:

1. AG-FND/AG-IAM: add authenticated scheduler/delivery command context, effective-access/entitlement checks, audit attribution, and atomic registry-state binding.
2. AG-PRV/AG-FND: implement production multi-holding route resolution, replacement-read selection, and worker composition in `apps/worker/src/main.rs`.
3. AG-MKT: implement signed provider callback ingress, inbox idempotency, transition events, and the read projector/rebuild path.
4. AG-QA: add disposable PostgreSQL integration tests for registry authorization, RLS, route mismatch, callback forgery/replay, and lease-loss idempotency; rerun the full wave.
5. Re-audit these findings before advancing the G6/G10 gates or claiming final completion.
