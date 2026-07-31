# AG-QA-061 — Independent read-only audit of G6-PRV and G10-MKT

Date: 2026-07-25  
Scope: currently integrated G6-PRV durable projection scheduling and G10 Marketing delivery persistence/worker claim handling.  
Verdict: **not gate-complete**. The work provides useful typed persistence seams, but production routing, authorization, callback security, read-side composition, and several claim invariants remain unverified or unmet.

No application source, migration, plan, ledger, environment, runtime, or Folio file was changed by this audit. Only this report was created.

## Delivered evidence that was independently confirmed

- G6 migration `0025_tenant_projection_scheduler.sql` creates control-plane job, batch, claim, and rebuild tables with bounded policy fields and `FOR UPDATE SKIP LOCKED` repository claims (`crates/migration/migrations/0025_tenant_projection_scheduler.sql:5-115`; `crates/infrastructure/src/tenant_projection_scheduler.rs:851-995`).
- G6 pure state rules cover exact lease expiry, claim ownership, retry/dead-letter transitions, scope matching, and bounded policy values (`crates/infrastructure/src/tenant_projection_scheduler.rs:322-524`).
- G10 migration `0026_marketing_delivery.sql` creates forced-RLS write-side dispatch/idempotency tables (`crates/migration/migrations_tenant_write/0026_marketing_delivery.sql:6-59`).
- G10 enqueue is transactionally coupled to the delivery row, aggregate snapshot, and queued outbox event (`crates/infrastructure/src/marketing_delivery_persistence.rs:70-136`, `424-462`). Due claims recheck write-side consent and suppression before reserving an attempt (`:138-216`).
- Worker tests now compile the new scheduler handler as well as the Marketing handler (`apps/worker/src/lib.rs:37-80`). This is still an injected seam, not a production composition.

These observations do not close G6 or G10 gates.

## Findings

### P1 — G6-01: Scheduler authorization and tenant-route enforcement are only a hook

`TenantProjectionSchedulerRepository::verify_tenant_route` exists, but `schedule`, `claim_due`, `complete`, `fail`, and `reclaim_stale` do not call it and accept caller-supplied `TenantHoldingBinding` values (`crates/infrastructure/src/tenant_projection_scheduler.rs:695-712`, `714-784`, `851-995`, `1045-1251`). The repository constructor contains only a control-plane `PgPool` (`:691-698`); there is no command context, authoritative RBAC decision, entitlement/module check, or audit context.

The G6 evidence correctly lists authorization and registry resolution as required follow-up (`audits/G6-PRV-005-059/README.md:94-105`), but the current repository can create and claim rows for a syntactically valid, unregistered binding. This fails the contract requirement that scheduling, claiming, replay, rebuilding, and dead-letter operations be authoritative, scoped, and audited.

### P1 — G6-02: No production multi-tenant projector or replacement-read execution is composed

The worker scheduler handler accepts an injected `TenantProjectionExecutor`, but the trait has no database/router behavior and there is no concrete executor (`apps/worker/src/tenant_projection_scheduler.rs:96-112`). The helper registration method requires callers to inject a source, executor, and one fixed binding (`apps/worker/src/lib.rs:1602-1627`). The executable instead always constructs the default registry (`apps/worker/src/main.rs:8-15`), whose registry does not register this handler (`apps/worker/src/lib.rs:1531-1560`).

Therefore no deployed worker enumerates tenant jobs, resolves the tenant write/read pair, executes a real projector, advances checkpoints, rebuilds a replacement read database, or activates a replacement safely. The ledger also records this as open (`EXECUTION_LEDGER.md:3338-3340`, `3398-3401`). The G6 gate must remain open.

### P1 — G6-03: Replaying a sealed claim key can re-execute completed work

For a sealed claim batch, `claim_due` returns every historical claim for the key (`crates/infrastructure/src/tenant_projection_scheduler.rs:888-894`). `claims_for_key` joins all claim states without filtering or returning the claim state (`:1276-1300`), and `ProjectionClaimRecord` exposes only a boolean `replayed` marker. The worker handler executes every returned record without checking `replayed`, claim state, or terminal job state (`apps/worker/src/tenant_projection_scheduler.rs:192-205`).

A retry using an already-completed key can therefore invoke the projector again; a retry using a stale/failed key can invoke it with an already-invalid lease and only discover the error after the side effect. The required idempotency behavior needs an explicit replay result/receipt that is not executable work, or a well-defined active-claim recovery path with tests for completed, failed, stale, and partial-crash retries.

