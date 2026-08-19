# G3–G6 platform-control completion slice

This slice owns only the following files:

- `erpx/crates/domain/src/platform_completion.rs`
- `erpx/crates/infrastructure/src/platform_completion.rs`
- `erpx/apps/server/src/platform_completion.rs`
- this audit

No Cargo manifest or lockfile, protobuf, migration, shared registration, shared authorization, worker registration, console shell, directory module, `MASTER_PLAN.md`, or `EXECUTION_LEDGER.md` was edited.

## Delivered behavior

The domain contract is authoritative and fail-closed for:

- holding lifecycle transitions (`provisioning`, `active`, `suspended`, `recovery`);
- full and restore-test backup commands, with PITR rejected until a WAL/recovery-point contract exists;
- replacement-read rebuild/ready/failed/activated transitions with source-version, generation, candidate-identity, and source-fence checks;
- platform and per-holding connector binding for Stripe, marketing, finance-bank, generic LDAP, AD DS, and UCS;
- opaque secret-manager references only, SHA-256 configuration digests, bounded capability sets, immutable connector IDs, and terminal connector revocation;
- authorization-proof actor/scope/permission/time validation, MFA for high-risk mutations, and local platform-only break-glass tickets.

The SQLx adapter persists each mutation in one PostgreSQL transaction. It updates the authoritative control tables and versioned aggregate snapshot, appends an append-only `outbox_events` row and `audit_events` row, and enforces idempotency/request-hash replay and optimistic concurrency. Reconciliation and activation read registry, provisioning, write/read database health, and storage health; evidence older than five minutes is not considered healthy. Replacement transitions reject stale source versions, SQL-range overflow, generation mismatches, and activation without the routed source-rebuild fence. Failure fencing is available from either `rebuilding` or `ready`, while activation remains `ready`-only.

`PlatformCompletionProjector` implements the existing worker projection contract. `SqlxPlatformReadModelWriter` is a real PostgreSQL writer with version-gated upserts and rebuild clearing; it never falls back to an in-memory map and fails closed when its table is absent. Connector projections omit credential references and replacement-read projections omit candidate database keys.

The server adapter provides mutation request/response handoffs and authorized UI-safe connector-health and reconciliation responses. Raw connector and reconciliation reads also require the shared read authorization proof. It maps persistence failures to stable gRPC status classes without returning SQL or credential details. Existing RPC handlers remain the authentication, shared-role, entitlement, SoD, protobuf, and composition owners.

## Coordinator hooks required before integration acceptance

1. Domain/infrastructure registrations, the worker projector registration, and
   the coordinator-owned `0010_platform_completion.sql` read table are
   present in concurrent coordinator changes; verify them against both
   control databases and retain the writer's payload bound and the table's
   unique projection/aggregate key.
   This lane did not edit those files.
2. Register `PlatformCompletionApi` in the server composition and map existing
   Platform Operations holding lifecycle, backup, projection-rebuild,
   integration, settings, and reconciliation handlers to these methods. The
   handler must authenticate the request, enforce the existing platform
   scope/`platform.holding.read` or mutation permission, bind the authenticated
   principal to `PlatformAuthorizationProof`, and preserve current command
   ownership. The current server file is only the typed adapter; composition
   wiring remains a coordinator hook.
3. Verify worker startup, projection checkpointing, retry on an unavailable
   read model, and clear/rebuild from retained outbox events before acceptance.
4. Keep protobuf compatibility unchanged and expose the typed server handoff
   through the existing RPC surface rather than inventing a second transport.

## Evidence and tests

Direct checks completed for the owned files:

```text
rustfmt --edition 2024 --check erpx/crates/domain/src/platform_completion.rs
rustfmt --edition 2024 --check erpx/crates/infrastructure/src/platform_completion.rs
rustfmt --edition 2024 --check erpx/apps/server/src/platform_completion.rs
```

With the coordinator registrations present in the current worktree, the focused suites also pass: `erpx-domain` 6 platform tests, `erpx-infrastructure` 3 platform tests, and `erpx-server` 3 platform tests. Registered-package Clippy also passes with warnings denied. The infrastructure tests are contract/redaction tests; live SQLx transaction and projector checks still require the registered control databases and read-model table.

After the coordinator hooks above are applied, run the focused suites:

```text
cargo test -p erpx-domain platform_completion
cargo test -p erpx-infrastructure platform_completion
cargo test -p erpx-server platform_completion
```

The focused unit coverage includes closed lifecycle transitions, MFA/break-glass restrictions, PITR and restore-shape rejection, connector digest/URL rejection, terminal revocation, source-fence enforcement, credential/candidate redaction, error redaction, and UI-safe categorical health mapping. The isolated real-code harnesses passed `cargo clippy --all-targets -- -D warnings` and `cargo test` for domain, infrastructure, and server (6, 9, and 12 tests respectively; no failures). SQLx transaction tests require the registered control database and are not represented by mocks.

This lane does not claim live LDAP, AD DS, UCS, MinIO, PITR, or production cutover evidence. Credentialed external vendor certification remains a separate blocker if those environments are unavailable. No customer directory was activated, no Folio write path was retired, and no production command owner was changed.

## Rollback

Rollback is limited to removing/reverting the four owned files and, if the coordinator has installed it, removing the coordinator-owned read-model registration through the normal migration rollback process. Outbox/audit rows are append-only and must be retained; operational rollback is a feature/registration disablement followed by projection rebuild from retained events. Folio files, unrelated user changes, directory data, and production command ownership are outside this slice.

## Acceptance status

| Gate | Status |
| --- | --- |
| Domain invariants and fail-closed authorization proof | Implemented in owned module |
| SQLx authority, idempotency, optimistic concurrency, outbox, audit | Implemented in owned module; requires existing control schema |
| Worker projector and rebuild contract | Implemented in owned module; coordinator registration present, runtime/rebuild evidence pending |
| Rebuildable platform read model | Implemented in owned module; coordinator migration present, live two-control-DB evidence pending |
| Server/API/UI handoff | Implemented in owned module; existing RPC wiring pending |
| Generic LDAP / AD DS / UCS live acceptance | External certification pending |
| Full workspace verification | Coordinator-owned integration step pending |
