# Native runtime and transaction contract

The Rust runtime is infrastructure for the owner migration described by [ADR-052](../../adr/ADR-052-rust-backend-and-domain-aligned-frontend.md). It does not establish business readiness or replace the [production plan](../../production-plan.md). Native business routers remain unmounted until their complete authorization and behavior contracts pass.

## Composition and dependencies

`src/composition` owns package `erpx-service` and executable `erpx-server`. It assembles infrastructure and owner public adapters. `packages/server-runtime` is a library; owner adapters may depend on its public infrastructure facade, and runtime cannot depend on an owner implementation. Portable owner domain and application modules define their own ports and cannot import SQLx, Axum, runtime adapters, process configuration or filesystem APIs.

The runtime facade is `erpx_server_runtime::public::postgres`. Composition creates one shared `DatabasePool` per selected database resource and process, then passes clones or borrowed connections to owners. Clones share the underlying pool and admission gates. An owner must not create a pool for every request or obtain a new connection when participating in another owner's transaction.

`DatabaseManifest::load` requires an explicit absolute path. The version-one manifest contains Control write/read bindings and explicit resource slots. Slot names are configuration identifiers, not Company identifiers. `DatabaseBinding::HoldingWrite` and `HoldingRead` take a trusted slot identifier; the Control Holding registration and exact host decision must select that resource first. A Company request cannot choose a database URL or derive a binding name. Configuration errors and `Debug` representations exclude database URLs and passwords.

## Serializable Holding units of work

`PgUnitOfWork::run` leases one PostgreSQL connection for an attempt and opens a tracked `SERIALIZABLE` transaction. Every participating owner adapter borrows that connection. Receipts, business writes, audit and outbox effects belong to that same transaction. Owned return values cannot borrow the transaction.

An optional `SessionLock` coordinates a busy aggregate before PostgreSQL establishes its snapshot. Cloned pools share a keyed admission gate acquired before a connection is checked out, so same-key waiters cannot consume every pool connection. The PostgreSQL advisory lock is acquired before `BEGIN` and explicitly unlocked after the attempt. A cancellation or uncertain unlock discards the connection instead of returning a potentially locked session to the pool.

Only SQLSTATE `40001` and `40P01` replay the complete callback, at most four attempts with bounded backoff. Callbacks must therefore be replayable and must not perform external effects. A durable outbox carries such work after commit. The shared portable `UnitOfWork` port uses `FnMut` and an associated error type for the same reason.

Borrowed owner ports preserve persistence errors until this coordinator sees them. For example, `OrganizationReadPort` has an associated `Error: From<BusinessError>`; its SQLx adapter returns `PgTransactionError<BusinessError>`. A SQLx read failure must not be flattened into a semantic unavailable error before retry classification. `PgTransactionError::Business` failures are returned without retry.

## Retained Control authorization leases

`PgRequestLease` uses one `READ COMMITTED` Control transaction and invokes its callback once. The Identity session verifier acquires Control read locks and checks the current Account parent, device selection, principal, expiry, environment and exact Holding audience. The caller retains that lease while an inner Holding unit of work runs and commits.

The Control callback is never retried automatically. Replaying it after a successful Holding commit could duplicate already committed effects. Inner Holding serialization conflicts may retry while the Control proof remains fenced. Control mutations use their own command transaction instead of this authorization lifecycle.

`HoldingWorkContextInspector` wires this lifecycle for current selected-grant inspection. It obtains session and user identifiers from the verified Control proof, binds one configured Holding pool, and rejects an unrelated host before either pool is acquired. Its result contains current Company/branch/department lineage, assignment and published-role versions. Those facts are not effective IAM permission, HR or emergency authority, GPO evaluation, or an owner's record policy. They cannot by themselves authorize a business mutation.

## Immutable SQL migrations

The custom migration runner preserves the existing `control`, `control-read`, `holding-write` and `holding-read` tracks. It sorts migration names, hashes raw UTF-8 bytes with SHA-256 and records each committed file in `erpx_migrations`. It intentionally does not use SQLx's built-in migration table or checksum format.

One leased connection holds advisory lock `714928001` across a run. Each complete SQL file, including PL/pgSQL bodies, and its receipt commit atomically. A failed file rolls back while earlier files remain committed. Applied checksum changes fail with the track and filename. The two existing grandfather exceptions require both exact predecessor and canonical hashes and never rewrite the old receipt.

Application inspection calls only `erpx_migration_receipts()`. If that narrow function is absent while a receipt table exists, inspection fails closed instead of reading the privileged table directly. Migration roles and application roles remain distinct deployment responsibilities.

The runtime Cargo manifest declares `package.metadata.erpx.verification-roots` for the migration directory. Native source selection and fingerprints include new and deleted SQL files under that root. Other native crates may declare reviewed repository resource roots for static catalogues and fixtures; private directories and paths escaping the repository are rejected.

## Permission metadata boundary

The Identity owner holds the canonical permission catalog in `packages/server-identity_access/resources/permissions.v1.json`. Its Rust model validates the metadata and generates `packages/contracts/src/permission-catalog.generated.json` for frontend projections. Regenerate with `bun scripts/docs/generate-permission-contracts.ts` and verify with `--check`; scoped CI rejects drift. The native catalog requires an explicit current IAM permission list and never substitutes a role name. Its metadata model and legacy decision corpus do not implement the pending effective-authority coordinator or grant business access by themselves. Current membership, HR, emergency, Directory eligibility and each owner's record policy remain separate requirements.

## Structural JSON numbers

The shared `public::json_numbers` inbound helpers accept finite integral JSON forms such as `1`, `1.0` and `1e0`, matching the existing JavaScript integer boundary. They reject string/boolean coercions, fractions and overflow; safe signed/unsigned helpers enforce the exact JavaScript integer range. Owner types still enforce positive or nonnegative versions and other business rules. Monetary decimal-string contracts are unchanged. The pinned `serde_json` enables `float_roundtrip` so boundary forms such as the maximum safe integer followed by `.0` are parsed with correct rounding. URL query strings require a separate typed query adapter.

## Development verification

`bun run rust:test` runs all registered native unit targets through the common runner and records every observed case. Optional worker and contract-generation features are selected explicitly. A zero exit with empty, ignored or filtered cases is not a passing native receipt. `bun run rust:check`, `bun run rust:clippy` and `bun run rust:fmt` provide compilation and source checks; scoped CI also verifies generated Account TypeScript and OpenAPI contracts.

PostgreSQL suites are deliberate integration invocations, for example:

```sh
bun scripts/testing/test.ts integration --suite=shared.persistence.rust-postgres.integration
bun scripts/testing/test.ts integration --suite=shared.persistence.rust-migrations.integration
```

Configure the ignored `ERPX_TEST_ADMIN_URL` using the existing local test-administrator bootstrap. The native fixture requires `erpx_test_admin` on `127.0.0.1:5432/postgres`, `CREATEDB`, no superuser and no role-creation privilege. It creates only fresh `erpx_dev_rust_*` databases, revokes public connection access, records a unique lease, and verifies exact ownership and lease before cleanup. It never migrates or clears an existing development, production or staging database. Cancellation of the entire test process can leave a leased disposable database for explicit fixture cleanup; it does not authorize broad prefix-based deletion.

These suites prove source behavior and isolated database contracts. They are not complete owner migration, browser, provider, deployment or production acceptance.