### P1 — G6-04: Claim/job/rebuild state transitions are not consistently atomic

`renew_lease` updates the job and then separately updates the claim outside a transaction, ignores the claim update row count, and can return success with divergent job and claim leases (`crates/infrastructure/src/tenant_projection_scheduler.rs:997-1043`). `complete` uses a transaction but also ignores whether the active claim row was updated (`:1073-1083`). `fail` updates the job, claim, and rebuild row through separate operations without one transaction (`:1124-1177`).

This can leave a completed/retryable job with an active claim, a renewed job with a stale claim lease, or a dead-letter job with incomplete claim history after an intermediate database failure. Add one transaction, require exactly one affected claim row, and test injected failures between each state write.

### P1 — G10-01: The read-side `get` path targets a table that is only migrated on the write database

`SqlxMarketingDeliveryStore::get` opens `read_pool` and selects `erpx_tenant.marketing_deliveries` (`crates/infrastructure/src/marketing_delivery_persistence.rs:310-328`). Migration 0026 creates that table only in `migrations_tenant_write` (`crates/migration/migrations_tenant_write/0026_marketing_delivery.sql:6-29`). The current tenant read Marketing migration creates profiles, consents, suppressions, audiences, and memberships, but no delivery table or delivery projection (`crates/migration/migrations_tenant_read/0018_marketing.sql:5-69`).

On a clean physically separated read database, `get` fails with a missing relation. This is both a runtime defect and a CQRS boundary violation: either add a rebuildable delivery read projection and project it, or make this method an explicitly write-side authoritative read and keep it off the read pool. A clean two-database integration test is required.

### P1 — G10-02: Delivery callback persistence bypasses provider-reference binding and authentication

The domain provides the safer `mark_delivered_for_provider(provider_message_ref, at)` method (`crates/domain/src/marketing_delivery.rs:585-597`), but infrastructure calls the unbound `mark_delivered(at)` method (`crates/infrastructure/src/marketing_delivery_persistence.rs:274-305`). The public persistence method accepts only delivery ID, expected version, and a caller-supplied timestamp; it does not require the provider message reference, callback event ID, signature verification result, provider/account binding, or authenticated tenant callback context.

The worker `MarketingDeliverySource` trait has no delivery-callback operation (`apps/worker/src/marketing_delivery.rs:36-57`), and no server callback route uses this method. Thus an authenticated provider callback is absent, and any future caller with access to the scoped store could mark a `provider_accepted` dispatch delivered without proving that the provider accepted that exact message. This contradicts the current packet evidence, which says callbacks are bound to the provider reference (`audits/G10-MKT-005-060/README.md:14-16`).

### P1 — G10-03: Worker leases can expire during provider submission, and the timestamp is caller-controlled

The handler claims with one `now`, calls the provider without a renewal or deadline, and records the result later with a new `Utc::now()` (`apps/worker/src/marketing_delivery.rs:162-207`). It does not check `claim.worker_id` or `claim.claim_until` before submission. The store validates the lease using the supplied `at` value (`crates/infrastructure/src/marketing_delivery_persistence.rs:248-255`), not database time or a trusted callback time.

A slow provider can outlive the lease while another worker claims the same dispatch. A caller that can reach the persistence method can also backdate `at` to pass the lease check. The implementation needs lease renewal/maximum provider-call duration, authoritative time checks, explicit stale-claim handling, and provider-side idempotency/callback reconciliation tests.

### P1 — G10-04: Marketing delivery is not constructed by the production worker executable

The default registry does not include Marketing delivery (`apps/worker/src/lib.rs:1531-1560`). The alternate registration method requires a caller-provided source and provider (`:1578-1599`), but `apps/worker/src/main.rs:8-15` never constructs or invokes it. The provider trait is only a classification boundary with no production channel adapter (`apps/worker/src/marketing_delivery.rs:93-101`).

The current behavior is fail-closed, which is desirable, but it means no email/SMS/social/push/webhook delivery claims or provider calls occur in the runtime. The evidence correctly lists durable worker composition, providers, and authenticated callbacks as open (`audits/G10-MKT-005-060:48-54`). No G10 delivery gate can be inferred from passing injected-handler tests.

### P2 — G6/G10-05: Database constraints do not encode all claim/scope invariants

