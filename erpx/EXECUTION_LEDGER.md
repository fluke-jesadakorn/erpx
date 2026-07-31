# ERPX Execution Ledger

This ledger is append-oriented. Each integration wave records independent evidence, known gaps, security review, and rollback state. The persistent ERPX goal remains active until the G14 requirement-by-requirement audit is complete.

## Current status

| Field | Value |
|---|---|
| Persistent objective | Complete ERPX migration under `MASTER_PLAN.md` while preserving Folio until verified write retirement |
| Current wave | G8-FIN-031 dedicated Finance close-review read projection and G11-PEO-002 HR/payroll write persistence integrated; all G1-G14 gates remain open |
| Last integrated packet | G8-FIN-031 typed tenant read projection, contiguous projector/rebuild worker, and read-only adapter transport; G11-PEO-002 is independently integrated in the same wave |
| Next dependency-ready packet | Add live disposable-database projection evidence and HR command/read worker composition, then continue CRM scope/freshness and remaining Finance/ERP contexts; no command-owner or Folio write-retirement change is authorized |
| Folio command owner | Folio (unchanged) |
| ERPX production source of truth | Not activated |
| Current ERPX server persistence | SQLx control write/read pools, identity/session/recovery/device storage, support persistence and bounded ticket queue reads, commercial/billing/Stripe inbox, authorization/SoD, physical tenant DB/RLS provisioning, validated scoped tenant routing, Operations tenant write/read persistence and production scheduler composition, Marketing delivery/callback inbox persistence, and a MinIO presign adapter are wired; support capability issuance is not yet atomic with its command/outbox, live storage/backup operations, all tenant-handler enforcement, source producers, provider-specific Marketing adapters, and production command ownership remain open |
| Folio dev server | Running on port 3004 |
| Goal status | Active |

## Entry 2026-07-24 — G0-001 current-state audit

**Owner:** Main agent  
**Dependencies:** User-supplied acceptance contract and repository access  
**Allowed paths:** Read-only inspection of repository/runtime; audit document write  
**Forbidden paths:** Destructive DB/runtime operations; reset/checkout of user work

### Delivered evidence

- Created `erpx/audits/G0-001_CURRENT_STATE_AUDIT.md`.
- Inventoried Folio UI and API route families, source/live database schemas, platform tables, legacy roles/permissions, transition vocabularies, Rust crates/proto/services, Dioxus features, migration scripts, and runtime processes.
- Confirmed Folio remains operational on `3004` and the prototype ERPX server is on `50051`.
- Confirmed local PostgreSQL and MinIO availability without exposing secret values.
- Confirmed ignored runtime files and `.runtime/n8n` preservation.

### Baseline evidence

| Command | Result |
|---|---|
| `bun run lint` | PASS; 4 warnings, 0 errors in `infra/scripts/start-n8n.js` |
| `bunx tsc --noEmit` | PASS |
| `cargo fmt --all --check` | PASS |
| `cargo clippy --workspace --all-targets -- -D warnings` | PASS |
| `cargo test --workspace` | PASS; 11 existing tests executed |
| `cargo check -p erpx-console --no-default-features --features web` | PASS |
| `cargo check -p erpx-console --no-default-features --features desktop` | PASS |

### Findings that remain open

- Production ERPX persistence is absent; in-memory server storage must be removed from the production graph.
- The current SQL platform migration is co-located in `folio_db`; it is not the required physical control write/read separation.
- Shared context/error/tenant/audit/idempotency contracts are not frozen.
- Support transition rules are incomplete against the required lifecycle.
- Identity, Stripe, provisioning, worker/projectors, all tenant modules, migration/cutover, and target CI are incomplete.

### Rollback

No mutation or destructive operation occurred. Existing Folio changes and runtime state are untouched.

## Entry 2026-07-24 — G0-002 master plan and ADRs

**Owner:** Main agent  
**Dependencies:** G0-001 audit  
**Status:** Complete

### Delivered

- Created `erpx/MASTER_PLAN.md` with architecture, waves, dependency gates, packet protocol, invariants, and full completion rule.
- Created ADR-001 through ADR-010 for tenant isolation, CQRS, no dual writes, expiring RBAC, global-read auditing, support capabilities, Stripe authority, Operations boundaries, Marketing/CRM boundaries, and Dioxus targets.
- This ledger is the integration record for all future packets.

### Verification

The same baseline command set remains green. No application code or database state was changed by this documentation packet.

### Rollback

Delete only the newly added planning/ADR/audit documents if explicitly requested; do not revert unrelated user changes.

## Dependency-ready queue

| Packet | Owner | Ready when | Status |
|---|---|---|---|
| G0-003 shared contracts | Main | G0-002 complete | Complete |
| G0-004 deterministic fixtures | Main + AG-MIG review | G0-003 compile-stable | Complete |
| G1-001 SQLx repositories | AG-FND | G0 complete and fixture schema decisions | Integrated; read/write split verified |
| G1-005 Dioxus foundation | AG-UI | G0-003 compatibility metadata | Shared shell integrated; authenticated session/client storage pending |
| G2-001 identity | AG-IAM + Main integration | G1-004 auth boundary contracts | Integrated; WebAuthn ceremony/recovery workflows pending |
| G2-002 role lifecycle | AG-IAM | G0-003 plus identity principal model | Integrated; full effective-access intersection pending |

Packets for G3 onward remain held until tenant routing, identity, RBAC, and entitlement enforcement pass their gates.

## Entry 2026-07-24 — G1-001 SQLx control repositories and foundation review

**Owners:** AG-FND packet; Main integration and independent verification  
**Dependencies:** G0-001 through G0-004  
**Allowed paths:** `erpx/crates/infrastructure`, `erpx/crates/migration`, `erpx/apps/server` persistence composition, `erpx/apps/worker`, infrastructure integration tests  
**Forbidden paths:** shared contract/protobuf definitions, Folio application routes/database, environment secrets, and unrelated business modules

### Handoff and delivered behavior

- Added `ControlPlaneDatabaseConfig` and distinct SQLx PostgreSQL write/read pools with bounded pool settings.
- Added an embedded SQLx control-plane migration under `erpx/crates/migration/migrations/0001_control_plane.sql` for aggregate snapshots, idempotency, customer projection, support aggregates, role assignments, audit events, outbox events, and projection checkpoints.
- Added `SqlxControlPlane` implementations for customer reads, support persistence, role persistence, and audit persistence with typed persistence errors and optimistic version checks.
- Added an atomic `ControlPlaneWriteTransaction` that persists aggregate snapshots and outbox envelopes in one transaction, including event version, aggregate version, tenant scope, actor, correlation, causation, idempotency, timestamps, and payload.
- Added outbox claiming with `FOR UPDATE SKIP LOCKED`, aggregate ordering, claim ownership, retry/backoff metadata, dead-lettering, replay, and projection checkpoints.
- Added worker and projection handler interfaces plus replacement projection rebuild primitives.
- Wired the Tonic server composition root to require the SQLx control write/read URLs and apply migrations to both databases. Production server code no longer imports the in-memory adapter; in-memory support is test-only.
- Added `erpx-worker` as the worker runtime surface and an isolated SQLx integration test.

### Independent verification

| Command/evidence | Result |
|---|---|
| `cargo fmt --all --check` | PASS |
| `cargo clippy --workspace --all-targets -- -D warnings` | PASS |
| `cargo test --workspace` | PASS; domain 14, infrastructure 4, migration 3, UI 8, remaining crates zero-test suites |
| `cargo check -p erpx-console --no-default-features --features web` | PASS |
| `cargo check -p erpx-console --no-default-features --features desktop` | PASS |
| `cargo test -p erpx-infrastructure --test sqlx_control_plane -- --ignored --nocapture` | PASS on a disposable, physically distinct PostgreSQL write/read database pair; databases dropped after the run |
| `bun run lint` | PASS; 4 pre-existing warnings in `infra/scripts/start-n8n.js`, 0 errors |
| `bunx tsc --noEmit` | PASS |

The isolated test proves migration application, atomic snapshot/outbox commit, optimistic conflict detection, idempotency replay/completion, per-aggregate claim ordering, and customer read projection round-trip. The evidence test is `erpx/crates/infrastructure/tests/sqlx_control_plane.rs`.

### Security and rollback review

- No tenant credentials are sent to the Dioxus client.
- Server startup rejects missing or equal control write/read URLs; no production fallback to in-memory state remains.
- A dead-lettered predecessor blocks later events for the same aggregate until replay, preserving ordering.
- Temporary verification databases were explicitly removed; Folio’s `folio_db` and runtime services were not modified.

### Open requirements and integration order

This is a foundation packet, not the G1 gate. `SupportService`, role, and customer application mutations still need to execute through a single authoritative command unit of work that couples state, audit, and outbox. The worker has safe infrastructure primitives but no production composition loop or context projectors; replacement read-database switching, tenant routing, auth/session/scope/entitlement/support interceptors, health/readiness/metrics, and stable error mapping remain open. The next integration order is G1-002 → G1-003 → G1-004, with G1-005 authenticated route integration alongside them.

## Entry 2026-07-24 — G1-002/G1-003 atomic commands and worker primitives

**Owner:** AG-FND packet; Main integration and independent verification  
**Dependencies:** G1-001, G0-003  
**Allowed paths:** `erpx/crates/application`, `erpx/crates/infrastructure`, `erpx/crates/migration`, `erpx/apps/worker`  
**Forbidden paths:** contracts/protobuf, UI, Folio, environment files, and business-context modules

### Delivered behavior

- Added `CommandMutation`, `CommandReceipt`, `CommandUnitOfWork`, and `ApplicationCommandService` ports in the application layer.
- Implemented `SqlxControlPlane::execute_command` as a transaction that validates aggregate identity, reserves idempotency, persists the snapshot, appends the complete outbox envelope, appends audit, stores a replayable receipt, and commits or rolls back as one unit.
- Added receipt columns and projection event receipt tables to the SQLx migration.
- Added worker execution over claimed outbox events, retry and dead-letter paths, replay support, projection event claim/completion/release, ordered rebuild traversal, and checkpoint handling.
- Kept in-memory adapters behind `#[cfg(test)]` and did not add unknown-event acknowledgement behavior.
- Added `AuthoritativeSupportService` and moved server ticket/session create/request/approve/revoke commands onto the validated domain `CommandContext` and `CommandUnitOfWork`; the SQLx integration test now verifies a command-backed support ticket can be reloaded from its aggregate snapshot during projection transition.
- Added `AuthoritativeRoleLifecycleService` and moved role renewal onto the same command unit of work, with immutable renewed assignment snapshots and outbox events. The existing role read path can resolve command-backed assignments during projection transition.

### Verification

- `cargo test --workspace` — PASS; command forwarding test plus domain, infrastructure, migration, server-boundary, and UI suites.
- `cargo clippy --workspace --all-targets -- -D warnings` — PASS after final boundary/lint fixes.
- Disposable two-database `sqlx_control_plane` test — PASS; it now exercises command commit/replay, snapshot/outbox atomicity, optimistic conflict, ordered worker publication, and read projection.

### Remaining gate items

The worker executable now has a real runtime loop, environment configuration, startup migrations, and a fail-closed binary entry point. A concrete Customer 360 projector is now wired to read-model rebuild/checkpoint/receipt handling. External publishing, migration jobs, durable scheduling, the remaining context projectors, and the physical replacement read-database switch remain incomplete. Remaining legacy repository mutations must be migrated to `CommandUnitOfWork` before the G1 gate passes. No G1 completion claim is made.

## Entry 2026-07-24 — G1-003 worker runtime composition follow-up

**Owner:** Main agent after AG-FND follow-up shutdown; independent integration review
**Dependencies:** G1-002 transaction/outbox primitives
**Allowed paths:** `erpx/apps/worker`, worker dependency declarations, worker unit tests
**Forbidden paths:** shared contracts/protobuf, Folio, identity, business modules, environment files, and tenant databases

### Delivered behavior

- Added `WorkerRuntimeConfig::from_env` with bounded batch, claim, polling, and retry settings and a stable worker identity override.
- Added `WorkerRuntime::connect`, which requires distinct control write/read URLs, applies the embedded control migration to both pools, and composes the persisted `OutboxWorker`.
- Added reusable `run_until_shutdown`, `run_until_stopped`, `rebuild_projection`, and rebuild-then-activate `rebuild_and_switch` APIs. Activation is delegated to a context-owned `ProjectionReadDatabaseSwitcher` and occurs only after rebuild success.
- Added `apps/worker/src/main.rs` as an executable `run` entry point. Until business handlers are registered it uses a fail-closed handler that never acknowledges unknown events.

### Independent verification

| Command/evidence | Result |
|---|---|
| `cargo check -p erpx-worker` | PASS |
| `cargo test -p erpx-worker` | PASS; runtime guard/config test |
| `cargo fmt --all --check` | PASS |
| `cargo clippy --workspace --all-targets -- -D warnings` | PASS |
| `cargo test --workspace` | PASS; 5 infrastructure, 3 migration, 14 domain, 8 UI, 4 server-boundary, and 1 worker unit test plus crate/doc suites |
| Disposable two-database SQLx integration test | PASS; command/outbox atomicity, ordering, idempotency, worker publication, and projection checkpoint evidence; both databases explicitly dropped |
| Disposable server boot | PASS; `/healthz`, `/readyz`, and `/metrics` returned 200/expected output; process stopped and both databases explicitly dropped |
| Folio `bun run lint` | PASS; 4 existing warnings, 0 errors |
| Folio `bunx tsc --noEmit` | PASS |
| Dioxus web/desktop/mobile feature checks | PASS |

### Security and rollback review

- The default executable cannot acknowledge an event without an installed business handler; the existing outbox claim ownership, bounded retry, ordering, and dead-letter rules remain authoritative.
- No production database or Folio runtime state was modified. Disposable verification databases were named explicitly and removed after testing.
- The switch interface does not itself provision or swap a physical database; AG-PRV/G6 must supply the tenant/control read-store implementation before that requirement can be accepted.

### Open integration order

1. Migrate production mutations in customer/support/role services to `CommandUnitOfWork`.
2. Add context-specific event handlers and projection read models.
3. Add external publisher, migration job scheduling, and physical replacement read-store activation.
4. Complete identity-backed server authorization before allowing G2.

## Entry 2026-07-24 — G1-003 Customer 360 projector

**Owner:** Main agent
**Dependencies:** G1-002 atomic events; G1-003 worker runtime and projection receipts
**Allowed paths:** `erpx/apps/worker`, control read-model repository adapter, worker integration tests
**Forbidden paths:** Folio, identity, business modules other than the Customer 360 projection, shared protobuf contracts, and secrets

### Delivered behavior

- Added `Customer360Projection` with strict projection-name validation, clear/rebuild behavior, customer event deserialization, and read-model upsert.
- Added `Customer360OutboxHandler`, which routes claimed outbox events through `project_once`; projection event receipts and checkpoints make replay/idempotency explicit before the outbox event is acknowledged.
- Added `WorkerRuntime::customer_360_handler` and executable handler selection through `ERPX_WORKER_HANDLER=customer_360`; unsupported handler selection fails before claiming events.

### Independent verification

- `cargo check -p erpx-worker --tests` — PASS.
- `cargo test -p erpx-worker --test customer_360 -- --ignored --nocapture` — PASS against disposable, physically distinct PostgreSQL databases; both databases dropped afterward.
- `cargo fmt --all --check` — PASS.
- `cargo clippy --workspace --all-targets -- -D warnings` — PASS.
- `cargo test --workspace` — PASS; normal suite includes the worker integration test as ignored because it requires disposable databases.
- Dioxus web/desktop/mobile checks, `bun run lint`, and `bunx tsc --noEmit` — PASS; Folio lint retains only the four existing n8n-script warnings.

### Remaining gate items

The projector covers only Customer 360. External event publication, all other context projectors, durable scheduled jobs, physical replacement read-database provisioning/switching, and target Android/iOS CI evidence remain open.

## Entry 2026-07-24 — G1-004 server boundary foundation

**Owner:** Main agent  
**Dependencies:** G0-003, G1-001  
**Allowed paths:** `erpx/apps/server`, additive protobuf compatibility field, `erpx/apps/console` request context wiring, infrastructure readiness probe  
**Forbidden paths:** Folio retirement, identity/RBAC implementation, business modules, and secret files

### Delivered behavior

- Added mandatory versioned protobuf query/command context validation before RPC handler logic.
- Validated actor attribution against request metadata, session attribution against `x-erpx-session`, tenant-scope presence, audit purpose, audit/request correlation, optional correlation metadata, idempotency, command IDs, and query consistency.
- Added a context field to `ListExpiringRolesRequest` additively, preserving existing field numbers.
- Added stable gRPC error mapping for not-found, forbidden, conflict, failed-precondition, repository/internal, and invalid request classes.
- Added one-process HTTP `/healthz`, `/readyz`, and `/metrics` listener, backed by both control pools for readiness and redacted readiness failures.
- Updated the console Customer 360 query to send versioned context and provisional actor/session metadata.

### Evidence

- `cargo test -p erpx-server` — PASS; 4 boundary tests.
- Disposable server boot with fresh write/read databases — PASS: `/healthz` 200, `/readyz` 200, `/metrics` 200; process stopped and databases removed.
- `cargo check -p erpx-server` — PASS.

### Security status

This remains a boundary foundation, not a passed G1/G2 gate. Signed short-lived access tokens are now required at transport, and every RPC performs a write-side identity session lookup. Command handlers for support and role mutations also perform recursive write-side role permission/scope checks. Tenant routing, full effective-access intersection (commercial entitlement, billing, SoD, support capability, represented-user authority), Stripe raw-body webhook handling, redaction middleware, tracing correlation, and rate limiting remain open. The server must not be treated as production-authorized until those dependencies pass.

## Entry 2026-07-24 — G1-005 shared Dioxus foundation handoff

**Owner:** AG-UI packet; Main integration and independent verification  
**Dependencies:** G0-003 compatibility metadata  
**Allowed paths:** `erpx/crates/ui`, `erpx/apps/console`  
**Forbidden paths:** infrastructure, server, worker, domain/protobuf contracts, Folio, migrations, and environment files

### Delivered behavior

- Added the shared `erpx-ui` crate with responsive navigation shell, route normalization, Thai/English locale context, theme tokens, forms and validation, filter/table/dialog primitives, and explicit loading, empty, denied, stale, read-only, conflict, offline, and error states.
- Wired the existing Customer 360 console to the shared shell and preserved the existing contract-version/security note.
- Added web, desktop, and mobile feature compilation coverage; no authentication tokens or authorization logic were introduced in the presentation crate.

### Verification and remaining work

- `cargo test -p erpx-ui` — PASS; 8 tests.
- `cargo check -p erpx-console --no-default-features --features web` — PASS.
- `cargo check -p erpx-console --no-default-features --features desktop` — PASS.
- Mobile feature check — PASS in the AG-UI handoff; Android/iOS target CI evidence remains required.
- Authenticated route enum/router integration, secure local sessions, deep links, focus trapping, offline drafts, file/camera flows, and route-specific pages remain open. This packet is therefore recorded as delivered foundation scope, not a passed G1 product gate.

## Entry 2026-07-24 — G1-005 typed route and session-state integration

**Owner:** AG-UI packet; Main integration review
**Dependencies:** G0-003 contracts and shared UI foundation
**Allowed paths:** `erpx/crates/ui`, `erpx/apps/console`
**Forbidden paths:** server, protobuf/contracts, infrastructure, worker, identity, Folio, migrations, and environment files

### Delivered behavior

- Added a typed platform/Customer 360 route registry covering the required existing route set, opaque customer identifiers, canonical deep-link normalization, and traversal/control-character rejection.
- Added explicit route state types for unauthenticated, expired, revoked, read-only, offline, denied, stale, and conflict states.
- Added safe session metadata, scope selector options, and persistent support/impersonation/read-only banners that contain no bearer/refresh tokens or credentials.
- Wired the console to resolve the typed route state while preserving the existing Customer 360 data boundary.

### Verification

- `cargo test -p erpx-ui` — PASS; 12 tests.
- `cargo check -p erpx-ui -p erpx-console --no-default-features --features web` — PASS.
- Desktop and mobile feature checks remain required in the next integration wave; secure token storage, real auth/session integration, deep-link navigation side effects, and full G12 target journeys remain open.

## Entry 2026-07-24 — G0-003 shared contract freeze

**Owner:** Main agent  
**Dependencies:** G0-002  
**Status:** Complete after independent compile/test verification

### Delivered

- Added `erpx/crates/domain/src/contracts.rs` with `TenantScope` containment, `ActorAttribution`, `AuditContext`, `CommandContext`, `QueryContext`, `QueryConsistency`, `ProjectionFreshness`, `EventEnvelope`, stable `ErrorCode`/`ErrorContext`, contract version `1`, idempotency/purpose validation, and correlated audit validation.
- Added `erpx.common.v1` protobuf context/error envelopes and carried query/command context fields into platform RPC request messages without reusing existing field numbers.
- Added `erpx/CONTRACT_POLICY.md` with additive protobuf/versioning rules, reserved-field policy, event/version compatibility, idempotency, consistency, stable errors, and support-context requirements.
- Fixed and tested a branch-restricted subtree containment leak: a branch-restricted subtree cannot authorize a whole company.

### Verification

- `cargo fmt --all --check` — PASS.
- `cargo clippy --workspace --all-targets -- -D warnings` — PASS.
- `cargo test --workspace` — PASS: 17 tests (14 domain, 1 infrastructure, 2 migration; other crates have zero tests).
- `cargo check -p erpx-server` — PASS.
- `cargo check -p erpx-console --no-default-features --features web` — PASS.
- `cargo check -p erpx-console --no-default-features --features desktop` — PASS.

### Security/remaining integration note

The contracts are now available to transports, but G1-004 still must make context presence, identity/session validation, tenant scope, authoritative RBAC, entitlement, support capability, and stable error mapping mandatory at the server boundary. The current prototype server remains in-memory and accepts an actor metadata string; this is an explicit G1/G2 gap, not an accepted security decision.

## Entry 2026-07-24 — G0-004 deterministic migration fixtures

**Owner:** Main agent  
**Dependencies:** G0-003  
**Status:** Complete

### Delivered

- Added `erpx/crates/migration` to the workspace.
- Added `erpx/fixtures/migration/deterministic.json` with two holdings, multiple companies/branches, active/expired parent/child assignments, customer/commercial/support/Operations, sales/procurement/inventory, balanced journals, marketing consent/suppression/attribution, assets/maintenance, employees/payroll/leave, and legal documents.
- Added typed fixture loading, deterministic validation, organization depth checks, active/expired role checks, non-empty domain checks, balanced journal checks, and holding-reference checks.

### Verification

- `cargo test -p erpx-migration -p erpx-domain -p erpx-proto` — PASS: 16 tests (14 domain, 2 migration).
- Full workspace checks listed in the G0-003 entry remain green after the final contract/module fixes.

### Rollback

The fixture crate and data are additive and can be removed as one packet if explicitly requested; no Folio database/runtime state was modified.

## Entry 2026-07-24 — G2-001 identity and recovery primitives

**Owner:** AG-IAM packet was closed after an incomplete handoff; Main agent completed and independently verified the bounded crate
**Dependencies:** G0-003 contracts; G1 transport context foundation
**Allowed paths:** `erpx/crates/identity`, identity-only migration directory, identity tests
**Forbidden paths:** server authorization wiring, shared contracts/protobuf, worker, UI, Folio, and environment secrets

### Delivered behavior

- Added the `erpx-identity` crate with Argon2id password hashing and verification, redacted secret/hash types, one-time hashed recovery codes, AES-256-GCM TOTP secret envelopes with key references and principal-bound associated data, expiring/replay-protected WebAuthn challenge primitives, passkey provider interfaces, device records, and rotating hashed refresh sessions.
- Enforced a five-minute maximum access-token lifetime with exact half-open expiry checks and HMAC integrity.
- Added `crates/identity/migrations/0001_identity.sql` outside the control migration directory. The identity schema is write-side only and is not accidentally applied to control read databases.

### Independent verification

- `cargo test -p erpx-identity` — PASS; 6 security primitive tests.
- `cargo clippy -p erpx-identity --all-targets -- -D warnings` — PASS.
- `cargo fmt --all --check` — PASS.
- Disposable two-database identity migration test — PASS; identity schema exists only on the explicit write database; both databases dropped afterward.
- `cargo clippy --workspace --all-targets -- -D warnings` — PASS.
- `cargo test --workspace` — PASS; control migrator remains independently valid and identity tests pass.

### Security and remaining integration

Raw WebAuthn browser ceremony verification is intentionally an explicit provider boundary until a pinned production provider is selected. The primitive packet itself did not include identity repository persistence, HTTP authentication endpoints, session revocation lookup, or server interceptor integration; those are recorded in the later G2-001 integration entry. Recovery workflows, approver identity resolution, and Android/iOS secure storage remain open. No G2 completion claim is made.

## Entry 2026-07-24 — G2-001 persisted identity/session integration

**Owner:** Main agent after identity primitive handoff; independently verified with disposable PostgreSQL and server boot
**Dependencies:** G0 contracts, G1 SQLx write/read pools, bounded identity primitives
**Allowed paths:** `erpx/crates/identity`, `erpx/apps/server`, identity integration evidence
**Forbidden paths:** Folio retirement, tenant business modules, secret files, and control read identity schema

### Delivered behavior

- Added `SqlxIdentityStore` for write-side principal/password lookup, device creation, session insertion, access-token authorization, refresh rotation, refresh-token reuse revocation, per-session revocation, and principal-wide session revocation.
- Added safe stored-digest/token-hash constructors and persisted-session reconstruction with validation.
- Added `/auth/login`, `/auth/refresh`, and `/auth/revoke` to the server HTTP listener. Invalid JSON, expired/revoked sessions, wrong-device state, and database failures return stable non-secret error responses.
- Added asynchronous identity authorization to every Tonic RPC handler while retaining synchronous signed-token/actor/session transport rejection.
- Kept `erpx_identity` migration outside the control migrator and applied it only to `pools.write`.

### Independent verification

- `cargo test -p erpx-identity` — PASS; 6 primitive tests.
- `cargo clippy -p erpx-identity --all-targets -- -D warnings` — PASS.
- `identity_store_persists_and_revokes_sessions` — PASS on disposable `erpx_identity_store_20260724`; database dropped afterward.
- `identity_schema_is_write_side_only` — PASS on disposable distinct write/read databases; both dropped afterward.
- `cargo check -p erpx-server` and server clippy — PASS.
- Disposable server boot — `/healthz` and `/readyz` returned 200; login, refresh, and revoke endpoint flow passed without exposing token values; databases and process were removed/stopped afterward.

### Security and remaining integration

No registration endpoint exists. Passwords and refresh values are not logged or persisted raw. The server still lacks a pinned WebAuthn ceremony provider, full recovery/TOTP/device-management workflows, rate limiting, secure platform-local token storage, and complete commercial/support/SoD effective-access intersection.

## Entry 2026-07-24 — G2-002 universal immutable expiring role lifecycle

**Owner:** AG-IAM packet; Main integration and independent verification
**Dependencies:** G0 contracts and G2-001 identity primitives
**Allowed paths:** `erpx/crates/domain/src/access.rs`, `erpx/crates/application/src/lib.rs`, `erpx/audits/G2-002/evidence/README.md`
**Forbidden paths:** shared contracts/protobuf, migrations, server transport, infrastructure, Folio, and unrelated business modules

### Delivered behavior

- Enforced finite role windows and exact half-open activity (`valid_from <= at < valid_until`); the maximum timestamp sentinel is rejected.
- Enforced parent identity, start/end containment, scope containment, authority containment, permission containment, and active-parent checks.
- Enforced distinct two-person approval for Platform Root, Administrator, and High risk tiers.
- Made renewal immutable: new UUID, checked version increment, no backdating, validated parent chain, and unchanged source assignment.
- Added authoritative command-backed grant and renewal methods and parent revalidation in the legacy lifecycle service.

### Independent verification

- `cargo test -p erpx-domain -p erpx-application` — PASS; domain 20, application 4.
- `cargo clippy -p erpx-domain -p erpx-application --all-targets -- -D warnings` — PASS.
- `cargo fmt --all --check` — PASS after integration.
- The focused evidence file is `audits/G2-002/evidence/README.md`.

### Remaining integration

Persistent revocation/review queues, recursive server-backed role loading, approver identity/role verification, recovery workflows, and full universal effective-access intersection remain open. No G2 gate or production command-owner change is claimed.

## Entry 2026-07-24 — G2-003 write-side role authorization slice

**Owner:** Main agent
**Dependencies:** G1 SQLx control write database, G2-001 session authorization, G2-002 role aggregate
**Allowed paths:** `erpx/crates/infrastructure/src/repositories.rs`, `erpx/apps/server/src/main.rs`, role authorization evidence
**Forbidden paths:** Folio, control read identity, Stripe, tenant modules, and environment files

### Delivered behavior

- Added write-side `principal_has_role`, loading role rows and command snapshots and recursively validating active parents, scope, authority, permissions, and cycle absence.
- Added server conversion from contract tenant scopes to role scopes.
- Support ticket/session commands and role renewal now fail closed before application mutation when the authenticated principal lacks the required active role permission in the requested scope.

### Independent verification

- Disposable `sqlx_control_plane` integration test — PASS; active permission allowed and unrelated permission denied, alongside command/outbox/optimistic-concurrency evidence.
- Domain/application focused suites — PASS; 20 domain and 4 application tests.
- Infrastructure/server clippy — PASS with `-D warnings`.

### Remaining boundary

This slice does not close G2-003: billing, module entitlements, approval limits, segregation of duties, support capabilities, represented-user authority, tenant routing, and review/recovery workflows remain to be implemented and integrated.

## Entry 2026-07-24 — G4 support persistence integration

**Owner:** AG-SUP handoff; Main integration and independent verification  
**Dependencies:** G1 SQLx control write migration, G4 support domain aggregates  
**Allowed paths:** `erpx/crates/infrastructure/src/support_persistence.rs`, `erpx/crates/infrastructure/tests/support_persistence.rs`, `erpx/crates/migration/migrations/0005_support_incidents.sql`, G4 evidence  
**Forbidden paths:** Folio, control read schema, identity secrets, unrelated modules, and environment secret values

### Delivered behavior

- Persisted tickets, access requests, incidents, and support capabilities on the control write database with optimistic versions and idempotency hashes.
- Enforced mode duration limits, approved-request prerequisites, immutable capability bindings, hash-only 256-bit capability storage, and immutable revocation decisions with database constraints/triggers.
- Exported the repository through `erpx-infrastructure`; production control migrations now include the support schema.

### Independent evidence

- `cargo fmt --all --check` — PASS.
- `cargo clippy --workspace --all-targets -- -D warnings` — PASS.
- `cargo test --workspace` — PASS.
- `cargo test -p erpx-infrastructure --test support_persistence -- --ignored --nocapture` — PASS on a named disposable PostgreSQL database; dropped after the test.
- Evidence: `audits/G4-001/evidence/README.md`.

### Open gate items

The authoritative server support capability intersection, customer approval identity/role verification, worker expiry/revocation, diagnostics/SLA handling, and support UI remain open. No G4 completion claim is made.

## Entry 2026-07-24 — G5 commercial/billing persistence and Stripe raw-body boundary

**Owner:** AG-BIL domain handoff; Main integration and independent verification  
**Dependencies:** G1 SQLx control write database, G5 commercial domain rules, G1 server HTTP boundary  
**Allowed paths:** `erpx/crates/infrastructure/src/commercial.rs`, `erpx/crates/migration/migrations/0004_commercial_billing.sql`, `erpx/apps/server/src/main.rs`, G5 evidence  
**Forbidden paths:** Folio command ownership, tenant databases, payment secrets, and unpinned external client generation

### Delivered behavior

- Persisted immutable catalog revisions, explicit grandfathered-price migrations, versioned billing lifecycles, and entitlement allocation books.
- Added a durable idempotent Stripe inbox storing the raw request body and parsed event envelope on the control write database.
- Added signature verification over the exact raw request bytes, timestamp tolerance, stable webhook errors, and duplicate-safe HTTP acceptance.

### Independent evidence

- `cargo fmt --all --check` — PASS.
- `cargo clippy --workspace --all-targets -- -D warnings` — PASS.
- `cargo test --workspace` — PASS.
- `cargo test -p erpx-infrastructure --test sqlx_control_plane -- --ignored --nocapture` — PASS on a disposable physically distinct write/read pair; dropped after the test.
- Disposable server boot with migrations 0001–0005 — PASS; health/readiness 200, auth revocation 200→401, Stripe valid 200, duplicate 200, invalid signature 400; databases/process removed afterward.
- Evidence: `audits/G5-002/evidence/README.md`.

### Open gate items

Stripe API client/catalog publication, semantic out-of-order reconciliation, billing worker transitions, allocation-to-entitlement command checks, and full G2 effective-access intersection remain open. No G5 completion claim or ERPX command-owner switch is made.

## Entry 2026-07-24 — G2-003 authoritative commercial and segregation-of-duties slice

**Owner:** Main agent
**Dependencies:** G1 SQLx control write database, G2-001 session authorization, G2-002 role lifecycle, G5 commercial persistence
**Allowed paths:** `erpx/crates/domain/src/access.rs`, `erpx/crates/domain/src/commercial.rs`, `erpx/crates/infrastructure/src/repositories.rs`, `erpx/crates/infrastructure/src/commercial.rs`, `erpx/crates/migration/migrations/0006_authorization.sql`, `erpx/apps/server/src/main.rs`, authorization evidence
**Forbidden paths:** Folio command ownership, tenant databases, client-provided secrets, and unrelated ERP contexts

### Delivered behavior

- Added persisted role approval limits with backward-compatible zero defaults.
- Added persisted SoD decisions with exact scope and half-open validity checks.
- Added recursive write-side effective assignment resolution with parent-chain validation.
- Added persisted billing lifecycle and entitlement-book reads for authorization decisions.
- Replaced the old client-asserted entitlement check with authenticated write-side evaluation of session, role, scope, permission, approval limit, billing state, entitlement, and SoD. Legacy billing/boolean request assertions are ignored.

### Independent verification

- `cargo clippy --workspace --all-targets -- -D warnings` — PASS.
- `cargo test --workspace` — PASS.
- Disposable `sqlx_control_plane` test — PASS; persisted SoD and effective assignment lookup exercised on a physically distinct PostgreSQL pair; databases dropped afterward.

### Remaining boundary

Support capability intersection, represented-user authority, recursive parent-chain depth hardening, billing reconciliation, approval amount transport, review/recovery workflows, and tenant routing remain open. This is not the G2 gate and does not authorize ERPX writes.

## Entry 2026-07-24 — G6-001 provisioning metadata and replacement-read foundation

**Owner:** Main agent
**Dependencies:** G1 SQLx control write/read pools and worker rebuild primitives; G3–G5 control-plane persistence
**Allowed paths:** `erpx/crates/infrastructure/src/provisioning.rs`, `erpx/crates/infrastructure/src/lib.rs`, `erpx/crates/infrastructure/tests/provisioning.rs`, `erpx/crates/migration/migrations/0007_provisioning.sql`, G6 evidence
**Forbidden paths:** Folio command ownership, tenant credentials, destructive changes to existing databases, and unrelated business contexts

### Delivered behavior

- Added a persisted tenant provisioning state machine and separate health records for write DB, read DB, and MinIO namespace references.
- Added full/PITR/restore-test backup metadata with restore-target validation.
- Added replacement read-database rebuild state and transactional registry switch after a ready checkpoint; the active read key remains unchanged until activation.

### Independent verification

- `cargo fmt --all --check` — PASS after formatting.
- `cargo check -p erpx-infrastructure --tests` — PASS.
- `cargo test -p erpx-infrastructure --test provisioning -- --ignored --nocapture` — PASS on two physically distinct disposable PostgreSQL databases; both dropped by cleanup.
- Evidence: `audits/G6-001/evidence/README.md`.

### Remaining boundary

MinIO namespace/key setup, real backup/PITR/restore execution, tenant routing, and platform operations UI remain open. No G6 completion or command-owner change is claimed.

## Entry 2026-07-24 — G6-001 physical tenant database and RLS follow-up

**Owner:** Main agent
**Dependencies:** G6-001 control-plane registry and health state; SQLx migration crate
**Allowed paths:** `erpx/crates/migration/migrations_tenant_write`, `erpx/crates/migration/migrations_tenant_read`, `erpx/crates/migration/src/lib.rs`, `erpx/crates/infrastructure/src/provisioning.rs`, provisioning integration tests
**Forbidden paths:** production Folio database, secret files, unvalidated SQL identifiers, and implicit deletion of databases

### Delivered behavior

- Added server-side `TenantDatabaseProvisioner` with a PostgreSQL admin URL and database URL template that are never returned to callers.
- Validated database keys before quoting them as SQL identifiers; creation is idempotent and partial failure leaves the tenant in retryable provisioning state.
- Applied separate tenant write/read migration directories. The write side owns aggregate snapshots, outbox, and command idempotency; the read side owns projection checkpoints and summaries.
- Enabled and forced RLS on tenant tables with a fail-closed `erpx.holding_ref` policy, and recorded schema/health evidence into the control plane.
- Added `TenantDatabaseRouter`, which rejects inactive tenants and holding mismatches before connecting and applies the holding scope transaction-locally before tenant SQL runs.
- Added a server-side MinIO/S3-compatible object-store adapter with SigV4 presigned upload/download URLs, idempotent namespace creation, bounded expiries, and object traversal/name validation. Credentials are redacted from debug output.

### Independent verification

- `cargo test -p erpx-migration` — PASS; six tests including separate tenant migration/RLS assertions.
- `cargo check -p erpx-infrastructure --tests` — PASS.
- `cargo test -p erpx-infrastructure object_storage -- --nocapture` — PASS; deterministic SigV4 URL, expiry, traversal, namespace, and credential-redaction tests.
- `cargo test -p erpx-infrastructure --test provisioning physical_provisioning_creates_separate_rls_backed_tenant_databases -- --ignored --nocapture` — PASS; the factory created two tenant databases, applied migrations, and verified RLS/source-table separation. Two control and two tenant databases were explicitly dropped afterward.

### Remaining boundary

Credential/key provisioning, actual PITR/full-backup and restore usability workflows, live MinIO health wiring into activation, integration of routing/object access into all tenant RPC handlers, and platform operations UI remain open. No G6 completion or command-owner change is claimed.

## Entry 2026-07-24 — G2-003/G4-005/G4-006 support capability authorization follow-up

**Owner:** Main agent
**Dependencies:** G2 session transport and role lifecycle, G4 support persistence, G5 billing/entitlement persistence
**Allowed paths:** `erpx/crates/infrastructure/src/repositories.rs`, `erpx/crates/infrastructure/src/lib.rs`, `erpx/crates/infrastructure/tests/support_persistence.rs`, `erpx/apps/server/src/main.rs`, `erpx/crates/proto/proto/erpx/platform/v1/platform.proto`, support evidence
**Forbidden paths:** Folio command ownership, tenant credentials, raw capability secrets, and unrelated ERP contexts

### Delivered behavior

- Added an authoritative write-side capability authorization request carrying customer, agent, device, support-session, ticket, mode, permission, scope, module, bearer, and evaluation time.
- Corrected bearer validation to compare the stored SHA-256 digest with a constant-time digest of the decoded 256-bit bearer. The raw bearer remains hash-only in PostgreSQL and is never logged or returned.
- Enforced the persisted intersection of immutable capability bindings, approved access request, non-closed ticket, active/revocation state, half-open issue/expiry window, requested permission, and exact customer/agent/device/session/ticket/mode/scope/module values.
- Extended the entitlement RPC contract additively for customer/subscription, approval amount, and support-capability context. The server now evaluates billing, allocation, role chain, approval limit, SoD decision, and capability state from write-side data; legacy client booleans are not authoritative.
- Customer support-session approval now requires the authenticated command actor to match the token principal, an active scoped `holding_owner` or `customer_security_head` role, the approval permission, and the existing customer approval signature.

### Independent verification

| Command/evidence | Result |
|---|---|
| `cargo test -p erpx-infrastructure --test support_persistence support_persistence_enforces_idempotency_hash_only_secrets_and_immutable_bindings -- --ignored --nocapture` | PASS on disposable `erpx_support_auth5_20260724`; valid bearer allowed, wrong bearer denied, immutable scope/module changes rejected, revoked capability denied; database dropped by cleanup trap |
| `cargo fmt --all --check` | PASS |
| `cargo clippy --workspace --all-targets -- -D warnings` | PASS |
| `cargo test --workspace` | PASS; all non-ignored suites green; disposable DB suites remain explicitly ignored unless supplied a DB URL |
| `bun run lint` | PASS; four pre-existing warnings, zero errors |
| `bunx tsc --noEmit` | PASS |
| `cargo check -p erpx-console --no-default-features --features web` | PASS |
| `cargo check -p erpx-console --no-default-features --features desktop` | PASS |

### Security and remaining boundary

The authoritative check is currently exposed through the entitlement boundary; every business mutation handler still needs to call the same universal policy before G1/G2/G4 gates can pass. Support scope is still transported as an opaque contract string and needs structured containment validation. Represented-user authority, worker-driven expiry/revocation, live capability termination on all listed causes, and support UI remain open. No ERPX command-owner change is made.

### Rollback

The protobuf additions are wire-compatible and legacy fields remain reserved by behavior. Removing the new authorization call path and additive fields restores the prior prototype boundary; no production database or Folio state was changed. The disposable support database was explicitly removed.

## Entry 2026-07-24 — G2-001 identity recovery and device persistence

**Owner:** AG-IAM packet; Main integration and independent verification  
**Dependencies:** G1 SQLx control write/read pools, identity/session schema, G2 shared contracts  
**Allowed paths:** `erpx/crates/identity`, identity migration, identity integration tests, G2-001 evidence  
**Forbidden paths:** Folio command ownership, tenant credentials, raw recovery codes, and unrelated ERP contexts

### Delivered behavior

- Added transactional one-time recovery-code issuance using Argon2id hashes, replacement of prior unused codes, and guarded single-consumption semantics.
- Added safe device metadata listing and transactional device revocation that also revokes all sessions for the device.
- Added the identity-store validation/advisory-lock boundaries needed to keep principal references and concurrent recovery issuance deterministic.
- Re-exported the safe device snapshot and identity-store errors from the crate root without exposing secrets or password hashes.

### Independent verification

| Command/evidence | Result |
|---|---|
| `cargo test -p erpx-identity --test sqlx_identity -- --ignored --nocapture` | PASS; schema ownership, session lifecycle, recovery-code concurrency, device listing, and device/session revocation exercised on disposable control write/read databases; databases dropped afterward |
| `cargo test --workspace` | PASS |
| `cargo clippy --workspace --all-targets -- -D warnings` | PASS |

### Remaining boundary

Identity HTTP recovery/device workflows, a production WebAuthn ceremony provider, encrypted TOTP, secure client session storage, rate limiting, and final Platform Root/Holding Owner recovery remain open. No identity or Folio command-owner cutover is claimed.

## Entry 2026-07-24 — G5-003 Stripe inbox durability follow-up

**Owner:** AG-BIL packet; Main integration and independent verification  
**Dependencies:** commercial SQLx write/read persistence, raw-body webhook boundary, Stripe authority ADR  
**Allowed paths:** `erpx/crates/infrastructure/src/commercial.rs`, commercial migration, billing evidence  
**Forbidden paths:** client-provided billing state, payment secrets, Folio command ownership, and unrelated contexts

### Delivered behavior

- Added bounded `FOR UPDATE SKIP LOCKED` Stripe inbox claiming with lease ownership and expiry, attempt limits, and deterministic retry backoff.
- Added owner-checked processed transitions, dead-lettering, and explicit dead-letter replay; duplicate/out-of-order inbox rows remain idempotent.
- Added metadata-only debug output for inbox event/claim records so raw webhook payloads and request bodies cannot appear in logs.

### Independent verification

| Command/evidence | Result |
|---|---|
| `cargo test -p erpx-infrastructure --lib commercial::tests::stripe_inbox_claims_and_transitions_are_durable_and_owned -- --ignored --nocapture` | PASS on a disposable control write/read pair; duplicate claim, lease ownership, retry, dead-letter, replay, and payload-redaction assertions passed; databases dropped afterward |
| `cargo fmt --all --check` | PASS |
| `cargo test --workspace` | PASS |

### Remaining boundary

Pinned official Stripe client generation, signature verification against the raw request body in the live HTTP adapter, semantic reconciliation, billing lifecycle application, and universal command adoption remain open. No payment or subscription authority is delegated to clients.

## Entry 2026-07-24 — G6-002 storage namespace verification seam

**Owner:** AG-PRV packet; Main integration and independent verification  
**Dependencies:** physical tenant provisioning, MinIO/S3 adapter, persisted health/activation state  
**Allowed paths:** `erpx/crates/infrastructure/src/provisioning.rs`, object storage adapter, provisioning tests/evidence  
**Forbidden paths:** secret files, tenant credentials in responses, destructive database changes, Folio command ownership

### Delivered behavior

- Added an injected object-store verification boundary to tenant provisioning. Namespace health is recorded pending before the external check, failed on error, and healthy only after successful verification.
- Kept activation separate from namespace verification: the provisioner never silently activates a tenant or extends a backup/recovery state.
- Retained bounded SigV4 presign behavior, namespace/object-name validation, and credential-redacted diagnostics.

### Independent verification

| Command/evidence | Result |
|---|---|
| `cargo test -p erpx-infrastructure --test provisioning -- --ignored --nocapture` | PASS; physical tenant write/read provisioning, forced RLS, scoped router, replacement-read metadata, and injected namespace verification exercised on disposable databases; all dropped afterward |
| `cargo test -p erpx-infrastructure object_storage -- --nocapture` | PASS; deterministic signing, expiry, traversal, namespace, and redaction cases |
| `cargo clippy --workspace --all-targets -- -D warnings` | PASS |

### Remaining boundary

Server composition wiring, live MinIO connectivity in runtime health, actual full/PITR backup and restore execution, signed-object handlers, projection freshness, and platform operations UI remain open.

## Entry 2026-07-24 — G1-003 clean Customer 360 rebuild evidence

**Owner:** Main agent; independently reviewed against worker/projector primitives  
**Dependencies:** G1-002 outbox/checkpoint primitives, G3 customer projection schema  
**Allowed paths:** Customer 360 projector, worker integration test, G1-003 evidence  
**Forbidden paths:** Folio write ownership, tenant business contexts, and unbounded production data mutation

### Delivered behavior

- Verified replacement Customer 360 read projection rebuild from a clean control write/read pair.
- Corrected the generic control-plane fixture aggregate type so unrelated test events cannot be interpreted as partial customer events by the Customer 360 projector.
- Confirmed the read model is disposable and rebuilt from authoritative outbox events; test databases are isolated and explicitly removed.

### Independent verification

| Command/evidence | Result |
|---|---|
| `cargo test -p erpx-worker --test customer_360 -- --ignored --nocapture` | PASS on a clean disposable control write/read pair; rebuild and projection checkpoint assertions passed; databases dropped afterward |
| `cargo test -p erpx-infrastructure --test sqlx_control_plane -- --ignored --nocapture` | PASS on a disposable pair after fixture isolation; databases dropped afterward |
| `cargo test --workspace` | PASS |

### Remaining boundary

Production worker composition, all context projectors, multi-worker runtime operation, replacement read-database switch execution, and end-to-end outbox adoption by every mutation remain open.

## Entry 2026-07-24 — G1-004/G3/G4 resource binding and universal command-policy slice

**Owner:** Main agent; AG-QA independent review  
**Dependencies:** G0 contracts, G1 SQLx control write schema, G2 authenticated session/role evaluation, G5 billing persistence  
**Allowed paths:** common command contract, control migration, write-side repository, server policy boundary, focused integration tests  
**Forbidden paths:** Folio command ownership, tenant credentials, client-provided authorization assertions, and unrelated ERP contexts

### Delivered behavior

- Added additive `CommandContext.subscription_id` transport/domain metadata for write-side commercial authority; control-plane commands may omit it, while customer-scoped entitled mutations require it.
- Added `customer_scope_bindings` to the control write database with holding/company/branch containment and a fail-closed `customer_scope_contains` lookup. The lookup supports explicit hierarchy wildcards and rejects sibling/unknown targets.
- Integrated customer-scope binding into the shared entitled-command policy and `CheckEntitlement`, so arbitrary customer/subscription identifiers cannot authorize a tenant-scoped request without a persisted organization binding.
- Centralized access-token actor/session attribution in `AuthContext::authorize`, covering every gRPC handler, and bounded command/query audit purpose/module text against control characters and oversized metadata.
- Added shared customer-scoped commercial policy evaluation for support ticket/session commands: write-side billing lifecycle, module entitlement/allocation, effective role chain, SoD, scope, and mutation state are evaluated before the application command.
- Classified Stripe webhook persistence/configuration failures as `503`; malformed JSON/signature/header failures remain client errors.

### Independent verification

| Command/evidence | Result |
|---|---|
| `cargo test -p erpx-infrastructure --test sqlx_control_plane -- --ignored --nocapture` | PASS on disposable control write/read databases; migration 0008 binding and sibling-scope assertions passed with all databases dropped afterward |
| `cargo test -p erpx-worker --test customer_360 -- --ignored --nocapture` | PASS on a clean disposable control pair; databases dropped afterward |
| `cargo test -p erpx-infrastructure --test provisioning physical_provisioning_creates_separate_rls_backed_tenant_databases -- --ignored --nocapture` | PASS on disposable control and tenant databases; all four databases dropped afterward |
| `cargo test -p erpx-server -- --nocapture` | PASS; 18 tests including identity HTTP, storage configuration/redaction, transport boundary, support scope/actor, and raw Stripe signature tests |
| `cargo fmt --all --check` | PASS |
| `cargo clippy -p erpx-server --all-targets -- -D warnings` | PASS |

### Independent QA findings retained

AG-QA confirmed the prior highest-risk gaps were cross-customer/session/subscription binding, non-centralized commercial policy, broad role-renewal direction, and absent `ErrorContext` envelopes/structured observability. Binding and policy coverage are improved here; stable structured errors, customer-to-ticket organization resolution, role-renewal SoD/commercial policy, represented-user authority, universal support-capability checks on all tenant mutations, rate limits, and tracing/redaction remain open. The audit was read-only and made no source or runtime changes.

### Rollback

The additive command field and migration can be rolled back only with an explicit migration strategy after verifying no rows use the binding table. Removing policy calls restores the prior prototype behavior but would re-open authorization risk; no Folio state or command ownership was changed.

## Entry 2026-07-24 — G2-001 identity HTTP recovery/device follow-up

**Owner:** AG-IAM packet; Main integration and independent verification  
**Dependencies:** persisted SQLx identity store, access-token/session validation, existing HTTP listener  
**Allowed paths:** `erpx/apps/server/src/identity_http.rs`, narrow server dispatch, G2 identity evidence  
**Forbidden paths:** raw recovery codes in responses/logs, password hashes, Folio, business contexts, and planning docs

### Delivered behavior

- Added authenticated `GET /auth/devices` returning only current-principal device metadata.
- Added authenticated `POST /auth/devices/{id}/revoke`, scoped to the current principal and atomically revoking the device and its sessions; current-device tokens fail immediately afterward.
- Added authenticated recovery-code issuance with bounded count and metadata-only response, plus recovery-code consumption with generic replay/invalid-code failure behavior.
- Recovery and device DTOs omit tokens, plaintext codes, hashes, passwords, and secret material.

### Independent verification

| Command/evidence | Result |
|---|---|
| `cargo test -p erpx-server -- --nocapture` | PASS; 18 tests |
| `cargo test -p erpx-identity` | PASS; 6 unit tests |
| Disposable identity SQLx integration suite | PASS; schema, session, recovery consumption, and device/session revocation cases; databases dropped afterward |
| Disposable HTTP smoke | PASS; device list 200, recovery issuance 200, recovery consume 200, replay 401, current-device revoke 200, post-revocation access 401 |
| `cargo clippy -p erpx-server --all-targets -- -D warnings` | PASS |

### Remaining boundary

Recovery delivery/display, rate limits and lockout/audit policy, production WebAuthn provider verification, secure platform-local token storage, encrypted-TOTP operational flows, and final root-owner recovery remain open. No identity or Folio command-owner cutover is claimed.

## Entry 2026-07-24 — G6-002 server storage composition follow-up

**Owner:** AG-PRV packet; Main integration and independent verification  
**Dependencies:** MinIO/S3 adapter, tenant namespace health gate, HTTP health/readiness boundary  
**Allowed paths:** `erpx/apps/server/src/storage.rs`, server composition/health dispatch, G6 evidence  
**Forbidden paths:** credentials in responses, tenant DB URLs in clients, backup deletion, Folio, and unrelated contexts

### Delivered behavior

- Added server-only MinIO configuration validation from environment with optional/required modes; startup fails closed on partial/invalid required configuration.
- Added redacted storage configuration signals to `/healthz` and `/readyz`; readiness never claims live MinIO connectivity when it has not been probed.
- Wired the server-only composition seam to the existing tenant provisioning namespace verification gate without exposing object-store handles or credentials.

### Independent verification

| Command/evidence | Result |
|---|---|
| `cargo check -p erpx-server --tests` | PASS |
| `cargo test -p erpx-server -- --nocapture` | PASS; storage configuration/redaction and injected boundary tests included |
| `cargo clippy -p erpx-server --all-targets -- -D warnings` | PASS |
| `cargo check -p erpx-infrastructure --tests` | PASS |

### Remaining boundary

No live MinIO probe, server provisioning command invocation, signed object handlers, backup/PITR/restore execution, storage-health freshness worker, or platform operations UI is claimed. No G6 gate or command-owner change is claimed.

## Entry 2026-07-24 — G2-001 durable identity throttling follow-up

**Owner:** AG-IAM packet; Main integration and independent verification  
**Dependencies:** SQLx identity write store, identity HTTP dispatcher, Argon2id/recovery primitives  
**Allowed paths:** identity store/migration/tests and identity HTTP route hooks  
**Forbidden paths:** raw principal identifiers in limiter rows, plaintext codes, business handlers, Folio, and planning docs

### Delivered behavior

- Added SQLx-backed, transactionally locked rate-limit buckets keyed by SHA-256 namespace/identifier digests; raw principal references are not persisted.
- Login is bounded to five attempts per five minutes with a fifteen-minute block; recovery issuance and consumption are bounded to three/five attempts per fifteen minutes with thirty-minute blocks.
- Successful login clears only its login bucket; invalid policy/database failures fail closed with generic service-unavailable behavior, and throttled requests return generic `429` responses.
- Added concurrent admission/persistence/clearing/non-disclosure tests and integrated the limiter into login and recovery HTTP flows.

### Independent verification

| Command/evidence | Result |
|---|---|
| `cargo test -p erpx-identity` | PASS; 7 tests |
| `cargo clippy -p erpx-identity --all-targets -- -D warnings` | PASS |
| Ignored SQLx identity rate-limit test | PASS; durable bucket, concurrent admission, blocking, clearing, and identifier non-disclosure; disposable database dropped afterward |
| `cargo test -p erpx-server -- --nocapture` | PASS after integrating the route dispatcher; 20 tests including identity routes and shared error metadata |

### Remaining boundary

There is no trusted client-IP/global abuse bucket in the current minimal HTTP listener; unauthenticated recovery can rotate principal references, and limiter denials need audit/metrics/retention cleanup. WebAuthn provider verification and secure client token storage remain open.

## Entry 2026-07-24 — G1-004 stable error metadata and role-scope correction

**Owner:** Main agent; AG-QA independent review  
**Dependencies:** G0 ErrorContext contract, boundary correlation validation, application error taxonomy, role scope model  
**Allowed paths:** server error adapter, handler error mapping, role renewal boundary, focused server tests  
**Forbidden paths:** business-context source records, Folio, raw secrets, and client authorization authority

### Delivered behavior

- Added a server error adapter that maps tonic/application failures to stable ERPX code/retryability/correlation metadata and preserves a transport-neutral `ErrorContext` representation.
- Application persistence/authorization/support failures from current RPC handlers now carry the request correlation ID in the ERPX error metadata fields.
- Corrected role-renewal containment direction so a narrow branch/company command cannot renew a broader holding/company assignment.
- Added tests for unavailable error metadata and the non-expanding role-renewal scope relation.

### Independent verification

| Command/evidence | Result |
|---|---|
| `cargo test -p erpx-server -- --nocapture` | PASS; 20 tests |
| `cargo clippy -p erpx-server --all-targets -- -D warnings` | PASS |
| `cargo fmt --all --check` | PASS at integration-wave boundary |

### Remaining boundary

Direct validation/authentication statuses and the manual HTTP response paths still need complete ErrorContext envelopes, correlation response headers, request/latency tracing, method restrictions, readiness probes for identity/worker/commercial/tenant state, provider-error redaction, and webhook audit retention. No G1-004 gate is claimed.

## Entry 2026-07-24 — G6-003 backup/PITR/restore executor seam

**Owner:** AG-PRV packet; Main integration and independent verification  
**Dependencies:** tenant backup metadata schema, registry/provisioning state, physical database keys  
**Allowed paths:** `erpx/crates/infrastructure/src/provisioning.rs` and focused provisioning tests  
**Forbidden paths:** Folio databases, secret files, destructive shell/database commands, server handlers, and unrelated contexts

### Delivered behavior

- Added strict backup-run shape validation for full, PITR, and restore-test kinds, status transitions, target references, verification, completion, and bounded error fields.
- Added `TenantProvisioner::execute_backup` with an injected executor callback. It validates active/recovery tenant state and physical key references, persists started before invocation, and persists completed/verified or failed outcomes afterward.
- The seam deliberately does not invoke `pg_dump`, `pg_restore`, shell commands, or database deletion; production execution remains an explicitly supplied server-side callback.

### Independent verification

| Command/evidence | Result |
|---|---|
| `cargo test -p erpx-infrastructure --lib` | PASS; 11 tests, 1 expected ignored SQLx test |
| `cargo check -p erpx-infrastructure --tests` | PASS |
| `cargo clippy -p erpx-infrastructure --tests -- -D warnings` | PASS |
| `cargo fmt -p erpx-infrastructure -- --check` | PASS |
| Deterministic injected executor unit evidence | PASS; physical write/read keys passed without external services |

### Remaining boundary

SQL-backed execute lifecycle requires a disposable integration case, actual backup/PITR/restore implementation is external, and replacement-read switching remains explicitly caller-controlled. No G6 gate or command-owner change is claimed.

## Entry 2026-07-24 — G1-005 authenticated Dioxus session boundary

**Owner:** AG-UI packet; Main integration and independent verification  
**Dependencies:** shared Dioxus UI crate, server HTTP auth response contract, route-state foundation  
**Allowed paths:** `erpx/crates/ui`, `erpx/apps/console`, UI evidence  
**Forbidden paths:** backend authorization authority, Folio routes, persistent secret storage, and business modules

### Delivered behavior

- Added typed in-memory `SessionController` states for signed-out, authenticating, authenticated, refreshing, revoking, expired, revoked, offline, and error conditions.
- Added login/refresh/revoke transport seams and redacted token wrappers; tokens are never serialized or written to browser storage, URLs, route state, or UI metadata.
- Preserved safe support/read-only banners across refresh and replaced console hard-coded actor/session metadata with controller-derived transport headers.
- Kept the backend authoritative: the UI state machine does not evaluate RBAC, entitlement, scope, or support capability.

### Independent verification

| Command/evidence | Result |
|---|---|
| `cargo test -p erpx-ui -- --nocapture` | PASS; 15 tests |
| `cargo check -p erpx-console --no-default-features --features web` | PASS |
| `cargo check -p erpx-console --no-default-features --features desktop` | PASS |
| UI/console clippy with `-D warnings` | PASS |

### Remaining boundary

Concrete HTTP/native adapters, login UI, Android/iOS secure keystore/keychain and biometric UX, background refresh scheduling, deep-link reauthentication, and all remaining Dioxus product pages/targets remain open. No mobile gate is claimed.

## Entry 2026-07-24 — G1-004 HTTP correlation and method hardening follow-up

**Owner:** Main agent  
**Dependencies:** HTTP listener, stable error metadata adapter, raw-body boundary  
**Allowed paths:** server HTTP response/health dispatch  
**Forbidden paths:** business data, Folio, secret files, and command ownership

### Delivered behavior

- HTTP responses now include a generated or validated `X-Correlation-Id`, `Cache-Control: no-store`, and `X-Content-Type-Options: nosniff`.
- Health, readiness, and metrics endpoints accept only `GET`; other methods receive `405`.
- The same correlation identifier is used for identity-dispatch and authenticated early-return responses in the HTTP listener.

### Independent verification

- `cargo test -p erpx-server -- --nocapture` — PASS; 20 tests.
- `cargo clippy -p erpx-server --all-targets -- -D warnings` — PASS.
- `cargo fmt --all --check` — PASS.

### Remaining boundary

HTTP bodies still need a complete serialized ErrorContext schema, webhook audit/digest retention, structured request/latency tracing, and security/readiness header tests. No G1-004 completion claim is made.

## Entry 2026-07-24 — G1-004 serialized errors and universal gRPC handler enrichment

**Owner:** Main agent  
**Dependencies:** G0 shared ErrorContext contract, boundary correlation validation, HTTP listener, Tonic service handlers  
**Allowed paths:** `erpx/apps/server/src/{boundary.rs,errors.rs,main.rs}`, G1-004 evidence  
**Forbidden paths:** Folio, secrets, business-context migrations, and command ownership

### Delivered behavior

- HTTP non-success responses are normalized to a serialized `ErrorContext` body containing stable code, safe message, correlation ID, retryability, and details.
- All HTTP response paths retain correlation/security headers, while health/readiness/metrics remain GET-only.
- Request completion tracing records only method, route, correlation ID, and latency; no body, authorization header, token, or provider payload is logged.
- Identity readiness probes the control write-side pool.
- Every typed Tonic RPC handler and the transport interceptor enrich failures with stable ERPX error metadata, a serialized `ErrorContext` metadata envelope, and a request correlation ID, including malformed-context and authentication failures.

### Independent verification

| Command/evidence | Result |
|---|---|
| `cargo test -p erpx-server -- --nocapture` | PASS; 22 tests |
| `cargo clippy -p erpx-server --all-targets -- -D warnings` | PASS (previous focused run; rerun in next full wave) |
| `rustfmt --edition 2024` on touched server files | PASS |
| Error-context unit tests | PASS; serialized HTTP body and success-preservation cases |

### Remaining boundary

HTTP and Tonic boundary normalization is integrated, but provider/raw-body audit retention, worker/tenant/commercial readiness probes, end-to-end redaction review, and complete ErrorContext propagation through worker and future module APIs remain open. No G1-004 gate is claimed.

## Entry 2026-07-24 — G3-002-003 Customer 360 projections and scope bindings

**Owner:** AG-CUS (Ohm); independently verified by Main  
**Dependencies:** G0 contracts, G1-003 projector/checkpoint primitives, G1-004 customer-scope authorization  
**Allowed paths:** Customer domain/projection, worker Customer 360 handler, infrastructure read/write persistence, packet evidence  
**Forbidden paths:** Folio, server authorization policy, unrelated module migrations, and secrets

### Delivered behavior

- Legacy customer events remain compatible while enriched projections canonicalize organization, contacts, commercial, modules, invoices, quotes, and discounts.
- Active structurally connected holding/company/branch units derive deterministic, sorted, deduplicated customer scope bindings on the control write database.
- Enriched Customer 360 payloads persist on the control read database; summaries remain redacted and legacy list/detail compatibility remains intact.
- Rebuilds are idempotent and byte-stable; disconnected/inactive organization units fail closed.

### Independent verification

| Command/evidence | Result |
|---|---|
| `cargo test -p erpx-domain -p erpx-infrastructure -p erpx-worker --lib -- --nocapture` | PASS; 37 domain, 15 infrastructure (2 expected ignored), 1 worker (1 expected ignored) |
| Customer 360 disposable two-database rebuild | PASS; enriched read payload, write-only bindings, isolation, redaction, deterministic second rebuild; databases dropped |
| `cargo test -p erpx-worker --test customer_360 -- --ignored --nocapture` with disposable pair | PASS; 1 test; databases dropped |
| Affected clippy | PASS in focused run with `-D warnings` |

### Residual boundary

Legacy events without organization IDs cannot create bindings; deactivation/removal reconciliation, enriched detail APIs, and full migration backfill remain open. This packet does not close G3.

## Entry 2026-07-24 — G5-003-004 typed Stripe semantic reconciliation

**Owner:** AG-BIL (Pascal); independently verified by Main  
**Dependencies:** G5 raw-body inbox/signature verification, commercial persistence, worker claiming/retry/DLQ  
**Allowed paths:** Commercial domain/infrastructure semantic dispatcher, worker semantic handler, packet evidence  
**Forbidden paths:** Raw webhook boundary, payment credentials, Folio, and command ownership

### Delivered behavior

- Typed mappings cover payment recovery/failure, subscription cancellation/status, entitlement updates, and explicitly accepted no-op events.
- Duplicate inbox IDs and stale provider timestamps do not mutate state; newer semantic events apply through optimistic SQLx persistence.
- Retryable application failures retry with bounded policy; malformed/permanent events dead-letter; no raw payload is copied to semantic errors.
- `ERPX_WORKER_HANDLER=stripe_semantic` exposes the worker handler without replacing the existing customer projection handler.

### Independent verification

| Command/evidence | Result |
|---|---|
| `cargo test -p erpx-domain -p erpx-infrastructure -p erpx-worker --lib -- --nocapture` | PASS; semantic mapping, no-op, redaction, and retry policy tests included |
| Disposable semantic SQLx test | PASS; idempotent/stale billing and entitlement behavior; databases dropped |
| Disposable worker semantic test | PASS; valid event applied and unknown event dead-lettered; databases dropped |
| Affected clippy with `-D warnings` | PASS |

### Residual boundary

Pinned official Stripe OpenAPI client/API synchronization, remote catalog publication, metadata-to-internal-ID onboarding, scheduled reconciliation, and full billing command adoption remain open. This packet does not close G5.

## Entry 2026-07-24 — G3-006 Customer 360 route surfaces

**Owner:** AG-UI (Curie); independently verified by Main  
**Dependencies:** G1-005 shared route/session state, G3 list/detail query and global-read audit  
**Allowed paths:** Dioxus console route/view/assets and packet evidence  
**Forbidden paths:** server/protobuf/database contracts, client authorization, token persistence, and Folio

### Delivered behavior

- Typed route navigation visibly covers all required Customer 360 paths.
- Customer-scoped routes use the existing audited `GetCustomer` query and show explicit read-only placeholders where secondary projections are not exposed.
- Opaque customer IDs are route-validated; session/auth states remain owned by the shared UI boundary and no credentials or tenant references are rendered.

### Independent verification

| Command/evidence | Result |
|---|---|
| `cargo test -p erpx-console --no-default-features --features web -- --nocapture` | PASS; 3 tests |
| `cargo test -p erpx-console --no-default-features --features desktop -- --nocapture` | PASS; 3 tests |
| `cargo test -p erpx-ui -- --nocapture` | PASS; 15 tests plus doc-tests |
| Web/desktop checks and focused clippy | PASS |

### Residual boundary

Secondary Customer 360 backend endpoints, browser/deep-link history, authenticated login/refresh adapters, Android/iOS builds, and the remaining product pages remain open. No G1-005 or G3 gate is claimed.

## Entry 2026-07-24 — G2-001 WebAuthn/provider configuration seam

**Owner:** AG-IAM (Socrates); independently verified by Main  
**Dependencies:** identity challenge/passkey primitives and shared session contract  
**Allowed paths:** identity challenge/error/provider modules and identity tests  
**Forbidden paths:** server ceremony routes, protobuf, UI, migrations, commercial/RBAC, and Folio

### Delivered behavior

- Validated WebAuthn RP/origin configuration and allowlists, with conservative HTTPS/local-origin rules.
- Provider wrapper validates request context before delegation; challenge principal/ceremony/time/replay checks fail closed.
- Credential ID binding and non-decreasing counter checks reject rollback while allowing equal counters.
- Unsupported provider remains explicit; no fake browser or cryptographic verification is claimed.

### Independent verification

| Command/evidence | Result |
|---|---|
| `cargo test -p erpx-identity -- --nocapture` | PASS; 12 tests, 4 expected SQLx tests ignored |
| `cargo clippy -p erpx-identity --all-targets -- -D warnings` | PASS |
| Scoped rustfmt | PASS |

### Residual boundary

Live browser ceremony transport, attestation/assertion cryptographic verification, passkey SQLx persistence and atomic counters, and server integration remain open. This packet does not close G2.

## Entry 2026-07-24 — G5-001 catalog invariants and reconciliation

**Owner:** AG-BIL (Pascal); independently verified by Main  
**Dependencies:** commercial catalog revisions, entitlement capacity, grandfathered-price records, Stripe semantic persistence  
**Allowed paths:** commercial domain/infrastructure/re-export/tests/evidence  
**Forbidden paths:** live Stripe calls, raw webhook handling, identity/RBAC, server/protobuf, migrations, secrets, and Folio

### Delivered behavior

- Published catalog revisions validate revision numbering, names, base/module/setup pricing, scoped units, charge types, trials, coupons, included usage, currencies, and overage invariants.
- Provider catalog revisions and deterministic safe mismatch categories support reconciliation without provider calls or payment data leakage.
- Existing immutable publication and explicit grandfathered-price migration rules remain authoritative.

### Independent verification

| Command/evidence | Result |
|---|---|
| Commercial domain catalog tests | PASS; 9 tests |
| Infrastructure commercial tests | PASS; 6 non-ignored tests, 3 expected SQLx tests ignored |
| Disposable catalog reconciliation persistence | PASS; databases dropped |
| `cargo check -p erpx-infrastructure --tests` | PASS |
| Affected clippy with `-D warnings` | PASS |
| `cargo fmt --all --check` | PASS |

### Residual boundary

Pinned Stripe client/API-version synchronization, provider catalog publication, automatic mismatch repair, and grandfather migration approval workflow remain open. This packet does not close G5.

## Entry 2026-07-24 — G1-004 QA fixes and universal Customer 360 detail transport

**Owner:** Main agent; QA findings from AG-QA independently reviewed  
**Dependencies:** G0 contracts, G1 boundary/error envelope, G2 persisted role/SOD state, G3 detail-read port

### Delivered behavior

- Correlation aliases are parsed through one conflict-checked helper for Tonic and HTTP; the context/audit/header UUID must agree.
- HTTP 500 is `internal`; 502–504 are `unavailable`; identity database and invalid-record failures have one classification across login, refresh, token authorization, and recovery paths.
- `RenewRole` now applies an explicit platform identity-control policy or the full customer subscription/module/scope/role-chain/SoD policy; customer and module authority are additive protobuf fields and tenant claims are required for tenant renewals.
- Worker outbox/projection failures emit bounded structured worker/event/aggregate/correlation context with credential/provider redaction.
- `GetCustomerProjection` exposes organization, contacts, commercial, entitlements, onboarding, health, support, access-audit, and notes envelopes through an audited platform gRPC method. Unsupported read models are explicit and read-only.

### Independent verification

| Evidence | Result |
|---|---|
| `cargo test -p erpx-server boundary::tests -- --nocapture` | PASS; 6 tests |
| `cargo test -p erpx-server errors::tests -- --nocapture` | PASS; 5 tests |
| `cargo clippy -p erpx-server --all-targets -- -D warnings` | PASS |
| `cargo test -p erpx-infrastructure worker::tests -- --nocapture` | PASS; structured context and retry tests |
| `cargo clippy -p erpx-infrastructure --all-targets -- -D warnings` | PASS |
| `cargo check -p erpx-server` | PASS |
| `audits/G1-004-QA-FIXES/evidence/README.md` | Recorded |

### Residual boundary

Real WebAuthn ceremonies, object-access transport, worker/tenant/commercial live readiness, Stripe raw-body digest retention, and universal adoption by future business commands remain open. No G1, G2, G3, or G5 gate is claimed.

## Entry 2026-07-24 — G3 detail-read and G6 physical provisioning reruns

**Owner:** Main agent; AG-CUS and AG-PRV handoffs independently verified  
**Dependencies:** SQLx control read/write pools, embedded migrations, tenant router and replacement-read state machine

### Independent verification

- `ERPX_TEST_CONTROL_WRITE_DATABASE_URL=postgres://localhost/erpx_c360_detail_w_20260724_a ERPX_TEST_CONTROL_READ_DATABASE_URL=postgres://localhost/erpx_c360_detail_r_20260724_a cargo test -p erpx-infrastructure --test customer360_detail_reads -- --ignored --nocapture` — PASS; 1 test; both databases dropped.
- `ERPX_TEST_CONTROL_WRITE_DATABASE_URL=postgres://localhost/erpx_prov_w_20260724_a ERPX_TEST_CONTROL_READ_DATABASE_URL=postgres://localhost/erpx_prov_r_20260724_a cargo test -p erpx-infrastructure --test provisioning -- --ignored --nocapture` — PASS; 2 tests, including distinct physical tenant write/read databases, forced RLS, storage gate, backup transitions, replacement-read activation; all four named databases dropped.
- `cargo test -p erpx-infrastructure provisioning --lib -- --nocapture` — PASS; 2 unit tests.
- `cargo clippy -p erpx-infrastructure --tests -- -D warnings` — PASS.

### Residual boundary

Customer access-audit/notes read models, binding deactivation reconciliation, live backup/PITR executor, object-access handler integration, tenant operations UI, and freshness monitoring remain open. Evidence is recorded in `audits/G3-001-004-CUSTOMER360-DETAIL-READS/evidence/README.md` and `audits/G6-001/evidence/README.md`.

## Entry 2026-07-24 — G2-001/G1-005/G5/G6 boundary hardening and clean verification

**Owner:** Main agent; packet handoffs from AG-IAM, AG-UI, AG-BIL, and AG-PRV independently reviewed
**Dependencies:** G1 SQLx control plane, G1 boundary/error contracts, G2 role/session primitives, G5 commercial persistence, G6 provisioning/storage seams

### Delivered behavior

- Persisted passkeys now have SQLx registration, principal-scoped lookup, idempotent revocation, and atomic non-decreasing sign-counter updates. Legacy incomplete records fail closed and safe identity error mappings cover duplicate, missing, and revoked passkeys.
- The shared Dioxus session boundary now has an explicit restoring state, typed redacted credentials, secure-store abstraction, refresh/revoke transport, login/sign-out UI, deep-link credential redaction, and no silent plaintext production fallback.
- Stripe provider configuration and typed catalog identifiers reject unpinned/unsafe endpoints and invalid idempotency references; provider failures and worker failures are bounded and redacted.
- Stripe webhook ingestion persists a SHA-256 digest of the authenticated raw body and request correlation ID in migration `0009`; workers receive metadata only and never the raw body.
- Server HTTP object routes now authorize tenant, holding, module permission, role chain, active billing/entitlement state, session, and exact MinIO namespace before returning bounded signed upload/download URLs.

### Independent verification

| Evidence | Result |
|---|---|
| `cargo test -p erpx-identity --all-targets -- --nocapture` | PASS; 13 unit tests, 5 expected SQLx tests ignored |
| Disposable identity SQLx test with separate control write/read databases | PASS; 5 ignored tests; databases dropped |
| `cargo test -p erpx-ui -- --nocapture` | PASS; 19 tests |
| Console web and desktop test suites | PASS; 5 tests each |
| `cargo test -p erpx-server` | PASS; 29 tests, including object access, error metadata, correlation aliases, and readiness semantics |
| `cargo clippy -p erpx-server --all-targets -- -D warnings` | PASS |
| `cargo test -p erpx-infrastructure` | PASS; 22 tests, 3 expected SQLx tests ignored |
| `cargo clippy -p erpx-infrastructure --all-targets -- -D warnings` | PASS |
| Clean disposable commercial SQLx suite after migration `0009` was embedded | PASS; 3 ignored tests, including 32-byte raw digest and persisted correlation metadata; databases dropped |
| `cargo fmt --all --check` | PASS |

### Security and rollback review

- Raw Stripe bodies remain write-side persistence only; claims and semantic workers do not select or log them. Stored audit metadata is fixed-length and correlation-linked.
- Signed object responses contain no access key, secret, tenant credentials, or raw authorization material; namespace and traversal checks remain server-side.
- Identity records and session DTOs do not expose password hashes, recovery material, refresh hashes, passkey public-key material, or access tokens.
- The combined commercial suite was first rerun against a contaminated disposable database and correctly failed on immutable catalog reuse; the databases were then explicitly recreated and the clean run passed. No Folio database or runtime state was changed.

### Residual boundary

The packet does not close G1–G6. Live WebAuthn browser cryptography/ceremonies, native Android/iOS secure stores and target builds, live MinIO readiness/backup execution, access-audit/notes read models, official Stripe client/API synchronization, universal command adoption, tenant operations UI, and all ERP contexts remain open. The readiness follow-up is active and must be independently integrated before treating configured object storage as production-ready.

## Entry 2026-07-24 — G3-005-006 Customer 360 secondary read models

**Owner:** AG-CUS (Ohm); independently verified by Main
**Dependencies:** G3 enriched Customer 360 projection and control read database

### Delivered behavior

- Added rebuildable read-side tables for customer access-audit summaries and customer notes.
- Added typed read-pool-only DTOs and `CustomerReadView` freshness/read-only envelopes; unknown customers remain `Absent`.
- Access-audit summaries expose only customer-visible aggregate fields; notes are deterministic newest-first values and are not authorization inputs.

### Independent verification

| Evidence | Result |
|---|---|
| Domain/application/migration/infrastructure focused suites | PASS; domain 42, application 4, migration 6, infrastructure 22; expected SQLx tests ignored |
| Fresh disposable Customer 360 detail read pair | PASS; 1 ignored test; both databases explicitly dropped |
| Affected strict clippy and test-target compilation | PASS |

### Residual boundary

The worker backfill/projector for these read models, transport/UI integration, and customer-visible detail workflow remain open. No authorization decision uses these read models.

## Entry 2026-07-24 — G1-004 object-store readiness and G5 webhook context integration

**Owner:** AG-PRV/AG-BIL handoffs; Main server integration and independent review
**Dependencies:** object presign boundary, raw Stripe-body verification, HTTP health/readiness boundary

### Delivered behavior

- Configured object storage starts `unprobed`; required storage keeps `/readyz` at 503 until a non-mutating live probe succeeds, and probe failure remains unavailable. Optional disabled storage remains ready.
- MinIO readiness uses a bounded signed HEAD probe; health output reports state only and never credentials.
- Stripe HTTP ingestion now constructs `StripeWebhookPersistenceContext` from the verified raw body and request correlation, then calls the strict persistence method that rechecks the digest before insertion.

### Independent verification

| Evidence | Result |
|---|---|
| Server focused readiness/object tests | PASS; 31 server tests in packet handoff |
| Infrastructure object/commercial tests | PASS; 25 tests, 3 expected SQLx tests ignored in packet handoff |
| Strict package clippy/checks and formatting | PASS in packet handoffs |
| Fresh disposable webhook persistence | PASS; digest length and correlation binding verified; databases dropped |

### Residual boundary

The live MinIO endpoint was not used in this verification; tenant namespace verification, backup/PITR execution, official Stripe client/API synchronization, encrypted/raw-body retention policy, and overall G1/G5 gates remain open.

## Entry 2026-07-24 — G3-005-006 access-audit summary projector

**Owner:** AG-CUS (Ohm); independently verified by Main
**Dependencies:** Customer 360 secondary read migration, authoritative write-side global-read audit table, worker projection primitives

### Delivered behavior

- Added a named rebuildable worker projection for `customer_access_audit_summary`.
- The projector reads aggregate counts/max timestamps from `global_read_audits` on the control write pool, writes only existing customer IDs to the control read model, clears/rebuilds idempotently, and never fabricates notes.
- Projection errors are bounded and redacted; actor, scope, purpose, correlation, and raw audit payload remain write-side only.

### Independent verification

| Evidence | Result |
|---|---|
| Worker unit tests | PASS; stable projection name and bounded redacted error test |
| Fresh disposable control write/read worker test | PASS; ordered/idempotent rebuild, orphan filtering, read-side isolation, and empty-notes behavior; databases dropped |
| Infrastructure read repository and worker affected tests | PASS |

### Residual boundary

The general worker runtime still needs durable scheduling/handler registration for every context; customer notes require a separate authoritative note aggregate before they can be projected. G3 and G1 gates remain open.

## Entry 2026-07-24 — G4-001 support lifecycle correction

**Owner:** Main agent; independently reviewed against the user-supplied transition contract
**Dependencies:** existing support aggregate and persistence adapter

### Delivered behavior

- Corrected the ticket state machine to include triage/in-progress/waiting/access-requested reopen paths and to make `closed` terminal.
- Removed the non-contract direct `resolved → closed` edge; closure now requires `customer_confirmed`.
- Added a table-driven lifecycle test covering reopen, approval, resolution, confirmation, and terminal behavior.

### Verification

| Evidence | Result |
|---|---|
| `cargo test -p erpx-domain support::tests -- --nocapture` | PASS; 10 support tests |
| Strict repository formatting | Pending after concurrent packet edits; no semantic test failures |

### Residual boundary

Ticket recurrence linkage, full support capability intersection (agent role, billing, entitlement, represented-user authority, SoD), customer-facing support UI, and incident transport remain open.

## Entry 2026-07-24 — G1-004 universal object policy and G2 passkey fail-closed transport

**Owner:** Main agent with AG-IAM handoff; independently verified by Main/AG-QA
**Dependencies:** tenant registry, SQLx control write authority, billing/entitlement tables, support capability persistence, signed object storage

### Delivered behavior

- Object upload/download requests now require customer and subscription identity plus an optional company/branch scope, and validate active tenant registration, exact storage namespace, customer scope binding, direct role or platform support capability, billing, `documents` entitlement, module activation/allocation, role chain, SoD, and bounded object policy.
- Downloads require an explicit purpose and append a customer-visible global-read audit with actor, session, scope, module, purpose, and correlation ID before a signed URL is returned.
- Support object access binds agent/device/session/ticket/mode/permission/scope/module/capability secret and rechecks the platform support role and capability at action time.
- Passkey ceremony helpers are now wired behind an explicit server provider slot; when no real browser-ceremony provider is linked, startup rejects requested live configuration and passkey HTTP routes return a safe 503 rather than a false-success or silent fake provider.

### Independent verification

| Evidence | Result |
|---|---|
| `cargo test -p erpx-server` | PASS; 33 tests, including passkey provider fail-closed configuration and object/readiness boundary tests |
| `cargo clippy -p erpx-server --all-targets -- -D warnings` | PASS |
| `cargo check -p erpx-server --tests` | PASS |
| Support lifecycle domain suite | PASS; 10 tests with frozen transition matrix |

### Residual boundary

Live object authorization still needs end-to-end disposable control/tenant fixtures for direct and support branches; native secure keystores, real WebAuthn cryptographic provider, TOTP enrollment/recovery sessions, and universal adoption by future business commands remain open. The official Stripe adapter and comprehensive readiness composition are still dependency-ready packets, not completed gates.

## Integration-wave checklist

For every future handoff:

1. Read active goal, worktree, runtime, plan, ledger, and last packet evidence.
2. Verify dependency readiness and non-overlapping ownership.
3. Review support-agent files and contract/database effects.
4. Run focused tests, affected integration tests, and required repository verification.
5. Perform security/isolation/rollback review.
6. Record exact commands and results here before selecting the next wave.

## Verification addendum — 2026-07-24 G4/G5 integration wave

The final affected-context verification was rerun after the support and
commercial migrations were embedded and exported through infrastructure:

| Evidence | Result |
|---|---|
| `bun run lint` | PASS; four pre-existing warnings in `infra/scripts/start-n8n.js`, zero errors |
| `bunx tsc --noEmit` | PASS |
| `cargo fmt --all --check` | PASS |
| `cargo clippy --workspace --all-targets -- -D warnings` | PASS |
| `cargo test --workspace` | PASS |
| `cargo check -p erpx-console --no-default-features --features web` | PASS |
| `cargo check -p erpx-console --no-default-features --features desktop` | PASS |
| Disposable control-plane integration pair | PASS; command/outbox/read projection, global audit, tenant registry, catalog, billing, entitlements, Stripe inbox; both DBs dropped |
| Disposable support write database | PASS; ticket/access/capability/incident idempotency, hash-only secret, immutable binding/revocation; DB dropped |
| Folio runtime | Still serving `/operations` and `/platform` on port 3004 with 307 login redirects |

The next selected work remains G2-003: replace client-asserted entitlement
inputs with authoritative write-side session, role, billing, module, scope,
support-capability, and segregation-of-duties evaluation. G6 tenant
provisioning and remaining G1 read-store switching are also dependency-ready;
business-context packets remain held until that intersection is enforced.

## Final audit placeholder

The final entry must map every G0–G14 requirement to direct evidence, test names/results, migration/reconciliation reports, target build artifacts, security findings, rollback proof, and Folio command-owner retirement. Until that entry exists, the goal cannot be marked complete.

## Entry 2026-07-24 — G3-005 customer-visible detailed access-audit transport

**Owner:** Main agent integrating AG-CUS (Ohm) read-model handoff
**Dependencies:** control read/write separation, customer access-audit detail query, authenticated query boundary

### Delivered behavior

- Added an additive `GetCustomerAccessAudit` gRPC contract with typed audit entries, freshness/read-only metadata, and an opaque cursor represented by timestamp plus audit ID.
- The handler requires an authenticated transport identity, explicit query context and purpose, an explicit holding/company/branch scope, customer binding, and an active `holding_owner` or `customer_security_head` assignment.
- The application service records the customer-visible audit-detail query before reading detail rows. The repository reads projection metadata from the control read pool and audit detail from the authoritative control write pool; raw payloads are never serialized.
- Partial cursors are rejected, timestamps and IDs are validated, and page bounds are enforced by the application query contract.

### Independent verification

| Evidence | Result |
|---|---|
| `cargo check -p erpx-server` | PASS |
| `cargo clippy -p erpx-server --all-targets -- -D warnings` | PASS |
| `cargo test -p erpx-server -- --nocapture` | PASS; 33 tests |
| Fresh disposable control write/read `customer_access_audit` integration | PASS; 1 ignored test; detail pagination, query audit, payload exclusion, idempotent rebuild, orphan filtering, and read-side isolation; both databases dropped |

### Residual boundary

The Customer 360 UI still needs to consume the detailed RPC, and customer-scope filtering for multi-holding security administrators requires the organization authorization model. The global-read gateway, support/object action audit, live identity/recovery, and full universal command adoption remain open; no G3 gate is closed.

## Entry 2026-07-24 — G1-004 typed system-readiness composition

**Owner:** AG-PRV (Arendt); Main server integration
**Dependencies:** object-store readiness classifier, worker health model, tenant/projection/backup health metadata

### Delivered behavior

- Added typed worker/tenant-routing/projection-freshness/backup-PITR readiness classifiers and a composable `WorkerReadinessComposition`.
- `/readyz` exposes the sanitized composition fields and can require the full system composition through `ERPX_SYSTEM_READINESS_REQUIRED`; the default remains explicit core control/identity/storage readiness while no live worker metadata is fabricated.

### Independent verification

| Evidence | Result |
|---|---|
| Workspace formatting and strict clippy | PASS |
| `cargo test --workspace` | PASS in the integrated wave |
| Server focused tests | PASS; 33 tests |

### Residual boundary

Live worker heartbeat, tenant-routing, projection-freshness, and backup/PITR probes are not yet wired to runtime metadata, so full-system readiness remains fail-closed when required. Backup execution/restore testing and the platform operations UI remain open.

## Current integration status — 2026-07-24

The active goal remains in progress. The next dependency-ready work is the P0/P1 security adoption wave: direct disposable object-policy authorization evidence, support-capability intersection and represented-user authority, real WebAuthn/TOTP/recovery wiring, and authoritative policy adoption by every future mutation boundary. Business-context packets remain gated until those shared controls are independently verified.

## Verification addendum — 2026-07-24 customer-audit transport/readiness wave

| Evidence | Result |
|---|---|
| `bun run lint` | PASS; four pre-existing warnings in `infra/scripts/start-n8n.js`, zero errors |
| `bunx tsc --noEmit` | PASS |
| `cargo fmt --all --check` | PASS |
| `cargo clippy --workspace --all-targets -- -D warnings` | PASS |
| `cargo test --workspace` | PASS; application 6, domain 43, infrastructure 26 plus 3 ignored, identity 13 plus 5 ignored, migration 6, server 33, UI 19, worker 5 plus 1 ignored, console 5 |
| `cargo check -p erpx-console --no-default-features --features web` | PASS |
| `cargo check -p erpx-console --no-default-features --features desktop` | PASS |
| Fresh disposable `customer_access_audit` write/read pair | PASS; 1 ignored integration test; both databases dropped |
| Folio runtime | PASS; `/operations` returned 307 on port 3004 |

This is an integration-wave baseline only. It does not close any G1–G14 gate or satisfy the final requirement-by-requirement audit.

## Verification addendum — 2026-07-24 customer-audit gateway disclosure correction

The detailed Customer 360 audit handler now writes the query itself to the
authoritative `global_read_audits` table before returning historical entries;
the generic application audit event remains for internal command/query
observability. Its scope is explicit; platform scope uses the existing
platform-customer permission, while subtree scope remains rejected because it
cannot yet be represented by the customer-security scope contract.

| Evidence | Result |
|---|---|
| `cargo fmt --all --check` | PASS |
| `cargo clippy -p erpx-server --all-targets -- -D warnings` | PASS |
| `cargo test -p erpx-server -- --nocapture` | PASS; 34 tests |

No completion claim is made: UI consumption, multi-holding scope semantics,
and the wider G3/G1/G2 security boundaries remain open.

The transport was then tightened to support both authorized platform-console
reads (`platform.customer.read` at platform scope) and customer-security reads
at explicit holding/company/branch scope. Subtree scope remains rejected until
the organization model can express its full containment without widening a
customer administrator's view.

## Entry 2026-07-24 — G3-006 Customer 360 access-audit UI

**Owner:** AG-UI (Curie); independently verified by Main
**Dependencies:** additive `GetCustomerAccessAudit` RPC, existing authenticated console transport, platform global-read policy

### Delivered behavior

- The `/platform/customers/:id/access-audit` route now calls the typed gRPC method on web and desktop/native transport branches with authenticated metadata and a bounded first page of 25 entries.
- The display model retains only audit ID, actor, module, scope, purpose, timestamp, and correlation ID; session references, customer wire IDs, and raw payloads are not copied into UI state.
- Loading, denied/error, empty, stale, read-only, freshness, and continuation-disclosure states are rendered. Other Customer 360 routes remain explicit read-only placeholders.

### Independent verification

| Evidence | Result |
|---|---|
| `cargo fmt --all --check` | PASS |
| `cargo test -p erpx-ui -- --nocapture` | PASS; 19 tests |
| `cargo test -p erpx-console --no-default-features --features web -- --nocapture` | PASS; 8 tests |
| `cargo test -p erpx-console --no-default-features --features desktop -- --nocapture` | PASS; 8 tests |
| Web/desktop console checks | PASS |

### Residual boundary

The UI currently exposes only the first page, uses platform scope for the platform console, and does not yet provide Android/iOS builds, native secure storage, deep-link event plumbing, or tenant self-service customer-security scope. G3/G12 remain open.

## Entry 2026-07-24 — G5-002-004 official Stripe adapter audit

**Owner:** AG-BIL (Pascal); independently reviewed as a gap-only handoff

The packet added only regression coverage for provider-configuration secret
redaction and confirmed that the existing Stripe boundary retains typed
provider IDs, API-version/HTTPS/account validation, idempotency binding,
raw-body digest/correlation context, and deterministic provider-error
redaction. It intentionally did not invent hand-written Stripe HTTP behavior.

The required generated official OpenAPI client, credential source, typed live
adapters for Products/Prices/Entitlements/Meters/Checkout/Quotes/Portal/Tax/
Test Clocks, and sandbox/live reconciliation evidence remain open. Focused
domain/infrastructure tests, clippy, and checks passed; no network or
credentialed call was made. G5 is not closed.

## Entry 2026-07-24 — G4-006 support intersection at object boundaries

**Owner:** Main agent integrating AG-SUP/domain support-intersection handoff
**Dependencies:** persisted support request/capability state, effective RBAC,
commercial entitlements, object authorization boundary

### Delivered behavior

- The object upload/download authorization path now reconstructs the approved
  support session and hash-only capability from the control write database,
  validates the presented 256-bit bearer and immutable binding, and invokes
  the domain support-intersection evaluator before allowing the object action.
- The evaluator intersects agent identity/session/device state, platform role,
  capability mode/permission/scope/expiry, represented-user authority,
  customer entitlement and billing state, domain policy, and segregation of
  duties. Capability and approved-scope values are normalized for evaluation;
  persisted legacy scope text remains used for binding validation.
- Support capability rehydration never reads or persists the bearer secret.

### Independent verification

| Evidence | Result |
|---|---|
| `cargo fmt --all --check` | PASS |
| strict clippy for domain/application/infrastructure/server | PASS |
| domain tests | PASS; 51 tests |
| application tests | PASS; 6 tests |
| infrastructure tests | PASS; 26 tests plus 3 ignored |
| server tests | PASS; 34 tests |
| disposable support persistence test with rehydration/authentication | PASS; 1 ignored integration test; database dropped |

### Residual boundary

The current object adapter does not yet resolve live device/session status for
represented users from a dedicated identity service; those inputs are fail-
closed at the outer authentication boundary but are not yet independently
queried in the intersection request. Tenant provisioning, full support UI,
diagnostic redaction, and every non-object mutation boundary remain open.

## Entry 2026-07-24 — G3-005 scoped customer audit filtering

**Owner:** Main agent; QA finding G3-005-AUDIT-SCOPE-FILTER addressed

The customer access-audit detail query now carries an explicit platform marker
or a validated holding/company/branch scope. The control read is filtered by
that scope before pagination and freshness metadata are computed. Platform
reads may view the customer-wide audit; tenant security administrators see
platform-wide entries plus only audit scopes contained by their own explicit
organization scope. Sibling companies, branches, and holdings are excluded.

Independent evidence: the disposable two-database integration test
`customer_access_audit_detail_filters_to_security_admin_scope` passed and both
databases were dropped. Application tests (6) and workspace checks passed
through compilation. The customer-audit transport and full platform/business
read-policy adoption remain open.

## Entry 2026-07-24 — G1-003 named worker composition

**Owner:** AG-FND (Arendt); independently reviewed by Main

The worker executable now selects from a named handler registry with bounded
concurrency and unique claim identities. Customer 360, Stripe semantic, and
customer access-audit summary handlers are registered; existing claim,
ordering, idempotency, retry/backoff, checkpoint, dead-letter, replay, and
projection-rebuild primitives remain authoritative. Readiness classification
is fail-closed for absent or stale worker, tenant, projection, and backup/PITR
observations.

Independent evidence: worker all-target check/clippy passed, worker tests
passed (6 with 1 ignored), infrastructure tests passed (26 with 3 ignored),
and formatting passed. Live scheduler heartbeat publication, tenant routing,
projection freshness queries, backup/PITR execution, and server readiness
integration remain open; G1-003 is not closed.

## Entry 2026-07-24 — G1/G4 signed-object resource binding

**Owner:** Main agent; QA object-key ownership finding addressed

Added the control-write `object_access_bindings` migration and repository
boundary. Authorized uploads create an immutable
`tenant + storage namespace + object key + customer + holding/company/branch`
binding; downloads require an active matching binding. A key cannot be
rebound to another customer, tenant, namespace, or sibling scope. Parent and
descendant organization scopes may access the same bound resource according to
the existing scope containment rules.

Independent evidence: the disposable `sqlx_control_plane` integration test
passed for create, same-scope read, sibling-scope denial, and cross-tenant
denial; the disposable databases were dropped. Server and infrastructure
tests/clippy passed. Existing object URL traversal/expiry tests remain green.
Legacy object backfill and object lifecycle/revocation workflows remain open.

## Entry 2026-07-24 — G2-001 real WebAuthn provider and server wiring

**Owner:** AG-IAM (Socrates); integrated and independently checked by Main

Pinned `webauthn-rs 0.5.5` now performs browser registration and assertion
verification, including RP/origin/challenge binding, attestation/assertion
parsing, credential-ID binding, persisted passkey serialization, counter
rollback protection, and fail-closed malformed/legacy/tampered state paths.
The server accepts `ERPX_WEBAUTHN_PROVIDER=webauthn-rs` only with explicit RP
ID and allowed origins, while disabled/default configuration remains
fail-closed. Browser ceremony state is persisted in the write-side identity
challenge row and is not returned to the client; HTTP finish routes accept
browser responses and use only server-side provider state.

Independent evidence: identity unit tests passed (16), identity clippy passed,
server tests passed (34), server/identity checks passed, and the real-provider
deterministic attestation/state tests passed. Positive server/browser and
disposable identity ceremony tests, TOTP enrollment/recovery sessions,
complete device/recovery workflows, approver identity resolution, and full
universal mutation adoption remain open; G2 is not closed.

## Entry 2026-07-24 — G2-001 TOTP and recovery HTTP composition

**Owner:** AG-IAM packet; independently reviewed and tested by Main

Added authenticated TOTP enrollment start/verify routes and recovery-code to
one-time bearer session exchange routes. The server constructs the configured
AES-GCM key resolver from environment-backed key material; clients cannot pick
the encryption key reference. Enrollment and recovery lifetimes are fixed at
15 minutes, durable rate-limit buckets are applied, and only the one-time
enrollment secret/URI, recovery bearer, or newly issued recovery-code list is
returned in the corresponding authenticated success response. Stored
ciphertext, hashes, key references, and credentials remain out of HTTP DTOs.

Independent evidence:

- `cargo fmt --all --check` — PASS.
- `cargo clippy --workspace --all-targets -- -D warnings` — PASS after the
  final HTTP conditional fix.
- Server tests — PASS; 38 tests, including route/lifetime/DTO redaction
  coverage.
- Identity tests — PASS; 20 unit tests.
- Disposable identity SQLx test
  `identity_totp_and_recovery_handoffs_are_encrypted_bounded_and_one_time` —
  PASS; encrypted TOTP persistence, replay rejection, bounded recovery
  session, and one-time consumption were exercised and the database was
  dropped.

The recovery-code issue route now returns newly generated plaintext codes only
in its authenticated one-time issuance response. Passwordless WebAuthn login,
device-management end-to-end routes, and universal mutation adoption remain
open. This does not close G2.

## Entry 2026-07-24 — G1-004 durable live-readiness composition

**Owner:** AG-FND packet; independently repaired and reviewed by Main

Added control migration `0011_worker_readiness.sql`, write-side heartbeat
record/load methods, successful worker-pass heartbeat publication, and server
readiness integration. `/readyz` now requires control, identity, storage, an
explicit readiness tenant's live route probes, durable projection/backup
observations, and an expected recent worker heartbeat. Missing or stale
observations fail closed with `503`; only bounded state labels are disclosed.

Independent evidence:

- `cargo fmt --all --check` — PASS.
- `cargo clippy --workspace --all-targets -- -D warnings` — PASS.
- Focused identity/server/worker/infrastructure suites — PASS.
- Disposable control write/read `sqlx_control_plane` test — PASS after
  rebuilding the embedded migration; migration 0011 and heartbeat round-trip
  passed, databases explicitly dropped.

Readiness is deliberately not green without explicit deployment configuration
and worker/projection/backup observations. Full worker deployment wiring,
tenant backup/restore execution, and production readiness SLO evidence remain
open; G1 is not closed.

## Entry 2026-07-24 — G2-001 passwordless passkey session issuance

**Owner:** Main agent; independently reviewed and tested by Main

Passkey authentication can now begin and finish without a pre-existing bearer
session when the caller supplies the principal reference, while an already
authenticated caller must match that reference. After the browser assertion
and server-side challenge/provider-state checks succeed, the identity write
side rechecks that the principal is active and atomically creates a device and
rotating five-minute-access/refresh session. The response uses the existing
redacted session DTO; registration remains an authenticated operation.

Independent evidence:

- `cargo fmt --all --check` — PASS.
- `cargo clippy --workspace --all-targets -- -D warnings` — PASS.
- `cargo test -p erpx-server -p erpx-identity` — PASS; 38 server tests and
  20 identity tests.

Positive browser/provider integration and end-to-end disposable passkey login
remain required before G2 can close.

## Entry 2026-07-24 — G2 recovery password replacement

**Owner:** Main agent; independently reviewed and tested by Main

Added `/auth/recovery-sessions/reset-password` and the corresponding
write-side operation. A bounded recovery bearer and new password are validated
without logging either secret; the operation locks the principal, verifies the
handoff window, replaces the Argon2id digest, revokes previous sessions,
creates a new device/session, and consumes the handoff in one transaction.

Independent evidence:

- `cargo test -p erpx-server -p erpx-identity` — PASS; 38 server tests and
  20 identity tests.
- Disposable `erpx-identity` SQLx suite — PASS; all 6 ignored tests, including
  old-password rejection, new-password login, one-time reset rejection, and
  encrypted TOTP/recovery coverage. Both databases were explicitly dropped.

The broader recovery UX, approver identity resolution, and universal
authorization adoption remain open.

## Entry 2026-07-24 — G5 typed Stripe HTTP provider boundary

**Owner:** AG-BIL (Euler); independently reviewed by Main

Added a typed, bounded Stripe HTTP client for Product, Price, list, retrieve,
and Checkout Session calls. It consumes the existing pinned HTTPS provider
configuration, keeps API keys redacted and zeroized, requires idempotency keys
for mutations, bounds timeout/request/response bodies, and maps provider
failures through safe categories. The client intentionally remains separate
from the raw-body webhook boundary.

Independent evidence from the handoff:

- Infrastructure formatting, check, strict clippy, and focused Stripe tests —
  PASS; 7 focused tests and 36 infrastructure tests with 3 ignored.
- No Stripe network calls were made.

Official generated OpenAPI synchronization, catalog publishing, Checkout/
Portal/Tax/Test Clock integration, webhook reconciliation, and server/worker
adoption remain open.

The repository pin was subsequently advanced to `2026-04-22.dahlia` after
checking Stripe's official versioning page; the typed boundary and its pin
validation tests remain green.

## Entry 2026-07-24 — G4-007 bounded diagnostics redaction boundary

**Owner:** AG-SUP packet; independently completed and reviewed by Main

Added a transport-neutral diagnostics snapshot builder with an explicit
allow-list for safe configuration, bounded versions, projection lag,
failed-job and integration summaries, worker/backup health, and correlation
IDs. Forbidden material is classified and omitted before serialization;
outputs are deterministic, deduplicated, capped, and size-bounded.

Independent evidence:

- `cargo fmt --all --check` — PASS.
- `cargo test -p erpx-infrastructure support_diagnostics::tests` — PASS;
  6 tests.
- `cargo clippy -p erpx-infrastructure --all-targets -- -D warnings` — PASS.

Diagnostics HTTP/gRPC routes, live producers, incidents, and full support UI
remain open.

## Entry 2026-07-24 — G7 organization and Operations domain contract

**Owner:** AG-OPS packet; independently repaired, tested, and integrated by Main

Added the transport-neutral organization and Operations aggregate contract in
`crates/domain/src/operations.rs` and exported it from the domain crate. The
contract covers holding/company/headquarters/branch/department/team hierarchy,
the eleven Operations aggregates, explicit scope containment, immutable source
transaction references, lifecycle transitions, canonical event metadata,
idempotency context, and deterministic canonicalization for unordered
references. Operations does not own source Sales, Procurement, Inventory,
Finance, Maintenance, Support, or Integration transactions.

The first worker handoff contained macro-hygiene and export defects; Main
independently found and repaired them before accepting the packet. The final
module passes strict package clippy and the full workspace verification wave.

Independent evidence:

- `cargo fmt --all --check` — PASS.
- `cargo clippy -p erpx-domain --all-targets -- -D warnings` — PASS.
- `cargo test -p erpx-domain` — PASS; 55 tests.
- `cargo clippy --workspace --all-targets -- -D warnings` — PASS.
- `cargo test --workspace` — PASS; all non-disposable tests passed.
- `cargo check -p erpx-console --no-default-features --features web` — PASS.
- `cargo check -p erpx-console --no-default-features --features desktop` — PASS.

G7 persistence, event-driven projectors, exception SLA/reassignment workers,
Operations pages, and Folio/ERPX reconciliation remain open. No database
migration or command-owner change was made; Folio remains the operational
write owner.

## Entry 2026-07-24 — G7 Operations tenant persistence and projection primitives

**Owner:** AG-OPS packet; independently executed and repaired by Main

Added tenant write/read migrations and `SqlxOperationsStore` for the
organization/Operations aggregates. Snapshot writes and immutable source
references are transactional and optimistic-concurrency checked. Event
application rejects invalid scopes, preserves canonical envelope metadata, and
is idempotent by event ID and idempotency key. Read projection application is
ordered, duplicate-safe, and checkpointed. The SQLx migration composition
marker was updated so the clean migrator includes the new tenant migrations.

Independent evidence:

- `cargo test -p erpx-infrastructure --test operations_persistence -- --ignored --nocapture` — PASS; one test against disposable PostgreSQL write/read databases, both dropped afterward.
- `cargo test --workspace` — PASS.
- `cargo clippy --workspace --all-targets -- -D warnings` — PASS.
- `cargo fmt --all --check` — PASS.

Worker composition, exception SLA/reassignment, authenticated Operations
routes, and Folio/ERPX reconciliation remain open. No command-owner change was
made; Folio remains the operational write owner.

## Entry 2026-07-24 — G6-003 typed tenant backup/restore executor seam

**Owner:** AG-PRV packet; independently reviewed by Main

Added a typed injectable backup/restore executor boundary that binds requests
and results to an opaque tenant, rejects mismatched tenants, requires verified
replacement-read state before switching, and redacts sensitive debug output.

Independent evidence:

- `cargo test -p erpx-infrastructure --test provisioning` — PASS; 2 tests, 2 database tests ignored.
- `cargo test -p erpx-infrastructure --lib` — PASS; 42 tests, 3 database tests ignored.
- `cargo clippy -p erpx-infrastructure --all-targets -- -D warnings` — PASS.
- `cargo fmt --all --check` — PASS.

Real backup/PITR adapters, restore-test scheduling/history, live producers,
and platform operations UI remain open. No production backup command was run.

## Entry 2026-07-24 — G4-005 module-bound support capability issuance

**Owner:** Main integration of AG-SUP support packet

Support approval requests now carry an immutable bounded module, repeat
approval is rejected, and the server issues a one-time capability secret only
after customer approval. SQLx persists the 32-byte hash only and binds the
capability to the request, customer, agent, device reference, ticket, mode,
permissions, scope, module, and expiry. Migration `0013_support_module_binding`
adds a database trigger preventing module drift between request and
capability. Approval metadata stores a digest of the approval token rather than
the raw token.

Independent evidence:

- Disposable `support_persistence` PostgreSQL test — PASS; hash-only storage, immutable approval/module binding, wrong-secret failure, and revocation; database dropped afterward.
- `cargo test -p erpx-domain` — PASS; 56 tests.
- `cargo test -p erpx-server` — PASS; 38 tests.
- `cargo clippy --workspace --all-targets -- -D warnings` — PASS.
- `cargo test --workspace` — PASS.

Issuance is not yet atomic with the authoritative support command/outbox;
authenticated agent device/session resolution, expiry/cascade workers,
diagnostics/routes, incidents, and support UI remain open. Folio remains the
write owner.

## Entry 2026-07-24 — G3-006 shared Customer 360 display model

**Owner:** AG-UI packet; independently reviewed by Main

Added the shared Dioxus Customer 360 display model covering all ten platform
sections, projection freshness/read-only metadata, explicit loading/empty/
denied/stale/read-only/conflict/offline/error states, and bounded safe field
redaction. It is transport-neutral and contains no credentials or tenant
secrets.

Independent evidence:

- `cargo test -p erpx-ui` — PASS; 26 tests.
- `cargo clippy -p erpx-ui --all-targets -- -D warnings` — PASS.
- `cargo fmt -p erpx-ui -- --check` — PASS.
- Web and desktop console checks — PASS.

Native target adapters, authenticated deep-link rendering, pagination, and
the complete platform/tenant UI remain open.

## Entry 2026-07-24 — G4-005 bounded capability expiry and session cascade

**Owner:** AG-SUP packet plus Main parser/cascade integration; independently
reviewed by Main

Added an explicit `Expired` revocation reason and a bounded
`FOR UPDATE SKIP LOCKED` support-capability sweep. The sweep uses an injected
clock, honors the exact `expires_at <= now` boundary, limits batches to 1,000,
increments versions, and is retry-safe. The repository parser recognizes the
new persisted reason. Session revocation now idempotently terminates active
capabilities issued from the revoked session. Approval mirroring and
capability insertion are committed together after the authoritative aggregate
command.

Independent evidence:

- `cargo test -p erpx-infrastructure --test support_persistence -- --ignored --nocapture` — PASS; 2 disposable PostgreSQL tests.
- `cargo test -p erpx-domain` — PASS; 56 tests.
- `cargo test -p erpx-infrastructure --lib` — PASS; 42 tests, 3 ignored.
- `cargo test -p erpx-server` — PASS; 38 tests.
- `cargo clippy --workspace --all-targets -- -D warnings` — PASS.
- `cargo fmt --all --check` — PASS.

Worker scheduling, role/ticket/customer cascade revocation, authenticated
device/session resolution, diagnostics/routes, incidents, and support UI remain
open. Folio remains the write owner.

## Entry 2026-07-24 — G7-UI-025 shared Operations presentation contract

**Owner:** AG-UI packet; independently reviewed by Main

Added the shared Dioxus Operations page registry, bilingual labels, scope and
projection metadata, safe display-field boundary, explicit state machine, and
source-domain ownership model. Operations is never represented as the owner
of Sales, Procurement, Inventory, Finance, Maintenance, Support, or
Integration transactions.

Independent evidence:

- `cargo fmt -p erpx-ui -- --check` — PASS.
- `cargo test -p erpx-ui` — PASS; 32 tests.
- `cargo clippy -p erpx-ui --all-targets -- -D warnings` — PASS.

Transport DTO mapping, server authorization/routes, live projectors, native
targets, and Folio reconciliation remain open.

## Entry 2026-07-24 — G4-005 support capability expiry worker

**Owner:** AG-FND packet; independently reviewed by Main

Registered `support_capability_expiry` in the worker handler registry. Each
run invokes the typed SQLx expiry sweep with `Utc::now()` and a batch bounded
to 1,000, reports affected capability rows as processed, and maps persistence
failures to redacted worker errors. The handler is retry-safe because the
underlying expiry update is idempotent.

Independent evidence:

- `cargo test -p erpx-worker` — PASS; 12 tests, 1 ignored.
- `cargo clippy -p erpx-worker --all-targets -- -D warnings` — PASS.
- `cargo fmt --all --check` — PASS.

Role/ticket/customer cascade revocation, live worker scheduling configuration,
and support command ownership remain open.

## Entry 2026-07-24 — G4-007 authenticated diagnostics gRPC boundary

**Owner:** Main agent

Added `SupportService.GetSupportDiagnostics`. The handler requires a valid
access-token/actor/session attribution, platform query scope, the
`diagnostics:read` effective role, and a validated purpose. It builds the
bounded allow-list snapshot, records an audit event, and returns only its
serialized safe payload, schema version, and correlation ID. Worker health is
reported as `not_probed` when no live worker observation is available rather
than being presented as healthy.

Independent evidence:

- `cargo check -p erpx-server` — PASS.
- `cargo test -p erpx-server` — PASS; 38 tests.
- `cargo clippy --workspace --all-targets -- -D warnings` — PASS.

Capability-bound tenant diagnostics, live producers, incidents, HTTP/native
support routes, and customer-facing support audit views remain open.

## Entry 2026-07-24 — G4-009 shared support and incident UI contract

**Owner:** AG-UI packet; independently reviewed by Main

Added shared Dioxus support/incident route definitions, bilingual lifecycle and
mode labels, exact support duration limits, approval/impersonation/read-only
metadata, safe display-field redaction, and explicit UI state variants. No
transport, database, authorization, or capability secret was added.

Independent evidence:

- `cargo fmt -p erpx-ui -- --check` — PASS.
- `cargo test -p erpx-ui` — PASS; 38 tests.
- `cargo clippy -p erpx-ui --all-targets -- -D warnings` — PASS.

Transport mapping, incidents persistence, capability-bound diagnostics,
native targets, and live support routes remain open.

## Entry 2026-07-24 — G4-001 authoritative ticket transition transport

**Owner:** Main agent; independently verified after integration

Added `SupportService.UpdateTicket` with authenticated command-context
validation, support entitlement checking, authoritative domain lifecycle
transition, versioned command/outbox persistence, and explicit terminal
handling for `closed`. Closing a ticket also invokes the bounded ticket
capability-revocation seam. The command is intentionally documented as a
follow-up revocation call until the transaction boundary is unified.

Independent evidence:

- `cargo test -p erpx-server` — PASS; 39 tests.
- `cargo fmt --all --check` — PASS.
- `cargo clippy --workspace --all-targets -- -D warnings` — PASS.

Full intake/reply/attachment/SLA/incident-link/satisfaction coverage and
customer/native support routes remain open. Folio remains the write owner.

## Entry 2026-07-24 — G4-006 bounded support capability cascade repository

**Owner:** AG-SUP packet; independently repaired and verified by Main

Added bounded active-only cascade revocation by customer, agent, parent-role
agent reference, and ticket. PostgreSQL row locking uses `FOR UPDATE SKIP
LOCKED`; updates are versioned, timestamped, reasoned, and retry-safe. Main
added an in-process guard to the shared disposable support integration tests
after proving that the intentionally global expiry sweep caused parallel test
interference.

Independent evidence:

- Disposable `support_persistence` PostgreSQL suite — PASS; all 4 ignored
  tests in parallel, database dropped afterward.
- `cargo test -p erpx-infrastructure --lib` — PASS; 42 tests, 3 ignored.
- Strict infrastructure clippy and package formatting — PASS.

Production cascade triggers/workers and full support lifecycle integration
remain open.

## Entry 2026-07-24 — G7-003 Operations event page and scoped rebuild preparation

**Owner:** AG-OPS packet; independently reviewed and verified by Main

Added a holding-bound Operations event-page API with a 500-row cap,
aggregate/version resume cursor, deterministic ordering, typed envelope
conversion, and holding mismatch validation. Added projection-specific,
holding-scoped read-model rebuild preparation that clears checkpoints and rows
without changing write-side events, snapshots, routing, or other projections.

Independent evidence:

- Disposable separate tenant write/read PostgreSQL test — PASS; one ignored
  `operations_persistence` test, both databases dropped afterward.
- `cargo test -p erpx-infrastructure` — PASS; 42 unit tests and 2 local
  integration tests.
- Strict infrastructure clippy and package formatting — PASS.

Worker composition, projector orchestration, exception workflows, routes, and
Folio reconciliation remain open.

## Entry 2026-07-24 — G7-003 tenant-routed Operations worker seam

**Owner:** AG-FND packet; independently reviewed and verified by Main

Added `OperationsEventStreamRunner` to the worker crate. It validates bounded
tenant/holding/projection references, routes both physical tenant databases
through the control-plane registry, reads immutable Operations event pages, and
prepares explicitly named, holding-scoped read-model rebuilds. Routed pools are
closed on both success and read/rebuild failure; errors are stable and
redacted. A server-side-only pool borrow was added to the routed database
handle. The seam intentionally does not claim aggregate consumption,
scheduling, checkpoint orchestration, or reconciliation.

Independent evidence:

- `cargo fmt -p erpx-worker -- --check` — PASS.
- `cargo test -p erpx-worker` — PASS; 14 tests, 1 ignored.
- `cargo clippy -p erpx-worker --all-targets -- -D warnings` — PASS.
- Full Rust workspace and Folio verification wave — PASS.

Aggregate consumers, scheduling, retries/DLQ, replacement-read activation,
exception workflows, routes, and Folio reconciliation remain open.

## Entry 2026-07-24 — G4-008/009 authenticated incident command transport

**Owner:** AG-SUP packet; independently reviewed and verified by Main

Added additive `CreateIncident` and `UpdateIncident` RPCs to `SupportService`.
Both require authenticated actor/session attribution, platform scope, the
corresponding incident permission, bounded inputs, correlation, and safe
idempotency/version handling. The existing Incident lifecycle remains
authoritative, including terminal `postmortem_complete`; responses expose only
the safe incident summary and audit records include actor, purpose, and
correlation. Internal timeline, broadcasts, mitigation, root cause,
corrective actions, credentials, and diagnostic payloads stay out of the DTO.

Independent evidence:

- `cargo fmt --all --check` — PASS.
- `cargo test -p erpx-server` — PASS; 42 tests.
- `cargo clippy -p erpx-server --all-targets -- -D warnings` — PASS.
- Full workspace Rust and Folio verification — PASS.

Incident detail reads, projections, broadcasts, worker orchestration, and
native/UI surfaces remain open; no migration or Folio ownership change was
made.

## Entry 2026-07-24 — G4-009 audited incident detail read transport

**Owner:** Main agent; independently verified after the incident command packet

Added additive `SupportService.GetIncident`. The query requires the existing
authenticated actor/session boundary, platform scope, the
`support.incident.read` permission, a validated query context, and a UUID
incident identifier. It records an audit event before reading the existing
control-plane persistence and returns only the safe incident summary DTO,
correlation ID, and presence flag.

Independent evidence:

- `cargo test -p erpx-server` — PASS; 42 tests.
- `cargo clippy -p erpx-server --all-targets -- -D warnings` — PASS.
- `cargo fmt -p erpx-server` — PASS.

Incident projections, broadcasts, worker orchestration, and native/UI rendering
remain open. The read is additive and makes no migration or Folio ownership
change; the later bounded list transport is recorded separately.

## Entry 2026-07-24 — G4-009 shared incident list/detail UI contracts

**Owner:** AG-UI packet; independently reviewed by Main

Added transport-neutral Dioxus incident list/detail display models for the
existing platform incident routes. The adapter allow-lists only the safe
incident summary fields, parses all lifecycle statuses, provides bilingual
labels, carries platform scope and projection freshness metadata, and keeps
loading/empty/denied/stale/read-only/conflict/offline/error states distinct.
No credentials, capability secrets, internal notes, timeline, broadcast,
mitigation, root-cause, corrective-action, or tenant connection data is
represented.

Independent evidence:

- `cargo fmt -p erpx-ui -- --check` — PASS.
- `cargo test -p erpx-ui` — PASS; 42 tests.
- `cargo clippy -p erpx-ui --all-targets -- -D warnings` — PASS.

The UI contract is not a rendered transport client. Projection workers,
broadcasts, notifications, browser/native rendering, and remaining G4 support
surfaces remain open; authenticated list/detail transport is recorded in a
separate packet.

## Entry 2026-07-24 — G4-008 bounded authenticated incident list transport

**Owner:** AG-SUP packet; independently implemented and verified by Main

Added additive `SupportService.ListIncidents` with an optional closed-set
status filter and a required page size bounded to 100. The handler requires
the authenticated active-session boundary, platform scope,
`support.incident.read`, and a validated correlated query/audit context. It
records `incident_list_read:platform` before the read and returns only the
safe incident summary DTO and aggregate version. The repository uses
`updated_at DESC, id DESC`, fetches only `page_size + 1`, and exposes a
`has_more` flag. No migration, tenant credential, or Folio ownership change
was made.

Independent evidence:

- `cargo fmt --all --check` — PASS.
- `cargo check -p erpx-server` and `cargo check -p erpx-infrastructure` — PASS.
- `cargo test -p erpx-server` — PASS; 42 tests.
- `cargo test -p erpx-infrastructure --lib` — PASS; 42 tests, 3 ignored.
- Strict infrastructure/server clippy — PASS.
- Disposable PostgreSQL support persistence suite — PASS; 5 ignored tests,
  including deterministic incident list ordering, bound, status filtering,
  and retry-safe support persistence.

Incident projections, broadcasts, notifications, worker orchestration,
timeline/postmortem disclosure, and rendered browser/native UI remain open.

## Entry 2026-07-24 — G4-004 customer approval scope containment recheck

**Owner:** Main agent

The support-session approval handler now parses and canonicalizes the persisted
requested scope and requires the authenticated Holding Owner or Customer
Security Head command scope to contain that exact requested scope before an
approval can issue a capability. Non-canonical stored scope fails closed, and
the existing session/customer/permission/billing checks remain in force.

Independent evidence:

- `cargo fmt -p erpx-server -- --check` — PASS.
- `cargo test -p erpx-server` — PASS; 43 tests.
- `cargo clippy -p erpx-server --all-targets -- -D warnings` — PASS.

Authenticated device identity resolution, atomic command/capability outbox
composition, production cascade triggers/workers, universal support-action
enforcement, capability-bound diagnostics, and full customer approval workflow
remain open.

## Entry 2026-07-24 — G4-005 authenticated support device binding

**Owner:** Main agent

Identity persistence now exposes a fail-closed active-session device lookup.
Support capability approval derives the device reference from that identity
session and rejects a client-supplied mismatch. Signed object access performs
the same active-session device recheck before evaluating the support capability,
so the capability binding cannot be replayed from a different authenticated
device/session.

Independent evidence:

- `cargo fmt -p erpx-identity -p erpx-server` — PASS.
- `cargo test -p erpx-identity` — PASS; 20 tests, 6 ignored integration tests.
- `cargo test -p erpx-server` — PASS; 44 tests.
- Strict identity/server clippy — PASS.

Atomic support aggregate/capability outbox composition, universal support
action enforcement, capability-bound diagnostics, production cascade workers,
and remaining support UI remain open.

## Entry 2026-07-24 — G7-004 bounded Operations exception read/write seam

**Owner:** Main agent

Added a typed `ExceptionCase` write wrapper that delegates to the existing
tenant-scoped aggregate/source-reference transaction boundary. Added a
bounded, read-database-only active-exception page ordered by update time and
opaque aggregate ID, with explicit holding binding, metadata/key validation,
and a maximum page size of 100. Terminal/resolved cases are excluded from the
active queue; the write side remains the authoritative aggregate/event store.

Independent evidence:

- `cargo fmt --all --check` — PASS.
- `cargo test -p erpx-infrastructure --lib` — PASS; 43 tests, 3 ignored.
- `cargo clippy -p erpx-infrastructure --all-targets -- -D warnings` — PASS.

Exception detection producers, assignment/acknowledgement/escalation,
SLA scheduling, expired-user reassignment, manager escalation, authenticated
routes, rendered UI, and Folio reconciliation remain open. No migration,
worker, source-context, or Folio change was made.

## Entry 2026-07-24 — G4-001 closed-ticket recurrence linkage

**Owner:** Main agent

The support domain now treats closed tickets as immutable terminal aggregates.
`SupportTicket::recurrence` creates a new `new` ticket linked by
`recurrence_of` to the closed predecessor and rejects recurrence creation from
any non-closed ticket. The authoritative application service exposes the same
operation through the command/outbox unit of work; existing persisted payloads
remain compatible because the link defaults to absent.

Independent evidence:

- `cargo fmt -p erpx-domain -p erpx-application -p erpx-infrastructure -- --check` — PASS.
- `cargo test -p erpx-domain -p erpx-application` — PASS; 57 domain tests and 6 application tests.
- Strict clippy for domain, application, and infrastructure — PASS.

Transport intake for recurrence, ticket replies/notes/attachments/SLA,
production projections, and rendered support UI remain open. No Folio change
or database migration was required.

## Entry 2026-07-24 — G4-008 incident read projection and worker registration

**Owner:** Main agent

Added a rebuildable control-read incident summary table containing only status,
commander, leads, affected services/customers, version, and safe payload. The
repository upsert is monotonic by aggregate version and omits timeline,
broadcast, mitigation, root-cause, and corrective-action material. Added a
typed `IncidentProjection` and outbox handler that rejects unrelated aggregate
types, supports clear/rebuild, and is registered under a bounded worker name.

Independent evidence:

- `cargo check -p erpx-worker -p erpx-infrastructure` — PASS before the parallel billing packet began editing its isolated commercial files.
- Control-read migration is embedded by the existing SQLx migration root.

The current packet does not claim incident command/outbox production adoption,
server reads switched to the read projection, broadcasts/notifications,
timeline/postmortem disclosure, or full support UI. A final focused worker and
workspace verification is required after the concurrent billing handoff.

## Entry 2026-07-24 — G7-004 ExceptionCase lifecycle commands

**Owner:** AG-OPS; independently reviewed by main agent

Added typed assign, reassign, acknowledge, escalate, and resolve commands over
the existing ExceptionCase lifecycle. Commands require active half-open actor,
assignee, and manager validity proofs, bounded SLA metadata, optimistic
aggregate version, and aggregate-bound idempotency. The tenant SQLx transaction
locks the snapshot, rechecks holding metadata, writes the event and snapshot
atomically, and returns a replay result without a second event. Terminal states
and cross-holding writes fail closed.

Independent handoff evidence:

- Domain tests — PASS; 59 tests.
- Infrastructure unit tests — PASS; 47 tests, 4 ignored.
- Infrastructure all-target tests — PASS; 53 tests, 17 ignored.
- Strict domain/infrastructure clippy — PASS.
- Focused disposable PostgreSQL lifecycle test — PASS; 1 test.
- Formatting, cargo check, and `git diff --check` — PASS for owned paths.

Routes, worker scheduling, detection producers, SLA monitors, expired-user
lookup, manager hierarchy resolution, projections, UI, and Folio reconciliation
remain open.

## Entry 2026-07-24 — G5-002/G5-003 typed Stripe catalog and webhook boundary

**Owner:** AG-BIL; independently reviewed by main agent

Added deterministic typed catalog publication plans bound to account, pinned
HTTPS endpoint/API version, revision, operation, and idempotency. Product and
Price steps use typed provider IDs and separate derived idempotency keys;
publication responses map into deterministic reconciliation snapshots. Added a
strict raw-body webhook parser requiring the verified digest/correlation
context, typed event ID/timestamp/data object, and an inbox insertion seam.
Dead-letter replay is now bounded by a durable control-plane replay counter and
lease ownership.

Independent handoff evidence:

- Infrastructure tests — PASS; 47 tests, 4 ignored.
- Commercial-focused tests — PASS; 19 tests, 4 ignored.
- Stripe-client tests — PASS; 7 tests.
- Strict infrastructure clippy — PASS.
- Rustfmt and test compilation — PASS.
- Disposable Stripe inbox/replay PostgreSQL tests — PASS; 1 test each.

The official generated Stripe client, live provider adapter, server wiring to
the strict webhook path, authenticated replay controls, and remote catalog/
entitlement reconciliation remain open.

## Entry 2026-07-24 — integrated verification wave after G4/G5/G7 packets

**Owner:** Main agent

Independently verified the integrated worktree after the recurrence,
capability-bound diagnostics, incident read projection, exception lifecycle,
and typed Stripe catalog/webhook packets.

Results:

- `cargo fmt --all --check` — PASS.
- `cargo clippy --workspace --all-targets -- -D warnings` — PASS.
- `cargo test --workspace` — PASS; server 48, infrastructure 51 unit tests
  with 4 ignored, worker 14 with 1 ignored, UI 42, domain 59, identity 20,
  migration 6, application 6, console 9; disposable integration tests remain
  explicitly ignored when no test database URL is provided.
- `cargo check -p erpx-console --no-default-features --features web` — PASS.
- `cargo check -p erpx-console --no-default-features --features desktop` — PASS.
- `bun run lint` — PASS with four pre-existing warnings in
  `infra/scripts/start-n8n.js`, zero errors.
- `bunx tsc --noEmit` — PASS.
- `git diff --check` — PASS.

- PostgreSQL readiness — PASS (`127.0.0.1:5432` accepting connections).
- Folio `/operations` and `/platform` — HTTP 307; existing Next dev process
  PID 14893 remains on port 3004.

The persistent ERPX goal remains active. The integrated wave does not close
any G1-G14 gate: live server Stripe strict-ingress adoption, full support
workflow, Operations producers/workers/routes/UI, all remaining ERP modules,
native targets, migration cutover, and Folio write retirement remain open.

## Entry 2026-07-24 — G4-007 capability-bound diagnostics

**Owner:** AG-SUP; independently reviewed by main agent

Added an additive diagnostics access context with explicit platform and
customer-support modes. Platform diagnostics reject capability material and
remain platform-role scoped. Customer-support diagnostics now fail closed until
the authenticated session/device, capability secret/hash, support session and
ticket, canonical scope, diagnostic mode, module, entitlement, billing,
platform role chain, SoD, and domain support-intersection checks all pass.
The existing bounded allow-list/redaction snapshot remains the only response
path, and customer-support reads retain customer attribution in the audit.

Independent handoff evidence:

- Server tests — PASS; 48 tests.
- Strict server clippy — PASS.
- Infrastructure tests — PASS; 43 tests, 3 ignored.
- Strict infrastructure clippy — PASS.
- Packet-local rustfmt — PASS.

The packet does not claim live diagnostic producers, full support command
coverage, production cascade triggers/workers, incident broadcasts, or native
UI. The full workspace rerun remains pending the isolated billing compile fix
and worker formatter cleanup.

## Entry 2026-07-24 — G5 strict server webhook ingress adoption

**Owner:** Main agent; independently verified against the AG-BIL contract

The HTTP Stripe webhook route now verifies the signature over the exact raw
request body before parsing. It constructs `StripeWebhookPersistenceContext`
only after verification, passes the context and bytes through
`StripeWebhookEvent::from_verified_raw_body`, and persists only through
`SqlxCommercialStore::insert_verified_stripe_webhook`. The prior manual,
optional-context HTTP insertion path is no longer used. The typed parser also
requires a valid event ID/type/timestamp and `data.object`, and the route
returns a bounded invalid-request response for incomplete envelopes.

Focused evidence:

- `cargo fmt -p erpx-server -- --check` — PASS after formatting.
- `cargo test -p erpx-server stripe_webhook_tests -- --nocapture` — PASS; 4
  tests, including valid typed ingress and incomplete-envelope rejection.
- `cargo clippy -p erpx-server --all-targets -- -D warnings` — PASS.

Official generated Stripe client/API synchronization, live provider execution,
worker composition, authenticated replay controls, remote catalog and
entitlement reconciliation, and the remaining billing commands remain open.

## Entry 2026-07-24 — G4 support ticket read and recurrence transport

**Owner:** Main agent; shared contract and server integration

Added additive `SupportService.GetTicket` and
`SupportService.CreateTicketRecurrence` RPCs. Ticket detail reads require an
authenticated query context, active `support.ticket.read`, customer-scope
containment, and a non-empty purpose; the server records a customer-attributed
audit event and returns only the safe ticket DTO. Recurrence commands require
the authenticated `support.ticket.create` permission and entitled support
mutation authorization, load the closed predecessor from the write side, and
delegate to the existing terminal-ticket recurrence aggregate rule.

Focused evidence:

- `cargo fmt -p erpx-server -- --check` — PASS.
- `cargo check -p erpx-proto` — PASS.
- `cargo test -p erpx-server` — PASS; 50 tests.
- `cargo clippy -p erpx-server --all-targets -- -D warnings` — PASS.

The application service now rehydrates the stored aggregate on an idempotent
recurrence replay. Replies, internal-note/reply transport adoption,
attachments, SLA workers, ticket lists, and customer/native support surfaces
remain open.

## Entry 2026-07-24 — integrated G4/G5/G7/G12 dependency-ready wave

**Owner:** Main agent with independently verified AG-SUP, AG-BIL, AG-OPS, and
AG-UI handoffs

Integrated and reviewed four bounded packets:

- G4-002: audience-separated customer replies/internal notes, bounded
  assignment/escalation/SLA commands, terminal-ticket enforcement, deterministic
  create/recurrence IDs, replay receipt rehydration, and atomic support-ticket
  metadata persistence.
- G5-002: typed Product-before-Price Stripe provider execution with bound
  idempotency keys, account/API-version pin checks, typed response mapping, and
  fail-closed provider mismatch/error handling.
- G7-003: validated contracts and lifecycle transitions for ten Operations
  aggregates, immutable source provenance, scope containment, canonical replay
  serialization, and persistence provenance matching.
- G12: shared Dioxus support/incident route surfaces with safe display
  redaction, bilingual labels, responsive/accessibility-oriented presentation,
  and explicit loading/empty/denied/stale/read-only/conflict/offline states.

Independent integrated verification:

- `cargo fmt --all --check` — PASS.
- `cargo clippy --workspace --all-targets -- -D warnings` — PASS.
- `cargo test --workspace` — PASS: application 7, console 11, domain 64,
  identity 20 (6 disposable tests ignored), infrastructure 50 unit with 4
  disposable unit tests ignored, migration 6, server 50, UI 48, worker 14
  with 1 disposable test ignored; all doc tests passed; disposable database
  integration tests without configured URLs remained explicitly ignored.
- `cargo check -p erpx-console --no-default-features --features web` — PASS.
- `cargo check -p erpx-console --no-default-features --features desktop` — PASS.
- `bun run lint` — PASS with four pre-existing warnings in
  `infra/scripts/start-n8n.js`, zero errors.
- `bunx tsc --noEmit` — PASS.
- `git diff --check` — PASS.
- PostgreSQL readiness — PASS (`127.0.0.1:5432` accepting connections).
- Folio runtime — existing PID 14893 remains on port 3004; `/operations` and
  `/platform` both return HTTP 307.

The packet owners independently reported no implementation errors. The full
wave does not close any G1-G14 gate: support transport adoption beyond the
added ticket reads/recurrence, worker schedules/producers, full Stripe
reconciliation and billing commands, Operations source producers/workers/
routes/UI/Folio reconciliation, all remaining ERP contexts, native target
evidence, migration cutover, and Folio write retirement remain open.

## Entry 2026-07-24 — G4-002 reply and internal-note transport adoption

**Owner:** Main agent; shared proto and server integration

Added additive `SupportService.AddTicketReply` and
`SupportService.AddTicketInternalNote` RPCs. Both require authenticated
command context, scoped role permission, an existing ticket, and entitled
support authorization before delegating to the authoritative application
service. The response mapper exposes customer-visible replies and excludes
internal notes by construction. Domain validation, terminal-ticket behavior,
optimistic concurrency, outbox, audit, and idempotency remain on the write
side.

Evidence: `erpx/audits/G4-002-TICKET-MESSAGE-046/evidence/README.md`.

Independent focused verification:

- `cargo fmt --package erpx-server --package erpx-proto` — PASS.
- `cargo test -p erpx-server` — PASS; 51 tests.
- `cargo clippy -p erpx-server --all-targets -- -D warnings` — PASS.

Attachments, ticket list/search transport, SLA timers and notifications,
incident producers/broadcasts, and live/native support UI remain open.

## Entry 2026-07-24 — integrated G2/G3/G6 repair and verification wave

**Owner:** Main agent with independently reviewed AG-IAM, AG-CUS, and AG-PRV
handoffs

Integrated and repaired three dependency-ready packets:

- G2-005: deterministic role expiry review classifications and notifications,
  reviewed renewal invariants, finite final Platform Root/Holding Owner
  recovery materialization, and authoritative application seams. The packet
  remains transport/persistence-ready only; identity/approver/session/SoD and
  worker enforcement remain open.
- G3-006: typed complete Customer 360 detail sections, freshness and absent or
  unsupported states, authoritative write-pool scope binding, deterministic
  redacted notes paging, and customer-detail read infrastructure seam.
- G6-003: bounded tenant-scoped restore-test schedule/claim/lease/retry/history
  policy, replacement-read verification requirement, opaque-reference
  redaction, and explicit fail-closed worker composition seam.

Independent repairs addressed generic `CustomerReadView` construction,
customer scope match exhaustiveness, deterministic test ownership, backup
claim attempt propagation, and the new role-review error mapping at the server
boundary. The original blocked handoff reports were not accepted as evidence.

Integrated verification:

- `cargo fmt --all --check` — PASS.
- `cargo test --workspace` — PASS: application 8, console 11, domain 73,
  identity 20 (6 disposable-DB tests ignored), infrastructure 54 with 4
  disposable-DB tests ignored, migration 6, server 51, UI 48, worker 15 with
  1 disposable-DB test ignored; all doc tests passed.
- `cargo clippy --workspace --all-targets -- -D warnings` — PASS.
- `cargo check -p erpx-console --no-default-features --features web` — PASS.
- `cargo check -p erpx-console --no-default-features --features desktop` — PASS.
- `bun run lint` — PASS with four existing warnings in
  `infra/scripts/start-n8n.js`, zero errors.
- `bunx tsc --noEmit` — PASS.
- PostgreSQL readiness — PASS (`127.0.0.1:5432` accepting connections).
- Folio runtime — PID 14893 remains on port 3004; `/operations` and
  `/platform` return HTTP 307 with redirects disabled.

The G2, G3, and G6 gates remain open because persistence/worker/transport,
full universal authorization, real backup/restore execution, native targets,
all remaining ERP contexts, migration cutover, and Folio write retirement are
not complete.

## Entry 2026-07-24 — integrated G1/G2/G4/G5 dependency-ready wave

**Owner:** Main agent with independently verified AG-FND, AG-SUP, and AG-BIL
handoffs

Integrated and reviewed:

- G1-003 durable outbox claim leases, aggregate ordering, bounded retry/DLQ/
  replay, sanitized errors, strict projection checkpoint sequencing, duplicate
  and out-of-order handling, receipt identity validation, and replacement-read
  rebuild/switch precondition seams.
- G2-005 authenticated scope-filtered role review and final-role recovery
  transport, with authoritative recovery start time and safe role DTOs.
- G4-002 bounded attachment metadata, opaque object references, visibility and
  expiry policy, SLA warning/breach classification, and deterministic
  audience-specific notification keys.
- G5-004 provider-only commercial command contexts, seven-day grace/read-only
  policy, capacity allocation and module activation constraints, and provider
  event replay semantics.

Independent integrated verification:

- `cargo fmt --all --check` — PASS.
- `cargo clippy --workspace --all-targets -- -D warnings` — PASS.
- `cargo test --workspace` — PASS: application 8, console 11, domain 80,
  identity 20 (6 disposable-DB tests ignored), infrastructure 54 with 4
  disposable-DB tests ignored, migration 6, server 51, UI 48, worker 15 with
  1 disposable-DB test ignored; all doc tests passed.
- `cargo check -p erpx-console --no-default-features --features web` — PASS.
- `cargo check -p erpx-console --no-default-features --features desktop` — PASS.
- `bun run lint` — PASS with four existing warnings in
  `infra/scripts/start-n8n.js`, zero errors.
- `bunx tsc --noEmit` — PASS.
- PostgreSQL readiness — PASS (`127.0.0.1:5432` accepting connections).
- Folio runtime — PID 14893 remains on port 3004; `/operations` and
  `/platform` return HTTP 307 with redirects disabled.

These packets advance foundations only. Durable projector replacement-read
execution, support attachment persistence and notification workers, Stripe
generated client/reconciliation and runtime command adoption, universal
authorization, remaining ERP contexts, native target evidence, migration
cutover, and Folio write retirement remain open.

## Entry 2026-07-24 — integrated G2/G4/G7/G8/G12 persistence and detail wave

**Owner:** Main agent; independent review of bounded AG-SUP, AG-OPS, and AG-FIN
source changes plus main-owned G2 and Customer 360 integration

Integrated and independently reviewed:

- G2-005 durable role-review snapshots, exact-scope notification claims with
  leases, idempotency/hash binding, and recovery approval receipts. The
  authoritative role assignment aggregate remains the command source of truth.
- G4-002 transactional support attachment metadata persistence: ticket locking,
  customer/ticket binding, typed scope containment, bounded active/customer-safe
  reads, optimistic revoke/expiry, sweep claiming, and replay-safe idempotency.
  Object bytes, signed URLs, credentials, and capability secrets remain outside
  this repository.
- G7-002/G7-003 closed typed Operations source-event integration for all seven
  declared source domains, deterministic event identity, provenance, scope and
  holding checks, bounded reconciliation pages, and exception lifecycle test
  fixtures. Operations remains orchestration-only.
- G8-001/G8-002 validated Finance domain boundary for chart hierarchy, balanced
  journals, period locks/close, immutable posting, idempotent replay, scope
  binding, and deterministic trial balances.
- G12/G3 Customer 360 detail transport consumption in the Dioxus console for
  the non-overview sections, including audited read metadata, loading/error/
  unsupported/absent/read-only states, and bounded notes request parameters.
- Cross-context compile repairs for attachment policy error redaction in server
  and worker boundaries, and the support integration test's dead-code module
  annotation required by strict all-targets linting.

Evidence:

- `audits/G2-005-ROLE-PERSISTENCE-048/README.md`
- `audits/G4-002-ATTACHMENT-PERSISTENCE-048/README.md`
- `audits/G7-002-OPERATIONS-EVENT-INTEGRATION-050/README.md`
- `audits/G8-001-002-FINANCE-DOMAIN-050/README.md`

Independent integrated verification:

- `cargo fmt --all --check` — PASS.
- `cargo clippy --workspace --all-targets -- -D warnings` — PASS.
- `cargo test --workspace` — PASS: application 8, console 11, domain 87,
  identity 20 (6 disposable PostgreSQL tests ignored), infrastructure 65
  library tests with 4 ignored plus integration tests (database-backed tests
  explicitly ignored without disposable URLs), migration 6, server 51, UI
  48, worker 15 with 1 disposable database test ignored; all doc tests passed.
- `cargo check -p erpx-console --no-default-features --features web` — PASS.
- `cargo check -p erpx-console --no-default-features --features desktop` — PASS.
- `bun run lint` — PASS with four pre-existing warnings in
  `infra/scripts/start-n8n.js`, zero errors.
- `bunx tsc --noEmit` — PASS.
- `git diff --check` — PASS.
- PostgreSQL readiness — PASS (`127.0.0.1:5432` listening).
- Folio runtime — PID 14893 remains on port 3004; `/operations` and
  `/platform` return HTTP 307 with redirects disabled.

The packet advances foundations but closes no G1-G14 gate. Disposable
PostgreSQL execution remains to be run with configured isolated URLs. Durable
role-review worker delivery and universal authorization, support attachment
object storage/signed transport/SLA workers, Stripe generated client and full
runtime adoption, Operations source producers/routes/UI/Folio reconciliation,
Finance persistence and all downstream modules, native Android/iOS evidence,
migration/cutover, and final Folio write retirement remain open.

## Entry 2026-07-24 — integrated G4/G10 worker and consent foundations

**Owner:** Main agent

Delivered and independently verified:

- G10-001/G10-002: tenant-bound Marketing profiles, deterministic deduplication,
  consent by purpose/channel/source/jurisdiction/policy version, pending
  double-opt-in and revocation, suppression, bounded audiences, campaign
  lifecycle, and send-time idempotent authorization.
- G4-002: bounded attachment expiry worker backed by the real SQLx support
  repository, injected SLA notification handler seam, half-open notification
  windows, fail-closed redacted worker errors, and default registry wiring for
  attachment expiry.

Evidence:

- `audits/G10-001-002-MARKETING-DOMAIN-051/README.md`
- `audits/G4-002-ATTACHMENT-WORKER-051/README.md`

Verification:

- `cargo fmt --all` — PASS.
- `cargo clippy -p erpx-domain --all-targets -- -D warnings` — PASS.
- `cargo test -p erpx-domain` — PASS: 91 tests.
- `cargo clippy -p erpx-worker --all-targets -- -D warnings` — PASS.
- `cargo test -p erpx-worker --lib` — PASS: 17 passed, 1 disposable test
  ignored.

Persistence, channel/journey/CRM/attribution delivery, durable SLA claims,
signed support objects, all remaining Finance/ERP modules, native clients,
migration cutover, and Folio write retirement remain open.

## Entry 2026-07-24 — integrated G8 Finance SQLx persistence wave

**Owner:** Main agent

Delivered and independently verified:

- G8-001/G8-002 tenant-routed `SqlxFinanceStore` for validated charts,
  accounting periods, immutable posted journals, request-hash idempotency,
  optimistic versions, period locking, read-side trial-balance calculation,
  and atomic Finance aggregate-snapshot/outbox insertion for posted journals.
- Tenant write migration `0017_finance.sql` stores authoritative aggregate
  state and idempotency receipts with forced holding RLS.
- Tenant read migration `0017_finance.sql` defines a separate disposable
  posted-journal projection table with forced holding RLS; it is not used as a
  write authority.
- Migration tests now assert that the Finance write/read sets remain separate
  and include forced RLS.

Evidence: `audits/G8-001-002-FINANCE-SQLX-052/README.md`.

Independent packet verification:

- `cargo fmt --all --check` — PASS.
- `cargo check -p erpx-infrastructure` — PASS.
- `cargo test -p erpx-infrastructure --lib` — PASS: 63 passed, 4 disposable
  PostgreSQL tests ignored.
- `cargo clippy -p erpx-infrastructure --all-targets -- -D warnings` — PASS.
- `cargo test -p erpx-migration` — PASS: 6 passed.

The G8 gate remains open: no AP/AR, banking, budgets, consolidation, Thai tax,
Finance transport, projector worker, or migration reconciliation is complete.

## Entry 2026-07-24 — integrated G4 durable SLA notification claims

**Owner:** Main agent

Delivered and independently verified:

- G4-002 control-plane migration `0017_support_sla_notifications.sql` with
  deterministic notification keys, audience separation, bounded windows, and
  claim/delivery lease state.
- `SupportPersistence` enqueue, idempotency-conflict, bounded `SKIP LOCKED`
  claim/reclaim, and delivery-acknowledgement methods.
- The real repository implements the worker's
  `SupportSlaNotificationSource`; channel delivery remains intentionally
  downstream of this persistence packet.

Evidence: `audits/G4-002-SLA-NOTIFICATION-053/README.md`.

Verification:

- `cargo fmt --all --check` — PASS.
- `cargo test -p erpx-infrastructure --lib` — PASS: 64 passed, 4 disposable
  PostgreSQL tests ignored.
- `cargo clippy -p erpx-infrastructure --all-targets -- -D warnings` — PASS.
- `cargo test -p erpx-worker --lib` — PASS: 17 passed, 1 disposable test
  ignored.
- `cargo clippy -p erpx-worker --all-targets -- -D warnings` — PASS.
- `cargo test -p erpx-migration` — PASS: 6 passed.

Ticket SLA producer invocation, actual notification providers, and full
support runtime composition remain open.

## Entry 2026-07-24 — integrated G10 Marketing SQLx persistence wave

**Owner:** Main agent; reviewed AG-MKT bounded handoff

Integrated and independently verified:

- Tenant write persistence for validated Marketing profiles with canonical
  dedupe keys and optimistic versions.
- Immutable consent append/revoke records with transaction-local idempotency
  hash binding and profile ownership checks.
- Suppression upsert and bounded audience membership replacement with atomic
  normalized membership rows, profile existence checks, version/hash receipts,
  and keyset-paginated read-pool access.
- Separate tenant write/read migration `0018_marketing.sql` files with forced
  `holding_ref` RLS; read rows carry projection source versions and are not
  command authority.
- Exported `SqlxMarketingStore`, receipts, page DTO, and page bound from the
  infrastructure crate. Fixed the nullable SQLx JSON response binding and
  made migration assertions version-aware.

Evidence: `audits/G10-MKT-003-052/README.md`.

Independent integrated verification:

- `cargo fmt --all --check` — PASS.
- `cargo check -p erpx-infrastructure` — PASS.
- `cargo test -p erpx-infrastructure --lib` — PASS: 67 passed, 4 disposable
  PostgreSQL tests ignored (including three Marketing persistence tests).
- `cargo clippy -p erpx-infrastructure --all-targets -- -D warnings` — PASS.
- `cargo test -p erpx-migration` — PASS: 6 passed, including Finance and
  Marketing migration/RLS assertions.

Outbox emission, Marketing projectors, delivery channels, journeys, CRM,
attribution, provider reconciliation, and database-backed execution remain
open.

Combined wave verification after G4/G8/G10 integration:

- `cargo fmt --all --check` — PASS.
- `cargo clippy --workspace --all-targets -- -D warnings` — PASS.
- `cargo test --workspace` — PASS: application 8, console 11, domain 91,
  identity 20, infrastructure 67 library tests plus the documented
  disposable-PostgreSQL integration skips, migration 6, server 51, UI 48,
  worker 17 plus one documented skip; all doc tests passed.
- `cargo check -p erpx-console --no-default-features --features web` — PASS.
- `cargo check -p erpx-console --no-default-features --features desktop` — PASS.
- `bun run lint` — PASS with four pre-existing n8n warnings, zero errors.
- `bunx tsc --noEmit` — PASS.
- Folio `/operations` and `/platform` — HTTP 200; port 3004 remains served.
- PostgreSQL — port 5432 remains listening.

## Entry 2026-07-24 — integrated G9 Sales domain contract slice

**Owner:** Main agent; reviewed AG-SCM bounded handoff

Integrated and independently verified:

- Tenant-bound customer master references, leads, opportunities, quotes, sales
  orders, delivery/invoice handoffs, and collection state.
- Explicit company/branch scope containment, customer binding, deterministic
  aggregate references and namespaced idempotency keys, monotonic lifecycle
  transitions, bounded integer totals, currency/line validation, and payment
  overage protection.
- Corrected the handoff fixture to preserve monotonic transition timestamps and
  boxed diagnostic scopes in `SalesError` so strict clippy remains clean.

Evidence: `audits/G9-SALES-001-052/README.md`.

Verification:

- `cargo fmt --all --check` — PASS.
- `cargo test -p erpx-domain` — PASS: 96 tests, including 5 Sales tests.
- `cargo clippy -p erpx-domain --all-targets -- -D warnings` — PASS.

Sales persistence/transport, procurement, inventory, logistics, source event
production, Operations reconciliation, and the full G9 workflow gate remain
open.

## Entry 2026-07-24 — integrated G9 Sales SQLx persistence seam

**Owner:** Main agent

Integrated and independently verified:

- `SqlxSalesStore` with tenant-scoped write/read pools and holding-reference
  validation.
- Typed persistence for Lead, Opportunity, Quote, SalesOrder,
  DeliveryHandoff, InvoiceHandoff, and CollectionState payloads.
- Optimistic aggregate versions and request-bound idempotency receipts.
- Atomic Sales aggregate row, shared tenant aggregate snapshot, outbox event,
  and idempotency receipt commit.
- Separate tenant write/read migration `0019_sales.sql` files with forced RLS;
  read rows expose source versions and are not command authority.
- Read projection application enforces payload identity, contiguous aggregate
  versions, idempotent replay, per-aggregate checkpoints, and transactional
  rebuild clearing.
- Bounded immutable source-event pages expose stable aggregate/type/version
  cursors for tenant worker ingestion without consulting the read database.
- Migration assertions and public infrastructure exports.

Evidence: `audits/G9-SALES-002-SQLX-054/README.md`.

Verification:

- `cargo fmt --all --check` — PASS.
- `cargo check -p erpx-infrastructure` — PASS.
- `cargo test -p erpx-infrastructure --lib` — PASS: 69 passed, 4 documented
  disposable-PostgreSQL tests ignored.
- `cargo clippy -p erpx-infrastructure --all-targets -- -D warnings` — PASS.
- `cargo test -p erpx-migration` — PASS: 6 passed.

Sales transport, projector execution, source event production, procurement,
inventory, logistics, and end-to-end reconciliation remain open.

## Entry 2026-07-24 — integrated G9 Sales tenant projector seam

**Owner:** Main agent

Integrated and independently verified:

- Stable bounded Sales source-event pages from the tenant write database.
- Tenant-routed `SalesProjectionRunner` with page application and
  clear-and-replay rebuild orchestration.
- Redacted worker errors and bounded tenant/page request validation.
- Infrastructure exports for source cursors/events/pages and projection
  reports.

Evidence: `audits/G9-SALES-003-056/README.md`.

Verification:

- `cargo fmt --all --check` — PASS.
- `cargo check -p erpx-worker` — PASS.
- `cargo test -p erpx-worker --lib` — PASS: 18 passed, 1 documented
  disposable-PostgreSQL test ignored.
- `cargo clippy -p erpx-worker --all-targets -- -D warnings` — PASS.
- `cargo check -p erpx-infrastructure` — PASS.
- `cargo test -p erpx-infrastructure --lib` — PASS: 69 passed, 4 documented
  disposable-PostgreSQL tests ignored.
- `cargo clippy -p erpx-infrastructure --all-targets -- -D warnings` — PASS.

Durable tenant event claiming/scheduling, Sales transport, and the full G9
workflow gate remain open.

## Entry 2026-07-24 — integrated G9 SCM SQLx persistence seam

**Owner:** Main agent; reviewed AG-SCM bounded handoff

Integrated and independently verified:

- Tenant-routed `SqlxScmStore` for Product, Warehouse, PurchaseRequest,
  PurchaseOrder, GoodsReceipt, StockMovement, StockTransfer, and
  StockReservation.
- Domain validation, holding-reference RLS, optimistic versions, request-bound
  idempotency, and atomic source/snapshot/outbox/receipt writes.
- Separate tenant write/read migration `0020_scm.sql` files with forced RLS.
- Bounded source-event paging, typed read projection application, contiguous
  checkpoints, idempotent replay, and rebuild clearing.
- Public infrastructure exports and version-aware migration assertions.

Evidence: `audits/G9-SCM-003-055/README.md`.

Verification:

- `cargo fmt --all --check` — PASS.
- `cargo test -p erpx-infrastructure --lib` — PASS: 72 passed, 4 documented
  disposable-PostgreSQL tests ignored.
- `cargo clippy -p erpx-infrastructure --all-targets -- -D warnings` — PASS.
- `cargo test -p erpx-migration` — PASS: 6 passed.

SCM transport, worker composition, physical inventory ledger, supplier billing,
Finance/Operations event handoff, and the Procure-to-Pay gate remain open.

## Entry 2026-07-24 — integrated G9 SCM domain contracts

**Owner:** Main agent; reviewed AG-SCM bounded handoff

Integrated and independently verified:

- Tenant/company/branch-scoped vendor, product, warehouse, purchasing, and
  inventory contracts.
- Deterministic references and namespaced idempotency keys.
- Closed purchase request/order/receipt, stock movement/transfer, and
  reservation lifecycles with approval, issued-order, over-receipt, and
  availability guards.
- Integer quantities/minor-unit costs and checked weighted-average costing with
  deterministic recosting.

Evidence: `audits/G9-SCM-002-052/README.md`.

Verification:

- `cargo fmt --all --check` — PASS.
- `cargo test -p erpx-domain` — PASS: 106 tests, including four SCM tests.
- `cargo clippy -p erpx-domain --all-targets -- -D warnings` — PASS.

Persistence, migrations, repositories, projectors, transport, UI, physical
inventory ledger, supplier billing, and the Procure-to-Pay gate remain open.

## Entry 2026-07-24 — integrated G11 Assets/Maintenance domain contracts

**Owner:** Main agent; reviewed AG-AST bounded handoff

Integrated and independently verified:

- Holding/company/optional-branch asset scope with contained command authority.
- Asset service/maintenance/impairment/revaluation/disposal lifecycle and
  checked non-negative accounting values.
- Immutable custody/location transfers with actor attribution.
- Preventive/corrective maintenance work orders, parts/labor costs, downtime,
  MTBF/availability, and explicit Operations capacity-impact handoff.
- Deterministic references/idempotency and optimistic aggregate versions.

Evidence: `audits/G11-AST-001-052/README.md`.

Verification:

- `cargo fmt --all --check` — PASS.
- `cargo test -p erpx-domain` — PASS: 106 tests, including seven Asset and
  Maintenance tests.
- `cargo clippy -p erpx-domain --all-targets -- -D warnings` — PASS.

Persistence, migrations, projectors, workers, transport, UI, accounting
journal handoff, HR, Legal, and remaining ERP packets remain open.

## Entry 2026-07-24 — integrated G11 Asset/Maintenance SQLx persistence seam

**Owner:** Main agent; independently implemented after the support handoff
stalled

Integrated and independently verified:

- Tenant-routed `SqlxAssetMaintenanceStore` for Asset and
  MaintenanceWorkOrder aggregates.
- Domain validation, holding-reference RLS, optimistic contiguous versions,
  request-bound idempotency, and atomic source/snapshot/outbox/receipt writes.
- Append-only normalized custody history and source-owned Operations
  capacity-impact handoff persistence.
- Separate tenant write/read migration `0021_asset_maintenance.sql` files,
  forced RLS, immutable custody trigger, read checkpoints, projection replay,
  and rebuild clearing.
- Public infrastructure exports and migration assertions.

Evidence: `audits/G11-AST-002-055/README.md`.

Verification:

- `cargo fmt --all --check` — PASS.
- `cargo test -p erpx-infrastructure --lib` — PASS: 75 passed, 4 documented
  disposable-PostgreSQL tests ignored.
- `cargo clippy -p erpx-infrastructure --all-targets -- -D warnings` — PASS.
- `cargo check -p erpx-infrastructure` — PASS.
- `cargo test -p erpx-migration` — PASS: 6 passed.

Asset/Maintenance worker composition, transport, GL posting, Operations
projection consumption, HR/Legal integration, and reconciliation remain open.

## Entry 2026-07-24 — integrated SCM/Assets domain wave verification

Independent integration-wave verification after G9 SCM and G11 Assets/
Maintenance domain contracts:

- `cargo fmt --all --check` — PASS.
- `cargo clippy --workspace --all-targets -- -D warnings` — PASS.
- `cargo test --workspace` — PASS: application 8, console 11, domain 106,
  identity 20 plus documented disposable-PostgreSQL skips, infrastructure 69
  plus documented skips, migration 6, server 51, UI 48, worker 17 plus one
  documented skip; all doc tests passed.
- `cargo check -p erpx-console --no-default-features --features web` — PASS.
- `cargo check -p erpx-console --no-default-features --features desktop` — PASS.
- `bun run lint` — PASS with the four pre-existing n8n-script warnings and
  zero errors.
- `bunx tsc --noEmit` — PASS.
- Folio dev server — PID 14894, port 3004 listening; `/operations` and
  `/platform` return the expected unauthenticated redirect (HTTP 307).
- PostgreSQL — port 5432 listening.

This verifies integration health only; persistence, transport, projectors,
business workflow gates, mobile CI, and final migration requirements remain
open.

## Entry 2026-07-24 — full post-persistence integration-wave verification

- `cargo fmt --all --check` — PASS.
- `cargo clippy --workspace --all-targets -- -D warnings` — PASS.
- `cargo test --workspace` — PASS: application 8, console 11, domain 106,
  identity 20 plus six documented disposable-PostgreSQL skips,
  infrastructure 75 plus four documented skips, migration 6, server 51, UI
  48, worker 18 plus one documented skip; all doc tests passed.
- `cargo check -p erpx-console --no-default-features --features web` — PASS.
- `cargo check -p erpx-console --no-default-features --features desktop` — PASS.
- `bun run lint` — PASS with four pre-existing n8n-script warnings and zero
  errors.
- `bunx tsc --noEmit` — PASS.
- Folio dev server — PID 14894, port 3004 listening; `/operations` and
  `/platform` return HTTP 307 unauthenticated redirects.
- PostgreSQL — port 5432 listening.

This is an integration-health gate only. Production transport, business
workflow completion, migration/cutover, mobile CI, security/reliability
audits, and Folio write retirement remain open.

## Entry 2026-07-24 — integrated G11 Asset/Maintenance tenant projector seam

**Owner:** Main agent

Integrated and independently verified:

- Bounded holding-scoped Asset/Maintenance source-event pages from the tenant
  write outbox with stable aggregate/type/version cursors.
- Tenant-routed page projection and clear-and-replay rebuild orchestration.
- Redacted worker errors and bounded tenant, holding, cursor, and page-size
  request validation.
- Worker request coverage and public infrastructure exports.

Evidence: `audits/G11-AST-003-057/README.md`.

Verification:

- `cargo fmt --all --check` — PASS.
- `cargo check -p erpx-infrastructure` — PASS.
- `cargo test -p erpx-infrastructure --lib` — PASS: 75 passed, 4 documented
  disposable-PostgreSQL tests ignored.
- `cargo clippy -p erpx-infrastructure --all-targets -- -D warnings` — PASS.
- `cargo check -p erpx-worker` — PASS.
- `cargo test -p erpx-worker --lib` — PASS: 19 passed, 1 documented
  disposable-PostgreSQL test ignored.
- `cargo clippy -p erpx-worker --all-targets -- -D warnings` — PASS.

Durable per-tenant scheduling/claiming, transport, GL posting, Operations
consumption, HR/Legal integration, and the G11 workflow gate remain open.

## Entry 2026-07-24 — integrated G9 SCM tenant projector runner

**Owner:** Main agent; reviewed AG-SCM bounded handoff

Integrated and independently verified:

- Exported the standalone SCM tenant-routed projector module from the worker
  crate.
- Bounded page projection and catch-up from the SCM write outbox into the
  replaceable read model.
- Holding-scoped clear-and-replay rebuild with fail-closed routing, redacted
  errors, and no writes to Operations or Finance source tables.
- Pure request validation and error-redaction tests compiled as part of the
  worker crate.

Evidence: `audits/G9-SCM-004-056/README.md`.

Verification:

- `cargo fmt --all --check` — PASS.
- `cargo test -p erpx-worker --lib` — PASS: 22 passed, 1 documented
  disposable-PostgreSQL test ignored.
- `cargo clippy -p erpx-worker --all-targets -- -D warnings` — PASS.

Durable per-tenant scheduling/claiming, authenticated transport, source event
production, downstream Operations/Finance handoffs, and the G9 workflow gate
remain open.

## Entry 2026-07-24 — integrated G8 Finance subledger and journal projection wave

**Owner:** Main agent; reviewed AG-FIN bounded handoff

Integrated and independently verified:

- Exported the AP/AR, payment, allocation, cash, reconciliation, and Finance
  journal-handoff domain contracts.
- Added bounded Finance journal source-event paging from the tenant write
  outbox with stable aggregate/version cursors.
- Added holding-scoped read projection application, contiguous checkpoints,
  idempotent replay, and disposable read-model clearing.
- Added a tenant-routed Finance projector runner with redacted errors and
  bounded request validation.
- Added read-side checkpoint migration `0022_finance_projection.sql` and
  migration assertions.

Evidence: `audits/G8-FIN-003-056/README.md`,
`audits/G8-002-FINANCE-PROJECTION-057/README.md`.

Verification:

- `cargo fmt --all --check` — PASS.
- `cargo test -p erpx-domain --lib` — PASS: 114 passed.
- `cargo clippy -p erpx-domain --all-targets -- -D warnings` — PASS.
- `cargo test -p erpx-infrastructure --lib` — PASS: 75 passed, 4 documented
  disposable-PostgreSQL tests ignored.
- `cargo clippy -p erpx-infrastructure --all-targets -- -D warnings` — PASS.
- `cargo test -p erpx-migration` — PASS: 6 passed.
- `cargo test -p erpx-worker --lib` — PASS: 24 passed, 1 documented
  disposable-PostgreSQL test ignored.
- `cargo clippy -p erpx-worker --all-targets -- -D warnings` — PASS.

AP/AR persistence, cash/bank workflows, Finance transport, budgets, SoD and
approval limits, consolidation, Thai tax, statements, Operations event
production, reconciliation, and the G8 gate remain open.

## Entry 2026-07-24 — full Finance/SCM/Assets integration-wave verification

Independent verification after the Finance subledger/projection and SCM/Assets
worker integrations:

- `cargo fmt --all --check` — PASS.
- `cargo clippy --workspace --all-targets -- -D warnings` — PASS.
- `cargo test --workspace` — PASS: application 8, console 11, domain 114,
  identity 20 plus six documented disposable-PostgreSQL skips,
  infrastructure 75 plus four documented skips, migration 6, server 51, UI
  48, worker 24 plus one documented skip; all doc tests passed.
- `cargo check -p erpx-console --no-default-features --features web` — PASS.
- `cargo check -p erpx-console --no-default-features --features desktop` — PASS.
- `bun run lint` — PASS with four pre-existing n8n-script warnings and zero
  errors.
- `bunx tsc --noEmit` — PASS.
- Folio dev server — PID 14894, port 3004 listening; PostgreSQL port 5432
  listening.

This is integration health evidence only. Finance AP/AR persistence, business
transport, source event production, durable tenant scheduling, mobile CI,
migration/cutover, hardening audits, and Folio write retirement remain open.

## Entry 2026-07-24 — integrated G10 Marketing tenant projector adapter

**Owner:** Main agent; reviewed AG-MKT bounded handoff

Integrated and independently verified:

- Public Marketing source-event, projection-application, and rebuild DTOs in
  infrastructure.
- Atomic Marketing aggregate snapshot/outbox writes for profile, consent,
  suppression, and audience mutations.
- Holding-scoped, forced-RLS read checkpoint migration `0024`.
- Generic tenant-routed Marketing page/catch-up/rebuild runner with a SQLx
  adapter over already-routed pools, redacted errors, and bounded requests.

Evidence: `audits/G10-MKT-003-058/README.md`.

Verification:

- `cargo fmt --all --check` — PASS.
- `cargo check -p erpx-infrastructure` — PASS.
- `cargo test -p erpx-infrastructure --lib` — PASS: 79 passed, 4 documented
  disposable-PostgreSQL tests ignored.
- `cargo clippy -p erpx-infrastructure --all-targets -- -D warnings` — PASS.
- `cargo test -p erpx-migration` — PASS: 6 passed.
- `cargo check -p erpx-worker` — PASS.
- `cargo test -p erpx-worker --lib` — PASS: 28 passed, 1 documented
  disposable-PostgreSQL test ignored.
- `cargo clippy -p erpx-worker --all-targets -- -D warnings` — PASS.

Marketing channels, durable scheduling/claiming, journeys, CRM, attribution,
transport/UI, and the G10 gate remain open.

## Entry 2026-07-24 — integrated G8 Finance AP/AR persistence and read transport

**Owner:** Main agent; reviewed AG-FIN bounded handoff

Integrated and independently verified:

- Tenant-routed AP/AR, allocation, cash-control, reconciliation, and
  subledger SQLx persistence with forced RLS, optimistic versions,
  request-bound idempotency, and atomic outbox delivery.
- Finance subledger read projection/checkpoint/rebuild APIs and migration
  assertions for tenant read migration `0023`.
- Authenticated, explicitly scoped, bounded `ListPostedJournals` Finance
  gRPC read transport using the configured tenant router.

Evidence: `audits/G8-FIN-004-058/README.md`,
`audits/G8-FIN-005-058/README.md`.

Verification:

- `cargo fmt --all --check` — PASS.
- `cargo check -p erpx-infrastructure` — PASS.
- `cargo test -p erpx-infrastructure --lib` — PASS: 79 passed, 4 documented
  disposable-PostgreSQL tests ignored.
- `cargo clippy -p erpx-infrastructure --all-targets -- -D warnings` — PASS.
- `cargo test -p erpx-migration` — PASS: 6 passed.
- `cargo check -p erpx-worker` — PASS.
- `cargo test -p erpx-worker --lib` — PASS: 28 passed, 1 documented
  disposable-PostgreSQL test ignored.
- `cargo clippy -p erpx-worker --all-targets -- -D warnings` — PASS.
- `cargo test -p erpx-server --bin erpx-server` — PASS: 51 passed.
- `cargo clippy -p erpx-server --all-targets -- -D warnings` — PASS.

Finance allocation-batch transport and the remaining G8 workflows/gate remain open.

## Entry 2026-07-24 — integrated Finance subledger command transport

**Owner:** Main agent

Added `FinanceService.SaveSubledger` beside the bounded posted-journal read:
typed aggregate decoding, explicit scope/type binding, authenticated active
Finance role checks, commercial entitlement/billing checks, bounded payloads,
optimistic/idempotent SQLx persistence, tenant write/read route acquisition,
safe status mapping, and audit attribution. The allocation-batch command,
banking workflows, and remaining G8 requirements remain open.

Evidence: `audits/G8-FIN-005-058/README.md`.

Verification:

- `cargo fmt --all --check` — PASS.
- `cargo check -p erpx-server` — PASS.
- `cargo test -p erpx-server --bin erpx-server` — PASS: 51 passed.
- `cargo clippy -p erpx-server --all-targets -- -D warnings` — PASS.

## Entry 2026-07-24 — integrated Finance allocation-batch transport

**Owner:** Main agent

Added `FinanceService.SaveAllocationBatch` with bounded JSON input, typed
payment/invoice/allocation decoding, explicit command-scope matching,
authenticated Finance role and commercial entitlement checks, required
optimistic payment version, tenant-routed atomic repository execution, safe
receipt mapping, and audit attribution. Banking, budgets, and remaining G8
workflows remain open.

Evidence: `audits/G8-FIN-005-058/README.md`.

Verification:

- `cargo fmt --all --check` — PASS.
- `cargo check -p erpx-server` — PASS.
- `cargo test -p erpx-server --bin erpx-server` — PASS: 51 passed.
- `cargo clippy -p erpx-server --all-targets -- -D warnings` — PASS.

## Entry 2026-07-24 — full Marketing/Finance integration-wave verification

Independent verification after the Marketing projector adapter and Finance
AP/AR/read-transport integrations:

- `cargo fmt --all --check` — PASS.
- `cargo clippy --workspace --all-targets -- -D warnings` — PASS.
- `cargo test --workspace` — PASS; all non-disposable tests passed, with only
  the documented disposable-PostgreSQL integration tests ignored.
- `cargo check -p erpx-console --no-default-features --features web` — PASS.
- `cargo check -p erpx-console --no-default-features --features desktop` — PASS.
- `bun run lint` — PASS with four pre-existing n8n-script warnings and zero
  errors.
- `bunx tsc --noEmit` — PASS.
- Folio dev server — PID 14894, port 3004 listening; PostgreSQL port 5432
  listening.

This wave is integration health evidence only. Finance mutation transport and
remaining Finance workflows, Marketing channels/journeys/CRM/attribution,
durable tenant scheduling, mobile CI, migration/cutover, hardening audits,
and Folio write retirement remain open.

## Entry 2026-07-24 — integrated G10 Marketing delivery contract

**Owner:** Main agent; reviewed AG-MKT bounded handoff

Integrated and independently verified the pure Marketing delivery contract:

- Closed channel records for email, SMS, social, push, and webhook.
- Send-time consent/suppression/double-opt-in/proof decisions.
- Tenant-scoped deterministic delivery idempotency and duplicate-dispatch
  protection.
- Bounded retry/backoff, exact due/expiry boundaries, in-flight attempt
  ownership, suppression cancellation, terminal failure states, and explicit
  provider acceptance versus delivery confirmation.

Evidence: `audits/G10-MKT-004-059/README.md`.

Verification:

- `cargo fmt --all --check` — PASS.
- `cargo clippy -p erpx-domain --all-targets -- -D warnings` — PASS.
- `cargo test -p erpx-domain --lib` — PASS: 122 passed.

Dispatch persistence, provider adapters, durable worker delivery, journeys,
CRM, attribution, and the G10 gate remain open.

## Entry 2026-07-24 — integrated G6-PRV-005 durable tenant projection scheduler

**Owner:** Main agent; reviewed AG-PRV bounded handoff

Integrated the typed tenant projection scheduler into the infrastructure and
control-plane migration composition:

- Tenant/holding-bound incremental and rebuild jobs with bounded page, batch,
  attempt, lease, and backoff policies.
- Transactional `FOR UPDATE SKIP LOCKED` claim records, active-claim
  uniqueness, worker/claim/lease ownership, exact lease-boundary stale
  reclaim, retry/dead-letter outcomes, rebuild records, and idempotent scoped
  claim keys.
- Control migration `0025_tenant_projection_scheduler.sql` and migration
  assertions, without placing tenant aggregate or read data in the control
  scheduler tables.
- Public infrastructure exports are limited to typed scheduler records and
  repository contracts; tenant credentials are not part of the contract.

Evidence: `audits/G6-PRV-005-059/README.md`.

Verification:

- `cargo fmt --all --check` — PASS.
- `cargo clippy -p erpx-infrastructure --all-targets -- -D warnings` — PASS.
- `cargo test -p erpx-infrastructure --lib` — PASS: 89 passed, 4 documented
  disposable-PostgreSQL tests ignored.
- `cargo test -p erpx-migration` — PASS: 6 passed.

Worker claim-loop composition, real projection handlers, replacement-read
activation, platform authorization/audit at scheduler APIs, and the G6 gate
remain open.

## Entry 2026-07-24 — integrated G6-PRV-005 tenant scheduler worker seam

**Owner:** Main agent

Added `apps/worker/src/tenant_projection_scheduler.rs` and registered its
explicit composition API:

- A named handler accepts only an exact tenant/holding binding, durable
  scheduler source, and injected projector executor.
- Claims use bounded leases and per-pass idempotency keys; successful jobs are
  completed through the scheduler source, while retryable/permanent executor
  failures are persisted through the scheduler's retry/dead-letter state
  machine.
- The default worker registry remains fail-closed and never claims scheduler
  rows without explicit dependencies.

Verification:

- `cargo clippy -p erpx-worker --all-targets -- -D warnings` — PASS.
- `cargo test -p erpx-worker --lib` — PASS: 34 passed, 1 documented
  disposable-PostgreSQL test ignored.

Production multi-tenant source enumeration, real projection executor wiring,
authorization/audit at scheduler APIs, replacement-read activation, and the
G6 gate remain open.

## Entry 2026-07-24 — integrated G10-MKT-005 tenant Marketing delivery persistence

**Owner:** Main agent

Integrated tenant write persistence for the Marketing delivery contract:

- Migration `migrations_tenant_write/0026_marketing_delivery.sql` stores
  dispatch state, attempt/lease ownership, bounded failure metadata, payload,
  idempotency reservations, and forced-RLS holding scope.
- `SqlxMarketingDeliveryStore` atomically reserves deterministic idempotency,
  inserts the dispatch, writes the aggregate snapshot, and appends the queued
  outbox event.
- Due claims are tenant-scoped and bounded with `FOR UPDATE SKIP LOCKED`;
  consent and suppression are rechecked before attempt reservation, and
  provider result transitions require the current claim UUID, worker lease,
  and attempt authorization.
- Version-checked delivery callbacks and safe state/error mappings preserve
  the domain state machine. Provider credentials and provider calls remain
  outside this repository boundary.
- Added a named worker handler with explicit tenant-scoped source and provider
  traits. It persists accepted, retryable, and permanent provider outcomes;
  the default registry remains fail-closed until both dependencies are
  injected.

Evidence: `audits/G10-MKT-005-060/README.md`.

Verification:

- `cargo clippy -p erpx-infrastructure --all-targets -- -D warnings` — PASS.
- `cargo test -p erpx-infrastructure --lib` — PASS: 89 passed, 4 documented
  disposable-PostgreSQL tests ignored.
- `cargo clippy -p erpx-worker --all-targets -- -D warnings` — PASS.
- `cargo test -p erpx-worker --lib` — PASS: 31 passed, 1 documented
  disposable-PostgreSQL test ignored.
- `cargo test -p erpx-migration` — PASS: 6 passed.

The worker/provider adapter, authenticated delivery callbacks, channel
integrations, journeys, CRM, attribution, and the G10 gate remain open.

## Entry 2026-07-24 — full worker and delivery integration-wave verification

Independent verification after the scheduler, Marketing delivery persistence,
claim UUID hardening, and named worker-handler integration:

- `cargo fmt --all --check` — PASS.
- `cargo clippy --workspace --all-targets -- -D warnings` — PASS.
- `cargo test --workspace` — PASS; all active tests passed, with only the
  documented disposable-PostgreSQL integration tests ignored.
- `cargo check -p erpx-console --no-default-features --features web` — PASS.
- `cargo check -p erpx-console --no-default-features --features desktop` — PASS.
- `bun run lint` — PASS with four pre-existing n8n-script warnings and zero
  errors.
- `bunx tsc --noEmit` — PASS.
- Folio dev server — PID 14894, port 3004 listening; PostgreSQL port 5432
  listening.

This is integration health evidence, not a completion claim. Live provider
adapters/callback authentication, actual tenant scheduler repository
composition, replacement-read execution, all remaining ERP workflows and
clients, migration/cutover, hardening, and Folio write retirement remain open.

## Entry 2026-07-25 — integrated G8-FIN-004-061 banking domain slice

**Owner:** AG-FIN handoff; Main review and independent verification

Integrated the pure Finance banking import/reconciliation contract:

- Company/branch-bound bank accounts and statement imports with explicit
  command-scope containment.
- Immutable imported transaction identities and content digests, deterministic
  import idempotency, duplicate detection, and changed-content conflicts.
- Versioned reconciliation lifecycle with exact zero-difference acceptance,
  explicit exception handling, checked arithmetic, and terminal close rules.

Evidence: `audits/G8-FIN-004-061/README.md`.

Verification:

- `cargo fmt --all --check` — PASS.
- `cargo clippy -p erpx-domain --all-targets -- -D warnings` — PASS.
- `cargo test -p erpx-domain --lib` — PASS: 128 passed.

SQLx banking persistence/import adapters, journal handoff, approvals/SoD,
bank provider authentication, reconciliation workers, transport, UI, and the
G8 gate remain open.

## Entry 2026-07-25 — QA follow-up fixes for G6/G10 seams

**Owner:** Main agent; AG-QA independent findings reviewed

Applied the safe corrections identified by `audits/G14-QA-061/README.md`:

- Added tenant read migration `0026_marketing_delivery_projection.sql` with
  forced RLS and changed `get` to read only the disposable projection table;
  `project_read_model` is an explicit worker-facing projection operation.
- Delivery callbacks now require the stored provider message reference before
  the domain can transition to Delivered. Provider signature/authentication and
  callback ingress remain open.
- Scheduler renew, complete, fail, stale-reclaim, and claim-reclaim paths now
  use one transaction where multiple rows participate and require exactly one
  claim-history update.
- Scheduler payloads and error text are bounded at the application and SQL
  migration boundaries, with a regression test for oversized payloads.
- Sealed scheduler claim-key replays are marked non-executable and the worker
  skips them; a regression test covers this behavior.

Verification after the fixes:

- `cargo clippy -p erpx-infrastructure --all-targets -- -D warnings` — PASS.
- `cargo test -p erpx-infrastructure --lib` — PASS: 90 passed, 4 ignored.
- `cargo clippy -p erpx-worker --all-targets -- -D warnings` — PASS.
- `cargo test -p erpx-worker --lib` — PASS: 35 passed, 1 ignored.
- `cargo test -p erpx-migration` — PASS: 6 passed.

Authoritative scheduler authorization/route resolution, production tenant
enumeration/projector composition, callback signature/idempotency ingress,
lease renewal/time authority, database tamper constraints, live provider
adapters, and G6/G10 gates remain open.

## Entry 2026-07-25 — integrated G6 scheduler authorization boundary

**Owner:** Main agent

The scheduler repository now requires a bounded `ProjectionSchedulerAuthorization`
at construction. Before schedule, claim, renewal, completion, failure, or
stale-reclaim work, it verifies the active control-plane tenant registration and
exact holding binding, then evaluates the actor's authoritative role permission
(`tenant.projection.execute`) for the UUID holding scope on the control write
side. Each accepted operation records a bounded control-plane audit event with
actor, operation, purpose, tenant/holding binding, permission, timestamp, and
correlation ID. The worker exposes an explicit runtime composition helper that
constructs this repository from its control-plane pools and tenant registry;
projector execution remains an injected dependency and the default registry
still fails closed.

Evidence:

- `crates/infrastructure/src/tenant_projection_scheduler.rs`
- `crates/infrastructure/src/repositories.rs`
- `apps/worker/src/lib.rs`

Verification:

- `cargo fmt --all --check` — PASS.
- `cargo clippy -p erpx-infrastructure --all-targets -- -D warnings` — PASS.
- `cargo test -p erpx-infrastructure --lib` — PASS: 91 passed, 4 ignored.
- `cargo clippy -p erpx-worker --all-targets -- -D warnings` — PASS.
- `cargo test -p erpx-worker --lib` — PASS: 35 passed, 1 ignored.

This closes the previously missing scheduler authorization/audit seam but does
not claim the G6 gate: production tenant enumeration, real projector routing,
replacement-read activation, backup/restore adapters, and platform operations
UI remain open.

## Entry 2026-07-25 — integrated G6 active-tenant routed scheduler composition

**Owner:** Main agent

Added the next production composition seam:

- `SqlxTenantStoreRegistry::list_active` enumerates only active control-plane
  registrations on every worker pass.
- `MultiTenantProjectionSchedulerHandler` claims separately for each active
  tenant/holding binding and preserves the existing claim ownership,
  retry/dead-letter, and sealed-replay behavior.
- `RoutedTenantProjectionExecutor` resolves both server-side physical write and
  read routes through `TenantDatabaseRouter` before calling a routed executor;
  route failures are retryable and database handles are closed after execution.
- `WorkerRuntime::handler_registry_with_control_plane_multi_tenant_projection_scheduler`
  composes the registry, authorized scheduler repository, active-binding
  enumeration, router, and explicit routed projector dependency.

Verification:

- `cargo fmt --all --check` — PASS.
- `cargo clippy -p erpx-worker --all-targets -- -D warnings` — PASS.
- `cargo test -p erpx-worker --lib` — PASS: 36 passed, 1 ignored.

The worker binary still does not invent a projector implementation or register
this handler by default. Durable target projector implementations, worker
binary wiring, replacement-read activation, and SQLx race/integration tests
remain open. See `audits/G14-QA-062-FOLLOWUP/README.md`.

## Entry 2026-07-24 — integrated G10-MKT-006 Marketing callback boundary

**Owner:** Main agent; independent domain, infrastructure, migration, server,
worker, and disposable-PostgreSQL verification

Integrated the next dependency-ready Marketing callback slice:

- Added the raw-body `POST /webhooks/marketing` boundary. It verifies the
  exact request bytes with HMAC-SHA256, requires a configured provider account
  and verifier key reference, retains only a body digest, and carries the
  request correlation ID into tenant-write persistence.
- Added tenant-write migration 0027 for a forced-RLS callback inbox with
  unique provider-event identity, deterministic fingerprint, timestamp/order
  classification, authentication metadata, and correlation ID.
- Added `accept_provider_callback`, which locks and binds the exact delivery,
  safely handles duplicates/conflicts/stale events, applies only accepted
  provider outcomes, and writes the delivery snapshot plus callback outbox
  event atomically.
- Added a real disposable two-database integration test covering migrations,
  callback transition, duplicate no-op behavior, inbox cardinality, RLS
  holding scope, version advancement, and callback outbox emission.
- Added control-plane migration 0026 and an authoritative provider-account
  resolver. Callback routing now requires an active provider account, active
  tenant registration, exact tenant/holding match, and matching verifier-key
  reference; suspended accounts are rejected before tenant routing.

Evidence: `audits/G10-MKT-006-INTEGRATION/README.md`.

Verification:

- `cargo fmt --all --check` — PASS.
- `cargo clippy -p erpx-domain --all-targets -- -D warnings` — PASS.
- `cargo test -p erpx-domain --lib` — PASS: 135 passed.
- `cargo clippy -p erpx-infrastructure --all-targets -- -D warnings` — PASS.
- `cargo test -p erpx-infrastructure --lib` — PASS: 91 passed, 4 ignored.
- `cargo test -p erpx-infrastructure --test marketing_delivery_persistence -- --ignored --nocapture` — PASS: 1 passed against two temporary local PostgreSQL databases, cleaned up after execution.
- `cargo test -p erpx-migration` — PASS: 6 passed.
- `cargo clippy -p erpx-server --all-targets -- -D warnings` — PASS.
- `cargo test -p erpx-server --bin erpx-server` — PASS: 54 passed.
- `cargo clippy -p erpx-worker --all-targets -- -D warnings` — PASS.
- `cargo test -p erpx-worker --lib` — PASS: 36 passed, 1 ignored.

This is not the G10 gate. Provider-specific parsers/adapters and credential
rotation, live channel calls, worker reconciliation and read projection
execution, journeys, CRM, attribution, UI, and all other open ERP requirements
remain outstanding.

## Entry 2026-07-24 — full verification wave after G10-MKT-006 integration

Independent verification after the callback contract, raw-body ingress,
tenant inbox, and transactional callback repository integration:

- `cargo fmt --all --check` — PASS.
- `cargo clippy --workspace --all-targets -- -D warnings` — PASS.
- `cargo test --workspace` — PASS; active tests passed, with only documented
  disposable-environment tests ignored. The new callback integration test is
  included and remains explicitly ignored in the workspace default run.
- `cargo check -p erpx-console --no-default-features --features web` — PASS.
- `cargo check -p erpx-console --no-default-features --features desktop` — PASS.
- `bun run lint` — PASS with four existing warnings in
  `infra/scripts/start-n8n.js` and zero errors.
- `bunx tsc --noEmit` — PASS.
- Folio dev server — PID 14894, port 3004 listening.
- PostgreSQL — port 5432 listening.

This wave is integration evidence only. ERPX remains non-production and the
persistent goal remains active; no Folio command-owner change or completion
claim was made.

## Entry 2026-07-24 — integrated G12-UI-007 Marketing and CRM route scaffold

**Owner:** AG-UI handoff; Main review and independent verification

Integrated the disjoint shared-Dioxus UI packet:

- Added 23 typed, exact, segment-aware Marketing and CRM route definitions.
- Added static bilingual English/Thai labels and navigation metadata.
- Reused the existing safe deep-link normalization/redaction boundary; no
  credential-bearing route is retained in client state or displayed.
- Kept the packet target-neutral and data-free: no backend route, permission,
  migration, or live Marketing/CRM behavior was fabricated.

Evidence: `audits/G12-UI-007/README.md`.

Verification:

- `cargo fmt --all --check` — PASS.
- `cargo clippy -p erpx-ui --all-targets -- -D warnings` — PASS.
- `cargo test -p erpx-ui --lib` — PASS: 50 passed.
- `cargo check -p erpx-console --no-default-features --features web` — PASS.
- `cargo check -p erpx-console --no-default-features --features desktop` — PASS.
- Full Rust/Folio integration wave — PASS; Folio lint had four existing
  warnings and zero errors, TypeScript passed, and ports 3004/5432 remained
  listening.

This packet does not close G12. Authenticated Marketing/CRM adapters and
backend routes, native secure storage, Android/iOS evidence, notifications,
complete product journeys, and all other ERP requirements remain open.

## Entry 2026-07-25 — integrated G7-OPS-006 Operations projector and G6-PRV-006 worker composition

**Owner:** AG-OPS handoff; Main review and cross-context integration

### Delivered behavior

- Added a tenant-routed `OperationsProjectionRunner` for bounded incremental
  pages and full rebuilds.
- Added closed allow-list decoding for all 17 Operations aggregate types,
  envelope identity/holding/scope/contract/version validation, contiguous
  checkpoint application, idempotent replay, and redacted fail-closed errors.
- Restricted the scheduler executor to the Operations read projection and
  explicitly closed routed pools on every path.
- Wired `apps/worker/src/main.rs` to register the multi-tenant scheduler only
  for `ERPX_WORKER_HANDLER=tenant_projection_scheduler`; it now requires
  validated `ERPX_TENANT_ADMIN_DATABASE_URL` and
  `ERPX_TENANT_DATABASE_URL_TEMPLATE`, bounded page size, and typed scheduler
  authorization.
- Centralized server-only tenant routing configuration validation in
  `TenantDatabaseProvisioningConfig::from_env/from_values`.

Evidence: `audits/G7-OPS-006/README.md` and `audits/G6-PRV-006/README.md`.

### Independent verification

- `cargo fmt --all --check` — PASS.
- `cargo clippy --workspace --all-targets -- -D warnings` — PASS.
- `cargo test -p erpx-worker --lib` — PASS: 41 passed, 1 ignored.
- Disposable four-database
  `operations_projector_executes_through_active_tenant_route` — PASS: 1
  passed; control and tenant databases were explicitly removed afterward.
- `cargo test --workspace` — PASS; only documented disposable-database tests
  remain ignored by default.
- Dioxus web and desktop checks — PASS.
- Folio `bun run lint` — PASS with four existing warnings and zero errors;
  `bunx tsc --noEmit` — PASS.

### Security and remaining gate

The scheduler re-enumerates active control-plane tenant bindings and never
returns tenant credentials. Unknown aggregate payloads, mismatched holding or
scope identity, projection-name crossover, and invalid routing configuration
fail closed. G7 source producers, exception detection/reassignment,
authenticated Operations routes, Folio reconciliation, replacement-read
activation evidence, and the broader G6/G7 gates remain open.

### Rollback

Unset the scheduler handler selection or stop the worker before reverting the
worker composition. No Folio command ownership or tenant write data changed.

## Entry 2026-07-25 — integrated G4-SUP-008/G4-SUP-009 bounded ticket queue read and cursor

**Owner:** Main agent

### Delivered behavior

- Added bounded `ListTickets` SupportService transport with optional status and
  customer filters.
- Added deterministic ticket queue persistence ordering and `has_more`
  metadata.
- Added an exclusive typed keyset cursor in persistence and an opaque bounded
  transport encoding with `next_cursor` response metadata.
- Enforced active query context, `support.ticket.read`, per-ticket customer
  scope containment, customer-visible audit attribution, and transport-safe
  ticket DTOs with internal notes excluded.

Evidence: `audits/G4-SUP-008/README.md`.

### Independent verification

- `cargo check -p erpx-server` — PASS.
- `cargo test -p erpx-infrastructure --test support_persistence` — PASS: 8
  active tests, 8 ignored environment tests.
- Disposable PostgreSQL `support_ticket_list_is_bounded_ordered_and_status_filterable`
  — PASS: 1 passed; `erpx_support_queue_20260725_01` was removed afterward.
- Disposable PostgreSQL
  `support_ticket_list_cursor_is_exclusive_deterministic_filtered_and_customer_scoped`
  — PASS: 1 passed; `erpx_support_cursor_20260725_01` was removed afterward.
- Server cursor round-trip/security test — PASS.
- Full workspace/Folio wave — PASS.

### Remaining gate

This query does not close G4. Complete support command/outbox atomicity,
cursor/filter binding at the UI data adapter, customer/platform data adapters,
and the full support/incident workflow still require implementation and
independent audit.

## Entry 2026-07-25 — integrated G6/G12-UI-008 Platform Operations route registry

**Owner:** AG-UI handoff; Main review and independent verification

Added a target-neutral seven-route Platform Operations registry with opaque
tenant IDs, exact segment-aware resolution, deep-link normalization/redaction,
platform/auth metadata, and Thai/English labels. Evidence:
`audits/G6-G12-UI-008/README.md`.

Verification: 55 `erpx-ui` tests passed, UI clippy passed, console web/desktop
checks passed, and Folio lint/TypeScript passed. Backend platform operations
RPCs, data adapters, authorization, provisioning actions, and complete UI
journeys remain open.

## Entry 2026-07-25 — mandatory integration-wave verification after G7/G6/G4/UI packets

**Owner:** Main agent

The integrated Operations projector, production worker scheduler composition,
bounded/cursor-based Support ticket queue, provider callback binding fix, and
Platform Operations route registry were rechecked together before selecting the
next packets.

### Independent verification

- `cargo fmt --all --check` — PASS.
- `cargo clippy --workspace --all-targets -- -D warnings` — PASS.
- `cargo test --workspace` — PASS: all active workspace tests passed; only the
  documented disposable-PostgreSQL integration tests remain ignored by
  default. The active counts included application 8, console 11, domain 135,
  identity 20, infrastructure 92, migration 6, server 55, UI 55, and worker
  41 library tests, plus the active integration assertions and doc tests.
- `cargo check -p erpx-console --no-default-features --features web` — PASS.
- `cargo check -p erpx-console --no-default-features --features desktop` — PASS.
- Folio `bun run lint` — PASS with four pre-existing warnings in
  `infra/scripts/start-n8n.js` and zero errors.
- Folio `bunx tsc --noEmit` — PASS.
- Folio remains served by PID 14894 on port 3004; PostgreSQL PID 791 remains
  listening on port 5432.
- No disposable `erpx_*_20260725%` databases remain after the earlier real
  Operations and Support queue/cursor tests.

### Security and remaining gate

This is integration health evidence, not a G1-G14 completion claim. The
Operations projector fails closed on tenant/holding/scope/projection identity
and uses idempotent checkpoints; Support cursor transport is opaque and
bounded, scoped, and customer-audited; Platform Operations route metadata
contains no tenant credentials. Durable source production, full support and
Operations command/read adapters, replacement-read activation, live backup and
storage operations, banking and remaining Finance workflows, all other ERP
contexts, native target evidence, migration/cutover, hardening, and Folio write
retirement remain open.

### Rollback

No Folio command ownership, production tenant data, environment files, or
runtime service state changed in this verification wave. Revert only the
associated additive ERPX packets after preserving their audit evidence.

## Entry 2026-07-25 — integrated G8-FIN-011 banking write persistence

**Owner:** Main agent; partial AG-FIN implementation independently reviewed and completed with audit evidence

Added tenant-scoped Finance banking persistence for opaque accounts, immutable
statement imports and transactions, reconciliation aggregates, deterministic
request hashes, typed idempotency receipts, atomic snapshots/outbox events,
and bounded source-event pages. Migration `0028_finance_banking.sql` enables
forced holding RLS for every banking table. Provider credentials and payment
instruments remain outside the boundary.

Evidence: `audits/G8-FIN-011/README.md`.

Independent verification:

- `cargo fmt --all --check` — PASS.
- `cargo clippy -p erpx-infrastructure --all-targets -- -D warnings` — PASS.
- `cargo test -p erpx-infrastructure --lib` — PASS: 93 passed, 4 documented
  disposable-PostgreSQL tests ignored.
- `cargo test -p erpx-infrastructure --test finance_banking_persistence` —
  PASS: 2 active tests passed, 1 live test ignored by default.
- `cargo test -p erpx-migration` — PASS: 6 passed.
- Real disposable PostgreSQL banking test — PASS: 1 passed; the explicit
  write/read databases `erpx_bank_20260725_w` and `erpx_bank_20260725_r` were
  dropped afterward and cleanup returned no rows.

The G8 gate remains open: banking read projection/provider adapters, Finance
transport, GL handoff, budgets, SoD/approval limits, consolidation, Thai tax,
statements, and migration reconciliation remain incomplete.

## Entry 2026-07-25 — integrated G6-PRV-007/G6-G12-UI-009 and Platform Operations read API

**Owner:** Main agent; AG-PRV and AG-UI handoffs independently reviewed

Integrated real disposable PostgreSQL evidence for physical tenant
provisioning, forced-RLS write/read separation, activation gates, failed
restore no-switch behavior, and verified replacement-read activation. The
Platform Operations console now renders all seven typed routes with safe
read-only/non-ready states, and the server exposes a bounded,
platform-role-authorized `ListTenants` RPC with safe readiness summaries and
platform audit attribution.

Evidence: `audits/G6-PRV-007/README.md`,
`audits/G6-G12-UI-009/README.md`, and
`audits/G6-PRV-008/README.md`.

Independent verification:

- Real `G6-PRV-007` disposable PostgreSQL test — PASS: 1 passed; all six
  generated databases were dropped and the cleanup query returned no rows.
- `cargo test -p erpx-server --bin erpx-server` — PASS: 56 tests.
- `cargo clippy -p erpx-server --all-targets -- -D warnings` — PASS.
- `cargo test -p erpx-console --bin erpx-console` — PASS: 14 tests.
- Console web and desktop checks — PASS.

The G6 gate remains open: external backup/PITR adapters, freshness producers,
complete tenant provisioning/backup/integration/audit/settings actions, live
Platform Operations data consumption, and complete cross-target journeys
remain incomplete.

## Entry 2026-07-25 — full post-banking/platform integration-wave verification

- `cargo fmt --all --check` — PASS.
- `cargo clippy --workspace --all-targets -- -D warnings` — PASS.
- `cargo test --workspace` — PASS: all active workspace tests passed; the
  updated active library counts include application 8, console 14, domain 135,
  identity 20, infrastructure 97, migration 6, server 56, UI 55, and worker
  41, with only documented disposable-PostgreSQL tests ignored by default and
  all doc tests passing.
- `cargo check -p erpx-console --no-default-features --features web` — PASS.
- `cargo check -p erpx-console --no-default-features --features desktop` — PASS.
- Folio `bunx tsc --noEmit` — PASS.
- Folio `bun run lint` — PASS on the post-build retry with four pre-existing
  warnings in `infra/scripts/start-n8n.js` and zero errors. A concurrent first
  invocation encountered a transient Cargo target-directory scan race and was
  rerun after Rust compilation completed.
- Folio remains served on port 3004; PostgreSQL remains on port 5432.

This is integration health evidence only. Banking read/provider/transport
work, universal command adoption, support/Operations completion, remaining ERP
modules, native Android/iOS evidence, migration cutover, hardening, and Folio
write retirement remain open.

## Entry 2026-07-25 — integrated G6-PRV-009 tenant registry pagination evidence

Added a focused disposable control-plane test for the Platform Operations
tenant registry page. It verifies bounded page sizes, deterministic tenant-ID
ordering, exclusive keyset pagination, and separate control write/read
migrations.

Evidence: `audits/G6-PRV-009/README.md`.

Verification: focused compilation passed; the real disposable PostgreSQL run
passed 1 test using `erpx_tenant_page_cw_20260725` and
`erpx_tenant_page_cr_20260725`, both of which were dropped afterward. A
post-test database query returned no rows.

This does not close G6: Platform Operations UI data consumption, tenant detail
reads, provisioning actions, projection rebuild controls, backup/restore
actions, integrations, audit/settings pages, and full platform authorization
journeys remain open.

## Entry 2026-07-25 — verification after G6-PRV-009 registry evidence

- `cargo fmt --all --check` — PASS.
- `cargo clippy --workspace --all-targets -- -D warnings` — PASS.
- `cargo test --workspace` — PASS: all active tests passed; application 8,
  console 14, domain 135, identity 20, infrastructure 97, migration 6,
  server 56, UI 55, and worker 41 library tests, with documented disposable
  PostgreSQL tests ignored and all doc tests passing.
- Dioxus console web and desktop checks — PASS.
- Folio `bun run lint` — PASS with four pre-existing n8n warnings and zero
  errors; `bunx tsc --noEmit` — PASS.

No runtime services or Folio ownership changed; port 3004 and PostgreSQL 5432
remain active. This remains an integration-health wave, not a completion
claim.

## Entry 2026-07-25 — integrated G8-FIN-012 banking read projection and worker seam

**Owner:** Main agent; incomplete AG-FIN handoff reviewed, repaired, and independently verified

Added tenant-read migration `0029_finance_banking_projection.sql`, typed
banking read rows, forced-RLS projection checkpoints, idempotent contiguous
event application, read-model clearing, and a tenant-routed worker runner for
bounded projection pages and rebuilds. The read path never falls back to source
tables; the write database remains authoritative.

Evidence: `audits/G8-FIN-012/README.md`.

Independent verification:

- `cargo fmt --all` — PASS.
- `cargo check -p erpx-worker` — PASS.
- `cargo clippy -p erpx-worker --all-targets -- -D warnings` — PASS.
- `cargo test -p erpx-infrastructure --test finance_banking_persistence` —
  PASS: 2 active tests, 2 documented disposable-PostgreSQL tests ignored.
- `cargo test -p erpx-worker` — PASS: 43 active tests, 1 documented database
  test ignored.
- Real disposable PostgreSQL projection/rebuild test — PASS: 1 passed; the
  explicit write/read databases `erpx_bank_read_20260725_w` and
  `erpx_bank_read_20260725_r` were dropped afterward and a catalog query found
  no matches.

The G8 gate remains open: provider adapters, banking gRPC transport and
scheduler registration, bank-to-GL matching, budgets, approval/SoD enforcement,
consolidation, Thai tax, statements, migration reconciliation, and the rest of
Finance remain incomplete.

### Rollback

No Folio command ownership, environment files, or runtime service state
changed. Roll back only the additive G8-FIN-012 module/exports, audit, test, and
tenant-read migration through a reviewed migration plan that preserves any
projection retention obligations.

## Entry 2026-07-25 — integrated G8-FIN-014 banking transport and scheduler

**Owner:** Main agent

Added four Finance Tonic RPCs for banking account registration, statement
import, reconciliation save, and projected read-row access. Commands use the
existing authoritative command authorization intersection and tenant router;
reads are role/scope checked and customer-audited. Payloads are bounded typed
JSON, provider secrets are not accepted, and route closure is centralized.

Added a selectable `finance_banking` multi-tenant scheduler executor under the
existing durable projection scheduler. It validates claims, consumes bounded
source pages, applies read checkpoints idempotently, and supports rebuilds. The
default Operations scheduler kind is unchanged.

Evidence: `audits/G8-FIN-014/README.md`.

Independent verification:

- `cargo fmt --all` — PASS.
- `cargo check -p erpx-server` and `cargo check -p erpx-worker` — PASS.
- `cargo clippy -p erpx-server --all-targets -- -D warnings` — PASS.
- `cargo clippy -p erpx-worker --all-targets -- -D warnings` — PASS.
- `cargo test -p erpx-server --bin erpx-server` — PASS: 56 tests.
- `cargo test -p erpx-worker` — PASS: 43 active tests, 1 documented database
  test ignored.

The G8 gate remains open: provider network adapters, full banking read/list
transport, bank-to-GL matching/posting, budgets, approval/SoD enforcement,
consolidation, Thai tax, statements, migration reconciliation, and the rest of
Finance remain incomplete.

### Rollback

No Folio command ownership, environment files, runtime services, or database
migrations changed. Revert only the additive proto/server/worker packet after
coordinating generated-client compatibility.

## Entry 2026-07-25 — integrated G8-FIN-013 provider-neutral banking adapter boundary

**Owner:** AG-FIN handoff; Main review and independent verification

Integrated an injected provider-neutral adapter contract for typed bank account
references, bounded statement pages, opaque cursors, deterministic request and
content bindings, stable transaction identity, and domain-ready mapping. The
boundary cannot represent credentials, tokens, cards, payment instruments, or
raw provider payloads. It does not persist or make network calls.

Evidence: `audits/G8-FIN-013/README.md`.

Independent verification:

- `cargo check -p erpx-infrastructure` — PASS.
- `cargo clippy -p erpx-infrastructure --all-targets -- -D warnings` — PASS.
- `cargo test -p erpx-infrastructure --test finance_banking_provider` — PASS:
  6 tests.
- Direct provider-module/test rustfmt checks — PASS.
- The subsequent full-tree `cargo fmt --all --check` wave is the authoritative
  formatting result for the adjacent worker integration.

The G8 gate remains open: concrete provider SDK/credential vault integration,
multi-page production workflows, bank-to-GL matching/posting, full Finance
transport/read surfaces, budgets, approvals/SoD, consolidation, Thai tax,
statements, migration reconciliation, and the rest of Finance remain
incomplete.

### Rollback

No database, Folio, environment, or runtime state changed. Remove only the
additive provider module/export, focused test, and audit after checking any
future provider implementation dependencies.

## Entry 2026-07-25 — mandatory integration-wave verification after G8-FIN-013/G8-FIN-014

- `cargo fmt --all --check` — PASS after formatting the main transport tests
  and banking scheduler assertion.
- `cargo clippy --workspace --all-targets -- -D warnings` — PASS.
- `cargo test --workspace` — PASS: application 8, console 14, domain 135,
  identity 20, infrastructure 97, migration 6, server 58, UI 55, and worker
  45 active library tests; documented disposable PostgreSQL tests remain
  ignored by default and all doc tests pass.
- Dioxus console web and desktop checks — PASS.
- Folio `bun run lint` — PASS with four pre-existing warnings in
  `infra/scripts/start-n8n.js` and zero errors; `bunx tsc --noEmit` — PASS.
- Independent `cargo test -p erpx-infrastructure --test
  finance_banking_provider` — PASS: 6 tests; provider clippy passed.
- Folio remains PID 14894 on port 3004; PostgreSQL PID 791 remains on 5432.
- No disposable `erpx_*20260725%` databases remain.

This is integration-health evidence only. G8 provider SDK/credential vault,
bank-to-GL, full Finance, remaining ERP contexts, native Android/iOS evidence,
migration/cutover, hardening, and Folio write retirement remain open.

## Entry 2026-07-25 — G8-FIN-022 Finance bank-provider control-plane routing

**Owner:** AG-FIN handoff; main-agent review pending

Added control-plane migration 0027 and the `SqlxControlPlane` exact resolver
for Finance bank-provider account bindings. The table stores only opaque
provider/account/tenant/holding metadata, an opaque credential-manager key
reference, active/suspended state, and a positive version. It has a tenant
registry foreign key, globally unique provider account reference, and an exact
route index. The resolver validates all opaque references, joins the active
tenant, and requires exact provider key, account reference, tenant, holding,
and credential-key matching. The DTO redacts the credential key reference in
`Debug` output.

Evidence: `audits/G8-FIN-022/README.md` and
`crates/infrastructure/tests/finance_banking_provider_accounts.rs`.

The ignored disposable PostgreSQL test covers clean migrations, exact active
resolution, wrong-provider/tenant/holding/credential rejection, suspended and
inactive-tenant rejection, malformed references, positive version, and
diagnostic redaction. Provider SDK calls, credential-manager retrieval,
tenant banking writes, bank matching, journal posting, worker scheduling,
transport, UI, and the remaining G8 Finance work remain open.

Focused verification: `cargo fmt --all --check`, `cargo check
-p erpx-infrastructure`, `cargo clippy -p erpx-infrastructure --all-targets
-- -D warnings`, the two repository unit tests, and the migration assertion
all passed. The ignored PostgreSQL test passed with one test against two
temporary databases; cleanup removed both databases.

### Rollback

Disable or ignore the lookup and retain migration 0027. No tenant data,
Folio state, environment file, or runtime service was changed.

## Entry 2026-07-25 — integrated G6-PRV-010 safe tenant detail and GetTenant

Added `TenantStoreRegistrationDetail` and a bounded registry `get_detail`
adapter. It validates opaque tenant IDs before querying and returns only
tenant/holding references, lifecycle/version metadata, and registration
presence flags; opaque database, storage, credential, and backup values are not
represented. The authenticated Platform Operations `GetTenant` RPC now uses
this safe adapter and combines it with existing readiness observations.

Evidence: `audits/G6-PRV-010/README.md`.

Independent verification:

- `cargo fmt --all` — PASS.
- `cargo clippy -p erpx-infrastructure --all-targets -- -D warnings` — PASS.
- `cargo test -p erpx-infrastructure --test tenant_registry_detail` — PASS:
  2 tests.
- `cargo test -p erpx-server --bin erpx-server` — PASS: 60 tests, including
  Platform Operations and Finance banking transport safety tests.

G6 remains open: list/detail data adapters beyond this read, provisioning
actions, backups/PITR, projections, integrations, audit/settings pages, and
complete cross-target UI journeys remain incomplete.

### Rollback

No database schema, Folio ownership, environment, or runtime service changed.
Remove only the additive safe detail DTO/method/export, GetTenant transport
integration, focused test, and audit after coordinating generated clients.

## Entry 2026-07-25 — integrated G8-FIN-015 bounded banking read-list transport

Added a bounded, tenant-scoped Finance banking read-list boundary over the
existing forced-RLS read database. The SQLx store now validates aggregate type,
company filters, opaque keyset cursors, and page size; applies Holding,
Company, or Branch scope predicates; decodes persisted rows defensively; and
returns deterministic pages without a write-side fallback. The authenticated
server RPC routes only to the read side, rejects Platform/Subtree scopes,
binds tenant/holding/context/role checks, audits customer-visible reads, and
returns only typed safe read rows plus an opaque continuation cursor.

The same integration completed the pre-existing Platform Operations
`GetTenant` contract by using the safe registry detail adapter and exposing no
database, storage, credential, key, or backup values.

Independent verification:

- Infrastructure library tests — PASS: 94 passed, 4 ignored.
- Banking persistence tests — PASS: 2 active, 2 ignored.
- Provider boundary tests — PASS: 6 passed.
- Server binary tests — PASS: 60 passed.
- Disposable live banking projection/list test — PASS: 1 passed; both
  temporary databases were explicitly dropped and catalog-checked absent.
- Direct rustfmt checks for all changed Rust files — PASS.

No migration, database schema, worker, Folio, credential, payment-instrument,
or provider-network changes were made by this packet. The previously observed
adjacent scheduler compilation issue was repaired in G6-PRV-011 and
independently verified; authenticated Platform Operations transport for that
adapter remains the next dependency. Provider SDK integration,
credential-vault access, bank-to-GL matching, and Finance command ownership
remain unfinished.

### Rollback

Remove only the additive scoped read-list store/API, protobuf/server transport
implementation, safe GetTenant implementation if separately reverted, focused
tests, exports, and this audit entry. No database rollback is required.

## Entry 2026-07-25 — integrated G6-PRV-011 bounded projection status adapter

The durable tenant scheduler now exposes a bounded, payload-free status read
adapter for Platform Operations. It validates tenant/holding/projection
filters, rejects holding-only scope, applies deterministic keyset pagination,
redacts persisted error text, and returns only state/checkpoint metadata. Job
payloads, claim IDs, worker IDs, and credentials are excluded from the DTO.

Evidence: `audits/G6-PRV-011/README.md`.

Independent verification:

- `cargo fmt --all` — PASS.
- `cargo test -p erpx-infrastructure --test tenant_projection_status` — PASS:
  4 tests.
- `cargo clippy -p erpx-infrastructure --all-targets -- -D warnings` — PASS.

The adapter currently reads authoritative scheduler coordination rows from the
control write pool; authenticated/audited Platform Operations transport and a
rebuildable control-read projection remain the next G6 dependencies. All G6
and broader ERP gates remain open.

### Rollback

Remove only the additive status DTO, repository method, export, focused test,
and audit. No database rollback is required.

## Entry 2026-07-25 — mandatory verification after G6-PRV-011/G6-PRV-012

- `cargo fmt --all --check` — PASS.
- `cargo clippy --workspace --all-targets -- -D warnings` — PASS.
- `cargo test --workspace` — PASS: application 8, console 14, domain 135,
  identity 20, infrastructure 94 active plus 4 documented ignored database
  tests, migration 6, server 61, UI 55, and worker 44 active plus 1
  documented ignored database test; all doc tests passed.
- `cargo check -p erpx-console --no-default-features --features web` — PASS.
- `cargo check -p erpx-console --no-default-features --features desktop` — PASS.
- Folio `bun run lint` — PASS with four pre-existing warnings in
  `infra/scripts/start-n8n.js` and zero errors.
- Folio `bunx tsc --noEmit` — PASS.
- Folio remained operational on port 3004; PostgreSQL remained on port 5432.
- No disposable `erpx_%20260725` databases remained.

This is integration-health evidence only. Control-read projection storage,
projection actions/rebuild switching, full Platform Operations journeys, all
remaining ERP contexts, native Android/iOS evidence, migration/cutover,
hardening, and Folio write retirement remain open.

## Entry 2026-07-25 — integrated G6-PRV-012 authenticated projection status transport

The existing Platform Operations service now exposes the G6-PRV-011 bounded
status adapter through authenticated `ListProjectionStatuses`. The transport
requires an active session, platform scope, authoritative tenant-read
permission, attributed query context, bounded filters/page size, and a
revalidated opaque cursor. It records actor, purpose, action, and correlation
in the platform audit sink before returning typed status metadata.

Evidence: `audits/G6-PRV-012/README.md`.

Independent verification:

- `cargo fmt --all --check` — PASS.
- `cargo check -p erpx-server` — PASS.
- `cargo clippy -p erpx-server --all-targets -- -D warnings` — PASS.
- `cargo clippy --workspace --all-targets -- -D warnings` — PASS.
- `cargo test -p erpx-server --bin erpx-server` — PASS: 61 tests.
- `cargo test --workspace` — PASS; all active tests passed and documented
  disposable-database suites remain ignored by default.
- Dioxus web/desktop checks — PASS.
- Folio lint — PASS with four existing warnings; TypeScript — PASS.
- Folio remains on port 3004 and PostgreSQL on 5432.

Projection rebuild/replace actions, control-read projection storage, live
freshness producers, native UI data adapters, backup/PITR execution, and
complete Platform Operations workflows remain open.

### Rollback

Remove only the additive protobuf messages/RPC, server handler/helpers/tests,
and audit. Retain the G6-PRV-011 repository adapter; no schema rollback is
required.

## Entry 2026-07-25 — integrated G6-PRV-013 control-read status projection

Added control-read migration `0004_tenant_projection_status.sql` with a
rebuildable, metadata-only status table and deterministic keyset indexes. The
new `TenantProjectionStatusReadRepository` reads only the control-read pool,
validates filter/cursor binding, and returns the bounded status DTO. The
authenticated Platform Operations status transport now uses this physical
read-side adapter; the control-write scheduler remains authoritative.

Evidence: `audits/G6-PRV-013/README.md`.

Independent focused verification:

- `cargo fmt --all` — PASS.
- `cargo test -p erpx-migration` — PASS: 6 tests, including migration 0004
  embedding and sensitive-column assertions.
- `cargo test -p erpx-infrastructure --test tenant_projection_status` — PASS:
  5 tests.
- `cargo check -p erpx-server` — PASS.
- `cargo test -p erpx-server --bin erpx-server` — PASS: 61 tests.

Scheduler-to-read publishing, freshness producers, projection rebuild/action
controls, and complete Platform Operations workflows remain open.

### Rollback

Remove only the additive read migration, repository/export, server pool
selection, focused tests, and audit. The read table is rebuildable and no
write-side rollback is required.

## Entry 2026-07-25 — mandatory verification after G6-PRV-013

- `cargo fmt --all --check` — PASS.
- `cargo clippy --workspace --all-targets -- -D warnings` — PASS.
- `cargo test --workspace` — PASS: all active workspace tests passed,
  including the 5-test status adapter suite and 2-test read-adapter suite;
  documented disposable-database suites remain ignored by default and all doc
  tests passed.
- `cargo check -p erpx-console --no-default-features --features web` — PASS.
- `cargo check -p erpx-console --no-default-features --features desktop` — PASS.
- Folio `bun run lint` — PASS with four pre-existing warnings in
  `infra/scripts/start-n8n.js` and zero errors.
- Folio `bunx tsc --noEmit` — PASS.
- Folio remained operational on port 3004; PostgreSQL remained on port 5432.
- No disposable `erpx_%20260725` databases remained.

This is integration-health evidence only. Scheduler-to-read publishing,
freshness producers, projection rebuild/action controls, all remaining ERP
contexts, native Android/iOS evidence, migration/cutover, hardening, and
Folio write retirement remain open.

## Entry 2026-07-25 — integrated G6-PRV-015 worker scheduler-status publisher

Added an explicit worker handler and composition method for steady-state
scheduler-status publication. It reads the authoritative control-write
scheduler through the existing typed projection writer and performs
version-ordered, metadata-only upserts into the separate control-read pool.
The default worker registry remains fail-closed; the binary selects the
publisher only for `ERPX_WORKER_HANDLER=tenant_projection_status_projection`.

Evidence: `audits/G6-PRV-015/README.md`.

Independent verification:

- `cargo fmt --all --check` — PASS.
- `cargo check -p erpx-worker --bin erpx-worker` — PASS.
- `cargo clippy -p erpx-worker --all-targets -- -D warnings` — PASS.
- `cargo test -p erpx-worker` — PASS: 47 tests passed, one documented
  disposable-database test ignored.
- `cargo test -p erpx-infrastructure --test tenant_projection_status_read
  --test tenant_projection_status --lib` — PASS: 95 library tests passed,
  four documented ignored; status and read suites passed (5 and 2 tests).

Default deployment cadence, freshness producers, replacement-read switching,
rebuild/action controls, complete Platform Operations workflows, and all
remaining ERP requirements remain open.

### Rollback

Remove only the additive worker publisher module/export, worker composition
branch, and steady-state writer method. Retain the rebuildable status schema
and G6-PRV-014 writer until a reviewed retention decision.

## Entry 2026-07-25 — integrated G6-PRV-014 scheduler status projection writer

Reviewed and integrated the validated control-write to control-read status
projection writer. It performs version-ordered idempotent upserts, bounded
error redaction, read-model clearing, and deterministic rebuilds without
claiming jobs or registering an implicit worker. Invalid status counters,
references, and kind/state combinations fail closed before persistence.

Evidence: `audits/G6-PRV-014/README.md`.

Independent verification:

- `cargo fmt --all` — PASS.
- `cargo test -p erpx-infrastructure --test tenant_projection_status_read` —
  PASS: 2 tests.
- `cargo clippy -p erpx-infrastructure --all-targets -- -D warnings` — PASS.

Worker registration/cadence, incremental publishing, freshness, replacement
read switching, and Platform Operations rebuild/action controls remain open.

### Rollback

Remove only the additive projection writer/export, focused test, and audit.
Retain any populated rebuildable read table until a reviewed retention plan.

## Entry 2026-07-25 — mandatory verification after G6-PRV-014/G6-PRV-015

- `cargo fmt --all --check` — PASS.
- `cargo clippy --workspace --all-targets -- -D warnings` — PASS.
- `cargo test --workspace` — PASS: all active workspace tests passed,
  including infrastructure 95 passed with 4 documented database tests
  ignored, worker 47 passed with 1 documented database test ignored, server
  61, migration 6, UI 55, console 14, domain 135, and application 8; all doc
  tests passed.
- `cargo check -p erpx-console --no-default-features --features web` — PASS.
- `cargo check -p erpx-console --no-default-features --features desktop` — PASS.
- Folio `bun run lint` — PASS with four pre-existing warnings in
  `infra/scripts/start-n8n.js` and zero errors.
- Folio `bunx tsc --noEmit` — PASS.
- Folio remained operational on port 3004; PostgreSQL remained on port 5432.
- No disposable `erpx_%20260725` databases remained.

This is integration-health evidence only. Incremental durable source
checkpoints, freshness producers, replacement-read switching, rebuild/action
controls, all remaining ERP contexts, native Android/iOS evidence,
migration/cutover, hardening, and Folio write retirement remain open.

## Entry 2026-07-25 — integrated G6-PRV-016 projection publication freshness

Added `published_at` to the rebuildable control-read scheduler-status model
and transport. The writer records publication time independently from source
`updated_at`; Platform Operations returns both timestamps and classifies each
response row as fresh, stale, or unknown for future clock-skewed timestamps.
The 300-second threshold is inclusive at the exact boundary and is evaluated
once per response.

Evidence: `audits/G6-PRV-016/README.md`.

Focused verification passed: migration 6 tests, infrastructure status/read
suites, server 62 tests including exact freshness-boundary coverage, and
worker 47 tests with one documented database integration test ignored.

## Entry 2026-07-25 — integrated G6-PRV-017 durable incremental checkpoint

Added migration `0006_tenant_projection_status_checkpoint.sql`, the typed
`ProjectionStatusChangeCursor`, bounded source-update keyset paging, and an
atomic page/upsert/checkpoint transaction in the control-read projection
writer. Replayed pages are idempotent; full rebuild clears and reconstructs
the status model and source cursor. The cursor contains only `(updated_at,
job_id)` ordering metadata.

Evidence: `audits/G6-PRV-017/README.md` and
`crates/infrastructure/tests/tenant_projection_status_incremental.rs`.

Independent live verification passed on two physically separate disposable
PostgreSQL databases: first publication wrote one row, immediate replay wrote
zero rows, a later source update advanced the checkpoint and read status, and
the second replay again wrote zero rows. Both databases were dropped after
the run.

Focused verification passed: six status contract tests, two read-adapter
tests, and the ignored integration target when run with disposable databases.

## Entry 2026-07-25 — mandatory verification after G6-PRV-016/G6-PRV-017

- `cargo fmt --all --check` — PASS.
- `cargo clippy --workspace --all-targets -- -D warnings` — PASS.
- `cargo test --workspace` — PASS; all active workspace tests passed, with
  documented disposable-database tests ignored, including infrastructure
  unit/integration suites, worker 48 passed with 1 ignored, server 62,
  migration 6, UI 55, console 14, domain 135, identity 20, and application
  8; all doc tests passed.
- `cargo check -p erpx-console --no-default-features --features web` — PASS.
- `cargo check -p erpx-console --no-default-features --features desktop` — PASS.
- Folio `bun run lint` — PASS with four pre-existing warnings in
  `infra/scripts/start-n8n.js` and zero errors.
- Folio `bunx tsc --noEmit` — PASS.
- Folio remained operational on port 3004; PostgreSQL remained on port 5432.
- No disposable `erpx_ctrl_*20260725_g6p017` or other packet databases
  remained.

This is integration-health evidence only. Physical replacement-read switching,
rebuild/action controls, source-lag observability, external backup/restore,
all remaining ERP contexts, native Android/iOS evidence, migration/cutover,
hardening, and Folio write retirement remain open.

## Entry 2026-07-25 — integrated G6-PRV-018 failure-safe replacement-read orchestration

The worker replacement-read boundary now includes an explicit abort operation.
If a candidate rebuild fails, the orchestrator best-effort records the
candidate as failed and preserves the active read database. Activation errors
also use bounded cleanup, while projection names, candidate references, and
worker error messages remain validated and redacted. This packet only closes
the failure cleanup state transition; it does not claim physical candidate
database routing or authenticated Platform Operations actions.

Evidence: `audits/G6-PRV-018/README.md`.

Independent verification:

- `cargo fmt --all --check` — PASS.
- `cargo check -p erpx-worker` — PASS.
- `cargo clippy -p erpx-worker --all-targets -- -D warnings` — PASS.
- `cargo test -p erpx-worker replacement_switcher_is_bounded_and_fail_closed_on_projection_mismatch` — PASS.

Physical candidate rebuild/provisioning, projector-to-candidate pool binding,
authenticated rebuild/switch controls, external backup/restore execution,
and the remaining G6/platform requirements remain open.

## Entry 2026-07-25 — mandatory verification after G6-PRV-018

- `cargo fmt --all --check` — PASS.
- `cargo clippy --workspace --all-targets -- -D warnings` — PASS.
- `cargo test --workspace` — PASS: application 8, console 14, domain 135,
  identity 20, infrastructure 95 with 4 ignored, migration 6, server 63,
  UI 55, and worker 48 with 1 ignored; no failures.
- `cargo check -p erpx-console --no-default-features --features web` — PASS.
- `cargo check -p erpx-console --no-default-features --features desktop` — PASS.
- Folio `bun run lint` — PASS with the same four pre-existing warnings in
  `infra/scripts/start-n8n.js` and zero errors.
- Folio `bunx tsc --noEmit` — PASS.
- Folio remained operational on port 3004; PostgreSQL remained on port 5432.
- The two disposable G6-PRV-017 databases were removed; no packet databases
  remained.

This is integration-health evidence only. Physical candidate read-pool
binding, rebuild/action transport, source-lag observability, external
backup/restore, all remaining ERP contexts, native Android/iOS evidence,
migration/cutover, hardening, and Folio write retirement remain open.

## Entry 2026-07-25 — integrated G6-PRV-019 authenticated rebuild intent

Platform Operations now exposes `StartProjectionRebuild`. The command requires
an authenticated command context, platform scope, matching actor attribution,
and `platform.projection.rebuild`. It accepts only the implemented
`tenant_projection_status` projection and a positive source version, validates
opaque candidate references, records `rebuilding` state through the existing
tenant replacement-read state machine, and writes an audit event. It never
returns connection material and cannot directly activate a read database.

Evidence: `audits/G6-PRV-019/README.md`.

Independent verification:

- `cargo fmt --all --check` — PASS.
- `cargo test -p erpx-server --bin erpx-server` — PASS: 63 tests.
- `cargo clippy --workspace --all-targets -- -D warnings` — PASS.
- `cargo check -p erpx-console --no-default-features --features web` — PASS.
- `cargo check -p erpx-console --no-default-features --features desktop` — PASS.

## Entry 2026-07-25 — mandatory verification after G6-PRV-019

- `cargo fmt --all --check` — PASS.
- `cargo clippy --workspace --all-targets -- -D warnings` — PASS.
- `cargo test --workspace` — PASS; all active tests passed, including server
  63, worker 48 with 1 documented database test ignored, infrastructure 99
  library tests plus documented integration tests, migration 6, UI 55,
  console 14, domain 135, identity 20, and application 8; all doc tests
  passed.
- `cargo check -p erpx-console --no-default-features --features web` — PASS.
- `cargo check -p erpx-console --no-default-features --features desktop` — PASS.
- Folio `bun run lint` — PASS with four pre-existing warnings in
  `infra/scripts/start-n8n.js` and zero errors.
- Folio `bunx tsc --noEmit` — PASS.
- Folio remained operational on port 3004; PostgreSQL remained on port 5432.
- No disposable packet databases remained.

This is integration-health evidence only. Candidate read-pool binding,
worker rebuild execution, activation through the product surface,
source-lag observability, external backup/restore, all remaining ERP contexts,
native Android/iOS evidence, migration/cutover, hardening, and Folio write
retirement remain open.

## Entry 2026-07-25 — integrated G6-PRV-020 candidate read provisioning and fenced activation

Bound replacement-read work to a physical, server-created candidate database
and a concrete Operations projector. Candidate routing now validates the
tenant/holding switch binding, rejects database keys owned by another tenant,
and probes physical database identity, required tenant-read relations, forced
RLS/policies, and holding isolation before handing a pool to a projector.
The worker candidate path clears and replays Operations against the candidate
read pool while reading immutable source events from the write pool.
Candidate preparation is an injected worker boundary backed by the physical
tenant provisioner, including explicit holding binding and tenant-read
migration before projector access.

Replacement completion, abort, and activation now accept a monotonic switch
generation for compare-and-swap fencing. Platform Operations rebuild intent
returns that generation, and the authenticated audited activation command
requires the exact ready candidate generation. Candidate/database references
remain server-side opaque and no credentials cross the protobuf boundary.

Evidence: `audits/G6-PRV-020/README.md`.

Independent focused verification:

- `cargo check -p erpx-infrastructure` — PASS.
- `cargo check -p erpx-worker` — PASS.
- `cargo test -p erpx-worker --lib` — PASS: 49 active tests, one documented
  disposable control-database test ignored.
- `cargo test -p erpx-server --bin erpx-server` — PASS: 63 tests.
- `cargo test -p erpx-infrastructure --test provisioning_replacement --
  --ignored --nocapture` — PASS against disposable PostgreSQL databases.
- `cargo test -p erpx-worker --test operations_projection -- --ignored
  --nocapture` — PASS against disposable control/write/read/candidate
  PostgreSQL databases; one Operations source event was replayed into the
  candidate before generation-fenced activation.

Source high-watermark verification, durable worker dispatch from the intent,
external backup/restore execution, complete Platform Operations actions,
remaining ERP contexts, native Android/iOS evidence, migration/cutover,
hardening, and Folio write retirement remain open.

### Rollback

Remove only the additive candidate provisioning/projector binding, generation
fences, activation RPC, and focused evidence. Preserve active tenant registry
read routing and existing backup/restore behavior until a reviewed replacement
is available.

## Entry 2026-07-25 — mandatory verification after G6-PRV-020

- `cargo fmt --all --check` — PASS.
- `cargo clippy --workspace --all-targets -- -D warnings` — PASS.
- `cargo test --workspace` — PASS; all active workspace tests passed,
  including application 8, console 14, domain 135, identity 20,
  infrastructure 99, migration 6, server 63, UI 55, and worker 49, with
  only documented disposable-database tests ignored; all doc tests passed.
- `cargo check -p erpx-console --no-default-features --features web` — PASS.
- `cargo check -p erpx-console --no-default-features --features desktop` — PASS.
- Folio `bun run lint` — PASS with the same four pre-existing warnings in
  `infra/scripts/start-n8n.js` and zero errors.
- Folio `bunx tsc --noEmit` — PASS.
- Folio remained operational on port 3004; PostgreSQL remained on port 5432.
- No packet databases remained after the disposable G6-PRV-020 live test.

This is integration-health evidence only. Source high-watermark/catch-up
verification, durable worker dispatch from the Platform Operations intent,
external backup/restore execution, complete Platform Operations actions, all
remaining ERP contexts, native Android/iOS evidence, migration/cutover,
hardening, and Folio write retirement remain open.

## Entry 2026-07-25 — integrated G6-PRV-021 candidate source high-watermark verification

Candidate Operations and Finance Banking rebuilders now receive the expected
source version from the rebuild intent and compare a holding-scoped,
append-only source-event count before and after replay. A mismatch with the
intent is permanent; movement during replay is retryable and prevents
activation. Existing generation-fenced completion, abort, and activation
remain the only switch transitions.

Evidence: `audits/G6-PRV-021/README.md`.

Focused verification:

- `cargo fmt --all --check` — PASS.
- `cargo clippy -p erpx-worker --all-targets -- -D warnings` — PASS.
- `cargo check -p erpx-infrastructure -p erpx-worker --tests` — PASS.
- `cargo test -p erpx-worker --test operations_projection -- --ignored
  --nocapture` — PASS against disposable control/write/read/candidate
  PostgreSQL databases; the disposable databases were dropped afterward.

Durable dispatch from the authenticated Platform Operations intent, external
backup/restore execution, complete Platform Operations actions, remaining ERP
contexts, native Android/iOS evidence, migration/cutover, hardening, and
Folio write retirement remain open.

### Rollback

Remove the additive source-watermark methods, candidate-rebuilder argument,
and G6-PRV-021 evidence. No migration or database rollback is required.

## Entry 2026-07-25 — mandatory verification after G6-PRV-021

- `cargo fmt --all --check` — PASS.
- `cargo clippy --workspace --all-targets -- -D warnings` — PASS.
- `cargo test --workspace` — PASS; all active workspace tests passed and all
  doc tests passed; disposable-database tests remained explicitly ignored.
- `cargo check -p erpx-console --no-default-features --features web` — PASS.
- `cargo check -p erpx-console --no-default-features --features desktop` — PASS.
- Folio `bun run lint` — PASS with the same four pre-existing warnings in
  `infra/scripts/start-n8n.js` and zero errors.
- Folio `bunx tsc --noEmit` — PASS.
- Folio remained operational on port 3004; PostgreSQL remained on port 5432.
- No disposable packet databases remained after the focused live test.

This is integration-health evidence only. Durable worker dispatch from the
Platform Operations intent, external backup/restore execution, complete
Platform Operations actions, all remaining ERP contexts, native Android/iOS
evidence, migration/cutover, hardening, and Folio write retirement remain
open.

## Entry 2026-07-25 — integrated G6-PRV-022 durable candidate dispatch

The authenticated Platform Operations rebuild intent now reaches the durable
control-plane scheduler with an opaque candidate key, source version, and
generation payload. The server validates platform scope, actor attribution,
tenant activity, and the current Operations source watermark before creating
the generation-fenced switch intent. Enqueue failure fails that exact
generation.

The worker binary explicitly composes the multi-tenant routed scheduler for
Operations and Finance Banking. Candidate payload validation, preparation,
write/read routing, projector replay, generation-fenced completion, and
activation are handled by the existing worker path; the default worker
registry remains fail-closed and does not claim scheduler rows implicitly.

Evidence: `audits/G6-PRV-022/README.md`.

Focused verification:

- `cargo test -p erpx-infrastructure --test tenant_projection_scheduler --
  --ignored --nocapture` — PASS against two disposable control-plane
  PostgreSQL databases; the databases were explicitly dropped afterward.
- Existing worker candidate replay and the full post-G6-PRV-021 verification
  wave remain PASS as recorded above.

External backup/restore execution, complete Platform Operations actions, all
remaining ERP contexts, native Android/iOS evidence, migration/cutover,
hardening, and Folio write retirement remain open.

### Rollback

Remove only the G6-PRV-022 evidence and the durable candidate-dispatch
composition if a reviewed replacement is available. Do not delete scheduler
rows or alter active tenant routing as part of rollback; no migration rollback
is required.

## Amendment 2026-07-25 — G6-PRV-022 atomic dispatch integration

The durable dispatch packet is now backed by
TenantProjectionSchedulerRepository::begin_rebuild_after_platform_authorization.
That method creates the generation-fenced switch, scheduler job, rebuild
record, and platform start audit event in one control-write transaction after
rechecking the active tenant/holding binding. The server no longer commits a
switch, enqueue, and audit as separate operations.

Additional evidence:

- Disposable scheduler integration now verifies the atomic switch/job/rebuild/
  audit result and drops both control databases.
- cargo clippy -p erpx-infrastructure -p erpx-worker -p erpx-server
  --all-targets -- -D warnings — PASS.
- Physical provisioning and Operations projector disposable tests remain
  PASS after the fenced route and ready/activation retry changes.

## Amendment 2026-07-25 — G6-PRV-022 source fence and terminal stale intent

Independent QA initially identified that an untyped rebuild could fall
through to the active read route and that candidate projector errors lost
retry classification. The worker now rejects every rebuild without the
candidate database key, source version, and generation before routing. The
executor preserves retryable projector/completion/activation failures,
classifies a source move distinctly, and the scheduler dead-letters that
stale intent immediately through `fail_terminal`.

Operations and Finance Banking write repositories acquire a shared
holding-scoped transaction advisory lock. The candidate worker acquires the
same lock after replay, rechecks the source watermark on the locked tenant
write transaction, completes the exact generation, activates it, and commits
the lock transaction. Source commits therefore cannot occur between the
final source check and promotion; cancellation rolls the lock back.

Independent evidence:

- `cargo fmt --all --check` — PASS.
- `cargo clippy --workspace --all-targets -- -D warnings` — PASS.
- `cargo test -p erpx-infrastructure tenant_projection_scheduler::tests::stale_candidate_failure_is_terminal_without_consuming_retry_budget` — PASS.
- `cargo test -p erpx-worker --lib tenant_projection_scheduler::tests` — PASS; 7 tests.
- Disposable Operations routed projection — PASS; explicit cleanup verified no
  `erpx_fence_*` or `erpx_ops_projection_*` databases remained.

Remaining G6 concerns are explicit: source fences are implemented only for
the Operations and Finance Banking producer paths, generic scheduler payloads
still need a typed persistence contract, activation acknowledgement recovery
needs a durable idempotency response, and external backup/restore remains
unimplemented. All G1-G14 gates remain open.

## Amendment 2026-07-25 — G6-PRV-022 activation recovery and lease renewal

Ready-candidate recovery and authenticated Platform Operations activation now
reacquire the holding-scoped transaction advisory lock before checking the raw
source watermark and promoting a generation. The legacy direct switcher fails
closed, and initial source mismatches are terminal stale outcomes rather than
ordinary retries. The scheduler also renews active projection claims at half
lease while a long-running executor is in progress.

Evidence:

- `cargo test -p erpx-worker --lib tenant_projection_scheduler::tests` — PASS;
  8 tests, including `long_projection_renews_claim_before_completion`.
- `cargo clippy --workspace --all-targets -- -D warnings` — PASS.
- Full Rust workspace tests, Dioxus web/desktop checks, Folio lint, and
  TypeScript checks — PASS in the current integration wave.
- Folio remains on port 3004 and PostgreSQL on port 5432.

The fence still applies only to the typed Operations and Finance Banking source
writers. Crash injection around candidate failure/terminalization, durable
monotonic source versions, external backup/restore, remaining ERP contexts,
native Android/iOS evidence, migration/cutover, hardening, and Folio write
retirement remain open.

## Entry 2026-07-25 — G8-FIN-016 bank-to-GL matching persistence

AG-FIN's pure deterministic bank-match contract was independently integrated
with tenant SQLx persistence. Migration 0030 adds RLS-backed proposal and
decision facts with proposal-to-decision binding. The write adapter reloads the
authoritative bank account and transaction, reconstructs the proposal, checks
immutable posted-journal existence/scope/date, validates decisions against the
stored proposal, and uses typed request-hash replay receipts.

Evidence: `audits/G8-FIN-016/README.md`.

Focused disposable live persistence, migration assertion, domain, and
infrastructure tests pass. The G8 gate remains open for legacy posted-journal
fact backfill and because the packet does not implement the remaining Finance
close, tax, consolidation, reporting, or provider workflows.

## Amendment 2026-07-25 — G8-FIN-016 journal-fact and immutability hardening

QA identified that match persistence was trusting caller-supplied journal
amount/currency and that the posted-journal table had no database mutation
guard. Migration 0031 now stores currency and the balanced debit total when a
journal is posted and rejects posted-journal updates/deletes with a database
trigger. Matching checks those persisted facts and fails closed for legacy rows
where the new facts are absent. The decision table also enforces one immutable
decision per proposal.

The disposable Finance banking test now passes through a real posted journal,
matches its persisted amount/currency, verifies exact replay, and proves the
database trigger rejects mutation. The G8 gate remains open for legacy fact
backfill and the remaining Finance close, tax, consolidation, reporting, and
provider workflows.

QA also found that the generic backup/recovery metadata-only activation path
could not hold a tenant source fence. `activate_read_database` now fails closed
and leaves route-aware activation to the fenced scheduler; backup/PITR
execution remains an open implementation requirement rather than an unfenced
success path.

## Amendment 2026-07-25 — G6 stale activation terminalization and G8 replay concurrency

Manual Platform Operations activation now treats a source-watermark mismatch
as a terminal stale generation: it fails the exact candidate, terminalizes
the matching pending/retryable/claimed rebuild job and active claim, marks the
rebuild record dead-lettered, and records a platform audit event before
returning a failed-precondition response. The terminalization path validates
the registered tenant/holding binding and exact candidate/source/generation
payload, while permission and actor authorization remain at the server
boundary.

Finance bank-match proposal and decision writes now take transaction-scoped
holding/request advisory locks before idempotency lookup and authoritative
validation. This makes concurrent identical requests resolve to one durable
write plus a typed replay receipt instead of leaking a uniqueness race.

Verification after this integration wave:

- `cargo fmt --all --check` — PASS.
- `cargo clippy --workspace --all-targets -- -D warnings` — PASS.
- `cargo test --workspace` — PASS; active workspace tests and doc tests passed.
- Disposable Finance live test with two concurrent proposal writes — PASS;
  persisted journal amount/currency, exact replay, database immutability
  trigger, and reconciliation were exercised against fresh tenant databases,
  then the databases were dropped.
- `cargo check -p erpx-console --no-default-features --features web` — PASS.
- `cargo check -p erpx-console --no-default-features --features desktop` — PASS.
- Folio `bun run lint` — PASS with the four existing n8n warnings and zero
  errors.
- Folio `bunx tsc --noEmit` — PASS.
- Folio remained operational on port 3004; PostgreSQL remained on port 5432;
  no disposable packet databases remained.

The G6 backup/PITR route-aware activation adapter, remaining ERP contexts,
native Android/iOS evidence, migration/cutover, hardening, and Folio write
retirement remain open. This ledger entry is integration evidence, not a
completion claim.

## Amendment 2026-07-25 — G6-PRV-022 route-aware restore activation

The backup/recovery seam now has a routed restore-test API. It validates a
shared control plane, tenant/holding binding, candidate identity, and source
projection; invokes the injected restore executor; acquires the tenant write
route's holding-scoped source lock; verifies the source watermark before and
after exact candidate routing/probing; and promotes only the exact generation
through `activate_read_database_if_generation`. The metadata-only activation
entry point remains fail-closed.

Independent evidence:

- `cargo fmt --all --check` — PASS.
- `cargo check -p erpx-infrastructure` — PASS.
- `cargo clippy -p erpx-infrastructure --all-targets -- -D warnings` — PASS.
- Disposable `real_physical_provisioning_and_replacement_read_path_is_safe` —
  PASS; stale-source rejection and successful route-aware activation were
  verified against disposable control, tenant-write, tenant-read, and
  candidate databases, then cleaned up.
- Disposable scheduler terminalization test — PASS; the job, active claim,
  and rebuild row were all asserted as dead-lettered with the bounded stale
  outcome.

The route-aware method still depends on an injected external backup/PITR
executor and supports only the currently typed Operations and Finance Banking
source fences. Cross-database crash recovery, additional source contexts,
remaining ERP modules, migration/cutover, hardening, native mobile evidence,
and Folio write retirement remain open.

## Amendment 2026-07-25 — G8-FIN-016 legacy fact backfill

Tenant-write migration 0032 now performs a bounded, idempotent best-effort
backfill for legacy posted journals only when historical journal-payload
currency and complete numeric line facts are available. It does not use the
mutable current chart to invent a historical fact, preserves existing partial
facts, safely ignores malformed/overflow/incomplete line sets, disables only
the named immutability trigger during the migration, and restores both that
trigger and forced RLS.

Evidence:

- Migration embedding test verifies migration 0032 contains named trigger
  handling, temporary RLS mode, historical source currency, numeric parsing,
  JSON line extraction, and complete-line gating.
- Upgrade-shaped disposable PostgreSQL test applied tenant-write migrations
  through 0030 using the actual legacy posted-journal payload shape, then
  applied 0031 and 0032; valid debit/credit totals became `NULL:1000` because
  historical currency is absent, unsafe amounts stayed NULL, partial values
  were preserved/completed, and forced RLS plus the immutability trigger were
  restored. The scenario is captured by
  the ignored committed test in
  `crates/infrastructure/tests/finance_backfill_upgrade.rs`.
- Fresh disposable tenant migration and Finance live persistence test pass;
  posted journal facts, concurrent proposal replay, trigger rejection, and
  reconciliation remain green.

Rows whose historical payload or chart cannot support a deterministic
backfill, the remaining Finance close/tax/consolidation/reporting/provider
workflows, and the full ERPX completion audit remain open.

## Entry 2026-07-25 — G8-FIN-017 Finance budget controls and commitments

The company-scoped Finance budget slice is now integrated. `FinanceBudget`
validates company scope, lines, periods, currency, lifecycle, two-person
approval, approval limits, lock/close ownership, and exact budget-control
boundaries. `SqlxFinanceBudgetStore` persists optimistic versions and atomic
snapshots/outbox events. Commitment recording uses holding/request advisory
locks, reloads the approved budget, rejects over-control, stores immutable
source commitment facts plus the original control decision, and returns exact
idempotent replays without double counting even after budget closure. Source
and idempotency locks are acquired in deterministic order to close cross-key
unique-conflict races.

Migration 0033 adds forced-RLS budget and commitment tables, unique
holding-scoped idempotency/source references, and a database immutability
trigger for commitments.

Evidence: `audits/G8-FIN-017/README.md`; focused domain, migration,
infrastructure check/Clippy, disposable live budget persistence, and
upgrade-shaped Finance migration safety tests all pass.

This packet does not close G8. The commitment source is not yet integrated
with Procurement/Sales/Operations producers; budget transport/UI, Finance
close, AP/AR completion, consolidation, Thai tax/statutory reporting,
statements, and universal SoD/approval adoption remain open.

## Entry 2026-07-25 — G10-MKT-019 Marketing-to-CRM contract

AG-MKT delivered and the main agent integrated a pure Marketing-to-CRM
handoff/attribution contract. Typed Marketing acquisition and qualification
references remain distinct from CRM accepted leads, accounts, contacts, and
opportunities. The CRM lifecycle is `draft → qualified → won/lost`. First,
last, linear, U-shaped, and weighted attribution canonicalize touch order,
allocate exact integer revenue totals, and reject cross-tenant, negative,
duplicate, invalid-weight, and conflicting idempotency inputs.

Evidence: `audits/G10-MKT-019/README.md`; `cargo test -p erpx-domain
marketing_crm` (5 passed), `cargo clippy -p erpx-domain --all-targets --
-D warnings` (PASS), and `cargo fmt --all --check` (PASS).

This pure contract does not close G10. CRM persistence/transport/UI,
campaign journeys, provider delivery, consent enforcement at delivery,
attribution projections, and Finance revenue reconciliation remain open.

## Entry 2026-07-25 — G8-FIN-018 Finance close and reporting contract

AG-FIN delivered and the main agent integrated a pure Finance period-close
contract. `FinanceCloseReview` enforces company scope, period/fact binding,
balanced immutable debit/credit facts, duplicate-account rejection, checked
arithmetic, unresolved-control fail-closed behavior, and opener/reviewer/
closer separation. Deterministic statement totals and account drill-down rows
are derived without mutating posted journals.

Evidence: `audits/G8-FIN-018/README.md`; `cargo test -p erpx-domain
finance_close` (6 passed), `cargo clippy -p erpx-domain --all-targets --
-D warnings` (PASS), and `cargo fmt --all --check` (PASS).

This is a pure contract slice and does not close G8. SQLx close persistence,
period-lock orchestration, transport/UI, close workers, Thai tax, statements
integration, consolidation, migration/reconciliation, and universal Finance
SoD adoption remain open.

## Entry 2026-07-25 — G8-FIN-019 Finance close review persistence

The Finance close aggregate now has a holding-scoped SQLx write adapter with
optimistic versioning, domain-replayed lifecycle transitions, context-bound
idempotency, atomic snapshot/outbox writes, and closed-row database
immutability. Persistence verifies every submitted close fact against the
immutable `finance_posted_journals` payload, including identity, period/date,
balanced source lines, and account totals. Resolved control flags must exactly
match the forced-RLS `finance_close_control_evidence` ledger, whose rows are
frozen after period close.

Evidence: `audits/G8-FIN-019/README.md`; focused infrastructure tests and
`live_finance_close_review_schema_is_scoped_and_closed_rows_are_immutable`,
`live_finance_close_persistence_requires_authoritative_facts_and_controls`
both pass against disposable PostgreSQL tenant databases; migration and
infrastructure Clippy/checks pass.

This packet does not close G8. Control-evidence producers, close transport/UI,
workers, AP/AR completion, consolidation, Thai tax/statutory reporting,
statements integration, Finance SoD adoption, and migration/reconciliation
remain open.

## Entry 2026-07-25 — G8-FIN-020 Finance close-control evidence source adapter

The authoritative close-control evidence table now has a separate SQLx source
writer. It validates bounded source identity and payloads before opening a
holding-scoped transaction, reloads the Finance period, serializes per-control
advisory updates, preserves the source aggregate binding, returns exact
replays, rejects stale or same-version conflicting writes, and rejects writes
after period or close-review closure. Migration 0037 adds the positive source
version column. The main agent exported the adapter and independently
verified its unit tests, Clippy, and disposable PostgreSQL
replay/scope/version/immutability coverage.

Evidence: `audits/G8-FIN-020/README.md`; `cargo test -p
erpx-infrastructure finance_close_control_evidence` (2 passed),
`cargo clippy -p erpx-infrastructure --all-targets -- -D warnings` (PASS),
and `live_source_evidence_is_scoped_replayed_and_closed_immutable` (PASS,
including stale-version rejection).

This packet does not close G8. Source-context outbox publication, close
transport/workers/UI, AP/AR completion,
consolidation, Thai tax/statutory reporting, statements, Finance SoD adoption,
and migration/reconciliation remain open.

## Verification wave 2026-07-25 — G8-FIN-017/018/019 integration

After integrating the budget hardening and close contract, the required
workspace checks passed:

- `cargo fmt --all --check` — PASS.
- `cargo clippy --workspace --all-targets -- -D warnings` — PASS.
- `cargo test --workspace` — PASS; 153 domain tests and all active workspace
  tests/doc tests passed; disposable tests remain explicitly ignored unless
  supplied a database URL.
- `cargo check -p erpx-console --no-default-features --features web` — PASS.
- `cargo check -p erpx-console --no-default-features --features desktop` — PASS.
- Folio `bun run lint` — PASS with four pre-existing n8n warnings and zero
  errors; `bunx tsc --noEmit` — PASS.
- Fresh disposable budget persistence, legacy backfill upgrade, and Finance
  close persistence/authority tests — PASS; all temporary databases were
  dropped.

Folio remains operational on port 3004 and PostgreSQL on port 5432. No
completion claim is made; G1-G14 gates and final Folio write retirement remain
open.

## Entry 2026-07-25 — G10-MKT-020 CRM write persistence integration

**Owner:** AG-MKT packet; main-agent integration and independent verification  
**Dependencies:** G10-MKT-019 pure CRM contract; G1 tenant write/outbox
primitives  
**Allowed paths:** `crates/infrastructure` CRM persistence module and focused
test, `crates/migration` additive tenant-write migration/assertion, audit  
**Forbidden paths:** shared domain/protobuf contracts, Finance, Folio, UI,
environment/runtime state, and read-side projection paths

### Delivered behavior

- Integrated `SqlxMarketingCrmStore` for immutable Marketing accepted-lead
  handoffs, optimistic CRM opportunity lifecycle writes, and deterministic
  first/last/linear/U-shaped/weighted attribution results.
- Enforced holding-scoped transactions, forced-RLS tables, composite source
  foreign keys, immutable source/commercial bindings, aggregate snapshots,
  outbox events, and idempotency receipts.
- Added migration 0036 assertions and exported the store/receipt types.
- Corrected the domain transition contract so each opportunity lifecycle
  transition binds its command ID and deterministic idempotency key to its new
  command context; this permits safe versioned transitions without reusing the
  create-command idempotency binding.

### Evidence

- `cargo fmt --all --check` — PASS.
- `cargo clippy -p erpx-infrastructure --all-targets -- -D warnings` — PASS.
- `cargo test -p erpx-migration` — 6 passed.
- `live_marketing_crm_write_boundary_is_atomic_idempotent_and_scoped` — PASS
  against a disposable PostgreSQL database: handoff, create, new-context
  qualification, deterministic attribution, exact replays, four outbox events,
  and forced-RLS metadata. The database was dropped after the run.
- `cargo check -p erpx-server` — PASS after shared export integration.

### Remaining boundary

The CRM read database/projection, worker registration, gRPC transport, UI,
provider journeys, consent/delivery enforcement, and Finance revenue
reconciliation remain open. This packet does not close G10.

### Rollback

Remove only migration 0036, the CRM persistence module/export, its focused
test, the domain transition metadata amendment, and this audit entry after
confirming no later migration or projection depends on them. Do not reset or
remove existing Folio work.

## Entry 2026-07-25 — G8-FIN-020 source-version and QA hardening

The close-control source adapter now persists a positive `source_version`,
rejects stale versions, rejects same-version conflicting evidence, and keeps
the source aggregate binding immutable. The live acceptance test explicitly
covers stale and same-version conflict paths. Independent follow-up QA found
no High or Medium findings; the only remaining recommendation was the now-added
same-version live assertion.

Evidence: `audits/G8-FIN-020/README.md`, migration 0037, focused unit tests,
and the disposable source-evidence live test all pass. Source-context outbox
publication remains a separate open dependency.

## Verification wave 2026-07-25 — G8-FIN-020 and G10-MKT-020

Focused integration evidence after both packets:

- Finance source-evidence live test — PASS.
- Finance close schema/authority live tests — 2 PASS.
- CRM write-boundary live test — PASS.
- `cargo check -p erpx-server` — PASS.
- `cargo test -p erpx-migration` — 6 PASS.

The full workspace/Folio verification wave is still required below before
selecting the next integration checkpoint. G1-G14 gates and Folio write
retirement remain open.

## Verification wave 2026-07-25 — full Rust/Dioxus/Folio checks after CRM/Finance integration

- `cargo fmt --all --check` — PASS.
- `cargo clippy --workspace --all-targets -- -D warnings` — PASS.
- `cargo test --workspace` — PASS; 153 domain tests and all active workspace
  suites passed; database-dependent suites remain explicitly ignored unless
  supplied a disposable URL.
- `cargo check -p erpx-console --no-default-features --features web` — PASS.
- `cargo check -p erpx-console --no-default-features --features desktop` — PASS.
- `cargo check -p erpx-server` — PASS.
- `bun run lint` — PASS with four existing warnings in
  `infra/scripts/start-n8n.js` and zero errors.
- `bunx tsc --noEmit` — PASS.
- `git diff --check` — PASS.

Runtime evidence: Folio remains listening on port 3004, PostgreSQL is ready,
and no `erpx_*` disposable databases remain. The active ERPX goal remains
open; this is verification evidence only.

## Entry 2026-07-25 — G8-FIN-021 source close-control outbox publication

**Owner:** AG-FIN packet; main-agent review and hardening  
**Dependencies:** G8-FIN-018, G8-FIN-019, G8-FIN-020  
**Allowed paths:** Finance close-control source adapter, focused disposable
test, audit  
**Forbidden paths:** domain contracts, close review, other Finance contexts,
shared transport, worker, UI, Folio, and environment/runtime state

### Delivered behavior

- Insert and strictly newer source-version updates now append one bounded
  `finance.close_control_evidence_published.v1` event in the same transaction
  as the evidence row.
- Exact replay emits no second event; stale, same-version conflict,
  source-rebind, scope mismatch, and closed-period/review failures emit none.
- Main review made the outbox aggregate identity deterministic per
  holding/company/period/control, preserving ordered streams when multiple
  controls share one source aggregate.

### Evidence

- Four source adapter unit tests pass.
- Disposable `live_source_evidence_is_scoped_replayed_and_closed_immutable`
  passes after the envelope and control-scoped aggregate-ID refinements.
- `cargo fmt --all --check`, infrastructure Clippy, and infrastructure check
  pass.

### Remaining boundary

Worker/read projection consumption and canonical event-envelope migration
remain open. This packet does not close G8.

## Entry 2026-07-25 — G10-MKT-021 CRM tenant-read projection

**Owner:** AG-MKT packet; main-agent integration and independent extension test  
**Dependencies:** G10-MKT-020 CRM write persistence, existing Marketing
projection runner and read migration 0030  
**Allowed paths:** established Marketing SQLx projection adapter, worker event
type propagation, migration assertions, focused test, audit  
**Forbidden paths:** domain/protobuf/server/console/UI/provider code, Finance,
Folio, and credentials/runtime state

### Delivered behavior

- Allow-listed CRM handoff, opportunity, and attribution outbox events in the
  existing Marketing source page and projection adapter.
- Added payload-hashed event receipts, event envelope/type/version/identity
  validation, stale-version handling, conflict rejection, and CRM rebuild
  clearing across the existing forced-RLS read schema.
- Preserved event type through the worker’s existing Marketing projection
  seam; no tenant credentials or new worker database path were introduced.
- Added migration assertions for all four CRM read tables and policies.

### Evidence

- `cargo fmt --all --check` — PASS.
- Infrastructure Clippy/checks and worker checks — PASS.
- `cargo test -p erpx-migration` — 6 PASS.
- Expanded disposable CRM read test — PASS against separate tenant write/read
  databases, covering handoff, opportunity create/transition, attribution,
  replay receipts, rebuilt-row clearing, forced-RLS tables, and policies.

### Remaining boundary

The existing Marketing projection runner is now wired into the production
worker scheduler composition. CRM gRPC transport/UI, provider journeys,
consent/delivery enforcement, and Finance reconciliation remain open.

### Rollback

Retain migration history after deployment; disable CRM event scheduling and
clear the disposable CRM read rows to roll back behavior without affecting
CRM write authority.

## Verification wave 2026-07-25 — G8-FIN-021 and G10-MKT-021

- Finance source outbox disposable test — PASS.
- CRM read projection disposable test — PASS with two physically distinct
  tenant databases.
- Migration assertions — 6 tests PASS.
- Infrastructure Clippy/checks and worker checks — PASS.

The program remains active. Transport/UI, remaining ERP contexts, cutover,
hardening, all-target mobile evidence, and Folio write retirement are not
complete.

PASS: the post-audit G14 local protobuf compatibility suite passed 11/11,
including generated Directory/PITR round trips, reserved-field and duplicate
declaration rejection, wire-stability checks, and fail-closed handling when
the approved descriptor baseline is absent. The missing approved descriptor
remains queued rather than inferred from the generated tree.

## Verification wave 2026-07-25 — full workspace after G8-FIN-021/G10-MKT-021

- `cargo fmt --all --check` — PASS.
- `cargo clippy --workspace --all-targets -- -D warnings` — PASS.
- `cargo test --workspace` — PASS; 153 domain tests and all active workspace
  suites passed; database-dependent suites remain explicitly ignored without
  disposable URLs.
- `cargo check -p erpx-console --no-default-features --features web` — PASS.
- `cargo check -p erpx-console --no-default-features --features desktop` — PASS.
- `bun run lint` — PASS with four pre-existing warnings in
  `infra/scripts/start-n8n.js`, zero errors.
- `bunx tsc --noEmit` — PASS.

Runtime remains healthy: Folio is still on port 3004, PostgreSQL is ready,
and disposable packet databases were removed. This is not a gate-completion
claim.

## Entry 2026-07-25 — G10-MKT-022 Marketing/CRM worker scheduler composition

**Owner:** main agent  
**Dependencies:** G10-MKT-021 Marketing/CRM read projection adapter and the
existing tenant projection scheduler/Operations/Finance composition  
**Allowed paths:** worker Marketing projection, worker composition root,
focused worker tests, and audit  
**Forbidden paths:** infrastructure, migrations, domain, proto, server,
console/UI, Finance, Folio, and environment/runtime state

### Delivered behavior

- Registered Marketing/CRM projection execution through the existing
  `tenant_projection_scheduler` handler and active-tenant claim/lease/retry/
  dead-letter path.
- Added `ERPX_TENANT_PROJECTION_KIND=marketing` and bounded
  `ERPX_MARKETING_PROJECTION_PAGE_SIZE` selection in the worker executable.
- Kept routing in the existing `TenantDatabaseRouter` via
  `MarketingProjectionRunner<SqlxMarketingProjectionFactory>`; no credential
  or database-routing path was added.
- Supported bounded incremental pages and in-place disposable read-model
  rebuilds. Marketing event-type validation remains in the existing
  infrastructure adapter.

### Evidence

- `cargo fmt --all --check` — PASS.
- `cargo check -p erpx-worker` — PASS.
- `cargo test -p erpx-worker marketing_projection -- --nocapture` — 6 PASS.
- `cargo clippy -p erpx-worker --all-targets -- -D warnings` — PASS.
- Audit: `audits/G10-MKT-022/README.md`.

### Limitations and remaining boundary

Marketing does not yet expose a source high-watermark or replacement-read
candidate activation interface. Therefore this packet intentionally does not
use the Operations/Finance replacement-read wrapper: rebuilds are in-place
clear/replays of the disposable Marketing/CRM read model and reject source
fence/candidate payloads. Source-fenced replacement activation, CRM gRPC/UI,
provider journeys, consent/delivery, Finance attribution reconciliation, and
the broader ERPX gates remain open. This packet does not close G10 or the
persistent ERPX goal.

## Verification wave 2026-07-25 — full workspace after G10-MKT-022

- `cargo fmt --all --check` — PASS.
- `cargo clippy --workspace --all-targets -- -D warnings` — PASS.
- `cargo test --workspace` — PASS; all active workspace suites passed, with
  database-dependent suites remaining explicitly ignored without disposable
  URLs.
- `cargo check -p erpx-console --no-default-features --features web` — PASS.
- `cargo check -p erpx-console --no-default-features --features desktop` — PASS.
- `bun run lint` — PASS with four pre-existing warnings in
  `infra/scripts/start-n8n.js`, zero errors.
- `bunx tsc --noEmit` — PASS.

Runtime remains healthy: Folio is listening on port 3004, PostgreSQL is ready,
no disposable `erpx_%` databases remain, and `git diff --check` is clean. The
program remains active; CRM transport/UI, provider journeys, consent/delivery,
Finance reconciliation, remaining ERP contexts, migration/cutover, hardening,
all-target mobile evidence, and Folio write retirement remain open.

## Entry 2026-07-25 — G10-MKT-023 CRM read transport

**Owner:** main agent  
**Dependencies:** G10-MKT-021 CRM read projection and G10-MKT-022 bounded
Marketing projection scheduler  
**Shared ownership:** protobuf compatibility, server authorization, and
tenant read routing remained with the main agent

### Delivered behavior

- Added the versioned `CrmService.ListCrmReadRows` Tonic/gRPC-Web contract for
  accepted handoffs, opportunities, and attribution projections.
- Added `SqlxMarketingStore::read_crm_page` with deterministic UUID ordering,
  opaque continuation, a maximum page size of 100, and holding/company/branch
  SQL predicates.
- Attribution rows join the projected opportunity to recover company/branch
  scope before they can be returned.
- The server requires authenticated query context, explicit non-composite
  scope, the `crm.read` role, an active customer scope binding, and routed
  tenant read access. Errors remain redacted.

### Evidence

- Infrastructure and server checks, focused tests, migration tests, and
  Clippy passed; details are recorded in `audits/G10-MKT-023/README.md`.
- Disposable CRM projection test passed against physically separate tenant
  write/read databases, covering opportunity and attribution reads, a
  wrong-company empty result, replay, forced RLS, and rebuild.

### Remaining boundary and rollback

CRM writes/richer typed transport, CRM UI, provider journeys, consent and
delivery enforcement, Finance attribution reconciliation, and all broader
ERPX gates remain open. Removing the additive service registration and handler
rolls back this packet without changing tenant write state or Folio.

## Verification wave 2026-07-25 — G10-MKT-023 focused integration

- `cargo fmt --all --check` — PASS.
- `cargo check -p erpx-infrastructure` — PASS.
- `cargo check -p erpx-server` — PASS.
- `cargo clippy -p erpx-infrastructure -p erpx-server --all-targets -- -D warnings` — PASS.
- `cargo test -p erpx-infrastructure marketing_persistence::tests -- --nocapture` — 5 PASS.
- `cargo test -p erpx-server -- --nocapture` — 63 PASS.
- `cargo test -p erpx-migration -- --nocapture` — 6 PASS.
- Disposable `marketing_crm_read_projection` test — 1 PASS with separate
  tenant write/read databases; databases were dropped afterward.

The full workspace wave remains required after the next integration boundary.
The persistent ERPX goal remains active.

## Append-only integration record — G6-PRV-029 tenant-bound backup artifact retention

The retention worker now requires a configured PostgreSQL/MinIO adapter for
non-dry-run actions, selects claim-bound artifact locators, deletes them with
tenant/namespace/chain/reference binding and 404-idempotency, and completes
metadata only after object cleanup. Production backup and restore targets now
carry the registry-owned storage namespace. Missing adapters and object
failures fail closed or retry without exposing provider details.

Evidence: `audits/G6-PRV-029/README.md`.

Credentialed live MinIO artifact proof, PITR/WAL recovery, backup
reconciliation, owner seeding, remaining Platform Operations actions, and the
G6/G14 gates remain open. The persistent ERPX goal remains active.

## Verification wave 2026-07-26 — G6-PRV-029 full integration

- `cargo fmt --all --check` — PASS.
- `cargo clippy --workspace --all-targets -- -D warnings` — PASS.
- `cargo test --workspace` — PASS; disposable PostgreSQL tests remain
  explicitly ignored without their required databases.
- Dioxus console web/desktop checks — PASS.
- `bun run lint` — PASS with four pre-existing n8n warnings, zero errors.
- `bunx tsc --noEmit` — PASS.
- PostgreSQL readiness — accepting connections; Folio `/operations` returned
  HTTP 307.
- `git diff --check` — PASS.
- Fresh disposable two-control-database retention proof — PASS; exact cleanup
  removed both temporary databases.

Folio remains operational. The persistent ERPX goal remains active.

## Verification wave 2026-07-26 — G7-004 full integration

- `cargo fmt --all --check` — PASS.
- `cargo clippy --workspace --all-targets -- -D warnings` — PASS.
- `cargo test --workspace` — PASS; database-dependent suites remain explicitly
  ignored without their required disposable URLs.
- `cargo check -p erpx-console --no-default-features --features web` — PASS.
- `cargo check -p erpx-console --no-default-features --features desktop` — PASS.
- `bun run lint` — PASS with the four known n8n baseline warnings, zero
  errors.
- `bunx tsc --noEmit` — PASS.
- `git diff --check` — PASS.
- Runtime checks — PASS: PostgreSQL accepts connections, Folio
  `/operations` responds with `307`, MinIO health responds with `200`, and
  the port-3004 Folio development server remains active.

Folio remains operational. The persistent ERPX goal remains active.

## Append-only integration record — G7-004 Operations exception transport

The main agent independently integrated the authenticated exception lifecycle
command boundary. The new RPC constructs assign/reassign/acknowledge/escalate/
resolve commands only after control-plane role-chain resolution for all
principal validity proofs, then invokes the existing atomic tenant write
transaction. Scope, actor, optimistic-version, idempotency, SLA, and safe
transition metadata are preserved; source-context records remain untouched.

Evidence: `audits/G7-004-TRANSPORT/README.md`.

This packet does not claim SLA scheduling, detection producers, expired-user
reassignment, manager hierarchy, Operations UI, Folio reconciliation, or
G7/G14 completion. The persistent ERPX goal remains active.

## Verification wave 2026-07-26 — G7-004 focused integration

- `cargo fmt --all --check` — PASS.
- `cargo clippy -p erpx-server --all-targets -- -D warnings` — PASS.
- `cargo test -p erpx-server --all-targets` — PASS; database-dependent owner
  seeding remains explicitly ignored without its required URLs.
- Disposable Operations lifecycle persistence test — PASS.

Folio remains operational. The persistent ERPX goal remains active.

## Verification wave 2026-07-26 — G7-002 full integration

- `cargo fmt --all --check` — PASS.
- `cargo clippy --workspace --all-targets -- -D warnings` — PASS.
- `cargo test --workspace` — PASS; database-dependent suites remain explicitly
  ignored without their required disposable URLs.
- `cargo check -p erpx-console --no-default-features --features web` — PASS.
- `cargo check -p erpx-console --no-default-features --features desktop` — PASS.
- `bun run lint` — PASS with the four known n8n baseline warnings, zero
  errors.
- `bunx tsc --noEmit` — PASS.
- `git diff --check` — PASS.
- Runtime checks — PASS: PostgreSQL accepts connections, Folio
  `/operations` responds with `307`, MinIO health responds with `200`, and
  the port-3004 Folio development server remains active.

Folio remains operational. The persistent ERPX goal remains active.

## Append-only integration record — G7-002 Operations query boundary

The main agent independently reviewed and integrated the tenant-scoped
Operations query boundary. `GetOperationsSummary` reports separate physical
write/read reconciliation counts, and `ListOperationsExceptions` exposes a
bounded active queue with deterministic opaque pagination. Both handlers
require authenticated `operations.read` access, contained holding scope,
active tenant routing, and the opaque registry route; safe error mapping and
correlation IDs are preserved.

Evidence: `audits/G7-002/README.md`.

This packet does not claim Folio/ERPX KPI reconciliation, source-context
adapters, tenant Dioxus Operations pages, complete exception lifecycle
processing, or G7/G14 completion. The persistent ERPX goal remains active.

## Verification wave 2026-07-26 — G7-002 focused integration

- `cargo fmt --all --check` — PASS.
- `cargo clippy -p erpx-infrastructure --all-targets -- -D warnings` — PASS.
- `cargo clippy -p erpx-server --all-targets -- -D warnings` — PASS.
- `cargo test -p erpx-infrastructure --test operations_persistence` — PASS;
  database tests remain explicitly ignored without URLs.
- Disposable two-database Operations boundary test — PASS.

Folio remains operational. The persistent ERPX goal remains active.

## Verification wave 2026-07-26 — G7-001 full integration

- `cargo fmt --all --check` — PASS.
- `cargo clippy --workspace --all-targets -- -D warnings` — PASS.
- `cargo test --workspace` — PASS; active suites and doc-tests passed, with
  database-dependent suites explicitly ignored without disposable URLs.
- `cargo check -p erpx-console --no-default-features --features web` — PASS.
- `cargo check -p erpx-console --no-default-features --features desktop` —
  PASS.
- `bun run lint` — PASS with four pre-existing warnings in
  `infra/scripts/start-n8n.js`; zero errors.
- `bunx tsc --noEmit` — PASS.
- `git diff --check` — PASS.
- Runtime checks — PASS: PostgreSQL accepts connections, Folio
  `/operations` responds with `307`, and MinIO health responds `200`.

Folio remains operational. The persistent ERPX goal remains active.

## Append-only integration record — G7-001 organization hierarchy boundary

Main independently reviewed and integrated tenant-local organization command
and read transport for Holding, Company, Headquarters, Branch, Department,
and Team. The adapter is authenticated, role-gated, tenant-route-bound, scope
contained, idempotent, optimistic-versioned, and backed by Operations write
events plus a separate rebuildable read projection. Reporting lines and
delegated administration remain open.

Evidence: `audits/G7-001/README.md`.

## Verification wave 2026-07-26 — G7-001 focused integration

- `cargo fmt --all` — PASS.
- `cargo check -p erpx-infrastructure` — PASS.
- `cargo check -p erpx-server` — PASS.
- `cargo clippy -p erpx-infrastructure --all-targets -- -D warnings` — PASS.
- `cargo test -p erpx-infrastructure --test organization` — PASS; one unit
  test passed and the database test remained ignored without URLs.
- Disposable two-database organization proof — PASS; six hierarchy nodes,
  idempotent replay, projection reads, and cross-holding fail-closed behavior
  were verified. Temporary databases were removed.

Folio remains operational. The persistent ERPX goal remains active.

## Verification wave 2026-07-26 — G6-PRV-033 full integration

- `cargo fmt --all --check` — PASS.
- `cargo clippy --workspace --all-targets -- -D warnings` — PASS.
- `cargo test --workspace` — PASS; active suites passed and database-dependent
  suites remain explicitly ignored without their required disposable URLs.
- `cargo check -p erpx-console --no-default-features --features web` — PASS.
- `cargo check -p erpx-console --no-default-features --features desktop` —
  PASS.
- `bun run lint` — PASS with four pre-existing warnings in
  `infra/scripts/start-n8n.js`; zero errors.
- `bunx tsc --noEmit` — PASS.
- `git diff --check` — PASS.
- Runtime checks — PASS: PostgreSQL accepts connections, Folio
  `/operations` responds with `307`, and MinIO health responds `200`.

Folio remains operational. The persistent ERPX goal remains active.

## Append-only integration record — G6-PRV-033 activation-time organization root

Main independently reviewed and integrated activation-time organization
bootstrap for provisioned holdings. Tenant provisioning now requires the
owner seed before physical initialization, then creates a deterministic
tenant-local `organization.holding` write snapshot/event and the matching
Operations read row/checkpoint. The write event remains authoritative and
the read row is rebuildable; this is not a long-term dual-write path.

Evidence: `audits/G6-PRV-033/README.md`.

The persistent ERPX goal remains active.

## Verification wave 2026-07-26 — G6-PRV-033 focused integration

- `cargo fmt --all` — PASS.
- `cargo check -p erpx-infrastructure` — PASS.
- `cargo check -p erpx-server` — PASS.
- `cargo clippy -p erpx-infrastructure --test provisioning -- -D warnings` —
  PASS.
- Disposable four-database proof
  `tenant_organization_bootstrap_is_write_authoritative_and_read_projected` —
  PASS; replay was stable, write counts were `(snapshot=1,event=1)`, read
  counts were `(row=1,checkpoint=1)`, and temporary databases were removed.

Folio remains operational. The persistent ERPX goal remains active.

## Append-only integration record — G6-G12-UI-013 authenticated Platform audit read surface

Main independently reviewed and integrated the control-plane audit read
boundary and Dioxus `/platform/audit` adapter. The repository reads only
allow-listed audit columns through a bounded stable cursor. The server
requires authenticated Platform scope plus `platform.tenant.read`, audits
the read itself, and returns no payload JSON or customer identifier. Web and
native clients use the same safe model and explicit non-ready/read-only states.

Evidence: `audits/G6-G12-UI-013/README.md`.

Remaining integrations/settings routes, mobile target builds, and the
G6/G12/G14 gates remain open. The persistent ERPX goal remains active.

## Verification wave 2026-07-26 — G6-G12-UI-013 focused integration

- `cargo fmt --all --check` — PASS after formatting.
- `cargo check -p erpx-server` — PASS.
- `cargo test -p erpx-console` — PASS; 21 tests.
- `cargo check -p erpx-console --no-default-features --features web` — PASS.
- `cargo check -p erpx-console --no-default-features --features desktop` — PASS.

Folio remains operational. The persistent ERPX goal remains active.

## Verification wave 2026-07-26 — G6-G12-UI-013 full integration

- `cargo fmt --all --check` — PASS.
- `cargo clippy --workspace --all-targets -- -D warnings` — PASS.
- `cargo test --workspace` — PASS; database-dependent suites remain
  explicitly ignored without their required disposable databases.
- `cargo check -p erpx-console --no-default-features --features web` — PASS.
- `cargo check -p erpx-console --no-default-features --features desktop` — PASS.
- `bun run lint` — PASS with four pre-existing warnings in
  `infra/scripts/start-n8n.js`, zero errors.
- `bunx tsc --noEmit` — PASS.
- PostgreSQL readiness — accepting connections PASS.
- MinIO `/minio/health/live` on the host-native configured port — HTTP 200 PASS.
- Folio `/operations` — HTTP 307 PASS; development server remains on port
  3004.
- `git diff --check` — PASS.

Credentialed MinIO object operations, PostgreSQL PITR/WAL recovery, and the
remaining Platform Operations routes remain open; no completion claim is
made for those requirements.

Folio remains operational. The persistent ERPX goal remains active.

## Append-only integration record — G6-PRV-030 retention claim renewal

Main independently reviewed and integrated database-clock lease renewal for
physical backup retention cleanup. The worker renews the exact tenant/holding,
claim, worker, and active lease immediately before each bounded object-delete
request. Claim loss stops further deletion and uses the existing redacted
retry/terminal path. Physical storage authority remains in the tenant-bound
backup adapter.

Evidence: `audits/G6-PRV-030/README.md`.

Credentialed MinIO proof, object inventory reconciliation, PostgreSQL PITR/WAL
recovery, owner seeding, remaining Platform Operations actions, and the
G6/G14 gates remain open. The persistent ERPX goal remains active.

## Verification wave 2026-07-26 — G6-PRV-030 focused integration

- `cargo fmt --all` — PASS.
- `cargo test -p erpx-infrastructure --lib backup_retention::tests` — PASS;
  4 tests.
- `cargo test -p erpx-worker backup_retention::tests` — PASS; 1 test.
- Fresh disposable two-control-database retention SQL flow with claim renewal
  — PASS; 1 test; temporary databases were removed by cleanup.

Folio remains operational. The persistent ERPX goal remains active.

## Verification wave 2026-07-26 — G6-PRV-030 full integration

- `cargo fmt --all --check` — PASS.
- `cargo clippy --workspace --all-targets -- -D warnings` — PASS.
- `cargo test --workspace` — PASS; database-dependent suites remain
  explicitly ignored without their required disposable databases.
- `cargo check -p erpx-console --no-default-features --features web` — PASS.
- `cargo check -p erpx-console --no-default-features --features desktop` — PASS.
- `bun run lint` — PASS with four pre-existing warnings in
  `infra/scripts/start-n8n.js`, zero errors.
- `bunx tsc --noEmit` — PASS.
- PostgreSQL readiness — accepting connections PASS.
- MinIO `/minio/health/live` — HTTP 200 PASS.
- Folio `/operations` — HTTP 307 PASS; development server remains on port
  3004.
- `git diff --check` — PASS.

Credentialed MinIO artifact operations, object inventory reconciliation,
PostgreSQL PITR/WAL recovery, owner seeding, remaining Platform Operations
actions, and the G6/G14 gates remain open.

Folio remains operational. The persistent ERPX goal remains active.

## Append-only integration record — G6-G12-UI-014 Platform integrations and settings

Main independently reviewed and integrated authenticated, audited reads for
`/platform/integrations` and `/platform/settings`. Server responses contain
only categorical integration health/capability labels or architecture
metadata; the Dioxus models exclude endpoints, credentials, tokens, database
keys, provider payloads, and mutation controls.

Evidence: `audits/G6-G12-UI-014/README.md`.

Provider onboarding/live proof, mobile targets, owner seeding, true PITR/WAL,
and the remaining G6/G12/G14 requirements remain open. The persistent ERPX
goal remains active.

## Verification wave 2026-07-26 — G6-G12-UI-014 focused integration

- `cargo fmt --all` — PASS.
- `cargo check -p erpx-server` — PASS.
- `cargo test -p erpx-console` — PASS; 22 tests.
- `cargo check -p erpx-console --no-default-features --features web` — PASS.
- `cargo check -p erpx-console --no-default-features --features desktop` — PASS.

Folio remains operational. The persistent ERPX goal remains active.

## Verification wave 2026-07-26 — G6-G12-UI-014 full integration

- `cargo fmt --all --check` — PASS.
- `cargo clippy --workspace --all-targets -- -D warnings` — PASS.
- `cargo test --workspace` — PASS; active workspace suites and doc tests
  passed. Database-dependent suites remain explicitly ignored unless their
  required disposable PostgreSQL URLs are supplied; the focused two-database
  G6-PRV-030 flow passed independently.
- `cargo check -p erpx-console --no-default-features --features web` — PASS.
- `cargo check -p erpx-console --no-default-features --features desktop` — PASS.
- `bun run lint` — PASS with four pre-existing warnings in
  `infra/scripts/start-n8n.js`, zero errors.
- `bunx tsc --noEmit` — PASS.
- `git diff --check` — PASS.
- Runtime checks — PASS: PostgreSQL accepts connections, Folio `/operations`
  responds with its expected `307` redirect, and MinIO health responds `200`.

Credentialed MinIO object operations, provider onboarding, mobile target
builds, owner seeding, true PITR/WAL recovery, and the remaining G6/G12/G14
requirements remain open. Folio remains operational. The persistent ERPX goal
remains active.

## Append-only integration record — G6-PRV-031 backup inventory reconciliation

Main independently reviewed and integrated the bounded S3-compatible inventory
listing, tenant/backup-chain prefix binding, truncated-page rejection, adapter
normalization, retention-worker comparison, redacted control-plane report, and
separate read projection. Provider inventory failure is retryable and cannot
complete metadata retention from partial evidence.

Evidence: `audits/G6-PRV-031/README.md`.

Credentialed live MinIO proof, true PITR/WAL recovery, owner seeding, provider
onboarding, mobile targets, and the remaining G6/G12/G14 requirements remain
open. The persistent ERPX goal remains active.

## Verification wave 2026-07-26 — G6-PRV-031 focused integration

- `cargo fmt --all` — PASS.
- `cargo clippy -p erpx-infrastructure --all-targets -- -D warnings` — PASS.
- `cargo clippy -p erpx-worker --all-targets -- -D warnings` — PASS.
- `cargo test -p erpx-infrastructure --lib object_storage::tests` — PASS; 5 tests.
- `cargo test -p erpx-infrastructure --lib backup_retention::tests` — PASS; 4 tests.
- Fresh disposable two-database SQL retention flow — PASS; migration 0056/0009,
  inventory evidence and read projection verified; temporary databases were
  removed by cleanup.

Folio remains operational. The persistent ERPX goal remains active.

## Verification wave 2026-07-26 — G6-PRV-031 full integration

- `cargo fmt --all --check` — PASS.
- `cargo clippy --workspace --all-targets -- -D warnings` — PASS.
- `cargo test --workspace` — PASS; active workspace suites and doc tests
  passed. Database-dependent suites remain explicitly ignored without their
  required disposable URLs.
- `cargo check -p erpx-console --no-default-features --features web` — PASS.
- `cargo check -p erpx-console --no-default-features --features desktop` — PASS.
- `bun run lint` — PASS with four pre-existing warnings in
  `infra/scripts/start-n8n.js`, zero errors.
- `bunx tsc --noEmit` — PASS.
- `git diff --check` — PASS.
- Runtime checks — PASS: PostgreSQL accepts connections, Folio `/operations`
  responds with `307`, and MinIO health responds `200`.

Credentialed live MinIO inventory, provider onboarding, mobile target builds,
owner seeding, true PITR/WAL recovery, and the remaining G6/G12/G14
requirements remain open. Folio remains operational. The persistent ERPX goal
remains active.

## Append-only integration record — G6-G12-UI-015 mobile target compilation

Main independently verified the shared Dioxus mobile feature for
`aarch64-apple-ios` and `aarch64-linux-android` and added the same matrix to
`.github/workflows/erpx-targets.yml`. This is compilation evidence only;
device execution, secure keystore/keychain persistence, camera/scanner,
notifications, offline drafts, and mobile role journeys remain open.

Evidence: `audits/G6-G12-UI-015/README.md`.

The persistent ERPX goal remains active.

## Verification wave 2026-07-26 — G6-G12-UI-015 focused integration

- Host mobile feature check — PASS.
- `cargo check -p erpx-console --target aarch64-apple-ios --no-default-features --features mobile` — PASS.
- `cargo check -p erpx-console --target aarch64-linux-android --no-default-features --features mobile` — PASS.
- Workflow syntax reviewed: both target checks use the pinned Rust 1.91.1
  toolchain and separate macOS/Ubuntu runners.

Folio remains operational. The persistent ERPX goal remains active.

## Verification wave 2026-07-26 — G6-G12-UI-015 full integration

- `cargo fmt --all --check` — PASS.
- `cargo clippy --workspace --all-targets -- -D warnings` — PASS.
- `cargo test --workspace` — PASS; active workspace suites and doc tests
  passed; database-dependent suites remain explicitly ignored without their
  required disposable URLs.
- Web and desktop Dioxus checks — PASS.
- Host mobile feature check — PASS.
- iOS target check (`aarch64-apple-ios`) — PASS.
- Android target check (`aarch64-linux-android`) — PASS.
- `bun run lint` — PASS with four pre-existing warnings in
  `infra/scripts/start-n8n.js`, zero errors.
- `bunx tsc --noEmit` — PASS.
- Workflow YAML parse and `git diff --check` — PASS.
- Runtime checks — PASS: PostgreSQL accepts connections, Folio `/operations`
  responds with `307`, and MinIO health responds `200`.

Device execution, OS-secure keystore/keychain persistence, camera/scanner,
notifications, offline drafts, provider onboarding, owner seeding, true
PITR/WAL recovery, and the remaining G6/G12/G14 requirements remain open.
Folio remains operational. The persistent ERPX goal remains active.

## Verification wave 2026-07-25 — full workspace after G10-MKT-023

- `cargo fmt --all --check` — PASS.
- `cargo clippy --workspace --all-targets -- -D warnings` — PASS.
- `cargo test --workspace` — PASS; all active workspace suites and doc tests
  passed, with database-dependent suites remaining explicitly ignored without
  disposable URLs.
- `cargo check -p erpx-console --no-default-features --features web` — PASS.
- `cargo check -p erpx-console --no-default-features --features desktop` — PASS.
- `bun run lint` — PASS with four pre-existing warnings in
  `infra/scripts/start-n8n.js`, zero errors.
- `bunx tsc --noEmit` — PASS.

The focused disposable CRM databases were removed after their passing test;
Folio and PostgreSQL remain healthy. This is an integration-wave gate record,
not a claim that any ERPX milestone or the persistent goal is complete.

## Amendment 2026-07-25 — G8-FIN-022 main-agent integration review

The main agent independently reviewed the control-plane migration, exact
resolver query, active-tenant/holding binding, malformed-reference validation,
and diagnostic redaction. The live two-control-database test passed, the DTO
was exported from `erpx-infrastructure`, and no provider secret material or
tenant credential was added to the database schema. Provider SDK and
secret-manager retrieval remain intentionally outside this packet.

## Entry 2026-07-25 — G8-FIN-023 bank-match proposal/decision transport

**Owner:** main agent  
**Dependencies:** G8-FIN-016 authoritative bank-match persistence and
G8-FIN-022 control-plane provider routing  
**Shared ownership:** Finance protobuf compatibility and server authorization
remained with the main agent

Added additive Finance RPCs `SaveBankMatchProposal` and
`SaveBankMatchDecision`. Both use bounded typed JSON envelopes for the existing
validated domain records, require explicit holding/company/branch scope,
authenticated actor equality, active customer scope binding, `finance.write`
authorization, Finance entitlement, and routed tenant writes. The existing
SQLx boundary reloads authoritative bank and posted-journal facts, applies
idempotency and proposal binding atomically, and never posts or mutates a
journal.

Evidence: `audits/G8-FIN-023/README.md`; 64 server tests, infrastructure
provider-route unit tests, migration assertion, server/infrastructure Clippy,
and the full workspace wave all passed. The persistent ERPX goal remains
active; provider SDKs, automatic proposal generation, journal application,
Finance UI and remaining G8 requirements remain open.

## Verification wave 2026-07-25 — full workspace after G8-FIN-022/G8-FIN-023

- `cargo fmt --all --check` — PASS.
- `cargo clippy --workspace --all-targets -- -D warnings` — PASS.
- `cargo test --workspace` — PASS; all active workspace suites and doc tests
  passed, with database-dependent suites remaining explicitly ignored without
  disposable URLs.
- `cargo check -p erpx-console --no-default-features --features web` — PASS.
- `cargo check -p erpx-console --no-default-features --features desktop` — PASS.
- `bun run lint` — PASS with four pre-existing warnings in
  `infra/scripts/start-n8n.js`, zero errors.
- `bunx tsc --noEmit` — PASS.

The control-plane and tenant disposable databases used by these packets were
removed after their passing tests. Folio remains operational on port 3004 and
the ERPX program remains active.

## Entry 2026-07-25 — G8-FIN-024 bounded bank-provider import coordinator

**Owner:** AG-FIN  
**Dependencies:** G8-FIN-013 typed provider-neutral adapter and
`BankProviderDomainRecords`; G8-FIN-022 exact control-plane routing remains a
future composition dependency  
**Allowed paths:** additive provider facts mapper, worker coordinator/tests,
worker export, audit, and plan/ledger evidence  
**Forbidden paths:** SQLx persistence/migrations, server/protobuf, scheduler
registration, credentials, provider SDKs, UI, Marketing, Finance close, Folio,
and runtime environment state

### Delivered behavior

- Added `BankProviderImportCoordinator::import_statement`, which consumes an
  injected `BankProviderAdapter` and returns the existing redacted
  `BankProviderError` or `BankProviderDomainRecords`.
- Rebuilt every follow-up request from the original typed account/date/
  currency/page-size binding and the validated opaque cursor.
- Enforced 256 pages and 100,000 transactions per import, global strict
  transaction-reference ordering, statement identity, account binding,
  cursor uniqueness, adjacent page balance continuity, and final-page
  metadata invariants.
- Preserved the first-page opening balance and final-page closing balance and
  mapped only complete typed facts to the existing domain import.
- Added the minimal infrastructure complete-facts mapper needed because a
  complete statement can exceed one provider page.

### Evidence

- `cargo fmt --all --check` — PASS.
- `cargo check -p erpx-worker` — PASS.
- `cargo clippy -p erpx-worker --all-targets -- -D warnings` — PASS.
- `cargo test -p erpx-worker finance_banking_provider -- --nocapture` — PASS:
  5 tests.
- No database was created and no persistence or command owner changed.

Audit: `audits/G8-FIN-024/README.md`.

### Security and remaining boundary

Only typed facts and opaque validated cursors cross the coordinator. Provider
payloads, credentials, tokens, URLs, and payment instruments have no
representation or logging path. Provider SDKs, secret-manager retrieval,
authorized worker composition, tenant import persistence, bank-to-GL
generation/application, reconciliation, Finance UI, and the broader ERPX
gates remain open. This packet does not close G8 or the persistent goal.

### Rollback

Remove the additive worker module/export and complete-facts mapper. Existing
provider page validation, control-plane routing, tenant persistence, and Folio
remain unchanged.

## Entry 2026-07-25 — G10-MKT-024 CRM command transport

Added additive Tonic/gRPC-Web commands for CRM handoff, opportunity lifecycle,
and attribution writes. The server validates command/scope bindings, requires
the authenticated `crm.write` role and entitlement, routes only to the tenant
write database, and leaves idempotency, outbox, optimistic transitions, and
attribution recomputation to the existing SQLx boundary. Bounded JSON is an
interim transport representation; richer typed field-level protobufs remain
open.

Evidence: `audits/G10-MKT-024/README.md`; 64 server tests, server Clippy,
protobuf tests, and the full workspace wave passed. CRM UI, journeys, consent,
provider delivery, Finance reconciliation, and the broader ERPX gates remain
open.

## Verification wave 2026-07-25 — full workspace after G10-MKT-024

- `cargo fmt --all --check` — PASS.
- `cargo clippy --workspace --all-targets -- -D warnings` — PASS.
- `cargo test --workspace` — PASS; all active workspace suites and doc tests
  passed, with database-dependent suites remaining explicitly ignored without
  disposable URLs.
- `cargo check -p erpx-console --no-default-features --features web` — PASS.
- `cargo check -p erpx-console --no-default-features --features desktop` — PASS.
- `bun run lint` — PASS with four pre-existing warnings in
  `infra/scripts/start-n8n.js`, zero errors.
- `bunx tsc --noEmit` — PASS.

The persistent ERPX goal remains active and Folio command ownership is
unchanged.

## Entry 2026-07-25 — G8-FIN-025 fail-closed provider credential boundary

The main agent independently reviewed G8-FIN-024's bounded multi-page
coordinator and added the next non-overlapping Finance composition seam:
`BankProviderCredentialKey`, zeroizing `ResolvedBankProviderCredential`, the
async `BankProviderCredentialResolver`, and an explicit unconfigured resolver
that fails closed. Credential bytes remain outside PostgreSQL and provider DTOs;
debug/error output is redacted. The main agent also tightened the coordinator
with cross-page balance continuity validation.

Evidence: `audits/G8-FIN-025/README.md`; worker provider tests 5 PASS (including
discontinuous balance rejection), worker Clippy PASS, and infrastructure
credential tests 3 PASS with infrastructure Clippy PASS.

The deployment secret-manager adapter, provider SDKs, authorized worker import
command, bank-to-GL proposal generation, Finance UI, and all broader ERPX gates
remain open. The persistent goal remains active and Folio command ownership is
unchanged.

## Verification wave 2026-07-25 — full workspace after G8-FIN-025

- `cargo fmt --all --check` — PASS.
- `cargo clippy --workspace --all-targets -- -D warnings` — PASS.
- `cargo test --workspace` — PASS; active workspace suites and doc tests
  passed. Database-dependent suites remain explicitly ignored without
  disposable URLs.
- `cargo check -p erpx-console --no-default-features --features web` — PASS.
- `cargo check -p erpx-console --no-default-features --features desktop` — PASS.
- `bun run lint` — PASS with four pre-existing warnings in
  `infra/scripts/start-n8n.js`, zero errors.
- `bunx tsc --noEmit` — PASS.

Folio remains operational on port 3004 and PostgreSQL is ready. No command
ownership or Folio write-retirement change was made. The persistent ERPX goal
remains active; deployment secret-manager/provider adapters, authorized bank
import commands, and all remaining ERPX gates remain open.

## Entry 2026-07-25 — G8-FIN-026 authorized worker bank-provider import workflow

Added `FinanceBankProviderImportWorkflow` as an injection-only worker
composition boundary. It requires an authoritative authorizer and provider
adapter factory, validates command/account scope and provider identity,
resolves the exact active control-plane route, resolves the opaque credential
key, invokes bounded multi-page coordination, and persists through separate
routed tenant write/read pools. It returns only typed receipts and maps all
failures to redacted categories. No scheduler, transport, provider SDK, or
command-owner change was made.

Evidence: `audits/G8-FIN-026/README.md`; focused worker tests 2 PASS and worker
Clippy PASS. The production secret-manager/provider adapters, authorized
transport/scheduler registration, disposable database workflow evidence,
bank-to-GL generation, and remaining Finance requirements remain open.

## Verification wave 2026-07-25 — full workspace after G8-FIN-026

- `cargo fmt --all --check` — PASS.
- `cargo clippy --workspace --all-targets -- -D warnings` — PASS.
- `cargo test --workspace` — PASS; active workspace suites and doc tests
  passed. Database-dependent suites remain explicitly ignored without
  disposable URLs.
- `cargo check -p erpx-console --no-default-features --features web` — PASS.
- `cargo check -p erpx-console --no-default-features --features desktop` — PASS.
- `bun run lint` — PASS with four pre-existing warnings in
  `infra/scripts/start-n8n.js`, zero errors.
- `bunx tsc --noEmit` — PASS.

Folio remains operational on port 3004 and PostgreSQL is ready. No command
ownership or Folio write-retirement change was made. The persistent ERPX goal
remains active; production provider/secret-manager adapters, database-backed
import evidence, and all remaining ERPX gates remain open.

## Entry 2026-07-25 — G10-MKT-026 shared CRM display models

The main agent independently reviewed and exported the new target-neutral CRM
UI module. It provides safe handoff, opportunity, and attribution display
models; bilingual lifecycle labels; bounded page/cursor metadata; deterministic
sorting; freshness/read-only metadata; and explicit loading, empty, denied,
stale, read-only, conflict, offline, and error states. Sensitive and
unsupported display candidates are rejected and counted. No transport,
authorization, tenant data, or Folio behavior changed.

Evidence: `audits/G10-MKT-026/README.md`; 9 focused UI tests, UI Clippy, and
web/desktop console checks passed. CRM client transport, page rendering,
journeys, consent/delivery, Finance attribution reconciliation, and all broader
ERPX gates remain open.

## Entry 2026-07-25 — G8-FIN-027 provider adapter registry

Added and independently reviewed the worker-side `BankProviderAdapterRegistry`.
It accepts at most 16 exact restricted provider keys, dispatches only to an
explicitly injected factory, verifies returned adapter identity, and fails
closed for empty/missing providers without fallback. Debug and errors are
redacted. Main integration changed only the worker module/export; no SDK,
credential, network, scheduler, persistence, transport, command-owner, or
Folio state was changed.

Evidence: `audits/G8-FIN-027/README.md`; 5 focused registry tests and worker
Clippy passed. Real provider factories and deployment composition remain open.

## Verification wave 2026-07-25 — full workspace after G8-FIN-027

- `cargo fmt --all --check` — PASS.
- `cargo clippy --workspace --all-targets -- -D warnings` — PASS.
- `cargo test --workspace` — PASS; active workspace suites and doc tests
  passed. Database-dependent suites remain explicitly ignored without
  disposable URLs.
- `cargo check -p erpx-console --no-default-features --features web` — PASS.
- `cargo check -p erpx-console --no-default-features --features desktop` — PASS.
- `bun run lint` — PASS with four pre-existing warnings in
  `infra/scripts/start-n8n.js`, zero errors.
- `bunx tsc --noEmit` — PASS.

Folio remains operational on port 3004 and PostgreSQL is ready. No command
ownership or Folio write-retirement change was made. The persistent ERPX goal
remains active; real provider SDK factories, deployment secret-manager
composition, and all remaining ERPX gates remain open.

## Verification wave 2026-07-25 — full workspace after G10-MKT-026

- `cargo fmt --all --check` — PASS.
- `cargo clippy --workspace --all-targets -- -D warnings` — PASS.
- `cargo test --workspace` — PASS; active workspace suites and doc tests
  passed. Database-dependent suites remain explicitly ignored without
  disposable URLs.
- `cargo check -p erpx-console --no-default-features --features web` — PASS.
- `cargo check -p erpx-console --no-default-features --features desktop` — PASS.
- `bun run lint` — PASS with four pre-existing warnings in
  `infra/scripts/start-n8n.js`, zero errors.
- `bunx tsc --noEmit` — PASS.

Folio remains operational on port 3004 and PostgreSQL is ready. No command
ownership or Folio write-retirement change was made. The persistent ERPX goal
remains active; CRM client transport/page rendering and all remaining ERPX
gates remain open.

## Entry 2026-07-25 — G11-PEO-001 HR and payroll domain foundation

The main agent added the pure HR/payroll domain module and independently
verified its scope-bound employee lifecycle, leave approvals, integer-unit
expense claims, payroll total reconciliation, distinct preparer/reviewer/
poster controls, and expired-role workflow reassignment. The packet has no
persistence, transport, provider, migration, or Folio effects.

Evidence: `audits/G11-PEO-001/README.md`; focused HR tests 3 PASS, domain
Clippy PASS, and formatting PASS. Authoritative RBAC/SoD integration, payroll
calculation/provider integration, read projections, and the broader G11 gate
remain open.

## Entry 2026-07-25 — G11-LEGAL-001 Legal and document domain foundation

The main agent added and independently verified pure Legal/document contracts
for scoped contract parties, review/approval/retention lifecycle, versioned
opaque document metadata, expiring download authorization metadata, and
immutable access history. Raw document bytes, URLs, secrets, tokens, and
storage credentials are rejected at the domain boundary. No persistence,
transport, MinIO, migration, or Folio behavior changed.

Evidence: `audits/G11-LEGAL-001/README.md`; focused Legal tests 4 PASS, domain
Clippy PASS, and formatting PASS. Retention jobs, authorized storage access,
transport, read projections, and the broader G11 gate remain open.

## Integration review 2026-07-25 — G8-FIN-029 and G10-MKT-027 sidecars

The main agent independently reviewed both sidecars before integration.
G8-FIN-029's infrastructure adapter returns only bounded close metadata,
domain-derived report summaries, and explicit missing/stale/unknown freshness;
it does not enforce server authorization and therefore remains a composition
seam. G10-MKT-027's console slice uses existing CRM protobuf reads, rejects
platform/subtree or absent tenant scope, renders only allow-listed fields, and
keeps mutations unavailable. No migrations, command-owner changes, or Folio
write-retirement changes were introduced.

Evidence: `audits/G8-FIN-029/README.md` and `audits/G10-MKT-027/README.md`;
independent infrastructure focused tests 5 PASS and Clippy PASS; independent
console tests 18 PASS, Clippy PASS, and web/desktop checks PASS. Dedicated
close read projection/worker, authenticated close read transport, CRM scope
provider/freshness/cursor continuation, and broader G8/G10/G12 gates remain
open.

## Verification wave 2026-07-25 — full workspace after G8-FIN-029/G10-MKT-027/G11 domain packets

- `cargo fmt --all --check` — PASS.
- `cargo clippy --workspace --all-targets -- -D warnings` — PASS.
- `cargo test --workspace` — PASS; active workspace suites and doc tests
  passed. Database-dependent suites remain explicitly ignored without
  disposable URLs.
- `cargo check -p erpx-console --no-default-features --features web` — PASS.
- `cargo check -p erpx-console --no-default-features --features desktop` — PASS.
- `bun run lint` — PASS with four pre-existing warnings in
  `infra/scripts/start-n8n.js`, zero errors.
- `bunx tsc --noEmit` — PASS.

Folio remains operational on port 3004 and PostgreSQL is ready. No command
ownership or Folio write-retirement change was made. The persistent ERPX goal
remains active; dedicated close projection/transport, authoritative CRM scope
and freshness, and all other ERPX gates remain open.

## Entry 2026-07-25 — G8-FIN-028 authenticated Finance close-review command transport

The main agent added and independently reviewed the typed
`SaveFinanceCloseReview` Finance RPC. The boundary decodes bounded typed period
and review payloads, rejects broad mutation scopes, binds the review's
status-specific command context to the outer request, requires authenticated
actor equality and `finance.write` entitlement, routes only to the tenant write
database, supports optimistic expected versions, emits a typed receipt, and
records an audited save action. No worker, UI, provider, command-owner, or
Folio write-retirement behavior changed.

Evidence: `audits/G8-FIN-028/README.md`; protobuf tests PASS, server tests 64
PASS, server Clippy PASS, and formatting PASS. Database-backed close transport
evidence, close read/report/UI/workers, and all remaining G8-G14 gates remain
open.

## Verification wave 2026-07-25 — full workspace after G8-FIN-028

- `cargo fmt --all --check` — PASS.
- `cargo clippy --workspace --all-targets -- -D warnings` — PASS.
- `cargo test --workspace` — PASS; active workspace suites and doc tests
  passed. Database-dependent suites remain explicitly ignored without
  disposable URLs.
- `cargo check -p erpx-console --no-default-features --features web` — PASS.
- `cargo check -p erpx-console --no-default-features --features desktop` — PASS.
- `bun run lint` — PASS with four pre-existing warnings in
  `infra/scripts/start-n8n.js`, zero errors.
- `bunx tsc --noEmit` — PASS.

Folio remains operational on port 3004 and PostgreSQL is ready. No command
ownership or Folio write-retirement change was made. The persistent ERPX goal
remains active; close database evidence, remaining Finance modules, and all
other ERPX gates remain open.

## Entry 2026-07-25 — G8-FIN-029 bounded Finance close-review/report adapter

The main agent independently reviewed and integrated the infrastructure read
adapter. `SqlxFinanceCloseStore` now exposes a validated read-only snapshot
containing aggregate version, idempotency key, and update time without raw
JSONB. `SqlxFinanceCloseReadAdapter` composes that snapshot with the existing
tenant Finance journal read projection, preserves holding/company/period
identity, maps close status explicitly, derives deterministic statement totals
and account drill-down through the domain rules, and returns bounded control
counts. Missing close rows, missing source facts, stale projection timestamps,
and future/unknown timestamps are explicit metadata states. Errors are fixed,
payload-free categories for later server mapping.

No migration, write, transport, worker, provider, UI, or command-owner change
was made. The absence of a dedicated close-review read table remains an
intentional follow-up boundary; authenticated read transport and production
close projection storage remain open.

Evidence: `audits/G8-FIN-029/README.md`; focused infrastructure close-read
tests 5 PASS, `cargo test -p erpx-infrastructure` PASS, and infrastructure
Clippy PASS.

## Verification wave 2026-07-25 — full workspace after G8-FIN-029

- `cargo fmt --all --check` — PASS.
- `cargo clippy --workspace --all-targets -- -D warnings` — PASS.
- `cargo test --workspace` — PASS; active workspace suites and doc tests
  passed. Database-dependent suites remain explicitly ignored without
  disposable URLs.
- `cargo check -p erpx-console --no-default-features --features web` — PASS.
- `cargo check -p erpx-console --no-default-features --features desktop` — PASS.
- `bun run lint` — PASS with four pre-existing warnings in
  `infra/scripts/start-n8n.js`, zero errors.
- `bunx tsc --noEmit` — PASS.

Folio remains operational on port 3004 and PostgreSQL is ready. No command
ownership or Folio write-retirement change was made. The persistent ERPX goal
remains active; authenticated close read transport, dedicated close read
projection storage, remaining Finance requirements, and all other ERPX gates
remain open.

## Entry 2026-07-25 — G10-MKT-027 CRM client/page-rendering slice

**Owner:** Main agent  
**Dependencies:** G10-MKT-026 shared CRM display models; existing
`ListCrmReadRows` protobuf/service contract; existing target-neutral
Marketing/CRM route registry  
**Allowed paths:** `apps/console/src/main.rs`; audit and append-only ledger
records  
**Forbidden paths:** protobuf, server, domain, infrastructure, worker,
Folio, migrations, and command-owner paths  

### Delivered evidence

- Added additive Marketing/CRM navigation for the exact existing 23-route
  allow-list with bilingual route labels.
- Wired typed request construction and target-specific gRPC dispatch for the
  existing `ListCrmReadRows` contract, including actor/session metadata,
  eventual consistency, customer-visible audit context, bounded page size,
  and aggregate selection.
- Rejected platform and subtree scope before transport. The current console
  has no authorized holding/company/branch scope provider, so it renders a
  typed unavailable/read-only surface instead of issuing an unsafe query.
- Mapped only the existing handoff, opportunity, and attribution aggregates to
  the shared CRM models. Payload fields are bounded and allow-listed; raw JSON
  and sensitive markers are discarded.
- Added explicit loading, empty, denied, stale, read-only, offline, and
  unavailable state rendering in English and Thai. No command UI was added.
- No database, migration, protobuf, server, domain, infrastructure, worker,
  Folio, or command-owner effect.

Evidence: `audits/G10-MKT-027/README.md`.

### Packet verification

- `cargo fmt --all` — PASS.
- `cargo test -p erpx-console -- --nocapture` — PASS; 18 passed.
- `cargo clippy -p erpx-console --all-targets -- -D warnings` — PASS.
- `cargo check -p erpx-console --no-default-features --features web` — PASS.
- `cargo check -p erpx-console --no-default-features --features desktop` — PASS.

### Open boundary and rollback

An authoritative tenant-scope provider, cursor continuation, projection
freshness metadata, and remaining CRM/Marketing adapters are still required.
Rollback removes the additive console CRM resource/navigation/rendering code;
Customer 360, Platform Operations, Support, and Incident surfaces remain
unchanged. The persistent ERPX goal remains active and all G1-G14 gates remain
open.

## Verification wave 2026-07-25 — full workspace after G10-MKT-027

- `cargo fmt --all --check` — PASS.
- `cargo clippy --workspace --all-targets -- -D warnings` — PASS.
- `cargo test --workspace` — PASS; active workspace suites passed, including
  18 console tests, 160 domain tests, 118 infrastructure tests, 64 server
  tests, 62 shared UI tests, 66 worker tests, and migration fixture tests.
  Database-dependent suites remain explicitly ignored without disposable
  database URLs.
- `cargo check -p erpx-console --no-default-features --features web` — PASS.
- `cargo check -p erpx-console --no-default-features --features desktop` — PASS.
- `bun run lint` — PASS with four pre-existing warnings in
  `infra/scripts/start-n8n.js`, zero errors.
- `bunx tsc --noEmit` — PASS.
- `curl -fsS http://127.0.0.1:3004/operations` — PASS.
- `pg_isready` — PASS; PostgreSQL accepting connections.

Folio remains operational on port 3004. No migration, command-owner, tenant
write, or Folio write-retirement change was made. The persistent ERPX goal
remains active; an authoritative CRM scope provider, live CRM projection
freshness/cursor path, remaining Marketing/CRM requirements, and all other
G1-G14 gates remain open.

## Entry 2026-07-25 — G8-FIN-030 authenticated Finance close-review read transport

The main agent added and independently reviewed `GetFinanceCloseReview`.
Queries require an authenticated context, explicit holding/company/branch
scope, `finance.read`, tenant customer-scope membership, matching company
identity, and valid tenant routing. The response contains only typed close
metadata, report totals, account summaries, control counts, and explicit
missing/fresh/stale/unknown metadata. Each successful read records a
customer-auditable `finance.close_review.read` event. Platform/subtree and
cross-company reads fail closed; no command owner or Folio write behavior
changed.

Evidence: `audits/G8-FIN-030/README.md`; protobuf/server checks PASS, server
tests 64 PASS, added scope test PASS, and server Clippy PASS. Dedicated close
read projection storage/worker, database-backed query evidence, Finance UI,
and the broader G8 gate remain open.

## Verification wave 2026-07-25 — full workspace after G8-FIN-030

- `cargo fmt --all --check` — PASS.
- `cargo clippy --workspace --all-targets -- -D warnings` — PASS.
- `cargo test --workspace` — PASS; active workspace suites and doc tests
  passed, including 160 domain, 122 infrastructure, 65 server, 62 shared UI,
  and 66 worker tests.
  Database-dependent suites remain explicitly ignored without disposable URLs.
- `cargo check -p erpx-console --no-default-features --features web` — PASS.
- `cargo check -p erpx-console --no-default-features --features desktop` — PASS.
- `bun run lint` — PASS with four pre-existing warnings in
  `infra/scripts/start-n8n.js`, zero errors.
- `bunx tsc --noEmit` — PASS.

Folio remains operational on port 3004 and PostgreSQL is ready. No command
ownership or Folio write-retirement change was made. The persistent ERPX goal
remains active; dedicated close projection/worker/database evidence and all
other ERPX gates remain open.

## Entry 2026-07-25 — G8-FIN-031 dedicated Finance close-review read projection

The main agent independently reviewed and integrated the migration sidecar and
implemented the cross-crate projector path. Tenant read migration 0031 creates
typed close-review/report summaries, account summaries, and per-period
checkpoints with forced holding RLS. `SqlxFinanceCloseProjectionStore` reads
only the immutable close-review outbox stream from the tenant write database
and atomically writes the disposable tenant read database. It rejects invalid
event metadata and payload identity, requires contiguous aggregate versions,
makes duplicate delivery a no-op, replaces account summaries atomically, and
clears all projection state for rebuilds. `FinanceCloseProjectionRunner`
provides incremental and full tenant-routed replay. The close-read adapter and
server helper now route only to the tenant read database; no tenant write pool
is opened by the authenticated close-read query.

Task contract: migration sidecar was limited to
`crates/migration/migrations_tenant_read/0031_finance_close_projection.sql`;
main-owned integration covered infrastructure, worker, and server read
transport. No Folio or command-owner behavior changed.

Evidence: `audits/G8-FIN-031/README.md`; infrastructure projector tests 2 PASS,
worker projector tests 2 PASS, migration tests 6 PASS, targeted server and
workspace crate Clippy PASS. Live disposable database projection/rebuild
evidence, durable scheduler registration, Finance UI, and the broader G8 gate
remain open.

## Entry 2026-07-25 — G11-PEO-002 HR/payroll tenant-write persistence

The main agent independently reviewed and integrated the isolated HR sidecar.
`SqlxHrPersistence` now persists employee, leave, expense, payroll-run/line,
and expired-role-reassignment source state in the holding tenant write
database. Migration 0038 uses typed columns, forced holding RLS, optimistic
versions, idempotency request hashes, and terminal-state triggers. Accepted
mutations append bounded aggregate outbox events atomically; payroll lines and
totals reconcile, approval fields are separated, and replay/conflict behavior
is explicit. No HR transport, read projection, worker, or UI was claimed.

Evidence: `audits/G11-PEO-002/README.md`; focused HR persistence tests 4 PASS,
migration tests 6 PASS, and independent infrastructure checks PASS. Live
database mutation evidence, authoritative RBAC command wiring, read
projections, expiry-worker integration, payroll reports, and the broader G11
gate remain open.

## Verification wave 2026-07-25 — G8-FIN-031 and G11-PEO-002 integration

- `cargo fmt --all --check` — PASS.
- `cargo clippy -p erpx-infrastructure -p erpx-worker -p erpx-server
  --all-targets -- -D warnings` — PASS.
- Focused infrastructure close projection/read tests — PASS; 13 Finance-close
  tests and 4 HR persistence tests in their respective focused runs.
- Focused worker close projection tests — PASS; 2 tests.
- `cargo test -p erpx-migration -- --nocapture` — PASS; 6 tests.
- `cargo test --workspace` — PASS; active suites included 160 domain, 128
  infrastructure, 65 server, 62 UI, 69 worker, 8 application, 20 identity,
  and 6 migration tests. Database-dependent suites remain explicitly ignored
  without disposable database URLs.
- `cargo check -p erpx-console --no-default-features --features web` — PASS.
- `cargo check -p erpx-console --no-default-features --features desktop` —
  PASS.
- `bun run lint` — PASS with four pre-existing warnings in
  `infra/scripts/start-n8n.js`, zero errors.
- `bunx tsc --noEmit` — PASS.
- `curl -fsS http://127.0.0.1:3004/operations` — PASS.
- `pg_isready` — PASS; PostgreSQL accepting connections.

Folio remains operational on port 3004 and PostgreSQL remains available. No
tenant command owner, migration cutover, or Folio write-retirement action was
performed. The persistent goal remains active and all G1-G14 gates remain open.

## Entry 2026-07-25 — G11-PEO-004 authenticated HR employee command

The main agent added a dedicated `HrService.SaveHrEmployee` transport. The
handler requires an authenticated token and shared command context, rejects
platform/subtree scope, bounds and decodes only an `EmployeeRecord`, binds its
identity/scope/command/idempotency/version fields to the request, and invokes
the common customer-scoped authorization path for HR entitlement, RBAC,
billing, and SoD. It writes only through the routed tenant write database via
`SqlxHrPersistence`, records `hr.employee.saved` audit metadata, and returns a
typed redacted receipt. Leave, expense, payroll, reassignment, read projection,
and UI transport remain open.

Task contract: main agent owned the shared protobuf/service and server
integration; no subagent path overlap occurred. Folio command ownership was
unchanged.

Evidence: `audits/G11-PEO-004/README.md`; protobuf/server check PASS, server
Clippy PASS, and 65 server tests PASS. The full wave after this packet remains
required before the next integration boundary.

## Entry 2026-07-25 — G11-PEO-003 HR/payroll read projection

The main agent independently reviewed and integrated the migration sidecar and
implemented the HR read projector. Migration 0032 adds typed employee, leave,
expense, payroll run/line, and expired-role reassignment read rows, plus
aggregate checkpoints and event receipts; every table has forced holding RLS.
`SqlxHrProjectionStore` reads only the allow-listed five HR aggregate types
from the tenant write outbox, rejects malformed identity/version metadata,
enforces contiguous per-aggregate versions, treats receipt replay as a no-op,
and writes only typed fields to the tenant read database. Payroll line rows are
replaced atomically per event. `HrProjectionRunner` supports bounded incremental
catch-up and disposable read-model rebuild/clear.

Task contract: the support migration packet was limited to migration 0032;
main-owned integration covered infrastructure and worker paths. No shared
transport, Folio, or command-owner change was made by this packet.

Evidence: `audits/G11-PEO-003/README.md`; infrastructure projection tests 2
PASS, worker projection tests 2 PASS, migration tests 6 PASS, and strict
infrastructure/worker Clippy PASS. HR read transport, UI, durable scheduler
registration, live replay, and full reconciliation remain open.

## Entry 2026-07-25 — G11-PEO-005 authenticated HR employee read transport

The main agent added the typed `HrService.ListHrEmployees` query. The handler
requires an authenticated session and query context, rejects platform/subtree
scope, enforces explicit contained company or branch scope and `hr.read`,
validates tenant and holding references plus bounded exclusive keyset paging,
binds the requested scope to the customer account, and records a customer
auditable `hr.employee.read` event with purpose and correlation. It routes only
through `RoutedDatabaseSide::Read` to `SqlxHrReadStore`, returns typed employee
projection fields and source/freshness metadata, and closes the route without
opening a write pool. Leave, expense, payroll, and reassignment read transports,
live database evidence, UI, and the broader G11 gate remain open.

Task contract: main agent owned the shared protobuf and server integration;
the HR read store was already isolated to the infrastructure projection path.
No Folio or command-owner behavior changed.

Evidence: protobuf/infrastructure/server checks PASS; focused infrastructure HR
projection tests 2 PASS, worker HR projection tests 2 PASS, server tests 65
PASS, migration tests 6 PASS, and strict infrastructure/worker/server Clippy
PASS.

## Verification wave 2026-07-25 — G11-PEO-005 HR read transport integration

- `cargo fmt --all --check` — PASS.
- `cargo clippy --workspace --all-targets -- -D warnings` — PASS.
- `cargo test --workspace` — PASS; active suites included 8 application, 18
  console, 160 domain, 20 identity, 126 infrastructure, 6 migration, 65
  server, 62 shared UI, and 70 worker tests. Four infrastructure and one
  worker database-dependent tests plus other database suites remain explicitly
  ignored without disposable database URLs.
- `cargo check -p erpx-console --no-default-features --features web` — PASS.
- `cargo check -p erpx-console --no-default-features --features desktop` —
  PASS.
- `bun run lint` — PASS with four pre-existing warnings in
  `infra/scripts/start-n8n.js`, zero errors.
- `bunx tsc --noEmit` — PASS.
- `curl -fsS http://127.0.0.1:3004/operations` — PASS.
- `pg_isready -h /tmp -p 5432` — PASS; PostgreSQL accepting connections.

Folio remains operational on port 3004. No tenant command owner, migration
cutover, or Folio write-retirement action was performed. The persistent ERPX
goal remains active and all G1-G14 gates remain open.

## Entry 2026-07-25 — G11-PEO-006 typed HR read adapters

The support infrastructure packet was independently reviewed and integrated.
`SqlxHrReadStore` now exposes typed bounded pages for leave requests, expense
claims, payroll runs with normalized line children, and expired-role
reassignment metadata. Every query uses the holding-bound read pool and
explicit holding/company/optional-branch scope, validates nil/zero/bounds
before database access, orders by deterministic exclusive keysets, and keeps
payroll line limits and truncation visible. No raw aggregate JSON, command
context, credential, payment instrument, or tenant write pool crosses this
read boundary.

Task contract: allowed paths were limited to the infrastructure HR projection
adapter and its exports. No migration, protobuf, server, worker, UI, Folio, or
command-owner changes were made.

Evidence: `cargo fmt --all --check` PASS, infrastructure HR projection tests
5 PASS, infrastructure Clippy PASS, and infrastructure/server compile checks
PASS. Live database verification is recorded separately below.

## Entry 2026-07-25 — G11-PEO-007 authenticated HR leave read transport

The main agent added `HrService.ListHrLeaveRequests`. The query requires an
authenticated session and correlated query context, rejects platform/subtree
scope, enforces explicit contained company/branch scope and `hr.read`, validates
tenant/holding references and bounded page size, binds customer scope, routes
only to the tenant read database, and records a customer-visible
`hr.leave.read` audit event. Leave rows are mapped from typed projection fields
with source/freshness metadata. The transport cursor is hex-opaque and
round-trips only a validated date/UUID keyset position; it contains no payload
or authorization material. Expense, payroll, reassignment transport, HR UI,
and the broader G11 gate remain open.

Task contract: main agent owned the shared protobuf and server integration;
the read adapter remained in its previously isolated infrastructure packet.
No Folio or command-owner behavior changed.

Evidence: protobuf/infrastructure/server compile checks PASS, the two new
bounded cursor tests PASS (with the previously verified 65 server tests still
passing), and strict server Clippy PASS.

## Entry 2026-07-25 — G11-PEO-008 authenticated HR expense read transport

The main agent added `HrService.ListHrExpenseClaims`. It requires an
authenticated correlated query context, explicit contained company/branch
scope, `hr.read`, customer scope binding, bounded page size, and a customer-
auditable `hr.expense.read` event. The handler routes only to the tenant read
database, maps typed expense projection fields and source/freshness metadata,
and exposes only an opaque UUID keyset cursor. Payroll/reassignment read
transport, all remaining HR mutation commands, UI, and the broader G11 gate
remain open.

Evidence: protobuf/infrastructure/server compile checks PASS, three HR
transport cursor tests PASS (including the new expense cursor test), and strict
server Clippy PASS. No Folio or command-owner behavior changed.

## Entry 2026-07-25 — G11-PEO-009 authenticated HR payroll read transport

The main agent added `HrService.ListHrPayrollRuns`. It requires an
authenticated correlated query context, explicit contained company/branch
scope, `hr.read`, customer scope binding, independently bounded parent and
line page sizes, and a customer-auditable `hr.payroll.read` event. It routes
only to the tenant read database and maps typed payroll totals, normalized
line rows, source/freshness metadata, and explicit line truncation state. The
parent cursor is opaque and contains only a validated date/UUID keyset
position. Reassignment transport, HR command completion, UI, and the broader
G11 gate remain open.

Evidence: protobuf/infrastructure/server compile checks PASS, four HR
transport cursor tests PASS, and strict server Clippy PASS. No Folio or
command-owner behavior changed.

## Entry 2026-07-25 — G11-ERP-POS-001 typed POS domain slice

The support agent delivered an isolated pure-domain POS slice. `pos.rs`
contains scope-bound customer/product/register references, bounded sale lines,
typed non-secret payment facts, integer minor-unit totals and currency
invariants, closed Draft/Open/Held/Completed/Voided/Refunded transitions,
optimistic version checks, bounded command history with replay/conflict
semantics, and typed audit event metadata. Card numbers, bank details, tokens,
payment instruments, and raw JSON are structurally absent. No persistence,
outbox, projection, transport, UI, Finance refund handoff, or Folio behavior
changed.

Task contract: allowed paths were `crates/domain/src/pos.rs` and the module
declaration in `crates/domain/src/lib.rs`; all other paths were forbidden.

Evidence: main-agent independent `cargo test -p erpx-domain -- --nocapture`
PASS with 165 tests, and `cargo clippy -p erpx-domain --all-targets --
-D warnings` PASS. The POS packet is partial and does not close G11.

## Live verification 2026-07-25 — HR write/read migration and projection

Four explicitly named disposable databases were created only for this check.
The complete tenant write and tenant read migration directories were applied
successfully by SQLx CLI; PostgreSQL confirmed all six HR read-row tables and
HR write tables have forced RLS. The added ignored integration test
`hr_payroll_projection::live_hr_write_projection_and_read_are_idempotent_and_separate`
then passed against SQLx-initialized fresh write/read databases: the employee
write returned a versioned receipt, exact idempotent replay was a no-op, the
outbox event projected once with duplicate receipt no-op, and the separate read
store returned typed employee data with source metadata.

The first direct-SQL fixture run was rejected by SQLx with the expected
duplicate-history/version setup errors and was not counted as evidence; the
fresh SQLx migration-history run passed. The disposable databases were removed
after verification. No existing Folio or development database was touched.

## Entry 2026-07-25 — G11-ERP-POS-002 normalized POS write persistence

The support infrastructure packet was independently reviewed and integrated.
Migration 0039 creates typed normalized sale, line, payment, and idempotency
tables with forced holding RLS, scope/reference checks, payment-method
allow-lists, deferred totals/payment reconciliation, and terminal-sale/child
immutability triggers. `SqlxPosPersistence` validates the pure POS aggregate,
enforces expected-version and request-hash replay/conflict semantics, replaces
normalized children atomically, records typed idempotency, and appends the POS
outbox event in the same transaction. No payment instrument, provider token,
secret, or raw source aggregate JSON is stored in the POS source tables.

Task contract: allowed paths were the new POS infrastructure adapter, its
export, and migration 0039 only. No domain, protobuf, server, worker, UI,
Folio, plan, or ledger file was changed by the support packet.

Evidence: POS infrastructure tests 3 PASS, migration tests 6 PASS, strict
infrastructure Clippy PASS, and a main-agent live disposable SQLx test PASS.
The live test confirmed one normalized sale/line, zero payments, one POS
outbox event, and exact idempotent replay. The disposable database was removed.
POS read projection, worker, authenticated command/read transport, UI, and
Finance refund handoff remain open.

## Verification wave 2026-07-25 — HR leave transport and POS source persistence

- `cargo fmt --all --check` — PASS.
- `cargo clippy --workspace --all-targets -- -D warnings` — PASS.
- `cargo test --workspace` — PASS; active suites included 8 application, 18
  console, 165 domain, 20 identity, 132 infrastructure, 6 migration, 67
  server, 62 shared UI, and 70 worker tests. Four infrastructure and one
  worker database-dependent unit tests, plus database integration suites,
  remain explicitly ignored without disposable database URLs.
- `cargo check -p erpx-console --no-default-features --features web` — PASS.
- `cargo check -p erpx-console --no-default-features --features desktop` —
  PASS.
- `bun run lint` — PASS with four pre-existing warnings in
  `infra/scripts/start-n8n.js`, zero errors.
- `bunx tsc --noEmit` — PASS.
- `curl -fsS http://127.0.0.1:3004/operations` — PASS.
- `pg_isready -h /tmp -p 5432` — PASS; PostgreSQL accepting connections.

Folio remains operational on port 3004. No migration cutover, tenant command
owner switch, or Folio write retirement was performed. The persistent ERPX goal
remains active and all G1-G14 gates remain open.

## Verification wave 2026-07-25 — HR payroll transport and POS projection

- `cargo fmt --all --check` — PASS.
- `cargo clippy --workspace --all-targets -- -D warnings` — PASS.
- `cargo test --workspace` — PASS; active suites included 8 application, 18
  console, 165 domain, 20 identity, 136 infrastructure, 6 migration, 69
  server, 62 shared UI, and 72 worker tests. Four infrastructure and one
  worker database-dependent unit tests, plus database integration suites,
  remain explicitly ignored without disposable database URLs.
- `cargo check -p erpx-console --no-default-features --features web` — PASS.
- `cargo check -p erpx-console --no-default-features --features desktop` —
  PASS.
- `bun run lint` — PASS with four pre-existing warnings in
  `infra/scripts/start-n8n.js`, zero errors.
- `bunx tsc --noEmit` — PASS.
- `curl -fsS http://127.0.0.1:3004/operations` — PASS.
- `pg_isready -h /tmp -p 5432` — PASS; PostgreSQL accepting connections.

Folio remains operational on port 3004. No migration cutover, tenant command
owner switch, or Folio write retirement was performed. The persistent ERPX goal
remains active and all G1-G14 gates remain open.

## Entry 2026-07-25 — G11-ERP-POS-003 typed POS read projection and worker seam

The support packet was independently reviewed and integrated. Read migration
0033 creates typed sale, line, payment, checkpoint, and event-receipt tables
with forced holding RLS. `SqlxPosProjectionStore` reads only the allow-listed
`pos_sale` outbox stream, validates payload identity and aggregate versions,
enforces contiguous checkpoints, makes duplicate receipts no-ops, writes typed
read rows, and clears all projection state for rebuild. `PosProjectionRunner`
provides bounded incremental/rebuild execution through tenant write/read
routing with redacted errors.

Task contract: allowed paths were read migration 0033, the infrastructure
projection adapter/export, and the worker projection adapter/export. No domain,
protobuf, server, UI, Folio, plan, or ledger file was changed by the packet.

Evidence: infrastructure projection tests 3 PASS, worker projection tests 2
PASS, strict infrastructure/worker Clippy PASS, and SQLx CLI applied the full
tenant-read migration set including 0033. PostgreSQL confirmed all five POS read
tables have forced RLS; the disposable database was removed. Live event replay,
durable scheduler activation, POS read transport/UI, and the broader G11 gate
remain open.

## Entry 2026-07-25 — G11-ERP-POS-004 authenticated POS read gateway

The main agent added the typed `PosService.ListPosSales` gRPC boundary. It
requires an authenticated correlated query context, explicit contained
company/branch scope, `pos.read`, customer-scope binding, bounded parent and
child pages, and a customer-auditable `pos.sale.read` event. The service routes
only to the tenant read database and returns typed sale, line, and non-secret
payment summaries with source/freshness metadata and an opaque timestamp/UUID
keyset cursor. Payment instruments, provider tokens, idempotency keys, command
history, and raw aggregate JSON are not returned.

The infrastructure adapter independently enforces forced holding scope,
bounded child rows, typed decoding, and read-only queries. No write pool is
used by the transport. Command transport, live projection/read replay, UI
consumption, Finance refund handoff, durable scheduler activation, and the
broader G11 gate remain open.

Evidence: infrastructure POS projection/read focused tests 4 PASS; server POS
cursor transport tests 2 PASS; protobuf, infrastructure, and server compile
checks PASS; strict server Clippy PASS. No Folio, migration cutover, tenant
command-owner switch, or Folio write-retirement behavior changed.

## Entry 2026-07-25 — G11-PEO-010 authenticated HR leave command

The main agent added `HrService.SaveHrLeave`. The command requires an active
correlated command context, explicit contained holding/company/branch scope,
typed bounded leave input, aggregate/command identity binding, expected-version
matching, authoritative HR entitlement/RBAC/billing/SoD authorization, tenant
write-database routing, atomic idempotent persistence, and customer-auditable
`hr.leave.saved` metadata. It does not expose raw unbounded input or route a
mutation to a read database. Expense, payroll, and reassignment command
transports, HR UI, and the broader G11 gate remain open.

Evidence: protobuf/server/infrastructure compile checks PASS; full workspace
tests and strict workspace Clippy PASS in the following integration wave. No
Folio or command-owner behavior changed.

## Entry 2026-07-25 — G11-ERP-POS-005 shared Dioxus POS presentation contract

The support UI packet was independently reviewed and integrated under the
non-overlapping POS-005 UI ID. It adds the seven required allow-listed POS
routes, opaque bounded sale identifiers, bilingual route/status/payment copy,
explicit loading/empty/denied/stale/read-only/conflict/offline/error states,
and scalar-only redacted sale/line/payment-summary view models. Payment
instruments, provider tokens, sensitive account data, and raw serialized
payloads are structurally absent. No transport, network, database, or fake
success state was added.

Task contract: allowed paths were `crates/ui/src/pos.rs`, the module/export in
`crates/ui/src/lib.rs`, and required POS localization keys in
`crates/ui/src/i18n.rs`; server, protobuf, infrastructure, worker, domain,
Folio, plan, and ledger paths were forbidden.

Evidence: main-agent review confirmed the bounded scalar-only shapes and
allow-list; `cargo fmt --all --check` PASS; `cargo test -p erpx-ui pos` PASS
with 5 tests; strict UI Clippy PASS. Authenticated transport and consuming
adapters remain separate work.

## Verification wave 2026-07-25 — POS read gateway, HR leave command, and POS UI

- `cargo fmt --all --check` — PASS.
- `cargo clippy --workspace --all-targets -- -D warnings` — PASS.
- `cargo test --workspace` — PASS; active suites included 8 application, 18
  console, 165 domain, 20 identity, 136 infrastructure, 6 migration, 71
  server, 67 shared UI, and 72 worker tests. Database-dependent suites and
  the new POS disposable read test remain explicitly ignored without the
  required temporary database URLs.
- `cargo check -p erpx-console --no-default-features --features web` — PASS.
- `cargo check -p erpx-console --no-default-features --features desktop` —
  PASS.
- `bun run lint` — PASS with four pre-existing warnings in
  `infra/scripts/start-n8n.js`, zero errors.
- `bunx tsc --noEmit` — PASS.
- `curl -fsS http://127.0.0.1:3004/operations` — PASS.
- `pg_isready -h /tmp -p 5432` — PASS; PostgreSQL accepting connections.

Folio remains operational on port 3004. No migration cutover, tenant
command-owner switch, or Folio write retirement was performed. The persistent
ERPX goal remains active and all G1-G14 gates remain open.

## Live verification 2026-07-25 — POS typed read projection and pagination

A fresh disposable PostgreSQL database was created for this check. After
rebuilding the embedded migration crate so migration 0033 was present,
`pos_projection_read::live_pos_read_projection_is_scope_bound_bounded_and_cursor_stable`
passed. It applied the tenant-read migration, inserted two scoped sale rows
with one normalized line and payment summary, verified bounded parent paging,
opaque cursor continuation, company scope selection, and typed child reads.
The initial stale-build attempt was rejected because the disposable database
did not yet contain migration 0033; it was not counted as evidence. The
database was removed after the passing run and no development database was
touched.

## Entry 2026-07-25 — G11-PEO-011 authenticated HR expense and payroll commands

The main agent added `HrService.SaveHrExpense` and `HrService.SaveHrPayroll`.
Both boundaries require active correlated command contexts, explicit contained
scope, bounded typed JSON, expected-version matching, authoritative HR
entitlement/RBAC/billing/SoD authorization, tenant write-database routing,
atomic idempotent persistence, and customer-auditable save events. Payroll
input remains subject to domain-enforced line totals and preparer/reviewer/
approver separation. No read database is used by either command and no raw
transport payload is returned.

Evidence: protobuf/server/infrastructure compile checks PASS and strict server
Clippy PASS. Reassignment command/read transport, HR UI, and the broader G11
gate remain open; the next full integration wave will include this packet.

## Entry 2026-07-25 — G11-PEO-012 authenticated HR reassignment read

The main agent added `HrService.ListHrReassignments`. It requires an active
correlated query context, explicit contained company/branch scope, `hr.read`,
customer-scope binding, bounded page size, tenant read-database-only routing,
and a customer-auditable `hr.reassignment.read` event. The response exposes
only typed expired-principal/replacement metadata, reason, scope, source
version/event, and freshness; the keyset cursor is opaque and contains only a
validated timestamp/workflow position.

Evidence: five HR transport cursor tests PASS and strict server Clippy PASS;
protobuf/server/infrastructure compile checks PASS. Reassignment command
transport, expiry-worker execution, HR UI, and the broader G11 gate remain
open.

## Entry 2026-07-25 — G11-ERP-POS-006 authenticated POS sale command

The main agent added `PosService.SavePosSale`. It requires an active command
context, bounded typed sale JSON, exact command/idempotency and scope binding,
expected-version matching, authoritative POS entitlement/RBAC/billing/SoD
authorization, tenant write-database-only routing, atomic normalized source
persistence, and customer-auditable `pos.sale.saved` metadata. It does not
accept platform/subtree mutation scope, return raw payloads, or touch the read
database.

Evidence: protobuf/server/infrastructure compile checks PASS and strict server
Clippy PASS. Refund handoff, command UI, durable projection activation, and the
broader G11 gate remain open.

## Entry 2026-07-25 — G11-LEG-001 Legal/document source persistence

The support packet was independently reviewed under its non-overlapping
allowlist: `crates/infrastructure/src/legal_persistence.rs`, its infrastructure
export, and tenant-write migration 0040. It adds typed normalized persistence
for scoped contracts, contract parties, immutable approval facts, retention
state, document metadata, opaque document versions, append-only access history,
and command idempotency. Writes use holding-scoped forced RLS, optimistic source
versions, atomic outbox events, replay/conflict binding, and database triggers
for archived contracts, approvals, document versions, and access history. The
packet deliberately stores no raw document bytes, credentials, tokens, or
provider state.

During independent review, the first disposable live attempt exposed a real
document SQL bind-order defect (`current_version` received the boolean archived
value). Main corrected the title/current-version/archived/source-version/
idempotency bindings and reran focused verification. The failed attempt is not
counted as evidence.

Evidence after correction: `cargo fmt --all --check` PASS; seven focused legal
infrastructure tests PASS; strict infrastructure Clippy PASS; six migration
tests PASS; and the fresh disposable
`live_legal_write_is_normalized_scoped_and_idempotent` test PASS. The live test
applied the full tenant-write migration set, saved a typed contract and empty
document, replayed the contract idempotently with the same event receipt,
verified normalized contract/document/outbox counts, and removed the database.
Legal read projection, transport, MinIO signed access, and the broader G11 gate
remain open. No Folio route, migration cutover, command-owner switch, or write
retirement changed.

## Verification wave 2026-07-25 — Legal source persistence integration

- `cargo fmt --all --check` — PASS.
- `cargo clippy --workspace --all-targets -- -D warnings` — PASS.
- `cargo test --workspace` — PASS; all active workspace suites passed. Only
  explicitly database-dependent tests remain ignored without their required
  disposable database URLs.
- `cargo check -p erpx-console --no-default-features --features web` — PASS.
- `cargo check -p erpx-console --no-default-features --features desktop` —
  PASS.
- `bun run lint` — PASS with the four pre-existing warnings in
  `infra/scripts/start-n8n.js`, zero errors.
- `bunx tsc --noEmit` — PASS.
- `curl -fsS http://127.0.0.1:3004/operations` — PASS.
- `pg_isready -h /tmp -p 5432` — PASS.

The first combined invocation was rejected before running because it used the
Folio root for Cargo; the corrected invocation used `erpx/` and passed. Folio
remains operational on port 3004. No tenant command-owner switch, migration
cutover, or Folio write retirement was performed. The persistent ERPX goal
remains active and all G1-G14 gates remain open.

## Correction 2026-07-25 — G11-LEG-002 Legal source remediation after independent QA

The preceding G11-LEG-001 entry was provisional and is superseded for
acceptance by this correction. Independent QA found that its narrow live proof
did not inspect the normalized contract idempotency value or the full event
context, and identified lifecycle/party mutation, source-version overflow,
weak object-reference/digest validation, and live RLS/trigger coverage gaps.
The first live document bind-order defect had already been corrected; these
additional QA findings were also real and were not waived.

Main-agent remediation delivered:

- `save_contract`, `save_document`, and access-history writes now require a
  validated shared `CommandContext`; command identity, actor, scope, contract
  version, and idempotency are bound at the write boundary.
- Contract inserts/updates persist the actual request idempotency key; creator
  identity is immutable; transitions follow the closed domain lifecycle;
  parties can change only while draft and are compared against stored rows
  otherwise.
- Document source-version increment uses checked arithmetic and fails at the
  BIGINT boundary.
- Object references reject URLs, data/base64 markers, traversal, query/fragment
  material, unsafe characters, and empty path segments; digests require the
  exact `sha256:` plus 64-hex format in code and migration constraints.
- Legal outbox writes now populate the shared event envelope (contract/event
  versions, scope, actor, correlation, causation, and idempotency) using
  migration 0041; access-event `occurred_at` is server-authored while the
  domain fact retains its observed time.
- The disposable proof now includes one document version, one access-history
  fact and exact replay, a draft party mutation, stale contract-version
  rejection, normalized idempotency inspection, full envelope inspection,
  cross-holding RLS under a temporary non-superuser role, and document-version
  trigger rejection. The temporary role and database are removed after the
  run.

Evidence: `cargo fmt --all --check` PASS; legal infrastructure focused tests
7 PASS; `cargo test -p erpx-infrastructure --test legal_persistence --no-run`
PASS; six migration tests PASS; and the fresh disposable
`live_legal_write_is_normalized_scoped_and_idempotent` PASS after the expanded
proof. The initial RLS assertion under the development superuser correctly
failed because PostgreSQL superusers bypass RLS; the test was changed to a
temporary non-superuser with explicit grants and then passed. No disposable
database or RLS test role remains.

Legal read projection, authenticated transport, MinIO signed access,
retention workers, authorization integration, and the broader G11 gate remain
open. The paused Legal read sidecar must consume the repaired shared envelope
before any integration. Folio routes, migration cutover, tenant command-owner
switch, and Folio write retirement remain unchanged.

## Entry 2026-07-25 — G11-PEO-013 HR/Payroll presentation contract

The support packet was independently reviewed under its disjoint UI allowlist:
`crates/ui/src/hr.rs`, its shared export/localization changes, and the focused
HR UI integration test. It adds six authenticated route definitions (`/hr`,
`/hr/employees`, `/hr/leave`, `/hr/expenses`, `/hr/payroll`, and
`/hr/reassignments`), bounded opaque identifiers, Thai/English labels,
scalar-only redacted employee/leave/expense/payroll/reassignment models, and
explicit loading, empty, denied, stale, read-only, conflict, offline, and
error states. No network, database, raw JSON, token, payroll secret, fake
success, or authorization behavior is present.

Evidence: focused HR tests (4 library + 2 integration) PASS; strict UI
Clippy PASS; `cargo fmt --all --check` PASS; and Dioxus console web/desktop
checks PASS. Main review confirmed that the presentation contract cannot
authorize a mutation. Authenticated transport consumers and full HR journeys
remain open.

## Entry 2026-07-25 — G11-ERP-POS-007 POS command presentation contract

The support packet was independently reviewed under its disjoint allowlist:
new `crates/ui/src/pos_commands.rs` and its focused test. It adds four typed
presentation surfaces for sale entry, refund review, receipt issuance, and
shift-close approval; bounded opaque IDs; scalar-only redacted values;
Thai/English labels; and explicit loading, empty, ready, denied, stale,
read-only, conflict, offline, and error states. Main then exported the module
from the shared UI crate without adding transport or runtime behavior.

Evidence: five focused POS command UI tests PASS; strict UI Clippy PASS;
`cargo fmt --all --check` PASS; and Dioxus console web/desktop checks PASS
after the export. Payment instruments, provider tokens, network/database
access, and authorization are structurally absent. POS command consumers,
refund accounting, and the broader G11 gate remain open.

## Verification wave 2026-07-25 — Legal remediation and HR/POS UI contracts

- `cargo fmt --all --check` — PASS.
- `cargo clippy --workspace --all-targets -- -D warnings` — PASS.
- `cargo test --workspace` — PASS; all active workspace suites passed,
  including 71 shared UI tests and 5 POS command integration tests. Only
  explicitly database-dependent tests remain ignored without their required
  disposable URLs.
- `cargo check -p erpx-console --no-default-features --features web` — PASS.
- `cargo check -p erpx-console --no-default-features --features desktop` —
  PASS.
- `bun run lint` — stable rerun PASS with the four pre-existing warnings in
  `infra/scripts/start-n8n.js`, zero errors.
- `bunx tsc --noEmit` — PASS.
- `curl -fsS http://127.0.0.1:3004/operations` — PASS.
- `pg_isready -h /tmp -p 5432` — PASS.

One parallel lint invocation was not counted because Cargo was concurrently
rebuilding `erpx/target` and ESLint observed a disappearing incremental
directory; it reported no source lint error. The stable rerun after Rust
finished passed. Folio remains operational on port 3004. No migration
cutover, command-owner switch, or Folio write retirement occurred. The
persistent ERPX goal remains active and all G1-G14 gates remain open.

## Entry 2026-07-25 — G11-LEG-003 Legal transport contract freeze

Main froze an additive `LegalService` protobuf contract for the dependency-ready
Legal transport packet. It defines typed save receipts for contracts,
documents, and access-history facts, plus scoped bounded-cursor reads for
normalized contracts, document versions, and access history. Read rows carry
only bounded opaque object metadata, source event identifiers, correlation
metadata, and projection timestamps; raw document bytes, credentials, tokens,
and provider payloads are excluded by the wire shape.

Evidence: `cargo check -p erpx-proto` — PASS. The projection sidecar must still
provide the read-store implementation, and authenticated server handlers,
MinIO signed access, retention workers, authorization, and full Legal workflow
evidence remain open. No database migration, Folio route, command-owner switch,
or write retirement changed in this contract-only step.

## Entry 2026-07-25 — G11-LEG-005 authenticated Legal gRPC transport

AG-LEG delivered the six methods in the frozen `LegalService`: three typed
write commands and three scoped read queries. Writes authenticate the caller,
reject platform/subtree scope, bind typed aggregate/context identity, enforce
the authoritative Legal entitlement/RBAC decision, route only to the tenant
write database, return typed receipts/correlation IDs, and emit customer audit
events. Reads require an explicit contained company/branch scope and
`legal.read`, route only to the tenant read database, validate bounded opaque
cursors, map normalized typed rows, and emit customer audit events.

Main review confirmed no proto or migration changes and no raw document bytes
or credentials in the response mapping. Independent evidence: `rustfmt
--edition 2024 --check apps/server/src/main.rs` — PASS; `cargo check
-p erpx-server` — PASS; `cargo test -p erpx-server` — 72 PASS; strict server
Clippy — PASS; Dioxus console web and desktop checks — PASS. A live
auth/database transport proof remains open, as do retention worker, signed
object endpoint wiring, Legal UI, and the broader G11 gate.

## Entry 2026-07-25 — G11-LEG-006 bounded Legal retention sweep

Main added `SqlxLegalPersistence::expire_due_contracts`. It scans at most 100
active due contracts, reconstructs each from normalized source rows, invokes
the typed domain expiry transition under a `SystemWorker` actor, and persists
the transition with the normal optimistic version, deterministic idempotency
key, and shared outbox envelope. Concurrent version/not-found races are
reported as conflicts for the next sweep; malformed source facts fail closed.

Independent evidence: infrastructure compile and focused checks pass; fresh
disposable PostgreSQL proof
`live_legal_retention_sweep_expires_due_contract_once` — PASS. The proof
covered Draft → Review → Approved → Active, expiry at a past retention
deadline, exact version 5, and an empty second sweep. The database was dropped
after verification. Production worker scheduling and object deletion remain
open.

## Entry 2026-07-25 — G11-LEG-007 Legal Dioxus presentation contract

AG-UI delivered the allowlisted target-neutral Legal surface: five route
descriptors, bounded opaque IDs for all legal references, scalar-only redacted
contract/document/access models, Thai/English labels, and explicit
Loading/Empty/Ready/Denied/Stale/ReadOnly/Conflict/Offline/Error states.
No JSON, bytes, URLs, credentials, tokens, signed URLs, payment data,
authorization logic, network, database, or mutation behavior is present.

Independent evidence: file-scoped Rustfmt — PASS; `cargo test -p erpx-ui
--test legal -- --nocapture` — 4 PASS; strict UI Clippy — PASS. Authenticated
adapters, signed downloads, approval journeys, offline sync, conflict
resolution, and target-specific rendering remain open.

## Entry 2026-07-25 — G11-ERP-POS-008 bounded refund-review domain

AG-SCM delivered the allow-listed `domain::pos_refund` slice. It consumes a
completed-sale/reference fact and bounded refundable-line facts, requires an
exact company/branch scope, validates positive quantities/amounts and
over-refund limits, and exposes the closed requested → approved/rejected →
posted/cancelled lifecycle. Approval separates requester and reviewer. Command
records bind UUID/idempotency/fingerprint/version facts and terminal replay is
fail-closed; no payment instruments, provider tokens, accounting entries,
persistence, transport, authorization, or refund execution are included.

Main review tightened lifecycle fingerprints to bind expected version, scope,
and occurred-at facts, and required contiguous command-history versions.
Evidence: focused POS refund tests — 9 PASS; focused domain Clippy — PASS;
file-scoped Rustfmt check — PASS. The full workspace format gate is rerun in
the next integration wave after the concurrent migration-tooling sidecar
finishes. POS persistence/transport, Finance handoff, provider execution, and
the broader G11 gate remain open.

## Entry 2026-07-25 — G11-LEG-003 tenant-read Legal projection

The Legal read sidecar was independently reviewed and accepted under its
allowlist: tenant-read migration `0034_legal_documents.sql`, the typed
`SqlxLegalProjectionStore`/`SqlxLegalReadStore`, worker runner, exports, and
focused tests. It reads only Legal source events from the tenant-write outbox,
requires the repaired shared envelope (contract/event versions, typed tenant
scope, actor, correlation/causation, idempotency), rejects null/legacy or
malformed envelopes, and validates aggregate/payload scope containment.
Projection writes are atomic across normalized rows, receipts, and
checkpoints; duplicate events are harmless, version gaps fail closed, and
clear/rebuild is supported. Read access uses forced holding RLS, bounded
scope/cursors, typed source metadata, redacted object references/digests, and
freshness state.

Independent evidence: `cargo fmt --all --check` PASS; focused infrastructure
projection test PASS (1 structural, 1 explicitly ignored); focused worker test
PASS; strict infrastructure/worker Clippy PASS; fresh disposable live
two-database proof PASS. The live proof projected contract/document/access
events, replayed them idempotently, checked typed freshness and cross-holding
scope, cleared/rebuilt the read model, and rejected a legacy NULL-envelope
outbox row. Main additionally tightened projection object-reference validation
to reject traversal, URL/data/base64, query/fragment, and unsafe characters.

Read gRPC handlers, MinIO signed document access, retention expiry worker,
Legal authorization integration, and the broader G11 gate remain open. The
sidecar made no Folio change, tenant command-owner switch, migration cutover,
or write retirement.

## Entry 2026-07-25 — G11-LEG-004 Legal MinIO presign seam

Main added the infrastructure-only `MinioObjectStore::presign_legal_download`
adapter. It accepts the typed `SignedDownloadAuthorization`, rejects nil
document/version facts, expired grants, zero or over-64-MiB download limits,
lifetimes outside one second through seven days, and unsafe object-key
references, then returns only a bounded GET URL. It does not authorize
identity/scope/RBAC, read raw bytes, store credentials, or record access; those
remain server/source responsibilities.

Evidence: `rustfmt --edition 2024 --check
crates/infrastructure/src/object_storage.rs` — PASS; focused object-storage
tests — 4 PASS; `cargo clippy -p erpx-infrastructure --lib -- -D warnings` —
PASS. Server wiring, retention expiry, and the broader Legal gate remain open.

## Entry 2026-07-25 — G13-MIG-001 dry-run migration CLI

AG-MIG delivered the isolated `erpx-migrator` workspace binary. `fixture`
emits only a bounded deterministic fixture summary; `map` produces stable
namespace/context-bound UUID mappings; `reconcile` compares bounded source and
target snapshots while preserving explained versus unexplained differences;
and `plan` emits the full freeze/backfill/project/shadow-replay/reconcile/
pilot/rollback/final-delta/owner-switch/Folio-retention sequence as a
read-only plan. Unknown options, traversal/symlink output paths, oversized or
invalid JSON/UTF-8, and secret-looking keys/values fail closed without echoing
the input.

Independent evidence: `cargo fmt --all --check` PASS; `cargo test
-p erpx-migrator` PASS (3 unit + 4 CLI tests); strict migrator Clippy PASS.
No database, network, Folio state, migration schema, or cutover flag changed.
Live source adapters, backfill, shadow replay, cohort orchestration, rollback
execution, and final command-owner switching remain open.

## Correction 2026-07-25 — G13-MIG-001 path-boundary hardening

Main independently reviewed the migrator file boundary and tightened it before
acceptance: input and output paths must be relative, contain no parent
components, use existing non-symlink parent components, and reject symlink
targets or directories. Focused unit assertions cover absolute input and
output rejection. Rerun evidence: `cargo test -p erpx-migrator` — PASS (3 unit
+ 4 CLI tests); `cargo clippy -p erpx-migrator --all-targets -- -D warnings` —
PASS. The full workspace format check remains pending the active Legal server
handoff because its allowed `apps/server/src/main.rs` changes are not yet
formatted; no migrator source remains responsible for that pending diff.

## Entry 2026-07-25 — G8-FIN-032 Chart of Accounts source slice

AG-FIN delivered the company-scoped `FinanceChart` aggregate and normalized
tenant-write SQLx store. The domain enforces bounded canonical codes/names,
categories/normal balances, duplicate IDs/codes, parent containment and
cycle prevention, optimistic command facts, explicit lifecycle transitions,
and structural immutability when authoritative posted-journal usage exists.
Migration 0042 adds normalized chart/accounts/idempotency tables, deferred
parent/hierarchy checks, immutable identity/scope and posted-usage guards,
typed shared-envelope outbox events, and forced holding RLS.

Independent evidence: file-scoped Rustfmt — PASS; six Finance domain tests —
PASS; normalized persistence test — PASS (one live test ignored by default);
six migration tests — PASS; strict domain/infrastructure/migration Clippy —
PASS; fresh disposable PostgreSQL live proof
`live_chart_source_is_normalized_idempotent_scoped_and_usage_safe` — PASS.
The proof covered exact replay, versioned write/load, envelope fields,
posted-account structural rejection, lifecycle deactivation, forced RLS, and
cross-holding invisibility; the database was dropped afterward. Transport,
read projection, journal-posting adoption, legacy-chart reconciliation, and
the broader G8 gate remain open.

## Entry 2026-07-25 — G11-LEG-008 explicit Legal retention worker

AG-FND/WORKER delivered the write-only `legal_retention` handler. It validates
explicit tenant/holding references, bounded 1..=100 batch size, actor/purpose/
correlation context, routes only the tenant write database, invokes the typed
source sweep, maps counts, and redacts routing/source failures. The worker
does not open a read pool, touch MinIO, delete objects, or authorize callers;
the source adapter remains authoritative.

Independent evidence: `cargo fmt --all --check` — PASS; focused worker tests
— 2 PASS; full worker package tests — 77 PASS, 1 explicitly ignored; strict
worker Clippy — PASS; worker check — PASS. Explicit tenant environment
configuration is wired through the worker main branch. Durable multi-tenant
enumeration/claim scheduling, live worker/database proof, object deletion, and
the broader Legal gate remain open.

## Correction 2026-07-25 — workspace format gate reopened

After the Legal transport, Legal UI, Finance Chart, and retention-worker
packets were integrated and formatted, `cargo fmt --all --check` was rerun
independently and passed. The earlier migrator correction’s temporary pending
format note is superseded by this verification; no migrator source change was
needed.

## Verification wave 2026-07-25 — Finance Chart, Legal retention, and UI

- `cargo fmt --all --check` — PASS.
- `cargo clippy --workspace --all-targets -- -D warnings` — PASS.
- `cargo test --workspace --quiet` — PASS after the Finance Chart SQLx test
  fix; 148 active infrastructure tests passed with four explicitly ignored,
  and all other workspace suites passed. The first run exposed only the new
  synchronous `PgPool::connect_lazy` test context; converting that test to
  `#[tokio::test]` removed the regression.
- `cargo check -p erpx-console --no-default-features --features web` — PASS.
- `cargo check -p erpx-console --no-default-features --features desktop` —
  PASS.
- `bun run lint` — PASS with four pre-existing warnings in
  `infra/scripts/start-n8n.js`, zero errors.
- `bunx tsc --noEmit` — PASS.
- `curl -fsS http://127.0.0.1:3004/operations` — PASS.
- `pg_isready -h /tmp -p 5432` — PASS.

Folio remains operational; no migration cutover, command-owner switch, or
Folio write retirement occurred. G1-G14 gates remain open.

## Entry 2026-07-25 — G8-FIN-033 Finance Chart protobuf and command transport

Main froze an additive `SaveFinanceChart` protobuf contract with bounded
typed JSON input, explicit expected-version binding, shared command context,
and a metadata-only typed receipt. The authenticated `FinanceService`
implementation requires active identity, exact actor/context and company
scope binding, `finance.write` RBAC, Finance entitlement/billing/SoD
authorization, tenant write-database routing, atomic `SqlxFinanceChartStore`
persistence, correlation metadata, and a customer audit event. Platform and
subtree mutation scopes are rejected; no read pool is opened.

Independent evidence: `cargo fmt --all --check` — PASS; `cargo check
-p erpx-proto` — PASS; `cargo test -p erpx-proto` — PASS; `rustfmt --edition
2024 --check apps/server/src/main.rs` — PASS; `cargo check -p erpx-server` —
PASS; `cargo test -p erpx-server` — 72 PASS; strict server Clippy — PASS.
Live auth/database transport proof, Chart read projection, journal-posting
adoption, and the broader G8 gate remain open.

## Entry 2026-07-26 — G8-FIN-035 authoritative Finance journal posting

The existing tenant-write Finance journal command now reconstructs its
posting chart from normalized `finance_chart_of_accounts` and
`finance_chart_accounts` rows under the holding-scoped write transaction. It
does not trust legacy caller chart structure for account identity, scope,
status, or hierarchy. Missing charts/accounts, inactive accounts, and
holding/company mismatches fail closed. Successful posting inserts an
immutable `finance_posted_journal_account_usage` fact for every used account
in the same transaction as the journal, snapshot, outbox event, and command
idempotency receipt; replay returns the original journal without new facts.
Migration 0043 adds forced holding RLS, immutable usage rows, and a normalized
usage-aware structural account guard.

Independent evidence: migration contract test — PASS; focused Finance
persistence test — 1 PASS, 1 explicitly ignored; strict infrastructure
Clippy — PASS; fresh disposable PostgreSQL live proof
`live_journal_posting_is_authoritative_scoped_idempotent_and_atomic` — PASS,
covering exact replay counts, missing chart, inactive account, scope mismatch,
missing account, atomic usage-trigger rollback, and structural immutability
after posting. The disposable database was dropped and verified absent.
Legacy chart migration/reconciliation, provider integration, and the broader
G8 gate remain open.

## Ledger correction 2026-07-26 — append-only ordering note

The G8-FIN-034 entry was appended after the G8-FIN-035/036 integration-wave
entry because this ledger is append-only. The verification wave immediately
above the entry includes the independently rerun evidence for the integrated
Finance Chart, journal-posting, and scheduler changes; no historical entry was
reordered or removed.

## Entry 2026-07-26 — G8-FIN-036 Finance Chart durable scheduler adapter

The Finance Chart projection worker now implements the established durable
multi-tenant scheduler executor contract. Scheduler jobs use the
`finance_chart` projection name, bounded page sizes, validated
aggregate/version cursors, explicit incremental versus rebuild handling, and
redacted retry/permanent failure mapping. `apps/worker/src/main.rs` accepts
`ERPX_TENANT_PROJECTION_KIND=finance_chart` and composes the existing control
plane claim/lease/retry/dead-letter handler with the tenant router; no tenant
credentials or database URLs enter the worker adapter.

Independent evidence: `cargo fmt --all --check` — PASS; four focused Finance
Chart worker unit tests — PASS; strict worker Clippy — PASS; worker check —
PASS; Finance Chart server scope test — PASS. Full workspace integration will
be rerun after the next packet. Scheduler deployment registration and the
broader G8 gate remain open.

## Verification wave 2026-07-26 — G8-FIN-035/036 integration

- `cargo fmt --all --check` — PASS.
- `cargo clippy --workspace --all-targets -- -D warnings` — PASS.
- `cargo test --workspace --quiet` — PASS; the workspace suites completed
  with no failures, including 148 active infrastructure tests and the new
  Finance posting/scheduler coverage.
- `cargo check -p erpx-console --no-default-features --features web` — PASS.
- `cargo check -p erpx-console --no-default-features --features desktop` —
  PASS.
- `bun run lint` — PASS with four pre-existing warnings in
  `infra/scripts/start-n8n.js`, zero errors.
- `bunx tsc --noEmit` — PASS.
- `curl -fsS http://127.0.0.1:3004/operations` — HTTP 307 PASS.
- `pg_isready -h /tmp -p 5432` — accepting connections PASS.

Folio remains operational; no command-owner switch, migration cutover, or
write retirement occurred. G1-G14 gates remain open.

## Entry 2026-07-26 — G8-FIN-034 Finance Chart read projection and transport

AG-FIN delivered the tenant-read Chart of Accounts sidecar. Migration 0035
stores normalized chart/account rows, complete source-envelope metadata,
projection checkpoints, and idempotent event receipts with indexes and forced
holding RLS. `SqlxFinanceChartProjectionStore` reads only the tenant-write
outbox, validates contract/event/aggregate identity, scope, bounded typed
payloads and hierarchy, rejects version gaps, handles replay idempotently,
and clears/rebuilds the disposable read model atomically. The worker routes
the write/read pair explicitly and exposes bounded incremental/rebuild runs
with redacted errors. `ListFinanceCharts` is an additive typed protobuf and
server read boundary: it requires an attributed query context, contained
company scope, `finance.read`, customer scope membership, a tenant read route,
bounded opaque continuation, scalar-only source metadata, and a customer audit
event; it never opens or returns tenant-write data.

Independent evidence: focused infrastructure projection test — 1 PASS, 1
explicitly ignored; worker projection tests — 2 PASS; strict infrastructure,
worker, proto, and server Clippy — PASS; proto tests — PASS; server tests — 72
PASS; fresh disposable two-database live projection proof — PASS, covering
projection, replay, scope isolation, version-gap rejection, clear/rebuild,
and forced RLS, with both databases dropped and verified absent. Full
integration evidence after this packet: `cargo fmt --all --check` — PASS;
`cargo clippy --workspace --all-targets -- -D warnings` — PASS;
`cargo test --workspace --quiet` — PASS; Dioxus console web and desktop
checks — PASS; `bun run lint` — PASS with four pre-existing n8n warnings and
zero errors; `bunx tsc --noEmit` — PASS; Folio `/operations` — HTTP 307; local
PostgreSQL readiness — PASS. ESLint now explicitly ignores the already
gitignored generated `erpx/target/**` tree so Dioxus build artifacts do not
become application lint input. Journal-posting adoption, durable scheduler
registration, and the broader G8 gate remain open.

## Entry 2026-07-26 — G8-FIN-037 Finance subledger durable scheduler

AG-FIN added the `finance_subledger` tenant projection executor. It uses only
the existing `SqlxFinanceSubledgerStore` source-event and read-projection
methods, supports bounded incremental and rebuild jobs, validates scheduler
projection names, tenant/holding references, allowed aggregate cursors, page
sizes, and source-fence shapes, and maps malformed jobs to permanent failure
while redacting retryable routing/database failures. Worker runtime
registration accepts `ERPX_TENANT_PROJECTION_KIND=finance_subledger` and a
bounded `ERPX_FINANCE_SUBLEDGER_PROJECTION_PAGE_SIZE`. Main-agent review added
explicit allow-list validation for cursor aggregate types. No migrations,
credentials, or database URLs were added. Source-fenced replacement-read
activation remains open because the existing store lacks a source
high-watermark API.

Independent handoff evidence: four focused worker tests — PASS; worker check
— PASS; strict worker Clippy — PASS; workspace format check — PASS.

## Entry 2026-07-26 — G8-FIN-038 Finance subledger read transport

The tenant-read Finance subledger store now exposes bounded deterministic
keyset pages with explicit holding/company/branch predicates, opaque cursors,
aggregate identity checks, and stored-column/payload scope consistency
validation. The additive `ListFinanceSubledgerReadRows` RPC is authenticated,
requires `finance.read` and customer scope membership, rejects platform and
composite scopes plus cross-company filters, returns bounded typed metadata
and aggregate payloads, routes only to the tenant read database, and records a
customer-visible audit event. No write pool is opened by the read boundary.

Independent focused evidence: two server transport security tests — PASS;
strict infrastructure and server Clippy — PASS; disposable two-database
`live_finance_subledger_read_is_projected_scoped_paginated_and_rls_forced`
proof — PASS. The disposable databases were dropped and verified absent.
Authenticated gRPC transport proof and the broader G8 gate remain open.

## Verification wave 2026-07-26 — G8-FIN-037/038 integration

- `cargo fmt --all --check` — PASS.
- `cargo clippy --workspace --all-targets -- -D warnings` — PASS.
- `cargo test --workspace --quiet` — PASS; no failures, with the new
  subledger scheduler/read coverage included and disposable tests explicitly
  ignored unless invoked with database URLs.
- `cargo check -p erpx-console --no-default-features --features web` — PASS.
- `cargo check -p erpx-console --no-default-features --features desktop` —
  PASS.
- `bun run lint` — PASS with four pre-existing warnings in
  `infra/scripts/start-n8n.js`, zero errors.
- `bunx tsc --noEmit` — PASS.
- Folio `/operations` — HTTP 307 PASS.
- `pg_isready -h /tmp -p 5432` — accepting connections PASS.

Folio remains operational. No command-owner switch, migration cutover, or
write retirement occurred; source-fenced subledger replacement activation,
authenticated live gRPC proof, and the broader G8-G14 requirements remain
open.

## Entry 2026-07-26 — G8-FIN-039 Finance subledger source fences

The subledger store now exposes a holding-scoped, read-only source-event
high-watermark over the closed Finance aggregate allow-list. Rebuild jobs may
carry a source fence: a mismatch before clearing or source movement after
replay returns `CandidateSourceMoved`; incremental source fences remain
permanently rejected. The source watermark and replay path use existing
tenant-write/read routing and introduce no schema or credential changes.

Independent handoff evidence: six focused worker tests — PASS; 148 active
infrastructure tests — PASS; strict worker and infrastructure Clippy — PASS;
workspace formatting — PASS. Candidate replacement-read activation remains
open because this projection is not yet registered with the generic candidate
activation mapping.

## Entry 2026-07-26 — G8-FIN-041 Finance close durable scheduler

The existing Finance close projection runner is now selectable as the durable
`finance_close` tenant scheduler kind. Its worker adapter validates bounded
page sizes, tenant/holding references, UUID/version cursors, and incremental
versus unfenced rebuild mode; it routes through the physical tenant write/read
pools and redacts retryable projection failures. Runtime configuration uses
`ERPX_FINANCE_CLOSE_PROJECTION_PAGE_SIZE`. Close source-fenced jobs remain
fail-closed because the close store has no source high-watermark API.

Independent handoff evidence: five focused worker tests — PASS; strict worker
Clippy — PASS; worker check — PASS; workspace formatting — PASS. No live
database proof or authenticated transport proof was claimed.

## Entry 2026-07-26 — G8-FIN-040 shared Finance UI contract

AG-UI added the target-neutral shared Dioxus Finance module with ten stable
routes, Thai/English labels, bounded opaque identifiers, scalar-only display
models, projection freshness metadata, and explicit loading/empty/ready/
denied/stale/read-only/conflict/offline/error states. Structured values,
secrets, tokens, URLs, and infrastructure markers are rejected at the
presentation boundary. The module performs no networking, protobuf, database,
authorization, or mutation work; live adapters and target journeys remain
open.

Independent handoff evidence: four focused UI tests — PASS; strict UI
Clippy — PASS; console web and desktop checks — PASS; workspace format check —
PASS.

## Verification wave 2026-07-26 — G8-FIN-039/040/041 integration

- Fresh disposable two-database
  `live_finance_subledger_read_is_projected_scoped_paginated_and_rls_forced`
  proof — PASS, including the new source high-watermark assertion; both
  databases were dropped and verified absent.
- `cargo fmt --all --check` — PASS.
- `cargo clippy --workspace --all-targets -- -D warnings` — PASS.
- `cargo test --workspace --quiet` — PASS; no failures, including 148 active
  infrastructure tests, 90 active worker tests, and the four Finance UI tests.
- `cargo test -p erpx-ui --test finance` — 4 PASS.
- `cargo check -p erpx-console --no-default-features --features web` — PASS.
- `cargo check -p erpx-console --no-default-features --features desktop` —
  PASS.
- `bun run lint` — PASS with four pre-existing warnings in
  `infra/scripts/start-n8n.js`, zero errors.
- `bunx tsc --noEmit` — PASS.
- Folio `/operations` — HTTP 307 PASS.
- `pg_isready -h /tmp -p 5432` — accepting connections PASS.

Folio remains operational. No command-owner switch, migration cutover, or
write retirement occurred. Close/subledger candidate activation, authenticated
live gRPC proof, remaining Finance workflows, and the broader G0-G14 gates
remain open.

## Entry 2026-07-26 — G8-FIN-042 Finance close source fences

The Finance close tenant projection now exposes a holding-scoped source-event
high-watermark. Rebuild jobs carrying a source fence verify it before clearing
the candidate and after replay; source movement maps to terminal
`CandidateSourceMoved`. Incremental jobs with source fences remain rejected,
unfenced rebuilds remain supported, malformed jobs remain permanent failures,
and operational failures remain redacted/retryable. No migration or generic
candidate-activation change was included.

Independent handoff evidence: six focused worker tests — PASS; two focused
infrastructure projection tests — PASS; strict worker and infrastructure
Clippy — PASS; worker check — PASS; workspace format check — PASS. No live
PostgreSQL proof was claimed. Candidate activation and the broader G8 gate
remain open.

## Entry 2026-07-26 — G8-FIN-043 authoritative journal handoff enforcement

The Finance subledger write boundary now loads the immutable posted journal
inside the same tenant-write transaction whenever an aggregate carries a
`FinanceJournalHandoff`, and calls the domain journal-binding validation. It
rejects missing, malformed, immutable-state, journal, period, holding,
company, accounting-date, and source-record mismatches before aggregate
snapshot/outbox/receipt persistence. Allocation-batch writes revalidate the
locked stored and next payment/invoice states before each mutation, retaining
all-or-nothing behavior and exact request replay. Draft/unposted records may
remain without a handoff.

Independent handoff evidence: focused unit cases — PASS; focused
infrastructure test target — 2 PASS with one live test explicitly ignored by
default; strict infrastructure Clippy — PASS; workspace format check — PASS.
Main-agent fresh disposable PostgreSQL proof — PASS, covering valid binding,
exact idempotent replay, missing and mismatched journals, allocation-batch
rejection, zero-row atomicity, and cleanup with the disposable database
dropped and verified absent. No migrations, protobuf, transport, worker/UI,
or Folio changes were made. Authenticated transport and complete AP/AR/cash
workflow evidence remain open.

## Verification wave 2026-07-26 — G8-FIN-042/043 integration

- `cargo fmt --all --check` — PASS.
- `cargo clippy --workspace --all-targets -- -D warnings` — PASS.
- `cargo test --workspace --quiet` — PASS; no failures. The workspace
  included 148 active infrastructure tests, the Finance handoff target with
  two active unit tests and one explicitly ignored live test, and all active
  worker/UI/server/domain suites.
- `cargo check -p erpx-console --no-default-features --features web` — PASS.
- `cargo check -p erpx-console --no-default-features --features desktop` —
  PASS.
- Fresh disposable PostgreSQL handoff proof — PASS; the database was dropped
  and verified absent.
- `bun run lint` — PASS with four pre-existing warnings in
  `infra/scripts/start-n8n.js`, zero errors.
- `bunx tsc --noEmit` — PASS.
- Folio `/operations` — HTTP 307 PASS.
- `pg_isready -h /tmp -p 5432` — accepting connections PASS.

Folio remains operational. ERPX remains below completion: candidate read
activation, authenticated live gRPC proof, remaining Finance workflows, and
the broader G0-G14 requirements are open. No command-owner switch, migration
cutover, or Folio write retirement occurred.

## Entry 2026-07-26 — G8-FIN-044 Finance candidate activation adapters

AG-FIN added routed execution implementations for Finance subledger and
Finance close. Each adapter validates the exact tenant/holding binding and
physical write/read sides, consumes only already-routed pools, supports
bounded incremental and candidate-fenced rebuild replay, rejects fenced
incremental jobs, preserves redacted retryable failures, and returns
`CandidateSourceMoved` on source drift. The shared scheduler now maps
Operations, Finance Banking, Finance subledger, and Finance close to their
source high-watermarks. Main integration added a Finance composition that
preserves unfenced rebuilds while candidate rebuilds retain generic source
locking and activation.

Independent handoff evidence: Finance subledger focused tests — 8 PASS;
Finance close focused tests — 9 PASS; shared scheduler focused tests — 9
PASS; infrastructure provisioning focused tests — 10 PASS; worker check and
strict Clippy — PASS; workspace format — PASS. No migrations, transport,
server, UI, or Folio changes were made by the support packets. Live candidate
read activation remains open.

## Verification wave 2026-07-26 — G8-FIN-044 integration

- `cargo fmt --all --check` — PASS.
- `cargo clippy --workspace --all-targets -- -D warnings` — PASS.
- `cargo test --workspace --quiet` — PASS; no failures, with 148 active
  infrastructure tests and the expanded worker scheduler/projector suites.
- Finance subledger projector — 8 focused tests PASS.
- Finance close projector — 9 focused tests PASS.
- Shared tenant scheduler — 9 focused tests PASS.
- `cargo check -p erpx-console --no-default-features --features web` — PASS.
- `cargo check -p erpx-console --no-default-features --features desktop` —
  PASS.
- Folio `bun run lint` — PASS with four pre-existing n8n warnings and zero
  errors.
- Folio `bunx tsc --noEmit` — PASS.
- Folio `/operations` — HTTP 307 PASS.
- `pg_isready -h /tmp -p 5432` — accepting connections PASS.

Folio remains operational. Candidate rebuild execution is now wired, but no
command-owner switch, migration cutover, or Folio write retirement occurred;
live candidate activation and the broader G0-G14 requirements remain open.

## Entry 2026-07-26 — G8-FIN-046 Finance write transport pool isolation

The authenticated `SaveSubledger` and `SaveAllocationBatch` handlers now
route only the tenant write database. They pass the write pool to the
write-only Finance persistence operations and no longer open or require the
tenant read route for mutation commands. Read projection transports retain
their separate tenant-read-only route. This preserves physical CQRS boundary
behavior for the Finance command path.

Independent evidence: server check — PASS; strict server Clippy — PASS; 75
server binary unit tests — PASS; workspace checks remain green. Authenticated
live gRPC/database transport evidence and the broader Finance workflow gates
remain open.

## Entry 2026-07-26 — G8-FIN-045 Finance candidate activation proof

Added only `erpx/apps/worker/tests/finance_candidate_activation.rs`. The
ignored integration test uses unique disposable control, tenant-write,
tenant-read, and candidate PostgreSQL databases and existing public
provisioning/router/projector APIs. It seeds one Finance payable invoice,
captures the authoritative source event and watermark, starts a generation,
rebuilds through `RoutedTenantProjectionExecutor`, and verifies the candidate
row, physical database, checkpoint event/version, source watermark, activated
switch generation, and tenant registry target. Cleanup terminates connections
and drops all generated databases; a follow-up catalog query found no
`erpx_fin045_*` databases. No URLs, credentials, or secrets are printed.

Exact focused evidence:
`cargo test -p erpx-worker --test finance_candidate_activation -- --ignored --exact finance_subledger_candidate_rebuild_activates_generation_and_projects_checkpoint`
— 1 passed, 0 failed. Targeted Clippy and workspace formatting also passed.
The test does not claim durable scheduler `claim_due`/lease competition or
full worker-loop evidence. No production behavior, migration, protobuf,
server, UI, or Folio change was made for this packet.

## Entry 2026-07-26 — G5-STRIPE-LINT-001 typed provider DTO lint remediation

The low-level Stripe transport now has narrow documented `dead_code`
allowances on entitlement-feature and billing-meter request/response DTOs
whose construction belongs to external catalog integrations or serde. The
change is lint-only: endpoint paths, form encoding, API credentials,
redaction, and response parsing are unchanged. This removes the previously
observed 12 Stripe dead-code errors from the required workspace Clippy gate.

Independent evidence: infrastructure Clippy — PASS; 151 infrastructure unit
tests — PASS; workspace Clippy — PASS. Stripe catalog/provider adoption and
the broader G5 gate remain open.

## Verification wave 2026-07-26 — G8-FIN-045 and G5-STRIPE-LINT-001

- `cargo fmt --all --check` — PASS.
- `cargo clippy --workspace --all-targets -- -D warnings` — PASS.
- `cargo test --workspace --quiet` — PASS; no failures, including 151 active
  infrastructure tests and the ignored Finance candidate proof target.
- `cargo check -p erpx-console --no-default-features --features web` — PASS.
- `cargo check -p erpx-console --no-default-features --features desktop` —
  PASS.
- `bun run lint` — PASS with four pre-existing warnings in
  `infra/scripts/start-n8n.js`, zero errors.
- `bunx tsc --noEmit` — PASS.
- Folio `/operations` — HTTP 307 PASS.
- `pg_isready -h /tmp -p 5432` — accepting connections PASS.

Folio remains operational. No command-owner switch, migration cutover, or
Folio write retirement occurred. Authenticated live gRPC proof, the durable
worker-loop claim path, remaining Finance workflows, and the broader G0-G14
requirements remain open.

## Verification wave 2026-07-26 — G8-FIN-045/G5-BIL-045 integration

- `cargo fmt --all --check` — PASS.
- `cargo clippy --workspace --all-targets -- -D warnings` — PASS.
- `cargo test --workspace --quiet` — PASS; no failures. The workspace
  included the Finance candidate integration target (ignored by default), 165
  infrastructure unit tests, 75 server binary tests, 98 worker tests, and all
  other active domain/UI/application suites.
- `cargo check -p erpx-console --no-default-features --features web` — PASS.
- `cargo check -p erpx-console --no-default-features --features desktop` —
  PASS.
- Focused disposable Finance candidate activation — PASS; 1 ignored live
  test against fresh PostgreSQL databases, with cleanup verified by an empty
  `erpx_fin045_*` database query.
- Focused Stripe client tests — PASS; 10 tests.
- `bun run lint` — PASS with four pre-existing warnings in
  `infra/scripts/start-n8n.js`, zero errors.
- `bunx tsc --noEmit` — PASS.
- Folio `/operations` — HTTP 307 PASS.
- `pg_isready -h /tmp -p 5432` — accepting connections PASS.

Folio remains operational. These packets do not complete Billing, Finance,
or the ERPX program: authenticated live gRPC/database transport, full Stripe
catalog and billing reconciliation, remaining Finance workflows, later ERP
contexts, migration cutover, and Folio write retirement remain open.

## Entry 2026-07-26 — G8-FIN-045 Finance candidate activation proof

The main agent independently reviewed and executed the disposable candidate
activation proof. It uses only public worker/infrastructure APIs, seeds one
authoritative Finance subledger event in a tenant write database, rebuilds a
replacement read candidate through the routed scheduler executor, verifies
the control-plane switch is `activated`, and verifies the projected row and
checkpoint are present in the candidate's physical read database. The test
creates separate control-write/control-read, tenant-write/tenant-read, and
candidate-read databases and drops them in cleanup.

Evidence: `audits/G8-FIN-045/README.md`; compile-only test — PASS; ignored
live integration test — PASS (1 test, 1.05s); strict test-target Clippy —
PASS; post-run query found no `erpx_fin045_*` databases. Candidate activation
is evidenced for Finance subledger, but authenticated live gRPC transport,
other projections, and the broader Finance/product gates remain open.

## Entry 2026-07-26 — G5-BIL-045 typed Stripe entitlement and meter resources

The bounded typed Stripe HTTP adapter now includes Entitlement Feature and
Billing Meter endpoint variants, validated request models, typed list/create/
retrieve methods, typed response models, and focused redaction/bounds/
idempotency/path/decoding tests. It remains a provider boundary and does not
claim catalog publication, webhook reconciliation, billing lifecycle, or
entitlement activation.

Independent evidence: infrastructure Stripe focused tests — PASS (10 tests);
strict infrastructure Clippy — PASS; workspace formatting — PASS after the
main-agent review. No live Stripe call or production credential was used.

## Entry 2026-07-26 — G5-BIL-046 remote Stripe catalog reconciliation

The existing typed Stripe catalog provider now performs a bounded remote
reconciliation path. It lists Product and Price pages with a fixed page size
and page cap, validates pinned API/account response envelopes, selects one
coherent ERPX metadata revision, checks Product/Price relationships, rebuilds
typed provider identifiers and normalized prices, and returns deterministic
catalog reconciliation data. Main-agent review repaired pagination ownership,
list-envelope decoding, recorder trait coverage, and strict-Clippy issues.

Evidence: `audits/G5-BIL-046/README.md`; commercial focused tests — 23 PASS
with 4 ignored; infrastructure strict Clippy — PASS; workspace format —
PASS. No Stripe network call, credential, migration, worker, or server
change was used. Worker scheduling, legacy metadata onboarding, webhook
replay controls, and full Billing runtime adoption remain open.

## Entry 2026-07-26 — G10-MKT-046 bounded Marketing journey domain

The domain packet adds a tenant-scoped Journey graph and resume-safe cursor.
Lifecycle transitions enforce the required campaign states and expected
aggregate versions. Node validation bounds identifiers, wait durations,
channel payloads, branch reachability/order, CRM scope containment, conversion
values, and secret/URL/token-like text. Cursor advancement emits stable
idempotency facts, handles waits and branch decisions, and supports pause/
resume without performing channel, integration, or CRM side effects.

Evidence: `audits/G10-MKT-046/README.md`; focused journey tests — 3 PASS;
strict domain Clippy — PASS; workspace format — PASS. Main-agent review
repaired the large CRM handoff variant and collapsed strict-Clippy branches.
Persistence, durable journey worker execution, consent-at-schedule/delivery,
provider integrations, and Marketing/CRM completion remain open.

## Verification wave 2026-07-26 — G5-BIL-046/G10-MKT-046 integration

- `cargo fmt --all --check` — PASS.
- `cargo clippy --workspace --all-targets -- -D warnings` — PASS.
- `cargo test --workspace --quiet` — PASS; no failures. The workspace
  included 168 active domain tests, 156 active infrastructure tests with 4
  ignored, 75 server binary tests, 98 worker tests, and all other active
  application/UI suites.
- `cargo check -p erpx-console --no-default-features --features web` — PASS.
- `cargo check -p erpx-console --no-default-features --features desktop` —
  PASS.
- Focused commercial reconciliation — PASS; 23 active tests and 4 ignored.
- Focused Marketing journey domain tests — PASS; 3 tests.
- `bun run lint` — PASS with four pre-existing warnings in
  `infra/scripts/start-n8n.js`, zero errors.
- `bunx tsc --noEmit` — PASS.
- Folio `/operations` — HTTP 307 PASS.
- `pg_isready -h /tmp -p 5432` — accepting connections PASS.

Folio remains operational. Stripe worker scheduling, metadata onboarding,
webhook replay controls, journey persistence/execution, consent delivery,
remaining Finance/Billing work, later ERP contexts, migration cutover, and
Folio write retirement remain open; the persistent ERPX goal remains active.

## Entry 2026-07-26 — G5-BIL-047 fail-closed remote Price ownership

Main-agent review tightened the remote Stripe catalog reconciliation boundary:
only active Prices carrying a complete, coherent ERPX revision/provider/item
binding are eligible for adoption. Unmanaged active Prices are ignored and
conflicting item metadata fails closed. This prevents accidental adoption of
third-party Prices attached to an ERPX Product.

Evidence: `audits/G5-BIL-047/README.md`; focused commercial tests — 24 active
passed with 4 ignored; strict infrastructure Clippy and workspace formatting
passed. No Stripe network call, credential, migration, worker, server, or
Folio write was used. The broader Billing gate remains open.

## Entry 2026-07-26 — G10-MKT-047 durable Journey tenant-write persistence

Main-agent implementation delivered the Marketing Journey authoritative
tenant-write boundary. Migration `0044_marketing_journeys.sql` adds bounded
journey state and command-idempotency tables with forced RLS. The typed
`SqlxMarketingJourneyStore` validates the shared command context and tenant
scope, persists create and lifecycle transitions through the routed holding
write pool, records the complete event envelope, updates the generic
aggregate snapshot, and returns exact replay receipts. The domain’s initial
Draft version is positive one so the creation snapshot and outbox event obey
the existing positive aggregate-version database contract.

Support handoff Sartre produced no files and was closed; main owned the
implementation and independently reviewed it. No server, worker, protobuf,
client, or Folio behavior was changed by this packet.

Evidence: `audits/G10-MKT-047/README.md`; focused migration/persistence tests
and strict targeted Clippy passed. Disposable live PostgreSQL evidence passed
create, exact replay, transition, transition replay, shared envelope fields,
forced RLS, and cross-holding isolation; cleanup queries found no packet rows.

## Verification wave 2026-07-26 — G10-MKT-047 integration

- `cargo fmt --all --check` — PASS.
- `cargo clippy --workspace --all-targets -- -D warnings` — PASS.
- `cargo test --workspace --quiet` — PASS; no failures, including 168 active
  domain tests, 153 active infrastructure tests with 4 ignored, 75 server
  tests, and 97 active worker tests with 1 ignored.
- `cargo check -p erpx-console --no-default-features --features web` — PASS.
- `cargo check -p erpx-console --no-default-features --features desktop` —
  PASS.
- Focused Marketing Journey persistence contract/live tests — PASS; the live
  test ran against PostgreSQL and cleaned its unique holding rows.
- `bun run lint` — PASS with four pre-existing warnings in
  `infra/scripts/start-n8n.js`, zero errors.
- `bunx tsc --noEmit` — PASS.
- Folio `/operations` — HTTP 307 PASS.
- `pg_isready -h /tmp -p 5432` — accepting connections PASS.

Folio remains operational. Durable journey worker execution, read projection,
consent-at-schedule/delivery enforcement, provider integrations, CRM handoff,
remaining Finance/Billing work, later ERP contexts, migration cutover, and
Folio write retirement remain open; the persistent ERPX goal remains active.

## Verification wave 2026-07-26 — G5-BIL-047 integration

- `cargo fmt --all --check` — PASS.
- `cargo clippy --workspace --all-targets -- -D warnings` — PASS.
- `cargo test --workspace --quiet` — PASS; no failures, including 168 active
  domain tests, 154 active infrastructure tests with 4 ignored, 75 server
  tests, and 97 active worker tests with 1 ignored.
- `cargo check -p erpx-console --no-default-features --features web` — PASS.
- `cargo check -p erpx-console --no-default-features --features desktop` —
  PASS.
- Focused commercial reconciliation — PASS; 24 active tests and 4 ignored.
- `bun run lint` — PASS with four pre-existing warnings in
  `infra/scripts/start-n8n.js`, zero errors.
- `bunx tsc --noEmit` — PASS.
- Folio `/operations` — HTTP 307 PASS.
- `pg_isready -h /tmp -p 5432` — accepting connections PASS.

Folio remains operational. Billing provider scheduling, metadata onboarding,
webhook replay controls, journey execution, consent delivery, remaining
Finance/Billing work, later ERP contexts, migration cutover, and Folio write
retirement remain open; the persistent ERPX goal remains active.

## Entry 2026-07-26 — G10-MKT-048 durable Marketing Journey execution

Main-agent implementation delivered migration `0045_marketing_journey_execution.sql`,
the tenant-write `SqlxMarketingJourneyExecutionStore`, and an explicit worker
handler. Version-zero cursor seeds are durable without violating the positive
outbox version contract. Claims use bounded `FOR UPDATE SKIP LOCKED` leases;
expired leases can be resumed, stale owners fail closed, retryable failures
are delayed, and permanent failures are retained as dead letters. Execution,
pause, and resume commit cursor state, generic snapshot, complete event
envelope, and idempotency receipt atomically. Worker command identity is
deterministic for exact replay. The server worker binary routes the write pool
from the control-plane registry for an explicit tenant/holding binding; the
default decision source refuses branch nodes without a permission-aware
provider.

No support-agent files were integrated for this packet after the dependency-
ready handoff stalled; main independently reviewed and completed the bounded
implementation. No Folio behavior or Folio writes were changed.

Evidence: `audits/G10-MKT-048/README.md`; migration/unit tests, focused worker
tests, strict package Clippy, and disposable live PostgreSQL evidence passed.
The live proof covered expired-lease takeover, stale-owner rejection, exact
worker replay, pause/resume, outbox/idempotency counts, forced RLS, and
cross-holding isolation. The one earlier failed fixture was removed by its
exact unique reference; the successful run left no packet rows.

## Verification wave 2026-07-26 — G10-MKT-048 integration

- `cargo fmt --all --check` — PASS.
- `cargo clippy --workspace --all-targets -- -D warnings` — PASS.
- `cargo test --workspace --quiet` — PASS; no failures, including 168 domain
  tests, 155 active infrastructure tests with 4 ignored, 75 server tests,
  and 99 active worker tests with 1 ignored.
- `cargo check -p erpx-console --no-default-features --features web` — PASS.
- `cargo check -p erpx-console --no-default-features --features desktop` —
  PASS.
- Focused live execution proof — PASS against PostgreSQL; unique fixture rows
  were cleaned.
- `bun run lint` — PASS with four pre-existing warnings in
  `infra/scripts/start-n8n.js`, zero errors.
- `bunx tsc --noEmit` — PASS.
- Folio `/operations` — HTTP 307 PASS.
- `pg_isready -h /tmp -p 5432` — accepting connections PASS.

Folio remains operational. Journey read projection, consent enforcement,
provider delivery, CRM side effects, Marketing UI, remaining ERP modules,
migration cutover, and Folio write retirement remain open; the persistent ERPX
goal remains active.

## Entry 2026-07-26 — G10-MKT-049 consent and suppression delivery enforcement

Main-agent implementation added independent acceptance coverage for the
authoritative Marketing delivery claim boundary. The live PostgreSQL proof
persisted a profile and granted consent, queued deliveries, then applied a
late suppression, a revocation, and a consent expiring exactly at the claim
time. Suppressed work became terminal `suppressed`; revoked and expired work
retained no provider claim or authorization. The test also asserts the
append-only consent and forced-RLS migration contracts.

The support handoff for this test-only packet stalled and was closed without
files; main independently completed and reviewed the allowed test boundary.
No production, migration, worker, transport, or Folio behavior changed.

Evidence: `audits/G10-MKT-049/README.md`; focused contract/live tests and
strict infrastructure Clippy passed. Unique fixture rows were cleaned, with
zero remaining `marketing-consent-*` delivery or consent rows.

## Verification wave 2026-07-26 — G10-MKT-049 integration

- `cargo fmt --all --check` — PASS.
- `cargo clippy --workspace --all-targets -- -D warnings` — PASS.
- `cargo test --workspace --quiet` — PASS; no failures, including the new
  Marketing consent/suppression contract test.
- `cargo check -p erpx-console --no-default-features --features web` — PASS.
- `cargo check -p erpx-console --no-default-features --features desktop` —
  PASS.
- Focused live consent/suppression claim proof — PASS against PostgreSQL;
  exact packet fixture rows were cleaned.
- `bun run lint` — PASS with four pre-existing warnings in
  `infra/scripts/start-n8n.js`, zero errors.
- `bunx tsc --noEmit` — PASS.
- Folio `/operations` — HTTP 307 PASS.
- `pg_isready -h /tmp -p 5432` — accepting connections PASS.

Folio remains operational. Marketing provider integrations, campaign and
journey effects, CRM attribution, remaining ERP contexts, migration cutover,
and Folio write retirement remain open; the persistent ERPX goal remains
active.

## Verification wave 2026-07-26 — G10-MKT-059 full integration

- `cargo fmt --all --check` — PASS.
- `cargo clippy --workspace --all-targets -- -D warnings` — PASS.
- `cargo test --workspace --quiet` — PASS; 173 domain, 158 infrastructure
  with 4 ignored, 79 server, and 108 worker tests passed; worker has 1
  ignored.
- `cargo check -p erpx-console --no-default-features --features web` — PASS.
- `cargo check -p erpx-console --no-default-features --features desktop` — PASS.
- `bun run lint` — PASS with four pre-existing warnings in
  `infra/scripts/start-n8n.js`, zero errors.
- `bunx tsc --noEmit` — PASS.
- Folio `/operations` — HTTP 307 PASS; development server remains on port 3004.
- `pg_isready -h /tmp -p 5432` — accepting connections PASS.
- `git diff --check` — PASS.

The focused G10-MKT-059 evidence and this full wave are supplemental
append-only records; the persistent ERPX goal remains active.

## Entry 2026-07-26 — G10-MKT-059 Marketing administration transport and application port

Main integrated additive Tonic commands for preview issue/revoke, domain challenge issue/verify, and domain binding. The server enforces authentication, command context, explicit tenant scope, Marketing entitlement/RBAC, routed SQLx persistence, customer-visible audit, and redacted errors. Preview/domain credentials are generated with 256 bits of OS randomness and only their SHA-256 digests enter tenant write state. AG-MKT-APP independently delivered the application-layer typed commands, receipt DTO, and object-safe Marketing write port in its exclusive application file; main reviewed the validation and contract boundary.

Evidence: `audits/G10-MKT-059/README.md`. The SQLx adapter is not yet mapped behind the application port; live all-command transport proof, public abuse controls, semantic validation, SSR/UI, MinIO, host-only discovery, durable retry/DLQ, remaining Marketing/ERP scope, migration/cutover, and Folio write retirement remain open.

## Verification wave 2026-07-26 — G10-MKT-059 focused integration

- `cargo fmt --all --check` — PASS.
- `cargo clippy -p erpx-application --all-targets -- -D warnings` — PASS.
- `cargo test -p erpx-application --quiet` — PASS; 11 passed.
- `cargo clippy -p erpx-server --all-targets -- -D warnings` — PASS.
- `cargo test -p erpx-server --quiet` — PASS; 79 passed.
- `cargo check -p erpx-proto` — PASS.
- Secret-boundary unit proof — PASS; 32 random bytes, 64-hex-character raw secret, and 64-hex-character stored digest.

Folio remains operational. The persistent ERPX goal remains active.

## Append-only integration record — G6-G12-UI-012 authenticated backup history (true EOF)

The Dioxus console consumes the audited `ListTenantBackupJobs` read for
`/platform/backups` through WebAssembly gRPC-Web and native Tonic. It uses the
authenticated tenant registry, caps the tenant fan-out at 100, and requests at
most 100 history rows per tenant. Only bounded job state is rendered; backup
object references, restore targets, physical database keys, credentials, and
raw errors remain excluded. No mutation or restore control is exposed.

Evidence: `audits/G6-G12-UI-012/README.md`.

## Verification wave 2026-07-26 — G6-G12-UI-012 focused integration (true EOF)

- `cargo fmt --all --check` — PASS.
- `cargo test -p erpx-console` — PASS; 20 tests.
- `cargo check -p erpx-console --no-default-features --features web` — PASS.
- `cargo check -p erpx-console --no-default-features --features desktop` — PASS.

Folio remains operational. The persistent ERPX goal remains active.

## Append-only integration record — G6-G12-UI-012 authenticated backup history

The Dioxus console now consumes the audited `ListTenantBackupJobs` read for
`/platform/backups` through WebAssembly gRPC-Web and native Tonic. It first
uses the authenticated tenant registry and caps the tenant fan-out at 100,
with 100 history rows per tenant. The display model contains only bounded
state metadata; backup object references, restore targets, physical database
keys, credentials, and raw errors are excluded. No scheduling, restore,
deletion, or activation control is exposed.

Evidence: `audits/G6-G12-UI-012/README.md`.

Physical MinIO/PITR evidence, backup mutation controls, server history paging,
and the remaining Platform Operations routes remain open. The persistent ERPX
goal remains active.

## Verification wave 2026-07-26 — G6-G12-UI-012 focused integration

- `cargo fmt --all --check` — PASS after integration.
- `cargo test -p erpx-console` — PASS; 20 tests.
- `cargo check -p erpx-console --no-default-features --features web` — PASS.
- `cargo check -p erpx-console --no-default-features --features desktop` — PASS.

Folio remains operational. The persistent ERPX goal remains active.

## Verification wave 2026-07-26 — G6-G12-UI-011 full integration (true EOF)

- `cargo fmt --all --check` — PASS.
- `cargo clippy --workspace --all-targets -- -D warnings` — PASS.
- `cargo test --workspace` — PASS; disposable PostgreSQL integration tests
  remain explicitly ignored without their required databases.
- `cargo check -p erpx-console --no-default-features --features web` — PASS.
- `cargo check -p erpx-console --no-default-features --features desktop` — PASS.
- `bun run lint` — PASS with four pre-existing warnings in
  `infra/scripts/start-n8n.js`; zero errors.
- `bunx tsc --noEmit` — PASS.
- PostgreSQL readiness — PASS; Folio `/operations` returned HTTP 307.
- `git diff --check` — PASS.

Folio remains operational. The persistent ERPX goal remains active.

## Append-only integration record — G6-PRV-032 two-person tenant owner seeding

Main independently reviewed and integrated control migration 0057, immutable
tenant-bound owner-seed registration, independent active platform approval,
single-transaction identity/recovery/role creation, finite role expiry, and
idempotent replay behavior. The plaintext recovery code is returned only once
from the authenticated provisioning response and is never persisted or
projected.

Evidence: `audits/G6-PRV-032/README.md`.

The persistent ERPX goal remains active.

## Verification wave 2026-07-26 — G6-PRV-032 focused integration

- `cargo fmt --all --check` — PASS after formatting.
- `cargo check -p erpx-server` — PASS.
- `cargo clippy -p erpx-server --all-targets -- -D warnings` — PASS.
- `cargo clippy -p erpx-infrastructure --all-targets -- -D warnings` — PASS.
- `cargo test -p erpx-infrastructure --lib provisioning::tests` — PASS; 13
  tests.
- `cargo test -p erpx-identity --lib` — PASS; 20 tests.
- Disposable two-database owner-seed registration proof — PASS.
- Disposable two-database server owner-seed transaction proof — PASS.

Folio remains operational. The persistent ERPX goal remains active.

## Verification wave 2026-07-26 — G6-PRV-032 full integration

- `cargo fmt --all --check` — PASS.
- `cargo clippy --workspace --all-targets -- -D warnings` — PASS.
- `cargo test --workspace` — PASS; active suites passed and disposable
  PostgreSQL suites remain explicitly ignored without their required URLs.
- Web and desktop Dioxus checks — PASS.
- `bun run lint` — PASS with four pre-existing warnings in
  `infra/scripts/start-n8n.js`; zero errors.
- `bunx tsc --noEmit` — PASS.
- `git diff --check` — PASS.
- Runtime checks — PASS: PostgreSQL accepts connections, Folio
  `/operations` responds with `307`, and MinIO health responds `200`.

Folio remains operational. The persistent ERPX goal remains active.

## Append-only integration record — G6-G12-UI-012 authenticated backup history (true EOF)

The Dioxus console consumes the audited `ListTenantBackupJobs` read for
`/platform/backups` through WebAssembly gRPC-Web and native Tonic. It uses the
authenticated tenant registry, caps the tenant fan-out at 100, and requests at
most 100 history rows per tenant. Only bounded job state is rendered; backup
object references, restore targets, physical database keys, credentials, and
raw errors remain excluded. No mutation or restore control is exposed.

Evidence: `audits/G6-G12-UI-012/README.md`.

## Verification wave 2026-07-26 — G6-G12-UI-012 focused integration (true EOF)

- `cargo fmt --all --check` — PASS.
- `cargo test -p erpx-console` — PASS; 20 tests.
- `cargo check -p erpx-console --no-default-features --features web` — PASS.
- `cargo check -p erpx-console --no-default-features --features desktop` — PASS.

Folio remains operational. The persistent ERPX goal remains active.

## Append-only integration record — G6-G12-UI-012 authenticated backup history (true EOF)

The Dioxus console consumes the audited `ListTenantBackupJobs` read for
`/platform/backups` through WebAssembly gRPC-Web and native Tonic. It uses the
authenticated tenant registry, caps the tenant fan-out at 100, and requests at
most 100 history rows per tenant. Only bounded job state is rendered; backup
object references, restore targets, physical database keys, credentials, and
raw errors remain excluded. No mutation or restore control is exposed.

Evidence: `audits/G6-G12-UI-012/README.md`.

## Verification wave 2026-07-26 — G6-G12-UI-012 focused integration (true EOF)

- `cargo fmt --all --check` — PASS.
- `cargo test -p erpx-console` — PASS; 20 tests.
- `cargo check -p erpx-console --no-default-features --features web` — PASS.
- `cargo check -p erpx-console --no-default-features --features desktop` — PASS.

Folio remains operational. The persistent ERPX goal remains active.

## Verification wave 2026-07-26 — G6-G12-UI-011 full integration

- `cargo fmt --all --check` — PASS.
- `cargo clippy --workspace --all-targets -- -D warnings` — PASS.
- `cargo test --workspace` — PASS; disposable PostgreSQL integration tests are
  explicitly ignored without their required databases.
- `cargo check -p erpx-console --no-default-features --features web` — PASS.
- `cargo check -p erpx-console --no-default-features --features desktop` — PASS.
- `bun run lint` — PASS with four existing warnings in
  `infra/scripts/start-n8n.js`; zero errors.
- `bunx tsc --noEmit` — PASS.
- `pg_isready` — accepting connections; `/operations` returned HTTP 307.
- `git diff --check` — PASS.

Folio remains operational. The persistent ERPX goal remains active.

## Append-only integration record — G6-PRV-028 retention lifecycle hardening and worker registration (true EOF)

Main independently reviewed and extended the partial G6-PRV-027 retention
slice. The read projection now clears/rebuilds atomically through keyset pages,
completion uses the database clock for lease fencing, and retryable versus
terminal failure transitions are durable and secret-free. The
`tenant_backup_retention` worker is registered in the executable and claims
due actions through bounded control-plane queries.

Evidence: `audits/G6-PRV-028/README.md`.

Fresh disposable SQL evidence passed for the infrastructure lifecycle,
including projection pagination/rebuild, retry and terminal failure, and a
worker claim/complete/project run. The databases were removed after each
proof. Physical MinIO object deletion, true PITR, remaining provisioning and
Platform Operations work, migration/cutover, and Folio write retirement
remain open. The persistent ERPX goal remains active.

## Verification wave 2026-07-26 — G6-PRV-028 focused integration

- `cargo fmt --all --check` — PASS.
- `cargo clippy -p erpx-infrastructure -p erpx-worker --all-targets -- -D warnings` — PASS.
- `cargo test -p erpx-infrastructure backup_retention --lib` — PASS; 4 tests.
- `cargo test -p erpx-worker backup_retention --lib` — PASS; 1 test.
- Fresh disposable infrastructure SQL proof — PASS.
- Fresh disposable worker SQL proof — PASS.

Folio remains operational. The persistent ERPX goal remains active.

## Append-only integration record — G6-PRV-027 bounded backup retention metadata cleanup (true EOF)

Main independently reviewed the bounded retention handoff. The control
write-side action is tenant/holding-bound, idempotent, cutoff/row/attempt/lease
bounded, and supports dry-run cleanup of only terminal backup job history.
Cleanup deletes linked job/run metadata atomically and exposes only bounded
counts through the receipt/report and rebuildable read projection. The
disposable SQL proof used fresh control write/read databases and passed
idempotent replay, tenant/holding rejection, dry-run non-deletion, cleanup,
read projection, and redaction assertions; both databases were removed after
the run.

Evidence: `audits/G6-PRV-027/README.md`.

The handoff explicitly leaves read-projection pagination/stale-row clearing,
database-clock lease validation in one completion path, durable failure/retry
transition, worker registration, physical MinIO object deletion, true PITR,
owner seeding, remaining Platform Operations actions, and the G6/G14 gates
open. The persistent ERPX goal remains active.

## Verification wave 2026-07-26 — G6-PRV-027 focused integration

- `cargo fmt --all --check` — PASS after formatting the Dioxus tenant route
  adapter.
- `cargo check -p erpx-infrastructure` — PASS.
- `cargo test -p erpx-infrastructure backup_retention --lib` — PASS; 3 tests.
- `cargo test -p erpx-infrastructure --test backup_retention retention_sqlx_flow_binds_tenant_holding_and_replays_idempotently -- --ignored --nocapture` — PASS; fresh disposable control write/read databases covered the SQL flow and were removed after the run.
- `cargo check -p erpx-console --no-default-features --features web` — PASS.

Folio remains operational. The persistent ERPX goal remains active.

## Append-only integration record — G6-PRV-025 durable backup scheduling and worker execution

Main independently reviewed and integrated the durable backup job packet.
Control migration 0052 stores only bounded opaque references and worker state;
control-read migration 0007 stores a secret-free rebuildable history
projection. Scheduling is idempotent and rejects PITR at the command boundary.
Claims use `FOR UPDATE SKIP LOCKED`, exact worker/claim/lease ownership, bounded
attempts, exponential retry delay, and durable dead letters. The executable
`tenant_backup` handler refreshes the read projection, executes full backups,
prepares tenant-bound restore candidates, and invokes the fenced routed restore
activation boundary.

Evidence: `audits/G6-PRV-025/README.md`.

True PITR, live disposable PostgreSQL/MinIO restore evidence, lifecycle
activation/recovery, retention cleanup, remaining Platform Operations pages,
and the G6/G14 gates remain open. The persistent ERPX goal remains active.

## Append-only integration record — G6-PRV-026 authenticated tenant lifecycle transitions

Main independently reviewed and integrated the tenant lifecycle packet.
Registry and provisioning state now transition atomically under the control
write transaction. Activation is gated by reachable write/read database and
storage health; suspension and recovery are explicit, dedicated-permission
platform commands with actor binding and audit records. The worker claim query
accepts only active/recovery registry state, so suspended tenants do not receive
new backup work.

Evidence: `audits/G6-PRV-026/README.md`.

Owner seeding, recovery restore orchestration, retention cleanup, remaining
Platform Operations pages, and the G6/G14 gates remain open. The persistent
ERPX goal remains active.

## Append-only integration record — G6-PRV-023 PostgreSQL/MinIO backup adapter (true EOF)

Main independently reviewed the untrusted support-agent worktree handoff and
integrated only the bounded adapter evidence. `PostgresMinioBackupAdapter`
provides argv-only `pg_dump`/`pg_restore` seams, redacted configuration and
errors, MinIO presigned bounded transfer, tenant/chain/backup-key binding,
checksum-verified idempotent replay, and candidate schema/RLS verification.
`BackupRuntime` composes the adapter with the existing durable provisioner
state machine. It does not wire credentials or database URLs through worker
payloads.

The packet explicitly rejects true PITR because the current contract lacks a
recovery point/LSN, WAL manifest, and physical restore target. No G6 gate or
Folio command-owner change is claimed.

Evidence: `audits/G6-PRV-023/README.md` and
`crates/infrastructure/tests/backup_adapter.rs`.

## Verification wave 2026-07-26 — G6-PRV-023 focused integration (true EOF)

- `cargo fmt --all --check` — PASS.
- `cargo clippy -p erpx-infrastructure --all-targets -- -D warnings` — PASS.
- `cargo clippy -p erpx-worker --all-targets -- -D warnings` — PASS.
- `cargo test -p erpx-infrastructure --test backup_adapter` — PASS; 7 passed.
- `cargo test -p erpx-worker backup_runtime` — PASS; no matching tests, with
  the worker test target compiling successfully.
- `cargo test -p erpx-infrastructure provisioning` — PASS; 10 provisioning
  unit tests passed.
- `git diff --check` — PASS.

The focused tests use injected fakes and prove bounds, redaction, replay,
tenant binding, and failed-restore safety. A live PostgreSQL/MinIO backup or
restore was not run; true PITR, worker executable job registration, and the G6
gate remain open.

## Append-only integration record — G8-FIN-047 Authenticated Finance period and journal posting transport (true EOF)

Main independently reviewed and integrated the authenticated Finance write
transport. `SaveFinancePeriod` and `PostFinanceJournal` now validate the
authenticated actor, command context, tenant/holding identity, explicit
company scope, bounded JSON payloads, and chart/period/journal containment.
Both routes use the shared entitlement/RBAC authorization boundary, write only
through the tenant write database, and emit redacted audit records. Journal
posting delegates to the existing atomic SQLx transaction, including period
locking, authoritative normalized chart/account loading, idempotency, posted
journal persistence, snapshot, and outbox behavior.

Focused evidence:

- `cargo test -p erpx-domain finance::tests` — PASS; 5 passed.
- `cargo test -p erpx-server` — PASS; 81 passed.
- `cargo check -p erpx-proto -p erpx-server` — PASS.
- `bash audits/G2-IAM-008/verify_authorization_routes.sh` — PASS.

## Verification wave 2026-07-26 — G8-FIN-047 full integration (true EOF)

- `cargo fmt --all --check` — PASS.
- `cargo clippy --workspace --all-targets -- -D warnings` — PASS.
- `cargo test --workspace` — PASS; only the existing explicitly ignored
  disposable-database tests remained ignored.
- `cargo check -p erpx-console --no-default-features --features web` — PASS.
- `cargo check -p erpx-console --no-default-features --features desktop` —
  PASS.
- `bun run lint` — PASS with four pre-existing warnings in
  `infra/scripts/start-n8n.js`, zero errors.
- `bunx tsc --noEmit` — PASS.
- `git diff --check` — PASS.
- `pg_isready -h 127.0.0.1 -p 5432` — accepting connections PASS.
- Folio `/operations` — HTTP 307 PASS; development server remains on port
  3004.

AP/AR, bank/cash, close, consolidation, Thai tax, Finance UI, migration
reconciliation, broader Finance completion, and the remaining ERPX and Folio
retirement requirements remain open. The persistent ERPX goal remains active.

## Append-only integration record — G13-MIG-001 Ordered migration run and cutover guard (true EOF)

Main independently implemented and reviewed the transport-neutral migration
run contract. `MigrationRun` validates bounded opaque references and an exact
source-contract hash. Its `MigrationPhase` can advance only through the fixed
G13 sequence: source freeze, ID mapping, write backfill, read projection,
shadow replay, reconciliation, read-only pilot, workflow acceptance, rollback
rehearsal, mutation freeze, final delta, ERPX command ownership, Folio
read-only, retention completion, and Folio retirement. Command ownership is
derived from the phase. Retirement requires an explicit retention window and
fails closed at the exact pre-expiry boundary.

Focused evidence:

- `cargo fmt --all --check` — PASS.
- `cargo test -p erpx-migration -p erpx-migrator` — PASS; 8 migration tests,
  3 migrator unit tests, and 4 CLI integration tests.

This packet changes no database, runtime, Folio route, or command-owner
persistence. Live source freezing, physical backfill/projection, shadow
replay, cohort orchestration, durable cutover persistence, rollback rehearsal,
and Folio write retirement remain open. The persistent ERPX goal remains
active.

## Append-only integration record — G13-MIG-002 Read-only Folio source extractor (true EOF)

Main independently reviewed and integrated the legacy source extractor. It
captures aggregate-only migration evidence in one PostgreSQL `READ ONLY`
transaction and rolls the transaction back. Counts, status distributions,
integer money totals, object-byte totals, missing-table inventory, and bounded
deterministic mappings are emitted without source IDs, PII, documents,
credentials, or raw event payloads. Mapping keys are source fingerprints and
values are opaque ERPX UUIDs. A canonical SHA-256 source-contract hash binds
the aggregate content while excluding capture time. The CLI reads the database
URL only from a named environment variable and redacts connection failures.

Focused evidence:

- `cargo fmt --all --check` — PASS.
- `cargo test -p erpx-migration -p erpx-migrator` — PASS; 9 migration unit
  tests, 3 migrator unit tests, and 5 CLI integration tests.
- Live `folio_db` read-only extractor smoke test — PASS; aggregate counters,
  statuses, money/file totals, mappings, and source hash were emitted without
  URL or raw source payload.

The extractor does not freeze Folio, backfill ERPX, project/read-replay,
reconcile target state, persist cutover ownership, or retire Folio writes.
The persistent ERPX goal remains active.

## Append-only integration record — G2-IAM-007 Durable recovery approval receipts (true EOF)

Main independently reviewed and integrated the existing control-plane recovery
receipt table into the authoritative final-role recovery path. The receipt
hash binds the complete recovery request, assignment version, and decision;
reason and ticket references remain hash-only. Transaction-scoped advisory
locking serializes per-request review versions. Exact idempotent replay
returns the original receipt, while changed recovery material under the same
idempotency key fails closed.

Evidence: `audits/G2-IAM-007/README.md`.

This packet makes no claim of a separate ordinary-renewal approval artifact,
notification provider policy, or G2-G14 completion.

## Verification wave 2026-07-26 — G2-IAM-007 focused integration (true EOF)

- `cargo fmt --all --check` — PASS.
- `cargo check -p erpx-server -p erpx-infrastructure` — PASS.

Folio remains operational. The persistent ERPX goal remains active.

## Append-only integration record — G2-IAM-008 Universal mutation authorization route audit (true EOF)

Main independently audited the authenticated server mutation inventory. All
tenant business writes route through `authorize_entitled_command`; explicit
platform/control-plane writes use platform role authorization; role lifecycle
commands use the dedicated role policy. The bounded route inventory script
passed for 43 tenant methods, five platform/control-plane methods, and both
role lifecycle methods. Public Marketing acquisition remains intentionally
covered by consent, suppression, deduplication, and abuse controls.

Evidence: `audits/G2-IAM-008/README.md`.

## Verification wave 2026-07-26 — G2-IAM-008 full integration (true EOF)

- Authorization route inventory — PASS.
- `cargo fmt --all --check` — PASS.
- `cargo clippy --workspace --all-targets -- -D warnings` — PASS.
- `cargo test --workspace` — PASS; explicitly ignored disposable-database
  tests remained ignored.
- `cargo check -p erpx-console --no-default-features --features web` — PASS.
- `cargo check -p erpx-console --no-default-features --features desktop` — PASS.
- `bun run lint` — PASS with four pre-existing warnings in
  `infra/scripts/start-n8n.js`, zero errors.
- `bunx tsc --noEmit` — PASS.
- `git diff --check` — PASS.
- `pg_isready -h 127.0.0.1 -p 5432` — accepting connections PASS.
- Folio `/operations` — HTTP 307 PASS; development server remains on port
  3004.

Folio remains operational. The persistent ERPX goal remains active.

## Append-only integration record — G2-IAM-004 Recursive parent-chain authorization (true EOF)

Main independently reviewed and integrated recursive role-chain evaluation.
The SQLx control repository now returns the validated immediate-to-root chain
from authoritative role rows/snapshots. The shared evaluator rechecks every
edge for parent identity, half-open activity, scope containment, authority,
permission containment, missing ancestors, and cycles. The universal server
access, entitlement-check, and object-access paths use this complete chain;
legacy immediate-parent callers remain available.

Evidence: `audits/G2-IAM-004/README.md`.

This packet makes no claim of recursive support-intersection transport,
complete universal mutation adoption, durable session resolution in every
caller, approver identity resolution, role-review worker delivery, recovery
replay persistence, or G2-G14 completion.

## Verification wave 2026-07-26 — G2-IAM-004 focused integration (true EOF)

- `cargo fmt --all --check` — PASS.
- `cargo test -p erpx-domain access::tests` — PASS; 19 passed.
- `cargo check -p erpx-server` — PASS.

Folio remains operational. The persistent ERPX goal remains active.

## Append-only integration record — G6-PRV-025 durable backup scheduling and worker execution (final ledger append)

Main independently reviewed and integrated the durable backup job packet.
Control migration 0052 stores only bounded opaque references and worker state;
control-read migration 0007 stores a secret-free rebuildable history
projection. Scheduling is idempotent and rejects PITR at the command boundary.
Claims use `FOR UPDATE SKIP LOCKED`, exact worker/claim/lease ownership, bounded
attempts, exponential retry delay, and durable dead letters. The executable
`tenant_backup` handler refreshes the read projection, executes full backups,
prepares tenant-bound restore candidates, and invokes the fenced routed restore
activation boundary.

Evidence: `audits/G6-PRV-025/README.md`.

True PITR, live disposable PostgreSQL/MinIO restore evidence, lifecycle
activation/recovery, retention cleanup, remaining Platform Operations pages,
and the G6/G14 gates remain open. The persistent ERPX goal remains active.

## Append-only finalization note — G6-PRV-023 review (actual EOF)

The G6-PRV-023 adapter review and focused evidence are integrated above. The
packet is intentionally partial: true PITR, live PostgreSQL/MinIO proof,
worker executable backup-job registration, and the G6 gate remain open. The
persistent ERPX goal remains active.

## Append-only finalization note — G6-PRV-023 review (actual EOF)

The G6-PRV-023 adapter review and focused evidence are integrated above. The
packet is intentionally partial: true PITR, live PostgreSQL/MinIO proof,
worker executable backup-job registration, and the G6 gate remain open. The
persistent ERPX goal remains active.

## Verification wave 2026-07-26 — G8-FIN-048 and G13 migration packets (true EOF)

- `cargo fmt --all --check` — PASS after formatting the provisioning/object
  storage worktree changes.
- `cargo clippy --workspace --all-targets -- -D warnings` — PASS.
- `cargo test --workspace` — PASS; the existing explicitly ignored disposable
  database tests remained ignored. The infrastructure test build emitted only
  unused-import warnings from the still-in-progress provisioning adapter.
- `cargo test -p erpx-migration -p erpx-migrator` — PASS; 9 migration unit
  tests, 3 migrator unit tests, and 5 CLI integration tests.
- `cargo clippy -p erpx-migration -p erpx-migrator --all-targets -- -D warnings`
  — PASS.
- `cargo check -p erpx-console --no-default-features --features web` — PASS.
- `cargo check -p erpx-console --no-default-features --features desktop` —
  PASS.
- `bun run lint` — PASS with four pre-existing warnings in
  `infra/scripts/start-n8n.js`, zero errors.
- `bunx tsc --noEmit` — PASS.
- `git diff --check` — PASS.
- `pg_isready -h 127.0.0.1 -p 5432` — accepting connections PASS.
- Folio `/operations` — HTTP 307 PASS; development server remains on port
  3004.

This wave independently verifies the G8-FIN-048 transport and the G13
source-extraction/cutover guards. It does not close G6 provisioning, physical
backup/PITR execution, target backfill, shadow replay, live command-owner
switching, or Folio write retirement.

## Append-only integration record — G2-IAM-005 Durable role-review notification delivery (true EOF)

Main independently reviewed and integrated the durable role-review worker
boundary. Expired claims can be reclaimed, successful notification keys are
reconciled under their claim identity, and failed/unprocessed keys are
released to pending for retry. The worker has bounded horizon and lease
configuration, redacted persistence error mapping, and an explicit injected
publisher. The default worker registry remains fail-closed until that
publisher is composed.

Evidence: `audits/G2-IAM-005/README.md`.

This packet makes no claim of external provider delivery, localized templates,
recipient policy, complete universal mutation adoption, approver identity
resolution, recovery replay persistence, or G2-G14 completion.

## Verification wave 2026-07-26 — G2-IAM-005 focused integration (true EOF)

- `cargo fmt --all --check` — PASS after formatter application.
- `cargo test -p erpx-worker role_review_notifications` — PASS; 3 passed.
- `cargo check -p erpx-worker` — PASS.
- `cargo check -p erpx-infrastructure` — PASS.

Folio remains operational. The persistent ERPX goal remains active.

## Verification wave 2026-07-26 — G2-IAM-005 full integration (true EOF)

- `cargo fmt --all --check` — PASS after formatter application.
- `cargo clippy --workspace --all-targets -- -D warnings` — PASS.
- `cargo test --workspace` — PASS; 132 worker library tests and 1 worker
  binary test passed; explicitly ignored disposable-database tests remained
  ignored.
- `cargo check -p erpx-console --no-default-features --features web` — PASS.
- `cargo check -p erpx-console --no-default-features --features desktop` —
  PASS.
- `bun run lint` — PASS with four pre-existing warnings in
  `infra/scripts/start-n8n.js`, zero errors.
- `bunx tsc --noEmit` — PASS.
- `git diff --check` — PASS.
- `pg_isready -h 127.0.0.1 -p 5432` — accepting connections PASS.
- Folio `/operations` — HTTP 307 PASS; development server remains on port
  3004.

Folio remains operational. The persistent ERPX goal remains active.

## Append-only integration record — G2-IAM-006 Authoritative approver identity resolution (true EOF)

Main independently reviewed and integrated second-party approver resolution.
High-risk role renewal and final-role recovery now query active identity state
and write-side role chains for `role.assignment.approve`; optional recovery
reviewers require `role.assignment.review`. The requester cannot self-approve,
and inactive, missing, or out-of-scope references fail before command
dispatch.

Evidence: `audits/G2-IAM-006/README.md`.

This packet makes no claim of a separate interactive approval artifact,
complete universal mutation adoption, notification recipient/provider policy,
recovery replay persistence, or G2-G14 completion.

## Verification wave 2026-07-26 — G2-IAM-006 focused integration (true EOF)

- `cargo fmt --all --check` — PASS.
- `cargo test -p erpx-identity` — PASS; 20 passed, 6 disposable SQLx tests
  explicitly ignored.
- `cargo check -p erpx-server` — PASS.

Folio remains operational. The persistent ERPX goal remains active.

## Verification wave 2026-07-26 — G2-IAM-006 full integration (true EOF)

- `cargo fmt --all --check` — PASS.
- `cargo clippy --workspace --all-targets -- -D warnings` — PASS.
- `cargo test --workspace` — PASS; explicitly ignored disposable-database
  tests remained ignored.
- `cargo check -p erpx-console --no-default-features --features web` — PASS.
- `cargo check -p erpx-console --no-default-features --features desktop` —
  PASS.
- `bun run lint` — PASS with four pre-existing warnings in
  `infra/scripts/start-n8n.js`, zero errors.
- `bunx tsc --noEmit` — PASS.
- `git diff --check` — PASS.
- `pg_isready -h 127.0.0.1 -p 5432` — accepting connections PASS.
- Folio `/operations` — HTTP 307 PASS; development server remains on port
  3004.

Folio remains operational. The persistent ERPX goal remains active.

## Append-only integration record — G2-IAM-003 Universal effective-access intersection (true EOF)

Main independently reviewed and integrated the shared effective-access
intersection. The evaluator now binds the session-resolved principal to the
selected role assignment, uses the authoritative allocation book when present,
and requires active company allocation plus explicit branch activation for
company/branch requests. The three existing server authorization paths now
pass the control-plane allocation book into this boundary. Missing allocation
context fails closed for company and branch scope.

Evidence: `audits/G2-IAM-003/README.md`.

This packet makes no claim of complete universal mutation adoption, durable
session resolution in every caller, recursive parent-chain loading,
authoritative approver resolution, worker notification delivery, recovery
replay persistence, or G2-G14 completion.

## Verification wave 2026-07-26 — G2-IAM-003 full integration (true EOF)

- `cargo fmt --all --check` — PASS.
- `cargo clippy --workspace --all-targets -- -D warnings` — PASS (exit 0).
- `cargo test --workspace` — PASS (exit 0); 195 domain tests, 129 worker
  library tests plus 1 worker binary test, and the remaining workspace suites
  passed; explicitly ignored disposable-database tests remained ignored.
- `cargo check -p erpx-console --no-default-features --features web` — PASS.
- `cargo check -p erpx-console --no-default-features --features desktop` —
  PASS.
- `bun run lint` — PASS with four pre-existing warnings in
  `infra/scripts/start-n8n.js`, zero errors.
- `bunx tsc --noEmit` — PASS.
- `git diff --check` — PASS.
- `pg_isready -h 127.0.0.1 -p 5432` — accepting connections PASS.
- Folio `/operations` — HTTP 307 PASS; development server remains on port
  3004.

Folio remains operational. The persistent ERPX goal remains active.

## Append-only integration record — G10-MKT-063 Marketing profile-erasure application and Tonic authorization

Main independently verified and integrated the AG-MKT-063-A application
handoff. `EraseMarketingProfileCommand` now carries authorizing scope,
idempotency, correlation, and causation metadata; the object-safe privacy port
and authorized decorator reject scope violations before delegation. The main
agent added the SQLx adapter, shared protobuf request/response, and Tonic
command. The RPC enforces authenticated identity, explicit tenant scope,
entitlement, dedicated `marketing.profile.erase` RBAC, billing, SoD, tenant
routing, customer-visible audit, and redacted receipts/errors.

Evidence: `audits/G10-MKT-063/README.md` and the fresh two-database
`marketing_profile_erasure` proof.

## Append-only integration record — G10-MKT-050 Account-bound Marketing provider delivery

Main independently verified and integrated the AG-MKT-050-A worker handoff.
Provider requests now carry exact non-secret account/tenant/holding/version
context, preserve idempotency through headers and the bounded envelope, reject
unsafe routes and redirects, stream bounded responses, redact failures, and
fail closed when the active control-plane binding changes or an unbound
provider is used for an account-bound call. Existing pilot environment
credential composition remains explicitly non-production.

Evidence: `audits/G10-MKT-050/README.md`.

## Verification wave 2026-07-26 — G10-MKT-063 and G10-MKT-050 full integration

- `cargo fmt --all --check` — PASS.
- `cargo clippy --workspace --all-targets -- -D warnings` — PASS.
- `cargo test --workspace --quiet` — PASS; application 17, domain targets
  181/6/9, infrastructure 163 passed with 4 ignored, server 79, and worker
  116 passed with 1 ignored.
- `cargo check -p erpx-console --no-default-features --features web` — PASS.
- `cargo check -p erpx-console --no-default-features --features desktop` — PASS.
- Application privacy focused tests — 6 PASS; infrastructure privacy adapter
  tests — 2 PASS; server Marketing transport tests — 4 PASS; provider focused
  tests — 7 PASS.
- Fresh disposable two-database application erasure proof — PASS; the
  application port performed the write, replay was idempotent, source/outbox/
  snapshots contained no raw contact, read rebuild was redacted, forced-RLS
  routing held, and exact cleanup left no `erpx_mkt063_*` residue.
- `bun run lint` — PASS with four pre-existing warnings in
  `infra/scripts/start-n8n.js`, zero errors.
- `bunx tsc --noEmit` — PASS.
- Folio `/operations` — HTTP 307 PASS; development server remains on port 3004.
- `pg_isready -h /tmp -p 5432` — accepting connections PASS.
- `git diff --check` — PASS.

Folio remains operational. The persistent ERPX goal remains active.

## Append-only integration record — G10-MKT-063 Marketing profile-erasure application and Tonic authorization (final tail)

Main independently verified and integrated the AG-MKT-063-A application
handoff. `EraseMarketingProfileCommand` carries authorizing scope,
idempotency, correlation, and causation metadata; the object-safe privacy port
and authorized decorator reject scope violations before delegation. The main
agent added the SQLx adapter, shared protobuf request/response, and Tonic
command. The RPC enforces authenticated identity, explicit tenant scope,
entitlement, dedicated `marketing.profile.erase` RBAC, billing, SoD, tenant
routing, customer-visible audit, and redacted receipts/errors.

Evidence: `audits/G10-MKT-063/README.md` and the fresh two-database
`marketing_profile_erasure` proof.

## Append-only integration record — G10-MKT-050 Account-bound Marketing provider delivery (final tail)

Main independently verified and integrated the AG-MKT-050-A worker handoff.
Provider requests carry exact non-secret account/tenant/holding/version
context, preserve idempotency through headers and the bounded envelope, reject
unsafe routes and redirects, stream bounded responses, redact failures, and
fail closed when the active control-plane binding changes or an unbound
provider is used for an account-bound call. Existing pilot environment
credential composition remains explicitly non-production.

Evidence: `audits/G10-MKT-050/README.md`.

## Verification wave 2026-07-26 — G10-MKT-063 and G10-MKT-050 full integration (final tail)

- `cargo fmt --all --check` — PASS.
- `cargo clippy --workspace --all-targets -- -D warnings` — PASS.
- `cargo test --workspace --quiet` — PASS; application 17, domain targets
  181/6/9, infrastructure 163 passed with 4 ignored, server 79, and worker
  116 passed with 1 ignored.
- `cargo check -p erpx-console --no-default-features --features web` — PASS.
- `cargo check -p erpx-console --no-default-features --features desktop` — PASS.
- Application privacy focused tests — 6 PASS; infrastructure privacy adapter
  tests — 2 PASS; server Marketing transport tests — 4 PASS; provider focused
  tests — 7 PASS.
- Fresh disposable two-database application erasure proof — PASS; the
  application port performed the write, replay was idempotent, source/outbox/
  snapshots contained no raw contact, read rebuild was redacted, forced-RLS
  routing held, and exact cleanup left no `erpx_mkt063_*` residue.
- `bun run lint` — PASS with four pre-existing warnings in
  `infra/scripts/start-n8n.js`, zero errors.
- `bunx tsc --noEmit` — PASS.
- Folio `/operations` — HTTP 307 PASS; development server remains on port 3004.
- `pg_isready -h /tmp -p 5432` — accepting connections PASS.
- `git diff --check` — PASS.

Folio remains operational. The persistent ERPX goal remains active.

## G10-MKT-061 — Marketing form semantics and privacy retention

Main integrated G10-MKT-061-A from AG-MKT-SEMANTICS and G10-MKT-061-B from
AG-MKT-PRIVACY-WORKER after reviewing their exclusive paths. Domain form
submissions now validate deterministic email syntax, canonical international
phone digits, finite plain decimals, exact lowercase consent, and required
blank values. The tenant write submission contract now carries a bounded
retention deadline and redaction timestamp. Main-owned SQLx persistence
irreversibly replaces expired submitted values with `{}`, appends an attributed
`marketing.content_form_submission_redacted.v1` event with no submitted value,
and rebuilds read rows from the redacted source state. The worker registers
`marketing_content_privacy`, constructs a system-worker holding command
context, routes only tenant write state, bounds the batch, closes the route,
and redacts persistence failures.

Evidence: `audits/G10-MKT-061/README.md`.

## Verification wave 2026-07-26 — G10-MKT-061 integration

- `cargo fmt --all --check` — PASS.
- `cargo clippy --workspace --all-targets -- -D warnings` — PASS.
- `cargo test --workspace --quiet` — PASS; domain targets passed, 161
  infrastructure tests passed with 4 ignored, 79 server tests passed, and
  113 worker tests passed with 1 ignored.
- `cargo test -p erpx-worker marketing_content_privacy --quiet` — PASS; 5
  focused tests.
- `cargo clippy -p erpx-worker --all-targets -- -D warnings` — PASS.
- `cargo test -p erpx-domain --quiet` — PASS; 178, 6, and 9 tests passed in
  the domain targets.
- `cargo check -p erpx-console --no-default-features --features web` — PASS.
- `cargo check -p erpx-console --no-default-features --features desktop` — PASS.
- Focused disposable two-database Marketing proof — PASS; tenant write
  migration 48 and read migration 38 applied, source values redacted, the
  redaction outbox payload contained no submitted value, replay was empty,
  replacement read rebuild remained redacted, and exact cleanup left no
  `erpx_mkt061_*` databases.
- `bun run lint` — PASS with four pre-existing warnings in
  `infra/scripts/start-n8n.js`, zero errors.
- `bunx tsc --noEmit` — PASS.
- Folio `/operations` — HTTP 307 PASS; development server remains on port 3004.
- `pg_isready -h /tmp -p 5432` — accepting connections PASS.
- `git diff --check` — PASS.

Profile-level consent/erasure, suppression and delivery enforcement, public
abuse controls, SSR/UI, live MinIO, host-only discovery, durable retry/DLQ,
remaining Marketing/ERP contexts, migration/cutover, and Folio write
retirement remain open; the persistent ERPX goal remains active.

## Entry 2026-07-26 — G10-MKT-057 authenticated Marketing form submission transport

Main added and registered the authenticated Tonic `MarketingContentService.SubmitMarketingForm` command. It enforces active authentication, command context, explicit tenant/holding scope, Marketing entitlement, and authoritative write-side authorization; rejects platform and subtree scopes; bounds request size and string-map values; routes through the tenant database router and SQLx content store; preserves idempotent receipts; records a customer-visible audit event; and redacts persistence errors while retaining correlation metadata.

Evidence: `audits/G10-MKT-057/README.md`. Anonymous/public HTTP submission and active-domain lookup, preview and domain transport, executable worker scheduling, an application-layer Marketing port, semantic typed-field validation, retention/erasure, live MinIO, SSR/UI, vendor/secret adapters, remaining ERP contexts, migration/cutover, and Folio write retirement remain open.

## Verification wave 2026-07-26 — G10-MKT-057 integration

- `cargo fmt --all --check` — PASS.
- `cargo clippy --workspace --all-targets -- -D warnings` — PASS.
- `cargo test --workspace --quiet` — PASS; 173 domain, 158 infrastructure with 4 ignored, 77 server, and 108 worker tests passed; worker has 1 ignored.
- `cargo check -p erpx-console --no-default-features --features web` — PASS.
- `cargo check -p erpx-console --no-default-features --features desktop` — PASS.
- Focused server transport unit tests — PASS; 77 server tests passed, including bounded form-value validation and redacted error mapping.
- `bun run lint` — PASS with four pre-existing warnings in `infra/scripts/start-n8n.js`, zero errors.
- `bunx tsc --noEmit` — PASS.
- Folio `/operations` — HTTP 307 PASS; development server remains on port 3004.
- `pg_isready -h /tmp -p 5432` — accepting connections PASS.
- `git diff --check` — PASS.

Folio remains operational. The persistent ERPX goal remains active.

## Entry 2026-07-26 — G10-MKT-058 public Marketing form and preview transport

Main integrated public HTTP form submission and preview authorization. Active tenant registry routing, custom-domain Host binding, published-form loading, idempotent service-attributed form writes, hashed expiring preview authorization, exact template binding, and replacement-read published content loading are now enforced at the boundary. AG-MKT-WORKER independently delivered executable preview-expiry registration in its exclusive worker main path; main reviewed the route, configuration, bounded polling, and SQLx tenant routing before integration.

Evidence: `audits/G10-MKT-058/README.md`. Public abuse/rate limiting, semantic field validation, SSR/UI, MinIO rendering, control-plane host-only discovery, durable retry/DLQ, issuance/revocation and domain-management transport, the application-layer Marketing port, remaining ERP contexts, migration/cutover, and Folio write retirement remain open.

## Verification wave 2026-07-26 — G10-MKT-058 integration

- `cargo fmt --all --check` — PASS.
- `cargo clippy --workspace --all-targets -- -D warnings` — PASS.
- `cargo test --workspace --quiet` — PASS; 173 domain, 158 infrastructure with 4 ignored, 78 server, and 108 worker tests passed; worker has 1 ignored.
- `cargo check -p erpx-console --no-default-features --features web` — PASS.
- `cargo check -p erpx-console --no-default-features --features desktop` — PASS.
- Worker-focused strict Clippy/tests — PASS; 108 passed, 1 ignored.
- Focused two-database Marketing live proof — PASS; published form load, hashed preview authorization, active-domain binding lookup, replacement-read template load, workflow projection/rebuild, forced-RLS inspection, and exact cleanup. Temporary databases were dropped and residue was checked.
- `bun run lint` — PASS with four pre-existing warnings in `infra/scripts/start-n8n.js`, zero errors.
- `bunx tsc --noEmit` — PASS.
- Folio `/operations` — HTTP 307 PASS; development server remains on port 3004.
- `pg_isready -h /tmp -p 5432` — accepting connections PASS.
- `git diff --check` — PASS.

Folio remains operational. The persistent ERPX goal remains active.

## Entry 2026-07-26 — G10-MKT-050 bounded provider-neutral delivery adapter

Main-agent implementation added `marketing_provider.rs`, a bounded HTTPS
adapter for Email, SMS, Social, Push, and Webhook delivery records. It uses
one explicit route per configured channel, zeroizing credential material,
redacted debug output, bounded provider responses, explicit provider-message
references, and safe retry/permanent classification. The worker executable now
has an explicit `marketing_delivery` branch that routes a tenant holding's
write and read pools before constructing the delivery source and provider.

Independent AG-QA review found no code changes and confirmed this packet must
not be treated as the provider gate: control-plane provider-account binding,
secret-manager resolution, provider idempotency headers, ambiguous-timeout
reconciliation, vendor-specific adapters, and authoritative callback scope
remain open. The pilot resolver is intentionally environment-backed and
fails closed when its referenced material is absent. Folio behavior and Folio
writes were not changed.

Evidence: worker configuration tests, zeroization/redaction tests, a
disposable local HTTP response-mapping test, strict worker Clippy, and the
worker package test passed. The full integration wave follows this entry.

## Entry 2026-07-26 — G10-MKT-051 account-bound provider and callback routing

AG-FND supplied and main independently reviewed the additive control-plane
outbound account migration and repository fields. The worker now requires an
active provider-account reference for the Marketing delivery handler, resolves
the exact tenant/holding binding before registration, and rechecks the
account/provider/credential/version tuple before each submission. Provider
requests carry the delivery idempotency key header.

The callback ingress now resolves tenant and holding from the provider account
record, routes to the corresponding tenant write/read databases, loads the
delivery aggregate from tenant write state, derives its stored scope, and only
then invokes the existing authenticated callback/domain/inbox/outbox path.
Caller-provided tenant, holding, company, and branch headers are no longer
used for scope selection. Legacy provider rows receive explicit unconfigured
outbound defaults and therefore fail closed until configured.

AG-FND changed only the assigned migration/repository/test paths and reported
that its final live test was interrupted; main retained the files and
independently verified formatting, compilation, and server/worker strict
checks. No Folio behavior or Folio writes changed.

Evidence: `audits/G10-MKT-051/README.md`. The full integration wave follows.

## Verification wave 2026-07-26 — G10-MKT-051 integration

- `cargo fmt --all --check` — PASS.
- `cargo clippy --workspace --all-targets -- -D warnings` — PASS.
- `cargo test --workspace --quiet` — PASS; no failures, including 168 active
  domain tests, 155 active infrastructure tests with 4 ignored, 75 server
  tests, and 103 active worker tests with 1 ignored.
- `cargo check -p erpx-console --no-default-features --features web` — PASS.
- `cargo check -p erpx-console --no-default-features --features desktop` —
  PASS.
- Worker provider unit tests — PASS; four tests including local HTTP response
  mapping and the `Idempotency-Key` assertion.
- `cargo check -p erpx-server` — PASS.
- `cargo clippy -p erpx-server --all-targets -- -D warnings` — PASS.
- Marketing callback signature tests — PASS; three tests.
- Disposable two-database provider-account proof — PASS; active exact,
  outbound, callback, debug-redaction, and suspended fail-closed assertions.
  Both exact databases were dropped after the test.
- `bun run lint` — PASS with four pre-existing warnings in
  `infra/scripts/start-n8n.js`, zero errors.
- `bunx tsc --noEmit` — PASS.
- Folio `/operations` — HTTP 307 PASS.
- `pg_isready -h /tmp -p 5432` — accepting connections PASS.
- `git diff --check` — PASS.

Folio remains operational. Secret-manager credential resolution, vendor
adapters, ambiguous provider-timeout reconciliation, end-to-end callback
delivery proof, journeys, CRM/attribution, Finance reconciliation, remaining
ERP contexts, migration cutover, and Folio write retirement remain open; the
persistent ERPX goal remains active.

## Entry 2026-07-26 — G10-MKT-052 end-to-end Marketing delivery proof

Main-agent verification added `marketing_delivery_provider_flow.rs`, a
disposable two-database acceptance test covering granted consent, durable
queueing, write-side claim, provider acceptance, exact provider-message
reference, authenticated callback, delivered state, callback inbox, callback
outbox event, and version progression from 1 through 4. Cleanup is exact and
restores the append-only consent trigger. The exact test databases were dropped
after the successful run.

This is evidence for the delivery/callback path only; vendor adapters,
secret-manager resolution, provider timeout reconciliation, journey/campaign
effects, CRM/attribution, Finance reconciliation, content/UI, and the wider
G10 gate remain open. No Folio behavior or Folio writes changed.

## Verification wave 2026-07-26 — G10-MKT-052 integration

- `cargo fmt --all --check` — PASS.
- `cargo clippy --workspace --all-targets -- -D warnings` — PASS.
- `cargo test --workspace --quiet` — PASS; no failures, including 168 active
  domain tests, 155 active infrastructure tests with 4 ignored, 75 server
  tests, and 103 active worker tests with 1 ignored.
- `cargo check -p erpx-console --no-default-features --features web` — PASS.
- `cargo check -p erpx-console --no-default-features --features desktop` —
  PASS.
- Focused two-database delivery/provider/callback proof — PASS; exact
  databases were dropped after the test.
- `bun run lint` — PASS with four pre-existing warnings in
  `infra/scripts/start-n8n.js`, zero errors.
- `bunx tsc --noEmit` — PASS.
- Folio `/operations` — HTTP 307 PASS.
- `pg_isready -h /tmp -p 5432` — accepting connections PASS.
- `git diff --check` — PASS.

Folio remains operational. Vendor adapters, secret-manager resolution,
ambiguous provider-timeout reconciliation, journey/campaign effects,
CRM/attribution, Finance reconciliation, content/UI, remaining ERP contexts,
migration cutover, and Folio write retirement remain open; the persistent ERPX
goal remains active.

## Entry 2026-07-26 — G10-MKT-053 Marketing content foundation

AG-MKT delivered the domain-only content contract in its allowed paths;
main-agent review found the stated lifecycle, locale, scope, object-reference,
form, custom-domain, and preview rules compile. Main then integrated the
tenant-write/read migrations and SQLx persistence for versioned assets and
templates. The write boundary commits snapshots, immutable revision rows,
aggregate snapshots, complete outbox envelopes, and command idempotency in one
transaction. A replacement-read rebuild clears and repopulates the separate
read database with content rows, checkpoints, and event receipts under forced
RLS. Main added aggregate validation gates and monotonic preview revocation
after independent Clippy review found both issues in the handoff.

Evidence: `audits/G10-MKT-053/README.md`. Forms/domain persistence, MinIO,
preview workers, transport, SSR/UI, and the broader G10 gate remain open. No
Folio behavior or Folio writes changed.

## Verification wave 2026-07-26 — G10-MKT-053 integration

- Focused domain tests — PASS; 5 Marketing content tests.
- Focused infrastructure Clippy/tests — PASS; strict package Clippy and
  migration contract test passed.
- Disposable two-database content proof — PASS; exact databases were dropped.

The complete workspace/Folio verification wave follows after the remaining
integration edits in this continuation. The persistent ERPX goal remains
active.

## Verification wave 2026-07-26 — G10-MKT-053 integration complete

- `cargo fmt --all --check` — PASS.
- `cargo clippy --workspace --all-targets -- -D warnings` — PASS.
- `cargo test --workspace --quiet` — PASS; no failures, including 173 active
  domain tests, 156 active infrastructure tests with 4 ignored, 75 server
  tests, and 105 active worker tests with 1 ignored.
- `cargo check -p erpx-console --no-default-features --features web` — PASS.
- `cargo check -p erpx-console --no-default-features --features desktop` —
  PASS.
- Focused Marketing content tests — PASS; domain contract, migration
  contract, strict infrastructure Clippy, and disposable two-database
  lifecycle/rebuild proof. Exact temporary databases were dropped and residue
  was checked.
- `bun run lint` — PASS with four pre-existing warnings in
  `infra/scripts/start-n8n.js`, zero errors.
- `bunx tsc --noEmit` — PASS.
- Folio `/operations` — HTTP 307 PASS.
- `pg_isready -h /tmp -p 5432` — accepting connections PASS.
- `git diff --check` — PASS.

Folio remains operational. Marketing forms/domain persistence, MinIO/content
authorization, preview/publication workers, transport, SSR/UI, provider
adapters/secret management, remaining ERP contexts, migration cutover, and
Folio write retirement remain open; the persistent ERPX goal remains active.

## Entry 2026-07-26 — G10-MKT-054 Marketing signed content-object access

Main added the Marketing-specific signed object boundary over MinIO. It binds
an already-authoritative actor/purpose/correlation/scope/permission context to
a validated asset version and tenant storage namespace, uses only the stored
opaque object key, and bounds upload/download URLs to one hour. Cross-holding,
platform, permission-confusion, invalid-namespace, and excessive-lifetime
requests fail closed. No object bytes, credentials, or bearer tokens cross the
domain or persistence boundary.

Evidence: `audits/G10-MKT-054/README.md`; strict infrastructure Clippy and
focused unit tests passed. Live MinIO and server transport/audit integration
remain open.

## Verification wave 2026-07-26 — G10-MKT-054 integration

- `cargo fmt --all --check` — PASS.
- `cargo clippy --workspace --all-targets -- -D warnings` — PASS.
- `cargo test --workspace --quiet` — PASS; no failures, including 173 active
  domain tests, 158 active infrastructure tests with 4 ignored, 75 server
  tests, and 105 active worker tests with 1 ignored.
- `cargo check -p erpx-console --no-default-features --features web` — PASS.
- `cargo check -p erpx-console --no-default-features --features desktop` —
  PASS.
- Focused Marketing object-access tests — PASS.
- `bun run lint` — PASS with four pre-existing warnings in
  `infra/scripts/start-n8n.js`, zero errors.
- `bunx tsc --noEmit` — PASS.
- Folio `/operations` — HTTP 307 PASS.
- `pg_isready -h /tmp -p 5432` — accepting connections PASS.
- `git diff --check` — PASS.

Folio remains operational. Live MinIO, server transport/audit integration,
forms/domain persistence, preview/publication workers, SSR/UI, remaining ERP
contexts, migration cutover, and Folio write retirement remain open; the
persistent ERPX goal remains active.

## Entry 2026-07-26 — G10-MKT-053 form/domain aggregate persistence amendment

Main completed the remaining source-aggregate slice of the G10-MKT-053
foundation. Tenant-write persistence now stores validated form and custom
domain snapshots through the same optimistic-concurrency, atomic outbox, and
command-idempotency transaction boundary as assets and templates. The
replacement-read rebuild projects all four content aggregate kinds into the
separate tenant-read database under forced RLS. The live proof now creates a
template, form, and custom domain, rebuilds the read database, verifies three
rows, proves stale-version rejection and exact replay, and removes both
temporary databases.

This amendment does not claim domain binding/verification, form submission
commands, preview-grant persistence, live MinIO, server transport/audit,
preview/publication workers, SSR/UI, provider secret management, or the wider
G10 gate. No Folio behavior or Folio writes changed.

## Verification wave 2026-07-26 — G10-MKT-053 source completeness integration

- `cargo fmt --all --check` — PASS.
- `cargo clippy --workspace --all-targets -- -D warnings` — PASS.
- `cargo test --workspace --quiet` — PASS; no failures, including 173 active
  domain tests, 158 active infrastructure tests with 4 ignored, 75 server
  tests, and 105 active worker tests with 1 ignored.
- `cargo check -p erpx-console --no-default-features --features web` — PASS.
- `cargo check -p erpx-console --no-default-features --features desktop` —
  PASS.
- Focused live content proof — PASS; two physically separated temporary
  tenant databases covered template lifecycle, exact idempotency, stale
  version rejection, form/domain snapshots, forced-RLS assertions, and
  replacement-read rebuild to three rows. Databases were dropped and residue
  was checked.
- `bun run lint` — PASS with four pre-existing warnings in
  `infra/scripts/start-n8n.js`, zero errors.
- `bunx tsc --noEmit` — PASS.
- Folio `/operations` — HTTP 307 PASS.
- `pg_isready -h /tmp -p 5432` — accepting connections PASS.
- `git diff --check` — PASS.

Folio remains operational. Domain binding/verification, form submission,
preview persistence/workers, live MinIO, server transport/audit, SSR/UI,
vendor adapters/secret management, remaining ERP contexts, migration cutover,
and Folio write retirement remain open; the persistent ERPX goal remains
active.

## Entry 2026-07-26 — G10-MKT-055 Marketing content workflow persistence

Main integrated the tenant-write workflow slice after an independent AG-QA
audit. Preview issue/revoke now requires the authoritative published template
revision and stores only a SHA-256 token hash. Published form submission
re-reads and validates the write-side form snapshot, persists bounded values
under the holding scope, and emits an event without submitted values. Domain
verification stores only a challenge hash, activates only on exact proof
matching, and domain binding requires the verified active domain and current
published template revision. These facts use advisory locking, request-hash
idempotency, attributed outbox events, and the existing forced-RLS tenant
transaction boundary.

Evidence: audits/G10-MKT-055/README.md. Public transport, preview expiry
workers, workflow read projections, semantic typed-field validation,
retention/privacy policy, live MinIO/server integration, SSR/UI, remaining ERP
contexts, migration cutover, and Folio write retirement remain open. No Folio
behavior or Folio writes changed.

## Verification wave 2026-07-26 — G10-MKT-055 integration

- cargo fmt --all --check — PASS.
- cargo clippy --workspace --all-targets -- -D warnings — PASS.
- cargo test --workspace --quiet — PASS; no failures, including 173 active
  domain tests, 158 active infrastructure tests with 4 ignored, 75 server
  tests, and 105 active worker tests with 1 ignored.
- cargo check -p erpx-console --no-default-features --features web — PASS.
- cargo check -p erpx-console --no-default-features --features desktop — PASS.
- Focused live Marketing content workflow proof — PASS; two temporary
  physically separated tenant databases covered published preview issue/replay
  and revoke, published form submission, hashed domain verification and
  binding, forced-RLS inspection, replacement-read rebuild, and exact cleanup.
- bun run lint — PASS with four pre-existing warnings in
  infra/scripts/start-n8n.js, zero errors.
- bunx tsc --noEmit — PASS.
- Folio /operations — HTTP 307 PASS.
- pg_isready -h /tmp -p 5432 — accepting connections PASS.
- git diff --check — PASS.

Folio remains operational. Public transport, preview serving/expiry workers,
workflow read projections, typed form semantics, live MinIO, server
authorization/audit, SSR/UI, vendor adapters/secret management, remaining ERP
contexts, migration cutover, and Folio write retirement remain open; the
persistent ERPX goal remains active.

## Entry 2026-07-26 — G10-MKT-056 Marketing workflow projection and expiry

Main integrated tenant-read migration 0037 and a replacement workflow rebuild
for preview metadata, form submissions, domain/binding state, checkpoints, and
event receipts. Token and verification hashes are excluded from the read
database. Preview selection is holding- and scope-bound with a maximum batch
of 100. The independently delivered worker seam was reviewed and integrated
with a main-owned SQLx adapter that derives deterministic command and
correlation IDs, uses stable operation keys, continues after item failures,
and classifies database failures for retry.

Evidence: audits/G10-MKT-056/README.md. Worker executable registration and
scheduling, public transport, preview serving, DNS/HTTP verification,
semantic typed-field validation, privacy retention/erasure, live MinIO,
server authorization/audit, SSR/UI, remaining ERP contexts, migration cutover,
and Folio write retirement remain open. No Folio behavior or Folio writes
changed.

## Verification wave 2026-07-26 — G10-MKT-056 integration

- cargo fmt --all --check — PASS.
- cargo clippy --workspace --all-targets -- -D warnings — PASS.
- cargo test --workspace --quiet — PASS; 173 domain tests passed, 158
  infrastructure tests passed with 4 ignored, 75 server tests passed, and
  108 worker tests passed with 1 ignored.
- Worker-focused strict Clippy/tests — PASS; 108 passed, 1 ignored.
- cargo check -p erpx-console --no-default-features --features web — PASS.
- cargo check -p erpx-console --no-default-features --features desktop — PASS.
- Focused two-database Marketing workflow proof — PASS; due preview expiry,
  form/domain writes, workflow read rebuild, counts, forced-RLS inspection,
  and exact cleanup. Temporary databases were dropped and residue was checked.
- bun run lint — PASS with four pre-existing warnings in
  infra/scripts/start-n8n.js, zero errors.
- bunx tsc --noEmit — PASS.
- Folio /operations — HTTP 307 PASS.
- pg_isready -h /tmp -p 5432 — accepting connections PASS.
- git diff --check — PASS.

Folio remains operational. Executable worker registration/scheduling, public
transport, preview serving, DNS/HTTP verification, typed form semantics,
privacy retention/erasure, live MinIO, server authorization/audit, SSR/UI,
vendor adapters/secret management, remaining ERP contexts, migration cutover,
and Folio write retirement remain open; the persistent ERPX goal remains
active.

## Append-only integration record — G10-MKT-059 full verification

- `cargo fmt --all --check` — PASS.
- `cargo clippy --workspace --all-targets -- -D warnings` — PASS.
- `cargo test --workspace --quiet` — PASS; 173 domain, 158 infrastructure
  with 4 ignored, 79 server, and 108 worker tests passed; worker has 1
  ignored.
- `cargo check -p erpx-console --no-default-features --features web` — PASS.
- `cargo check -p erpx-console --no-default-features --features desktop` — PASS.
- `bun run lint` — PASS with four pre-existing warnings in
  `infra/scripts/start-n8n.js`, zero errors.
- `bunx tsc --noEmit` — PASS.
- Folio `/operations` — HTTP 307 PASS; development server remains on port 3004.
- `pg_isready -h /tmp -p 5432` — accepting connections PASS.
- `git diff --check` — PASS.

This is an append-only supplemental record for G10-MKT-059; the persistent
ERPX goal remains active.

## Append-only integration record — G10-MKT-060 application-to-SQLx Marketing adapter

Main reviewed and integrated the exclusive infrastructure adapter delivered by AG-MKT-ADAPTER. `SqlxMarketingContentWriteAdapter` implements the application Marketing write port for form, preview, and domain workflows, maps receipts to stable application metadata, and maps all persistence failures to a single redacted repository error. Infrastructure exports it, and authenticated Tonic Marketing writes now construct application commands and call the port. The live Marketing proof exercises the adapter on form submission while retaining the existing physically separated write/read and forced-RLS checks.

Evidence: `audits/G10-MKT-060/README.md`. Worker composition through the application port, live all-command transport proof, semantic validation, privacy, public abuse controls, SSR/UI, MinIO, host-only discovery, durable retry/DLQ, remaining ERP contexts, migration/cutover, and Folio write retirement remain open.

## Verification wave 2026-07-26 — G10-MKT-060 integration

- `cargo fmt --all --check` — PASS.
- `cargo clippy --workspace --all-targets -- -D warnings` — PASS.
- `cargo test --workspace --quiet` — PASS; 173 domain, 161 infrastructure
  with 4 ignored, 79 server, and 108 worker tests passed; worker has 1
  ignored.
- `cargo check -p erpx-console --no-default-features --features web` — PASS.
- `cargo check -p erpx-console --no-default-features --features desktop` — PASS.
- Application/infrastructure/server focused checks — PASS; application 11
  tests, infrastructure 161 passed with 4 ignored, server 79 passed.
- Focused two-database Marketing live proof — PASS; application-port form
  submission, preview/domain workflow, replacement-read rebuild, forced-RLS
  inspection, exact cleanup, and zero database residue.
- `bun run lint` — PASS with four pre-existing warnings in
  `infra/scripts/start-n8n.js`, zero errors.
- `bunx tsc --noEmit` — PASS.
- Folio `/operations` — HTTP 307 PASS; development server remains on port 3004.
- `pg_isready -h /tmp -p 5432` — accepting connections PASS.
- `git diff --check` — PASS.

Folio remains operational. The persistent ERPX goal remains active.

## Append-only integration record — G10-MKT-061 Marketing semantics and privacy retention

Main reviewed and integrated the exclusive domain semantic-validation and
worker handoffs, then added the disjoint write/read privacy migrations and
authoritative SQLx redaction path. Form submissions now enforce deterministic
field semantics; expired values are replaced with `{}` in tenant write state,
redaction is recorded as an attributed non-sensitive outbox event, and the
replacement read rebuild cannot restore the values. The executable
`marketing_content_privacy` worker routes only tenant write state and is now
registered in the worker binary.

Evidence: `audits/G10-MKT-061/README.md`.

## Verification wave 2026-07-26 — G10-MKT-061 full integration

- `cargo fmt --all --check` — PASS.
- `cargo clippy --workspace --all-targets -- -D warnings` — PASS.
- `cargo test --workspace --quiet` — PASS; domain targets passed, 161
  infrastructure tests passed with 4 ignored, 79 server tests passed, and
  113 worker tests passed with 1 ignored.
- `cargo test -p erpx-worker marketing_content_privacy --quiet` — PASS; 5
  focused tests.
- `cargo clippy -p erpx-worker --all-targets -- -D warnings` — PASS.
- `cargo test -p erpx-domain --quiet` — PASS; 178, 6, and 9 tests passed in
  the domain targets.
- `cargo check -p erpx-console --no-default-features --features web` — PASS.
- `cargo check -p erpx-console --no-default-features --features desktop` — PASS.
- Focused disposable two-database proof — PASS; write migration 48 and read
  migration 38 applied, source redaction, non-sensitive outbox payload,
  idempotent replay, redacted read rebuild, forced-RLS checks, and exact
  cleanup. No `erpx_mkt061_*` database residue remained.
- `bun run lint` — PASS with four pre-existing warnings in
  `infra/scripts/start-n8n.js`, zero errors.
- `bunx tsc --noEmit` — PASS.
- Folio `/operations` — HTTP 307 PASS; development server remains on port 3004.
- `pg_isready -h /tmp -p 5432` — accepting connections PASS.
- `git diff --check` — PASS.

The persistent ERPX goal remains active. Profile-level consent/erasure,
suppression and delivery enforcement, public abuse controls, SSR/UI, live
MinIO, host-only discovery, durable retry/DLQ, remaining ERP contexts,
migration/cutover, and Folio write retirement remain open.

## Append-only integration record — G10-MKT-062 Marketing profile erasure and suppression

Main reviewed and integrated the AG-MKT-ERASURE-DOMAIN handoff, then added
tenant-write/read migrations 0049/0039 and the authoritative SQLx erasure path.
The typed command is contact-free and scope-bound; source profile contact and
attributes are irreversibly redacted, the profile is frozen, permanent
profile-erasure suppression is recorded, and consent/suppression mutations
reject erased profiles. Existing profile outbox payloads are scrubbed in the
same transaction into a redacted tombstone while preserving event identity and
audit metadata. The read projector handles scrubbed historical events and
rebuilds from an empty read database without rehydrating contact data.

Evidence: `audits/G10-MKT-062/README.md` and
`crates/infrastructure/tests/marketing_profile_erasure.rs`.

The persistence path is complete for this packet. Application/Tonic actor
authorization must still call `MarketingProfileErasure::validate_for_scope`
before exposing the command. Queued delivery-payload scrubbing, public abuse
controls, SSR/UI, MinIO, host-only discovery, durable retry/DLQ, remaining
Marketing/ERP contexts, migration/cutover, and Folio write retirement remain
open.

## Verification wave 2026-07-26 — G10-MKT-062 full integration

- `cargo fmt --all --check` — PASS.
- `cargo clippy --workspace --all-targets -- -D warnings` — PASS.
- `cargo test --workspace --quiet` — PASS; domain targets passed (181, 6, and
  9 tests), infrastructure 161 passed with 4 ignored, server 79 passed, and
  worker 113 passed with 1 ignored.
- `cargo check -p erpx-console --no-default-features --features web` — PASS.
- `cargo check -p erpx-console --no-default-features --features desktop` — PASS.
- Focused infrastructure compile, strict Clippy, static migration test, and
  fresh disposable two-database erasure proof — PASS. The proof verified
  idempotent erasure, source redaction, permanent suppression, zero raw email
  in profile outbox payloads and aggregate snapshots, empty-read rebuild,
  forced-RLS routing, redacted read state, and exact database cleanup.
- `bun run lint` — PASS with four pre-existing warnings in
  `infra/scripts/start-n8n.js`, zero errors.
- `bunx tsc --noEmit` — PASS.
- Folio `/operations` — HTTP 307 PASS; development server remains on port 3004.
- `pg_isready -h /tmp -p 5432` — accepting connections PASS.
- `git diff --check` — PASS.

Folio remains operational. The persistent ERPX goal remains active.

## Append-only integration record — G10-MKT-063 Marketing profile-erasure application and Tonic authorization (true EOF)

Main independently verified and integrated the AG-MKT-063-A application
handoff. The typed command carries authorizing scope, idempotency, correlation,
and causation metadata; the object-safe privacy port and authorized decorator
reject scope violations before delegation. Main added the SQLx adapter, shared
protobuf request/response, and Tonic command enforcing authenticated identity,
explicit tenant scope, entitlement, dedicated `marketing.profile.erase` RBAC,
billing, SoD, tenant routing, customer-visible audit, and redacted
receipts/errors.

Evidence: `audits/G10-MKT-063/README.md` and the fresh two-database
`marketing_profile_erasure` proof.

## Append-only integration record — G10-MKT-050 Account-bound Marketing provider delivery (true EOF)

Main independently verified and integrated the AG-MKT-050-A worker handoff.
Provider requests carry exact non-secret account/tenant/holding/version
context, preserve idempotency through headers and the bounded envelope, reject
unsafe routes and redirects, stream bounded responses, redact failures, and
fail closed when the active control-plane binding changes or an unbound
provider is used for an account-bound call. Existing pilot environment
credential composition remains explicitly non-production.

Evidence: `audits/G10-MKT-050/README.md`.

## Verification wave 2026-07-26 — G10-MKT-063 and G10-MKT-050 full integration (true EOF)

- `cargo fmt --all --check` — PASS.
- `cargo clippy --workspace --all-targets -- -D warnings` — PASS.
- `cargo test --workspace --quiet` — PASS; application 17, domain targets
  181/6/9, infrastructure 163 passed with 4 ignored, server 79, and worker
  116 passed with 1 ignored.
- `cargo check -p erpx-console --no-default-features --features web` — PASS.
- `cargo check -p erpx-console --no-default-features --features desktop` — PASS.
- Application privacy focused tests — 6 PASS; infrastructure privacy adapter
  tests — 2 PASS; server Marketing transport tests — 4 PASS; provider focused
  tests — 7 PASS.
- Fresh disposable two-database application erasure proof — PASS; application
  port write, idempotent replay, contact-free source/outbox/snapshots,
  redacted empty-read rebuild, forced-RLS routing, and exact cleanup with no
  `erpx_mkt063_*` residue.
- `bun run lint` — PASS with four pre-existing warnings in
  `infra/scripts/start-n8n.js`, zero errors.
- `bunx tsc --noEmit` — PASS.
- Folio `/operations` — HTTP 307 PASS; development server remains on port 3004.
- `pg_isready -h /tmp -p 5432` — accepting connections PASS.
- `git diff --check` — PASS.

Folio remains operational. The persistent ERPX goal remains active.

## Append-only integration record — G10-MKT-064 Anonymous Marketing abuse controls (true EOF)

Main independently reviewed and integrated the pure domain abuse policy,
durable control-plane quota, and public HTTP ingress wiring. The policy binds
tenant/resource/hostname-derived identities, validates bounded opaque
fingerprints, rejects honeypots, exposes explicit challenge outcomes, and
redacts policy diagnostics. The server uses a configured keyed HMAC fingerprint
and fails closed when it is absent. Control migration 0047 atomically counts
minute windows keyed by tenant, holding, normalized hostname, resource kind,
and resource ID; no IP, contact, token, or body value is stored. The quota is
consumed before tenant database routing and exhausted windows return 429.

Evidence: `audits/G10-MKT-064/README.md`, the 7 domain policy tests, 2 server
abuse tests, static migration test, and the fresh disposable PostgreSQL proof.

## Verification wave 2026-07-26 — G10-MKT-064 full integration (true EOF)

- `cargo fmt --all --check` — PASS.
- `cargo clippy --workspace --all-targets -- -D warnings` — PASS.
- `cargo test --workspace` — PASS; application 17, domain 188, infrastructure
  suite passed with its existing ignored disposable tests, server 81, worker
  117, and all other workspace targets passed.
- `cargo check -p erpx-console --no-default-features --features web` — PASS.
- `cargo check -p erpx-console --no-default-features --features desktop` — PASS.
- Focused domain abuse policy — 7 PASS; server abuse signal/binding tests — 2
  PASS; infrastructure migration test — PASS.
- Fresh disposable control-plane PostgreSQL proof — PASS; atomic two-request
  allowance, third-request rejection, new-window allowance, and database
  cleanup verified.
- `bun run lint` — PASS with four pre-existing warnings in
  `infra/scripts/start-n8n.js`, zero errors.
- `bunx tsc --noEmit` — PASS.
- Folio `/operations` — HTTP 307 PASS; development server remains on port 3004.
- `pg_isready -h /tmp -p 5432` — accepting connections PASS.
- `git diff --check` — PASS.

Folio remains operational. The persistent ERPX goal remains active.

## Append-only integration record — G10-MKT-064-B Durable abuse-window cleanup worker (true EOF)

Main independently reviewed the AG-MKT-064-B worker handoff, corrected its
test-only type-complexity issue, exported the module, and registered the
handler in the executable composition root. The opt-in handler uses only the
control-plane purge port, enforces a bounded batch and 24-hour cutoff, rejects
nil worker IDs, and redacts persistence failures. No tenant route or client
identity is accessed.

Evidence: `audits/G10-MKT-064-B/README.md` and the fresh disposable
`marketing_public_abuse` PostgreSQL proof, including bounded cleanup and zero
residue before database drop.

## Verification wave 2026-07-26 — G10-MKT-064-B full integration (true EOF)

- `cargo fmt --all --check` — PASS.
- `cargo clippy --workspace --all-targets -- -D warnings` — PASS.
- `cargo test --workspace` — PASS; all workspace targets passed with only the
  existing explicitly ignored disposable-database tests.
- `cargo check -p erpx-console --no-default-features --features web` — PASS.
- `cargo check -p erpx-console --no-default-features --features desktop` — PASS.
- Worker cleanup focused tests — 6 PASS; workspace worker target finished with
  122 tests passed and 1 existing ignored test; worker check and strict Clippy
  — PASS.
- Infrastructure abuse migration/static test and strict Clippy — PASS.
- Fresh disposable control-plane abuse proof — PASS; atomic quota behavior,
  minute rollover, bounded purge, and exact residue cleanup verified.
- `bun run lint` — PASS with four pre-existing warnings in
  `infra/scripts/start-n8n.js`, zero errors.
- `bunx tsc --noEmit` — PASS.
- Folio `/operations` — HTTP 307 PASS; development server remains on port 3004.
- `pg_isready -h /tmp -p 5432` — accepting connections PASS.
- `git diff --check` — PASS.

Folio remains operational. The persistent ERPX goal remains active.

## Append-only integration record — G10-MKT-065 Marketing callback secret-resolution boundary (true EOF)

Main independently reviewed and integrated the callback-secret boundary. The
domain now validates bounded opaque secret-manager references and rejects raw
or unsafe forms without echoing values. Infrastructure provides an injected
resolver trait, a host-native `env:NAME` pilot resolver, an explicit
unconfigured fail-closed resolver, and zeroizing resolved material. The HTTP
callback path resolves the authoritative control-plane binding, verifies the
exact raw body through the injected resolver, drops the resolved secret before
tenant routing, and maps resolution failures to a generic configuration
response.

Production external secret-manager adapters and rotation/reconciliation remain
open and are not represented as complete by this packet.

Evidence: `audits/G10-MKT-065/README.md`; 5 domain reference tests, 3
infrastructure resolver tests, 3 server callback tests, and strict focused
Clippy checks.

## Verification wave 2026-07-26 — G10-MKT-065 full integration (true EOF)

- `cargo fmt --all --check` — PASS.
- `cargo clippy --workspace --all-targets -- -D warnings` — PASS.
- `cargo test --workspace` — PASS; all workspace targets passed, with only the
  existing explicitly ignored disposable-database tests remaining ignored.
- `cargo check -p erpx-console --no-default-features --features web` — PASS.
- `cargo check -p erpx-console --no-default-features --features desktop` — PASS.
- Focused callback-reference/resolver/HTTP tests — 5, 3, and 3 PASS.
- `bun run lint` — PASS with four pre-existing warnings in
  `infra/scripts/start-n8n.js`, zero errors.
- `bunx tsc --noEmit` — PASS.
- Folio `/operations` — HTTP 307 PASS; development server remains on port
  3004.
- `pg_isready -h /tmp -p 5432` — accepting connections PASS.
- `git diff --check` — PASS.

Folio remains operational. The persistent ERPX goal remains active.

## Append-only integration record — G10-MKT-066 Production Marketing callback secret-manager adapter (true EOF)

Main independently reviewed and integrated the bounded vendor-neutral HTTPS
secret-manager adapter. `http` and `secret-manager` resolver modes now load a
fixed HTTPS gateway from server composition, reject unsafe endpoint forms,
disable redirects, bound timeout and response bytes, encode the validated
manager reference as one path segment, and accept only the allow-listed
`value`/`secret` JSON response. Non-manager refs fail closed, resolver errors
are redacted, and material is validated through the zeroizing resolved-secret
boundary. Pilot env and explicit unconfigured modes remain unchanged.

Vendor-native AWS/Vault adapters, endpoint health/rotation reconciliation,
deployment identity binding, and live TLS proof remain open.

Evidence: `audits/G10-MKT-066/README.md`; 4 infrastructure adapter tests, 3
server callback tests, strict infrastructure/server/workspace Clippy, and the
full workspace verification wave.

## Verification wave 2026-07-26 — G10-MKT-066 full integration (true EOF)

- `cargo fmt --all --check` — PASS.
- `cargo clippy --workspace --all-targets -- -D warnings` — PASS.
- `cargo test --workspace` — PASS; all workspace targets passed, with only the
  existing explicitly ignored disposable-database tests remaining ignored.
- `cargo check -p erpx-console --no-default-features --features web` — PASS.
- `cargo check -p erpx-console --no-default-features --features desktop` — PASS.
- Infrastructure secret-manager focused tests — 4 PASS; server callback
  focused tests — 3 PASS.
- `bun run lint` — PASS with four pre-existing warnings in
  `infra/scripts/start-n8n.js`, zero errors.
- `bunx tsc --noEmit` — PASS.
- Folio `/operations` — HTTP 307 PASS; development server remains on port
  3004.
- `pg_isready -h /tmp -p 5432` — accepting connections PASS.
- `git diff --check` — PASS.

Folio remains operational. The persistent ERPX goal remains active.

## Append-only integration record — G10-MKT-067 Marketing delivery secret-manager bridge (true EOF)

Main independently reviewed and integrated the worker bridge. Account-bound
Marketing delivery construction now accepts an injected credential resolver;
the executable selects pilot env, production HTTP secret-manager, or explicit
unconfigured mode. Manager references pass through the shared validated
opaque-reference contract, resolved bytes are converted only into the
worker’s zeroizing credential type, and invalid constructor paths zeroize
temporary bytes. Existing provider-key, account-binding, tenant/holding,
scope, idempotency, and version checks remain authoritative.

Vendor-native provider SDKs, channel onboarding, provider reconciliation,
durable delivery DLQ scheduling, and live external provider evidence remain
open.

Evidence: `audits/G10-MKT-067/README.md`; 3 focused worker resolver tests,
125 full worker tests with 1 existing ignored test, strict worker/workspace
Clippy, and the full workspace verification wave.

## Verification wave 2026-07-26 — G10-MKT-067 full integration (true EOF)

- `cargo fmt --all --check` — PASS.
- `cargo clippy --workspace --all-targets -- -D warnings` — PASS.
- `cargo test --workspace` — PASS; all workspace targets passed, with only the
  existing explicitly ignored disposable-database tests remaining ignored.
- `cargo check -p erpx-console --no-default-features --features web` — PASS.
- `cargo check -p erpx-console --no-default-features --features desktop` — PASS.
- Worker resolver focused tests — 3 PASS; full worker target — 125 PASS and 1
  existing ignored test.
- `bun run lint` — PASS with four pre-existing warnings in
  `infra/scripts/start-n8n.js`, zero errors.
- `bunx tsc --noEmit` — PASS.
- Folio `/operations` — HTTP 307 PASS; development server remains on port
  3004.
- `pg_isready -h /tmp -p 5432` — accepting connections PASS.
- `git diff --check` — PASS.

Folio remains operational. The persistent ERPX goal remains active.

## Append-only integration record — G10-MKT-068 Multi-tenant Marketing delivery execution (true EOF)

Main independently reviewed the worker composition and corrected the initial
documentation placement before integration. The delivery executable now
enumerates active tenant/holding registrations through a SQL-bounded
control-plane query, lists active outbound provider accounts through the
authoritative control-plane binding, and applies a global bounded claim budget
across those accounts. Each pass routes separate tenant write/read pools,
injects the shared credential resolver, and invokes the existing account-bound
durable delivery handler, preserving tenant-write claim leases, consent and
suppression rechecks, idempotency, retry scheduling, permanent failure, and
dead-letter state. The single-tenant environment selectors are no longer part
of this handler's composition.

The packet adds a focused fail-closed batch/lease configuration test and
independent compile/lint/workspace evidence. It does not claim a dedicated
control-plane delivery scheduler job/lease/DLQ, vendor-native provider
reconciliation, channel onboarding, external provider calls, disposable
multi-tenant database execution, or completion of the broader Marketing/ERP
gate.

Evidence: `audits/G10-MKT-068/README.md`; bounded active-registry and provider
account queries; 1 focused worker runtime test; workspace Clippy and tests;
Dioxus web/desktop checks; Folio lint/typecheck; live Folio/PostgreSQL health
probes.

## Verification wave 2026-07-26 — G10-MKT-068 full integration (true EOF)

- `cargo fmt --all --check` — PASS.
- `cargo clippy --workspace --all-targets -- -D warnings` — PASS.
- `cargo test --workspace` — PASS; all workspace targets passed, with only the
  existing explicitly ignored disposable-database tests remaining ignored.
- Focused worker runtime-bound test — 1 PASS; worker library target — 126
  PASS and 1 existing ignored database test; worker binary target — 1 PASS.
- `cargo check -p erpx-infrastructure`, `cargo check -p erpx-worker`, and
  `cargo check -p erpx-server` — PASS.
- `cargo check -p erpx-console --no-default-features --features web` — PASS.
- `cargo check -p erpx-console --no-default-features --features desktop` —
  PASS.
- `bun run lint` — PASS with four pre-existing warnings in
  `infra/scripts/start-n8n.js`, zero errors.
- `bunx tsc --noEmit` — PASS.
- Folio `/operations` — HTTP 307 PASS; development server remains on port
  3004.
- `pg_isready -h 127.0.0.1 -p 5432` — accepting connections PASS.
- `git diff --check` — PASS.

Folio remains operational. The persistent ERPX goal remains active.

## Append-only integration record — G10-MKT-069 Durable Marketing delivery scheduler (true EOF)

Main independently reviewed and integrated the durable control-plane
Marketing delivery scheduler. Control migration 0048 adds recurring scheduler
and claim tables. The repository validates bounded policy and job references,
creates an idempotent global schedule, claims due or stale work with
`FOR UPDATE SKIP LOCKED`, renews half-open leases, reschedules successful
passes, applies bounded retry backoff, terminally dead-letters exhausted work,
and exposes explicit dead-letter replay. The worker wrapper renews the
control-plane lease while the existing multi-tenant delivery pass executes;
tenant delivery claims, provider results, consent, suppression, and
idempotency remain authoritative in tenant write databases.

The first disposable probe exposed and fixed a migration embedding cache
invalidation gap by updating the migration composition marker. The rerun
proved the new schema and full scheduler lifecycle against a fresh PostgreSQL
database, which was removed after the test. No external provider call or
broader Marketing completion is claimed.

Evidence: `audits/G10-MKT-069/README.md`; 2 infrastructure scheduler tests, 2
worker scheduler tests, 2 migration embedding tests, the fresh disposable
PostgreSQL lifecycle proof, workspace Clippy/tests, Dioxus checks, Folio
lint/typecheck, runtime probes, and diff hygiene.

## Verification wave 2026-07-26 — G10-MKT-069 full integration (true EOF)

- `cargo fmt --all --check` — PASS.
- `cargo clippy --workspace --all-targets -- -D warnings` — PASS.
- `cargo test --workspace` — PASS; all workspace targets passed, with only the
  existing explicitly ignored disposable-database tests remaining ignored.
- Worker library target — 127 PASS and 1 existing ignored database test;
  worker binary target — 1 PASS; infrastructure scheduler unit tests — 2
  PASS; worker scheduler unit tests — 2 PASS.
- Fresh disposable Marketing scheduler PostgreSQL proof — 1 PASS.
- Migration embedding tests — 2 PASS.
- `cargo check -p erpx-server` — PASS.
- `cargo check -p erpx-console --no-default-features --features web` — PASS.
- `cargo check -p erpx-console --no-default-features --features desktop` —
  PASS.
- `bun run lint` — PASS with four pre-existing warnings in
  `infra/scripts/start-n8n.js`, zero errors.
- `bunx tsc --noEmit` — PASS.
- Folio `/operations` — HTTP 307 PASS; development server remains on port
  3004.
- `pg_isready -h 127.0.0.1 -p 5432` — accepting connections PASS.
- `git diff --check` — PASS.

Folio remains operational. The persistent ERPX goal remains active.

## Append-only integration record — G10-MKT-070 Marketing provider idempotency reconciliation (true EOF)

Main independently reviewed and integrated the provider-neutral reconciliation
boundary. Channel routes now support an optional distinct reconciliation URL,
validated with the same HTTPS, host, userinfo, query, fragment, and byte
bounds as submission routes. On ambiguous submit outcomes, the adapter sends
only dispatch identity, the exact idempotency key, channel, and non-secret
account context to the reconciliation endpoint. It never resubmits the
dispatch record. Responses are bounded and accept only `accepted`, `pending`,
`not_found`, `retryable`, or `permanent`; accepted results require a valid
provider reference. Existing account binding and tenant delivery persistence
remain authoritative.

Evidence: `audits/G10-MKT-070/README.md`; 11 focused provider tests including
the local no-resubmit reconciliation proof; workspace Clippy/tests; Dioxus
checks; Folio lint/typecheck; runtime probes; and diff hygiene. Vendor-native
providers, onboarding, and live external-provider evidence remain open.

## Verification wave 2026-07-26 — G10-MKT-070 full integration (true EOF)

- `cargo fmt --all --check` — PASS.
- `cargo clippy --workspace --all-targets -- -D warnings` — PASS.
- `cargo test --workspace` — PASS; all workspace targets passed, with only the
  existing explicitly ignored disposable-database tests remaining ignored.
- Focused provider suite — 11 PASS; worker library target — 129 PASS and 1
  existing ignored database test; worker binary target — 1 PASS.
- `cargo check -p erpx-server` — PASS.
- `cargo check -p erpx-console --no-default-features --features web` — PASS.
- `cargo check -p erpx-console --no-default-features --features desktop` —
  PASS.
- `bun run lint` — PASS with four pre-existing warnings in
  `infra/scripts/start-n8n.js`, zero errors.
- `bunx tsc --noEmit` — PASS.
- Folio `/operations` — HTTP 307 PASS; development server remains on port
  3004.
- `pg_isready -h 127.0.0.1 -p 5432` — accepting connections PASS.
- `git diff --check` — PASS.

Folio remains operational. The persistent ERPX goal remains active.

## Append-only integration record — G10-MKT-071 Marketing provider onboarding and channel capability lifecycle (true EOF)

Main independently reviewed and integrated the provider onboarding boundary.
Control migration 0049 adds onboarding state, bounded channel capabilities,
verification timestamp, and hash-only verification metadata. The application
commands validate opaque secret-manager references and normalize channels.
The SQLx adapter atomically persists configuration, verification, and state
changes with idempotency, outbox, audit, and optimistic concurrency. New
configuration is suspended/pending; activation requires a separate verified
transition; suspension removes active routing. Tonic exposes the three
write commands, and worker account validation enforces the onboarded channel
set and shared secret-reference contract.

The disposable two-database proof passed all lifecycle assertions, including
replay, stale-version rejection, suspension, reactivation, and credential
reference redaction. This packet does not claim vendor-native SDKs, external
provider verification, live provider evidence, or broader G10/ERP completion.

Evidence: `audits/G10-MKT-071/README.md`.

## Verification wave 2026-07-26 — G10-MKT-071 full integration (true EOF)

- `cargo fmt --all --check` — PASS.
- `cargo clippy --workspace --all-targets -- -D warnings` — PASS.
- `cargo test --workspace` — PASS; only the existing explicitly ignored
  disposable-database tests remained ignored.
- Application onboarding unit test — 1 PASS; worker provider suite — 11
  PASS; disposable two-database PostgreSQL onboarding proof — 1 PASS.
- `cargo check -p erpx-console --no-default-features --features web` — PASS.
- `cargo check -p erpx-console --no-default-features --features desktop` —
  PASS.
- `bun run lint` — PASS with four pre-existing warnings in
  `infra/scripts/start-n8n.js`, zero errors.
- `bunx tsc --noEmit` — PASS.
- Folio `/operations` — HTTP 307 PASS; development server remains on port
  3004.
- `pg_isready -h 127.0.0.1 -p 5432` — accepting connections PASS.
- `git diff --check` — PASS.

Folio remains operational. The persistent ERPX goal remains active.

## Append-only integration record — G10-MKT-072 Provider account-bound channel routes (true EOF)

Main independently reviewed and integrated the route-configuration packet.
Control migration 0050 adds a JSONB route array to provider accounts. The
application command carries one normalized route per enabled channel. The
SQLx adapter validates credential-free HTTPS routes and persists them in the
same onboarding transaction, while repository bindings decode only the
non-secret route metadata. Tonic exposes the route configuration request, and
the worker account-bound constructor now builds endpoint and reconciliation
routes from the control-plane binding instead of per-channel environment URLs.

The disposable two-database proof passed route round-trip, idempotent replay,
verified activation, suspension/reactivation, stale-version rejection, audit,
and credential-reference redaction. The worker suite directly proved that the
account-bound constructor uses the control-plane endpoint. No vendor-native,
live external-provider, broader Marketing/ERP, cutover, or Folio-retirement
claim is made.

Evidence: `audits/G10-MKT-072/README.md`.

## Verification wave 2026-07-26 — G10-MKT-072 full integration (true EOF)

- `cargo fmt --all --check` — PASS.
- `cargo clippy --workspace --all-targets -- -D warnings` — PASS.
- `cargo test --workspace` — PASS; explicitly ignored disposable-database
  tests remained ignored.
- Application route contract — 1 PASS; worker Marketing provider suite — 12
  PASS; disposable two-database route/onboarding proof — 1 PASS.
- `cargo check -p erpx-console --no-default-features --features web` — PASS.
- `cargo check -p erpx-console --no-default-features --features desktop` —
  PASS.
- `bun run lint` — PASS with four pre-existing warnings in
  `infra/scripts/start-n8n.js`, zero errors.
- `bunx tsc --noEmit` — PASS.
- `git diff --check` — PASS.
- `pg_isready -h 127.0.0.1 -p 5432` — accepting connections PASS.
- Folio `/operations` — HTTP 307 PASS; development server remains on port
  3004.

Folio remains operational. The persistent ERPX goal remains active.

## Append-only integration record — G10-MKT-073 Channel-scoped Marketing delivery claims (true EOF)

Main independently reviewed and integrated the channel-claim boundary.
`SqlxMarketingDeliveryStore::new_for_channels` canonicalizes a bounded
provider account channel set. Its authoritative claim query applies the set
to the JSON dispatch channel before row locking and lease creation. The
multi-tenant worker now creates this filtered source for each active control
plane provider account, preventing an email-only account from claiming SMS
work and vice versa.

The disposable two-database tenant proof seeded due email and SMS dispatches
with valid consent and proved that separate email- and SMS-scoped sources each
claimed only their own channel. No priority policy for multiple accounts with
the same channel, vendor-native, live external-provider, broader ERP,
cutover, or Folio-retirement claim is made.

Evidence: `audits/G10-MKT-073/README.md`.

## Verification wave 2026-07-26 — G10-MKT-073 full integration (true EOF)

- `cargo fmt --all --check` — PASS.
- `cargo clippy --workspace --all-targets -- -D warnings` — PASS.
- `cargo test --workspace` — PASS; explicitly ignored disposable-database
  tests remained ignored.
- Infrastructure channel-filter unit coverage — 3 PASS; disposable
  two-database channel-claim proof — 1 PASS.
- `cargo check -p erpx-console --no-default-features --features web` — PASS.
- `cargo check -p erpx-console --no-default-features --features desktop` —
  PASS.
- `bun run lint` — PASS with four pre-existing warnings in
  `infra/scripts/start-n8n.js`, zero errors.
- `bunx tsc --noEmit` — PASS.
- `git diff --check` — PASS.
- `pg_isready -h 127.0.0.1 -p 5432` — accepting connections PASS.
- Folio `/operations` — HTTP 307 PASS; development server remains on port
  3004.

Folio remains operational. The persistent ERPX goal remains active.

## Append-only integration record — G10-MKT-074 Deterministic multi-account channel selection (true EOF)

Main independently reviewed and integrated explicit provider-account priority.
Control migration 0051 adds bounded priority metadata with a default of 100.
The onboarding command, SQLx lifecycle, safe receipt, Tonic request, and
worker binding carry the field without credential material. Active outbound
account discovery orders ascending priority and uses the opaque provider
account reference as its deterministic tie-breaker.

The fresh two-database control proof configured and activated priority-10 and
priority-100 accounts and verified the ordered binding list while retaining
all prior replay, concurrency, lifecycle, audit, and redaction assertions.
This packet makes no live failover, weighted traffic, vendor-native,
broader ERP, cutover, or Folio-retirement claim.

Evidence: `audits/G10-MKT-074/README.md`.

## Verification wave 2026-07-26 — G10-MKT-074 full integration (true EOF)

- `cargo fmt --all --check` — PASS.
- `cargo clippy --workspace --all-targets -- -D warnings` — PASS.
- `cargo test --workspace` — PASS; explicitly ignored disposable-database
  tests remained ignored.
- Fresh two-database priority/onboarding proof — 1 PASS.
- `cargo check -p erpx-console --no-default-features --features web` — PASS.
- `cargo check -p erpx-console --no-default-features --features desktop` —
  PASS.
- `bun run lint` — PASS with four pre-existing warnings in
  `infra/scripts/start-n8n.js`, zero errors.
- `bunx tsc --noEmit` — PASS.
- `git diff --check` — PASS.
- `pg_isready -h 127.0.0.1 -p 5432` — accepting connections PASS.
- Folio `/operations` — HTTP 307 PASS; development server remains on port
  3004.

Folio remains operational. The persistent ERPX goal remains active.

## Append-only integration record — G2-IAM-004 Recursive parent-chain authorization (true EOF)

Main independently reviewed and integrated recursive role-chain evaluation.
The SQLx control repository now returns the validated immediate-to-root chain
from authoritative role rows/snapshots. The shared evaluator rechecks every
edge for parent identity, half-open activity, scope containment, authority,
permission containment, missing ancestors, and cycles. The universal server
access, entitlement-check, and object-access paths use this complete chain;
legacy immediate-parent callers remain available.

Evidence: `audits/G2-IAM-004/README.md`.

This packet makes no claim of recursive support-intersection transport,
complete universal mutation adoption, durable session resolution in every
caller, approver identity resolution, role-review worker delivery, recovery
replay persistence, or G2-G14 completion.

## Verification wave 2026-07-26 — G2-IAM-004 focused integration (true EOF)

- `cargo fmt --all --check` — PASS.
- `cargo test -p erpx-domain access::tests` — PASS; 19 passed.
- `cargo check -p erpx-server` — PASS.

Folio remains operational. The persistent ERPX goal remains active.
## Append-only integration record — G6-PRV-028 final lifecycle verification note

The retained G6-PRV-028 evidence is independently verified in
`audits/G6-PRV-028/README.md`; the persistent ERPX goal remains active.
## Append-only integration record — G6-G12-UI-010 authenticated Platform Operations tenant list/detail

The Dioxus console now calls authenticated `ListTenants` and `GetTenant`
through both WebAssembly gRPC-Web and native Tonic adapters. Cursor paging,
opaque route validation, platform query context, actor/session metadata, and
customer-visible audit purpose are preserved. The UI renders only safe
registry/readiness fields with explicit loading, denied, error, empty, stale,
and read-only boundaries; credentials, database keys, storage namespaces,
backup references, and mutation controls are excluded.

Evidence: `audits/G6-G12-UI-010/README.md`.

The remaining Platform Operations routes, native secure storage, Android/iOS
build evidence, and full product journeys remain open. The persistent ERPX
goal remains active.

## Verification wave 2026-07-26 — G6-G12-UI-010 focused integration

- `cargo fmt --all --check` — PASS.
- `cargo check -p erpx-console --no-default-features --features web` — PASS.
- `cargo check -p erpx-console --no-default-features --features desktop` — PASS.
- Console route/model tests — PASS in `cargo test --workspace`.

Folio remains operational. The persistent ERPX goal remains active.

## Append-only verification correction — G6-PRV-028 QA findings resolved

The independent QA review initially flagged stale formatting evidence,
application-only holding binding, insufficient multi-page/stale-row proof, and
worker error propagation. Main independently reran the current tree after
formatting, added forward migration 0055 for the composite database binding,
expanded the fresh SQL fixture to force page-size-one rebuild and stale-row
removal, and mapped worker persistence/SQL failures to redacted operation
categories. Fresh infrastructure and worker disposable proofs passed again;
the bounded advisory candidate scan remains explicitly documented while exact
per-action claims fence concurrent workers. No MinIO/PITR or G6/G14 completion
claim is made.

The persistent ERPX goal remains active.
## Append-only integration record — G6-G12-UI-011 authenticated projection status surface

The Dioxus console now calls audited `ListProjectionStatuses` through both
gRPC-Web and native Tonic for `/platform/projections`. It follows bounded
opaque cursors and renders only tenant/holding/projection identity, kind,
state, attempts, freshness, and timestamps. Job identifiers, raw error text,
credentials, database keys, and mutation controls are excluded. Loading,
denied, empty, error, and read-only states remain explicit.

Evidence: `audits/G6-G12-UI-011/README.md`.

Projection rebuild commands, filtering, Android/iOS evidence, remaining
Platform Operations routes, and the broader G6/G12 gates remain open. The
persistent ERPX goal remains active.

## Verification wave 2026-07-26 — G6-G12-UI-011 focused integration

- `cargo fmt --all --check` — PASS.
- `cargo check -p erpx-console --no-default-features --features web` — PASS.
- `cargo check -p erpx-console --no-default-features --features desktop` — PASS.

Folio remains operational. The persistent ERPX goal remains active.

## Append-only integration record — G6-G12-UI-012 authenticated backup history (true EOF)

The Dioxus console consumes the audited `ListTenantBackupJobs` read for
`/platform/backups` through WebAssembly gRPC-Web and native Tonic. It uses the
authenticated tenant registry, caps the tenant fan-out at 100, and requests at
most 100 history rows per tenant. Only bounded job state is rendered; backup
object references, restore targets, physical database keys, credentials, and
raw errors remain excluded. No mutation or restore control is exposed.

Evidence: `audits/G6-G12-UI-012/README.md`.

## Verification wave 2026-07-26 — G6-G12-UI-012 focused integration (true EOF)

- `cargo fmt --all --check` — PASS.
- `cargo test -p erpx-console` — PASS; 20 tests.
- `cargo check -p erpx-console --no-default-features --features web` — PASS.
- `cargo check -p erpx-console --no-default-features --features desktop` — PASS.

Folio remains operational. The persistent ERPX goal remains active.

## Verification wave 2026-07-26 — G6-G12-UI-011 full integration (true EOF)

- `cargo fmt --all --check` — PASS.
- `cargo clippy --workspace --all-targets -- -D warnings` — PASS.
- `cargo test --workspace` — PASS; disposable PostgreSQL integration tests
  remain explicitly ignored without their required databases.
- `cargo check -p erpx-console --no-default-features --features web` — PASS.
- `cargo check -p erpx-console --no-default-features --features desktop` — PASS.
- `bun run lint` — PASS with four pre-existing warnings in
  `infra/scripts/start-n8n.js`; zero errors.
- `bunx tsc --noEmit` — PASS.
- PostgreSQL readiness — PASS; Folio `/operations` returned HTTP 307.
- `git diff --check` — PASS.

Folio remains operational. The persistent ERPX goal remains active.

## Entry 2026-07-27 — ERPX parallel completion wave: directory and product slices

**Owner:** Main coordinator; disjoint directory/auth, directory/sync, directory/RBAC,
G3-G6, G7, G8, G9-G11, G12, and G14 lanes were assigned/reassigned through the
available agent capacity.

The directory integration is now real write-side behavior rather than schema-only
work: TLS-only provider policy, immutable provider-key links, bounded one-million
user import planning, native feed cursor contracts, SQLx import/checkpoint/member
persistence, append-only role facts, five-minute revocation state, rebuildable
projection rows, transactional redacted outbox events, authenticated protobuf
connector/import/role-mapping transport, and explicit fail-closed worker
registration are integrated. G8 Finance, G9-G11 source reconciliation, and G12
typed UI/QA contract slices are also present with their own evidence packages.

No live LDAP/AD DS/UCS credentialed environment is available, so vendor acceptance
is not claimed. Queue generic LDAP (RFC 4533), AD DS (DirSync), and UCS
Provisioning Service certification plus million-user volume, five-minute
revocation, and two-database reconciliation runs. The directory worker refuses
to claim queued work without a real external adapter. No customer directory was
activated; ERPX has not taken production command ownership; Folio writes remain
operational.

The initial eight-agent spawn hit the runtime thread limit, but completed lanes
were closed and reused immediately; the remaining lanes were continued by the
coordinator and subsequently reassigned as capacity returned. This is an
execution constraint, not an ERPX acceptance claim.

## Verification wave 2026-07-27 — ERPX parallel completion final evidence

- `cargo fmt --all --check` — PASS.
- `cargo clippy --workspace --all-targets -- -D warnings` — PASS.
- `cargo test --workspace` — PASS; current non-ignored workspace targets are
  green, including G13 and G14 structural acceptance tests.
- `cargo test --workspace --no-fail-fast -- --ignored --nocapture` — exercised
  all ignored targets; 50 database-backed targets failed at their explicit
  missing disposable database URL guards. No ignored test had a configured
  database pair to certify.
- `cargo check -p erpx-proto --offline` and G13 protobuf compatibility markers —
  PASS.
- Dioxus console web, desktop, host-mobile, `aarch64-apple-ios`, and
  `aarch64-linux-android` checks — PASS.
- `bun run lint` — PASS with four pre-existing warnings in
  `infra/scripts/start-n8n.js`; zero errors.
- `bunx tsc --noEmit` — PASS.
- Folio `GET /operations` — HTTP 307 to the login boundary; the development
  server is listening on port 3004 in the background.

The complete ERPX acceptance gate remains open. Queue generic LDAP (RFC 4533),
AD DS (DirSync), UCS Provisioning Service, one-million-user volume, live
five-minute revocation, native device execution, disposable SQLx databases,
G13 rollback/cutover rehearsal, and final G14 evidence. No customer directory
was activated, Folio writes remain operational, and production command-owner
or cutover changes were not performed.

## Verification wave 2026-07-27 — coordinator integration continuation

- Worker focused tests — PASS; 158 `erpx-worker` library tests and 5 worker
  binary tests passed after registering platform, Finance, HR, and product
  projection runtime seams.
- `cargo clippy -p erpx-worker --all-targets -- -D warnings` — PASS.
- `cargo test -p erpx-server --bin erpx-server` — PASS; 84 passed, 1 ignored.
- Server platform tenant detail now consumes the proof-bound platform
  reconciliation read model through the existing authenticated RPC composition.
- Product scheduler registration now covers `scm`, `pos`, `sales`,
  `asset_maintenance`, `legal_documents`, `hr`, and `marketing`; malformed
  cursor/page and candidate payload inputs fail closed.

This is local composition evidence, not live acceptance. The ignored database
suite still lacks disposable control/tenant/read/rollback URLs; external
directory/vendor credentials and native device runners remain unavailable.
No customer directory was activated, Folio writes remain operational, and no
production command-owner or cutover action was performed.

## Verification wave 2026-07-27 — product transport and directory UI continuation

- `cargo fmt --all --check` — PASS after coordinator protobuf/server/console
  integration and the parallel Sales, SCM, and Asset/Maintenance packets.
- `cargo check -p erpx-server --offline` — PASS; typed Sales, SCM, and
  Asset/Maintenance RPC implementations compile with the existing auth,
  tenant-router, audit, and SQLx store boundaries.
- `cargo check -p erpx-console --offline` — PASS after cfg-gating the
  WebAssembly-only directory adapter; web targets consume the authorized
  DirectoryService read model and native targets fail closed without a
  fabricated transport.
- Sales, SCM, Asset/Maintenance, G9, G11, G12, and G14 focused packets report
  green local tests and explicit missing-database/vendor/device blockers.

This is implementation and structural evidence only. Live two-database
reconciliation, LDAP/AD DS/UCS certification, one-million-user volume,
five-minute revocation timing, native device execution, G13 rehearsal, and
final G14 acceptance remain open. Folio remains the operational write owner.

## Verification wave 2026-07-27 — G11 product-context shell integration and final local checks

- Coordinator wired actual read-only console routes, route tabs, deep-link paths,
  and AppShell navigation for Sales, Procurement, Inventory, Maintenance,
  HR/Payroll, Marketing/CRM, POS, and Legal/Documents. Each route reads the
  authenticated platform projection-status RPC and renders explicit state
  boundaries through `ProductCompletionSurface`; no fixture rows or mutation
  controls were introduced.
- `cargo test --offline -p erpx-migrator --test g11_product_context_acceptance
  -- --nocapture` — PASS; 7 passed, with all seven product surfaces reporting
  complete local API/server/console/read-surface markers. The verifier now
  asserts route wiring and no longer treats missing UI as a non-failing log.
- `cargo fmt --all --check` — PASS.
- `cargo clippy --workspace --all-targets --offline -- -D warnings` — PASS.
- `cargo test --workspace --offline --no-fail-fast` — PASS on the current tree;
  console 41, domain 216, identity 58, worker library 158 plus 5 worker binary,
  server 84 plus 1 ignored, and all non-ignored acceptance/read-model targets
  passed. The one ignored server and one ignored worker test are not live proof.
- `cargo test --workspace --offline --no-fail-fast -- --ignored --nocapture`
  — exercised all ignored targets; 51 database-backed targets failed at their
  explicit missing URL guards. Missing references include control/tenant
  write/read/admin/template, support, scheduler, and identity database URLs.
  No live database acceptance is claimed.
- `cargo check -p erpx-proto --offline` — PASS. G13 parallel acceptance 6/6 and
  deployment/recovery acceptance 4/4 passed; G14 directory acceptance 4/4
  passed; Sales/SCM/Asset and G11 focused packets are green.
- Dioxus checks — PASS serially for web, desktop, host mobile,
  `aarch64-apple-ios`, and `aarch64-linux-android`.
- `bun run lint` — PASS with four pre-existing warnings in
  `infra/scripts/start-n8n.js`, zero errors; `bunx tsc --noEmit` — PASS.
- `git diff --check` — PASS. Folio `GET /operations` returned HTTP 307 to the
  login boundary and node PID 26151 remains listening on `*:3004`.

External LDAP/AD DS/UCS environments and credentials, one-million-user volume,
live five-minute revocation timing, disposable SQLx databases, G13 migration /
rollback rehearsal, native device execution, and final G14 acceptance remain
queued. No activation, command-owner switch, Folio write retirement, or
production cutover was performed.

## Final ignored-suite confirmation 2026-07-27

The required ignored suite was rerun after the final console route integration:
`cargo test --workspace --offline --no-fail-fast -- --ignored --nocapture`.
It again exited with 51 targets failed, all at explicit missing disposable
database URL guards; no new source or compilation failure appeared. This
confirms the queued SQLx/environment blocker on the final tree.

## Final live ignored-suite confirmation 2026-07-27

- Fresh disposable databases were provisioned for control-write,
  control-read, tenant-write, tenant-read, and the worker commercial control
  path. No user or production database was used.
- `cargo test --workspace --offline --no-fail-fast -- --ignored --nocapture
  --test-threads=1` — PASS with the full live ignored suite. The earlier
  shared-fixture failures were resolved by scoped Finance backfill assertions,
  unique Finance banking fixture references, monotonic incremental-status
  checkpoints, unique registry pagination anchors, and the isolated worker
  commercial database.
- Live SQLx evidence is now present for identity sessions/passkeys/TOTP,
  commercial semantic processing, Finance backfill/chart/journal/banking,
  Operations, Customer 360, Organization, Sales, SCM, Asset/Maintenance,
  HR/Payroll, Legal, Marketing/CRM, POS, provisioning/replacement,
  readiness, support, scheduler, server owner seed, and worker projection
  paths. Each target completed through its authoritative persistence and
  projection/reconciliation assertions.
- `cargo check -p erpx-proto --offline` — PASS; G11 Asset/Product/Sales,
  G13 deployment/recovery and parallel, and G14 directory focused acceptance
  suites — PASS. Dioxus web, desktop, host-mobile, iOS, and Android target
  checks — PASS. Folio lint/typecheck and the port-3004 probe — PASS.

This supersedes the earlier missing-URL-only ignored-suite records for this
local run. External LDAP/AD DS/UCS credentials and environments, measured
million-user import, live five-minute revocation timing, native device
execution, provider/vendor acceptance, G13 rollback/cutover rehearsal, and
final G14 acceptance remain certification blockers. No directory activation,
production command-owner switch, Folio write retirement, or production
cutover was authorized or performed. The five disposable databases are
explicitly disposable test artifacts and are queued for cleanup after this
ledger entry is recorded.

## Coordinator continuation 2026-07-27 — durable G13 and focused live evidence

- Added control migration 0058 and control-read migration 0011 for durable G13
  runs, transitions, idempotency, checkpoints/work items, reconciliation,
  rollback evidence, and rebuildable status/read models.
- Added and verified `SqlxG13MigrationStore` with phase/version fences,
  request-hash idempotency, append-only evidence, MFA, local-only break-glass,
  and explicit cutover approval. Fresh disposable control-write/control-read
  PostgreSQL acceptance: PASS.
- Registered the MFA-bound `g13_migration` worker projector, audited read-only
  `MigrationService.ListMigrationStatuses`, and Dioxus Platform Settings
  migration status surface. No cutover or owner-switch RPC exists.
- Corrected and reran the identity directory volume suite: deterministic
  one-million-user import, restart/replay, native cursor, five-minute
  revocation, append-only role, and local break-glass/MFA tests: PASS.
- Reran live SCM two-database cursor continuation, outbox, projection, scoped
  read, and reconciliation acceptance: PASS.

External generic LDAP/RFC 4533, AD DS/DirSync, UCS Provisioning Service,
physical mobile, approved protobuf descriptor, measured external revocation,
and live Folio source/target/rollback/cutover evidence remain queued. Folio
remains the operational write owner; no directory activation, write retirement,
owner switch, or production cutover occurred.

## Final coordinator verification confirmation — 2026-07-27

- Final `cargo fmt --all --check`, workspace Clippy with `-D warnings`,
  `cargo test --workspace --offline --no-fail-fast`, and `git diff --check`:
  PASS.
- Complete ignored workspace suite with fresh disposable PostgreSQL control
  write/read, tenant write/read, commercial, marketing-scheduler, and support
  databases: PASS after the commercial snapshot state was corrected from Rust
  enum casing to the authoritative snake_case read value. Databases were
  dropped after the run.
- `cargo check -p erpx-proto --offline`: PASS. G13 parallel/deployment and G14
  directory acceptance: PASS. Dioxus web, desktop, host-mobile,
  `aarch64-apple-ios`, and `aarch64-linux-android`: PASS.
- Folio `bun run lint`: PASS with the four pre-existing n8n-script warnings and
  zero errors. `bunx tsc --noEmit`: PASS. Folio port 3004 remained live at the
  protected-route HTTP 307 boundary.
- External LDAP/RFC 4533, AD DS/DirSync, UCS, physical-device, approved
  descriptor, measured external revocation, live Folio migration/rollback,
  retention, and production approval evidence remain queued. No activation,
  owner switch, write retirement, or cutover occurred.

## Coordinator continuation — live directory transports and G13 work leases — 2026-07-27

- `ldap3 0.12.1` is coordinator-owned in the workspace with only
  `tls-rustls-ring`; the worker has no native-TLS or plaintext directory path.
- The directory worker now executes real LDAPS/StartTLS LDAP paging, RFC 4533
  Sync refresh-only controls, AD DS DirSync BER controls, and the UCS
  Provisioning API subscription message/confirm sequence. It loads its durable
  checkpoint before polling, maps immutable directory IDs into identity links,
  commits through SQLx identity/outbox/read projection, and confirms a UCS
  sequence only after commit. Missing live credentials and malformed or stale
  protocol responses are fail-closed; UCS empty queues are idle.
- G13 now has durable idempotent work enqueue, expired-lease reclaim, bounded
  claim, completion, retry/dead-letter, and work outbox primitives. The G13
  runtime remains a status projector with zero migration claims until a real
  Folio source/ERPX target executor is supplied; it does not fabricate
  migration success.
- Verification: worker all-target tests PASS, infrastructure library tests
  PASS, offline worker/infrastructure checks PASS, and `cargo fmt --all --check`
  PASS.

Certification blockers remain: no external LDAP/AD/UCS endpoints or
credentials are available; no measured million-user run or five-minute live
revocation run exists; no physical mobile execution, approved protobuf
descriptor, live Folio source/target/rollback/retention evidence, or explicit
production cutover approval exists. No directory activation, owner switch,
Folio write retirement, or cutover was performed.

## Coordinator final verification addendum — 2026-07-27

- `cargo fmt --all --check`, workspace Clippy with `-D warnings`, the
  non-ignored workspace suite, and the complete ignored workspace suite using
  fresh disposable control/tenant/commercial/scheduler/support databases:
  PASS. The databases were removed after verification.
- `cargo check -p erpx-proto --offline`, G13/G14 focused acceptance, Dioxus
  web/desktop/host-mobile/iOS/Android target checks, `bun run lint`,
  `bunx tsc --noEmit`, and `git diff --check`: PASS. Folio lint has four
  pre-existing n8n-script warnings and zero errors.
- Folio port 3004 remained live at the protected-route HTTP 307 boundary.

The remaining queue is certification and controlled migration evidence:
credentialed LDAP/AD/UCS vendor acceptance, measured external one-million-user
import and five-minute revocation, physical-device execution, an approved
protobuf descriptor comparison, live Folio source/target/rollback/retention
rehearsal, post-cutover write rejection, and explicit production approval.
 No directory activation, command-owner switch, Folio write retirement, or
 cutover occurred.

## Coordinator continuation — support claims and G13 retention — 2026-07-27

- Added control migration `0060_support_sla_claim_identity.sql` and wired the
  SQLx support repository to persist claimant UUID, claim token, claim times,
  and a bounded lease. The worker acknowledges only the exact live claim
  returned by the repository after provider acceptance; stale/reclaimed claims
  affect zero rows and remain retryable. Focused support unit/integration
  tests and warnings-denied Clippy passed.
- Wired G13 target-worker retention handling to durable control state: expiry
  requires `folio_read_only`, a passed retention horizon, no pending or failed
  work, matched reconciliation, and successful rollback evidence. The importer
  validates the source fence and rollback rows before removing target and
  read-model rows, preserves rollback evidence, and appends an idempotent
  retention event.

These are local executable/disposable-database boundaries. Credentialed
Generic LDAP, AD DS/DirSync, and UCS certification, measured million-user and
five-minute external runs, physical mobile execution, approved descriptor
comparison, live Folio migration/retention/post-cutover write-rejection, and
explicit production approval remain queued. Folio remains the operational
write owner.

## Coordinator continuation — reconciliation migration and G1 conflict fence — 2026-07-27

- Added identity migration `0002_directory_reconciliation_runs.sql` and wired
  the identity migration helper to apply the forward fragment. Reconciliation
  runs and the directory outbox envelope metadata are now authoritative SQLx
  persistence rather than a schema-only dependency.
- `reconcile_import` now persists the report and its deterministic,
  connector-scoped reconciliation outbox event in one transaction. Fresh local
  directory SQLx acceptance: 2/2; ignored identity SQLx acceptance: 6/6.
- The G1 checkpoint projector rejects a different event ID at the current
  aggregate version, releases the claimed event, and fails closed. Fresh
  control-plane SQLx acceptance: 1/1. Current workspace Clippy with warnings
  denied, non-ignored tests, fresh local ignored database targets, protobuf,
  Dioxus web/desktop/host-mobile/iOS/Android, Folio lint/typecheck, and the
  protected port-3004 probe are green.

The complete ignored aggregate retains two explicit failures: the G14 vendor
preflight reports all missing Generic LDAP/RFC 4533, AD DS/DirSync, and UCS
Provisioning Service references, and the backup external-credentials test is
fixture-order sensitive when aggregated. The backup test passes independently
on a fresh control database and records the intended retryable blocker. No
vendor certification, measured external import/revocation, physical mobile
execution, approved descriptor comparison, live Folio migration/retention or
post-cutover write-rejection evidence, directory activation, command-owner
switch, Folio write retirement, or production cutover is claimed.

## Coordinator verification correction — current tree after checkpoint fix — 2026-07-27

The current-tree acceptance run found and fixed a real outbox projector defect:
checkpoint advancement previously filtered aggregate versions above one before
the `ON CONFLICT` update could run. It now preserves the version-one insert
fence, reaches the atomic conflict update for sequential versions, and remains
fail-closed for out-of-order first events. Fresh Customer 360 revocation and
backup-blocker SQLx tests pass after this fix.

Current evidence is green for `cargo fmt --all --check`, workspace Clippy with
warnings denied, non-ignored workspace tests, protobuf tests, G13 parallel and
deployment/recovery suites, G14 directory/readiness suites, all Dioxus
web/desktop/host-mobile/iOS/Android checks, `bun run lint`,
`bunx tsc --noEmit`, `git diff --check`, and the Folio port-3004 protected-route
probe. The fresh aggregate ignored SQLx run passes every local database-backed
target except `erpx-migrator --test g14_external_readiness` (the intentional
external certification blocker) and `erpx-worker --test backup_external_blocker`
when aggregated with other fixtures. The latter passes independently on a
fresh control database, so the aggregate result remains an integration-order
gap, not a product failure or a full ignored-suite pass.

The explicit ignored G14 preflight fails closed with the exact missing Generic
LDAP/RFC 4533, AD DS/DirSync, and UCS Provisioning Service database, endpoint,
bind or subscription, CA fingerprint, server-name, and TLS-mode references.
No vendor acceptance is claimed. The disposable verification databases for
this run were removed afterward. G0–G14 closure remains unclaimed pending
credentialed vendor acceptance, measured external import and revocation,
physical mobile execution, approved descriptor comparison, live Folio
migration/retention/post-cutover write-rejection evidence, and explicit
production approval. No directory activation, command-owner switch, Folio
write retirement, or production cutover occurred.

## Entry 2026-07-28 — G11 HR complete read bundle

PASS: the authenticated HR/Payroll route now consumes the existing typed
employee, leave, expense, payroll-run/line, and expired-role reassignment
read RPCs under one explicit tenant/holding/company/branch scope. The console
adapter preserves only bounded opaque/scalar facts and reconciled payroll
totals, validates scope containment, lifecycle/date/cursor/source metadata,
currency and amount bounds, and payroll line/aggregate reconciliation, and
renders explicit read-only sections. Credentials, accounts, tax, bank,
payment, attachment, raw payload, and identity-proof material remain outside
the client. Server authorization, customer audit, SQLx read projection,
outbox history, freshness, and rebuild remain authoritative.

Evidence: audit G11-UI-HR-CONTEXT-READ; console tests 100/100; package and
workspace warnings-denied Clippy pass; workspace non-ignored tests pass;
console web/desktop/host-mobile/iOS/Android and shared UI iOS/Android checks
pass. The required ignored sweep remains exactly 66 queued
missing-resource/certification targets. No migration, server/worker
registration, activation, owner switch, Folio write retirement, or
production cutover occurred.

## Entry 2026-07-31 — live HTTPS backup and physical PITR correction

CORRECTED: the local backup/PITR lanes are no longer merely queued. Using
PostgreSQL 18 over `sslmode=verify-full`, PostgreSQL 18 `pg_dump`/
`pg_restore`/`pg_basebackup`/`pg_ctl`/`psql`, a disposable PostgreSQL WAL-
archiving cluster, and HTTPS MinIO with a private CA, the real live backup
acceptance passed dump/upload/inventory, candidate restore with tenant/RLS
verification, routed activation, failed restore rollback, and cleanup.

PASS: the real physical PITR acceptance captured base/WAL artifacts, verified
HTTPS object hashes, restored an isolated candidate, paused at or beyond the
requested LSN, rejected an intentionally corrupted artifact with durable
failure/cleanup evidence, and removed the disposable candidate. The verifier
uses paused recovery plus `pg_last_wal_replay_lsn() >= target_lsn`, matching
PostgreSQL's valid replay within a WAL segment. The exact aggregate local
ignored run now has only the unavailable credentialed Generic LDAP/RFC 4533,
AD DS/DirSync, and UCS Provisioning Service certification target remaining;
measured external one-million-user/revocation and production-approval gates
remain queued. No activation, command-owner switch, Folio-write retirement,
or production cutover occurred.

## Entry 2026-07-31 — final current-tree verification after PITR correction

PASS: serialized `cargo fmt --all --check`; workspace Clippy with warnings
denied; the complete non-ignored workspace and doc-test suite; protobuf plus
G13/G14 compatibility; console web/desktop/mobile and iOS/Android target
checks; shared UI iOS/Android checks; Folio `bun run lint` and `bunx tsc
--noEmit`; bridge/step-up/Operations tests; `git diff --check`; and live
port-3004 probes. Folio lint retained four pre-existing n8n warnings and no
errors.

PASS: the real ignored `backup_live` and `pitr_live` targets were rerun after
the final PITR verifier change. Backup passed PostgreSQL 18 TLS dump/upload,
HTTPS MinIO inventory, routed candidate restore, rollback, and cleanup. PITR
passed real base/WAL capture, HTTPS object verification, paused recovery at or
beyond the target LSN, corrupted-artifact rollback evidence, and cleanup. The
disposable PostgreSQL 18 source cluster and MinIO process were stopped after
acceptance, and their temporary databases/artifacts were removed.

QUEUED, FAIL-CLOSED: the exact no-input aggregate
`cargo test --workspace --offline --no-fail-fast -- --ignored --nocapture
--test-threads=1` exited 101 with 86 environment-gated targets; raw output is
`/tmp/erpx-ignored-20260731-postpitr.log`. The isolated local SQLx matrix
passed its configured targets, while credentialed Generic LDAP/RFC 4533, AD
DS/DirSync, UCS, approved descriptor, device/provider, measured external
scale/revocation, and production-approval inputs remain unavailable. No
directory activation, command-owner switch, Folio-write retirement, or
production cutover occurred.

## Entry 2026-07-30 — integrated parallel completion evidence

IMPLEMENTED: parallel directory, platform, operations, finance, product,
migration, and UI/QA lanes were integrated under coordinator ownership of
shared manifests, protobuf, registrations, authorization, migrations, the
console shell, and verification. Native directory cursor fencing and
fail-closed reconciliation cover Generic LDAP/RFC 4533, AD DS/DirSync, and
UCS provisioning; identity links are immutable and role revocations are
append-only.

PASS: fresh disposable PostgreSQL acceptance passed all eight G9 contexts and
all ten G11 contexts. G13 parallel passed 15/15; G14 local compatibility
passed 13/13; G14 directory acceptance passed 12/12; isolated G13 tamper/replay
target-import SQLx acceptance passed 1/1. The complete local verification set
passed fmt, workspace Clippy `-D warnings`, non-ignored workspace tests,
protobuf checks/tests, Dioxus web/desktop/mobile plus iOS/Android target
checks, Android bridge checks, `bun run lint`, `bunx tsc --noEmit`, and
`git diff --check`.

RUNTIME: Folio stayed on port 3004 with `/`=200 and protected platform and
operations routes returning 307. No production activation, command-owner
switch, Folio write retirement, or cutover occurred.

QUEUED: the exact ignored workspace sweep is exit 101 with 82 explicit
fail-closed external-resource guards. LDAP/AD/UCS credentials and endpoints,
1M-user vendor-scale measurements, approved descriptor provenance, production
backup/PITR targets, device/provider evidence, and production approval remain
required for certification. No local/static result is promoted to vendor
acceptance.

## Entry 2026-07-30 — entitlement command current-tree verification

PASS: serialized `cargo fmt --all --check`, workspace warnings-denied Clippy,
and the complete non-ignored workspace/doc-test suite passed after the G5
entitlement packet. Protobuf, G13 parallel/recovery (14/14 and 6/6), G14
local/directory (11/11 and 11/11), console web/desktop/host-mobile/iOS/
Android, shared UI host/iOS/Android, Android bridge, Folio lint/typecheck,
`git diff --check`, and port-3004 probes passed. Folio lint returned zero
errors and four pre-existing n8n warnings; no production Next.js build ran.

The exact ignored command exited 101 with exactly 81 explicit missing-resource
or certification guards. The entitlement SQLx acceptance was separately
executed 1/1 with fresh disposable PostgreSQL databases. No application
assertion failed in the guarded sweep.

QUEUED: credentialed Generic LDAP/RFC 4533, AD DS/DirSync, UCS Provisioning
Service, approved protobuf provenance, physical PITR/device/scale evidence,
live Stripe/catalog reconciliation, and production approval. G0–G14 remain
active; no directory activation, command-owner switch, Folio write retirement,
or production cutover occurred.

## Entry 2026-07-30 — approved Marketing integration boundary correction

The current implementation has an authoritative approved-integration boundary:
journey execution re-resolves an active, verified holding-bound provider,
persists an idempotent invocation/outbox/read-model record, and the registered
worker performs TLS-only delivery with bounded retry and reconciliation. Older
entries calling this node absent are historical. External provider credentials
and live certification remain queued; no activation or production cutover was
performed.

## Coordinator continuation — durable approved Marketing journey integration — 2026-07-30

PASS: `InvokeApprovedIntegration` is now an authoritative domain handoff. The
journey executor re-reads profile scope from tenant-write SQLx, resolves the
active verified holding-bound provider account from the control plane, and
commits a deterministic invocation, snapshot, and outbox event atomically with
the cursor. Replays reuse the same invocation and idempotency identities.

PASS: tenant-write SQLx owns invocation state, leases, retries, bounded
provider outcomes, snapshots, and immutable versioned events. Tenant-read SQLx
owns a rebuildable projector, bounded read page, and source/read reconciliation.
A real PostgreSQL acceptance passed enqueue → claim → accepted result → two
source events → read projection → reconciliation, and disposable databases
were removed afterward.

PASS: the registered multi-holding worker re-resolves binding before each call,
requires the deployment provider key and shared secret-manager credential,
permits only HTTPS routes without credentials/query/fragment data, sends a
profile-redacted idempotent envelope, bounds responses, and reconciles
ambiguous requests when configured. Worker tests passed 344/344 non-ignored
library tests plus 7/7 binary tests; infrastructure/worker warnings-denied
Clippy passed. Existing authenticated provider onboarding RPCs and the
read-only Marketing Channels console document the exact `provider_account_ref`
handoff boundary.

QUEUED: external provider endpoint/credential certification is absent; the
worker remains fail-closed. No mock success path, customer-directory
activation, command-owner switch, Folio write retirement, or production
cutover occurred. G0–G14 remain active and are not claimed 100% complete.

## Coordinator verification correction — current-tree final sweep — 2026-07-27

Current-tree PASS evidence: `cargo fmt --all --check`; workspace Clippy with
`-D warnings`; workspace tests; protobuf tests and G13 compatibility suites;
Dioxus web, desktop, host-mobile, iOS, and Android checks; `bun run lint`;
`bunx tsc --noEmit`; `git diff --check`; and the running Folio port-3004
protected-route probe. The fresh ignored SQLx aggregate completed all targets
except `erpx-worker --test backup_external_blocker`; its isolated fresh-
database rerun passed and verified the intended failed backup run,
`retryable` state, and durable blocker error. The aggregate result is retained
as an integration-order verification gap, not relabeled as a full ignored-
suite pass.

The explicit ignored G14 certification preflight correctly failed closed with
the exact missing Generic LDAP/RFC 4533, AD DS/DirSync, and UCS Provisioning
Service environment references. The blocker is queued in
`audits/G14-CERTIFICATION-READINESS/README.md`; no vendor certification,
directory activation, command-owner switch, Folio write retirement, or
production cutover occurred. G0–G14 closure is not claimed.

## Coordinator continuation — product and directory wave — 2026-07-27

Accepted bounded packets added fail-closed backup/retention evidence, SQLx and
worker Operations detection batches, durable Marketing CRM-handoff journey
execution, typed G12 product-row evidence, and local directory import/resume,
role-mapping, AD/UCS change, revocation, replay, projection, and outbox tests.
Focused packet verification was reported green where disposable fixtures were
available; the coordinator still owns the serialized workspace rerun.

The directory vendor queue is unchanged: Generic LDAP/RFC 4533, AD DS/DirSync,
and UCS Provisioning Service credentials/endpoints/CA/server references are not
present, so no vendor certification is claimed. The local harness additionally
reports the migration-owned `erpx_identity.directory_reconciliation_runs`
dependency as queued. No customer directory activation, command-owner switch,
Folio write retirement, or production cutover occurred.

## Coordinator continuation — automatic bank-to-GL proposal path — 2026-07-27

The Finance banking persistence boundary now derives exact-match candidates
from immutable tenant-write posted-journal facts, validates journal payload and
match columns together, uses a deterministic proposal ID, and persists the
proposal through the existing idempotent snapshot/outbox path. The authorized
bank-provider import worker invokes this path for each bounded imported
transaction and records only bounded proposal-count evidence. No journal
posting or source mutation is performed. Focused domain/infrastructure/worker
verification is running in the current parallel wave; the broader G8 gate,
transport/UI, provider certification, and full serialized verification remain
open.

## Coordinator continuation — directory capacity and G13 read/shadow boundary — 2026-07-27

- Directory feed contract: exact one-million-user capacity accounting,
  bounded pages/checkpoints, and five-minute disable/delete revocation
  scheduling are covered by deterministic contract tests. Generic LDAP, AD
  DirSync, RFC 4533, and UCS Provisioning Service remain explicit provider
  paths; credentialed vendor acceptance is still queued.
- G13 target: separate tenant-read migration/read model, payload-free
  projection, durable shadow replay, complete immutable-link gate, explicit
  Folio write-fence-gated final delta, full outbox envelope metadata, rollback
  capture/restore, and retention expiry with append-only evidence are now
  locally executable. No source writes, command-owner switch, or cutover was
  performed.
- Final coordinator evidence: `cargo fmt --all --check`, workspace Clippy
  with `-D warnings`, non-ignored workspace tests, complete ignored SQLx tests
  on fresh disposable control/tenant/commercial/scheduler/support and G13
  target-read/rollback databases, protobuf/G13 compatibility suites, Dioxus
  web/desktop/mobile/iOS/Android checks, `bun run lint`, `bunx tsc --noEmit`,
  `git diff --check`, and Folio `/operations` on port 3004 (HTTP 307) passed.

This remains local implementation and disposable-database evidence. External
LDAP/AD/UCS credentials, measured external volume/revocation, physical mobile
execution, approved descriptor comparison, credentialed Folio source/target/
rollback/retention/post-cutover evidence, and explicit production approval
remain queued. No directory activation, command-owner switch, Folio write
retirement, or production cutover occurred.

## Entry 2026-07-28 — final verification after G7 source producer RPC

PASS: after the additive source-producer protobuf RPC, serialized format,
warnings-denied workspace Clippy, and the complete non-ignored workspace test
suite passed again. G13 protobuf compatibility passed 9/9 and G14 local
compatibility passed 5/5. All five Dioxus console checks (web, desktop,
host-mobile, iOS, Android) passed.

QUEUED: the final mandated ignored sweep again exited 101 with exactly 70
environment-gated targets. The blockers remain absent disposable SQLx/
MinIO/PITR resources, credentialed Generic LDAP/RFC 4533, on-premises AD
DS/DirSync, UCS Provisioning Service, operator-revocation evidence, approved
descriptor baseline, physical mobile execution, and explicit production
migration/cutover approval. G0–G14 remain open; no production state changed.

## Entry 2026-07-28 — G7 lifecycle MFA step-up

PASS: `TransitionOperationsException` now carries an ephemeral `mfa_code`.
The server requires fresh write-side TOTP verification for assign, reassign,
and escalate before tenant routing or SQLx mutation. Acknowledge and resolve
remain lower-risk lifecycle commands. The Dioxus console exposes the code as
a password/one-time-code input and does not place it in command context,
events, receipts, or read models.

PASS: server, native console, and console web target checks passed after the
protobuf and UI boundary change. The existing manager-chain proof remains
required for manager-bearing transitions.

QUEUED: live TOTP/device execution, provider/device notification delivery,
credentialed directory certification, physical mobile execution, PITR/WAL
proof, and explicit production approval remain open. No activation, owner
switch, Folio write retirement, or production cutover occurred.

## Coordinator final verification addendum — boundary hardening — 2026-07-27

- Integrated the final object-storage boundary hardening: operational signing
  and HTTP access require HTTPS, namespaces and keys are validated, inventory
  truncation is bounded and rejected, and endpoint credentials/query data are
  redacted from errors.
- Integrated G2 role-review reconciliation fencing: notifications require the
  exact authoritative count and partial acknowledgements release retryable
  work. Integrated Finance subledger replay fencing: checkpoint event ID and
  payload must match, while same-version conflicts, checkpoint/read-row
  divergence, and orphaned rows fail closed without mutation.
- Focused object-storage tests passed 8/8; role-review unit tests passed 6/6;
  the live role-review SQLx acceptance and live Finance subledger SQLx
  acceptance each passed against fresh disposable databases.
- Final workspace Clippy and non-ignored workspace tests passed. The serial
  ignored suite retains only the intentional G14 external-readiness preflight;
  Stripe and backup worker acceptance now pass in the aggregate run through
  their own disposable database isolation.

Credentialed Generic LDAP/RFC 4533, AD DS/DirSync, and UCS Provisioning
Service certification, measured million-user and five-minute revocation runs,
physical mobile execution, approved protobuf descriptor comparison, live
Folio migration/rollback/retention/post-cutover write rejection, and explicit
production approval remain queued. Folio remains the operational write owner;
no directory activation, command-owner switch, Folio write retirement, or
production cutover occurred.

## Entry 2026-07-28 — G10 Marketing profile erasure command boundary

PASS: the Marketing/CRM surface now submits the authoritative
`EraseMarketingProfile` command with bounded profile UUID, reason, and
optional RFC3339 timestamp. The console binds explicit authenticated scope,
marks the command as marketing-audited, decodes a typed privacy receipt, and
keeps profile content out of the client contract. Server entitlement,
scope-containment, source redaction, SQLx privacy mutation, outbox/read
projection rebuild, idempotency, reconciliation, and rollback remain
authoritative. Console coverage remains 99/99 and includes privacy command
context, bounds, identity, replay receipt, and malformed UUID assertions.
Broader Marketing evidence remains queued; no activation or cutover occurred.

## Entry 2026-07-28 — G10 Marketing privacy current-tree verification

PASS: focused `erpx-console` formatting and tests pass after adding profile
erasure. The serialized full workspace verification is queued for this tree;
the known ignored sweep remains exactly 66 explicit missing-resource/
certification targets. These are queued blockers, not completion evidence. No
temporary databases, activation, owner switch, Folio write retirement, or
production cutover occurred.

## Entry 2026-07-28 — G11 HR employee command boundary

PASS: the HR command form now submits `SaveHrEmployee` in addition to the
existing leave, expense, and payroll routes. Employee JSON is bounded and
scope-bound to holding/company/optional branch; expected versions are carried
when supplied; client employee command identity and idempotency facts are
replaced with the authenticated HR command context; typed receipts and read
refresh use the existing server boundary. HR domain validation, hr.write
authorization/entitlement, SQLx persistence, audit/outbox, worker/read
projection, idempotency, reconciliation, and rollback remain authoritative.
Console coverage remains 99/99 and includes employee identity/context
assertions. Broader HR/payroll/privacy/device/reconciliation/live SQLx
evidence remains queued; no activation or cutover occurred.

## Entry 2026-07-28 — G11 HR employee current-tree verification

PASS: `cargo fmt --all` and the focused `erpx-console` test suite pass after
adding the employee route. The serialized full workspace verification and
exact ignored sweep remain queued; the ignored sweep is still 66 explicit
missing-resource/certification targets. These are queued blockers, not
completion evidence. No temporary databases, activation, owner switch, Folio
write retirement, or production cutover occurred.

## Coordinator final verification addendum — G14 harness and ignored-suite isolation — 2026-07-27

- G14 live acceptance now requires `FULL_IMPORT=true` and
  `MAX_IMPORT_USERS=1000000` for every vendor. Generic LDAP/RFC 4533 requires
  operator-supplied real disable/delete evidence; AD DS/DirSync and UCS use
  fixed native feed kinds without unused `*_FEED` variables. Each harness
  validates immutable target identity, append-only revocation state, outbox,
  SLA, and observation timing, failing closed when provider evidence is
  absent, malformed, stale, or later than 300 seconds.
- Fixed ignored-suite fixture isolation for the Stripe semantic worker and
  backup external-credentials blocker. Both now create, migrate, and clean up
  dedicated disposable control databases, leaving production behavior and
  assertions unchanged.
- Fresh serial ignored-suite evidence: every local database-backed target
  passed, including support persistence/attachments and both isolated worker
  cases. The sole failed target was the intentional credentialless G14
  external-readiness preflight; all disposable databases were removed.
- Corrected G14 directory acceptance and live vendor focused suites passed;
  the approved protobuf descriptor remains absent, so only structural/local
  compatibility evidence is recorded.

Credentialed Generic LDAP/RFC 4533, AD DS/DirSync, and UCS certification,
measured million-user/five-minute revocation runs, physical mobile execution,
approved descriptor comparison, live Folio migration/rollback/retention/
post-cutover write rejection, and explicit production approval remain queued.
Folio remains the operational write owner; no directory activation,
command-owner switch, Folio write retirement, or production cutover occurred.

## Entry 2026-07-30 — final local verification and dev-runtime handoff

PASS: the final exact tree passed `cargo fmt --all --check`, workspace Clippy
with `-D warnings`, and the complete non-ignored workspace suite. Protobuf
check/tests, G13 acceptance (15/15), G14 local compatibility (11/11), G14
directory acceptance (12/12), console Dioxus web/desktop/mobile, shared UI
iOS/Android, and Android bridge target checks passed. Folio `bun run lint`
had zero errors and four pre-existing n8n warnings; `bunx tsc --noEmit` and
`git diff --check` passed.

PASS: Folio remains on port 3004. Turbopack's CSS-worker timeout was
reproducible during the handoff, so the local development process was
restarted with Next webpack dev mode without application-code changes; `/`
returns 200 and protected `/platform` and `/operations` return 307.

QUEUED: the exact ignored sweep exited 101 with 82 explicit fail-closed
resource/certification guards and no unexpected application assertion. The
queue includes credentialed Generic LDAP/RFC 4533, on-premises AD DS/DirSync,
UCS Provisioning Service, approved protobuf descriptor provenance, production
backup/PITR authorization, measured 1M-user/device/provider evidence, and
production approval. No static/local result was promoted to vendor acceptance;
no directory activation, command-owner change, Folio write retirement, or
production cutover occurred.

## Entry 2026-07-30 — current-tree certification addendum

The integrated local tree is verified on `codex/erpx-parallel-completion`.
The ignored sweep remains an exit-101 queue of 82 fail-closed
external-resource guards; LDAP/AD/UCS environments and credentials,
vendor-scale, approved descriptor, backup/PITR, device/provider, and explicit
production approval evidence remain unavailable. No local result is promoted
to vendor acceptance and no production activation or cutover occurred.

## Entry 2026-07-30 — current-tree certification addendum

The integrated local tree is verified on `codex/erpx-parallel-completion`.
The ignored sweep remains an exit-101 queue of 82 fail-closed
external-resource guards; LDAP/AD/UCS environments and credentials,
vendor-scale, approved descriptor, backup/PITR, device/provider, and explicit
production approval evidence remain unavailable. No local result is promoted
to vendor acceptance and no production activation or cutover occurred.

## Entry 2026-07-30 — parallel packet integration and serialized evidence

IMPLEMENTED: Generic LDAP/RFC 4533 acceptance now contains executable exact
1,000,000-user, cursor, reconciliation, rollback, immutable-link, and
directory-managed RBAC durability checks. AD/UCS adapters enforce bounded
native envelopes and TLS trust/handshake evidence. POS persistence validates
historical idempotent replay and source outbox timestamps. Legal projection
freshness compares source watermarks, approval metadata is immutable, and
rebuild clearing/replay is transactionally fenced. PITR/backup live tests
verify candidate fencing, generation/version checks, rollback, and cleanup.
UI/readiness tests cover MFA, credential redaction, break-glass-only fallback,
and explicit per-vendor certification queues.

PASS: serialized `cargo fmt --all --check`, workspace `cargo clippy` with
`-D warnings`, and `cargo test --workspace --offline --no-fail-fast --quiet`
passed. Protobuf, G13 (15/15), G14 local compatibility (13/13), G14 directory
(12/12), and G14 readiness (10 passed, 1 ignored) passed. Console web,
desktop, Android, and iOS checks; shared UI host/Android/iOS; and Android
bridge target checks passed. Folio lint/typecheck and port-3004 probes passed.

QUEUED: the exact ignored workspace sweep
`cargo test --workspace --offline --no-fail-fast -- --ignored --nocapture
--test-threads=1` exited 101 with 82 explicit fail-closed guards and no
unexpected application assertion. The queue is limited to unavailable
disposable SQLx databases, Generic LDAP/RFC 4533, AD DS/DirSync, UCS,
approved protobuf descriptor provenance, production backup/PITR authorization,
live scale/device/provider evidence, and explicit production approval. No
directory activation, command-owner switch, Folio write retirement, or
production cutover occurred.

## Entry 2026-07-30 — serialized final verification

PASS: `cargo fmt --all --check`, workspace Clippy with warnings denied, the
complete non-ignored workspace tests, protobuf tests, and selected G13/G14
compatibility suites passed. The mandated ignored sweep returned exactly 81
explicit missing-resource or certification guards; no unexpected application
assertion was observed. Dioxus console web/desktop/host-mobile/iOS/Android,
shared UI web/iOS/Android, and Android bridge checks passed. Folio `bun run
lint` and `bunx tsc --noEmit` passed, with four pre-existing n8n-script lint
warnings and zero errors; `git diff --check` passed; port 3004 returned 200 for
`/` and 307 for the protected platform route.

QUEUED: credentialed Generic LDAP/RFC 4533, on-premises AD DS/DirSync, UCS
Provisioning Service, approved protobuf descriptor provenance, production
backup/PITR and provider credentials, physical device/scale evidence, and
production approval. The guards remain queued and are not certification
passes; no directory activation, command-owner switch, Folio write retirement,
or production cutover occurred.

## Entry 2026-07-30 — parallel completion packets and final verification

PASS: parallel agents completed the remaining unblocked audits and fixes across
directory auth/sync, platform control, backup/PITR, Operations, Finance,
POS/SCM/Quality, Support/Customer 360, Legal/HR, UI/QA, and G13 runtime. The
directory fixes reject provider/transport mismatches before bind, retain
disabled/deleted identities for revocation, accept the UCS initial empty
cursor, and reject mixed connector/native-cursor changes. G10 owner-notification
replay now preserves the authoritative occurrence time and stable delivery
payload. G13 execution plans are typed, dependency-ordered, evidence-bound,
and rollback-safe before any cutover step.

PASS: local disposable PostgreSQL evidence passed identity 6/6, RBAC 2/2,
directory 3/3, Finance consolidation 2/2, Marketing journey 6/6, delivery 5/5,
scheduler 1/1, HR 5/5, Sales 4/4, SCM 10/10, POS 9/9, Quality 2/2 plus scope
1/1, Finance controls 10/10, and Operations authoritative/SQLx slices. These
were explicitly non-TLS local runs and are not vendor or production evidence.

PASS: final `cargo fmt --all --check`, workspace `cargo clippy --workspace
--all-targets --offline -- -D warnings`, complete non-ignored workspace tests,
protobuf/docs, G13 recovery/parallel 6/6 and 14/14, G14 local/directory 11/11
and 11/11, all console/shared-UI web/desktop/mobile/iOS/Android checks,
Android bridge, `bun run lint`, `bunx tsc --noEmit`, and port 3004 probes
passed. Folio lint has zero errors and four pre-existing n8n warnings; no
production Next.js build ran.

QUEUED: the exact ignored workspace sweep returned 81 explicit missing-resource
or certification guards and no unexpected assertion failure. Missing evidence
includes credentialed Generic LDAP/RFC 4533, AD DS/DirSync, UCS Provisioning
Service, remaining control/tenant SQLx URLs, approved protobuf provenance, TLS
MinIO/physical PITR, device/scale evidence, live provider credentials, G13
runtime references, and production approval. G0–G14 remain active rather than
being falsely marked complete; no activation, command-owner switch, Folio write
retirement, or production cutover occurred.

## Coordinator final parallel-completion addendum — 2026-07-27

- G7 Operations: immutable source/outbox envelope drift is detected and
  rollback completion requires a fresh healthy reconciliation.
- G8 Finance: close source-fence handling and journal projection replay now
  reject candidate movement, orphaned rows, version drift, and conflicting
  event payloads.
- G9–G11: Legal session provenance, Sales/SCM/POS/HR/Asset replay integrity,
  Marketing delivery/CRM complete outbox provenance, and bounded SCM cursor
  continuation are covered by SQLx, outbox, projection, reconciliation, and
  live disposable acceptance tests.
- G12/G14: product-completion UI exposes typed persistence/reconciliation/
  authorization evidence and explicit blocked/unavailable states; Generic
  LDAP, AD DS/DirSync, and UCS live tests use the real worker path and queue
  exact missing vendor references without claiming certification.
- G13: the durable Folio source bridge and target import/rollback boundary are
  covered by separate target/rollback PostgreSQL databases, including source
  snapshot reload, idempotent import, reconciliation, restore, checkpoints,
  and append-only rollback-replay events.

Final verification PASS: `cargo fmt --all --check`, workspace Clippy with
`-D warnings`, workspace tests, complete ignored SQLx tests using fresh
disposable databases, protobuf check, all Dioxus web/desktop/host-mobile/
iOS/Android target checks, `bun run lint`, `bunx tsc --noEmit`, `git diff
--check`, and Folio port 3004 (`/operations` returned HTTP 307). The four
Folio lint warnings remain the pre-existing n8n-script warnings.

This is not a G0–G14 closure claim. Missing credentialed LDAP/AD/UCS vendor
acceptance, measured external one-million-user/five-minute-revocation runs,
physical mobile execution, approved protobuf descriptor comparison, live
Folio source/target/retention/post-cutover rejection evidence, and explicit
production approval remain queued. No directory activation, command-owner
switch, Folio write retirement, or production cutover occurred.

## Coordinator continuation — executable G13 target boundary — 2026-07-27

- Added tenant-write migration `0050_g13_migration_import.sql` for immutable,
  RLS-scoped target import rows and separate rollback capture rows. Target
  imports emit append-only tenant outbox events and reject an existing row
  whose source hash/fence or deterministic target ID differs.
- Added `G13TargetImporter` and the registered `g13_target_import` worker.
  Backfill work imports only source identity scopes present in the retained
  source snapshot; source counts without an identity mapping remain visible as
  reconciliation differences. The worker saves durable checkpoints, records
  reconciliation evidence, and requires `ERPX_G13_ROLLBACK_WRITES_BLOCKED=true`
  for rollback work before capture/delete/restore and explicit rollback-replay
  outbox events.
- Disposable PostgreSQL acceptance `g13_target_import` PASS: two source
  identities imported, replay applied zero new rows, reconciliation matched,
  rollback restored both rows, and two rollback-replay events were appended.

This does not certify a credentialed Folio source or customer target. The
remaining G13/G14 queue includes live Folio source/target/rollback/retention
evidence, shadow replay/final delta, post-cutover write rejection, vendor
directory certification, measured volume/revocation, approved descriptor
comparison, and explicit production approval. No command-owner switch,
directory activation, Folio write retirement, or cutover occurred.

## Coordinator continuation — durable Folio source bridge — 2026-07-27

- Added control migration 0059 and control-read migration 0012 for bounded,
  idempotent Folio source snapshots and read-model summary fields. The source
  snapshot record stores the source contract hash, extractor version,
  missing-table/entity/row counts, capture time, actor/correlation metadata,
  and publishes `migration.source_snapshot_recorded` through the outbox.
- Added `g13_folio_bridge`, a registered worker handler that requires a
  TLS-only source database URL and verified MFA, extracts through the existing
  single read-only SQLx transaction, starts/replays a stable migration run,
  records the snapshot, and queues deterministic bounded backfill work. It
  reports no migration claims and never writes Folio business data or marks
  unexecuted target work successful.
- Live disposable `g13_migration_persistence` acceptance: PASS, including
  source-snapshot persistence, replay, projection, and read-model assertions.
  Worker all-target tests: PASS (168 library + 5 binary active tests); full
  workspace Clippy with warnings denied and non-ignored workspace tests: PASS.
- Complete ignored workspace SQLx suite: PASS against fresh disposable
  control-write/read, tenant-write/read, commercial, scheduler, support, and
  provisioning databases; all disposable databases were removed afterward.
  Protobuf compilation, Dioxus web/desktop/host-mobile/iOS/Android checks,
  Folio lint/typecheck, `git diff --check`, and the port-3004 HTTP 307 probe:
  PASS. Folio retains four pre-existing lint warnings and zero errors.

The bridge is local execution evidence, not vendor or production acceptance.
Credentialed LDAP/AD/UCS certification, measured million-user import and
five-minute external revocation, physical mobile execution, approved protobuf
descriptor comparison, live Folio source/target/rollback/retention evidence,
post-cutover write rejection, and explicit production approval remain queued.
No directory activation, command-owner switch, Folio write retirement, or
cutover occurred.

## Entry 2026-07-30 — full-import checkpoint rollback boundary

The coordinator audit found and fixed a real native-directory consistency gap:
full-import staging previously advanced `directory_sync_checkpoints` before
canonical import finalization. A late immutable-link, role, or projection
failure could therefore leave incremental sync past data that had been rolled
back. The worker now keeps the ordinary connector checkpoint unchanged while
pages are staged, recovers the staged source cursor after a restart, and
publishes that cursor only after finalization succeeds. Empty terminal imports
remain without a fabricated native cursor. The worker regression and identity
focused suites passed, followed by the serialized workspace gate.

The protocol audit also checked the live LDAP/AD/UCS wire paths against their
authoritative contracts: AD DirSync response `MoreResults` remains the
specified INTEGER form, and UCS confirmation uses the documented
`/messages/{sequence}/status` endpoint. No vendor endpoint was activated.

## Entry 2026-07-30 — current-tree verification after rollback fix

The current tree passed formatting, warnings-denied workspace Clippy, the
complete non-ignored workspace and doc-test suite, protobuf tests, G13
parallel/recovery suites (14/14 and 6/6), G14 local compatibility/directory
suites (11/11 and 10/10), all required Dioxus console web/desktop/iOS/Android
checks, shared UI host/iOS/Android checks, and the Android bridge. Folio lint
and TypeScript passed with zero errors; the four existing n8n lint warnings
remain. `git diff --check` passed, and port 3004 remained healthy (`/` 200,
protected routes 307). No production Next.js build ran.

The exact ignored workspace sweep remains an 81-target prerequisite queue.
Every failure was an explicit missing disposable database, backup/MinIO,
credentialed Generic LDAP/RFC 4533, AD DS/DirSync, UCS Provisioning Service,
approved protobuf descriptor, physical PITR/device/scale, or production
approval input; no application assertion failure was observed. No customer
directory activation, command-owner change, Folio write retirement, or
production cutover occurred; G0–G14 remain active and are not claimed 100%.

## Entry 2026-07-30 — TLS backup round-trip acceptance

PASS: ran `real_postgres_minio_backup_restore_and_cleanup_are_authoritative`
1/1 against a real ephemeral MinIO server configured for HTTPS with a
generated test CA and real PostgreSQL databases. The adapter performed
namespace setup, PostgreSQL full dump, TLS object upload/inventory, restore
into a disposable candidate, forced-RLS and tenant-marker verification, and
artifact cleanup. The host PostgreSQL 18.4 server required the installed
PostgreSQL 18.4 `pg_dump` and `pg_restore` binaries; using the default
PostgreSQL 14 client correctly failed closed on the version mismatch, then the
version-matched acceptance passed. All temporary databases and TLS/MinIO
state were removed and catalog residue was checked.

QUEUED: local adapter evidence does not replace approved production CA and
credentials, physical PITR/WAL restore evidence, or production authorization.
Generic LDAP/RFC 4533, on-premises AD DS/DirSync, UCS Provisioning Service,
approved protobuf provenance, physical device/scale, and production approval
remain queued.

## Entry 2026-07-28 — G10 Marketing content replacement-read list

PASS: the Marketing content replacement read model now has a bounded
holding-scoped metadata list for `asset`, `template`, `form`, and `domain`.
The SQLx adapter selects no aggregate state, object bytes, actors, tokens, or
credentials; it validates RLS scope, safe labels/status, positive source
version/event identity, checkpoint continuity, receipt continuity, page-size
bounds, UUID cursors, and ambiguous unfiltered pagination.

PASS: the typed RPC requires authenticated `marketing.read`, explicit
holding/company/branch scope, customer-scope containment, tenant read routing,
row-scope revalidation, bounded cursors, and append-only audit publication.
The console renders read-only tables for content, templates, forms, and
domains and rejects unsupported types, malformed IDs, unsafe text, stale
source metadata, and non-read-only rows. Evidence:
`audits/G10-MKT-CONTENT-READ-2026-07-28/README.md`.

PASS: protobuf/server/infrastructure/console checks, console wire tests
113/113, and strict focused Clippy. The dedicated live SQLx acceptance is
wired but queued because `ERPX_TEST_TENANT_WRITE_DATABASE_URL` is unavailable.
Rollback is additive transport/UI/adapter removal; no writes, migrations,
activation, owner switch, Folio retirement, or cutover occurred.

OPEN: G10 and G0–G14 still require broader workflow, external directory and
provider certification, device/scale/PITR evidence, approved descriptor
comparison, and explicit production approval.

## Entry 2026-07-28 — G8 Finance close-review bounded list

PASS: the tenant-read Finance close adapter now provides checkpoint-validated,
holding/company-scoped, keyset-paginated lifecycle/control/statement scalar
rows. `ListFinanceCloseReviews` enforces `finance.read`, explicit scope,
customer scope, read routing, opaque cursor bounds, and audit publication; the
Finance console renders the bounded read-only table and safe continuation
states. Focused infrastructure tests passed 5/5, console close tests 3/3,
warnings-denied package Clippy passed, and format/proto/server checks passed.

The ignored two-database acceptance was extended for list rows, freshness,
source-version, sibling-company isolation, and rollback, but remains queued
behind the supplied tenant write/read databases. This is a slice completion,
not full G8 closure; provider credentials/SDKs, broader Finance workflows,
external certification, and production approval remain open. No activation,
owner switch, Folio write retirement, or cutover occurred.

## Entry 2026-07-28 — authenticated G9 Sales lifecycle transition

Added `SalesService.TransitionSalesAggregate`. The command validates explicit
scope, `sales.write`, positive expected source version, bounded transition
names, and command-context version binding. `SqlxSalesStore::transition`
reads only authoritative write state, applies the domain transition, and
delegates the updated aggregate to the atomic source/snapshot/outbox/
idempotency writer. The Dioxus Sales surface exposes the action and refreshes
the typed read model only after a successful receipt; stale versions and
invalid transition names return safe errors.

PASS: infrastructure/server/console focused checks, real ignored Sales SQLx
acceptance on fresh disposable write/read PostgreSQL databases, and clear,
replay, reconciliation, and scope assertions. The Sales packet remains
bounded; full Lead-to-Cash, downstream Operations/accounting, vendor/device,
and production evidence remain open.

## Entry 2026-07-28 — authenticated G9 SCM procurement lifecycle transition

Added `ScmService.TransitionScmAggregate` and a typed Procurement/Inventory
console action. The command enforces `scm.write`, explicit scope, source
version binding, bounded transitions, source/read routing, and audit receipt.
`SqlxScmStore::transition` reads authoritative write state, applies the
relationship-safe domain lifecycle method, and commits through the existing
atomic source/snapshot/outbox/idempotency writer. Direct Goods Receipt posting
and stock-balance mutations stay relationship-aware and are not exposed as a
status shortcut.

PASS: infrastructure/server/console focused checks, domain regression tests,
and a fresh disposable PostgreSQL acceptance covering Purchase Request
submit/approve, projection, reconciliation, clear-and-replay rebuild, and
source/read separation. Broader Procure-to-Pay, stock/accounting/Operations,
worker, vendor/device, and production evidence remain open.

## Coordinator continuation — final support claims and G13 retention — 2026-07-27

- Added control migration `0060_support_sla_claim_identity.sql` and wired the
  SQLx support repository to persist claimant UUID, claim token, claim times,
  and a bounded lease. The worker acknowledges only the exact live claim
  returned by the repository after provider acceptance; stale/reclaimed claims
  affect zero rows and remain retryable. Focused support unit/integration
  tests and warnings-denied Clippy passed.
- Wired G13 target-worker retention handling to durable control state: expiry
  requires `folio_read_only`, a passed retention horizon, no pending or failed
  work, matched reconciliation, and successful rollback evidence. The importer
  validates the source fence and rollback rows before removing target and
  read-model rows, preserves rollback evidence, and appends an idempotent
  retention event.

These are local executable/disposable-database boundaries. Credentialed
Generic LDAP, AD DS/DirSync, and UCS certification, measured million-user and
five-minute external runs, physical mobile execution, approved descriptor
comparison, live Folio migration/retention/post-cutover write-rejection, and
explicit production approval remain queued. Folio remains the operational
write owner.

## Coordinator final verification addendum — current tree — 2026-07-27

- Integrated the final G4/G10/G14 worker lanes and the server’s exhaustive
  support-policy error mapping. Support diagnostics persistence is atomic,
  redacted, scoped, freshness-bound, idempotent, and reconciliable; support
  expiry is ordered and retry-safe; Marketing incremental projection validates
  cursor/page continuation and drains all authoritative source pages.
- PASS: `cargo fmt --all --check`; workspace Clippy with warnings denied;
  non-ignored workspace tests; protobuf compilation plus G13/G14 focused
  acceptance; Dioxus web/desktop/mobile/iOS/Android checks; `bun run lint`
  (four pre-existing n8n warnings, zero errors); `bunx tsc --noEmit`;
  `git diff --check`; and Folio `/operations` on port 3004 (HTTP 307).
- The full ignored suite ran serially with fresh disposable control, tenant,
  commercial, scheduler, and support databases. All local database targets
  passed except the intentional G14 external-readiness failure. The Stripe
  semantic worker and backup external-credentials acceptance now isolate their
  own disposable databases and pass in the aggregate run. Disposable
  databases were removed after each run.
- G14 external readiness remains explicitly queued with the exact missing
  Generic LDAP/RFC 4533, AD DS/DirSync, and UCS Provisioning Service endpoint,
  TLS, credential, CA/server, feed, and disposable-database references. The
  approved protobuf descriptor baseline is also absent; compilation and
  structural compatibility markers do not substitute for that comparison.

This is not a G0–G14 closure claim. Credentialed directory certification,
measured million-user/five-minute external evidence, physical mobile
execution, approved descriptor comparison, live Folio migration/rollback/
retention/post-cutover write rejection, and explicit production approval
remain queued. No directory activation, command-owner switch, Folio write
retirement, or production cutover occurred.

## Coordinator final verification addendum — descriptor and ignored-suite evidence — 2026-07-27

- Protobuf compatibility is now an executable descriptor-set comparison. The
  build emits the current descriptor set; with an approved reference, the
  migrator test compares message fields, enum values, and service RPC
  contracts, rejecting removals and incompatible changes while allowing
  compatible additive changes. The approved reference is absent, so the
  comparison reports a queued blocker and does not claim approval.
- Corrected fresh serial ignored workspace run: all local database-backed
  targets passed, including provisioning, Finance candidate activation,
  Operations projection, support persistence/attachments, Stripe semantic
  isolation, and backup-blocker isolation. The sole failed target was the
  intentional credentialless `erpx-migrator --test g14_external_readiness`
  preflight. The fixture harness used the required literal
  `postgres://localhost/{database}` template for dynamically named databases;
  all unique disposable databases were removed afterward.
- Focused G13/G14, Generic LDAP, AD DS/DirSync, UCS, protobuf, Dioxus, Folio
  lint/typecheck, and port-3004 checks remain green. External vendor
  certification, measured million-user/five-minute revocation, physical
  mobile execution, live Folio migration/rollback/retention/post-cutover
  evidence, approved descriptor input, and explicit production approval
  remain queued. No activation, command-owner switch, Folio write retirement,
  or cutover occurred.

## Coordinator continuation — G2 recovery retry repair — 2026-07-27

- Closed a local recovery crash-window gap in the authoritative application
  service. Recovery rehydrates and validates the deterministic request-ID
  assignment on retry, so a committed role command cannot be rejected merely
  because its separate review receipt was interrupted; a mismatched ID still
  fails closed and no second command is emitted.
- `cargo fmt --all --check` and `cargo test -p erpx-application --offline`:
  PASS (19 tests, including the new recovery retry regression).
- This advances G2 local evidence but does not close provider delivery,
  universal mutation adoption, external certification, or any production
  cutover gate.

## Coordinator continuation — bounded G9–G13 hardening — 2026-07-27

- G9 SCM recosting now rejects missing authoritative average cost without
  mutating stock state.
- G10 Marketing delivery authorization now rejects invalid consent records.
- G11 POS completion/refund requires full payment; HR/payroll rejected review/
  post and version-overflow paths remain state-atomic.
- G13 migration evidence writes now require the locked authoritative run
  version, rejecting stale evidence writers.
- Evidence and rollback boundaries: `audits/COORDINATOR-CONTINUATION-2026-07-27/README.md`.

Focused current-tree tests are green. Full serialized workspace and ignored
database verification remains the next coordinator step; external directory,
mobile, descriptor, live Folio, and production-approval evidence remains
queued. No activation, command-owner switch, write retirement, or cutover
occurred.

## Entry 2026-07-28 — G11 POS sale command boundary

PASS: the POS console now submits bounded authenticated SavePosSale requests
through the existing authoritative service. Company/branch command scope,
immutable command ID and idempotency replacement, optional expected-version
handling, typed receipts, categorical errors, and read refresh are covered.
Server-owned POS sale invariants, payment/line totals, pos.write
authorization, SQLx persistence, audit/outbox, worker/read projection,
idempotency, and rollback remain authoritative. Focused console coverage is
96/96. Device/payment execution, accounting handoff, full POS workflow, live
SQLx/provider evidence, and production approval remain queued; no activation,
owner switch, Folio write retirement, or cutover occurred.

## Entry 2026-07-28 — G11 Legal command boundaries

PASS: the Legal console now submits bounded authenticated SaveLegalContract,
SaveLegalDocument, and RecordLegalAccess requests. Contract command identity,
document scope, access actor binding, expected versions, typed
UUID/version/idempotency/event/replay/correlation receipts, and projection
refresh are covered. Server-owned Legal validation, legal.write
authorization, SQLx persistence, audit/outbox, retention/projection,
immutable access history, and rollback remain authoritative. Focused console
coverage remains 96/96. Object-store/provider certification, broader
document/retention workflows, live SQLx evidence, and production approval
remain queued; no activation, owner switch, Folio write retirement, or
cutover occurred.

## Coordinator final verification — 2026-07-27

- PASS: `cargo fmt --all --check`.
- PASS: workspace Clippy with `-D warnings`.
- PASS: complete non-ignored workspace tests and doc tests.
- PASS: Dioxus web, desktop, host-mobile, iOS, and Android checks.
- PASS: protobuf compilation plus focused G13/G14 acceptance. The approved
  descriptor comparison is implemented but queued because no approved
  baseline descriptor is configured.
- PASS: fresh serial ignored workspace run for every local database-backed
  target. Finance handoff passed after its acceptance fixture used one shared
  authoritative party. The sole failure was the intentional credentialless
  G14 external-readiness preflight; all disposable databases were removed.
- PASS: Folio `bun run lint` (zero errors, four pre-existing n8n warnings),
  `bunx tsc --noEmit`, read-only G14 structural audit, `git diff --check`, and
  port-3004 `/operations` HTTP 307 probe.

Queued blockers: credentialed Generic LDAP/RFC 4533, AD DS/DirSync, and UCS
Provisioning Service acceptance with measured one-million-user imports and
five-minute revocation; physical mobile execution; approved protobuf baseline;
live Folio migration/rollback/retention/post-cutover evidence; and explicit
production approval. No directory activation, command-owner switch, Folio
write retirement, or cutover occurred.

## Entry 2026-07-28 — current-tree verification after Finance banking packet

PASS: focused console coverage is 94/94; all seven current Dioxus web,
desktop, host-mobile, native iOS, native Android, shared UI iOS, and shared
UI Android checks passed. Serialized current-tree Cargo verification passed
format, warnings-denied workspace Clippy, and all non-ignored workspace tests.
Protobuf tests, G13 compatibility/registration (9/9), G14 directory
acceptance (8/8), Folio lint/typecheck, git diff --check, and port-3004
root/protected health (200/307) also passed. The required ignored
workspace run exits 101 at exactly 66 queued missing-resource/certification
targets, including disposable PostgreSQL, MinIO/PITR, credentialed directory/
provider environments, and the approved protobuf descriptor baseline. Bun
lint has zero errors and four pre-existing n8n warnings. No temporary
databases, activation, owner switch, Folio write retirement, or cutover
occurred.

## Entry 2026-07-28 — current-tree verification after POS and Legal packets

PASS: focused console coverage is 96/96. Current serialized format,
warnings-denied workspace Clippy, and all non-ignored workspace tests passed;
all Dioxus web/desktop/host-mobile/iOS/Android and shared UI native checks,
protobuf tests, G13 compatibility/registration (9/9), G14 directory
acceptance (8/8), Folio lint/typecheck, diff checks, and port-3004 health
also passed. The required ignored aggregate exits 101 at exactly 66 queued
missing-resource/certification targets for live SQLx, provider, MinIO/PITR,
directory, and protobuf-descriptor references. No temporary databases,
activation, owner switch, Folio write retirement, or cutover occurred.

## Entry 2026-07-28 — G8 Finance banking and reconciliation command boundary

PASS: the Finance console now submits bounded authenticated
RegisterBankAccount, ImportBankStatement, SaveBankReconciliation,
SaveBankMatchProposal, and SaveBankMatchDecision requests. Import,
reconciliation, proposal, and decision payloads receive the matching domain
command context at the client boundary; reconciliation carries a positive
expected version. Typed receipts validate UUIDs, positive aggregate versions,
transaction counts, proposal/decision identifiers, idempotency keys, replay
state, and correlation IDs. Focused console coverage passes 94/94, including
holding scope, context equality, version binding, malformed payload, and
receipt checks. Server-owned banking validation, authorization, SQLx writes,
audit/outbox, worker/read projection, deterministic matching, and rollback
remain authoritative. Broader live provider/certification and Finance
workflow evidence remains queued; no activation, owner switch, Folio write
retirement, or cutover occurred.

## Entry 2026-07-28 — final post-Asset/Finance ignored sweep

The exact mandated post-Asset/Finance command `cargo test --workspace --offline --no-fail-fast
--quiet -- --ignored --nocapture --test-threads=1` exited 101 with exactly 71
targets. Failures were the existing explicit guards for missing disposable
SQLx/MinIO/PITR resources, Generic LDAP/RFC 4533, AD DS/DirSync, UCS
Provisioning Service, operator-revocation, approved descriptor, physical
mobile/scale, and production migration/cutover approval. No new application
assertion failure appeared. G0–G14 remain open; no production state,
directory activation, command-owner switch, Folio write retirement, or
cutover occurred.

## Entry 2026-07-28 — G9 SCM aggregate command boundary

PASS: Procurement and Inventory now submit bounded authenticated
`SaveScmAggregate` requests through the existing SCM service. The console
validates aggregate JSON and optional expected versions, derives the explicit
holding command context, and renders typed aggregate/version/replay/
correlation receipts. Maintenance remains read-only. Server-side SCM domain
validation, scm.write RBAC, tenant SQLx source/read routing, audit/outbox,
worker projection, optimistic concurrency, idempotency, and rollback remain
authoritative. Focused console coverage passes 93/93, including malformed
payload rejection and command-context equality. Broader G9 workflows,
reconciliation/device acceptance, live SQLx/provider evidence, and production
approval remain queued; no activation or cutover occurred.

## Entry 2026-07-28 — current-tree verification after SCM packet

PASS: latest serialized `cargo fmt --all --check`, workspace Clippy with
warnings denied, complete non-ignored workspace tests, console web/desktop/
host-mobile/iOS/Android checks, shared UI native iOS/Android checks, and the
known Folio/protobuf/G13/G14 verification wave. The aggregate ignored run
continues to identify exactly 66 missing-resource/certification targets for
disposable PostgreSQL URLs, MinIO/PITR, credentialed directory/vendor
environments, and the approved protobuf descriptor baseline. Folio lint/
typecheck and port-3004 root/protected health remain green (`200`/`307`).
Queued results are not completion evidence; no temporary databases,
activation, owner switch, Folio write retirement, or cutover occurred.

## Entry 2026-07-28 — G9 Sales aggregate command boundary

PASS: the Dioxus Sales context submits bounded authenticated
`SaveSalesAggregate` requests in addition to lifecycle transitions. The
console validates aggregate JSON and optional expected versions, derives the
explicit holding command context, and renders typed aggregate/version/replay/
correlation receipts. Server-side Sales domain validation, sales.write RBAC,
tenant SQLx source/read routing, audit/outbox, worker projection, optimistic
concurrency, idempotency, and rollback remain authoritative. Focused console
coverage passes 92/92, including malformed-payload rejection and command
context equality. SCM save, broader G9 workflows/reconciliation/device
acceptance, live SQLx/provider evidence, and production approval remain
queued; no activation or cutover occurred.

## Entry 2026-07-28 — current-tree verification after Sales packet

PASS: latest serialized `cargo fmt --all --check`, workspace Clippy with
warnings denied, complete non-ignored workspace tests, console web/desktop/
host-mobile/iOS/Android checks, shared UI native iOS/Android checks, Folio
lint/typecheck, and port-3004 root/protected health (`200`/`307`). The exact
aggregate ignored workspace run remains 101 at 66 explicit missing-resource/
certification targets for disposable PostgreSQL URLs, MinIO/PITR, credentialed
directory/vendor environments, and the approved protobuf descriptor baseline.
Protobuf/G13/G14 structural checks remain green; queued failures are not
completion evidence. No temporary databases, activation, owner switch, Folio
write retirement, or cutover occurred.

## Entry 2026-07-28 — G8 Finance subledger command boundary

PASS: the Dioxus Finance surface submits bounded authenticated `SaveSubledger`
requests for supported AP/AR aggregate types. The command boundary validates
the JSON envelope, derives the explicit holding scope, generates matching
command/audit/idempotency context, and renders typed aggregate ID/version/
replay/correlation receipts. Existing server domain validation, scope/RBAC,
tenant SQLx persistence, audit/outbox, worker/read projection, idempotency,
and rollback behavior remain authoritative. Console coverage passes 91/91,
including malformed-payload rejection and holding-context binding. Allocation
batches, bank reconciliation/matching, broader Finance workflows, live SQLx/
provider evidence, and production approval remain queued; no activation or
cutover occurred.

## Entry 2026-07-28 — current-tree verification after Finance subledger packet

PASS: `cargo fmt --all --check`; workspace Clippy with `-D warnings`;
non-ignored workspace tests; all five console Dioxus checks and shared UI
native iOS/Android checks; protobuf tests; G13 compatibility/registration
(9/9); G14 directory acceptance (8/8); Folio lint/typecheck; and port-3004
root/protected health (`200`/`307`). The aggregate ignored run exits 101 at
exactly 66 explicit missing-resource/certification targets for disposable
PostgreSQL URLs, MinIO/PITR, credentialed directory/vendor environments, and
the approved protobuf descriptor baseline. These are queued blockers, not
completion evidence. No temporary databases, activation, owner switch, Folio
write retirement, or cutover occurred.

## Coordinator continuation — current-tree directory/provider hardening — 2026-07-27

- Directory auth evidence is boundary-timed, high-risk MFA remains required,
  break-glass is explicitly local-only, and MFA references are debug-redacted.
- Native LDAP, AD DirSync, RFC 4533, and UCS feed cursors reject empty,
  malformed, regressing, and repeating continuations. Identity tests and
  warnings-denied identity Clippy pass.
- Directory RBAC/revocation preserves immutable links and append-only facts;
  contradictory actions, same-revision conflicts, and identifier collisions
  fail closed.
- Marketing provider result recording uses authoritative database time and
  renews a still-live claim before persistence. Focused worker/infrastructure
  tests and the fresh disposable provider-flow acceptance pass.
- The credentialless external-readiness preflight remains the only expected
  local ignored-suite failure. Vendor certification, measured one-million-user
  imports/five-minute revocation, physical mobile execution, approved protobuf
  descriptor input, live Folio migration/rollback/retention/post-cutover
  evidence, and explicit production approval remain queued. No activation,
command-owner switch, Folio write retirement, or cutover occurred.

## Coordinator follow-up — platform/product boundaries and SQLx evidence — 2026-07-27

- G3–G6 holding bindings now authorize through a stable holding-scoped key;
  per-holding connectors cannot authorize as platform-wide.
- G9–G11 payroll lines are validated for identity, duplicates, shape, totals,
  and lifecycle invariants; SCM transfers validate exact scoped references and
  both balances before any mutation.
- Directory SQLx tests that require PostgreSQL are explicitly ignored without
  `ERPX_G14_LOCAL_DIRECTORY_DATABASE_URL`; the ordinary test logs a queued
  blocker instead of presenting a persistence claim. The ignored verification
  must set a disposable URL.
- Current scheduler composition and Marketing callback ingress are implemented;
  historical QA-062 absence claims are superseded, but ignored SQLx live
  acceptance and all external certification remain open.
- Folio TypeScript excludes generated `erpx/target` output; `bunx tsc
  --noEmit` passes after Dioxus checks.

No directory activation, production command-owner switch, Folio write
retirement, or cutover occurred. Vendor credentials, measured million-user and
five-minute evidence, physical mobile execution, approved protobuf baseline,
live Folio migration/rollback/retention/post-cutover proof, and production
approval remain queued.

## Coordinator final verification — current tree — 2026-07-27

- PASS: format, workspace warnings-denied Clippy, clean workspace tests/doc
  tests, Dioxus web/desktop/host-mobile/iOS/Android checks, G13/protobuf
  surfaces, G13 parallel acceptance, G14 directory acceptance, Folio lint and
  TypeScript, structural audit, and port-3004 probe.
- PASS: fresh serial ignored database suite for directory SQLx, identity,
  platform, Finance, HR, Legal, SCM, POS, Marketing, Operations, provisioning,
  support, and G13 paths. Disposable databases were removed after the run.
- EXPECTED QUEUED FAILURE: the ignored G14 external-certification preflight
  fails closed without credentialed Generic LDAP/RFC 4533, AD DS/DirSync, and
  UCS Provisioning Service references. It makes no vendor claim; focused
  readiness checks otherwise pass.
- QUEUED: approved protobuf descriptor baseline; measured one-million-user and
  five-minute external evidence; physical mobile execution; live Folio
  migration/rollback/retention/post-cutover proof; explicit production
  approval. No directory activation, command-owner switch, Folio write
  retirement, or cutover occurred.

## Coordinator verification — current G3/G4/G6 tree — 2026-07-28

- PASS: current-tree `cargo fmt --all --check`, workspace Clippy with
  warnings denied, serialized workspace/doc tests, and Dioxus web, desktop,
  host-mobile, iOS, and Android checks.
- PASS: focused current counts are `erpx-ui` 81/81, `erpx-console` 75/75,
  server 84/84 non-ignored, and infrastructure 219/219 unit tests. Folio
  lint and TypeScript checks pass; lint has only four pre-existing n8n
  warnings. Structural/read-only, diff, branch, and port-3004 checks pass.
- PASS: a fresh current-tree eight-database ignored PostgreSQL run passed all
  local targets and cleaned its disposable databases. The sole nonzero target
  is the intentional credentialless G14 external-readiness preflight for
  Generic LDAP/RFC4533, AD DS/DirSync, and UCS; no certification is claimed.
- QUEUED: approved descriptor, credentialed directory/vendor certification,
  MinIO/PITR, physical mobile execution, live Folio migration/cutover, and
  production approval. G0–G14 remain open; no activation, owner switch,
  Folio write retirement, or cutover occurred.

## Coordinator continuation — G13 worker lease-reclaim rehearsal — 2026-07-28

- PASS: the ignored `g13_migration_persistence` acceptance ran against fresh
  control write/read PostgreSQL databases and verified idempotent work enqueue,
  first-worker claim, simulated crash lease expiry, restarted-worker reclaim
  at attempt two, stale-claim fencing, bounded retry exhaustion, durable
  dead-letter state, and projected pending/failed read-model counts.
- PASS: the disposable databases were removed by the run trap.
- QUEUED: live process kill/restart telemetry, credentialed Folio migration,
  approved protobuf baseline, retention/cutover approval, and Folio write
  retirement remain open.

## Coordinator correction — AD DS DirSync response BER — 2026-07-27

- Implemented: `apps/worker/src/directory_live.rs` now requires the AD DS
  DirSync response `moreData` field to be BER BOOLEAN and retains strict
  reserved-field/cookie/trailing-byte validation.
- Verified: focused `erpx-worker` directory-live tests passed (8/8); the worker
  Clippy rerun and the full workspace Clippy pass with warnings denied also
  passed after the disk cleanup.
- Still queued: credentialed AD DS/DirSync acceptance, measured one-million-user
  import and five-minute revocation evidence, and all explicit production
  approval/cutover gates.

## Coordinator continuation — parallel local hardening — 2026-07-27

- PASS: loopback-only break-glass origin validation and revalidation with
  focused identity authentication/volume tests.
- PASS: directory disabled/deleted full-import retention, immutable-envelope
  rejection, and exact native LDAP/RFC4533/AD continuation-control checks.
- PASS: G7 orphan-outbox reconciliation regression and G8 authoritative
  accounting-period/journal atomicity regression against disposable SQLx.
- PASS: G12 fail-closed directory and G13 reconciliation/rollback evidence
  rendering; console web tests (59/59) and Dioxus checks remain green.
- PASS: G13 migration persistence against fresh PostgreSQL, including expired
  lease reclaim, stale-worker completion rejection, retry exhaustion, and
  projected dead-letter accounting.
- PASS: G13/G14 runtime reference and endpoint/path validation focused suites.
- QUEUED: credentialed Generic LDAP/RFC4533, AD DS/DirSync, and UCS acceptance;
  measured external one-million-user/five-minute evidence; approved protobuf
  descriptor comparison; physical mobile execution; live Folio migration,
  rollback, retention, and post-cutover proof; and explicit production
  approval. No activation, owner switch, Folio write retirement, or cutover.
- PASS: connector-scope reload/locking for directory RBAC, source-fenced
  Finance banking rebuilds, exact HR outbox continuity, holding-bound platform
  read-model projection, role-review notification-state validation, and
  Operations orphan-outbox detection. G6 MinIO round-trip remains queued for
missing local endpoint/credentials.

## Coordinator final verification record — 2026-07-27

Verification passed on the current branch: formatting, warnings-denied
workspace Clippy, and the non-ignored workspace tests/doc-tests. The fresh
ignored SQLx suite passed all local database targets; its sole nonzero target
was the intentionally ignored credentialless G14 vendor preflight, which
recorded the exact Generic LDAP/AD DS/UCS blocker references and made no vendor
acceptance claim. Eighteen explicitly disposable databases created for the run
were removed.

G13 deployment 4/4, G13 parallel 9/9, G14 directory 8/8, G14 readiness 7/7
plus one ignored preflight, protobuf compilation, all Dioxus web/desktop/mobile
and iOS/Android target checks, Folio lint/typecheck, the structural probe, and
the port-3004 route probe passed. The approved descriptor baseline, credentialed
directory/vendor acceptance, measured million-user/five-minute evidence,
physical mobile execution, live Folio migration/cutover/rollback/retention,
MinIO round-trip, and explicit production approval remain queued. No
activation, owner switch, Folio write retirement, or cutover occurred.

## 2026-07-27 — coordinator local G2 runtime slice

- Implemented scoped `role_review_snapshot` worker evaluation from authoritative
  control-plane role assignments into persisted review snapshots and expiry
  notification rows.
- Registered `role_review_notifications` in the worker executable with an
  explicit scope and bounded horizon/lease configuration.
- Added the SQLx control-outbox publisher for
  `role_expiry_notification.requested`, keyed by the stable notification key;
  the worker reconciles only after the append succeeds.
- Evidence: `cargo check -p erpx-worker -p erpx-infrastructure --offline` PASS;
  focused role-review worker tests 7/7 PASS; fresh disposable PostgreSQL
  role-review ignored tests 2/2 PASS; no provider delivery or production
  activation was performed.

## 2026-07-27 — coordinator native UCS continuity slice

- Added exact-continuity validation for UCS Provisioning Service sequences,
  retaining the explicit already-committed duplicate path and failing closed on
  gaps/regression.
- Evidence: focused `directory_live` worker tests 12/12 PASS. External UCS
  certification remains queued because no credentialed environment is
  available.

## Coordinator final verification — corrected current tree — 2026-07-27

- PASS: `cargo fmt --all --check`; workspace Clippy with warnings denied; and
  serialized `cargo test --workspace --offline --no-fail-fast`, including
  workspace doc-tests.
- PASS: the fresh serial ignored SQLx matrix for all local database targets.
  The explicit local directory SQLx rerun passed 2/2 after supplying its
  disposable URL. The only expected nonzero ignored target is the intentional
  G14 external-certification preflight, which fails closed for missing
  credentialed Generic LDAP/RFC4533, AD DS/DirSync, and UCS references.
  Eighteen databases created for this verification were removed afterward.
- PASS: `cargo check -p erpx-proto --offline`; G13 parallel 9/9; G13 recovery
  4/4; G14 directory 8/8; G14 readiness 7/7 plus one ignored preflight; and
  all five Dioxus web/desktop/mobile/iOS/Android target checks.
- PASS: Folio `bun run lint` (zero errors, four pre-existing n8n warnings),
  `bunx tsc --noEmit`, the read-only structural audit, `git diff --check`,
  and the port-3004 `/operations` probe (HTTP 307). No production Next.js
  build was run.
- QUEUED: approved protobuf descriptor baseline, credentialed vendor
  certification, measured one-million-user/five-minute external evidence,
  physical mobile execution, live Folio migration/rollback/retention/
  post-cutover evidence, MinIO round-trip, and explicit production approval.
  No activation, command-owner switch, Folio write retirement, or cutover
  occurred.

## Coordinator continuation — G1 durable worker failure context — 2026-07-27

- Added control migration `0061_worker_error_context.sql` and an additive
  `last_error_context` JSONB field on control outbox rows.
- Worker retry/dead-letter persistence now carries exact event/aggregate
  identity, tenant scope, actor, correlation, causation, idempotency, worker,
  retryability, and a sanitized display message. Context/event mismatch is
  rejected before persistence; secret-bearing metadata is omitted.
- PASS: infrastructure worker context tests 4/4, outbox tests 3/3, worker
  and migration checks, and fresh disposable SQLx context acceptance 1/1.
- Residual: dead-letter replay still requires an authenticated, audited
  operator command boundary; no activation, command-owner switch, Folio write
  retirement, or production cutover occurred.

## Coordinator continuation — current-tree G1 context verification — 2026-07-27

- PASS: current-tree serialized eight-database ignored workspace matrix for
  every local database target, including the worker-context SQLx acceptance;
  the only nonzero target was the intentional credentialless G14 vendor
  preflight, which recorded exact LDAP/AD DS/UCS references and made no
  certification claim.
- PASS: dedicated fresh identity SQLx checks — directory RBAC scope authority
  1/1 and local directory persistence 2/2. All disposable databases were
  removed after verification.
- QUEUED: authenticated/audited dead-letter replay, credentialed vendor
  certification, measured million-user/five-minute external evidence,
  descriptor approval, physical mobile execution, live Folio migration /
  rollback / retention / post-cutover evidence, MinIO round-trip, and explicit
  production approval. No activation, owner switch, Folio write retirement,
  or cutover occurred.

## Coordinator verification — replay-complete current tree — 2026-07-27

- PASS: `cargo fmt --all --check`; warnings-denied workspace Clippy; and the
  serialized workspace test/doc-test suite.
- PASS: corrected ten-database ignored SQLx matrix with dedicated directory
  RBAC/local-directory URLs. Local targets all pass, including identity SQLx,
  worker context, and all three `sqlx_control_plane` tests. The sole expected
  nonzero is the intentional G14 external-certification preflight, which
  fails closed with the exact missing Generic LDAP/RFC4533, AD DS/DirSync,
  and UCS references. Every disposable database was removed.
- PASS: G13 compatibility 9/9, G13 deployment recovery 4/4, G14 directory
  acceptance 8/8, Dioxus web/desktop/mobile/iOS/Android checks, Folio lint,
  Folio TypeScript check, `git diff --check`, and the port-3004 `/operations`
  HTTP 307 probe. No production Next.js build was run.
- QUEUED: credentialed vendor certification, measured million-user/five-
  minute external evidence, approved descriptor baseline, physical mobile
  execution, live Folio migration/rollback/retention/post-cutover evidence,
  MinIO round-trip, and explicit production approval. No activation, owner
  switch, Folio write retirement, or cutover occurred.

## Coordinator continuation — authenticated dead-letter replay — 2026-07-27

- Added the versioned `ReplayDeadLetterOutbox` Platform Operations command.
  Its server boundary authenticates the session, binds actor attribution,
  requires platform scope plus `platform.outbox.replay`, and verifies a fresh
  one-time TOTP step-up. The MFA code is transport-only.
- Added atomic SQLx replay persistence with command idempotency, bounded
  dead-letter eligibility, claim/error-context clearing, and one append-only
  audit row. Audit payloads contain safe event/aggregate identity only; raw
  outbox payloads, worker failures, and MFA values are excluded. Duplicate
  requests return the stored receipt without a second audit row.
- Added the Dioxus operator panel with event ID, RFC3339 availability, and
  fresh MFA inputs; the MFA field is cleared after submission and the client
  renders only safe response metadata.
- PASS: fresh disposable PostgreSQL SQLx acceptance
  `audited_dead_letter_replay_is_atomic_idempotent_and_redacted` (1/1);
  server and console protobuf consumers compile.
- QUEUED: credentialed Generic LDAP/RFC4533, AD DS/DirSync, and UCS
  certification; measured million-user/five-minute evidence; approved
  protobuf baseline; physical mobile execution; live Folio migration/
  rollback/retention/post-cutover evidence; MinIO round-trip; and explicit
  production approval. No activation, owner switch, Folio write retirement,
  or cutover occurred.

## Coordinator continuation — G7 authenticated Operations read surface — 2026-07-27

- Implemented the Dioxus `/operations` adapter over the existing
  `OperationsService::GetOperationsSummary` and
  `OperationsService::ListOperationsExceptions` authoritative read boundary.
- The route requires an explicit opaque tenant reference and UUID holding
  scope. It sends authenticated, audited tenant-scoped query contexts and
  never infers a holding or falls back to cached/local rows.
- Added bounded 25-row exception reads, projection/scope/correlation/cursor
  validation, safe categorical error mapping, and display-only summary,
  source-count, and exception models. The existing
  `TransitionOperationsException` command is now wired behind a holding-scoped
  command context, allow-listed action validation, a positive expected version,
  and a fresh idempotency key; stale versions and rejected commands remain
  server-owned failures.
- PASS: `cargo check -p erpx-console --offline`; focused console tests 67/67,
  including scope validation, malformed-projection rejection, pagination
  consistency, holding-scoped command context, and redaction. Evidence packet:
  `audits/G7-UI-OPERATIONS-READ/README.md`.
- QUEUED: broader Operations command journeys/source-producer acceptance,
  credentialed directory/vendor certification, measured million-user/five-
  minute evidence, approved protobuf baseline, physical mobile execution,
  live Folio migration/rollback/retention/post-cutover evidence, MinIO
  round-trip, and explicit production approval. No activation, owner switch,
  Folio write retirement, or cutover occurred.

## Coordinator continuation — G8 authenticated Finance journal read — 2026-07-27

- Implemented the Dioxus `/finance` route over the existing
  `FinanceService::ListPostedJournals` authoritative read boundary.
- The route requires explicit tenant, holding route reference, and UUID
  holding scope; it sends an authenticated/audited Finance query context and
  never infers a holding or uses a platform-scope read.
- Added bounded 25-row journal metadata display. UUID/reference/timestamp,
  correlation, page-size, and cursor checks fail closed. Raw `payload_json` is
  discarded before the display model; no credentials or journal payload is
  rendered. Finance posting/close/banking/AP/AR commands remain server-owned.
- PASS: `cargo check -p erpx-console --offline`; focused console tests 66/66,
  including Finance scope validation, cursor rejection, payload redaction,
  and safe error categories. Evidence packet:
  `audits/G8-UI-FINANCE-JOURNAL-READ/README.md`.
- QUEUED: broader Finance provider/close/AP/AR/banking workflows and product
  journeys, credentialed directory/vendor certification, measured
  million-user/five-minute evidence, approved protobuf baseline, physical
  mobile execution, live Folio migration/rollback/retention/post-cutover
  evidence, MinIO round-trip, and explicit production approval. No
  activation, owner switch, Folio write retirement, or cutover occurred.

## Coordinator continuation — G8 authenticated Finance close-review read — 2026-07-27

- Extended the Dioxus `/finance` route over the existing
  `FinanceService::GetFinanceCloseReview` authoritative read boundary. Close
  review requests require the existing tenant/holding scope plus explicit
  company and period UUIDs; the console sends an authenticated, audited
  holding-scoped query context.
- Added fail-closed wire validation for matching holding/company/period scope,
  read-only metadata, present/missing availability, freshness, review status,
  bounded account totals, statement balance, control counts, fact counts, and
  correlation identity. The UI renders statement/control/account metadata and
  missing/stale states while keeping close transitions and approvals
  server-owned.
- PASS: `cargo fmt --all --check`; focused console suite 68/68, including
  close-review scope/read-only/fact-boundary tests. Evidence packet:
  `audits/G8-UI-FINANCE-JOURNAL-READ/README.md`.
- QUEUED: Finance provider/close command execution, AP/AR/banking workflows,
  broader product journeys, credentialed directory/vendor certification,
  measured million-user/five-minute evidence, approved protobuf baseline,
  physical mobile execution, live Folio migration/rollback/retention/
  post-cutover evidence, MinIO round-trip, and explicit production approval.
No activation, owner switch, Folio write retirement, or cutover occurred.

## Coordinator verification — current Finance close-review and Sales tree — 2026-07-27

- PASS: `cargo fmt --all --check`; workspace Clippy with warnings denied; and
  `cargo test --workspace --offline --no-fail-fast` including all workspace
  tests and doc tests. The console suite is 68/68 after the Finance
  close-review and Sales read adapters.
- PASS: the full ignored workspace suite against fresh, physically distinct
  disposable control-write/read, tenant-write/read, admin, commercial,
  scheduler, support, directory-RBAC, and local-directory PostgreSQL
  databases. Every local ignored database target passed, including G13
  disposable acceptance, directory SQLx, identity, Finance close/banking,
  product contexts, Operations, support, provisioning/replacement, server,
  and worker paths. The only nonzero target was the deliberate
  `g14_external_readiness` preflight, which failed closed and recorded the
  missing Generic LDAP/RFC4533, AD DS/DirSync, and UCS references. The ten
  disposable databases were removed after the run.
- PASS: `cargo check -p erpx-proto --offline`; G13 parallel compatibility
  checks 9/9; G13 deployment/recovery checks 4/4; G14 directory acceptance
  8/8; G14 readiness 7/7 plus its intentional ignored preflight; and
  Dioxus web, desktop, host-mobile, `aarch64-apple-ios`, and
  `aarch64-linux-android` checks.
- PASS: Folio `bun run lint` (zero errors, four pre-existing n8n warnings),
  `bunx tsc --noEmit`, the read-only G14 structural probe, `git diff --check`,
  and the protected `/operations` HTTP 307 probe on port 3004. No production
  Next.js build was run.
- QUEUED: credentialed Generic LDAP/RFC 4533, AD DS/DirSync, and UCS vendor
  acceptance; measured one-million-user import and five-minute revocation;
  approved protobuf descriptor baseline; physical mobile execution; live
  Folio migration/rollback/retention/post-cutover evidence; MinIO round-trip;
  and explicit production approval. No activation, command-owner switch,
  Folio write retirement, or cutover occurred.

## Coordinator continuation — G9 authenticated Sales aggregate read — 2026-07-27

- Implemented the Dioxus `/sales` route over the existing
  `SalesService::GetSalesAggregate` authoritative read boundary. The route
  requires explicit tenant, holding route, holding UUID, aggregate type, and
  aggregate UUID inputs, with an authenticated/audited holding query context.
- Added response identity/presence/version/correlation validation and a
  one-megabyte payload bound. Aggregate JSON is discarded before display; the
  UI exposes only bounded identity/version metadata and explicit
  loading/empty/denied/offline/error states. Sales save, approval,
  fulfillment, and payment commands remain server-owned.
- PASS: focused console suite 68/68 and `cargo fmt --all --check`. Evidence
  packet: `audits/G9-UI-SALES-READ/README.md`.
- QUEUED: full Lead-to-Cash/Procure-to-Pay journeys, SCM breadth, workflow UI,
  live reconciliation and rollback evidence, credentialed directory/vendor
  certification, measured million-user/five-minute evidence, approved
  protobuf baseline, physical mobile execution, live Folio migration/
  rollback/retention/post-cutover evidence, MinIO round-trip, and explicit
  production approval. No activation, owner switch, Folio write retirement,
  or cutover occurred.

## Coordinator continuation — G9–G11 authenticated SCM and Asset/Maintenance reads — 2026-07-27

- Extended the Dioxus `/procurement`, `/inventory`, and `/maintenance` routes
  over the existing `ScmService::GetScmAggregate` and
  `AssetMaintenanceService::GetAssetMaintenanceAggregate` authoritative read
  boundaries. Each route requires explicit tenant, holding route, holding
  UUID, aggregate type, and aggregate UUID scope.
- Added separate SCM and Asset/Maintenance adapters with authenticated,
  audited holding query contexts, exact aggregate identity checks, bounded
  correlation/source-version/presence checks, and a one-megabyte transport
  bound. Aggregate JSON is discarded before the display model is created.
- Added loading, empty, denied, offline, and error states. The UI renders only
  allow-listed aggregate identity/version metadata; product mutations remain
  server-owned and audited.
- PASS: `cargo test -p erpx-console --offline` 69/69 and
  `cargo fmt --all --check`. Evidence packet:
  `audits/G9-G11-UI-PRODUCT-AGGREGATE-READ/README.md`.
- QUEUED: full G9–G11 workflow and vendor/device acceptance, browser/device
  journeys, live reconciliation and rollback evidence, credentialed
  directory/vendor certification, measured million-user/five-minute
  evidence, approved protobuf baseline, physical mobile execution, live Folio
  migration/rollback/retention/post-cutover evidence, MinIO round-trip, and
  explicit production approval. No activation, owner switch, Folio write
  retirement, or cutover occurred.

## Coordinator continuation — G6 scheduler registry lifecycle fence — 2026-07-28

- PASS: schedule, claim, renew, complete, fail, stale-rebuild terminalization,
  and stale reclaim now lock the exact tenant registry row with `FOR UPDATE`
  inside the same SQLx transaction as their side effects, then revalidate
  active state and exact holding binding.
- PASS: tenant lifecycle and replacement-read activation already use the same
  row lock, fencing the preflight-to-mutation lifecycle race. Permission and
  audit preflight remain mandatory.
- PASS: `cargo fmt --all --check`,
  `cargo check -p erpx-infrastructure --offline`, and the 219-test
  infrastructure unit suite.
- Evidence: `audits/G6-SCHEDULER-REGISTRY-FENCE/README.md`.
- QUEUED: full disposable PostgreSQL matrix, MinIO/PITR, external vendor and
  device acceptance, descriptor baseline, Folio migration/cutover, and
  explicit production approval. No activation, owner switch, Folio write
  retirement, or cutover occurred.

## Coordinator verification — current SCM/Asset read tree — 2026-07-27

- PASS: focused `erpx-console` suite 69/69 after the SCM and
  Asset/Maintenance adapters; formatting was applied and the route branch is
  split by service so every event handler has an unambiguous state owner.
- PASS: final serial `cargo fmt --all --check`, workspace Clippy with warnings
  denied, and `cargo test --workspace --offline --no-fail-fast`, including
  workspace and doc tests. The full ignored run passed every local
  database-backed target against ten fresh disposable databases and cleaned
  them afterward; its sole nonzero target was the intentional
  `g14_external_readiness` preflight, which failed closed with the exact
  missing Generic LDAP/RFC4533, AD DS/DirSync, and UCS references.
- PASS: `cargo check -p erpx-proto --offline`; G13 compatibility 9/9; G13
  deployment/recovery 4/4; G14 directory acceptance 8/8; G11 product-context
  7/7, Sales 3/3, and Asset/Maintenance 4/4; all six Dioxus web/desktop/mobile
  target checks; Folio lint (zero errors, four pre-existing n8n warnings),
  `bunx tsc --noEmit`, the read-only structural probe, `git diff --check`,
  and the port-3004 `/operations` HTTP 307 probe. No production Next.js build
  was run.
- QUEUED: the same external, device, descriptor, live Folio, MinIO, and
  production-approval evidence listed above. ERPX remains open across G0–G14;
  this continuation does not claim gate closure.

## Coordinator continuation — G0 current-state re-audit — 2026-07-27

- Added `audits/G0-CURRENT-RE-AUDIT/README.md` as a current-tree baseline
  packet covering workspace/contracts, deterministic fixtures, protobuf and
  Dioxus targets, the final serialized Cargo run, the ignored disposable
  PostgreSQL matrix, Folio lint/typecheck, the structural read-only probe, and
  the live port-3004 check.
- Updated the G14 matrix to replace the stale `RE-AUDIT REQUIRED` label with
  `OPEN / BLOCKED`: the current re-audit is present, while independent
  protobuf descriptor compatibility remains blocked because no approved
  baseline is configured.
- PASS: the re-audit records the final current-tree evidence without changing
  production databases, customer directories, command ownership, Folio write
  state, or cutover state.
- QUEUED: approved descriptor baseline, credentialed LDAP/AD DS/UCS
  certification, measured import/revocation, physical device execution, live
  Folio migration/rollback/retention evidence, MinIO round-trip, and explicit
  production approval. G0–G14 remain open.

## Coordinator continuation — G8 authenticated Finance subledger read — 2026-07-27

- Extended `/finance` over the existing
  `FinanceService::ListFinanceSubledgerReadRows` tenant-read projection for
  bounded AP/AR metadata. The route requires the existing explicit
  tenant/holding scope and supports optional validated aggregate-type and
  company filters without widening server authorization.
- Added authenticated/audited query transport, exact aggregate/company/branch
  identity checks, positive source-version checks, bounded pagination/cursors,
  correlation validation, and a one-megabyte payload bound. Raw aggregate JSON
  is discarded before the display model is created.
- Added explicit loading, empty, denied, offline, and error states. Posting,
  invoicing, payments, allocations, banking, and reconciliation remain
  server-owned commands.
- PASS: `cargo test -p erpx-console --offline` 70/70 and
  `cargo fmt --all --check`. Evidence packet:
  `audits/G8-UI-FINANCE-JOURNAL-READ/README.md`.
- QUEUED: broader G8 posting/AP/AR/banking/tax/consolidation workflows, live
  provider/two-database reconciliation and rollback evidence, credentialed
  directory/vendor certification, measured import/revocation, approved
  descriptor baseline, physical mobile execution, live Folio migration/
  rollback/retention/post-cutover evidence, MinIO round-trip, and explicit
  production approval. No activation, owner switch, Folio write retirement,
  or cutover occurred.

## Coordinator verification — current Finance subledger tree — 2026-07-27

- PASS: `cargo fmt --all --check`, workspace Clippy with warnings denied, and
  `cargo test --workspace --offline --no-fail-fast`; the console suite is
  70/70 and all workspace/doc tests passed.
- PASS: Dioxus web, desktop, host mobile, iOS, and Android target checks;
  Folio `bun run lint` (zero errors, four pre-existing n8n warnings) and
  `bunx tsc --noEmit`; the protected `/operations` route remains HTTP 307 on
  port 3004.
- The prior ten-database ignored acceptance baseline remains valid because
  this change only adds a client adapter over existing Finance SQLx/server/
  worker contracts. Its local targets passed and its sole nonzero target was
  the intentional credentialless G14 external-readiness preflight.
- QUEUED: full G8 posting/AP/AR/banking/tax/consolidation workflows, live
  provider and two-database reconciliation/rollback evidence, and the shared
  external, device, descriptor, Folio, MinIO, and production blockers.

## Coordinator continuation — G8 authenticated Finance banking read — 2026-07-27

- Extended `/finance` over the existing
  `FinanceService::ListFinanceBankingReadRows` tenant-read projection. The
  route now supports explicit tenant/holding scope with optional validated
  aggregate-type and company filters.
- Added authenticated/audited query transport, exact holding/filter identity
  checks, positive source-version checks, bounded pagination/cursors,
  correlation validation, and a one-megabyte payload bound. Raw banking JSON
  is discarded before the display model is created.
- Added explicit loading, empty, denied, offline, and error states. Bank
  imports, match proposals/decisions, reconciliation, and other Finance
  mutations remain server-owned and audited.
- PASS: `cargo test -p erpx-console --offline` 71/71 and
  `cargo fmt --all --check`. The Finance evidence packet now covers journal,
  AP/AR subledger, banking, and close-review reads.
- QUEUED: broader G8 workflows, provider execution, live reconciliation and
  rollback evidence, credentialed directory/vendor certification, measured
  import/revocation, approved descriptor baseline, physical mobile execution,
  live Folio migration/rollback/retention/post-cutover evidence, MinIO
  round-trip, and explicit production approval. No activation, owner switch,
  Folio write retirement, or cutover occurred.

## Coordinator continuation — G4 authenticated platform incident read — 2026-07-27

- Extended the Dioxus `/platform/incidents` route over the existing
  authenticated, platform-scoped `SupportService.ListIncidents` RPC. The
  request binds actor/session metadata and a support-specific audit purpose.
- Added a 100-row page bound, bounded correlation validation, strict conversion
  into the shared allow-listed incident DTO model, and fail-closed rejection of
  invalid rows. Internal incident timeline, broadcast, mitigation,
  root-cause, corrective-action, evidence, capability, credential, and command
  material remains outside the client model.
- Added explicit loading, empty, denied, offline, error, and read-only state
  rendering. Incident commands remain server-owned and are not exposed.
- PASS: `cargo test -p erpx-console --offline` — 72/72 after `cargo fmt --all`.
  Evidence packet: `audits/G4-UI-INCIDENT-READ/README.md`.
- QUEUED: full G4 Support/Incident workflow and approval/notification
  evidence, tenant support UI, live reconciliation and rollback, credentialed
  directory/vendor certification, measured import/revocation, approved
  descriptor baseline, physical mobile execution, live Folio migration/
  rollback/retention/post-cutover evidence, MinIO round-trip, and explicit
  production approval. No activation, owner switch, Folio write retirement,
  or cutover occurred.

## Coordinator verification — current G4 incident-read tree — 2026-07-27

- PASS: `cargo fmt --all --check`, `cargo check -p erpx-proto --offline`, all
  web/desktop/host-mobile/iOS/Android Dioxus target checks, workspace Clippy
  with warnings denied, and serialized `cargo test --workspace --offline
  --no-fail-fast`.
- PASS: `cargo test -p erpx-console --offline` — 72/72; the current workspace
  suite includes the incident adapter and all workspace/doc tests.
- PASS: Folio `bun run lint` and `bunx tsc --noEmit`; lint remains zero errors
  with four pre-existing warnings in `infra/scripts/start-n8n.js`.
- PASS: G14 read-only structural probe, `git diff --check`, correct branch,
  and the protected `/operations` HTTP 307 probe while the Folio development
  server remains on port 3004.
- The prior ignored disposable-PostgreSQL matrix remains the authoritative
  local database baseline for this console-only change. Every local target
  passed; its sole nonzero target was the intentional credentialless G14
  external-readiness preflight for Generic LDAP/RFC4533, AD DS/DirSync, and
  UCS. No external certification is claimed.

## Coordinator continuation — G4 authenticated platform support-ticket read — 2026-07-27

- Extended the shared G4 surface over `SupportService.ListTickets` for
  `/platform/support`, with authenticated platform scope and a support-specific
  audit purpose. The adapter caps the first page at 100 tickets, validates
  correlation/cursor consistency, and rejects invalid rows before rendering.
- Added a shared ticket-list state/model that retains only opaque ticket and
  customer references, typed lifecycle status, and bounded severity. Ticket
  descriptions, customer replies, internal notes, capability material, and
  commands remain outside the client model.
- PASS: `cargo test -p erpx-ui --offline` — 80/80 and
  `cargo test -p erpx-console --offline` — 73/73 after formatting. The
  incident/ticket evidence is consolidated in
  `audits/G4-UI-INCIDENT-READ/README.md`.
- QUEUED: full Support/Incident workflows, approval containment, notifications,
  tenant support, live reconciliation and rollback, credentialed
  directory/vendor certification, measured import/revocation, approved
  descriptor baseline, physical mobile execution, live Folio migration/
  rollback/retention/post-cutover evidence, MinIO round-trip, and explicit
  production approval. No activation, owner switch, Folio write retirement,
  or cutover occurred.

## Coordinator verification — current G4 incident/ticket tree — 2026-07-27

- PASS: serialized `cargo fmt --all --check`, protobuf package check, all five
  Dioxus web/desktop/mobile/iOS/Android checks, workspace Clippy with warnings
  denied, and `cargo test --workspace --offline --no-fail-fast`.
- PASS: focused shared UI suite 80/80 and console suite 73/73; the full
  workspace suite also passed all current non-ignored workspace/doc tests.
- PASS: Folio `bun run lint` and `bunx tsc --noEmit` with zero lint errors and
  four pre-existing n8n warnings; `git diff --check`, G14 structural probe,
  branch check, and `/operations` HTTP 307 on the still-running port-3004
  development server.
- The ignored disposable-PostgreSQL baseline remains valid because this wave
  changed only shared UI and console read adapters. Its local targets passed;
  the sole nonzero target was the intentional credentialless G14
  external-readiness preflight. No external certification or gate closure is
  claimed.

## Coordinator continuation — G3 Customer 360 notes transport and bounded read — 2026-07-28

- PASS: existing `SqlxControlPlane::get_customer_notes_page` behavior is now
  carried through additive `CustomerNote` protobuf fields and continuation
  cursor fields on `GetCustomer360DetailResponse`; no second notes authority
  or mutation path was introduced.
- PASS: authenticated server validation remains authoritative for customer
  scope, purpose, page size, and RFC3339/UUID cursor. Domain note redaction is
  preserved before transport; the wire contains only safe note fields and the
  opaque next cursor.
- PASS: console/shared UI boundary rejects page overflow, incomplete cursors,
  non-UUID continuation IDs, control characters, oversized values, and
  secret-bearing/invalid rows. The UI renders the bounded first page and
  requests continuation through the server cursor, binds cursor state to the
  customer UUID, and returns to the first page when the customer changes;
  local pagination never becomes authoritative.
- PASS: focused `erpx-ui` tests 81/81 and `erpx-console` tests 74/74;
  `cargo fmt --all --check`, `cargo check -p erpx-proto --offline`, UI,
  console web, and server checks passed.
- Evidence: `audits/G3-CUSTOMER360-NOTES-READ/README.md`.
- QUEUED: live multi-holding routing, backfill/reconciliation,
  binding-revocation replay, native/mobile deep-link acceptance, external
  vendor certification, measured one-million-user/five-minute evidence,
  approved descriptor baseline, live Folio cutover, MinIO round-trip, and
  explicit production approval. No activation, owner switch, Folio write
  retirement, or cutover occurred.

## Coordinator continuation — G4 authenticated platform diagnostics read — 2026-07-28

- PASS: the existing audited `GetSupportDiagnostics` server boundary now
  emits additive typed safe-summary fields alongside its server-owned raw
  snapshot. Schema version, capture timestamp, correlation ID, and bounded
  operational summaries are the only client-facing diagnostics data.
- PASS: the Dioxus console sends authenticated platform scope/actor/session
  context, rejects overflow/malformed/unknown/secret-bearing fields, and
  renders explicit loading, denied, offline, error, and read-only states. No
  capability secret, customer payload, credential, command, or local cache
  authority crosses the boundary.
- PASS: server tests 84/84 non-ignored, shared UI 81/81, console 75/75;
  focused server and console web checks plus formatting pass. Evidence:
  `audits/G4-UI-DIAGNOSTICS-READ/README.md`.
- QUEUED: full G4 support intersection/workflows, notifications, device and
  session execution, external LDAP/AD DS/UCS certification, measured
  million-user/five-minute evidence, MinIO/PITR, descriptor approval, Folio
  migration/cutover, and production approval. No activation, owner switch,
  Folio write retirement, or cutover occurred.

## Coordinator continuation — G3 Customer 360 access-audit cursor pagination — 2026-07-28

- PASS: the authenticated console now consumes the existing server-produced
  `GetCustomerAccessAudit` continuation cursor instead of suppressing it.
  Response bounds, paired RFC3339/UUID cursor shape, safe display fields, and
  explicit read-only/stale/denied/error states remain fail-closed.
- PASS: cursor state is bound to the customer UUID and customer changes reset
  to the first page; no local list is promoted to authority. Focused console
  verification is 76/76. Evidence:
  `audits/G3-CUSTOMER360-AUDIT-PAGING/README.md`.
- QUEUED: broader G3 multi-holding routing, notes/audit backfill,
  binding-revocation reconciliation, native/mobile deep links, and the shared
  external/migration/device/production blockers.

## Coordinator continuation — G8 authenticated Finance chart read — 2026-07-28

- PASS: the authenticated Dioxus Finance route now consumes the existing
  FinanceService::ListFinanceCharts tenant-read projection for an explicit
  tenant, holding, and company UUID scope. Server authorization, keyset
  pagination, SQLx persistence, source/outbox projection, reconciliation, and
  audit remain authoritative.
- PASS: the console display boundary keeps only bounded chart/account
  identifiers, labels, category, normal balance, status, source version, and
  projection timestamp. It rejects scope escapes, malformed UUIDs/cursors,
  unknown classifications, inconsistent parent flags, unsafe values, and
  overlarge account sets; chart payloads/source metadata are discarded.
- PASS: cargo check -p erpx-console --offline --no-default-features
  --features web and 77/77 focused console tests, including chart
  containment/cursor/redaction tests. Evidence:
  audits/G8-UI-FINANCE-CHART-READ/README.md.
- QUEUED: full G8 consolidation, tax/provider/live reconciliation, AP/AR
  workflows, broader Finance UI, independent database acceptance, external
  vendor certification, migration/cutover, and production approval. No
  activation, owner switch, Folio write retirement, or cutover occurred.

## Coordinator continuation — G11 authenticated HR employee read — 2026-07-28

- PASS: the authenticated Dioxus HR & Payroll route now consumes the existing
  HrService::ListHrEmployees tenant-read projection for explicit tenant,
  holding, company, and optional branch UUID scope. Server authorization,
  customer containment, pagination, audit, SQLx persistence, source/outbox
  projection, and reconciliation remain authoritative.
- PASS: the console keeps only bounded employee reference/display metadata,
  company/branch, lifecycle status, dates, source version, projection
  timestamp, and correlation. Scope escapes, malformed UUIDs/cursors,
  unknown statuses, invalid dates, inconsistent termination state, unsafe
  values, and zero source versions fail closed.
- PASS: cargo check -p erpx-console --offline --no-default-features
  --features web and 78/78 focused console tests, including branch
  containment, lifecycle consistency, date validation, cursor rejection, and
  redaction. Evidence: audits/G11-UI-HR-EMPLOYEE-READ/README.md.
- QUEUED: full HR/payroll workflows, leave/expense/reassignment, device
  journeys, independent database acceptance, migration/cutover, and
  production approval. No activation, owner switch, Folio write retirement,
  or cutover occurred.

## Coordinator continuation — G11 authenticated Legal contract read — 2026-07-28

- PASS: the authenticated Dioxus Legal route now consumes the existing
  LegalService::ListLegalContracts tenant-read projection for explicit tenant,
  holding, company, and optional branch UUID scope. Server authorization,
  customer containment, cursor pagination, retention projection, audit, SQLx
  persistence, source/outbox projection, and reconciliation remain
  authoritative.
- PASS: the console keeps only bounded contract reference/title,
  company/branch, lifecycle status, retention state/timestamp, party count,
  approval presence, source version, projection timestamp, and correlation.
  Object bytes/references/hashes and party identities are discarded; scope
  escapes, malformed cursors, unknown states, unsafe values, invalid parties,
  and overlarge party sets fail closed.
- PASS: cargo check -p erpx-console --offline --no-default-features
  --features web and 79/79 focused console tests, including Legal containment,
  cursor, object-redaction, retention, and party-bound tests. Evidence:
  audits/G11-UI-LEGAL-CONTRACT-READ/README.md.
- QUEUED: Legal document versions/access history, retention workers, device
  journeys, independent database acceptance, migration/cutover, and
  production approval. No activation, owner switch, Folio write retirement,
  or cutover occurred.

## Coordinator continuation — G11 authenticated POS sales read — 2026-07-28

- PASS: the authenticated Dioxus POS route now consumes the existing
  PosService::ListPosSales tenant-read projection for explicit tenant,
  holding, company, and optional branch UUID scope. Server authorization,
  customer containment, cursor pagination, audit, SQLx persistence,
  source/outbox projection, and reconciliation remain authoritative.
- PASS: the console keeps only sale reference, currency, bounded monetary
  totals, lifecycle status, line/payment counts with truncation markers,
  creation time, source version, projection timestamp, and correlation.
  Customer/register references and sale payloads are discarded; scope escapes,
  malformed cursors, invalid currency/status values, unsafe values, overlarge
  line/payment sets, and zero source versions fail closed.
- PASS: cargo check -p erpx-console --offline --no-default-features
  --features web and 80/80 focused console tests, including POS containment,
  currency/status, truncation, cursor, and payload-redaction tests. Evidence:
  audits/G11-UI-POS-SALES-READ/README.md.
- QUEUED: full POS workflow, device/payment execution, accounting
  reconciliation, independent database acceptance, migration/cutover, and
  production approval. No activation, owner switch, Folio write retirement,
  or cutover occurred.

## Coordinator continuation — G11 current product-context acceptance — 2026-07-28

- PASS: current-tree `g11_product_context_acceptance` — 7/7. The verifier
  reports typed API/server authorization/read handoffs for all seven contexts:
  Sales, Procurement/Inventory, Asset/Maintenance, HR/Payroll,
  Legal/Documents, POS, and Marketing/CRM.
- PASS: the worker composition root contains all seven product projection
  registrations: `sales`, `scm`, `asset_maintenance`, `hr`,
  `legal_documents`, `pos`, and `marketing`, each with its typed executor or
  factory.
- QUEUED: fresh disposable tenant write/read SQLx runs with reconciliation,
  mismatch, rebuild, and rollback evidence; provider/vendor acceptance for
  Sales, SCM, POS, payroll, document storage, and Marketing; and Folio bridge
  ownership acceptance. The read-only verifier performed no cutover or
  command-owner change.

## Coordinator verification — current G8/G11 completion wave — 2026-07-28

- PASS: serialized `cargo fmt --all --check`, workspace Clippy with
  warnings denied, and `cargo test --workspace --offline --no-fail-fast`.
- PASS: the ignored workspace database matrix against disposable,
  physically distinct local PostgreSQL targets. All configured local targets
  passed; the sole nonzero target was the intentional credentialless G14
  external-readiness preflight. Generic LDAP/RFC 4533, AD DS/DirSync, and UCS
  Provisioning Service certification remains queued with the exact missing
  endpoint, bind/subscription, CA/server, TLS, import, and revocation evidence
  references recorded by the preflight. No vendor certification pass is
  claimed.
- PASS: `cargo check -p erpx-proto --offline`, the protobuf package tests,
  all five Dioxus web/desktop/host-mobile/iOS/Android checks, G13 parallel
  acceptance, G14 structural acceptance, `git diff --check`, and branch
  validation. The repository descriptor comparator executed but correctly
  left compatibility queued because no approved descriptor baseline is
  configured.
- PASS: Folio `bun run lint` and `bunx tsc --noEmit`; lint has zero errors and
  four pre-existing n8n warnings. The protected `/operations` probe returns
  HTTP 307 while Folio remains operational on port 3004. No production
  Next.js build was run.
- QUEUED: full G0–G14 external/vendor, scale, mobile-device, approved
  descriptor, Folio migration/rollback/retention/reconciliation, MinIO/PITR,
  and production-approval evidence. No directory activation, command-owner
  switch, Folio write retirement, or production cutover occurred.

## Coordinator continuation — current local acceptance packet — 2026-07-28

- PASS: G14 directory acceptance 8/8; Generic LDAP/RFC 4533, AD DS/DirSync,
  and UCS certification remains explicitly queued with the exact missing
  references.
- PASS: G13 deployment/recovery acceptance 4/4; G9 product transport 3/3;
  G11 Sales transport 3/3; G11 Asset/Maintenance acceptance 4/4; G11
  product-context acceptance 7/7, including its opt-in domain,
  infrastructure, and worker library suites; and the G2 authorization route
  inventory.
- QUEUED: these focused tests still require real disposable database pairs,
  live vendor/provider endpoints, approved descriptor input, and G13 runtime
  migration/rollback references for full gate evidence. No activation,
  command-owner switch, Folio write retirement, or production cutover was
  performed.

## Coordinator continuation — directory full-import RBAC and projection — 2026-07-28

- PASS: durable full-import finalization now receives connector role mappings,
  validates persisted connector scope, persists newer mapping revisions, and
  appends immutable hash-chained grant/revoke facts for staged and missing
  snapshot identities. The role sequence advances beyond any existing tail.
- PASS: the same SQLx transaction hydrates `directory_identity_links` and the
  disposable identity projection for active/disabled/deleted/missing users,
  retaining canonical immutable-link conflict checks and redacted outbox
  publication. Processing is bounded in 500-row batches for the one-million
  user import ceiling.
- PASS: `cargo check -p erpx-identity -p erpx-worker --offline`, identity and
  worker focused tests, identity/worker Clippy with `-D warnings`, G14
  directory acceptance 8/8, and the local product acceptance packet pass.
- PASS: the new ignored PostgreSQL directory role/projection assertions and
  scope-escape test passed against a fresh disposable database; the database
  was dropped after the run. Generic LDAP/RFC 4533, on-prem AD DS/DirSync, and
  UCS credentialed acceptance remain unavailable. No activation, command-owner
  switch, Folio write retirement, or cutover was performed.

## Coordinator continuation — directory shared authorization boundary — 2026-07-28

- PASS: identity-side `SqlxIdentityStore::directory_role_access` now evaluates
  directory-managed permissions from the current connector projection and role
  mapping, including connector enablement, active/revoked state, group
  membership, exact platform/holding scope containment, and the five-minute
  feed freshness bound. High-risk mappings require a recent write-side TOTP
  verification timestamp.
- PASS: `AuthContext::authorize_role` consults this result before local
  control-plane roles. A persisted directory projection makes a missing or
  stale directory grant an explicit denial, so local assignments cannot bypass
  directory revocation; principals without a directory projection retain the
  existing local role path.
- PASS: focused server/identity compilation and warnings-denied Clippy, plus
  the ignored PostgreSQL active-grant, disabled-revoke, and stale-feed shared
  authorization assertions on a disposable database. The database was dropped
  after the run. Vendor certification, scale, physical MFA, and production /
  cutover evidence remain queued; no activation or owner switch occurred.

## Coordinator verification — directory authorization and current tree — 2026-07-28

- PASS: serialized `cargo fmt --all --check`, workspace Clippy with warnings
  denied, and `cargo test --workspace --offline --no-fail-fast`.
- PASS: current-tree directory SQLx acceptance on a fresh disposable database:
  active directory grant, disabled/stale denial, high-risk recent-TOTP gate,
  full-import role facts/projection, native changes, and scope containment. The
  database was dropped after the run.
- QUEUED/BLOCKED EXTERNAL: the aggregate ignored workspace command was
  attempted and stopped at the deliberate missing
  `ERPX_TEST_DIRECTORY_RBAC_DATABASE_URL` reference. The previously corrected
  disposable local matrix remains valid for the other local database targets;
  no mock or fabricated environment was used. Existing unrelated `erpx_*`
  PostgreSQL databases were left untouched.
- QUEUED/BLOCKED EXTERNAL: the dedicated ignored G14 certification preflight
  also failed closed and listed the missing Generic LDAP, AD DS/DirSync, and
  UCS endpoint, TLS/CA/server, bind/subscription credential, disposable
  database, one-million-user import, five-minute revocation, and evidence-path
  references. This is an intentional certification blocker, not a vendor pass.
- PASS: `cargo check/test -p erpx-proto`, G13 acceptance 9/9, G13 recovery 4/4,
  all five Dioxus target checks, read-only structural audit, authorization route
  inventory, `git diff --check`, Folio lint/typecheck, and HTTP 307 on port
  3004. Lint has four pre-existing n8n warnings and no errors.
- QUEUED: external LDAP/RFC 4533, AD DS/DirSync, UCS, scale/revocation,
  approved descriptor, physical device, live Folio migration/cutover,
  MinIO/PITR, and production-approval evidence. No activation, command-owner
  switch, Folio write retirement, or cutover occurred.

## Coordinator continuation — G13 authenticated execution-work command — 2026-07-28

- PASS: additive `MigrationService.QueueMigrationWork` protobuf/server/console
  path with authenticated platform scope, directory-aware `migration.execute`,
  fresh MFA, bounded work kind/scope/fence validation, and caller idempotency.
- PASS: `SqlxG13MigrationStore::enqueue_work_command` atomically persists the
  request hash, deterministic work item, and `migration.work_enqueued` outbox
  event; same-key replay is duplicate-free and conflicting payloads fail
  closed. The existing worker lease/claim/complete path remains the consumer.
- PASS: disposable `g13_migration_persistence` queue/claim/complete acceptance;
  disposable G13 source/target/import/replay/reconciliation/rollback acceptance;
  server/infrastructure/proto checks; and console web plus 81 unit tests.
- QUEUED: serialized full workspace verification after this shared proto/server
  boundary, live Folio source/target migration and rollback/retention evidence,
  approved descriptor comparison, credentialed directory certification, and
  production approval. No activation, ownership switch, Folio write retirement,
  or cutover occurred.

## Coordinator verification — G13 queue command integration — 2026-07-28

- PASS: `cargo fmt --all --check`; workspace Clippy with warnings denied; and
  the complete non-ignored workspace tests after the G13 structural verifier
  was updated to recognize the authenticated queue RPC.
- PASS: protobuf package check/tests, G13 parallel acceptance 9/9, G13
  deployment/recovery acceptance 4/4, G14 directory acceptance 8/8, and all
  five Dioxus web/desktop/host-mobile/iOS/Android checks plus the macOS desktop
  target check.
- PASS: Folio `bun run lint` (zero errors, four pre-existing n8n warnings),
  `bunx tsc --noEmit`, branch validation, and the protected `/operations` HTTP
  307 probe while the dev server remains listening on port 3004. No production
  Next.js build was run.
- QUEUED/BLOCKED EXTERNAL: the aggregate ignored workspace run was executed as
  required and failed closed at 64 targets because explicit disposable
  database and vendor references are not configured. Disposable G13 source/
  target/import/replay/reconciliation/rollback tests passed. The missing
  Generic LDAP/RFC 4533, AD DS/DirSync, UCS, approved descriptor, live Folio
  migration/rollback/retention, physical-device, scale/revocation, and
  production-approval evidence remains queued. No activation, command-owner
  switch, Folio write retirement, or cutover occurred.

## Coordinator continuation — G13 lifecycle command integration — 2026-07-28

- PASS: additive `MigrationService.BeginMigrationRun` and
  `MigrationService.AdvanceMigrationPhase` protobuf/server boundaries call the
  authoritative SQLx run aggregate and ordered transition event path.
- PASS: both commands require platform scope, authenticated actor matching,
  directory-aware `migration.execute`, fresh MFA, bounded fields, optimistic
  version/phase fencing, and caller idempotency; server-side phase allowlisting
  stops before command-owner approval and cutover.
- PASS: Platform Settings console controls and bounded request tests cover
  begin/advance without source rows, credentials, approval references, Folio
  retirement, or cutover fields. Focused server/console/proto checks passed.
- QUEUED: full serialized workspace verification follows this shared protobuf
  change. Credentialed directory certification, approved descriptor, live
  Folio migration/rollback/retention, physical-device, scale/revocation, and
  production-approval evidence remain unavailable. No activation, ownership
  switch, Folio write retirement, or cutover occurred.

## Coordinator verification — G13 lifecycle command integration — 2026-07-28

- PASS: `cargo fmt --all --check`, workspace Clippy with warnings denied, and
  the complete non-ignored workspace test suite. The structural G13 acceptance
  passed 9/9 after its safety marker was updated for the lifecycle boundary.
- PASS: protobuf package tests, G13 deployment/recovery 4/4, G14 directory
  acceptance 8/8, and Dioxus web/desktop/mobile/iOS/Android/macOS target
  checks.
- PASS: Folio `bun run lint` (zero errors; four pre-existing n8n warnings),
  `bunx tsc --noEmit`, branch validation on `codex/erpx-parallel-completion`,
  `git diff --check`, and protected `/operations` HTTP 307 while the Folio
  dev server remains listening on port 3004. No production Next.js build was
  run.
- QUEUED/BLOCKED EXTERNAL: the required aggregate ignored workspace run was
  executed and failed closed at 64 targets because explicit disposable
  PostgreSQL, Generic LDAP/RFC 4533, AD DS/DirSync, UCS, and related runtime
  references are absent. The local disposable G13 source/target/import and
  finance candidate checks that were configured passed; no missing-resource
  result was promoted to a certification pass.
- QUEUED: approved protobuf descriptor comparison, live two-worker restart and
  lease evidence, credentialed one-million-user/vendor directory acceptance,
  physical mobile secure-storage checks, live Folio migration/shadow/final
  delta/rollback/retention, post-cutover rejection, and explicit production
  approval. No directory activation, command-owner switch, Folio write
  retirement, or production cutover occurred.

## Coordinator continuation — G12 iOS secure-session adapter — 2026-07-28

- PASS: bounded/revalidated secure-session wire format tests; iOS-only
  `security-framework` adapter; `cargo test -p erpx-ui --offline`; UI
  warnings-denied Clippy; iOS-target UI check; and iOS-target console mobile
  check.
- PASS at source level: iOS save/load/clear route through the system Keychain
  and never use browser/file/plaintext fallback. Android and non-iOS targets
  still return the explicit unsupported error.
- QUEUED: Android Keystore bridge, iOS/Android app packaging and device or
  simulator execution, native notifications, offline/conflict recovery,
  accessibility, and mobile approval evidence. This does not close G12 or
  G0–G14 and did not activate any directory or production path.

## Coordinator continuation — G7 source outbox feed adapter — 2026-07-28

- PASS: `SqlxOperationsStore::read_source_outbox_event_page` adds a bounded,
  holding-routed source feed for Operations. It requires scope, actor, event
  version, correlation, identity, and timestamp envelope fields; maps only
  the closed source-domain set; and fails closed on unknown or incomplete
  legacy rows.
- PASS: focused infrastructure source-feed tests, warnings-denied
  infrastructure Clippy, and worker compilation. Source payload ownership
  remains with its bounded context; no new Operations source-of-truth payload
  was introduced.
- QUEUED: automatic detection policy evaluation, SLA notifications,
  manager-chain resolution, and live source/vendor certification. G7 and
  G0–G14 remain open; no directory activation, owner switch, Folio write
  retirement, or production cutover occurred.

## Coordinator verification — required ignored suite — 2026-07-28

- EXECUTED: `cargo test --workspace --offline --no-fail-fast -- --ignored
  --nocapture`.
- BLOCKED/QUEUED: the run failed closed at 64 targets because explicit
  disposable PostgreSQL, Generic LDAP/RFC 4533, AD DS/DirSync, UCS, backup,
  and related environment references are not configured. The failure output
  names the missing references; no fixture, mock, or fabricated vendor result
  was promoted to evidence.
- PASS: configured finance candidate acceptance completed, while Folio lint,
  typecheck, port-3004 health, branch, diff-check, and all non-ignored Rust
  gates remain green. G0–G14 stay open pending external evidence.

## Coordinator continuation — G12 Apple secure session and G7 source feed fence — 2026-07-28

- PASS: `erpx-ui` now wires the bounded, revalidated session envelope through
  the Apple system Keychain for iOS and macOS. Web, Android, and Windows/Linux
  desktop remain fail-closed without a target-owned protected adapter; no
  plaintext, browser, file, or local fallback was introduced.
- PASS: host macOS UI/console checks, 82 UI tests, UI Clippy with warnings
  denied, worker check/Clippy, and 204 worker unit tests plus the worker's
  integration/unit targets.
- PASS: every authenticated Operations reconciliation pass now reads the
  source-owned tenant outbox through the bounded SQLx cursor adapter, validates
  the complete shared envelope, counts inspected facts, and records out-of-
  scope feed violations as unhealthy. Detection policy and manager authority
  remain explicit scheduler inputs; the worker does not fabricate them.
- QUEUED: physical Apple/Android execution, Android Keystore, credentialed
  LDAP/AD/UCS certification, measured scale/revocation, approved protobuf
  descriptor comparison, live migration/cutover, and G7 producer scheduling
  remain open. No directory activation, command-owner switch, Folio write
  retirement, or production cutover occurred.

## Coordinator local SQLx acceptance re-audit — 2026-07-28

- PASS: the full ignored workspace command was rerun against fresh disposable
  local PostgreSQL databases for all configured control-plane, tenant,
  commercial, scheduler, support, directory, and G13 source/target/rollback
  lanes. Every configured local SQLx acceptance target passed, including the
  Operations source-feed reconciliation test; cleanup removed the temporary
  databases.
- PASS: the Operations worker consumed one complete source-owned outbox
  envelope and rejected a wrong-holding envelope through the authoritative
  scope fence. This is local persistence/worker evidence, not a vendor-feed
  claim.
- BLOCKED/QUEUED: the sole nonzero target was the intentionally credentialless
  G14 external-certification preflight. It enumerated missing Generic LDAP/RFC
  4533, AD DS/DirSync, and UCS Provisioning Service endpoint, credential,
  certificate, import, revocation, and evidence references. No vendor,
  one-million-user, or five-minute-revocation certification was claimed.
- QUEUED: approved descriptor comparison, physical mobile execution and
  Android Keystore, live Folio migration/rollback/retention/cutover, G7
  producer scheduling, and explicit production approval remain open. No
  directory activation, owner switch, Folio write retirement, or cutover
  occurred.

## Coordinator final verification — local acceptance re-audit — 2026-07-28

- PASS: serialized `cargo fmt --all --check`, workspace Clippy with
  warnings denied, and `cargo test --workspace --offline --no-fail-fast`.
- PASS: protobuf package check/tests, G13 compatibility and deployment/
  recovery suites, Dioxus web/desktop/host-mobile/macOS/iOS/Android checks,
  `bun run lint`, `bunx tsc --noEmit`, `git diff --check`, and the read-only
  G14 structural probe.
- PASS: Folio development server remains on port 3004 and the protected
  `/operations` probe returned HTTP 307. No production Next.js build ran.
- BLOCKED/QUEUED: the fresh-database ignored suite passed all configured local
  SQLx targets; only the credentialless G14 external-certification preflight
  failed closed with the exact Generic LDAP/RFC 4533, AD DS/DirSync, and UCS
  references still required. No vendor or production certification was
  claimed.

G0–G14 remain open pending external directory, scale/revocation, approved
descriptor, device, live migration/rollback/retention, and production approval
evidence. No directory activation, command-owner switch, Folio write
retirement, or production cutover occurred.

## Coordinator continuation — G7 source-outbox detection scheduling — 2026-07-28

- PASS: the Operations completion worker now has a bounded source-outbox
  scheduler driven by explicit coordinator policy rules and current
  authorization/assignee/manager-chain proof. It matches only named source
  domains and types, fails closed on scope and bounds, and derives stable
  exception/correlation/idempotency identities for replay.
- PASS: source outbox event IDs remain immutable provenance/causation while
  Operations application events use deterministic derived IDs, preventing a
  source row from colliding with its Operations outbox event.
- PASS: unit tests, infrastructure/worker warnings-denied Clippy, and the
  ignored fresh-PostgreSQL scheduler acceptance. The acceptance read one
  source envelope, reproduced identical IDs on a second scan, and persisted
  one assigned exception through SQLx, the Operations event/outbox, and the
  projection path; temporary databases were removed.
- QUEUED: automatic policy publication, manager authority evaluation, expired
  user reassignment scheduling, authenticated Operations route/UI, SLA
  notifications, Folio parity, and live vendor certification remain open.
  No directory activation, command-owner switch, Folio write retirement, or
  production cutover occurred.

## Coordinator final delta — deterministic notification expiry and serialized gates — 2026-07-28

- PASS: the Operations notification claim transition now compares expiry to
  the caller's explicit scheduler time throughout the transaction. Fresh
  disposable PostgreSQL source-reconciliation and scheduler/notification
  acceptance cases passed after the change, including one durable delivery
  outbox handoff and replay with no duplicate.
- PASS: post-change `cargo fmt --all --check`, workspace warnings-denied
  Clippy, and the complete non-ignored workspace test suite passed.
- QUEUED: all local SQLx targets in the fresh ignored matrix passed in the
  recorded re-audit; only the intentional credentialless G14 external
  preflight remains nonzero. Live LDAP/AD/UCS certification and measured
  scale/revocation, approved protobuf comparison, physical mobile evidence,
  live Folio migration/rollback/retention, and production approval remain
  open. No activation, owner switch, Folio write retirement, or cutover was
  performed.

## Coordinator continuation — G7 expired-assignee scheduling — 2026-07-28

- PASS: the Operations completion worker now drains the bounded active-
  exception read model after its current event projection pass and constructs
  reassignment requests only from coordinator-supplied expired-principal
  rules. Replacement, manager, manager-chain, MFA authorization, and actor
  proofs are explicit; scope escape, inactive proofs, duplicates, page/queue
  overflow, and invalid identities fail closed.
- PASS: deterministic exception-version-bound idempotency/correlation keys,
  SQLx expected-version persistence, append-only reassignment event/outbox,
  and the existing read-projector path are exercised by unit tests and the
  ignored fresh-PostgreSQL Operations acceptance. Two scans produced the same
  request and the persisted aggregate moved to the replacement principal.
- QUEUED: external identity-expiration evaluation, manager authority
  publication, authenticated route/UI, notifications, Folio parity, and live
  vendor certification remain open. No directory activation, owner switch,
  Folio write retirement, or production cutover occurred.

## Coordinator post-reassignment verification — 2026-07-28

- PASS: after the reassignment scheduler change, serialized formatting,
  workspace warnings-denied Clippy, and the complete non-ignored workspace
  test suite passed.
- PASS: the fresh-database ignored workspace matrix passed every configured
  local SQLx target, including both Operations scheduler tests. The sole
  nonzero target remained the deliberate G14 external-certification preflight,
  which failed closed with the exact unavailable Generic LDAP/RFC 4533,
  AD DS/DirSync, and UCS references; no external result was claimed.
- QUEUED: the approved protobuf baseline, credentialed vendor endpoints and
  measured scale/revocation evidence, physical mobile execution, live Folio
  migration/rollback/retention/cutover, and explicit production approval.

## Coordinator continuation — G7 due-SLA escalation scheduling — 2026-07-28

- PASS: the Operations completion worker scans the bounded active-exception
  read model after projection and creates escalation requests only for due,
  assigned, non-escalated cases matched by explicit coordinator rules.
  Manager-chain, MFA authorization, actor, and scope proofs are mandatory;
  unsafe rules, duplicates, missing due times, and page/queue overflow fail
  closed.
- PASS: exception/version/due-time-bound idempotency and correlation keys,
  SQLx expected-version/due-time enforcement, append-only escalation event and
  outbox, and read projection behavior are covered by unit tests and the
  ignored fresh-PostgreSQL Operations acceptance. Two scans produced the same
  request and the aggregate reached `escalated`.
- QUEUED: policy publication, notification delivery, authenticated UI
  integration, Folio parity, and live vendor certification remain open. No
  directory activation, owner switch, Folio write retirement, or production
  cutover occurred.

## Coordinator continuation — G7 transactional SLA notification delivery — 2026-07-28

- PASS: escalation now creates a forced-RLS Operations SLA notification row in
  the same SQLx transaction as the lifecycle event, snapshot, and outbox.
  Notification claims, lease expiry, exact-worker acknowledgement, retry
  release, and terminal expiry are bounded and durable.
- PASS: the registered notification worker emits one deterministic
  `operations.sla_notification_delivered.v1` tenant-outbox handoff before
  acknowledgement. The fresh disposable PostgreSQL Operations acceptance
  passed enqueue, claim, handoff, acknowledgement, and replay with no
  duplicate event.
- QUEUED: downstream provider/device delivery, automatic policy publication,
  manager authority evaluation, Folio parity, and external certification.
  No directory activation, owner switch, Folio write retirement, or
  production cutover occurred.

## Coordinator continuation — G6 credentialed PostgreSQL/MinIO round trip — 2026-07-28

- PASS: `cargo test -p erpx-infrastructure --test backup_live --offline -- --ignored --nocapture` completed against disposable PostgreSQL 18 databases and the credentialed host MinIO service. The test used real compatible `pg_dump`/`pg_restore` binaries, a read-side logical archive, HTTPS object transport with a trusted disposable internal CA, a distinct candidate read restore, forced-RLS verification, full inventory reconciliation, exact artifact deletion, and database cleanup.
- PASS: the adapter now captures `pg_dump` stdout without passing `--file=-`; the previous live mismatch was diagnosed as an empty artifact caused by PostgreSQL treating that value as a literal filename. Explicit tool paths are accepted by the live fixture so deployment tool/server version mismatches fail closed and are visible.
- PASS: `MinioObjectStore::new_with_ca_certificate` supports public internal-CA trust without weakening the HTTPS-only operational endpoint validation. Credentials and connection material remain redacted and environment-gated.
- QUEUED: this evidence covers only local read-side logical backup/restore/object cleanup. True PITR/WAL recovery, write-side production backup certification, provider-native retention, external directory/mobile/descriptor evidence, live Folio migration/rollback/cutover, and production approval remain open. No directory activation, command-owner switch, Folio write retirement, or production cutover occurred.

## Coordinator final verification delta — G6 live round trip — 2026-07-28

- PASS: after the `pg_dump` stdout regression assertion, serialized `cargo fmt --all --check`, workspace warnings-denied Clippy, and `cargo test --workspace --offline --no-fail-fast`.
- PASS: protobuf package check/tests, G13 compatibility/deployment-recovery, G14 structural/directory suites, Dioxus web/desktop/host-mobile/Android/iOS checks, Folio `bun run lint` (zero errors, four pre-existing warnings), and `bunx tsc --noEmit`.
- PASS: fresh disposable G6 SQLx job/retention/worker checks and the credential-gated real PostgreSQL/MinIO read-side backup/restore acceptance. The live test verified TLS object transport, candidate forced RLS, full inventory cleanup, and physical database cleanup; no test artifacts or disposable databases remain.
- PASS: Folio remains operational on port 3004 (`/operations` 307, root 200). No production Next.js build ran.
- QUEUED: prior current-tree ignored-matrix evidence remains the source for the full local database wave; its only expected nonzero is the intentional credentialless G14 preflight. Approved protobuf baseline, credentialed Generic LDAP/AD DS/UCS and measured million-user/five-minute evidence, physical mobile execution, true PITR/WAL, live Folio migration/rollback/retention/post-cutover proof, and explicit production approval remain open. No activation, owner switch, Folio write retirement, or cutover occurred.

## Coordinator continuation — G13 disposable migration rehearsal — 2026-07-28

- PASS: `cargo test -p erpx-infrastructure --test
  g13_disposable_acceptance --offline -- --ignored --nocapture` passed the
  real disposable PostgreSQL source/target/read/rollback flow. It verified
  source snapshot persistence, separate read projection, immutable mapped
  imports, outbox evidence, shadow replay, source-versus-target
  reconciliation, rollback restoration, and the Folio-write fence rejecting
  an unapproved final delta.
- PASS: the acceptance cleaned every disposable database after completion.
- QUEUED: credentialed Folio migration, two-worker lease kill/restart,
  approved descriptor baseline, retention/cutover approval, and Folio write
  retirement remain open. No command-owner switch, directory activation,
  Folio write retirement, or production cutover occurred.

## Coordinator continuation — G9–G11 product shell registry — 2026-07-28

- PASS: the target-neutral Dioxus UI now has an authenticated, read-only
  allow-listed shell registry for Sales, Procurement, Inventory, Assets,
  Maintenance, HR/Payroll, Legal, POS, and Marketing/CRM. Navigation targets
  pass through deep-link redaction and route resolution rejects credential or
  traversal material.
- PASS: `cargo test -p erpx-ui --offline -- --nocapture` (84 unit tests plus
  target integration suites), warnings-denied UI Clippy, and the updated
  `g11_product_context_acceptance` (8/8) passed.
- QUEUED: this closes shell metadata only; authenticated role journeys, live
  routed SQLx workflow/reconciliation, vendor/provider execution, and Folio
  parity remain open. No activation, owner switch, Folio write retirement, or
  production cutover occurred.

## Coordinator verification — G13/G14 and shell delta — 2026-07-28

- PASS: serialized `cargo fmt --all --check`, workspace warnings-denied
  Clippy, and `cargo test --workspace --offline --no-fail-fast` after the
  product shell change; the 1M-user directory test passed.
- PASS: G13 parallel 9/9, deployment recovery 4/4, G14 directory 8/8,
  external-readiness 7/7 plus its intentional ignored certification
  preflight, protobuf package test/check, all Dioxus web/desktop/host-mobile/
  Android/iOS/macOS checks, `bun run lint`, `bunx tsc --noEmit`, and the
  Folio port guard (root 200, `/operations` 307).
- QUEUED: credentialed Generic LDAP/AD DS/UCS certification and measured
  scale/revocation, approved descriptor baseline, physical mobile/Android
  Keystore, true PITR/WAL, live Folio migration/retention/cutover, and
  explicit production approval. No activation, owner switch, Folio write
  retirement, or production cutover occurred.

## Coordinator continuation — G6 durable PITR recovery-point admission — 2026-07-28

- PASS: `PitrRecoveryPoint` domain validation requires exactly one timestamp
  or LSN and bounded opaque WAL/physical-target references; malformed or
  future points fail closed.
- PASS: protobuf/API, idempotency hashing, control migration `0062`, read
  vocabulary migration `0013`, authoritative job/run persistence, claim
  binding, and redacted debug are wired. A fresh disposable PostgreSQL run
  persisted/projected 1,001 full jobs plus one PITR job and linked claims;
  both databases were cleaned up.
- QUEUED: physical `pg_basebackup` + WAL archive import and physical-cluster
  restore verification remain deployment-bound. No logical dump is called
  PITR; no activation, owner switch, Folio write retirement, or production
  cutover occurred.

## Coordinator continuation — physical PITR artifact capture seam — 2026-07-28

- PASS: focused infrastructure tests cover PITR target path/reference
  rejection, tenant/chain/backup object-key scoping, and bounded WAL archive
  collection.
- PASS: infrastructure compiles with warnings-denied package Clippy after the
  capture adapter and output-directory size guard were added.
- QUEUED: credentialed `pg_basebackup`/WAL capture, physical-cluster restore
  and replay verification, and worker registration remain deployment-bound.
  The restore executor is explicitly fail-closed; no activation, owner switch,
  Folio write retirement, or production cutover occurred.

## Coordinator continuation — physical PITR replay executor seam — 2026-07-28

- PASS: the concrete replay executor verifies manifest identity and SHA-256
  object digests, rehydrates base/WAL artifacts beneath a contained restore
  root, writes recovery signal/configuration, starts/stops `pg_ctl`, and
  requires `psql` recovery-mode evidence. Focused target-containment and
  recovery-configuration tests pass.
- QUEUED: the target resolver is intentionally unconfigured and worker
  registration remains disabled until credentialed physical replay and
  independent post-recovery certification pass. No production target was
  started, and no activation, owner switch, Folio write retirement, or
  cutover occurred.

## Coordinator verification delta — final local gates — 2026-07-28

- PASS: after the bounded PITR config hardening and corrected valid-metadata
  fail-closed fixture, serialized `cargo fmt --all --check`, workspace
  warnings-denied Clippy, and `cargo test --workspace --offline
  --no-fail-fast --quiet` passed.
- PASS: focused disposable PostgreSQL control acceptance persisted/projected
  1,001 full jobs plus one PITR job; protobuf/G13 registration, Dioxus web,
  desktop, host-mobile, iOS, Android, Folio lint/typecheck, and port 3004
  checks passed. No production Next.js build ran.
- QUEUED: aggregate ignored database/vendor targets remain dependent on the
  explicit external credentials and references recorded by the prior audit;
  physical PITR restore/replay, approved descriptor, physical mobile, live
  Folio migration/retention/cutover, and production approval remain open.

## Coordinator continuation — G12 Android Keystore-backed persistence — 2026-07-28

- PASS: added the Android-only JNI/NDK bridge and target adapter. It uses the runtime Android
  context, AndroidKeyStore AES-GCM key material, authenticated bounded session envelopes, and
  synchronous private SharedPreferences commits. Missing context/key, malformed ciphertext, and
  GCM authentication failures fail closed; there is no file, browser, URL, or plaintext fallback.
- PASS: the console now restores supported-target sessions and persists login/refresh/revoke
  lifecycle changes through the target-owned secure adapter. Web/unsupported desktop remain
  explicitly unsupported for local persistence.
- PASS: `cargo test -p erpx-ui --offline --no-default-features`; Android UI and console target
  checks; iOS console target check; host web/desktop checks.
- QUEUED: no Android SDK/NDK, ADB, emulator, or available iOS simulator is present. Device
 Keystore/Keychain round-trip, mobile package installation, notifications, deep links, offline
 conflict recovery, camera/accessibility, and mobile MFA journeys require supplied device
 tooling. No activation, owner switch, Folio write retirement, or cutover occurred.

## Entry 2026-07-28 — integrated G8 durable bank-provider import source

**Owner:** Main coordinator  
**Status:** Integrated local packet; G8 remains open

### Delivered

- Added control migration 0063 for durable Finance bank-provider import jobs,
  claim leases, retry/dead-letter transitions, and append-only reconciliation /
  rollback evidence.
- Added control-read migration 0014 for status-only rebuildable projection.
- Added idempotent SQLx enqueue, request/MFA-authorization validation,
  source-fenced claim/reclaim, bounded retry/dead-letter handling, rollback
  evidence, and exact tenant-write/import-outbox completion evidence.
- Added an explicit opt-in worker registry composition method requiring the
  injected router, authorizer, credential resolver, and provider adapter.

### Verification

- PASS: real disposable two-database PostgreSQL acceptance for enqueue,
  idempotency, read status, claim/retry/reclaim, rollback evidence, and
  reconciled completion.
- PASS: `cargo fmt --all --check`, worker/infrastructure focused tests, and
  warnings-denied focused Clippy.

### Security and rollback

The read projection contains no request, authorization, credential-key, or
provider response payload. No provider secret or SDK was added. Rollback is
additive removal of migration 0063/0014 and the source/export/opt-in
registration; existing tenant banking behavior remains. Provider/credential
composition, live certification, and the broader G8 gate remain open. No
directory activation, command-owner switch, Folio write retirement, or
production cutover occurred.

## Entry 2026-07-30 — bounded billing-policy worker adoption

The registered `stripe_billing_policy` worker now invokes the existing
authoritative `advance_billing_policy` command for due `past_due` and
expired-`grace` lifecycles. Selection is bounded with `FOR UPDATE SKIP LOCKED`;
deterministic row/time idempotency references and a single transaction preserve
domain transition, snapshot, outbox, audit, and receipt atomicity. The Stripe
semantic handler remains provider-evidence-only and does not gain payment
recovery or cancellation authority.

PASS: infrastructure commercial tests 29/29 active, worker registry 1/1,
warnings-denied focused Clippy, infrastructure disposable SQLx atomic worker
boundary 1/1, and worker disposable SQLx advance/replay acceptance 1/1. The
worker fixture advanced one due row, and the immediate replay processed zero.
Temporary PostgreSQL databases were dropped after both acceptance runs.

QUEUED: live Stripe credentials/provider/catalog reconciliation, credentialed
Generic LDAP/RFC 4533, AD DS/DirSync, UCS Provisioning Service, approved
protobuf provenance, physical PITR/device/scale evidence, and production
approval remain unavailable. G0–G14 are not claimed complete; no directory
activation, command-owner switch, Folio write retirement, or production
cutover occurred.

## Entry 2026-07-28 — G9–G11 disposable SQLx revalidation

**Status:** Local acceptance evidence integrated; G9–G11 remain open

Six disjoint temporary tenant write/read PostgreSQL pairs were used for real
ignored acceptance targets. Sales passed 2/2, SCM/Procurement–Inventory 5/5,
HR/payroll 4/4, Legal 4/4, POS 5/5, and Marketing CRM/delivery 5/5. The
targets exercised source/outbox atomicity, scoped reads, idempotency,
projection/rebuild checkpoints, mismatch detection, rollback/retention where
defined, and callback state. Explicit cleanup removed every temporary pair.

This is local SQLx evidence only. Vendor/device certification, full product
workflow and Finance/Operations reconciliation, runtime restart/lease proof,
browser/mobile journeys, and production approval remain queued. No source
retirement, command-owner switch, directory activation, Folio write retirement,
or cutover occurred.

## Entry 2026-07-28 — post-status-RPC serialized verification

After the protobuf/server status query was added, `cargo fmt --all --check`,
workspace warnings-denied Clippy, and `cargo test --workspace --offline
--no-fail-fast --quiet` passed again. The server suite now reports 85 passed
and 1 ignored; no new failures were introduced. The status API remains
additive, authenticated, and payload-free. External/vendor, mobile-device,
PITR/MinIO, Folio migration, approved-descriptor, scale, and production gates
remain queued; no activation or cutover occurred.

## Entry 2026-07-28 — authenticated G8 bank-import status query

Added `FinanceService.GetFinanceBankProviderImportJobStatus` over the durable
control-read status projection. The server performs authenticated explicit
scope and `finance.read` authorization, exact opaque tenant/holding matching,
bounded status mapping, and audit persistence. No request, authorization,
credential-key, or provider response payload crosses the wire.

PASS: protobuf/server checks, server strict Clippy, and 85/85 server tests.
The visible console status panel, live provider/credential acceptance, and
broader Finance workflow remain open. The RPC is additive and rollback is
limited to the RPC/mapping/status-read seam; no source or Folio ownership state
was changed.

## Entry 2026-07-28 — current-tree verification after G8/G9–G11 packets

The serialized verification gate is green after the current changes:

- `cargo fmt --all --check` passed;
- workspace Clippy with warnings denied passed;
- `cargo test --workspace --offline --no-fail-fast --quiet` passed;
- protobuf package, G13 compatibility/registration, and G14 directory
  acceptance checks passed;
- Dioxus web, desktop, Android, and iOS console target checks passed;
- Folio `bun run lint` and `bunx tsc --noEmit` passed;
- `git diff --check` passed, and Folio remained healthy on port 3004 (`/` 200,
  protected `/operations` 307). No production Next.js build was run.

The aggregate ignored database matrix still reports only its explicit missing
external/runtime resources; the newly supplied local G8 and G9–G11 disposable
PostgreSQL targets passed and were cleaned up. Credentialed LDAP/AD DS/UCS,
approved descriptor baseline, measured scale/revocation, physical mobile,
MinIO/PITR restore, live Folio migration/cutover, and production approval
remain queued. No activation, owner switch, Folio write retirement, or
cutover occurred.

## Entry 2026-07-28 — aggregate ignored matrix result

Reran the required aggregate command:
`cargo test --workspace --offline --no-fail-fast --quiet -- --ignored`.
The command exited 101 with 66 failed targets. Each observed failure was an
explicit missing-resource or queued-blocker path, including absent
`ERPX_TEST_CONTROL_WRITE_DATABASE_URL`, tenant/directory database URLs,
`ERPX_G14_*` directory certification variables, or external backup/runtime
credentials. No external directory or provider environment was activated,
and no test was treated as passing without its required authoritative
environment. The result is recorded as a certification/runtime blocker while
all locally unblocked work remains active.

## Entry 2026-07-28 — authenticated G11 HR employee transition

Added `HrService.TransitionHrEmployee` with explicit scope, `hr.write`
authorization, optimistic source-version binding, bounded termination date,
audit, and typed `EmployeeRecord` domain transitions. The tenant-write adapter
loads the authoritative row, rejects identity/version drift, and commits the
updated source row, outbox event, and idempotency receipt through the existing
atomic writer. The Dioxus HR surface exposes the bounded command and refreshes
the server-owned read model after success.

PASS: fresh disposable local PostgreSQL acceptance for Active → Suspended,
source status/version, two outbox events, and stale-version rejection. The
temporary database was dropped after the test. Provider/directory/device
certification and broader G11 workflow evidence remain queued; no activation,
owner switch, Folio write retirement, or production cutover occurred.

## Entry 2026-07-28 — G8 consolidated Finance trial balance

PASS: the new holding-level read contract validates explicit company charts
against the Finance chart projection, aggregates immutable posted journals by
account code through the domain, carries source version through SQLx, enforces
authenticated holding-scoped Finance reads, audits the query, and renders
typed server results in Dioxus. Intercompany elimination is explicit source
data, never an inferred client adjustment.

PASS: domain consolidation tests, formatter, proto/server checks, and focused
SQLx target compilation passed.

QUEUED: the ignored live consolidation acceptance fails closed because
`ERPX_TEST_TENANT_WRITE_DATABASE_URL` is unavailable (the read URL is therefore
not reached). FX/ownership/elimination rules, broader G8 reconciliation,
vendor certification, physical mobile execution, and production approval are
pending. No activation, owner switch, Folio write retirement, or production
cutover occurred.

The current-tree aggregate ignored rerun also exits 101 with 66 missing-
resource targets. `hr_persistence_envelope` is included and correctly
reports the unavailable tenant-write URL; its transition acceptance was
independently run against a disposable local database and passed. No
external directory/provider environment was activated.

## Entry 2026-07-28 — final local verification after G8/G11 UI packets

PASS: serialized `cargo fmt --all --check`, workspace warnings-denied Clippy,
and non-ignored workspace tests (including 83 console tests); protobuf and
G13/G14 structural checks; Dioxus web/desktop/Android/iOS checks; Folio
`bun run lint`; `bunx tsc --noEmit`; and the port-3004 root/protected-route
health guard. The current aggregate ignored matrix exits 101 with 66
fail-closed missing-resource targets. No production Next.js build, directory
activation, owner switch, Folio write retirement, or cutover occurred.

## Entry 2026-07-28 — G8 bank-provider import status UI

Added the Finance console status panel for the authenticated
`GetFinanceBankProviderImportJobStatus` read. It requires an explicit Finance
scope and opaque job UUID, verifies exact tenant/holding containment at the
wire boundary, and displays only bounded lifecycle metadata. The new console
test rejects scope escape and confirms no credential/provider-response fields
enter the client status model. Provider credentials/SDK, live vendor
acceptance, and broader Finance workflow evidence remain queued; no activation
or cutover occurred.

## Entry 2026-07-28 — local G14 directory SQLx and RBAC evidence

PASS: fresh disposable PostgreSQL acceptance covered Generic LDAP full
import/resume and immutable identity linking, AD DS DirSync disable and
revocation handling, UCS Provisioning Service delete handling, Generic LDAP
RFC 4533 cursor replay with role grant/revoke, projection, and outbox
persistence. Scope-containment acceptance rejected a platform mapping for a
holding connector with no mapping or outbox side effect, then persisted the
contained mapping. The independent directory-RBAC authority target passed
the persisted connector-scope rejection and valid mapping/outbox assertions.
All temporary databases were removed.

This is local SQLx evidence, not vendor certification. Credentialed Generic
LDAP/RFC 4533, on-premises AD DS/DirSync, and UCS Provisioning Service runs,
measured one-million-user/five-minute evidence, physical mobile execution,
approved descriptor comparison, and live Folio migration/rollback/cutover
remain queued. No activation, ownership switch, Folio write retirement, or
cutover occurred.

The directory SQLx fixture was rerun with a real platform connector and a
platform-owned holding role mapping; platform scope round-tripped and the
contained mapping committed. The deterministic directory volume suite passed
5/5 for one-million-user bounded import/checkpoint restart replay, native
LDAP/AD/UCS cursor continuity, exact five-minute revocation deadlines,
append-only role projection, and local-only break-glass/high-risk MFA. These
do not replace live vendor scale/revocation, mobile, or production evidence.

## Entry 2026-07-28 — serialized verification after platform/holding packet

PASS: `cargo fmt --all --check`; workspace Clippy with `-D warnings`;
non-ignored workspace tests; protobuf package and G13 compatibility checks;
Dioxus web/desktop/mobile/Android/iOS checks; Folio `bun run lint` and
`bunx tsc --noEmit`; `git diff --check`; and port-3004 health (`/` 200,
protected `/operations` 307). The aggregate ignored command was rerun and
exited 101 with 66 explicit missing-resource targets, including disposable
database URLs, MinIO/PITR credentials, credentialed LDAP/AD DS/UCS/provider
references, and the approved descriptor baseline. The failures are recorded
as queued blockers, not completion evidence. No activation, owner switch,
Folio write retirement, or cutover occurred.

## Entry 2026-07-28 — authoritative Operations SQLx revalidation

PASS: fresh disposable tenant write/read PostgreSQL pairs ran four Operations
persistence tests and two authoritative completion tests. Evidence covered
versioned/idempotent write/read boundaries, source-referenced aggregate replay,
exception lifecycle, scheduled detection with source event/outbox/read-model
state, authoritative source reconciliation with scope isolation, and stable
authority-bound scheduler requests. All temporary databases were removed.
Full source-domain breadth, restart/lease telemetry, Folio reconciliation,
external certification, measured external scale/revocation, and production
approval remain queued; no activation or cutover occurred.

## Entry 2026-07-28 — Finance bank import and tenant SQLx revalidation

PASS: fresh disposable PostgreSQL evidence covered the Finance bank-provider
worker job target 1/1 and tenant banking persistence 2/2. The worker exercised
claim fencing, retry/reclaim, reconciled completion, rollback evidence, and
status-only projection; tenant persistence exercised idempotent scoped bank
writes, source/outbox reconciliation, typed matching, and read-projection
rebuild/replay. Ignored tenant fixtures now use unique holding references per
test to prevent shared-database source-page contamination; this is test
isolation only. All temporary databases were removed. Provider SDK/live
vendor and broader Finance evidence remain queued.

## Entry 2026-07-28 — final current-tree verification after Finance isolation fix

PASS: after the Finance fixture isolation fix, `cargo fmt --all --check`,
warnings-denied workspace Clippy, non-ignored workspace tests, protobuf tests,
G13 compatibility 9/9, G14 structural acceptance 8/8, Dioxus web/desktop/
mobile/Android/iOS checks, Folio lint/typecheck, `git diff --check`, and
port-3004 root/protected health (`200`/`307`) remained green. The aggregate
ignored command still exits 101 at 66 explicit missing-resource targets for
disposable URLs, MinIO/PITR, credentialed directory/vendor environments, and
the approved descriptor baseline. Those are queued blockers, not completion
evidence; no activation, owner switch, Folio write retirement, or cutover
occurred.

## Entry 2026-07-28 — G10 CRM explicit holding scope provider

The CRM console now collects an opaque tenant reference, holding route
reference, holding UUID, and optional company/branch UUIDs before activating the existing authenticated
`ListCrmReadRows` resource. UUIDs are canonicalized; empty/control-character
and credential-marker values fail closed; no CRM business-data request is
made without a selected scope. Server-provided bounded cursors are carried by
the next-page control and reset when route or scope changes. Customer
containment, role/entitlement checks, audit context, tenant routing, and
projection authority remain on the server.

Evidence: `cargo fmt --all --check`; focused
`cargo test -p erpx-console --offline crm_holding_scope -- --nocapture` — PASS;
`cargo test -p erpx-console --offline
crm_response_preserves_bounded_cursor_continuation -- --nocapture` — PASS;
desktop console check — PASS. Projection freshness, remaining CRM/Marketing
adapters and journeys, Finance attribution
reconciliation, vendor/device/live production evidence, and G10/G12/G14 gates
remain open. No activation, owner switch, Folio write retirement, or cutover
occurred.

## Entry 2026-07-28 — CRM scope/cursor current-tree verification

PASS: G10 CRM explicit tenant/holding/company/branch scope, fail-closed
reference validation, authenticated read activation, and bounded server cursor
continuation are covered by the current console implementation. The complete
console package passed 85/85 tests, including explicit scope and cursor tests.

PASS: serialized `cargo fmt --all --check`, workspace Clippy with warnings
denied, non-ignored workspace tests, protobuf tests, G13 compatibility 9/9,
G14 structural acceptance 8/8, Dioxus web/desktop/host-mobile/iOS/Android
checks, Folio Bun lint/typecheck, `git diff --check`, branch validation, and
port-3004 root/protected health (`200`/`307`). Bun lint has four pre-existing
n8n warnings and zero errors. No temporary databases remain. The aggregate
ignored database suite remains queued at explicit missing-resource and vendor
certification targets; no activation, owner switch, Folio write retirement,
or cutover occurred.

## Entry 2026-07-28 — G8 Finance command boundary

PASS: the console Finance surface now submits bounded authenticated
`SaveFinancePeriod` and `PostFinanceJournal` requests. Period JSON is
server-compatible and scope-bound; journal JSON carries a generated domain
command context with the same command ID/idempotency key/actor/scope as the
transport context and two balanced immutable lines. Typed receipts, replay
state, correlation IDs, categorical errors, and read refresh are handled by
the console. Existing server-side finance.write authorization, SQLx
persistence, audit/outbox publication, and read projection remain the source
of truth. Console package tests passed 87/87. Remaining G8 provider, close,
AP/AR, SoD, tax, reconciliation, live certification, and production approval
evidence is still queued; no activation or cutover occurred.

## Entry 2026-07-28 — G8 Finance command current-tree verification

PASS: current-tree `cargo fmt --all --check`, workspace Clippy with warnings
denied, complete non-ignored workspace tests, protobuf package tests, G13
compatibility/registration 9/9, G14 directory acceptance 8/8, Dioxus
web/desktop/host-mobile/iOS/Android checks, Folio `bun run lint` (zero errors,
four pre-existing n8n warnings), `bunx tsc --noEmit`, `git diff --check`, and
port-3004 root/protected health (`200`/`307`). The required aggregate ignored
workspace run was rerun and exited 101 at 66 explicit missing-resource and
vendor-certification targets (database URLs, MinIO/PITR, directory/provider
environments, and approved protobuf descriptor baseline). No temporary
databases remain; queued failures are not completion evidence. No activation,
owner switch, Folio write retirement, or cutover occurred.

## Entry 2026-07-28 — G8 Finance close-review command boundary

PASS: the Finance console submits bounded authenticated
`SaveFinanceCloseReview` requests. It selects open/review/closed lifecycle
status, injects the matching domain context into the corresponding embedded
review context, binds expected versions, and displays typed receipts/status/
correlation metadata while refreshing the authoritative close projection.
Console package tests passed 89/89. Existing server period-lock,
reviewer-separation, scope/RBAC, SQLx, audit/outbox, and idempotency behavior
remain authoritative. Close fact/workflow/live database/provider and broader
Finance evidence remain queued; no activation or cutover occurred.

## Entry 2026-07-28 — G8 Finance chart command boundary

PASS: the Finance console also submits bounded authenticated
`SaveFinanceChart` requests. Client-supplied `last_command` facts are replaced
with server-matching command ID, idempotency key, actor, and correlation facts;
expected-version handling and typed receipt/event/correlation rendering are
covered. The current console package passes 88/88 tests, including explicit
holding binding and command-fact replacement. Existing server chart validation,
finance.write authorization, SQLx persistence, audit/outbox publication, and
idempotency remain authoritative. Remaining Finance workflows/provider
certification/reconciliation and production approval evidence remain queued;
no activation or cutover occurred.

## Entry 2026-07-28 — chart-inclusive current-tree verification

PASS: chart-inclusive `cargo fmt --all --check`, warnings-denied workspace
Clippy, complete non-ignored workspace tests including 88 console tests,
protobuf tests, G13 compatibility/registration 9/9, G14 directory acceptance
8/8, all Dioxus web/desktop/host-mobile/iOS/Android checks, Folio lint/type
check, `git diff --check`, and port-3004 root/protected health (`200`/`307`).
The exact aggregate ignored rerun exits 101 at 66 explicit missing-resource
and certification targets, including disposable PostgreSQL URLs, MinIO/PITR,
directory/provider environments, and the approved protobuf descriptor
baseline. No temporary databases remain; no activation, owner switch, Folio
write retirement, or cutover occurred.

## Entry 2026-07-28 — G11 HR leave/expense/payroll command boundary

PASS: the Dioxus HR employee context submits bounded authenticated
`SaveHrLeave`, `SaveHrExpense`, and `SaveHrPayroll` requests through the
existing server RPCs. The console derives company/branch tenant scope from
the selected employee scope, binds positive expected versions, injects the
matching immutable leave command ID, and renders typed receipts/replay/version
/correlation state. Server-side HR domain validation, RBAC, SQLx persistence,
outbox/audit, worker projection, idempotency, and reconciliation remain the
authority. Focused console coverage passes 90/90, including branch scope and
unsupported aggregate rejection. Broader HR/payroll workflows, live SQLx
evidence, provider/device integration, reconciliation, and production
approval remain queued; no activation or cutover occurred.

## Entry 2026-07-28 — current-tree verification after G11 HR packet

PASS: `cargo fmt --all --check`; workspace Clippy with `-D warnings`;
non-ignored workspace tests; protobuf tests; G13 compatibility/registration
(9/9); G14 directory acceptance (8/8); Dioxus console web/desktop/
host-mobile/iOS/Android and shared UI iOS/Android checks; Folio lint/typecheck;
and port-3004 root/protected health (`200`/`307`). Bun lint has zero errors
and four pre-existing n8n warnings. The exact aggregate ignored workspace
run exits 101 at 66 explicit missing-resource/certification targets for
disposable URLs, MinIO/PITR, credentialed directory/vendor environments, and
the approved protobuf descriptor baseline. Those remain queued blockers, not
completion evidence. No temporary databases, activation, owner switch, Folio
write retirement, or cutover occurred.

## Entry 2026-07-28 — G9 Asset/Maintenance command boundary

PASS: the Maintenance context now submits bounded authenticated
`SaveAssetMaintenanceAggregate` requests through the existing server RPC.
The console binds the selected holding scope, generates an immutable
idempotency key, carries an optional positive expected version, validates the
typed receipt, and refreshes the authoritative Asset/Maintenance read model
after success. Server-side aggregate validation, asset_maintenance.write
authorization, SQLx persistence, audit/outbox publication, worker projection,
idempotency, reconciliation, and rollback remain authoritative. Console
coverage passes 97/97, including malformed payload, zero-version, context,
scope, idempotency, and receipt assertions. Broader maintenance lifecycle,
reliability, accounting, device, reconciliation, live SQLx, and production
approval evidence remains queued; no activation or cutover occurred.

## Entry 2026-07-28 — current-tree verification after Asset/Maintenance

PASS: `cargo fmt --all --check`, `git diff --check`, warnings-denied
workspace Clippy, complete non-ignored workspace tests, `cargo test
-p erpx-proto --offline`, G13 compatibility/registration 9/9, G14 directory
acceptance 8/8, console Dioxus web/desktop/mobile/iOS/Android checks, shared
UI native iOS/Android checks, Folio `bun run lint` (zero errors, four
pre-existing n8n warnings), `bunx tsc --noEmit`, and port-3004 root/protected
health (`200`/`307`). The required
`cargo test --workspace --offline --no-fail-fast --quiet -- --ignored` sweep
exits 101 at exactly 66 explicit missing-resource/certification targets for
disposable PostgreSQL URLs, MinIO/PITR credentials, credentialed
LDAP/AD DS/UCS/provider environments, and the approved protobuf descriptor
baseline. These are queued blockers, not completion evidence. No temporary
databases, activation, owner switch, Folio write retirement, or production
cutover occurred.

## Entry 2026-07-28 — G10 CRM command boundary

PASS: the Marketing & CRM console now submits the existing authoritative
`SaveCrmHandoff`, `SaveCrmOpportunity`, and `RecordCrmAttribution` commands.
Explicit holding/company/branch scope is carried into the authenticated
command context; bounded handoff/opportunity JSON has its client command ID
replaced; opportunity and attribution versions are optimistic and positive;
attribution models are allow-listed; typed receipts, replay/version/reference
metadata, categorical errors, and authoritative read refresh are implemented.
Server CRM validation, crm.write authorization/entitlement, SQLx persistence,
audit/outbox, worker/read projection, idempotency, attribution, and
reconciliation remain the authority. Console coverage passes 98/98,
including scope/identity binding, receipt decoding, malformed JSON, model,
UUID, and zero-version checks. Broader Marketing journeys, consent/delivery/
provider/device workflows, live SQLx/provider evidence, Finance attribution
reconciliation, and production approval remain queued; no activation or
cutover occurred.

## Entry 2026-07-28 — current-tree verification after G10 CRM

PASS: `cargo fmt --all --check`, `git diff --check`, warnings-denied
workspace Clippy, complete non-ignored workspace tests including 98 console
tests, `cargo test -p erpx-proto --offline`, G13 compatibility/registration
9/9, G14 directory acceptance 8/8, console Dioxus web/desktop/mobile/iOS/
Android checks, shared UI native iOS/Android checks, Folio `bun run lint`
(zero errors, four pre-existing n8n warnings), `bunx tsc --noEmit`, and
port-3004 root/protected health (`200`/`307`). The required no-fail-fast
ignored workspace sweep exits 101 at exactly 66 explicit missing-resource/
certification targets for disposable PostgreSQL URLs, MinIO/PITR credentials,
credentialed directory/vendor environments, and the approved protobuf
descriptor baseline. These are queued blockers, not completion evidence. No
temporary databases, activation, owner switch, Folio write retirement, or
production cutover occurred.

## Entry 2026-07-28 — G8 Finance allocation batch command boundary

PASS: the Finance console now submits the existing authoritative
`SaveAllocationBatch` RPC with bounded payment/invoice/allocation JSON and a
required positive expected payment version. The boundary binds the explicit
holding scope, replaces nested record command identity/idempotency/context
facts with the authenticated command context, and decodes typed payment,
invoice, and allocation receipts with replay and correlation metadata. The
server remains authoritative for balanced allocation validation, finance.write
authorization/entitlement, atomic SQLx persistence, audit/outbox, worker/read
projection, idempotency, reconciliation, and rollback. Console coverage is
99/99, including scope/version/context binding, forged-identity replacement,
typed multi-receipts, and missing-version rejection. Broader Finance
provider/AP/AR/journal/close/tax/statement/reconciliation/live SQLx evidence
remains queued; no activation or cutover occurred.

## Entry 2026-07-28 — G8 Finance allocation current-tree verification

PASS: focused console tests pass 99/99 after the allocation boundary. The
serialized full workspace verification and exact aggregate ignored sweep are
queued for the current tree; the known ignored sweep remains 66 explicit
missing-resource/certification targets for disposable URLs, MinIO/PITR,
credentialed directory/vendor environments, and the approved protobuf
descriptor baseline. These are queued blockers, not completion evidence. No
temporary databases, activation, owner switch, Folio write retirement, or
production cutover occurred.

## Entry 2026-07-28 — serialized current-tree verification after G8/G11 packets

PASS: `cargo fmt --all --check`; warnings-denied workspace Clippy; complete
non-ignored workspace tests including 99 console tests; protobuf tests; and
Dioxus console/shared-UI web, desktop, mobile, iOS, and Android checks. Folio
`bun run lint` passed with zero errors and four pre-existing n8n warnings,
`bunx tsc --noEmit` passed, and port-3004 root/protected health was `200`/`307`
with the dev server still listening. The required ignored workspace sweep
completed with exactly 66 explicit missing-resource/certification targets for
disposable PostgreSQL URLs, MinIO/PITR credentials, local directory SQLx,
Generic LDAP/AD DS/UCS certification, and the protobuf descriptor baseline.
Those are queued blockers, not completion evidence. No temporary databases,
activation, owner switch, Folio write retirement, or production cutover
occurred.

## Entry 2026-07-28 — G10 Marketing erasure and G11 HR employee current-tree verification

PASS: Marketing/CRM profile erasure is now a bounded authenticated command
boundary over the existing authoritative RPC. The profile UUID, bounded
reason, optional RFC3339 timestamp, holding scope, privacy audit purpose, and
immutable command context are validated/replaced at the console boundary;
typed replay/correlation receipts and authoritative read refresh are exposed.
The HR employee lifecycle record is also admitted through the existing typed
HR command boundary with bounded JSON, explicit scope, optimistic version,
and authenticated command context. Authoritative SQLx, append-only audit/
outbox, worker/projector, authorization, reconciliation, and rollback remain
server-owned.

PASS: current-tree `cargo fmt --all --check`, warnings-denied workspace
Clippy, full non-ignored workspace tests (99 console tests), protobuf package
tests, G13 compatibility/registration 9/9, G13 deployment/recovery 4/4,
console Dioxus web/desktop/host-mobile/iOS/Android checks, shared UI iOS/
Android checks, Folio `bun run lint` (zero errors, four pre-existing n8n
warnings), `bunx tsc --noEmit`, and port-3004 health (`200`/`307`, listener
PID 24923) all passed. The required no-fail-fast ignored workspace sweep
exits 101 at exactly 66 missing-resource/certification targets. The approved
protobuf descriptor comparison remains queued without its approved baseline;
other queued targets require disposable PostgreSQL URLs, MinIO/PITR
credentials, local directory SQLx, and credentialed Generic LDAP/AD DS/UCS/
provider environments. These are blockers to certification, not evidence of
completion. No temporary databases, activation, owner switch, Folio write
retirement, or production cutover occurred.

## Entry 2026-07-28 — G11 HR complete read bundle current tree

PASS: the authenticated HR/Payroll route now consumes the existing typed
employee, leave, expense, payroll-run/line, and expired-role reassignment
read RPCs under one explicit tenant/holding/company/branch scope. The console
adapter preserves only bounded opaque/scalar facts and reconciled payroll
totals, validates scope containment, lifecycle/date/cursor/source metadata,
currency and amount bounds, and payroll line/aggregate reconciliation, and
renders explicit read-only sections. Credentials, accounts, tax, bank,
payment, attachment, raw payload, and identity-proof material remain outside
the client. Server authorization, customer audit, SQLx read projection,
outbox history, freshness, and rebuild remain authoritative.

Evidence: audit G11-UI-HR-CONTEXT-READ; console tests 100/100; package and
workspace warnings-denied Clippy pass; workspace non-ignored tests pass;
console web/desktop/host-mobile/iOS/Android and shared UI iOS/Android checks
pass. The latest ignored sweep remains exactly 66 queued
missing-resource/certification targets. No migration, server/worker
registration, activation, owner switch, Folio write retirement, or
production cutover occurred.

## Entry 2026-07-28 — coordinator local acceptance wave and G6 live round trip

PASS: real isolated PostgreSQL acceptance covered G6 backup/restore through a
temporary HTTPS MinIO endpoint using PostgreSQL 18 `pg_dump`/`pg_restore`,
candidate forced-RLS verification, inventory reconciliation, exact artifact
deletion, and database cleanup. G13 disposable migration/source-target/import
acceptance, G14 local directory/identity SQLx paths, G8 Finance journal,
banking, budget, chart, close, subledger, backfill, import-job, and candidate
activation paths, and G9–G11 product/worker/control-plane paths all passed in
focused runs with fresh databases per test.

PASS: temporary databases, TLS material, MinIO process, and MinIO data were
removed after the wave. No application schema, shared registration, server or
worker ownership, authorization policy, customer directory, Folio write path,
or production state changed.

QUEUED BLOCKER: G14 external certification remains fail-closed because the
credentialed Generic LDAP/RFC 4533, on-premises AD DS/DirSync, and UCS
Provisioning Service environments and evidence references are unavailable.
The approved protobuf descriptor baseline, measured one-million-user/five-
minute evidence, true PITR/WAL, physical mobile execution, and explicit
production Folio migration/cutover approval remain open. These are
certification blockers, not completion evidence.

## Entry 2026-07-28 — final serialized verification after local acceptance wave

PASS: `cargo fmt --all --check`, workspace warnings-denied Clippy, complete
non-ignored workspace tests, protobuf tests, G13 parallel 9/9, G13 deployment
recovery 4/4, G14 directory 8/8, Dioxus web/desktop/host-mobile/iOS/Android
checks, shared UI iOS/Android and Android bridge checks, Folio `bun run lint`
(zero errors, four pre-existing n8n warnings), and `bunx tsc --noEmit`.
The mandated ignored workspace sweep ran with `--test-threads=1` and exited
101 at exactly 66 explicit missing-resource/certification targets. Focused
real-database acceptance for the available targets passed separately with
fresh disposable resources.

PASS: Folio remained operational on port 3004 (`root=200`,
`/operations=307`, listener PID 24923). No production Next.js build ran.
No directory activation, command-owner switch, Folio write retirement, or
production cutover occurred.

## Entry 2026-07-30 — G10 owner notification delivery and G12 route safety

PASS: G10 `NotifyOwner` now atomically creates a deterministic and idempotent
Webhook delivery reservation beside the journey cursor, outbox, and source
notification. Delivery leases, retries, provider binding, projection, and
replay self-healing are authoritative; the read seam is holding-scoped and
the strict internal-owner marker is the only consent bypass. Focused
infrastructure and worker tests passed.

PASS: G12 `RouteState::return_to()` now rejects oversized, external, encoded,
whitespace, and ambiguous routes while preserving only bounded app-relative
mobile-safe paths. UI evidence passed with 104 unit tests plus integration
tests, including the existing redaction, MFA, offline, stale, error, and
no-local-fallback contracts.

QUEUED: G10 live tenant SQLx/provider delivery certification and credentials,
G12 Android/iOS secure-storage device certification, external directory
certification, approved protobuf provenance, physical recovery/scale
evidence, and production authorization remain unavailable. No directory
activation, command-owner switch, Folio write retirement, or production
cutover occurred.

## Entry 2026-07-30 — G7/G8/G9 additive completion packets

PASS: G7 Operations reconciliation now requires an exact holding-bound
baseline. Global, unscoped, and cross-holding baselines are rejected before
mutation; the worker packet's focused tests passed 22/22.

PASS: generic G9 Sales and SCM saves now enforce the authoritative command
binding across domain, infrastructure, server, and console transport:
containing scope, exact command ID, deterministic idempotency, and nested
relation scope. Sales persistence focused tests passed 9/9; the domain suite
passed 293/293 before the packet and focused save-boundary tests passed 4/4.

PASS: G8 Finance bank-provider import enqueue is now a real SQLx-backed
source boundary. It persists only job metadata plus append-only outbox/audit
evidence, requires fresh MFA and directory-managed finance authorization,
binds the exact holding-scoped route/account, and uses explicit fail-closed
provider/credential adapters. Finance domain consolidation now validates FX,
ownership, and elimination inputs; focused Finance tests passed 46/46.

QUEUED: live bank/provider, Generic LDAP/RFC 4533, AD DS/DirSync, UCS
Provisioning Service, approved protobuf provenance, physical recovery/device/
scale evidence, and production authorization remain unavailable. These
packets do not activate directories, change command ownership, retire Folio
writes, or authorize production cutover.

## Entry 2026-07-30 — Finance bank-provider import runtime

The coordinator closed the local G8 composition gap. Console → Finance gRPC →
SQLx job source → worker registration is now connected. The server performs
authenticated actor/scope and directory/local role checks, fresh MFA and
high-risk Finance authorization, exact active route binding, and source-fence
capture. Job insert, audit fact, and queued outbox intent share one SQLx
transaction; no credentials or provider payloads enter persistence. The worker
rechecks authorization and route state before opening the explicitly injected
credential and provider adapter.

Evidence: affected packages compile; Finance worker unit tests 7/7; server
status-boundary test 1/1; console Finance banking tests 3/3; focused
warnings-denied Clippy passed. The executable default remains fail-closed with
an unconfigured credential resolver and empty provider registry because no
credentialed bank environment or approved concrete adapter is available.

QUEUED: live bank-provider/credential certification, import scale and recovery
rehearsal, external directory certification, approved protobuf provenance,
physical recovery/device/scale evidence, live Stripe/catalog reconciliation,
and production approval. No activation, command-owner switch, Folio write
retirement, or production cutover occurred.

## Entry 2026-07-30 — final certification re-audit

PASS: the coordinator re-audited the current source and found no newly
available local implementation defect. The latest local evidence remains
green for the serialized Cargo gate, non-ignored workspace/doc-test suite,
protobuf compatibility, G13/G14 local acceptance, Dioxus target checks, Folio
lint/typecheck, diff hygiene, and port-3004 probes.

QUEUED: the exact ignored workspace sweep remains an 81-target fail-closed
resource/certification queue. Missing inputs are credentialed Generic
LDAP/RFC 4533, on-premises AD DS/DirSync, UCS Provisioning Service, approved
protobuf descriptor provenance, production TLS MinIO credentials/CA, physical
PITR/WAL and device/scale evidence, live Stripe/catalog credentials, and
explicit production authorization. No directory activation, command-owner
switch, Folio write retirement, or production cutover occurred.

## Entry 2026-07-28 — G4 Support and Incident command surface

PASS: the console Support and Incident routes now expose bounded create and
lifecycle-transition commands through the existing authenticated
`SupportService` RPCs. Ticket creation validates the customer UUID and
bounded description/severity; ticket transitions and incident transitions
carry positive expected aggregate versions; incident creation binds the
commander to the authenticated principal and validates bounded CSV fields.
Every request carries a platform command context with support audit module,
purpose, correlation ID, idempotency key, and session-bound actor identity.

PASS: server authorization, MFA/capability policy, scope, SQLx source state,
aggregate snapshots, idempotency, append-only outbox, projector/checkpoint,
reconciliation, and rollback remain authoritative in the pre-existing
Support application/server/infrastructure path. The console accepts only a
bounded receipt and refreshes the authenticated read resources; ticket bodies,
replies, internal notes, capability secrets, and incident timelines never
cross the display boundary. Focused console tests pass 102/102, strict
console Clippy passes, and console web/desktop/mobile/iOS/Android plus shared
UI iOS/Android checks pass. Evidence: `audits/G4-UI-SUPPORT-COMMANDS`.

This closes the local G4 command adapter only. External support/vendor,
disposable database certification, directory-provider certification, measured
scale/revocation, true PITR/WAL, physical mobile execution, and explicit
production migration/cutover approval remain queued. No activation, owner
switch, Folio write retirement, or production cutover occurred.

## Entry 2026-07-28 — G3 Customer 360 raw-payload redaction boundary

PASS: the authenticated Customer 360 detail adapter no longer retains or
renders the complete `payload_json` projection blob. It validates bounded
customer/freshness metadata and accepts only the closed account,
organization, contacts, commercial, entitlements, onboarding, health,
support, access-audit, and notes section states (`Present`, `Absent`, or
`Unsupported`). Unknown, oversized, malformed, or incomplete section payloads
fail closed. Existing authoritative customer scope, global-read audit, SQLx
read projection, cursor-bound notes, and reconciliation paths remain intact.

PASS: the UI renders section-state metadata plus the bounded notes read model;
arbitrary projection fields, credentials, provider material, and raw JSON do
not cross the display boundary. Focused console coverage is 103/103 and
warnings-denied console Clippy passes. Evidence:
`audits/G3-UI-CUSTOMER-360-REDACTION`.

PASS: the coordinator also tightened the transport boundary: the server now
derives typed `Customer360SectionState` values from the authoritative detail
read model and returns an empty legacy `payload_json` field. No aggregate JSON
is serialized into the Customer 360 response.

## Entry 2026-07-28 — G6 tenant owner bootstrap live acceptance

PASS: the ignored server owner-bootstrap test passed against fresh disposable
PostgreSQL control write/read databases. The transaction created the
`recovery_required` identity and exactly one recovery code, persisted a finite
holding-owner role with the independent approver binding, replayed without
issuing a second recovery code, and kept `tenant_owner_seeds` write-only (the
read database has no such table). The disposable databases were removed by an
exact-name cleanup trap after the test.

PASS: stale Platform Operations route descriptions were corrected to describe
the live authenticated integrations, settings, audit, backup, and projection
read adapters without implying that those adapters are absent. Credentials,
payloads, mutable settings, and operational commands remain excluded from the
console.

QUEUED BLOCKER: external directory/vendor certification, million-user and
five-minute measurements, approved protobuf descriptor comparison, true
PITR/WAL evidence, physical mobile execution, and explicit production
migration/cutover approval remain unavailable. No directory activation,
command-owner switch, Folio write retirement, or production cutover occurred.

PASS: the real `erpx-identity` directory-volume suite then processed the
one-million-user bounded import/checkpoint workload and passed all five
volume/replay/revocation cases in 51.74 seconds. This is local algorithmic
scale evidence only; it does not claim provider-side LDAP, AD DS, or UCS
transport certification or a production throughput SLA.

## Entry 2026-07-28 — final serialized verification after owner/scale evidence

PASS: after the owner-bootstrap and route-copy changes, `cargo fmt --all
--check`, warnings-denied workspace Clippy, the complete non-ignored workspace
suite (including 100 console tests and the five-case one-million-user volume
suite), all Dioxus web/desktop/mobile/iOS/Android checks, Folio `bun run lint`
(zero errors, four pre-existing n8n warnings), and `bunx tsc --noEmit` passed.

PASS: the mandated ignored workspace sweep ran with one test thread and
exited at exactly 66 explicit missing-resource/certification targets. The
failures are queued-resource assertions for absent disposable database URLs,
MinIO/PITR credentials, external Generic LDAP/AD DS/UCS environments, and
other configured evidence; they are not application assertion failures. The
owner-bootstrap target was separately run with fresh databases and passed.

PASS: Folio remained operational on port 3004 (`root=200`,
`/operations=307`, listener PID 24923). No production Next.js build ran.
No directory activation, command-owner switch, Folio write retirement, or
production cutover occurred.

## Entry 2026-07-28 — serialized verification after G4 Support command adapter

PASS: after the Support/Incident console command adapter and its boundary
tests, `cargo fmt --all --check`, workspace warnings-denied Clippy, the full
non-ignored workspace suite, protobuf tests, G13 compatibility/deployment
checks, G14 directory/readiness checks, and the Dioxus console/shared UI
target checks passed. The current console suite is 102/102.

PASS: the mandated `cargo test --workspace --offline --no-fail-fast --
--ignored --test-threads=1` sweep ran to completion and reported exactly 66
queued missing-resource/certification targets. These fail closed on absent
disposable SQLx URLs, MinIO/PITR credentials, external Generic LDAP/AD DS/UCS
references, or explicit protobuf/cutover evidence; no application assertion
failure was observed.

PASS: Folio `bun run lint` completed with zero errors and four pre-existing
n8n warnings, `bunx tsc --noEmit` passed, and Folio remained healthy on port
3004 (`root=200`, `/operations=307`, listener PID 24923). No production
Next.js build, directory activation, owner switch, Folio write retirement, or
production cutover occurred.

## Entry 2026-07-28 — G10 Marketing preview and custom-domain command surface

PASS: the shared Dioxus Marketing/CRM console now submits bounded,
authenticated `IssueMarketingPreview`, `RevokeMarketingPreview`,
`IssueMarketingDomainVerification`, `VerifyMarketingDomain`, and
`BindMarketingDomain` commands through the existing Marketing content
service. Requests are explicit-scope bound, UUID/version validated, rebuilt
with authenticated command context, and reduced to typed receipt metadata at
the display boundary. Preview tokens, verification challenges, raw form
values, persistence payloads, and provider credentials are not rendered or
retained by the client.

PASS: the server remains authoritative for entitlement/RBAC, publication and
verification state, tenant SQLx routing, hash-only secret persistence,
idempotency, audit/outbox, expiry worker, reconciliation, and rollback.
Focused console coverage is 104/104; strict console Clippy and the console,
shared-UI, web, desktop, host-mobile, iOS, Android, and Android bridge checks
pass. Evidence: `audits/G10-MKT-075/README.md`.

PASS: current-tree serialized formatting, warnings-denied workspace Clippy,
the complete non-ignored workspace tests, protobuf tests, G13 compatibility/
deployment checks, G14 directory/readiness checks, Folio lint/typecheck, and
port-3004 health all pass. Folio lint retains four pre-existing n8n warnings.

QUEUED: the mandated ignored workspace sweep exits 101 at exactly 66 explicit
missing-resource/certification targets, including disposable SQLx URLs,
MinIO/PITR resources, local directory SQLx evidence, credentialed Generic
LDAP/AD DS/UCS environments, and the approved protobuf descriptor baseline.
These are not completion evidence. No provider account activation, domain
binding, directory activation, command-owner switch, Folio write retirement,
or production cutover occurred.

## Entry 2026-07-28 — G10 Marketing provider account command surface

PASS: the authenticated Marketing Channels console now submits holding-bound
provider onboarding, verification acknowledgement, and active-state
commands through the existing Marketing content service. Configuration
accepts bounded enabled channels and credential-free HTTPS route metadata;
secret-manager references remain opaque. Verification and activation remain
server-policy gated, and suspension/reactivation uses the authoritative
optimistic control-plane command. Only safe provider state, onboarding state,
version, replay, and correlation metadata are rendered.

PASS: the server remains authoritative for provider scope, entitlement/RBAC,
channel/route validation, verification gating, SQLx persistence, audit/outbox,
worker account routing, idempotency, reconciliation, and rollback. Focused
console coverage is 105/105; strict console Clippy and all console/shared UI
web, desktop, host-mobile, iOS, Android, and bridge checks pass. Evidence:
`audits/G10-MKT-076/README.md`.

QUEUED: external provider verification and live provider/SQLx evidence remain
unavailable. No provider account was activated, no external provider was
contacted, and no directory activation, command-owner switch, Folio write
retirement, or production cutover occurred.

## Entry 2026-07-28 — current-tree serialized verification after G10-MKT-076

PASS: the current tree passed `cargo fmt --all --check`, workspace
warnings-denied Clippy, the complete non-ignored workspace suite, protobuf
tests, G13 compatibility/deployment checks, G14 directory/readiness checks,
and all available Dioxus web, desktop, host-mobile, iOS, Android, and
Android-bridge checks. Focused console coverage is 105/105.

PASS: Folio `bun run lint` completed with zero errors and four pre-existing
n8n warnings; `bunx tsc --noEmit` passed. Folio remained healthy on port 3004
(`root=200`, `/operations=307`, listener PID 24923). No production Next.js
build ran.

QUEUED: the mandated `cargo test --workspace --offline --no-fail-fast --
--ignored --test-threads=1` sweep exited 101 at exactly 66 explicit
missing-resource/certification targets. The queued targets fail closed on
absent disposable SQLx URLs, MinIO/PITR resources, credentialed Generic
LDAP/AD DS/UCS environments, the approved protobuf descriptor baseline, and
other external evidence; no application assertion failure was observed.
No provider or directory activation, command-owner switch, Folio write
retirement, or production cutover occurred.

## Entry 2026-07-28 — isolated local SQLx acceptance wave

PASS: fresh disposable PostgreSQL target runs covered identity and local
directory SQLx/RBAC, Support and attachment expiry, Customer 360/access audit,
role review, Finance banking/import/candidate/close/journal/chart/budget/
subledger, HR, Legal, Marketing/CRM/content/delivery/provider callback/
journeys/erasure/onboarding, Operations, POS, SCM, organization, provisioning,
replacement-readiness, G13 migration/target import, control-plane SQLx,
backup jobs/retention, registry, scheduler/status, and worker runtime paths.
The focused target results were clean after each target received isolated
write/read resources; deterministic duplicate-key failures from a reused
database are not cited as acceptance evidence.

PASS: the real PostgreSQL 18 + HTTPS TLS MinIO backup/restore acceptance
completed successfully. The resident HTTP-only MinIO service was intentionally
not used; the disposable TLS service and its bucket were removed after the
test. The one-million-user directory-volume suite remains local bounded
algorithmic evidence only and does not certify a real LDAP/AD/UCS provider.

QUEUED BLOCKERS: credentialed Generic LDAP/RFC 4533, AD DS/DirSync, and UCS
Provisioning Service environments; approved protobuf descriptor comparison;
true PITR/WAL evidence; physical mobile execution; and explicit production
migration/cutover approval. No activation, owner switch, Folio write
retirement, or production cutover was performed.

## Entry 2026-07-28 — G1 multi-tenant readiness enumeration

PASS: the server readiness adapter now enumerates the bounded active tenant
registry when no explicit diagnostic tenant is supplied. It probes every
active tenant through the existing live routing, projection freshness, and
backup/PITR readiness path and aggregates each component fail-closed. Empty,
over-limit, unavailable, or unprobed tenant sets cannot report ready; explicit
tenant targeting remains available for diagnostics. The public endpoint does
not expose database URLs, credentials, or registry references.

PASS: `cargo fmt --all --check` and `cargo test -p erpx-server --offline`
passed with 87 tests and 1 intentionally ignored database test; the two new readiness tests cover empty, ready,
not-probed, unavailable, and infrastructure-state mappings.

QUEUED: full workspace re-verification is still required after this packet.
Credentialed Generic LDAP/RFC 4533, AD DS/DirSync, and UCS Provisioning
Service certification, approved protobuf descriptor comparison, true PITR/WAL
evidence, physical mobile execution, and production approval remain queued.

## Entry 2026-07-28 — post-readiness serialized verification

PASS: current-tree `cargo fmt --all --check`, workspace warnings-denied
Clippy, complete non-ignored workspace tests, protobuf package tests, G13
compatibility/deployment tests, G14 directory tests, and all available
Dioxus web/desktop/mobile/iOS/Android/bridge checks passed. The local
one-million-user directory volume suite passed all five tests in 51.55s.

PASS: Folio lint and typecheck passed (`bun run lint` had zero errors and four
pre-existing n8n warnings; `bunx tsc --noEmit` passed). Port 3004 remains
healthy with root `200` and `/operations` `307`; no production Next build ran.

QUEUED: the mandated ignored workspace sweep exited 101 at exactly 66
resource-gated targets. Missing disposable SQLx/MinIO/PITR inputs,
credentialed Generic LDAP/AD DS/UCS environments, approved descriptor
baseline, true PITR/WAL, physical mobile execution, and explicit production
approval remain blockers. No activation, owner switch, Folio write
retirement, or production cutover occurred.

## Entry 2026-07-28 — G8 Thai statutory tax fact/report packet

PASS: explicit Thai tax facts are domain-validated, linked to immutable posted
journal lines, persisted with SQLx/idempotency/snapshot/outbox atomically,
projected to a payload-free tenant-read model, and registered with the durable
multi-tenant worker. The authenticated command enforces Finance entitlement and
scope; the report query enforces Finance read authorization and a fresh MFA
step-up. The console renders typed server totals/facts only. Evidence:
`audits/G8-THAI-STATUTORY-TAX-2026-07-28/README.md`.

PASS: focused domain, worker, proto, formatting, targeted Clippy, server,
worker, console, and Dioxus desktop verification.

QUEUED: live PostgreSQL projection acceptance requires
`ERPX_TEST_TENANT_WRITE_DATABASE_URL` and
`ERPX_TEST_TENANT_READ_DATABASE_URL`; the absent resources were recorded as a
certification blocker. Filing/export, consolidation, broader G8 reconciliation,
external directory/vendor evidence, mobile execution, and production approval
remain pending. No activation, owner switch, Folio write retirement, or
production cutover occurred.

## Entry 2026-07-28 — current-tree verification after G8 tax packet

PASS: serialized `cargo fmt --all --check`, workspace warnings-denied Clippy,
non-ignored workspace tests, protobuf package tests, G13 compatibility/deployment
checks, G14 directory checks, and all available Dioxus web/desktop/host-mobile/
iOS/Android/bridge checks completed successfully. Focused Thai tax domain,
worker, SQLx target compilation, server, console, and proto checks passed.

PASS: Folio `bun run lint` and `bunx tsc --noEmit` passed. The dev server remains
on port 3004 with root `200` and `/operations` `307`; no production build ran.

QUEUED: the required ignored sweep remains environment-gated and exited 101 at
exactly 67 targets, now including the Thai tax projection target. Missing
disposable SQLx/MinIO/PITR resources, credentialed Generic LDAP/AD DS/UCS
environments, approved descriptor baseline, true PITR/WAL, physical mobile
execution, and explicit production approval remain queued. No activation,
owner switch, Folio write retirement, or production cutover occurred.

## Entry 2026-07-28 — current-tree verification after G8 consolidation

PASS: serialized workspace warnings-denied Clippy and non-ignored workspace
tests passed. Formatting, protobuf package tests, the consolidation domain and
SQLx-target checks, server/infrastructure/console checks, and all available
Dioxus web/desktop/host-mobile/iOS/Android/bridge checks passed.

PASS: Folio lint and typecheck passed with the existing four n8n warnings and
zero errors. Port 3004 remains healthy with root `200` and `/operations` `307`;
no production build ran.

QUEUED: the mandated ignored sweep exited 101 at exactly 68 environment-gated
targets, including the Thai tax and consolidated Finance acceptances. Missing
SQLx/MinIO/PITR resources, credentialed Generic LDAP/AD DS/UCS environments,
approved descriptor baseline, true PITR/WAL, physical mobile execution, and
explicit production approval remain queued. No activation, owner switch, Folio
write retirement, or production cutover occurred.

## Entry 2026-07-28 — G10-MKT-077 delivery projection runtime and RFC 4533 continuation

PASS: the tenant-read Marketing delivery projection now has a durable ordered
cursor, version-fenced replay, disposable read-model reset, and source/read
reconciliation. The shipped multi-tenant Marketing delivery worker invokes the
projection before provider claims, while authenticated callback routing,
durable inbox idempotency, callback transition events, and account-bound
provider execution remain authoritative. Evidence:
`audits/G10-MKT-077/README.md`.

PASS: the directory full-import continuation guard covers LDAP polling, AD
DirSync, RFC 4533, and UCS Provisioning Service with cookie and one-million-user
stop-condition tests. Focused formatting, infrastructure/worker compilation,
tests, and warnings-denied Clippy passed.

QUEUED: the new disposable two-database SQLx projection acceptance fails closed
without `ERPX_TEST_TENANT_WRITE_DATABASE_URL` and
`ERPX_TEST_TENANT_READ_DATABASE_URL`. External directory/provider certification,
approved protobuf descriptor comparison, true PITR/WAL, physical mobile
execution, and explicit migration/cutover approval remain queued. No provider
or directory activation, owner switch, Folio write retirement, or production
cutover occurred.

## Entry 2026-07-28 — current-tree serialized verification after G10-MKT-077

PASS: `cargo fmt --all --check`, workspace warnings-denied Clippy, and the
complete non-ignored workspace tests passed. G13 parallel acceptance passed
9/9; protobuf package tests passed; console/shared UI Dioxus web, desktop,
mobile, iOS, Android, and Android-bridge checks passed. The directory volume
proof passed all five tests, including its one-million-user bounded replay.

PASS: Folio `bun run lint` had zero errors and four pre-existing n8n warnings;
`bunx tsc --noEmit` passed. Port 3004 remained healthy with root `200` and
`/operations` `307`; no production Next.js build ran.

QUEUED: the mandated ignored workspace sweep exited 101 at exactly 68
environment-gated targets, including the new delivery projection acceptance.
The failures are the recorded missing disposable SQLx/MinIO/PITR resources,
credentialed Generic LDAP/AD DS/UCS/provider environments, approved descriptor
baseline, or other certification inputs; no application assertion failure was
observed. Physical mobile execution, true PITR/WAL, and explicit migration /
cutover approval remain queued. No activation, owner switch, Folio write
retirement, or production cutover occurred.

## Entry 2026-07-28 — G10 delivery read transport and Marketing Channels surface

PASS: `GetMarketingDeliveryRead` is now an authenticated, tenant/holding/
caller-scope-bound read RPC over the authoritative delivery read model. The
response is typed and payload-free: state, attempts, retry/expiry timestamps,
immutable version, read-only, and presence only. The Marketing Channels route
renders this contract and never accepts or displays recipient payloads,
provider credentials, or outbound content. Console tests passed (106 total).

QUEUED: final resource-backed SQLx/provider acceptance, external directory and
provider certification, approved protobuf descriptor comparison, true PITR/WAL,
physical mobile execution, and explicit migration/cutover approval remain
queued. No activation, owner switch, Folio write retirement, or production
cutover occurred.

## Entry 2026-07-28 — final current-tree verification after delivery read UI

PASS: `cargo fmt --all --check`; workspace warnings-denied Clippy; complete
non-ignored workspace tests; `erpx-proto` package tests; G13 compatibility /
registration 9/9; and all Dioxus console/shared-UI web, desktop, host-mobile,
iOS, Android, and Android-bridge checks. Console coverage is 106/106 and the
one-million-user directory volume suite passes.

PASS: Folio `bun run lint` has zero errors and four pre-existing n8n warnings;
`bunx tsc --noEmit` passes. Port 3004 remains healthy (`root=200`,
`/operations=307`, PID 24923); no production Next.js build ran.

QUEUED: the required ignored sweep exits 101 at exactly 68 environment-gated
targets, all attributable to the recorded missing disposable SQLx/MinIO/PITR
resources, credentialed Generic LDAP/AD DS/UCS/provider inputs, approved
descriptor baseline, or other certification inputs. No application assertion
failure was observed. Physical mobile execution, true PITR/WAL, and explicit
migration/cutover approval remain queued. No activation, owner switch, Folio
write retirement, or production cutover occurred; G0–G14 are not claimed
complete.

## Entry 2026-07-28 — fresh local acceptance refresh

PASS: fresh exact-name PostgreSQL runs covered identity SQLx/RBAC, all local
Finance persistence/consolidation paths, Marketing delivery/provider/scheduler,
Sales, SCM, Asset/Maintenance, provisioning, control-plane, support,
role-review, worker recovery, server owner seeding, and both G13 disposable
import/rollback suites. The consolidated Finance acceptance now persists the
authoritative company charts before posting and passes balanced two-company
aggregation. The G6 backup acceptance passed through a temporary HTTPS MinIO
instance using PostgreSQL 18 `pg_dump`/`pg_restore`.

PASS: all exact disposable databases and temporary MinIO resources created for
this refresh were removed after verification. No application, Folio, customer,
directory, provider, ownership, or production state was changed.

QUEUED: only the deliberate G14 external-readiness preflight remains failed in
the local acceptance sweep, requiring credentialed Generic LDAP/RFC 4533,
on-premises AD DS/DirSync, and UCS Provisioning Service environments plus
operator revocation evidence. The approved protobuf descriptor baseline, true
PITR/WAL, and explicit production migration/cutover approval remain queued.

## Entry 2026-07-28 — serialized final verification refresh

PASS: serialized `cargo fmt --all --check`, workspace warnings-denied Clippy,
the complete non-ignored workspace tests, `erpx-proto`, G13 parallel and
deployment-recovery suites, and all Dioxus web/desktop/host-mobile/iOS/
Android/shared-UI/Android-bridge checks. The console suite passed 106/106 and
the directory volume suite passed all five bounded one-million-user tests.

PASS: Folio `bun run lint` had zero errors and four pre-existing n8n warnings;
`bunx tsc --noEmit` passed; port 3004 returned `root=200` and
`/operations=307`. No production Next.js build ran.

QUEUED: the mandated exact ignored workspace command exited 101 with exactly
68 environment-gated targets. The failures were missing disposable SQLx/
MinIO/PITR variables, missing approved protobuf descriptor input, or missing
credentialed external directory/provider/cutover evidence. Fresh isolated
resource-backed local reruns passed their corresponding targets separately;
no application assertion failure occurred in the no-resource sweep. G0–G14
are not claimed complete, and no activation, owner switch, Folio write
retirement, or production cutover occurred.

## Entry 2026-07-28 — final current-tree coordinator record

PASS: API-started directory imports now persist an authoritative import row and
`directory.import_started` outbox handoff under a connector advisory lock. The
native worker resumes the persisted import ID/configuration and enforces the
one-million-user bound; fresh local SQLx handoff evidence passed and legacy
`directory_import_jobs` was not used. Directory auth/feed/RBAC evidence covers
TLS-only, MFA high-risk access, local-only break-glass, immutable identity
links, scoped role mappings, five-minute revocation, AD DirSync, RFC 4533, and
UCS Provisioning Service contracts.

PASS: the current integrated packets cover G7 Operations, G8 Finance tax and
consolidation, G9 Sales/SCM worker reconciliation, G10 Marketing delivery
reconciliation/provider response binding/read transport, G11 Quality rollback
validation, G12 platform UI adapters, and G13 import/read/rollback readiness.
The serialized non-ignored workspace tests, workspace Clippy with warnings
denied, protobuf tests, all required Dioxus checks, `bun run lint`,
`bunx tsc --noEmit`, and Folio port-3004 health passed. Lint retained only the
four pre-existing n8n warnings.

QUEUED: `cargo test --workspace --offline --no-fail-fast -- --ignored
--test-threads=1` exited 101 with exactly 68 environment-gated targets. No
application assertion failure appeared in that no-resource sweep; the
available disposable G13 targets passed. Credentialed external directory
certification, disposable SQLx/MinIO/PITR inputs, approved protobuf descriptor
comparison, physical mobile execution, true PITR/WAL, and explicit production
migration/cutover approval remain unavailable. Goal is not 100% complete; no
activation, owner switch, Folio write retirement, or production cutover was
performed.

## Entry 2026-07-28 — final parallel-lane evidence refresh

PASS: identity lane evidence covers TLS-only Generic LDAP/AD DS/UCS
authentication, immutable connector-scoped identity links, native RFC 4533 /
AD DirSync / UCS feed contracts, one-million-user bounded imports, scoped
directory RBAC, append-only role revocation, five-minute disable/delete
deadlines, MFA high-risk gates, and local-only ticket-bound break-glass. The
G10 Marketing lane added contiguous source-envelope validation and a SQLx
missing-intermediate-event reconciliation acceptance. The G8 Finance lane
added atomic allocation/tax/projection rollback and authoritative journal
binding. G6/G7 added HTTPS object storage, fenced backup/retention/PITR
boundaries, and authoritative operational outbox validation.

PASS: the Folio bridge now has an authenticated read-only status API and UI
surface with bounded reconciliation state and fail-closed support/holding
scope checks. Directory-managed approver/security mappings are evaluated by
the coordinator-owned shared authorization path. Focused Folio bridge tests,
TypeScript, lint, full non-ignored Cargo tests, warnings-denied Clippy, all
Dioxus checks, protobuf/local G14 checks, and port-3004 probes passed.

QUEUED: the mandated ignored Cargo sweep exited 101 at exactly 68
environment-gated targets. Missing live vendor credentials/endpoints, SQLx /
MinIO/PITR resources, approved descriptor baseline, physical mobile execution,
true WAL/PITR proof, and explicit production migration/cutover approval are
recorded blockers. No production activation, command-owner switch, Folio write
retirement, or cutover was performed.

## Entry 2026-07-28 — current-turn verification and Operations reconciliation

PASS: fresh local resource-backed runs passed identity directory SQLx 3/3,
directory RBAC 1/1, Operations completion 2/2, Operations projection 1/1,
provisioning 4/4, control-plane SQLx 3/3, and real PostgreSQL 18 plus
CA-signed HTTPS MinIO backup/restore 1/1. The Operations reconciliation query
now ignores upstream source outbox rows when checking the Operations-owned
outbox, preserving authoritative source-feed reconciliation; the scheduler
fixture uses a past detection instant while the SQLx lease remains database-
clock fenced.

PASS: `cargo fmt --all --check`, warnings-denied workspace Clippy, the full
non-ignored workspace tests, protobuf tests, G13 compatibility 9/9 and
recovery 4/4, G14 local compatibility 5/5, shared UI tests (90), all console
web/desktop/mobile/iOS/Android checks, Android bridge checks, Folio lint and
TypeScript, bridge tests 5/5, and port-3004 probes (`200`/`307`) passed.

QUEUED: the full ignored invocation reported five targets in its shared
database aggregate. Fresh exact-input reruns passed the four local targets;
only `g14_external_readiness` remains failed because credentialed Generic
LDAP/RFC 4533, AD DS/DirSync, and UCS Provisioning Service environments plus
operator revocation evidence are unavailable. The approved protobuf
descriptor baseline, true PITR/WAL, physical mobile execution, and explicit
production migration/cutover approval remain queued. All exact disposable
databases and temporary MinIO resources were removed. No activation, owner
switch, Folio write retirement, or production cutover occurred; G0–G14 remain
open.

## Entry 2026-07-28 — coordinator final current-tree evidence

PASS: live LDAP now accepts an explicitly bounded private CA bundle through
`ERPX_DIRECTORY_CA_CERT_PATH`, parses one or more PEM certificates into the
normal Rustls root store, and fails closed on malformed/empty/oversized input.
The existing TLS hostname and verified-peer fingerprint checks remain in force;
no insecure verifier or certificate bypass was added. Focused worker directory
tests passed 13/13.

PASS: fresh SQLx-backed local evidence passed identity directory 3/3 plus RBAC
1/1, G3 Customer 360 1/1, G5 commercial 8/8, G7 Operations 5/5, G8 Thai tax
2/2, and G13 migration persistence 1/1. Protobuf tests passed; G13 parallel
acceptance passed 9/9, deployment recovery 4/4, and G14 local compatibility
5/5. G4 support workflows are integrated across domain/application/infra and
worker seams with atomic persistence, outbox/projector/reconciliation,
redaction, and append-only revocation; live SQLx acceptance remains queued
without its required disposable databases.

PASS: serialized format, warnings-denied workspace Clippy, and the complete
non-ignored workspace tests passed. UI tests, console web/desktop/mobile,
native iOS and Android target checks, Android bridge, Folio lint/typecheck and
bridge tests, and port-3004 health (`200`/`307`) passed. Folio lint has four
pre-existing n8n warnings and no errors; no production Next.js build ran.
The ten exact coordinator-created disposable databases were dropped after
verification; unrelated working-tree changes were preserved.

QUEUED: the mandated ignored workspace invocation exited 101 with exactly 70
environment-gated targets. The failures are missing disposable SQLx/MinIO/PITR
inputs, credentialed Generic LDAP/RFC 4533, AD DS/DirSync, UCS Provisioning
Service and provider environments, operator revocation evidence, approved
protobuf descriptor input, physical mobile execution, true PITR/WAL proof, or
explicit production migration/cutover approval. This is a fail-closed
certification boundary, not a claimed product pass. G0–G14 remain open; no
activation, ownership switch, Folio write retirement, or production cutover
occurred.

## Entry 2026-07-28 — G7 authoritative work-queue policy publication

PASS: WorkQueuePolicy publication is now an end-to-end server-owned path over
the existing Operations aggregate/event/outbox/snapshot/projector/read-model
contracts. `operations.manage` and contained holding scope are checked before
the command. The SQLx method rejects malformed/bounded inputs, uses expected
versions and command idempotency, emits `operations.work_queue_policy.published`,
and advances the read model. The Operations service has publish and bounded
list RPCs; the console reads the policy page and exposes publication fields.

PASS: a fresh two-database PostgreSQL acceptance verified create at version 1,
read projection, replay with no duplicate event, optimistic update at version
2, bounded JSON list, and two authoritative events. Console wire-boundary
tests passed and server/console package compilation passed. The exact
temporary policy databases were removed.

QUEUED: downstream notification/provider/device certification, Folio parity
for policy counts, vendor directory certification, physical mobile execution,
PITR/WAL evidence, and explicit production approval remain open. This packet
does not activate directories, change command ownership, retire Folio writes,
or perform cutover.

## Entry 2026-07-28 — G7 authoritative manager-chain enforcement

PASS: Operations exception assign/reassign/escalate server commands now bind
the requested manager to the assignee's complete effective role-assignment
chain. The root must carry `operations.exception.manage` or `operations.manage`;
active windows, scope containment, delegation links, authority levels, and
permission preservation are checked by `OperationsManagerChainProof` at the
mutation timestamp. A manager who is merely an active Operations reader, or a
manager outside the authoritative chain, is denied before persistence.

PASS: the domain test accepts a valid assignee-to-authorized-manager chain and
rejects a read-only root. Server check and warnings-denied package Clippy
passed. The existing SQLx aggregate/event/outbox path remains the only
mutation path.

QUEUED: live provider/device delivery, Folio parity, credentialed directory
certification, physical mobile execution, PITR/WAL proof, and explicit
production approval remain open. No activation, owner switch, Folio write
retirement, or production cutover occurred.

## Entry 2026-07-28 — current-tree verification after G7 MFA boundary

PASS: serialized `cargo fmt --all --check`, workspace warnings-denied Clippy,
and the complete non-ignored workspace test suite. G13 protobuf compatibility
tests passed 9/9 and G14 local compatibility passed 5/5. Dioxus console web,
desktop, host-mobile, iOS, and Android checks passed after the MFA protobuf/UI
change. Folio `bun run lint` and `bunx tsc --noEmit` passed; lint retains only
the four pre-existing n8n warnings. Port-3004 probes remained healthy (`/`
200 and `/operations` 307), and no production Next.js build ran.

QUEUED: the mandated ignored workspace sweep exited 101 with exactly 70
environment-gated targets. Missing disposable SQLx/MinIO/PITR resources,
credentialed Generic LDAP/RFC 4533, on-premises AD DS/DirSync, UCS
Provisioning Service, operator-revocation evidence, the approved protobuf
descriptor baseline, physical mobile execution, and explicit production
migration/cutover approval remain open. No activation, owner switch, Folio
write retirement, or production cutover occurred.

## Entry 2026-07-28 — G7 source-event producer boundary

PASS: the Operations service now exposes `CreateOperationsExceptionFromSource`
for authenticated source-context producers. The server enforces exact actor
binding, contained holding scope, `operations.exception.manage`, bounded
source/event identities and versions, RFC3339 occurrence time, and bounded
valid JSON. It derives the holding reference from the active tenant registry
and uses the existing SQLx source-provenance, aggregate, event, outbox, and
read-projection path. Client roles and permissions are never accepted; source
contexts remain owners of their transactions and payload authority.

This is an internal producer boundary, so no client UI was added. Server
compilation passed after the additive protobuf RPC. Vendor certification,
downstream delivery, physical mobile execution, PITR/WAL proof, and explicit
production approval remain queued. No activation, owner switch, Folio write
retirement, or production cutover occurred.

## Entry 2026-07-28 — G11 Quality inspection persistence

PASS: Quality inspections now use a versioned, scope-bound domain aggregate;
tenant SQLx source persistence commits the source row, snapshot, complete
outbox envelope, and idempotency receipt atomically; and the separate read
model validates authoritative event identity, contiguous versions, checkpoints,
reconciliation, and clear/rebuild rollback. The durable tenant projection
scheduler registers the Quality incremental/rebuild runner. Authenticated
Quality create/transition/read RPCs enforce entitlement, RBAC, explicit product
scope, tenant routing, customer-scope containment, and audit publication. The
target-neutral UI shell exposes authenticated read-only `/quality` navigation.

Evidence: Quality domain 6/6; Quality worker focused test PASS; G11 product
verifier 8/8; server check PASS; ignored two-database SQLx acceptance compiled
but remains queued until disposable URLs are supplied. No production state,
directory activation, Folio write ownership, or cutover changed.

## Coordinator current-tree verification — G11 Quality — 2026-07-28

PASS: serialized `cargo fmt --all --check`, warnings-denied workspace Clippy,
and the complete non-ignored workspace test suite. G13 protobuf compatibility
passed 9/9, G14 local compatibility passed 5/5, and all five Dioxus console
web/desktop/host-mobile/iOS/Android checks passed. Folio `bun run lint` and
`bunx tsc --noEmit` passed; lint retains four pre-existing n8n warnings. The
port-3004 probes remained healthy (`/` 200 and `/operations` 307). No
production Next.js build ran.

QUEUED: the mandated ignored workspace sweep exited 101 with 71
environment-gated targets, covering missing disposable SQLx/MinIO/PITR
resources, credentialed Generic LDAP/RFC 4533, on-premises AD DS/DirSync,
UCS Provisioning Service, operator-revocation evidence, an approved protobuf
descriptor baseline, physical mobile execution, and explicit production
migration/cutover approval. G0–G14 remain open; no activation, command-owner
switch, Folio write retirement, or production cutover occurred.

## Entry 2026-07-28 — migration embedding and live Quality acceptance

PASS: `crates/migration/build.rs` now explicitly watches the control,
control-read, tenant-write, and tenant-read SQLx migration directories. This
prevents an incremental build from retaining a stale compile-time migration
set after a numbered migration is added. A fresh disposable PostgreSQL
Quality write/read run applied migration 0053 and passed the complete source
write, outbox, projection, duplicate, reconciliation, clear/rebuild, and
read-model acceptance 1/1; both exact databases were dropped afterward.

PASS: current-tree G14 local directory SQLx acceptance passed 3/3, covering
the API/native-worker import handoff, connector feed persistence, and
role-mapping scope containment against a disposable PostgreSQL database. This
does not claim external LDAP, AD DS, or UCS certification.

## Entry 2026-07-28 — serialized verification after migration embedding fix

PASS: `cargo fmt --all --check`, workspace warnings-denied Clippy, the full
non-ignored workspace tests, G13 compatibility 9/9, G14 compatibility 5/5,
all five console Dioxus web/desktop/host-mobile/iOS/Android checks, shared UI
iOS/Android checks, Folio `bun run lint`, `bunx tsc --noEmit`, and port-3004
health (`/` 200, `/operations` 307).

QUEUED: the exact ignored workspace sweep exits 101 with 71 targets because
the environment lacks disposable SQLx/MinIO/PITR inputs, credentialed Generic
LDAP/RFC 4533, on-premises AD DS/DirSync, UCS Provisioning Service, approved
descriptor/mobile/scale evidence, and production migration/cutover approval.
G0–G14 remain open; no activation, command-owner switch, Folio write
retirement, or production cutover occurred.

## Entry 2026-07-28 — G11 Quality read surface

PASS: `ListQualityInspections` is now a bounded, scope-authorized read path
over the tenant read model. SQLx uses keyset pagination on
`(projected_at, inspection_id)`, validates projected identity/version and
scope, and never reads source command state. The server enforces explicit
holding/company/branch containment, Quality entitlement/RBAC, customer scope,
tenant routing, opaque cursor bounds, and append-only audit publication.
The console Quality route renders a read-only authoritative table with scope
form, safe error states, source versions, and next-page navigation.

Evidence: live two-database Quality acceptance passed 1/1 after the typed read
page and scope-filter assertions were added; console tests 108/108, G11
verifier 8/8, desktop check, and server/infrastructure checks passed. No
production state, directory activation, Folio write ownership, or cutover
changed. G0–G14 remain open.

## Entry 2026-07-28 — serialized verification after Quality read surface

PASS: post-change format, warnings-denied workspace Clippy, the complete
non-ignored workspace tests, all five console Dioxus web/desktop/host-mobile/
iOS/Android checks, shared UI iOS/Android checks, G13 9/9, G14 local 5/5,
Folio lint/typecheck, and port-3004 health. The exact mandated ignored sweep
exited 101 with 71 targets, all queued behind missing disposable SQLx,
MinIO/PITR, credentialed Generic LDAP/AD DS/UCS, mobile/scale, or production
approval inputs. No production state, directory activation, Folio write
ownership, or cutover changed; G0–G14 remain open.

## Entry 2026-07-28 — G9 SCM bounded list read

PASS: the SCM SQLx read model now provides a separate-read-pool,
holding/company/branch-scoped, checkpoint-validated, keyset-paginated list for
Procurement and Inventory. The typed `ListScmAggregates` RPC enforces
`scm.read`, product and customer scope, read routing, bounded opaque cursors,
audit publication, and validated aggregate rows. The Dioxus surfaces render
the authoritative list with explicit scope, pagination, loading/error/empty
states, and read-only boundaries.

Evidence: fresh disposable SCM SQLx acceptance 6/6; G11 product verifier 8/8;
console wire-boundary tests 108/108; server/infrastructure checks PASS. The
SCM databases were explicitly dropped after the run. Broader G9 workflow and
external vendor/device/browser/rollback evidence remain queued; no production
state, directory activation, command-owner switch, Folio write retirement, or
cutover occurred.

## Entry 2026-07-28 — serialized verification after G9 SCM list read

PASS: serialized `cargo fmt --all --check`, workspace warnings-denied Clippy,
and the complete non-ignored workspace tests. Fresh SCM two-database SQLx
acceptance passed 6/6; protobuf package tests passed; G13 compatibility and
deployment checks passed 9/9 and 4/4; G14 local compatibility passed 5/5.
Console web, desktop, host-mobile, iOS, Android, and shared UI iOS/Android
target checks passed. Folio `bun run lint` and `bunx tsc --noEmit` passed with
four pre-existing n8n warnings; port 3004 returned `/` 200 and `/operations`
307. No production Next.js build ran.

QUEUED: the exact ignored workspace sweep exited 101 with 71 environment-
gated targets, all at explicit missing SQLx/MinIO/PITR, external directory,
operator-revocation, approved descriptor/mobile, or production-approval
guards. G0–G14 remain open; no activation, command-owner switch, Folio write
retirement, or production cutover occurred.

## Entry 2026-07-28 — G11 Asset/Maintenance bounded list read

PASS: `SqlxAssetMaintenanceStore::list_for_scope` now provides a separate
read-pool, holding/company/branch-scoped, checkpoint-validated,
keyset-paginated list for assets and maintenance work orders. It validates
aggregate identity/version, typed scope columns, generated references, and
asset custody-history rows. The typed RPC enforces explicit product scope,
`asset_maintenance.read`, customer scope, tenant read routing, bounded opaque
cursors, audit publication, and safe typed rows. The Maintenance Dioxus route
renders the authoritative list with explicit scope and read-only state
handling.

Evidence: fresh disposable Asset/Maintenance SQLx acceptance 1/1; G11 product
verifier 8/8; console wire-boundary tests 109/109; focused warnings-denied
Clippy PASS. Broader G11 workflow, vendor/device/browser/rollback evidence and
production approval remain queued; no activation, owner switch, Folio write
retirement, or cutover occurred.

## Entry 2026-07-28 — G9/G11 Sales bounded list read

PASS: `SqlxSalesStore::list_for_scope` now reads only the tenant read pool and
returns a bounded company/branch page keyed by projected timestamp, allow-listed
aggregate type, and UUID. It revalidates the read checkpoint, source version,
aggregate identity, typed scope columns, domain payload, and reference bounds;
holding, subtree, platform, forged, stale, and mismatched rows fail closed.

PASS: `ListSalesAggregates` is an authenticated typed RPC requiring explicit
Sales product scope, `sales.read`, customer-scope containment, tenant read
routing, bounded opaque cursors, and append-only audit publication. The Dioxus
console adds an explicit-scope read-only Sales table with safe loading, empty,
denied/offline/error, and next-page states. Aggregate payload bytes are not
returned by the list surface.

PASS: focused console Sales list boundary 1/1, infrastructure Sales unit suite
6/6, and protobuf/infrastructure/server/console checks passed. The ignored
two-database Sales acceptance now asserts list scope rows, source version,
sibling-company isolation, reconciliation, and clear/replay rollback; it is
queued without disposable routed URLs. This closes the Sales list-read
implementation marker only. Full Lead-to-Cash workflow, live replay and
rollback, worker restart, accounting/Operations reconciliation, provider and
device/browser journeys, external directory certification, and production
approval remain open. No activation, command-owner switch, Folio write
retirement, or cutover occurred.

## Entry 2026-07-28 — serialized current-tree verification after Sales list

PASS: `cargo fmt --all --check`, workspace warnings-denied Clippy, and the
complete non-ignored workspace test suite. Focused Sales server and console
tests, Sales infrastructure unit tests, and G11 Sales transport acceptance
passed (3/3). Protobuf tests passed. G13 parallel acceptance passed 9/9,
deployment/recovery 4/4, and G14 local compatibility 5/5. Console web,
desktop, host-mobile, iOS, Android, and shared UI iOS/Android target checks
passed. Folio lint and TypeScript passed; lint retains four pre-existing n8n
warnings. Port 3004 remained healthy (`/` 200 and `/operations` 307); no
production Next.js build ran.

QUEUED: the exact ignored workspace invocation exited 101 with exactly 71
targets. Every failure remained an explicit missing SQLx/MinIO/PITR,
credentialed Generic LDAP/RFC 4533, AD DS/DirSync, UCS Provisioning Service,
operator-revocation, approved descriptor, physical mobile/scale, or production
approval guard. No new application assertion failure appeared. G0–G14 remain
open; no activation, command-owner switch, Folio write retirement, or
production cutover occurred.

## Entry 2026-07-28 — G6 scheduler composite binding hardening

PASS: additive control migration `0064_tenant_projection_scheduler_binding.sql`
now enforces composite tenant/holding references for scheduler jobs, claim
batches, claims, and rebuilds, with scoped job and worker-owned batch foreign
keys on claims. This is database defense-in-depth behind the existing
registry authorization, row locks, and transactional state transitions.

PASS: fresh local PostgreSQL control write/read acceptance asserted all seven
constraints and passed 1/1; `erpx-migration` passed 12/12 and formatting
passed. The temporary databases were dropped after the run. This packet does
not claim G6/G14 completion; external directory/vendor/device/scale inputs,
approved descriptors, and production approval remain queued. No production
state, activation, owner switch, Folio write retirement, or cutover occurred.

## Entry 2026-07-28 — final verification after G6 binding packet

PASS: serialized format, workspace warnings-denied Clippy, and all
non-ignored workspace tests. Protobuf tests, G13 parallel/recovery (9/9 and
4/4), G14 local compatibility (5/5), console Dioxus web/desktop/host-mobile/
iOS/Android, shared UI iOS/Android, Folio lint, and TypeScript checks passed.
Folio stayed healthy on port 3004 (`/` 200, `/operations` 307); lint retains
four pre-existing n8n warnings and no production Next.js build ran.

QUEUED: the exact ignored sweep exited 101 with exactly 71 explicit missing
SQLx/MinIO/PITR, credentialed Generic LDAP/RFC 4533, AD DS/DirSync, UCS,
operator-revocation, approved descriptor, mobile/scale, or production
approval guards. No new application assertion failure appeared. G0–G14 are
not complete; no activation, owner switch, Folio write retirement, or
production cutover occurred.

## Entry 2026-07-28 — G3 Customer 360 bounded detail facts

PASS: added the additive `Customer360SectionFact` protobuf field to the
Customer 360 detail response. The server derives bounded scalar facts from
the existing audited SQLx read model across account, organization, contacts,
commercial, entitlements, onboarding, health, support, access-audit, and
notes. The console rejects unknown sections/keys, duplicates, controls, and
oversized values, then renders only the validated read-only facts. Raw JSON,
provider references, credentials, object keys, contact contents, and audit
payloads remain excluded.

PASS: `cargo fmt --all --check`, proto/server/console checks, console web
tests 112/112, and strict server/console Clippy. No migration or source write
occurred. Rollback is additive field removal plus adapter revert; persisted
Customer 360 source/read state is unchanged.

OPEN: G3 and G0–G14 still require broader workflow/read-model evidence,
credentialed directory/vendor certification, device/scale/PITR evidence,
approved descriptor comparison, and explicit production approval. No
activation, command-owner switch, Folio write retirement, or cutover occurred.

## Entry 2026-07-28 — serialized verification after G3 detail facts

PASS: `cargo fmt --all --check`, the complete non-ignored workspace test
suite, warnings-denied workspace Clippy, G13 parallel compatibility 9/9, all
console Dioxus web/desktop/host-mobile/iOS/Android checks, shared UI iOS and
Android checks, `bun run lint` (0 errors; four pre-existing n8n warnings),
`bunx tsc --noEmit`, `git diff --check`, and Folio port-3004 probes (`/` 200,
`/operations` 307).

QUEUED: the exact ignored workspace sweep exited 101 with exactly 71 targets,
all failing their explicit environment/certification guards for disposable
SQLx/MinIO/PITR, Generic LDAP/RFC 4533, AD DS/DirSync, UCS Provisioning
Service, approved descriptor, physical mobile/scale, or production approval.
No new application assertion failure appeared. G0–G14 remain open; no
activation, command-owner switch, Folio write retirement, or production
cutover occurred.

## Entry 2026-07-28 — G10 Marketing content replacement-read list (current tree)

PASS: current-tree proto/server/infrastructure/console checks, console wire
tests 113/113, and strict focused Clippy. The SQLx replacement-read adapter
returns only bounded metadata for assets, templates, forms, and domains and
fails closed on invalid scope, source/checkpoint/receipt continuity, unsafe
labels, malformed IDs, oversized pages, or ambiguous unfiltered UUID cursors.
The RPC enforces `marketing.read`, explicit scope containment, customer scope,
read routing, and audit publication; the console renders read-only pages.

QUEUED: the dedicated live SQLx acceptance is wired but cannot run without
`ERPX_TEST_TENANT_WRITE_DATABASE_URL`; no production state or write ownership
changed. External directory/vendor/device/scale/PITR, approved descriptor,
and production-approval evidence remain open.

## Entry 2026-07-28 — G11/G8/G4/G13 reconciliation and rollback continuation

PASS: G11 Quality reconciliation detects inspection projection rows that are
stale, ahead, missing, orphaned, or inconsistent by version/count, with
bounded authoritative rebuild recovery. G8 Finance chart reads reject
normalized account rows whose aggregate version differs from the authoritative
chart header and preserve stale-row reconciliation evidence. G4 Support
reconciliation compares bounded capability bindings against read projections,
detecting tamper, missing, orphan, stale, and oversized sets. G13 target
recovery restores durable rollback snapshots for empty/partial target reads
and replays through source fences and idempotent outbox boundaries.

Evidence: Quality focused tests 4 passed; Finance chart focused tests 2 passed;
G13 parallel acceptance 9/9; G13 deployment/recovery 4/4; focused formatting,
strict infrastructure checks, and the integrated non-ignored workspace suite
passed. Audit records: `audits/G11-QUALITY-RECONCILIATION-2026-07-28`,
`audits/G8-FINANCE-CHART-2026-07-28`,
`audits/G4-SUPPORT-RECONCILIATION-2026-07-28`, and
`audits/G13-ROLLBACK-REPLAY-2026-07-28`.

QUEUED: live SQLx execution lacks configured disposable URLs. Credentialed
Generic LDAP/RFC 4533, AD DS/DirSync, UCS Provisioning Service, approved
protobuf descriptor, physical mobile, scale, PITR, and production-approval
evidence remain unavailable. No activation, command-owner switch, Folio write
retirement, or production cutover occurred.

## Entry 2026-07-28 — local SQLx acceptance wave and G4 revocation fix

PASS: local PostgreSQL acceptance found and corrected a G4 support contract
mismatch. `SupportSession::request_with_module` now rejects a represented
tenant user for non-impersonation modes, matching the authoritative schema and
intersection policy. `SupportWorkflowStore::revoke_grant` revokes active
capabilities before the access request in one transaction, satisfying the
approval-binding trigger and preserving append-only capability/request events.

Evidence: support workflow SQLx 1/1 and support worker projection 1/1; local
directory 3/3 and directory RBAC 1/1; identity 6/6; role-review persistence
2/2 and notification worker 1/1; Quality 1/1; Finance chart/close 1/1 each,
Finance persistence 2/2, banking 2/2; Legal 3/3; POS 4/4; HR 4/4; SCM 6/6;
Customer 360 worker 1/1 and detail 4/4; Operations completion 2/2 and
projector 1/1; Finance candidate 1/1; scheduler 1/1; platform completion
1/1; tenant registry 1/1; Marketing content/CRM 1/1 each; G13 disposable
acceptance 1/1; and isolated control-plane tests 3/3. Disposable databases
were removed after every run.

QUEUED: combined control-plane target execution must use isolated databases
per test because its fixtures intentionally publish pending outbox rows; the
three individual SQLx tests pass. Credentialed external directories,
descriptor, mobile/device, scale, PITR/MinIO credential, and production
approval remain unavailable. No activation, owner switch, Folio write
retirement, or production cutover occurred.

## Entry 2026-07-28 — current-tree verification after G4 fix

PASS: `cargo fmt --all --check`, workspace warnings-denied Clippy, and the
complete non-ignored workspace suite after the G4 domain and revocation-order
changes. Protobuf tests passed. Console web, desktop, host-mobile,
aarch64-apple-ios, aarch64-linux-android, and shared UI host/iOS/Android
checks passed. Folio lint and TypeScript passed; the four existing n8n lint
warnings remain. Port 3004 stayed healthy (`/` 200, `/operations` 307), and
no production Next.js build ran.

QUEUED: the exact ignored workspace invocation exited 101 with exactly 71
explicit missing SQLx/MinIO/PITR, credentialed Generic LDAP/RFC 4533, AD
DS/DirSync, UCS Provisioning Service, operator-revocation, approved
descriptor, physical mobile/scale, or production-approval guards. The local
disposable acceptance wave passed its isolated SQLx tests and removed every
test database afterward. G0–G14 are not complete; no activation,
command-owner switch, Folio write retirement, or production cutover occurred.

## Entry 2026-07-28 — parallel G9–G14 integrity wave

PASS: six disjoint worker lanes were continuously reassigned across G9–G14
packets. Focused evidence includes domain, identity, infrastructure, worker,
migrator, and shared-UI tests; disposable SQLx acceptance for Sales, SCM,
Finance close, Quality, HR, Legal, Asset/Maintenance, POS, Marketing CRM and
journeys, G13 rollback/bridge, and local directory role revocation. The
combined slices add authoritative source fences, append-only facts, scope/RLS
checks, reconciliation and rollback guards, mobile-safe session handling,
TLS/read-only Folio bridge enforcement, and fail-closed G14 certification
readiness for Generic LDAP/RFC4533, AD DS/DirSync, and UCS Provisioning.

QUEUED: credentialed external directories, approved descriptor/hash,
physical mobile/device execution, scale, PITR/MinIO credentials, and explicit
production approval remain unavailable. Focused tests do not close G0–G14 or
authorize activation, command-owner switch, Folio write retirement, or
production cutover. The exact required ignored workspace sweep and the
serialized combined-tree verification remain to be run after integration.

## Entry 2026-07-28 — serialized verification after parallel integrity wave

PASS: `cargo fmt --all --check`, workspace warnings-denied Clippy, and the
complete non-ignored workspace tests. Protobuf tests passed. G14 local
compatibility 6/6, G13 parallel acceptance 11/11, G13 deployment/recovery
5/5, G14 directory acceptance 9/9, and G14 readiness 7/7 passed. All console
and shared-UI Dioxus web/desktop/host-mobile/iOS/Android checks passed.
Folio lint passed with four pre-existing n8n warnings; TypeScript passed;
port 3004 remained healthy (`/` 200, `/operations` 307). No production
Next.js build ran.

QUEUED: the exact ignored workspace invocation exited 101 with exactly 71
targets. Every failure was an explicit environment/certification guard for
disposable SQLx/MinIO/PITR, Generic LDAP/RFC4533, AD DS/DirSync, UCS
Provisioning Service, approved descriptor, mobile/device/scale, or
production approval. No new application assertion failed. G0–G14 remain
open; no activation, owner switch, Folio write retirement, or production
cutover occurred.

## Entry 2026-07-29 — isolated SQLx completion wave and directory cursor guard

PASS: isolated acceptance evidence covered directory/RBAC/identity and role
review (13 checks), Finance/control-plane (29 checks after repairing the
fixture), product contexts (49 checks), and G13/G6/backup/object-storage,
Operations, and Folio bridge paths. The tenant projection-status acceptance
now seeds its active tenant/holding via the authoritative registry API before
writing the scheduler row; the composite tenant/holding FK remains enforced,
and the incremental durable-checkpoint/idempotency test passes. Disposable
databases were force-dropped and catalog/connection audits found no residue.

PASS: `directory_live` uses checked cursor advancement and returns an
explicit sequence-overflow error rather than repeating the terminal `u64`
sequence. The focused worker test passes.

QUEUED: the exact external/certification gates remain unverified: live
Generic LDAP/RFC 4533, AD DS/DirSync, UCS Provisioning Service, approved
descriptor/hash, credentialed MinIO backup plus physical PITR/WAL restore,
physical mobile/device and scale execution, and production approval. Local
MinIO liveness is not backup evidence. No directory activation, command-owner
switch, Folio write retirement, or production cutover occurred.

## Entry 2026-07-29 — live Generic LDAP/RFC4533 acceptance and native-feed handoff fix

PASS: the live Generic LDAP harness ran against a real local OpenLDAP server
over a custom trusted LDAPS certificate with the RFC 4533 syncprov overlay.
It proved a two-user full import with `max_import_users=1_000_000`, durable
RFC 4533 page/cursor continuity, canonical immutable links, a real provider
disable of `alice`, incremental native-feed observation, a durable disabled
projection, a `directory_identity` outbox event, and an exact five-minute
revocation-SLA row. The provider-side action was performed through LDAP;
no synthetic disable was inserted into ERPX.

PASS: full-import SQLx staging now publishes the native cursor checkpoint and
preserves the prior RFC 4533 cookie on terminal/empty responses. Incremental
pages preserve that cookie, stop cleanly on no-work responses, reuse the
canonical full-import immutable link, reject immutable rebinds, and append
the disable/delete event and revocation-SLA record atomically with the
projection transaction. The isolated identity directory SQLx suite passed
3/3 after the change; formatting and live Generic LDAP acceptance passed.

EVIDENCE HYGIENE: PostgreSQL TLS was enabled only for the local acceptance
because the harness requires `sslmode=require`; it was restored to `ssl=off`
afterward. The disposable databases and OpenLDAP container were removed.
AD DS/DirSync, UCS Provisioning Service, approved descriptor, MinIO/PITR,
mobile/device/scale, and production-approval evidence remain queued and are
not inferred from the Generic LDAP result.

## Coordinator continuation — final serialized verification and isolated acceptance — 2026-07-29

PASS: the final combined-tree gates passed: `cargo fmt --all --check`,
`git diff --check`, warnings-denied workspace Clippy, the complete
non-ignored workspace suite including the one-million-user volume test,
protobuf compilation, G13 compatibility/registration (12/12), G14 directory
acceptance (9/9), all Dioxus web/desktop/host-mobile/iOS/Android checks,
Folio lint/typecheck, and the port-3004 protected-route probe (307). No
production Next.js build ran.

PASS: fresh isolated PostgreSQL reruns passed Operations persistence (8/8),
Operations worker (2/2), the stale-health provisioning guard, control-plane
SQLx (3/3), support persistence (8/8), support worker (1/1), and the focused
directory/identity/RBAC and product acceptance lanes. The local Samba
AD-compatible LDAPS/DirSync probe passed with a full-DN bind, but remains
compatibility evidence rather than Microsoft AD DS certification.

QUEUED: the exact ignored workspace sweep without disposable references
exited 101 at 71 explicit guards. Remaining blockers are credentialed MinIO
and physical PITR/WAL evidence, approved protobuf descriptor comparison,
Microsoft AD DS/DirSync certification, UCS Provisioning Service certification,
physical mobile/device and measured-scale evidence, and explicit production
approval. No customer directory activation, command-owner switch, Folio write
retirement, or production cutover occurred.

## Coordinator continuation — directory authentication and compatibility hardening — 2026-07-29

PASS: durable directory authentication now has a live TLS-only LDAP transport
for Generic LDAP and on-prem AD-compatible LDAP endpoints. The bind path
validates the persisted connector/provider/endpoint, searches exactly one
user, extracts the provider immutable identifier (including AD little-endian
objectGUID), rejects disabled/deleted accounts, requires verified peer TLS
evidence, and issues a normal five-minute ERPX session through the atomic
identity write transaction. The transaction locks the directory link and
projection, refuses stale/revoked identities, persists device/session rows,
and emits a credential-free authentication outbox event. The isolated SQLx
directory suite passed 3/3, including access-token authorization and durable
authentication-event evidence.

PASS: persisted directory import concurrency is now read from the durable
connector row and bounded before the worker constructs its runner. Same-source
revision replays are idempotent only when their serialized payload digest
matches; conflicting same-revision payloads fail closed. The source payload
digest migration is additive and formatting, warnings-denied Clippy, and
focused SQLx tests pass.

PASS: G13 and G14 descriptor compatibility comparators now retain and compare
protobuf message/enum reserved ranges and reserved names in addition to field,
enum-value, and RPC contracts. The G13 focused acceptance passed 13/13 and the
G14 local compatibility suite passed 8/8. No approved descriptor baseline was
invented; baseline comparison remains queued until an operator supplies the
authoritative reference.

QUEUED: the live UCS image is a disposable official UCS 5.2-6 KVM instance
being initialized as a primary directory node. Native HTTPS/UMC setup
validation and configuration submission succeeded, but the vendor setup
restart/package phase is still in progress; the Provisioning Service app,
subscription, CA-bound messages, ERPX worker feed, and durable SQLx evidence
have not yet been certified. Microsoft AD DS remains unavailable; the Samba
probe is compatibility evidence only. No customer connector was activated and
no production ownership or Folio write path changed.

## Coordinator continuation — final parallel completion wave and UCS auth hardening — 2026-07-29

PASS: G9 goods-receipt posting now locks and validates both purchase-order and
receipt aggregates, writes both source snapshots and append-only outbox events
atomically, records idempotency, and projects/rebuilds the pair safely. G10
CRM read reconciliation detects payload drift against authoritative outbox
state and repairs through a source-fenced rebuild. G6 restore acceptance now
requires a tenant marker to survive real dump/upload/download/restore before
scoped read verification. G12 UI state panels expose stable state,
authorization, cached-payload, and accessibility markers.

PASS: UCS authentication now has a production-shaped HTTPS UDM REST transport:
certificate-pinned peer evidence is required, the response must contain exactly
one user, immutable `entryUUID` is the only link value, and disabled/locked
accounts are rejected by the directory authentication policy. TLS preflight
permits HTTPS only for UCS and ordinary local password login rejects
directory-linked principals, preserving the local break-glass-only boundary.

PASS: final serialized evidence is green for workspace formatting, warnings-
denied Clippy, all non-ignored workspace tests, G13 13/13, G14 8/8, all
Dioxus console targets, Folio lint/typecheck, and port-3004 health. Fresh local
directory SQLx passed 3/3. The exact aggregate ignored command exits 101 at
exactly 71 explicit missing-resource/certification guards.

QUEUED: the disposable UCS image did not complete native setup and therefore
has no Provisioning Service app, subscription, message acknowledgement, or ERPX
worker/feed evidence. Microsoft AD DS, approved descriptor/hash, credentialed
MinIO and physical PITR/WAL restore, physical mobile/device and measured-scale
execution, and explicit production approval remain blocked. No activation,
ownership switch, Folio write retirement, or production cutover occurred.

## Entry 2026-07-29 — final reassigned G7–G14 packets and verification

PASS: G11 Legal retention now loads candidates and parties under the holding
scope before typed idempotent expiry/outbox mutation. G10 Journey execution
release requires the matching worker and an unexpired lease. G7 Operations
notifications prove durable first-claim acknowledgement, failed-publisher
release, and restart reclaim behavior. G12 adds authenticated/platform-scoped
offline-cache redaction and protected-payload state tests. G14 adds a
descriptor-mutation fail-closed regression. These were disjoint product-owned
changes; shared coordinator-owned surfaces were untouched.

PASS: serialized `cargo fmt --all --check`, workspace Clippy with warnings
denied, and the full non-ignored workspace suite passed. Protobuf tests passed;
G13 compatibility passed 13/13; G14 local compatibility passed 9/9; Dioxus
web/desktop/Android/iOS checks passed; fresh local directory SQLx passed 3/3;
Folio `bun run lint` and `bunx tsc --noEmit` passed; and Folio remained healthy
on port 3004 (`/` 200, `/operations` 307). No production Next.js build ran.

QUEUED: the required ignored sweep exits 101 at exactly 71 explicit guards
for absent disposable SQLx/MinIO/PITR resources, external directory/vendor
credentials, approved descriptor/hash, mobile/device/scale evidence, or
production approval. UCS remains unconfigured/degraded without a machine
secret, healthy LDAP/HTTPS, installed Provisioning Service, subscription, or
ERPX feed evidence. No activation, command-owner switch, Folio write
retirement, or production cutover occurred; G0–G14 remain open.

## Entry 2026-07-29 — disposable UCS join and HTTPS UDM evidence

PASS: the disposable official UCS 5.2-6 image completed the documented
primary-node join path. The VM now has `ucs.erpx.test`, `/etc/machine.secret`,
the join marker, active slapd/directory-listener, and a repaired Apache TLS
configuration. A host-side HTTPS UDM REST request using the test-only
Administrator credential and the JSON/ HAL media type returned exactly one
user with immutable `univentionObjectIdentifier`, `disabled=false`, and
`locked=false`. This proves the real UCS HTTPS authentication surface and
TLS evidence; it does not claim native feed acceptance.

QUEUED: App Center metadata resolves Provisioning Service 2.2.1 and backend
2.0, but both the full app and backend installation stop at the official
`must_have_valid_license` guard because the disposable image has no UCS
license key. No vendor licensing bypass or synthetic message was used. The
Provisioning Service subscription, native message acknowledgement, ERPX
worker/feed SQLx evidence, Microsoft AD DS/DirSync, approved protobuf
descriptor/hash, credentialed MinIO/physical PITR, mobile/device/scale, and
production-approval gates remain open. No customer directory was activated,
and no command ownership or Folio write path changed.

## Entry 2026-07-29 — current parallel completion tree and serialized verification

PASS: the latest disjoint packets hardened Operations execution-time
authorization and notification release/retry behavior; tenant scheduler
dead-letter claim evidence; Finance tax/budget/subledger/close/chart source
event and replay checks; Marketing provider onboarding, delivery checkpoints,
and CRM source history; Legal/Quality scope integrity; HR payroll validation;
POS payment immutability; Sales duplicate-version paging; G13 source fences;
G14 descriptor and vendor preflight guards; and G12 accessibility/offline state
markers. The only coordinator-owned correction was deterministic test timing in
the Operations invalid-state regression; no production behavior was weakened.

PASS: `cargo fmt --all --check`, workspace warnings-denied Clippy, and the
complete non-ignored `cargo test --workspace --offline --no-fail-fast --quiet`
all pass. Focused evidence is G13 parallel 13/13, G13 deployment/recovery 6/6,
G14 local compatibility 10/10, G14 directory acceptance 10/10, and a fresh
three-test local directory SQLx acceptance 3/3. Dioxus web/desktop/iOS/Android
checks pass. Folio lint/typecheck pass with the four existing n8n warnings,
and port 3004 remains healthy (`/` 200, `/operations` 307). No production
Next.js build ran.

QUEUED: the exact ignored workspace invocation exits 101 at 78 explicit
environment/certification targets. Failures are missing disposable SQLx,
MinIO/PITR, credentialed AD DS/UCS/vendor references, approved protobuf
descriptor provenance, mobile/device/scale evidence, or production approval;
no new application assertion failure remains. Generic LDAP/RFC4533 is the
completed live vendor lane. UCS UDM authentication is real and TLS-only, but
Provisioning Service installation is blocked by the official missing-license
guard, so no native subscription/message/feed evidence is claimed. Microsoft
AD DS/DirSync, MinIO/physical PITR, descriptor baseline, device/scale, and
production approval remain open. No customer directory was activated, no
command ownership changed, and no Folio writes were retired.

## Entry 2026-07-29 — final directory/UI audits and serialized verification

PASS: directory-live hardening now rejects provider-native LDAP/AD locked or
deleted accounts before authentication, preserves immutable identity links,
rejects duplicate page identities, enforces TLS-version evidence, handles UCS
deleted/locked objects, and prevents disabled connector resume. Finance bank
imports, Legal retention, G14 readiness, console QA, and Dioxus auth received
bounded fail-closed hardening in disjoint files. High-risk UI rendering never
implies authorization or fabricated MFA; local credentials remain a
break-glass-only fallback.

PASS: `cargo fmt --all --check`, workspace warnings-denied Clippy, the full
non-ignored workspace suite, protobuf package tests, G13 13/13, G13 recovery
6/6, G14 local 10/10, G14 external 9/9 plus one ignored preflight, G14
directory 10/10, vendor harness 10/10, identity feed 3/3, identity sync
22/22, all four Dioxus target checks, Folio lint/typecheck, and port-3004
health passed. The exact ignored sweep exits 101 at 80 guarded targets; its
failures are missing external/disposable resource evidence, not application
assertions. No production Next.js build ran.

QUEUED: Generic LDAP/RFC4533 is the completed live lane. AD DS/DirSync has no
credentialed external environment. UCS UDM authentication is real and
TLS-only, but the official Provisioning Service app is license-gated, leaving
subscription/message/native-feed acceptance unproven. The approved protobuf
descriptor, MinIO/physical PITR, physical mobile/device and scale evidence,
and explicit production approval are also unavailable. No customer directory
was activated; command ownership, Folio writes, and production cutover remain
unchanged. G0–G14 remain open pending those certification gates.

## Entry 2026-07-29 — external-resource re-audit

RECHECK: the completion branch and Folio port-3004 runtime are intact. The
disposable UCS VM remains alive and its HTTPS UDM endpoint still presents the
expected Basic challenge without credentials; no new UCS Provisioning Service
license, subscription, or native-feed evidence is available. No AD DS image,
process, credential, or certification reference is present in the current
environment. No production ownership, directory activation, Folio write path,
or cutover state changed.

QUEUED: AD DS/DirSync and UCS Provisioning Service remain external
certification blockers. The goal remains active and is not marked complete or
blocked.

## Entry 2026-07-29 — cross-gate hardening and final verification wave

PASS: local hardening closed additional gaps in directory UUID/tombstone
handling, connector validation, durable RBAC/outbox rebuild fencing, support
acknowledgement and diagnostics boundaries, finance bank-import claims,
tenant/backup/provisioning identity fences, Customer 360 audit scope, G9–G11
worker route/source/cursor validation, secure-session revocation persistence,
and Dioxus Finance/Platform/Operations/Support/Customer 360 UI state and
redaction. No external certification or production authority was fabricated.

PASS: final post-change format, warnings-denied Clippy, complete non-ignored
workspace tests, protobuf tests, G13 14/14 and recovery 6/6, G14 local 11/11,
directory 10/10, readiness 9/9 plus one ignored preflight, vendor 10/10,
identity feed 3/3, identity sync 22/22, all Dioxus target checks, Folio
lint/typecheck, and port-3004 health passed. The exact ignored sweep exits
101 at 80 resource-gated targets; no application assertion failure was
observed. No production Next.js build ran.

QUEUED: AD DS/DirSync, UCS Provisioning Service licensing/native-feed
acceptance, approved protobuf provenance, disposable runtime DB references,
MinIO/physical PITR, physical mobile/device/scale execution, and production
approval remain unavailable. No directory activation, command-owner switch,
Folio write retirement, or cutover occurred. G0–G14 remain active pending
those evidence gates.

## Entry 2026-07-29 — final parallel hardening wave

PASS: G3 reconciliation now includes the unprojected-audit counter; Customer
360 and Support UI cursors/routes fail closed; Support workflow outbox claims
are restricted to its aggregate allow-list and only its explicit worker
composition registers the projector; Stripe query requests are GET-only with
redirects disabled; malformed claimed backup jobs persist failure receipts;
and product completion/QA surfaces enforce holding/tenant scope plus bounded,
redacted pagination evidence.

PASS: focused slice tests, `cargo fmt --all --check`, workspace warnings-denied
Clippy, and the complete non-ignored workspace suite pass. The required
ignored sweep ran and exits 101 at 80 guarded targets due to absent disposable
SQLx/control/tenant, MinIO/PITR, vendor, and runtime evidence; no application
assertion failure was observed. G13 14/14 plus recovery 6/6, G14 local 11/11,
directory 10/10, readiness 9/9 plus one intentional preflight ignore, vendor
10/10, identity feed 3/3, identity sync 22/22, protobuf, all Dioxus targets,
Folio lint/typecheck, and port-3004 probes pass.

QUEUED: AD DS/DirSync credentials and certification evidence; UCS Provisioning
Service license/subscription/native-feed evidence; approved protobuf baseline;
credentialed SQLx, MinIO, and physical PITR/WAL restore; physical mobile and
measured-scale evidence; and explicit production approval. Generic LDAP/RFC4533
is the completed live vendor lane. The UCS UDM lane is real and TLS-only, but
the official valid-license guard prevents claiming Provisioning Service
acceptance. No customer directory activation, command-owner change, Folio
write retirement, or production cutover was performed; the goal remains active.

## Entry 2026-07-29 — final product hardening and serialized verification

PASS: final G9–G12 packets hardened Sales collection-payment persistence and
source reconciliation, SCM cancellation/goods-receipt and one-million-event
worker pagination, Marketing provider identity and timeout/reconciliation
fences, Legal actor/version/retention integrity, POS one-million-event source
fencing with atomic rollback, and the 12-context UI/G14 acceptance matrix. The
coordinator raised the Sales worker budget to 2,000 pages at the 500-event
ceiling and changed replacement rebuilds to call the existing fenced
transactional infrastructure rebuild. Shared registration, protobuf,
manifests, migration numbering, production ownership, and cutover surfaces
were untouched.

PASS: `cargo fmt --all --check`; workspace Clippy with `-D warnings`; the
complete non-ignored workspace tests; `cargo test -p erpx-proto`; G13
parallel acceptance 14/14; G14 local compatibility 11/11; G14 readiness 9/9
plus one intentionally ignored preflight; local directory SQLx 3/3; directory
RBAC SQLx 2/2; and real PostgreSQL 18/TLS MinIO backup/restore 1/1. Dioxus
web/desktop/host-mobile/iOS/Android and shared UI iOS/Android target checks
passed. Folio `bun run lint` and `bunx tsc --noEmit` passed with four existing
n8n warnings; port 3004 returned `/` 200 and `/platform` 307. No production
Next.js build ran.

QUEUED: the exact ignored workspace sweep exits 101 with 80 guarded targets;
the failures are missing disposable SQLx/resource references, credentialed
directory/vendor references, MinIO/PITR references, approved protobuf
baseline, device/scale evidence, or production approval. The fail-closed
external preflight reports missing Generic LDAP/RFC 4533, AD DS/DirSync, and
UCS Provisioning Service references. The disposable UCS VM proves TLS UDM
authentication, but the official Provisioning Service install is license
gated. Local directory SQLx is not vendor certification. No directory was
activated, no command owner changed, no Folio writes were retired, and no
production cutover occurred; G0–G14 remain open.

## Entry 2026-07-29 — role-review authority and final target verification

PASS: role-review snapshot/recovery/notification persistence and worker
acknowledgement now require active authoritative role assignments. Fresh local
PostgreSQL acceptance passed role-review persistence 2/2 and worker partial-ack
reconciliation 1/1 after seeding the real authority fixture. Focused
role-review infrastructure/worker and Operations console/UI tests passed.

PASS: serialized fmt, warnings-denied workspace Clippy, non-ignored workspace
tests, protobuf/G13/G14 local checks, all requested Dioxus targets, Folio
lint/typecheck, and port-3004 probes passed. The full ignored workspace command
was rerun; it exits 101 at 80 fail-closed resource guards, with no new
application assertion failure.

QUEUED: external LDAP/AD DS/UCS credentials and vendor acceptance, approved
descriptor/hash, disposable runtime SQLx references, physical MinIO/PITR,
device/scale evidence, and production approval. No customer directory,
command ownership, Folio write path, or production cutover changed.

## Entry 2026-07-29 — parallel completion wave and serialized gates

PASS: identity session/device persistence is atomic; directory-managed
principals cannot use ordinary password login; principal-bound session
revocation and persisted revocation ordering are enforced. G2 approver scope
intersection is wired into SQLx recovery receipts with complete parent-chain
rechecks; fresh PostgreSQL role-review acceptance passed 2/2. Operations
manager hierarchy, Finance UI scope gating, and CRM UI typed-scope/redaction
guards passed focused tests.

PASS: Finance provider balance/replay, Marketing CRM provenance/replay, SCM
receipt and reconciliation, Sales workflow linkage/source reconciliation, and
commercial provider rollback focused checks passed. Sales workflow-linkage
drift now contributes to reconciliation failure. UI passed its complete
package suite, CRM 4/4, and Finance 5/5.

PASS: final serialized format, workspace warnings-denied Clippy, and
non-ignored workspace tests passed. The exact ignored sweep exited 101 with
80 guarded targets; failures were queued missing-resource/certification
inputs only. Folio lint/typecheck passed with four pre-existing n8n warnings;
Folio probes remained `/` 200 and `/platform` 307 on port 3004.

QUEUED: AD DS/DirSync credentials, UCS Provisioning Service license and
native-feed evidence, credentialed Generic LDAP/RFC 4533 acceptance, approved
protobuf descriptor/hash, disposable SQLx/vendor/MinIO/PITR inputs, physical
device/scale evidence, and explicit production approval. No activation,
ownership switch, Folio write retirement, or cutover was performed; G0–G14
remain active.

## Entry 2026-07-29 — final post-packet audit

PASS: late HR expense-lifecycle changes are compile-safe and HR domain tests
pass 10/10; four HR SQLx acceptance tests remain explicitly environment-gated.
The one-million-user directory-volume test completed during the final
workspace run. Final format, workspace Clippy/tests, Folio lint/typecheck,
and port-3004 probes remain green.

QUEUED: external AD DS/DirSync and UCS Provisioning Service certification,
physical PITR/device/scale execution, approved protobuf descriptor, credentialed
runtime databases, and production approval remain open. No activation,
ownership switch, Folio write retirement, or cutover occurred.

## Entry 2026-07-29 — final serialized verification

PASS: the parallel completion wave is integrated on
`codex/erpx-parallel-completion`. Identity directory authentication, immutable
linking, directory RBAC, LDAP/RFC 4533, AD DirSync, and UCS adapter contracts
are implemented with fail-closed feed/revocation behavior and local one-million
directory-volume coverage. G7–G12 focused domain, SQLx/persistence, worker,
read-model, authorization, API, reconciliation, and UI evidence remains green
for the applicable slices; late HR, Legal, Support, Finance close, and
Marketing provider hardening is compile-safe.

PASS: `cargo fmt --all --check`; workspace Clippy with `-D warnings`; the
complete non-ignored workspace suite; `cargo test -p erpx-proto --offline
--quiet`; G13 compatibility/registration 14/14; G14 local descriptor
compatibility 11/11; all requested Dioxus targets; Folio `bun run lint` (four
existing n8n warnings only); and `bunx tsc --noEmit`. Port-3004 probes were
`/` 200 and `/platform` 307. No production Next.js build ran.

BLOCKED BY INPUTS, NOT IMPLEMENTATION: the exact ignored workspace sweep
exited 101 at 80 guarded targets. The failures are explicit missing-resource
or certification gates for disposable SQLx URLs, MinIO/PITR, external
directory credentials, approved protobuf provenance, physical device/scale
evidence, or production approval; no application assertion failure was
observed.

QUEUED: credentialed Generic LDAP/RFC 4533, on-prem Microsoft AD DS/DirSync,
and UCS Provisioning Service acceptance; UCS official license/subscription;
credentialed runtime databases/object storage; approved descriptor/hash;
physical WAL/PITR, device, and measured one-million-user evidence; and explicit
production approval. No customer directory activation, command-owner switch,
Folio write retirement, or production cutover occurred. G0–G14 remain active
and are not claimed complete.

## Entry 2026-07-30 — parallel completion hardening and serialized verification

PASS: integrated the directory authentication/synchronization/RBAC wave,
G3–G6 support/backup hardening, G7–G8 HR/Finance persistence fixes, G9–G11
bounded projection/high-watermark fixes, and G12–G14 console/read-boundary
fixes. Coordinator-owned integration added the public projection bounds and
tenant-read migration 0044 for holding-scoped Marketing delivery cursors.
The HR transition SQL parameter alignment, verified-TOTP acceptance fixture,
strict manager-chain fixture, and Sales authoritative workflow fixture were
repaired without weakening fail-closed behavior.

PASS: `cargo fmt --all --check`; workspace Clippy with `-D warnings`; exact
workspace tests; `cargo test -p erpx-proto --offline`; G13 parallel 14/14 and
deployment recovery 6/6; G14 local 11/11; Dioxus console web/desktop/mobile,
iOS, Android plus shared UI iOS/Android; Folio lint/typecheck; and port-3004
probes all passed. Fresh disposable-database target reruns passed local
directory 1/1, directory RBAC 2/2, HR envelope 4/4, Marketing checkpoint 1/1,
Sales payment/checkpoint, control-plane replay 3/3, Support 1/1, Customer 360
1/1, and Operations completion 2/2. Four existing Folio n8n lint warnings
remain; no production Next.js build was run. All run-specific disposable
databases were explicitly dropped after verification.

QUEUED: the final aggregate ignored sweep exits 101 at six targets. One is
the missing credentialed live MinIO/backup environment; one is the deliberate
G14 external certification preflight reporting missing Generic LDAP,
AD DS/DirSync, and UCS Provisioning Service references; four are shared
fixed-fixture/database collisions in the aggregate invocation and passed as
independent fresh-database targets. Remaining certification blockers are the
credentialed vendor environments and native-feed evidence, UCS license/
subscription, approved protobuf descriptor baseline, measured external
one-million-user/five-minute revocation, physical PITR/mobile execution, and
explicit production approval. No activation, owner switch, Folio write
retirement, or production cutover occurred; G0–G14 remain active.

## Entry 2026-07-30 — final defect repair and certification audit

PASS: Operations SLA notification claiming is now correctly holding-scoped and
binds worker identity/claim token to the correct SQL parameters; fresh ignored
Operations persistence passed 9/9. The SQLx dead-letter replay fixture uses an
event-specific idempotency key; fresh control-plane replay passed 3/3.

PASS: the exact ignored workspace invocation ran against fresh shared control
and tenant databases. The 11 target failures were configuration/pollution
artifacts or explicit guards; dedicated reruns passed directory RBAC 2/2,
local directory SQLx 3/3, Marketing scheduler 1/1, Support persistence 8/8,
provisioning 4/4, Operations projector 1/1, Support attachment expiry 1/1,
and Customer 360 1/1. Protobuf, G13 parallel 14/14, G13 recovery 6/6, G14
local 11/11, G14 directory contracts 10/10, all Dioxus targets, Folio
lint/typecheck, and port-3004 probes passed. Four existing n8n lint warnings
remain; no production Next.js build ran.

QUEUED: credentialed live PostgreSQL/MinIO backup certification and the
fail-closed G14 external preflight remain unavailable because Generic
LDAP/RFC 4533, on-prem AD DS/DirSync, and UCS Provisioning Service references
are absent. Approved protobuf provenance, measured external one-million-user/
five-minute vendor evidence, physical PITR/device execution, and explicit
production approval remain open. No directory activation, command-owner
switch, Folio write retirement, or production cutover occurred; G0–G14 remain
active and are not claimed complete.

## Entry 2026-07-30 — live certification recheck and volume evidence

PASS: `cargo test -p erpx-identity --test directory_volume --offline --
--nocapture --test-threads=1` passed 5/5, including the one-million-user
bounded restart/replay path, local break-glass/MFA checks, exact five-minute
revocation deadlines, native cursor continuity, and append-only role-fact
projection. Generic LDAP live transport passed 2/2 and AD DS/DirSync plus UCS
vendor live/readiness targets passed 10/10.

QUEUED: the live targets printed exact missing references for credentialed
Generic LDAP, AD DS/DirSync, and UCS Provisioning Service environments, so no
vendor certification was claimed. Docker/Colima has no usable directory
service running; live PostgreSQL/MinIO backup credentials are also absent. No
activation, ownership switch, Folio write retirement, or production cutover
occurred; G0–G14 remain active.

## Entry 2026-07-30 — Operations claim regression

PASS: fresh PostgreSQL write/read SQLx acceptance
`sla_notification_claims_are_holding_scoped_and_persist_worker_identity`
passed 1/1. It verifies cross-holding isolation and that persisted
`claimed_by`/`claim_token` exactly match the worker claim returned by the
authoritative store. Formatting and warnings-denied focused Clippy passed.

## Entry 2026-07-30 — post-regression serialized verification

PASS: current-tree `cargo fmt --all --check`, workspace warnings-denied
Clippy, and the complete non-ignored workspace test suite passed. The full
Operations ignored SQLx target passed 10/10 on fresh write/read databases.
Folio lint/typecheck passed with four existing n8n warnings; all disposable
databases were removed and port-3004 probes returned 200/307/307 for `/`,
`/platform`, and `/operations`. No production build or cutover ran.

QUEUED: credentialed Generic LDAP, AD DS/DirSync, UCS Provisioning Service,
and live PostgreSQL/MinIO backup certification inputs remain unavailable.

## Entry 2026-07-30 — G13 target-worker optimistic-fence repair and final verification

PASS: G13 checkpoint persistence now enforces `expected_run_version` while
locking the authoritative migration row. The target worker reads the current
lifecycle version before checkpoint and reconciliation/rollback evidence writes
instead of hard-coding version 1. A fresh two-database SQLx acceptance passed
1/1, including lifecycle version 2, stale-version rejection, idempotency,
read-model projection, and checkpoint outbox evidence.

PASS: final current-tree `cargo fmt --all --check`, workspace Clippy with
warnings denied, complete non-ignored workspace tests, protobuf tests, G13
parallel 14/14 and recovery 6/6, G14 local 11/11, G14 directory 10/10, all
seven Dioxus console/shared-UI target checks, console/shared-UI focused tests,
Folio `bun run lint`, `bunx tsc --noEmit`, and port-3004 probes passed. The
lint result remains four pre-existing `infra/scripts/start-n8n.js` warnings;
no production Next.js build ran. Disposable databases were removed.

QUEUED: the exact aggregate ignored workspace sweep exits 101 at 80 guarded
targets because disposable SQLx URLs, credentialed MinIO/backup runtime,
Generic LDAP/RFC 4533, on-prem AD DS/DirSync, UCS Provisioning Service,
approved protobuf provenance, physical PITR/device/scale evidence, and
production approval are absent. Dedicated local G13 target acceptance remains
green, while external vendor certification, measured external one-million-user
and five-minute revocation evidence, and production cutover remain unclaimed.
No customer directory activation, command-owner switch, Folio write retirement,
or production cutover occurred; G0–G14 remain active and are not claimed 100%.

## Entry 2026-07-30 — G13 source-fence binding repair and current-tree verification

PASS: hardened `SqlxG13MigrationStore::enqueue_work` and
`enqueue_work_command`. A queued fence must be a 64-hex SHA-256 value and must
equal the locked authoritative `migration_runs.source_contract_hash` before an
existing-item replay is accepted, an idempotency reservation is made, or a new
work row is inserted. This prevents an authenticated caller or recovery path
from queueing work that cannot execute against the immutable Folio snapshot.
Added control migration 0065 with the direct-SQL `source_fence` check, updated
the durable fixtures to use the run contract hash, and added mismatch plus
no-side-effect assertions for both enqueue APIs. Removed the duplicate
`work_kind` field from the Folio bridge reconcile event hash.

PASS: fresh two-database `g13_migration_persistence` SQLx acceptance passed
1/1 after the final edit. Final current-tree `cargo fmt --all --check`,
workspace Clippy with warnings denied, and complete non-ignored workspace
tests passed. Focused protobuf, G13 parallel 14/14, G13 recovery 6/6, G14
local 11/11, G14 directory 10/10, G14 readiness 9 passed plus one explicitly
ignored live preflight, directory volume 5/5 including one million users,
vendor contract/readiness 10/10, console/shared-UI tests, and all seven
Dioxus target checks passed. Folio `bun run lint` and `bunx tsc --noEmit`
passed; lint retains four pre-existing n8n warnings. Folio port 3004 returned
200 for `/` and 307 for `/platform` and `/operations`. No disposable database
remained afterward and no production Next.js build ran.

QUEUED: the exact aggregate ignored workspace sweep was rerun and exited 101
with 80 guarded targets. Every reported failure was an explicit missing
disposable SQLx URL, credentialed backup/MinIO runtime, Generic LDAP/RFC 4533,
on-prem AD DS/DirSync, UCS Provisioning Service, or other external/device/
approval input; the dedicated fresh SQLx acceptance remained green. External
vendor certification, measured external one-million-user/five-minute
revocation evidence, approved protobuf provenance, physical PITR/device
execution, and explicit production approval remain open. No customer
directory activation, command-owner switch, Folio write retirement, or
production cutover occurred; G0–G14 remain active and are not claimed 100%.

## Entry 2026-07-30 — UCS full-import terminal boundary and current-tree verification

PASS: UCS full-import transport now converts an authoritative empty
Provisioning Service queue (`204 No Content` or JSON `null`) into a terminal
snapshot page. The normal change-feed path retains `NoMessage` retry behavior.
The synthetic terminal page advances only the provider sequence, carries no
records, has no confirmation operation, and lets the durable worker move the
import checkpoint to `Finalizing`. Live-page and durable-checkpoint regression
tests both passed.

PASS: the current worker suite passed 341 tests with two documented ignored
database guards. The corrected G14 source marker made the directory acceptance
pass 10/10. Serialized verification passed for formatting, workspace
warnings-denied Clippy, the complete non-ignored workspace suite, protobuf,
all five Dioxus target checks, console/UI focused tests, Folio lint/typecheck,
and port-3004 health (`200/307/307`). Folio lint reported zero errors and the
four existing `start-n8n.js` warnings. No production Next.js build ran.

QUEUED: the exact ignored workspace sweep remains 80 fail-closed targets,
covering unavailable disposable SQLx/backup resources, credentialed Generic
LDAP/RFC 4533, on-prem AD DS/DirSync, UCS Provisioning Service, approved
protobuf provenance, physical PITR/device/scale evidence, and production
approval. The live vendor harnesses queue missing references and make no
certification claim. No customer directory activation, command-owner switch,
Folio write retirement, or production cutover occurred; G0–G14 remain active
and are not claimed 100% complete.

## Entry 2026-07-30 — authenticated Platform role-review read surface

PASS: added the typed `/platform/role-review` route to the shared route
registry, bilingual navigation, console route mapping, and Platform Operations
adapter evidence. The console resource calls the authoritative
`RoleLifecycleService::ListRoleReview` RPC with the existing authenticated
query context and transport actor/session metadata. Its read boundary validates
the evaluated/horizon timestamps, requires the nested authoritative assignment,
rejects unsafe required fields and redacted values, and bounds the response to
1,000 review items. The UI renders a bounded read-only table and explicit
empty/loading/denied/error states; it does not fabricate data or expose role
renewal/final-role recovery controls.

PASS: `cargo test -p erpx-ui --lib platform -- --nocapture`, console Platform
Operations/QA tests, the role-review wire-boundary regression, and
`cargo test -p erpx-console --bin erpx-console --no-run` passed. `cargo fmt
--all` was applied before the focused checks; full serialized verification is
still required after this packet.

QUEUED: credentialed Generic LDAP/RFC 4533, on-prem AD DS/DirSync, UCS
Provisioning Service, external one-million-user/five-minute revocation
certification, physical PITR/device/scale evidence, and production approval
remain unavailable. No directory activation, command-owner switch, Folio write
retirement, or production cutover occurred.

## Entry 2026-07-30 — role-review packet serialized verification

PASS: after the role-review route/resource/read-model/UI change, current-tree
verification passed `cargo fmt --all --check`, workspace Clippy with warnings
denied, the complete non-ignored workspace suite, `cargo check/test -p
erpx-proto`, Dioxus console web/desktop/host-mobile/iOS/Android checks, G13
parallel 14/14, G13 recovery 6/6, G14 local compatibility 11/11, G14
directory acceptance 10/10, `bun run lint` (zero errors; four pre-existing
n8n warnings), and `bunx tsc --noEmit`. Folio remained healthy on port 3004
(`/` 200, `/platform` 307, `/operations` 307); no production Next.js build
ran.

QUEUED: the exact ignored Cargo sweep exited 101 with 80 fail-closed targets
for absent disposable SQLx runtimes, credentialed backup/MinIO, Generic LDAP,
AD DS/DirSync, UCS Provisioning Service, approved descriptor provenance,
physical PITR/device/scale execution, and production approval. No directory
activation, command-owner switch, Folio write retirement, or production
cutover occurred.

## Entry 2026-07-30 — real local SQLx acceptance refresh

PASS: a fresh disposable PostgreSQL run exercised 34 focused assertions across
18 isolated targets with no mocks: directory SQLx acceptance 3/3, directory
RBAC scope authority 2/2, identity persistence 6/6, Operations authoritative
completion 2/2, Finance persistence 2/2, consolidated trial balance 1/1,
Thai tax projection 2/2, chart persistence 1/1, budget persistence 1/1, CRM
read projection 1/1, marketing delivery checkpoint 1/1, profile erasure 1/1,
marketing delivery persistence 5/5, Support workflow 1/1, backup retention
1/1, tenant registry 1/1, G13 migration persistence 1/1, and role-review
persistence 2/2. The runs used real SQLx migrations and isolated write/read
databases; they passed authoritative mutation, outbox/projector or worker,
read-model, scope, idempotency, and fail-closed/rollback assertions exposed by
each target. All 37 databases created for this refresh were removed and the
host database was left unchanged.

QUEUED: the aggregate ignored sweep still requires unavailable credentialed
Generic LDAP/RFC 4533, on-prem AD DS/DirSync, UCS Provisioning Service,
external one-million-user/five-minute revocation measurements, physical
PITR/device/scale evidence, approved protobuf provenance, and production
approval. No directory activation, command-owner switch, Folio write
retirement, or production cutover occurred; G0–G14 remain active and are not
claimed 100% complete.

## Entry 2026-07-30 — final serialized verification

PASS: `cargo fmt --all --check`, workspace Clippy with `-D warnings`, the
complete non-ignored workspace suite, protobuf check/test, all five Dioxus
target checks, Folio `bun run lint`, and `bunx tsc --noEmit` passed after the
real SQLx refresh. Folio health probes remained `/` 200, `/platform` 307, and
`/operations` 307 on port 3004. Lint had four pre-existing n8n warnings and
zero errors; no production Next.js build ran.

QUEUED: the exact ignored workspace invocation exited 101 with 80 explicit
guarded targets. These are the unconfigured aggregate prerequisites for
disposable SQLx/backup resources, credentialed Generic LDAP/RFC 4533, on-prem
AD DS/DirSync, UCS Provisioning Service, approved protobuf provenance,
physical PITR/device/scale, external one-million-user/five-minute revocation
measurements, and production approval. The dedicated fresh SQLx acceptance
lanes passed and were cleaned up; no directory activation, command-owner
switch, Folio write retirement, or production cutover occurred.

## Entry 2026-07-30 — expanded real SQLx acceptance matrix

PASS: 67 additional assertions passed across 24 isolated fresh PostgreSQL
targets with no mocks. Operations 10/10, Platform 2/2, Support 8/8, Legal
3/3, HR 1/1, Marketing Content 1/1, Sales 4/4, SCM 10/10, Finance Banking
2/2, Finance Subledger 1/1, Finance Close persistence 2/2, Finance Close
projection 1/1, POS 8/8, Quality 1/1, Marketing Journey execution 2/2, CRM
persistence 1/1, Customer 360 1/1, access-audit 1/1, banking import jobs
1/1, provider-account authority 1/1, delivery-provider flow 1/1, tenant
projection scheduling 1/1, control-plane SQLx 3/3, and organization scoping
1/1 all passed. The runs exercised real migrations and authoritative
write/read, event/worker/projector, scope, idempotency, reconciliation, and
rollback/fail-closed paths exposed by each target. All 43 disposable
databases were removed.

QUEUED: external Generic LDAP/RFC 4533, on-prem AD DS/DirSync, UCS
Provisioning Service, measured one-million-user/five-minute revocation,
approved protobuf provenance, physical PITR/device/scale, and production
approval remain unavailable. No directory activation, command-owner switch,
Folio write retirement, or production cutover occurred.

## Entry 2026-07-30 — post-matrix current-tree verification

PASS: after the expanded acceptance matrix, current-tree formatting,
warnings-denied workspace Clippy, complete non-ignored workspace tests,
protobuf check/test, Dioxus web/desktop/host-mobile/iOS/Android checks, Folio
lint, and TypeScript checking passed. Port-3004 probes remained `/` 200,
`/platform` 307, and `/operations` 307. Folio lint retained four pre-existing
`start-n8n.js` warnings and zero errors; no production Next.js build ran.

QUEUED: the latest exact ignored workspace result remains exit 101 at 80
explicit prerequisite/certification targets. The remaining external inputs
are credentialed Generic LDAP/RFC 4533, on-prem AD DS/DirSync, UCS
Provisioning Service, approved protobuf provenance, measured one-million-user/
five-minute revocation, physical PITR/device/scale, and production approval.
No directory activation, command-owner switch, Folio write retirement, or
production cutover occurred.

## Entry 2026-07-30 — backup and recovery SQLx matrix

PASS: eight additional real PostgreSQL assertions passed: backup retention
reconciliation, backup external-blocker evidence, backup retention-blocker
retry evidence, tenant status checkpointing, tenant projection scheduler
rollback fencing, Finance subledger handoff, Finance budget replay
fail-closed behavior, and Finance chart projection read state, each 1/1.
The lanes exercised durable control/tenant write-read, blocker/inventory,
worker retry/reconciliation, checkpoint idempotency, and rollback boundaries;
all 14 disposable databases were removed.

QUEUED: credentialed TLS MinIO backup certification remains unavailable; the
running host service was not treated as certified without explicit endpoint,
credentials, namespace, and CA inputs. Generic LDAP/RFC 4533, on-prem AD
DS/DirSync, UCS Provisioning Service, approved provenance, measured
one-million-user/five-minute revocation, physical recovery/scale, and
production approval remain queued. No activation, ownership switch, Folio
write retirement, or production cutover occurred.

## Entry 2026-07-30 — finance, marketing, legal, and asset SQLx wave

PASS: nine additional assertions passed on eight fresh real PostgreSQL
targets: Finance legacy backfill 1/1, Finance provider-account resolution
1/1, Marketing provider onboarding 1/1, public-abuse window 1/1,
consent/suppression fencing 1/1, CRM source-fence rollback 1/1, Legal read
projection 1/1, and Asset/Maintenance persistence 2/2. The tests covered
durable migrations, authoritative writes, outbox/projector state, provider and
scope fences, idempotency, rollback, and fail-closed behavior. All 12
disposable databases were removed.

QUEUED: credentialed Generic LDAP/RFC 4533, on-prem AD DS/DirSync, UCS
Provisioning Service, TLS MinIO backup credentials/CA, measured
one-million-user/five-minute revocation, approved provenance, physical
recovery/scale, and production approval remain unavailable. No activation,
ownership switch, Folio write retirement, or production cutover occurred.

## Entry 2026-07-30 — worker and scoped-read SQLx wave

PASS: 14 assertions passed across eight fresh real PostgreSQL targets: role-
review notification failure handling 1/1, Support attachment expiry 1/1,
Customer 360 detail/audit/reconciliation 4/4, POS read projection 1/1, HR
persistence envelopes 4/4, Legal document scope integrity 1/1, Quality scope
integrity 1/1, and Marketing Journey persistence 1/1. The lanes exercised
worker transitions, read-only/security boundaries, outbox envelopes,
scope-drift rollback, cursor/read projections, and retry-safe behavior. All
11 disposable databases were removed.

QUEUED: Operations and Finance candidate activation require dedicated real
admin/template database orchestration. Vendor LDAP/AD DS/UCS, TLS MinIO,
measured scale/revocation, approved provenance, physical recovery, and
production approval remain unavailable. No activation, ownership switch, Folio
write retirement, or production cutover occurred.

## Entry 2026-07-30 — admin/template routing acceptance

PASS: real PostgreSQL admin/template harnesses passed Operations projection
through an activated tenant route 1/1 and Finance subledger candidate
generation activation/checkpoint projection 1/1. Unique control, tenant,
read, and candidate databases were migrated, provisioned, projected, verified
against physical read/checkpoint state, and cleaned up; the corrected
Operations control write/read pair was also removed.

QUEUED: credentialed LDAP/AD DS/UCS certification, TLS MinIO inputs, measured
scale/revocation, approved provenance, physical recovery, and production
approval remain unavailable. No customer directory activation, ownership
switch, Folio write retirement, or production cutover occurred.

## Entry 2026-07-30 — provisioning and G13 acceptance wave

PASS: ten real assertions passed: provisioning replacement, readiness
observations, G13 disposable PostgreSQL import, G13 target import, Sales
source pagination, Marketing delivery scheduler, Finance close-authority
replay, provisioning activation guard, immutable owner-seed registration, and
tenant organization bootstrap, each 1/1. The lanes exercised real
admin/template provisioning, migrations, forced-RLS tenant write/read
separation, G13 import/replay/rollback, cursors, worker scheduling, immutable
identity seeding, and authoritative/read projection state. All databases
created by the wave were removed; pre-existing fixed-name physical fixtures
were preserved.

QUEUED: full physical-provisioning fixture replay remains constrained by
pre-existing fixed database names; no destructive replacement was performed.
Credentialed LDAP/AD DS/UCS, TLS MinIO, measured scale/revocation, approved
provenance, physical recovery, and production approval remain unavailable. No
activation, ownership switch, Folio write retirement, or production cutover
occurred.

## Entry 2026-07-30 — Finance close-control completion

PASS: the final two local Finance close-control targets passed 1/1 each on
fresh tenant-write databases: missing-outbox replay fail-closed behavior and
scoped immutable source-evidence replay. Both used real migrations and
authoritative persistence; their databases were removed.

QUEUED: the preserved legacy fixed-name physical-provisioning fixture and
external LDAP/AD DS/UCS, TLS MinIO, measured scale/revocation, approved
provenance, physical recovery, and production approval remain outside local
proof. No activation, ownership switch, Folio write retirement, or production
cutover occurred.

## Entry 2026-07-30 — isolated physical-provisioning verification

PASS: the ignored physical-provisioning acceptance now supports test-only
database-name overrides without changing its legacy defaults. A fresh
forced-RLS write/read tenant pair passed the real migration, provisioning, and
separation assertions 1/1; all four disposable databases were removed. The
post-change verification passed formatting, workspace Clippy with warnings
denied, the offline workspace tests, protobuf check/test, Dioxus
web/desktop/mobile/iOS/Android checks, Folio lint, and TypeScript checking.

QUEUED: credentialed Generic LDAP/RFC 4533, on-prem AD DS/DirSync, UCS
Provisioning Service, TLS MinIO credentials/CA, measured one-million-user and
five-minute revocation certification, approved provenance, physical
recovery/scale, and production approval remain unavailable. The full ignored
database sweep remains guarded by those missing inputs. No activation,
ownership switch, Folio write retirement, or production cutover occurred.

The post-change full ignored sweep ran with
`cargo test --workspace --offline --no-fail-fast -- --ignored
--test-threads=1` and exited 101 with exactly 80 guarded failing suites from
absent disposable database, vendor, credential, or certification inputs; no
`erpx_goal_1785357010*` databases remained afterward.

## Entry 2026-07-30 — immutable identity-link read boundary

PASS: directory administration now reads immutable identity links through a
new cursor-bounded `ListDirectoryIdentityLinks` RPC backed by the
authoritative SQLx write-side relation. The server applies authenticated
platform directory-read authorization and audit persistence, exposes only
secret-free immutable-link metadata, and the web console renders the bounded
first page with an explicit truncation state. Fresh SQLx acceptance passed
the two-page ordering/immutability assertions 1/1; G13 protobuf compatibility
passed 14/14, G14 directory acceptance 10/10, warnings-denied workspace
Clippy passed, and the complete non-ignored workspace suite passed.

QUEUED: credentialed Generic LDAP/RFC 4533, on-prem AD DS/DirSync, UCS
Provisioning Service, TLS MinIO credentials/CA, measured one-million-user and
five-minute revocation certification, approved provenance, physical
recovery/scale, and production approval remain unavailable. The latest full
ignored sweep exited 101 with exactly 80 guarded suites and zero
`erpx_goal_1785357011*` database residue. No customer-directory activation,
ownership switch, Folio write retirement, or production cutover occurred.

## Entry 2026-07-30 — native feed transport audit

PASS: the registered worker path uses concrete live transports: `ldap3`
TLS/StartTLS searches with RFC 4533 sync controls and durable cookies, AD DS
DirSync controls with critical deleted-object controls and opaque cookies, and
UCS HTTPS Provisioning Service polling with explicit message confirmation.
The bounded worker intentionally uses RFC 4533 `refreshOnly`, committing each
native cookie with its SQLx page before acknowledgement; the source comment
now records that contract accurately. The worker unit suite passed 343 tests
with two documented ignored database guards.

QUEUED: source-level and local transport tests do not constitute vendor
certification. Credentialed Generic LDAP/RFC 4533, on-prem AD DS/DirSync,
UCS Provisioning Service, measured scale/revocation, and provider-side
failure/restart evidence remain unavailable and non-activatable.

## Entry 2026-07-30 — authoritative journey effects

PASS: production `marketing_journey_execution` now registers the SQLx-backed
decision source. Exact branch predicates use profile attributes read from the
routed tenant write database with scope containment. Email/SMS journey nodes
create deterministic `DeliveryDispatch` records and atomically reserve
delivery idempotency, snapshots, and delivery outbox events with the cursor
advance, so the existing delivery worker/projector path receives durable
queued work. Focused journey worker tests passed 9/9; infrastructure and
worker warnings-denied Clippy passed.

QUEUED: Social/Push/Webhook, SegmentChange, Conversion, and Action nodes remain
explicitly fail-closed pending real provider or aggregate boundaries. No
mock/provider-success path was introduced, and no production activation,
ownership switch, Folio write retirement, or cutover occurred.

## Entry 2026-07-30 — current-tree completion audit and serialized verification

PASS: the coordinator reviewed the latest G0–G14 audit packets and current
source tree for unresolved local P1/P2 findings. No new unblocked local defect
was identified; the older local findings are covered by later role-review,
Marketing callback/delivery, product SQLx, immutable identity-link, and
native-feed integration evidence. Remaining open boundaries are external
vendor certification, physical device/scale/PITR execution, approved
protobuf provenance, and explicit production authorization.

PASS: serialized verification on `codex/erpx-parallel-completion` passed
`cargo fmt --all --check`, warnings-denied workspace Clippy, and the complete
non-ignored workspace suite. The suite included 341 worker tests with two
documented database guards, 105 identity tests, 129 console tests, and the
50.97-second one-million-user bounded directory volume test. Focused protobuf,
G13 parallel 14/14, G13 recovery 6/6, G14 directory 10/10, identity,
console, and all five Dioxus checks passed. Folio lint had zero errors and
four pre-existing `start-n8n.js` warnings; TypeScript passed; port 3004
returned 200/307/307 for `/`, `/platform`, and `/operations`; `git diff --check`
passed; no production Next.js build ran.

QUEUED: the exact mandated ignored workspace command exited 101 with exactly
80 guarded targets. All failures were explicit missing disposable SQLx,
backup/MinIO, AD DS/DirSync, UCS Provisioning Service, approved descriptor,
physical device/scale/PITR, or production-approval references; no application
assertion failure appeared. Exact idle `erpx_goal_*` databases from prior
isolated acceptance runs were removed and catalog cleanup verified. No
customer directory activation, command-owner switch, Folio write retirement,
or production cutover occurred; G0–G14 remain active and are not claimed
complete.

## Entry 2026-07-30 — final current-tree verification

PASS: serialized formatting, warnings-denied workspace Clippy, and the full
non-ignored workspace suite passed after the authoritative journey-effect
packet. Protobuf passed; G13 parallel 14/14 and recovery 6/6; G14 local 11/11
and directory 10/10; console web/desktop/mobile plus iOS/Android; shared UI
host/iOS/Android; Folio lint (zero errors, four pre-existing n8n warnings);
TypeScript; diff hygiene; and port-3004 probes `/` 200, `/platform` 307,
`/operations` 307 all passed. No production Next.js build ran and the dev
server remains running.

QUEUED: the exact ignored workspace command exited 101 with exactly 80 guarded
targets. Missing inputs are disposable SQLx/control databases, live backup/
MinIO, credentialed Generic LDAP/RFC 4533, AD DS/DirSync, UCS Provisioning
Service, approved protobuf provenance, physical PITR/device/scale evidence,
and production approval. Local disposable target tests with inputs passed and
cleaned up; the PostgreSQL catalog has no `erpx_goal_*` residue. No directory
activation, ownership switch, Folio write retirement, or production cutover
occurred; G0–G14 remain active and are not claimed complete.

## Coordinator continuation — durable Marketing journey effects — 2026-07-30

PASS: SegmentChange now mutates the authoritative Marketing audience
aggregate in the journey cursor transaction; profile tags update the
authoritative profile aggregate; Conversion emits an immutable conversion
aggregate keyed by its business reference; and NotifyOwner emits an
immutable internal owner-notification handoff keyed by run/node. Each path
validates tenant scope, persists SQLx state, snapshot/outbox facts, and
replays as an exact no-op. Conversion and owner-notification read rows are
rebuildable through the existing Marketing projector and reconciliation
fence. Fresh disposable PostgreSQL write/read acceptance passed 1/1 for
conversion and 1/1 for owner notification, with both database pairs dropped.

PASS: Social, Push, and Webhook journey channels now resolve only bounded,
authoritative profile attributes (`social:<network>`, `push:device_ref`, or
`webhook:destination_ref`) and construct the existing durable delivery
records; missing contact facts still fail closed. Infrastructure/worker
focused checks and warnings-denied Clippy passed.

QUEUED: InvokeApprovedIntegration remains fail-closed because no authoritative
approved-integration aggregate/provider boundary exists in this tree. Generic
LDAP/AD DS/UCS certification, approved protobuf provenance, physical
device/scale/PITR evidence, live provider acceptance, and explicit
production approval remain external gates. No directory activation,
command-owner switch, Folio write retirement, or production cutover occurred.

## Coordinator verification — durable journey-effects continuation — 2026-07-30

PASS: after the journey-effect changes, serialized `cargo fmt --all --check`,
warnings-denied workspace Clippy, and the complete non-ignored workspace test
suite passed. `erpx-proto` check/test passed; console web, desktop, iOS, and
Android checks passed; shared UI iOS/Android and Android bridge checks passed.
Folio `bun run lint` and `bunx tsc --noEmit` passed, with four pre-existing
`start-n8n.js` warnings and zero lint errors. `git diff --check` passed, the
PostgreSQL catalog has no `erpx_goal_*` residue, and the running Folio bridge
returned `/` 200, `/platform` 307, and `/operations` 307 on port 3004. No
production Next.js build ran.

QUEUED: the exact mandated ignored workspace command exited 101 at exactly 80
explicit fail-closed guards. The guards require disposable SQLx/control
databases, live backup/MinIO credentials, credentialed Generic LDAP/RFC 4533,
AD DS/DirSync, UCS Provisioning Service, approved protobuf provenance,
physical PITR/device/scale evidence, or production authorization. The
remaining local journey gap is `InvokeApprovedIntegration`, which stays
fail-closed until an authoritative approved-integration aggregate and real
provider executor are supplied; no mock success path was added. No customer
directory activation, command-owner switch, Folio write retirement, or
production cutover occurred; G0–G14 remain active and are not claimed
complete.

PASS: final focused protocol and directory checks also passed: G13 parallel
compatibility/registration 14/14, G13 deployment/recovery 6/6, G14 directory
feed contracts 3/3, and immutable directory/RBAC contracts 2/2. These checks
continue to assert one-million-user capacity boundaries, five-minute
revocation timing, immutable identity links, read-only Folio bridging, and
fail-closed external acceptance without fabricating vendor evidence.

## Coordinator continuation — authoritative directory import status — 2026-07-30

PASS: added an authenticated, platform-scoped `GetDirectoryImportStatus`
read boundary. The identity store reads the latest or requested
`directory_imports` checkpoint from SQLx, derives bounded page progress and
protocol kind, and exposes only safe failure categories; opaque cursors,
external IDs, source payloads, and failure text remain server-side. The
protobuf/server path records an audit event and fails closed on invalid scope,
authorization, missing rows, or persistence errors.

PASS: the Dioxus directory administration model validates the one-million-user
bound, monotonic counters, allowed states, and non-empty feed protocol, then
renders read-only checkpoint progress per connector. A connector without an
import renders an empty state; a missing status does not fabricate progress.
Console contract tests and the SQLx acceptance handoff now cover the
persisted checkpoint shape, including the exact one-million-user limit.

PASS: the post-change serialized gates passed formatting, warnings-denied
workspace Clippy, the non-ignored workspace suite, protobuf package tests,
G13 parallel 14/14, G13 recovery 6/6, G14 local compatibility 11/11, G14
directory acceptance 10/10, console web/desktop/host-mobile/iOS/Android,
shared UI iOS/Android, Android bridge, Folio lint/typecheck, and port-3004
probes. Lint has zero errors and four pre-existing n8n warnings.

QUEUED: the mandated ignored workspace sweep again exited 101 with exactly 80
explicit environment/approval guards. The remaining blockers are disposable
SQLx/control/read databases, live backup/MinIO and PITR evidence, credentialed
Generic LDAP/RFC 4533, AD DS/DirSync, UCS Provisioning Service, approved
protobuf provenance, physical device/scale execution, and production
approval. No customer directory activation, command-owner switch, Folio write
retirement, or production cutover occurred; G0–G14 remain active.

## Coordinator verification — directory status fix and final local acceptance wave — 2026-07-30

PASS: the directory import-status SQLx boundary now decodes PostgreSQL `INT4`
page sizes safely before applying the one-million-user limit, validates the
persisted native cursor kind, and returns only bounded progress and safe
failure categories. Fresh isolated PostgreSQL acceptance passed the API/native
worker handoff (1/1), local G14 directory import (1/1), directory RBAC scope
authority (2/2), and identity persistence (6/6). Fresh isolated infrastructure
and worker lanes also passed the catalog, commercial semantic/read, finance
legacy, scheduler, dead-letter, support, and worker recovery targets recorded
by the coordinator; shared-DB failures were fixture contamination and were
not treated as application evidence.

PASS: serialized current-tree verification passed `cargo fmt --all --check`,
warnings-denied workspace Clippy, and `cargo test --workspace --offline
--no-fail-fast -- --test-threads=1`. Focused protobuf, G13 parallel 14/14,
G13 deployment/recovery 6/6, G14 local 11/11, G14 directory 10/10, all
console Dioxus web/desktop/host-mobile/iOS/Android targets, shared UI
host/iOS/Android targets, and the Android bridge target passed. Folio
`bun run lint` had zero errors and four pre-existing `start-n8n.js` warnings;
`bunx tsc --noEmit` passed. Port 3004 remained healthy with `/` 200,
`/platform` 307, and `/operations` 307. No production Next.js build ran.

QUEUED: the exact mandated ignored workspace command exited 101 with exactly
80 guarded targets because its disposable SQLx, live TLS MinIO/CA, approved
protobuf descriptor, physical PITR/device/scale, vendor-directory, and
production-approval inputs are absent. The host MinIO process is reachable
over plain HTTP, which the backup adapter correctly rejects; no default or
unapproved credential was promoted to TLS evidence. Generic LDAP/RFC 4533,
on-prem AD DS/DirSync, and UCS Provisioning Service credentials/endpoints are
also absent, so no vendor certification or directory activation occurred.
`InvokeApprovedIntegration` remains fail-closed pending an authoritative
approved-integration aggregate and real provider executor; no mock success
path was added. All explicitly created `erpx_goal_*` PostgreSQL databases
were dropped and catalog cleanup verified. G0–G14 remain active and are not
claimed 100% complete; no ownership switch, Folio write retirement, or
production cutover occurred.

## Coordinator verification — approved integration current-tree sweep — 2026-07-30

PASS: current-tree serialized `cargo fmt --all --check`, workspace Clippy with
warnings denied, and `cargo test --workspace --offline --no-fail-fast` all
passed. The real isolated Marketing integration SQLx acceptance passed and its
two disposable databases were removed. Protobuf tests, console web/desktop/
iOS/Android and shared UI native checks, Folio lint/typecheck, `git diff
--check`, and port-3004 probes (`/` 200, protected routes 307) passed.

QUEUED: the mandated ignored sweep now exits 101 with exactly 81 explicit
environment/approval guards, including the new Marketing integration SQLx
target. Missing inputs remain disposable SQLx/control databases, live
backup/MinIO TLS credentials, credentialed LDAP/AD/UCS certification,
approved protobuf provenance, physical PITR/device/scale evidence, and
production approval. No cutover, directory activation, owner switch, or Folio
write retirement occurred; G0–G14 remain active.

## Entry 2026-07-30 — isolated SQLx acceptance expansion

Five parallel focused waves ran 40 fresh disposable SQLx targets and passed
111 real assertions across directory RBAC/local directory import and identity
persistence, control-plane/G13/tenant registry, Operations, Customer 360,
Support diagnostics, role review, Finance, Marketing, Legal, Quality, POS,
Sales, SCM, HR, Asset/Maintenance, and journey integration. These were real
PostgreSQL migrations and authoritative write/read paths with forced RLS,
outbox/projector or worker transitions, idempotency, scope fences,
reconciliation, and rollback checks according to each target; no mocks were
used. Every database created by the waves was removed and the current-turn
database-name check is empty.

The mandated exact ignored workspace sweep was rerun on the current tree and
exited 101 with exactly 81 targets. The failures are explicit missing
database/credential/certification prerequisites, including the three real
vendor directory environments, TLS MinIO inputs, approved descriptor,
physical PITR/device/scale, and production approval; no application assertion
failure was observed. G0–G14 remain active. No directory activation,
command-owner change, Folio write retirement, or production cutover occurred.

## Entry 2026-07-30 — final serialized verification

The final current-tree run passed `cargo fmt --all --check`, workspace
warnings-denied Clippy, the complete non-ignored workspace and doc-test suite,
protobuf tests, all required Dioxus console web/desktop/iOS/Android checks,
shared UI host/iOS/Android checks, and the Android bridge. Folio lint and
TypeScript passed with zero errors; the four existing n8n warnings remain.
`git diff --check` passed, and port 3004 remained healthy (`/` 200,
protected routes 307). No production Next.js build ran.

The exact ignored workspace sweep remains an 81-target prerequisite queue.
The unresolved items are external Generic LDAP/RFC 4533, AD DS/DirSync, UCS
Provisioning Service, TLS MinIO credentials/CA, approved descriptor
provenance, physical PITR/device/scale evidence, and production approval.
No directory activation, command-owner change, Folio write retirement, or
production cutover occurred.

## Entry 2026-07-30 — final verification after checkpoint rollback fix

The native-directory audit fixed the full-import checkpoint boundary: staged
pages no longer advance the ordinary connector checkpoint before canonical
finalization, and restart recovery publishes the staged source cursor only
after finalization succeeds. Focused worker and identity tests passed.

The current tree passed the serialized Cargo gate, protobuf, G13 (14/14 and
6/6), G14 (11/11 and 10/10), all required Dioxus target checks, shared UI
native checks, Android bridge, Folio lint/typecheck, diff hygiene, and
port-3004 probes. The exact ignored sweep remains 81 fail-closed prerequisite
or certification guards; no application assertion failed. Vendor LDAP/AD/UCS,
approved descriptor, backup/MinIO, physical PITR/device/scale, and production
approval inputs remain queued. No directory activation, owner switch, Folio
write retirement, or production cutover occurred.

## Entry 2026-07-30 — holding-scoped Directory transport

Implemented the next unblocked G14 packet. DirectoryScope now exposes the
stable holding identity used by the native worker: UUID holding references are
preserved and opaque holding references use UUIDv5 with the URL namespace.
Server Directory RPCs accept platform or matching holding scope, filter
connector reads by authorized scope, fence imports/status/identity links/role
mappings to the connector, and enforce role-mapping scope containment.
Company/branch/subtree contexts fail closed. Canonical `holding:<ref>` and
legacy request forms are parsed with a consistency check.

Evidence: `cargo fmt --all`, 106 identity unit tests, 2 server directory-scope
tests, warnings-denied server Clippy, and the focused G14 transport acceptance
test passed.

Still queued: credentialed Generic LDAP/RFC 4533, AD DS/DirSync, UCS
Provisioning Service, disposable database matrix, TLS MinIO/backup, approved
protobuf descriptor, physical PITR/device/scale, and production approval. No
directory activation, command-owner switch, Folio write retirement, or
production cutover occurred.

## Entry 2026-07-30 — opaque holding RBAC compatibility

Closed a local Directory RBAC edge case discovered during the holding-scope
audit. `DirectoryScope::holding_id()` and `DirectoryScope::permits()` now
bind UUID and opaque holding references to the same immutable UUIDv5 identity.
Shared SQLx authorization reads the persisted mapping scope, normalizes
legacy opaque `holding:<ref>` keys at the authorization boundary, and only
then evaluates platform/holding containment; malformed keys are ignored for
authorization. Connector configuration and append-only role facts retain
their original scope key, avoiding a history rewrite or a duplicate-key
migration while preserving the five-minute stale/revocation fence.

Evidence: `cargo fmt --all`, 107 identity tests, identity warnings-denied
Clippy, server tests, the full G14 migrator acceptance target, fresh
disposable PostgreSQL directory SQLx acceptance (3/3), and fresh disposable
PostgreSQL RBAC scope authority (2/2) passed. The legacy opaque-key assertion
ran against the real write-side schema. Vendor LDAP/AD/UCS, TLS MinIO/backup,
descriptor, physical PITR/device/scale, and production approval inputs remain
queued.

## Entry 2026-07-30 — opaque holding RBAC current-tree verification

PASS: serialized current-tree verification passed `cargo fmt --all --check`,
workspace warnings-denied Clippy, and the complete non-ignored workspace and
doc-test suite. Focused protobuf, G13 parallel/recovery, G14 acceptance,
directory volume 5/5 including the 1,000,000-user bounded test, console/UI,
Android bridge, host web/desktop/mobile, iOS, Android, and UI-Android checks
passed. Fresh disposable PostgreSQL directory SQLx acceptance passed 3/3 and
RBAC scope authority passed 2/2; both explicitly created databases were
removed and catalog residue was checked.

PASS: Folio `bun run lint` returned zero errors with four pre-existing
`start-n8n.js` warnings; `bunx tsc --noEmit` passed. Port-3004 probes remained
`/` 200, `/platform` 307, `/operations` 307. No production Next.js build ran.

QUEUED: the exact ignored workspace command exited 101 with exactly 81
explicit prerequisite/certification guards. The unresolved inputs are
credentialed Generic LDAP/RFC 4533, on-premises AD DS/DirSync, UCS
Provisioning Service, TLS MinIO/backup and PITR evidence, approved protobuf
provenance, physical device/scale evidence, and production approval. No
directory activation, command-owner switch, Folio write retirement, or
production cutover occurred; G0–G14 remain active and are not claimed 100%.

## Entry 2026-07-30 — TLS backup round-trip acceptance (latest)

PASS: `real_postgres_minio_backup_restore_and_cleanup_are_authoritative`
passed 1/1 against a real ephemeral HTTPS MinIO server and disposable
PostgreSQL databases. The run used a generated test CA, real S3-compatible
TLS object transport, PostgreSQL 18.4 `pg_dump`/`pg_restore`, forced-RLS
candidate verification, tenant-marker verification, inventory, and cleanup.
The default PostgreSQL 14 client was rejected by the PostgreSQL 18 host as a
version mismatch; the installed PostgreSQL 18.4 clients then passed. Temporary
databases, certificate material, credentials, and MinIO state were removed;
catalog residue was checked.

QUEUED: approved production CA/credentials, physical PITR/WAL recovery, and
production authorization remain unavailable. This local G6 acceptance does
not authorize production use or cutover. Generic LDAP/RFC 4533, on-premises
AD DS/DirSync, UCS Provisioning Service, approved protobuf provenance,
physical device/scale, and production approval remain queued.

## Entry 2026-07-30 — ignored SQLx matrix isolation (latest)

The complete serialized ignored workspace sweep was rerun with isolated
disposable control-write/read, tenant-write/read, directory RBAC,
local-directory, and Marketing scheduler databases. The aggregate returned
five targets: `backup_live` still lacks the approved live TLS MinIO inputs,
and `g14_external_readiness` intentionally fails closed because the real
Generic LDAP, AD DS/DirSync, and UCS references are absent. The remaining
three aggregate failures were shared-database contamination, not application
failures.

Fresh isolated runs passed `sqlx_control_plane` 3/3, `support_workflow` 1/1,
`customer_360` 1/1, `directory_rbac_scope_authority` 2/2,
`directory_sqlx_acceptance` 3/3, and `marketing_delivery_scheduler` 1/1.
The real TLS MinIO/PostgreSQL backup round-trip passed separately 1/1 with
the installed PostgreSQL 18 clients. The external vendor and production
backup/cutover inputs remain queued; no activation or production cutover
occurred.

## Entry 2026-07-30 — isolated SQLx and current-tree verification (latest)

PASS: serialized `cargo fmt --all --check`, workspace warnings-denied
Clippy, and the complete non-ignored workspace/doc-test suite passed.
Protobuf, G13 parallel/recovery (14/14 and 6/6), G14 directory/local
compatibility (11/11 and 11/11), console/UI suites, Android bridge, and all
requested console web/desktop/mobile/iOS/Android plus shared UI iOS/Android
checks passed. Folio lint returned zero errors with four pre-existing n8n
warnings; TypeScript and `git diff --check` passed. Port 3004 stayed healthy
with `/` 200 and protected routes 307. No production Next.js build ran.

PASS: isolated ignored SQLx reruns passed control-plane 3/3, support 1/1,
Customer 360 1/1, Directory RBAC 2/2, local Directory SQLx 3/3, and
Marketing scheduler 1/1. The real TLS MinIO/PostgreSQL backup round-trip
passed 1/1 separately. All disposable databases created for these runs were
dropped and catalog residue was checked.

QUEUED: Generic LDAP/RFC 4533, AD DS/DirSync, UCS Provisioning Service,
approved protobuf provenance, physical PITR/device/scale evidence, and
production approval remain unavailable. G0–G14 are not claimed 100%; no
directory activation, command-owner switch, Folio write retirement, or
production cutover occurred.

## Entry 2026-07-30 — platform worker registration hardening

The legacy `platform_completion` registration name now delegates to the
existing hardened `PlatformCompletionRuntimeAdapter`, so it cannot bypass the
runtime platform-event actor/request-hash/read-model safety checks or the
control write/read boundary. `platform_completion_runtime` remains available
as the explicit runtime name; deployment must select one alias only.

Evidence: worker platform-completion tests 10/10, registry test 1/1,
warnings-denied worker Clippy, formatting, and diff hygiene passed. No live
worker or database was started by this packet. External LDAP/AD/UCS,
approved descriptor, TLS backup/PITR, device/scale, Folio migration, and
production approval inputs remain queued; no activation, owner switch, Folio
write retirement, or cutover occurred.

## Entry 2026-07-30 — current-tree verification after platform alias hardening

PASS: serialized `cargo fmt --all --check`, workspace warnings-denied Clippy,
and the complete non-ignored workspace/doc-test suite passed. Focused
protobuf, G13 parallel/recovery (14/14 and 6/6), G14 directory/local
compatibility (11/11 and 11/11), console web/desktop/mobile/iOS/Android,
shared UI iOS/Android, Folio lint/typecheck, `git diff --check`, and runtime
probes passed. Folio remained `/` 200 and protected routes 307 on port 3004;
no production Next.js build ran.

QUEUED: no approved protobuf baseline, credentialed Generic LDAP/RFC 4533,
AD DS/DirSync, UCS Provisioning Service, physical PITR/device/scale evidence,
or production approval is available. G0–G14 are not claimed 100%; no
directory activation, command-owner switch, Folio write retirement, or
production cutover occurred.

## Entry 2026-07-30 — billing-policy worker current-tree verification

PASS: after the billing-policy worker packet, serialized `cargo fmt --all
--check`, workspace warnings-denied Clippy, and the complete non-ignored
workspace/doc-test suite passed. The exact ignored invocation
`cargo test --workspace --offline --no-fail-fast --quiet -- --ignored` was
rerun and exited 101 with exactly 81 explicit missing-resource or
certification guards; no application assertion failed. Focused protobuf,
G13 parallel/recovery (14/14 and 6/6), G14 local/directory (11/11 and 11/11),
all console web/desktop/host-mobile/iOS/Android, shared UI host/iOS/Android,
Android bridge, Folio lint/typecheck, `git diff --check`, and port-3004 probes
passed. Folio lint had zero errors and four pre-existing n8n warnings; no
production Next.js build ran.

PASS: the new billing-policy worker ignored SQLx acceptance passed 1/1 with
fresh disposable PostgreSQL databases, and the existing infrastructure batch
boundary passed 1/1. Temporary databases were removed. QUEUED: credentialed
Generic LDAP/RFC 4533, AD DS/DirSync, UCS Provisioning Service, approved
protobuf provenance, TLS MinIO/production backup and physical PITR, physical
device/measured-scale evidence, live Stripe acceptance, and production
approval remain unavailable. G0–G14 are not claimed 100%; no activation,
command-owner switch, Folio write retirement, or production cutover occurred.

## Entry 2026-07-30 — entitlement command runtime and console

G5 entitlement allocation and activation now use authoritative command wrappers
for company allocation, company activation, and branch activation. Each
command locks the aggregate, checks an optional expected version, reserves
commercial idempotency, persists the aggregate snapshot, typed state event /
outbox record, append-only audit, and receipt atomically through SQLx. Replay
returns the original receipt; stale versions fail without mutation. The server
EntitlementService methods enforce authenticated actor, customer/scope,
subscription lifecycle, module entitlement, effective role chain, SoD, and
company/branch containment. The console has a Billing & Entitlements surface
for the authorized read/evaluation and all three commands.

Evidence: infrastructure active commercial tests 29/29, fresh disposable
PostgreSQL command acceptance 1/1, server tests 94/94, focused console desktop
check, and warnings-denied server/infrastructure Clippy passed. Full target,
ignored, Folio, and port-3004 verification is rerun after this entry.

QUEUED: Stripe/catalog production reconciliation, credentialed Generic
LDAP/RFC 4533, AD DS/DirSync, UCS Provisioning Service, approved protobuf
provenance, physical PITR/device/scale evidence, and production approval.
No directory activation, command-owner switch, Folio write retirement, or
production cutover occurred.

## Entry 2026-07-30 — entitlement MFA boundary and final verification

Entitlement allocation, company activation, and branch activation now require
a fresh server-verified one-time MFA step-up. The code is transport-only: it
does not enter command context, idempotency hashes, audit, outbox, receipts,
errors, or UI status. The console exposes the high-risk step-up input and
continues to use explicit scope, expected versions, and durable receipts.

PASS: current-tree serialized formatting, warnings-denied workspace Clippy,
non-ignored workspace/doc-test suite, protobuf, G13 14/14 plus recovery 6/6,
G14 local 11/11 plus directory 11/11, all console and shared UI target
checks, Android bridge, Folio lint/typecheck, diff hygiene, and port-3004
probes passed. The exact ignored sweep remains an 81-target fail-closed
resource/certification queue; no application assertion failure was observed.

QUEUED: credentialed LDAP/AD/UCS, approved protobuf descriptor, TLS MinIO and
physical PITR/device/scale, live Stripe/catalog reconciliation, and production
approval. No activation, command-owner switch, Folio write retirement, or
production cutover occurred.

## Entry 2026-07-30 — final serialized local verification

PASS: `cargo fmt --all --check`, workspace warnings-denied Clippy, and the
complete non-ignored workspace test suite passed. `cargo test -p erpx-proto`
including doc tests passed. G13 protobuf/migration compatibility passed 6/6,
parallel acceptance 14/14, and recovery 6/6. G14 local descriptor/wire
compatibility passed 11/11 and directory contract acceptance passed 11/11.
All requested console Dioxus web/desktop/host-mobile/iOS/Android checks,
shared UI host/iOS/Android checks, and Android bridge checks passed.

PASS: Folio `bun run lint` returned zero errors and four pre-existing n8n
warnings; `bunx tsc --noEmit` passed. Runtime probes stayed healthy on port
3004 (`/` 200, `/platform` 307, `/operations` 307). No production Next.js
build ran.

QUEUED: the exact ignored workspace invocation exited 101 with exactly 81
explicit missing-resource or certification guards and no application
assertion failure. Credentialed Generic LDAP/RFC 4533, AD DS/DirSync, UCS
Provisioning Service, disposable PostgreSQL URLs for the remaining SQLx
acceptance matrix, approved protobuf descriptor provenance, live TLS
MinIO/physical PITR, device secure-storage, measured-scale, live
Stripe/provider credentials, and production approval remain unavailable.
G0–G14 remain active and unclaimed at 100%; no directory activation,
command-owner switch, Folio write retirement, or production cutover occurred.

## Entry 2026-07-30 — current coordinator audit

PASS: final formatting, workspace warnings-denied Clippy, complete
non-ignored workspace tests, protobuf/docs, G13 recovery/parallel 6/6 and
14/14, G14 local/directory 11/11 and 11/11, all requested Dioxus console and
shared-UI target checks, Android bridge, Folio lint/typecheck, and port-3004
probes passed. The exact ignored sweep returned exactly 81 explicit
missing-resource or certification guards and no unexpected assertion failure.

PASS: isolated local SQLx evidence includes identity 6/6, RBAC 2/2, directory
3/3, Finance consolidation 2/2, Marketing journey 6/6, delivery 5/5,
scheduler 1/1, HR 5/5, Sales 4/4, SCM 10/10, POS 9/9, Quality 2/2 plus scope
1/1, Finance controls 10/10, and Operations slices. Local PostgreSQL was
non-TLS-only and is not production or vendor certification evidence.

QUEUED: external LDAP/AD/UCS environments and credentials, remaining SQLx
database URLs, approved protobuf provenance, TLS MinIO/physical PITR,
device/scale evidence, live provider credentials, G13 runtime references, and
production approval. G0–G14 remain active; no activation, command-owner switch,
Folio write retirement, or production cutover occurred.

## Entry 2026-07-30 — final coordinator verification

IMPLEMENTED: bounded directory TLS freshness plus trust-anchor pinning, UCS
sequence/RFC 4533 terminal-feed correctness, fail-closed directory RBAC scope
and revocation checks, and platform tenant/holding lifecycle validation.

VERIFIED: formatting, warnings-denied workspace Clippy, non-ignored workspace
tests, protobuf/G13/G14 compatibility, Dioxus web/desktop/Android/iOS and
Android bridge checks, Folio lint/typecheck, `git diff --check`, and port 3004.
The exact ignored sweep returned 81 explicit environment/certification guards;
no unexpected assertion failure was observed.

QUEUED: credentialed Generic LDAP, AD DS, UCS, remaining SQLx databases,
approved protobuf provenance, TLS MinIO/physical PITR, device/scale/provider
evidence, G13 runtime references, and production approval. No activation,
command-owner switch, Folio write retirement, or production cutover occurred.

## Entry 2026-07-30 — final parallel lane receipt

IMPLEMENTED: bounded directory TLS certificate/server-name evidence and late
bind deadline enforcement; source/outbox locking for Marketing content
projection rebuilds; Operations source-owned outbox fences and fail-closed
JSON cursor validation; unique accessible G12 panel/dialog IDs; structured
Generic LDAP revocation and immutable-link evidence; real G13 disposable-DB
acceptance harness; G14 scale/cursor/immutability evidence; and native feed
attribute/checkpoint validation for RFC 4533, AD DirSync, and UCS.

IMPLEMENTED: Finance banking replay/source fences, transaction digest and
payload integrity checks, budget/chart validation, read reconciliation, and
rollback-safe worker projection behavior. These remain authoritative SQLx,
outbox, worker/projector, read-model, authorization, API/UI, and test paths;
no mocks or fake providers were introduced.

VERIFIED: clean workspace tests, format, warnings-denied Clippy, protobuf and
G13/G14 compatibility, console/shared UI web/desktop/mobile/iOS/Android
checks, Android bridge, Folio lint/typecheck, diff hygiene, and port 3004.
Ignored verification returned exactly 81 explicit environment/certification
guards; all disposable ERPX databases were cleaned up.

QUEUED: external LDAP/AD/UCS credentials and endpoints, remaining SQLx URLs,
approved protobuf descriptor, TLS MinIO/physical PITR, device/scale/provider
evidence, G13 runtime references, and production approval. G0–G14 are not
claimed 100%; no activation, command-owner switch, Folio write retirement, or
production cutover occurred.

## Entry 2026-07-30 — isolated SQLx completion audit

IMPLEMENTED: repaired the Thai-tax live fixture to persist the authoritative
fact row and its append-only outbox event before projection. The acceptance
now proves SQLx source/read reconciliation, idempotent projection, drift
detection, rebuild, and rollback behavior against the actual source contract.

VERIFIED: isolated local database evidence passed identity/RBAC, G14 local
directory handoff and role mapping, Finance/Marketing/Operations/HR/Legal/
SCM/POS/Quality/Sales/Support paths, backup retention, role review, Stripe
semantic workers, Customer 360, access audit, banking import, operations
projector/scheduler, and support attachments/workflow. The final workspace
tests, format, warnings-denied Clippy, and `git diff --check` passed. Temporary
databases were explicitly cleaned up, leaving zero `erpx_goal26_*` databases.

QUEUED: the mandated aggregate ignored command exits 101 with exactly 81
explicit missing-resource or certification guards when no URLs are supplied.
Remaining real certification inputs are credentialed Generic LDAP/RFC 4533,
AD DS/DirSync, UCS Provisioning Service, live MinIO/PITR backup, approved
protobuf descriptor provenance, physical device/measured-scale/provider
evidence, and production approval. No guard was treated as a pass; G0–G14
remain active and unclaimed at 100%, and no production state changed.

## Entry 2026-07-30 — current G14 readiness recheck

VERIFIED: G14 external-readiness static coverage passed 9/9. The ignored
preflight failed closed with the exact absent references for Generic LDAP/RFC
4533, on-premises AD DS/DirSync, and UCS Provisioning Service, including
database URLs, endpoints, bind credentials, CA/server-name trust, full-import
limits, native revocation targets, and evidence paths. This is a queued
certification result, not a vendor pass.

QUEUED: no external directory resources or credentials have appeared, and
backup/MinIO, approved descriptor, physical/device/scale/provider, and
production-approval evidence remain unavailable. Folio port 3004 remains
healthy; no production state changed.

## Entry 2026-07-30 — real G13 disposable runtime

VERIFIED: the local PostgreSQL G13 disposable acceptance passed 1/1 with an
explicit admin URL. The test created six fresh databases, ran real source
extraction, control/tenant migrations, target import, read projection,
reconciliation, rollback, outbox/idempotency and Folio-write-fence checks,
then dropped all temporary databases. It did not switch command ownership or
touch production/Folio state.

QUEUED: G13 runtime is locally evidenced; external G14 directory certification,
live backup/MinIO/PITR, approved protobuf descriptor, physical/device/scale/
provider evidence, and production approval remain unavailable.

## Entry 2026-07-30 — directory transport audit

VERIFIED: real production transport wiring was re-audited: rustls-only LDAP/BER
for Generic LDAP/RFC 4533 and AD DS/DirSync, plus UCS HTTPS Provisioning
Service, all flow through bounded worker adapters into SQLx persistence and
outbox/projector paths. Feed/volume contracts passed 3/3, vendor adapter
harnesses 10/10, Generic LDAP harnesses 9/9, and the current directory
contracts cover one-million-user bounds, immutable identity linking,
directory-managed roles, append-only revocation, five-minute SLA, TLS-only
transport, and break-glass-only fallback.

QUEUED: live vendor environments, credentials/trust anchors, provider-side
revocation handoffs, measured external scale, and certification evidence remain
required; no static or local result was promoted to vendor acceptance.

## Entry 2026-07-30 — real TLS MinIO backup acceptance

PASS: `real_postgres_minio_backup_restore_and_cleanup_are_authoritative`
passed 1/1 against a disposable HTTPS MinIO server signed by a generated test
CA and local PostgreSQL. The acceptance used real S3-compatible object
transport, PostgreSQL 18 `pg_dump`/`pg_restore`, forced-RLS candidate
verification, tenant-marker verification, inventory, artifact cleanup, and
disposable database cleanup. The initial CA-as-server certificate was correctly
rejected by rustls; the corrected CA-signed server certificate then passed.
Temporary MinIO state and certificate material were moved to the OS trash, no
`erpx_goal26_*` databases remain, and the unrelated existing `erpx_bk_manual`
database was not touched.

QUEUED: approved production MinIO/TLS credentials, physical PITR/WAL recovery,
credentialed Generic LDAP/RFC 4533, on-premises AD DS/DirSync, UCS
Provisioning Service, approved protobuf descriptor provenance, physical
device/scale/provider evidence, and production approval. No activation,
command-owner switch, Folio write retirement, or production cutover occurred.

## Entry 2026-07-30 — physical PITR capture/restore acceptance

IMPLEMENTED: the worker now reaches the real physical PITR capture adapter for
persisted `pitr` runs only when explicit WAL-archive configuration is present;
otherwise it returns a fail-closed configuration error. Physical restore stays
unregistered in `apps/worker/src/main.rs` pending coordinator approval. The
capture adapter waits for the target-LSN WAL segment within the configured
bounded timeout before publishing its manifest. Restore extraction now
hardens PostgreSQL directory permissions, safely appends recovery settings to
an existing private `postgresql.auto.conf`, and validates/creates the regular
`recovery.signal` marker.

PASS: `real_postgres_physical_pitr_capture_restore_and_cleanup_are_authoritative`
passed 1/1 with a real isolated PostgreSQL 18 WAL-archiving cluster and
credentialed HTTPS MinIO signed by a generated CA. The acceptance used
explicit PostgreSQL 18 `pg_basebackup`, `pg_ctl`, and `psql` binaries, real
base/WAL/manifest object upload and read-back, PostgreSQL recovery verification,
artifact deletion, and source-database cleanup. No mock process, object store,
or recovery executor was used; no isolated service remained running after the
run.

QUEUED: local physical-PITR evidence does not satisfy approved production
restore-target authorization or production backup credentials. Generic
LDAP/RFC 4533, on-premises AD DS/DirSync, UCS Provisioning Service, approved
protobuf descriptor provenance, physical/device/scale/provider evidence, and
production approval remain queued. G0–G14 remain active and unclaimed at
100%; no directory activation, command-owner switch, Folio write retirement,
or production cutover occurred.

## Entry 2026-07-30 — serialized completion verification after PITR

PASS: after the PITR changes and warnings-denied cleanup, `cargo fmt --all
--check`, workspace Clippy with `-D warnings`, the complete non-ignored
workspace suite, and protobuf check/tests passed. Console Dioxus web, desktop,
host-mobile, aarch64-apple-ios, and aarch64-linux-android checks passed;
shared UI host/iOS/Android and the Android bridge target passed. Folio `bun
run lint` had zero errors and four pre-existing n8n warnings, `bunx tsc
--noEmit` passed, and `git diff --check` passed. Port 3004 remained healthy
with `/` 200, `/platform` 307, and `/operations` 307.

QUEUED: the exact no-environment ignored workspace sweep exited 101 with 82
explicit fail-closed resource/certification guards. The queue includes
disposable SQLx URLs, credentialed Generic LDAP/RFC 4533, on-premises AD
DS/DirSync, UCS Provisioning Service, backup/PITR credentials, approved
protobuf provenance, and physical device/scale/provider evidence. No
unexpected application assertion was observed; no directory activation,
command-owner switch, Folio write retirement, or production cutover occurred.

## Entry 2026-07-30 — approved integration audit correction

VERIFIED: `InvokeApprovedIntegration` is implemented and registered through
the holding-scoped SQLx control-plane binding boundary, durable idempotent
handoff, multi-tenant worker, projection/reconciliation path,
secret-manager credential resolver, HTTPS-only provider route, and
invoke/reconcile handling. Its no-environment ignored failures are queued
SQLx/provider certification prerequisites; no mock success path was added.

## Entry 2026-07-30 — directory, retention, G13, and UI safety continuation

IMPLEMENTED: Generic LDAP/RFC 4533, AD DS/DirSync, and UCS ingestion now
enforce committed native cursor advancement; AD terminal cookies are retained,
UCS empty queues fail closed, and RFC 4533 full imports persist bounded
composite synchronization/paged-results cursors. Directory RBAC persistence
uses typed immutable identity-linked facts and matching role outbox events,
including opaque holding scope normalization and serialized role-chain tails.
Import rollback preserves both the initiating and rollback errors.

IMPLEMENTED: platform outbox claims are aggregate-family filtered and duplicate
platform handler registration is gone. G13 claims are phase-fenced and phase
transitions reject unfinished work; rollback rehearsal transitions require
successful persisted source-preserved/read-model-rebuilt evidence. PITR
retention now selects physical artifacts explicitly, inventories and deletes
bounded exact PITR prefixes, and never routes them through logical `.dump`
deletion. Projection/restore rollback failures are observable.

IMPLEMENTED: shared dialogs handle Escape and autofocus the close control;
console surface navigation now emits tablist/tab/tabpanel semantics. Focused
identity tests passed 116/116, worker tests passed 355 with 3 ignored, UI tests
passed 110/110, and console/infrastructure focused compilation passed.

QUEUED: the current exact ignored workspace sweep remains exit 101 with 82
explicit fail-closed resource/certification guards. External LDAP/AD/UCS
servers and credentials, measured 1M-user vendor scale, approved protobuf
descriptor provenance, production backup/PITR targets, physical device/scale/
provider evidence, and production approval are still required. No static or
local result was promoted to vendor acceptance; no directory activation,
command-owner switch, Folio write retirement, or production cutover occurred.

## Entry 2026-07-30 — final local verification and dev-runtime handoff

PASS: the final exact tree passed `cargo fmt --all --check`, workspace Clippy
with `-D warnings`, and the complete non-ignored workspace suite. Protobuf
check/tests, G13 acceptance (15/15), G14 local compatibility (11/11), G14
directory acceptance (12/12), console Dioxus web/desktop/mobile, shared UI
iOS/Android, and Android bridge target checks passed. Folio `bun run lint`
had zero errors and four pre-existing n8n warnings; `bunx tsc --noEmit` and
`git diff --check` passed.

PASS: Folio remains on port 3004. Turbopack's CSS-worker timeout was
reproducible during the handoff, so the local development process was
restarted with Next webpack dev mode without application-code changes; `/`
returns 200 and protected `/platform` and `/operations` return 307.

QUEUED: the exact ignored sweep exited 101 with 82 explicit fail-closed
resource/certification guards and no unexpected application assertion. The
queue includes credentialed Generic LDAP/RFC 4533, on-premises AD DS/DirSync,
UCS Provisioning Service, approved protobuf descriptor provenance, production
backup/PITR authorization, measured 1M-user/device/provider evidence, and
production approval. No static/local result was promoted to vendor acceptance;
no directory activation, command-owner change, Folio write retirement, or
production cutover occurred.
## Entry 2026-07-30 — current-tree certification addendum (EOF)

The current-tree local verification and queued external certification
prerequisites are recorded; no production activation or cutover occurred.
## Entry 2026-07-30 — current-tree evidence at EOF

The integrated local ERPX tree is verified on `codex/erpx-parallel-completion`.
The exact ignored workspace sweep remains an exit-101 queue of 82 fail-closed
external-resource guards. Credentialed Generic LDAP/RFC 4533, on-premises AD
DS/DirSync, UCS Provisioning Service, measured 1M-user vendor-scale evidence,
approved protobuf descriptor provenance, production backup/PITR targets,
device/provider evidence, and explicit production approval remain unavailable.
G0–G14 and vendor acceptance remain open for certification; no activation,
owner switch, Folio write retirement, or production cutover occurred.
## Entry 2026-07-30 — coordinator final current-tree verification

PASS: the stable coordinator snapshot passed formatting, warnings-denied workspace Clippy, the complete non-ignored workspace tests, protobuf tests, G13/G14 acceptance suites, and all requested Dioxus web/desktop/mobile/wasm/iOS/Android, shared UI, and Android bridge checks. Folio lint/typecheck/bridge tests passed (6/6); lint retained four pre-existing n8n warnings. Port 3004 remained healthy with `/` 200 and protected routes redirecting 307.

QUEUED: the exact ignored sweep exited 101 with 82 explicit environment/resource guards. Missing evidence remains credentialed Generic LDAP/RFC 4533, on-premises AD DS/DirSync, UCS Provisioning Service, disposable SQLx endpoints, approved protobuf provenance, backup/PITR targets, measured 1M-user/device/provider evidence, and production approval. The queue is recorded without converting guards into successes; no activation, owner switch, Folio write retirement, or production cutover occurred.

## Entry 2026-07-31 — coordinator final integration and certification queue

PASS: repaired authoritative POS source identity rotation and the zero-payment
snapshot verifier edge case. Fresh local POS SQLx persistence/projection tests
passed 9/9. Isolated clean local ignored targets passed directory RBAC 4/4,
platform completion 2/2, quality scope integrity 1/1, control-plane SQLx 3/3,
support workflow 1/1, and Customer360 1/1.

PASS: serialized formatter, warnings-denied workspace Clippy, complete
non-ignored workspace tests, protobuf tests, G13/G14 acceptance, all requested
Dioxus targets, Folio lint/typecheck/bridge tests, and port-3004 probes passed.
Lint has four pre-existing n8n warnings and no errors.

QUEUED: the fresh single-threaded aggregate ignored run exited 101 with eight
explicit outcomes: five shared disposable-database fixture collisions,
credentialless backup/PITR live-resource guards, and the expected
credentialless G14 vendor preflight. The five local collisions are covered by
passing clean per-target runs; the aggregate log is
`/tmp/erpx_ignored_final_20260730_v2.log`.

QUEUED: external Generic LDAP/RFC 4533, AD DS/DirSync, UCS Provisioning
Service, TLS MinIO/physical PITR, approved protobuf descriptor provenance,
measured 1M-user/vendor/device/provider evidence, and production approval are
still unavailable. No customer directory activation, command-owner switch,
Folio write retirement, or production cutover occurred. G0–G14/vendor
acceptance remain open for those certification gates.

## Entry 2026-07-31 — isolated SQLx fixture hardening and final local handoff

IMPLEMENTED: ignored SQLx fixtures now isolate disposable control/read-side
state where the coordinator identified cross-run contamination. The fixes cover
quality idempotency keys, directory RBAC scope fixtures, control-plane and
support cleanup, Customer360 audit rebuild setup, and G13 source-kind-specific
tamper selection. These are persistence/test-harness correctness fixes; no
production command ownership or activation path changed.

PASS: fresh isolated local acceptance runs passed directory RBAC 4/4,
directory SQLx 3/3, identity SQLx 6/6, infrastructure ignored library 12/12,
platform completion 2/2, quality scope integrity 1/1, control-plane SQLx 3/3,
support workflow 1/1, support persistence 8/8, Customer360 1/1, marketing
delivery scheduler 1/1, role-review notifications 1/1, G13 target import 1/1,
and server owner-seed 1/1. The complete non-ignored workspace tests passed;
formatter, warnings-denied Clippy, Dioxus target checks, Folio lint/typecheck,
bridge tests 6/6, and port-3004 probes also passed. Lint retains four
pre-existing n8n warnings and zero errors.

QUEUED: the workspace-wide ignored invocation is not treated as proof when
multiple targets share one disposable URL pair; its exit-101 results include
fixture collisions plus deliberate credential/resource guards. The remaining
locally runnable ignored targets require an isolated per-target database matrix
for authoritative evidence. External Generic LDAP/RFC 4533, AD DS/DirSync,
UCS Provisioning Service, TLS MinIO/physical PITR, approved protobuf descriptor
provenance, measured one-million-user/vendor/device/provider evidence, and
explicit production approval remain unavailable. The exact coordinator-created
2026-07-31 disposable databases were dropped after verification; no other
databases were removed.

SAFETY: Folio remains on port 3004; no customer directory activation,
command-owner switch, Folio write retirement, or production cutover occurred.
G0–G14 and vendor acceptance are not claimed at 100% until the queued local
matrix and external certification gates are genuinely executed.

## Entry 2026-07-31 — locally runnable ignored matrix complete

PASS: the isolated local ignored database matrix is complete. The tenant lane
passed 43 targets / 93 tests; the infrastructure control lane passed 15
targets / 26 tests; infrastructure special targets passed support persistence
8/8, marketing scheduler 1/1, and role-review persistence 2/2; the ignored
infrastructure library passed 12/12; identity/server passed 14/14; and the
worker, provisioning, readiness, and G13 lane passed 18 target invocations /
24 tests. Every locally runnable target used fresh per-target PostgreSQL
databases, and each lane verified cleanup without removing pre-existing
databases.

QUEUED, FAIL-CLOSED AS EXPECTED: the only current ignored failures are the
external guards. `backup_live` reports all required `ERPX_BACKUP_LIVE_*`
control-plane, database-template, and TLS MinIO inputs missing; `pitr_live`
reports the required TLS MinIO, WAL archive, restore, and physical PostgreSQL
inputs missing; and `g14_external_readiness` reports missing Generic LDAP/RFC
4533, on-premises AD DS/DirSync, and UCS Provisioning Service references. No
external endpoint was contacted and no vendor acceptance was claimed.

REMAINING CERTIFICATION QUEUE: credentialed Generic LDAP/RFC 4533, AD
DS/DirSync, UCS Provisioning Service, measured one-million-user imports and
five-minute native revocation, TLS/PITR evidence, approved protobuf descriptor
provenance, device/provider evidence, and explicit production approval remain
unavailable. G0–G14/vendor acceptance therefore remain open only for those
external certification gates; no activation, owner switch, Folio write
retirement, or production cutover occurred.

## Entry 2026-07-31 — Folio bridge shadow handoff and current-tree verification

IMPLEMENTED: Folio platform writes retain Folio command ownership while
persisting a durable `platform.bridge_outbox` shadow handoff. Role grants,
append-only renewal/revocation, support-ticket/session lifecycle events, and
global-read audit events are queued transactionally; the bridge drain is
explicitly opt-in and writes only to the ERPX control outbox when its isolated
control-write URL and flag are configured. Strong bridge reconciliation now
requires count, active-count, immutable-link digest, no unlinked targets, and
equal source/target event high-watermarks. Missing evidence remains
unavailable rather than reconciled.

IMPLEMENTED: high-risk platform role creation, renewal, and revocation now
require fresh actor/action-bound signed step-up MFA evidence with an
independent approver; renewal revokes the prior assignment in the same
transaction. Operations authorization reads isolated control scope, enforces
holding/branch containment, and permits legacy Folio reads only under explicit
local break-glass. Platform UI exposes support approval/revocation and role
management journeys.

PASS: `cargo fmt --all --check`, workspace Clippy with warnings denied, the
complete non-ignored workspace/doc suite, protobuf package and G13/G14
compatibility tests, console Dioxus web/desktop/mobile plus
`aarch64-apple-ios`/`aarch64-linux-android` checks, shared UI iOS/Android and
Android bridge checks, Folio bridge/step-up/Operations focused tests,
`bun run lint`, `bunx tsc --noEmit`, and `git diff --check`. Folio runtime
probes remain healthy on port 3004. Lint has four pre-existing n8n warnings
and zero errors.

QUEUED, FAIL-CLOSED AS EXPECTED: the exact single-threaded aggregate ignored
workspace invocation exited 101 with 82 targets requiring unavailable
disposable SQLx URLs, live backup/PITR resources, or external certification
inputs; no assertion failure was promoted to a pass. The raw result is
`/tmp/erpx-ignored-20260731.log`. The isolated per-target local SQLx matrix
recorded above remains the authoritative local evidence.

REMAINING CERTIFICATION QUEUE: credentialed Generic LDAP/RFC 4533, on-premises
AD DS/DirSync, UCS Provisioning Service, measured one-million-user imports and
five-minute vendor revocation, TLS MinIO/physical PITR, approved protobuf
descriptor provenance, device/provider evidence, and explicit production
approval. No customer directory activation, command-owner switch, Folio write
retirement, or production cutover occurred; G0–G14/vendor acceptance remain
open until those gates are genuinely executed.

## Entry 2026-07-31 — bridge source aggregate identity hardening

IMPLEMENTED: the opt-in Folio bridge drain now maps each shadow event's target
aggregate ID from the source command aggregate rather than the bridge queue
row ID. Bridge event identity fields are bounded and reject control characters
before the transactional Folio outbox insert. The queue row ID remains the
durable event identity and idempotency key remains stable across retries.

PASS: bridge focused tests 10/10, `bun run lint`, `bunx tsc --noEmit`,
`git diff --check`, `cargo fmt --all --check`, `erpx-identity` focused tests
(122 unit tests; directory contract suites passed with external SQLx tests
correctly ignored), and port-3004 probes. No command-owner switch, directory
activation, Folio-write retirement, or production cutover occurred.

## Entry 2026-07-31 — directory synchronization preflight hardening

IMPLEMENTED: LDAP/RFC 4533, AD DirSync, and UCS feed contracts now validate
provider-native readiness, bounded DNs/filters/positions, and preflight before
import or change ingestion. Revocation breach transitions require the strict
five-minute deadline. Local focused evidence passed directory sync 45/45,
feed contracts 3/3, and volume/native cursor/revocation 5/5; no external
endpoint was contacted. Credentialed vendor, scale, and live revocation
evidence remain queued.

## Entry 2026-07-31 — reassigned local completion lanes

IMPLEMENTED: directory authentication now persists break-glass issuance time,
rejects future-dated grants, and includes issuance evidence in the
credential-free authentication record. Finance journal replay now verifies the
authoritative source row, immutable payload/version, and exactly one matching
outbox event on both replay branches. Operations gained the authoritative
Resolved-to-Closed transition and its authenticated tenant-scoped gRPC action
mapping. Dioxus secure-session restore and refresh/revoke state now remain
fail-closed until server revalidation.

PASS: identity unit and directory contract checks, Finance domain/infrastructure
focused suites, Operations domain/persistence checks, UI tests and target
checks, `cargo check -p erpx-server --offline`, and existing bridge/Folio
verification passed. The server transport change used the existing protobuf
action string and did not alter shared contracts. External SQLx, LDAP/AD/UCS,
backup/PITR, scale, native-device/provider, descriptor, and production gates
remain queued; no activation or cutover occurred.

## Entry 2026-07-31 — platform provisioning transport hardening

IMPLEMENTED: PostgreSQL admin and tenant provisioning URLs now require one
explicit `sslmode=verify-full` query mode before SQLx connection or mutation.
Fixture URLs and regression tests were updated accordingly. Infrastructure
unit tests (333/333), backup adapter tests (9/9), and provisioning local tests
passed; live MinIO/PITR and disposable database certification remain queued.

## Entry 2026-07-31 — role-review notification claim hardening

IMPLEMENTED: role-review notification claims now use a deterministic run time,
enforce a 100-row bound, validate expiry classification timestamps, and
release valid claims on malformed or oversized source responses. Focused worker
tests passed 8/8 and warnings-denied worker Clippy passed. Disposable SQLx
role-review acceptance remains queued because its database references are not
configured.

## Entry 2026-07-31 — product, backup, directory-worker, and UI bounds

IMPLEMENTED: Marketing journey integration persistence now performs a
source-fenced transactional rebuild with rollback preservation, a bounded
one-million-event scan, advisory locking, and strict reconciliation. Backup
retention releases claims fail-closed after reconciliation/history/report
errors with retry/dead-letter accounting; PITR requires a registry-owned
namespace. The directory worker rejects import progress beyond connector
capacity, and UI route/deep-link/component/session state now applies bounded,
redacted, fail-closed behavior.

PASS: Marketing rebuild 1/1 disposable acceptance, backup runtime focused
tests 7/7, directory worker focused/vendor/generic harnesses 11/10/10, and
UI route/state/component focused checks passed where their dependencies were
available. The backup worker focused test was queued by unrelated existing
infrastructure compile errors; no external resource was contacted. Remaining
external SQLx, provider/MinIO/PITR, LDAP/AD/UCS, native-device, and production
gates remain queued.

## Entry 2026-07-31 — final serialized verification

PASS: `cargo fmt --all --check`; workspace Clippy with `-D warnings`; the
complete non-ignored workspace tests and doc-tests; protobuf; G13/G14
compatibility; console Dioxus web, desktop, mobile, iOS, and Android checks;
UI iOS/Android checks; Android bridge tests; `bun run lint`; `bunx tsc
--noEmit`; bridge/step-up/Operations Bun tests; `git diff --check`; and
port-3004 probes. The full workspace suite passed after correcting the TLS
fixture used by the worker debug test and the G14 hash-deep-link source marker.

QUEUED, FAIL-CLOSED: the exact required ignored command exited 101 with 84
targets requiring unavailable disposable PostgreSQL, credentialed directory,
MinIO/PITR, approved descriptor, device/provider, or production-approval
inputs. The authoritative raw log is
`/tmp/erpx-ignored-20260731-final.log`. No external endpoint was contacted and
no certification was claimed. G0–G14/vendor acceptance remain open only for
these external gates; no activation, owner switch, Folio-write retirement, or
production cutover occurred.

## Entry 2026-07-31 — current-tree verification and certification disposition

FINAL CURRENT-TREE RESULT: formatting, warnings-denied Clippy, the complete
non-ignored workspace/doc suite, protobuf and G13/G14 compatibility, all
console/shared-UI target checks, Folio lint/typecheck, bridge/step-up/
Operations tests, diff hygiene, and port-3004 probes passed. Real ignored
`backup_live` and `pitr_live` acceptance also passed after the PITR verifier
change, including PostgreSQL 18 TLS, HTTPS MinIO, rollback, and cleanup; the
disposable services and artifacts were stopped/removed afterward.

The exact no-input ignored aggregate was rerun and exited 101 with 86
environment-gated targets; raw output is
`/tmp/erpx-ignored-20260731-postpitr.log`. The configured local SQLx matrix
and real backup/PITR targets are the available local database evidence.
Credentialed Generic LDAP/RFC 4533, AD DS/DirSync, UCS, approved descriptor,
physical device/provider, measured external scale/revocation, and explicit
production approval remain queued. No directory activation, command-owner
switch, Folio-write retirement, or production cutover occurred.

## Entry 2026-07-31 — current-tree external readiness and disposable LDAP probe

PASS: the focused current-tree directory acceptance suite passed 12/12 and
the worker generic/vendor harnesses passed 20/20 as fail-closed tests. Their
readiness output records the exact absent vendor endpoints, credentials, and
database references; no unconfigured external test was treated as acceptance.

A disposable Colima `epsx-e2e` OpenLDAP 1.5.0 probe was started to reproduce a
fresh LDAPS path. The image reached TLS startup, but its generated CA is
expired relative to the current date, and its bootstrap rewrite behavior is
incompatible with the host bind mount. This probe produced no certification
evidence and was removed. The earlier real local OpenLDAP/LDAPS/RFC4533
acceptance remains the authoritative Generic LDAP evidence. Microsoft AD DS,
UCS Provisioning Service licensing, external scale/revocation measurement,
approved descriptor/device inputs, and production approval remain queued.

## Entry 2026-07-31 — current certification-input audit

The resumed audit found no `ERPX_*`, LDAP, AD, UCS, backup, PITR, or tenant
database certification inputs in the environment. Local media inspection found
only the previously used UCS KVM image and overlays; no Microsoft Windows
Server/AD DS installation media or credentialed AD environment is present. The
UCS image remains license-gated for native Provisioning Service installation.
No license bypass, Samba compatibility result, or synthetic provider response
was promoted to vendor acceptance. The local Generic LDAP/RFC4533 evidence,
SQLx matrix, backup/PITR acceptance, and fail-closed readiness tests remain
the authoritative available evidence; all unavailable vendor and approval
gates stay queued.

## Entry 2026-07-31 — resumed current-tree serialized verification

PASS: after the resumed audit, `cargo fmt --all --check`, workspace Clippy
with `-D warnings`, and `cargo test --workspace --offline --no-fail-fast`
passed on the current tree. `bun run lint` passed with zero errors and the
four pre-existing n8n warnings; `bunx tsc --noEmit`, the G14 structural probe,
and the port-3004 runtime probes also passed (`/` 200, protected routes 307,
bridge unauthenticated 401). No production Next.js build was run.

The full workspace test output continues to report only deliberately ignored
resource-backed tests; those targets remain covered by the isolated matrix or
queued fail-closed external prerequisites. No vendor acceptance, directory
activation, command-owner switch, Folio-write retirement, or production
cutover was claimed.

## Entry 2026-07-31 — isolated ignored SQLx acceptance expansion

PASS: isolated TLS PostgreSQL runs closed 70 locally runnable ignored
assertions: local G13/G14 directory SQLx 3/3, identity persistence 6/6,
directory RBAC scope authority 4/4, G13 migration persistence 1/1, G13
disposable migration 1/1, G13 target import 1/1, infrastructure library
commercial/finance/support persistence 12/12, and eight disjoint tenant
context lanes totaling 42/42 across Assets/Maintenance, Finance, HR, Legal,
Operations, POS, Quality, and SCM. These runs exercised real migrations,
forced-RLS write/read separation, outbox/projector state, idempotency, scope
fences, reconciliation, and rollback behavior. Every coordinator-created
database was removed afterward; no pre-existing database was touched.

QUEUED, FAIL-CLOSED: the exact required ignored command was rerun with
`pipefail` and exited 101 with 86 targets. The raw output is
`/tmp/erpx-ignored-current-continuation-pipefail.log`; every failure was an
explicit missing URL/resource/vendor guard, including backup/PITR, external
directory certification, or the remaining database matrix not supplied to
that no-input aggregate. No unexpected application assertion failure was
observed, and no local result was promoted to Microsoft AD DS/UCS/vendor
acceptance.

## Entry 2026-07-31 — second isolated ignored SQLx wave

PASS: eight disjoint fresh TLS PostgreSQL lanes passed 32/32 ignored
assertions: Customer360 detail reads 8/8, Marketing CRM persistence 1/1,
Marketing delivery persistence 5/5, Marketing journey execution persistence
6/6, Support persistence 8/8, tenant projection scheduler 1/1, Finance
banking persistence 2/2, and HR payroll projection 1/1. These runs exercised
real migrations, forced-RLS write/read separation, outbox/projector state,
idempotency, scope/reconciliation, and rollback behavior. Every
coordinator-created database was removed afterward; no pre-existing database
was touched.

## Entry 2026-07-31 — resumed current-tree final verification

PASS: `cargo fmt --all --check`, workspace Clippy with `-D warnings`, the
complete non-ignored workspace suite, `cargo test -p erpx-proto --offline`,
G13 parallel/recovery (15/15 and 6/6), G14 local compatibility (13/13), G14
directory acceptance (12/12), all console Dioxus web/desktop/host-mobile/
iOS/Android checks, shared UI host/iOS/Android checks, and the Android bridge
target check. Folio `bun run lint` returned zero errors with four pre-existing
`infra/scripts/start-n8n.js` warnings; `bunx tsc --noEmit`, `git diff --check`,
and the port-3004 probes passed (`/` 200, `/platform` 307, `/operations` 307,
and the unauthenticated bridge 401). No production Next.js build ran.

QUEUED, FAIL-CLOSED: the mandated no-input ignored aggregate remains exit 101
with 86 explicit missing-resource/vendor guards; the isolated SQLx waves now
cover 102/102 locally runnable ignored assertions. Remaining closure gates
are credentialed/on-prem AD DS with native DirSync, licensed UCS Provisioning
Service with native feed evidence, measured external one-million-user and
five-minute vendor revocation evidence, approved protobuf descriptor
provenance, and explicit production approval. No directory activation,
command-owner switch, Folio write retirement, or production cutover occurred.

## Entry 2026-07-31 — expanded locally runnable ignored acceptance

PASS: additional isolated TLS PostgreSQL lanes closed 56/56 assertions,
bringing the locally runnable ignored total to 158/158. The wave covered
Customer/worker read and projector paths, Operations authoritative and routed
projection paths, Finance close/budget/chart/subledger/consolidation/Thai-tax
and candidate activation paths, Legal projection/scope paths, Marketing CRM/
content/journey/delivery/provider/erasure/abuse paths, Quality and
organization paths, Platform provisioning/readiness/scheduler/status paths,
backup job/retention/blocker/reconciliation paths, server owner-seed
transaction, and worker commercial handlers. All coordinator-created
databases were removed; pre-existing databases were not touched.

The first three harness attempts in this expansion were discarded for
misbound admin/template input or a database-name collision; corrected reruns
passed and no application assertion failure remained. The mandated no-input
ignored aggregate is still exit 101 with 86 explicit missing-resource/vendor
guards. No local result was promoted to AD DS/UCS/vendor acceptance.

## Entry 2026-07-31 — current G14 external-readiness recheck

PASS: the current tree's Generic LDAP live harness passed 10/10, including
one explicit queued live-transport case and the RFC 4533, TLS, immutable-link,
one-million-user, RBAC/revocation, and evidence-boundary checks. The AD/UCS
vendor harness passed 10/10 with both live vendor cases explicitly queued;
the external-readiness suite passed 10/10 with one intentionally ignored
credentialed preflight. All missing references were reported by name without
secret values, and no provider socket was contacted.

QUEUED, FAIL-CLOSED: Generic LDAP, on-premises AD DS/DirSync, and UCS
Provisioning Service still lack the required credentialed endpoints,
certificates, native-feed evidence, and measured provider-side revocation
action. No local harness result was promoted to vendor acceptance.

## Entry 2026-07-31 — current-tree verification after G14 recheck

PASS: after the current G14 readiness recheck, `cargo fmt --all --check`,
workspace Clippy with `-D warnings`, and the complete non-ignored workspace
suite passed. Folio `bun run lint` returned zero errors with four pre-existing
`infra/scripts/start-n8n.js` warnings; `bunx tsc --noEmit`, `git diff --check`,
and the port-3004 probes passed (`/` 200, `/platform` 307, `/operations` 307).
No production Next.js build ran, and no vendor socket, directory activation,
command-owner switch, Folio-write retirement, or production cutover occurred.

## Entry 2026-07-31 — third consecutive external-state audit

QUEUED, FAIL-CLOSED: the external state is unchanged after the third
consecutive goal audit. No AD DS/UCS credentials, endpoints, Windows Server
media, or running vendor environments are available; the local UCS image
remains license-gated. The native transports, worker registration, local
Generic LDAP/RFC 4533 evidence, SQLx persistence/projectors, and required
serialized verification remain green. No synthetic vendor response,
Samba-compatibility result, or local readiness result is promoted to AD DS or
UCS acceptance.