G6 claim rows carry tenant/holding/worker fields but have only individual foreign keys to `claim_batches` and `jobs`; there is no composite foreign key tying those fields to the referenced rows (`crates/migration/migrations/0025_tenant_projection_scheduler.sql:61-94`). The control migration also has no RLS policy; this is acceptable only if the control database is strictly platform/worker-only, which the current unauthenticated repository does not prove.

G10 permits arbitrary combinations of `claim_id`, `claimed_by`, and `claim_until` for any state, and its idempotency table has a primary key on `idempotency_key` alone with no foreign key to the delivery row (`crates/migration/migrations_tenant_write/0026_marketing_delivery.sql:21-36`). The repository maintains these relationships on its normal path, but direct SQL, recovery tooling, or a future path can create orphaned/mismatched claim state. Add database checks/composite references where the physical model permits, and add tamper/isolation tests.

### P2 — G6-06: Scheduler payload/error redaction is not a complete bounded storage boundary

Scheduler payload validation requires a JSON object but does not impose a size limit (`crates/infrastructure/src/tenant_projection_scheduler.rs:270-280`); migration `payload JSONB` and `last_error TEXT` are unbounded (`crates/migration/migrations/0025_tenant_projection_scheduler.sql:30-37`). `redact_error` performs a small literal replacement set (`crates/infrastructure/src/tenant_projection_scheduler.rs:1529-1540`), while `Database` errors preserve `sqlx::Error::to_string()` (`:49-52`) and route errors are copied into that variant (`:703-712`). Worker-facing errors are redacted, but the repository/API boundary itself is not.

Add bounded payload/error checks, structured safe error classes, and secret-safe serialization tests. Do not treat the current pure redaction test as proof that arbitrary scheduler payloads or database errors are safe.

### P2 — G14-07: Integrated evidence is stale or overstates one callback invariant

`G10-MKT-005-060` reports 31 worker tests passed (`audits/G10-MKT-005-060/README.md:33-43`), while the current worker run after scheduler-handler integration is 34 passed and 1 ignored. More importantly, its provider-reference callback statement is contradicted by the infrastructure call noted in P1-G10-02. The G6 packet evidence also says no worker code changed (`audits/G6-PRV-005-059/README.md:27-35`), while current worker code now contains and exports a scheduler handler (`apps/worker/src/lib.rs:37-80`). Those packet statements can be historically scoped, but a follow-up integration handoff and corrected evidence are required before QA can rely on them.

## Verification executed

All commands were read-only and run from `erpx/`; no disposable PostgreSQL test was enabled or modified.

```text
cargo fmt --all --check                              PASS
cargo clippy -p erpx-infrastructure --all-targets -- -D warnings  PASS
cargo test -p erpx-infrastructure --lib              PASS: 89 passed, 4 ignored
cargo test -p erpx-migration                          PASS: 6 passed, 0 failed
cargo clippy -p erpx-worker --all-targets -- -D warnings          PASS
cargo test -p erpx-worker --lib                       PASS: 34 passed, 1 ignored
cargo clippy -p erpx-domain --all-targets -- -D warnings          PASS
cargo test -p erpx-domain marketing_delivery          PASS: 8 passed, 114 filtered
```

Passing unit/clippy results establish compile and pure-state health only. They do not verify clean two-database migrations, RLS isolation, concurrent claims, crash atomicity, provider signature verification, route authorization, callback idempotency, replacement-read activation, or executable runtime composition.

## Recommended next integration order

1. Correct G10 read/write separation and add disposable PostgreSQL tests for clean write/read databases, forced RLS, concurrent enqueue/claim, consent/suppression races, and optimistic transitions.
2. Implement the authenticated provider callback boundary: raw-body/signature verification, provider-account and tenant binding, provider message reference, callback-event idempotency, and a worker/infrastructure method that calls `mark_delivered_for_provider`.
3. Make G6 scheduler APIs authoritative: resolve bindings through the control registry/router, require platform RBAC/entitlement/high-risk approval where applicable, and persist audit context.
4. Make scheduler claim/job/claim-history/rebuild writes atomic and define non-executable sealed-key replay receipts; add crash-injection and concurrent-worker tests.
5. Compose real tenant enumeration, router-backed write/read projector executors, lease renewal, replacement-read rebuild/switch, and worker executable registration. Compose Marketing provider adapters only after the same route/secret boundary is present.
6. Refresh packet evidence and ledger counts, then run clean/upgrade migrations, cross-tenant RLS/security tests, full workspace verification, and the G6/G10 requirement audit. Neither gate is complete from the current evidence.
