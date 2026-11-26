# Native production accounting-profiles owner read — parity and handoff

Task: `G4 / exec.manufacturing.routes` (worker `exec-manufacturing`, run `run-20261126-f`).
Route family: `production`. Bounded operation: the `RD_ACCOUNTING_PROFILES`
projection used by `GET /api/v1/holdings/{holdingSlug}/production`
(`productionWorkspace`). Acceptance remains deferred; the lead verifies,
mounts and cuts over.

## What is now native

| Layer | File | Responsibility |
| --- | --- | --- |
| domain | `packages/server-manufacturing/src/domain/production_profiles.rs` | frozen SQL, bounded company scope, complete `AccountingProfileRow` DTO |
| application | `packages/server-manufacturing/src/application/production_profiles.rs` | read-only query, validation, `AccountingProfilesRead` owner port, handler |
| adapter (SQL) | `packages/server-manufacturing/src/adapters/production_profiles_postgres.rs` | **real sqlx execution** via `PgUnitOfWork` on `DatabasePool`; nine-column row mapping |
| adapter (HTTP) | `packages/server-manufacturing/src/adapters/production_profiles_http.rs` | exact Holding-target axum ingress, no SQL/fallback in the handler |
| public | `packages/server-manufacturing/src/public/production_profiles.rs`, `public/production_profiles_http.rs` | stable owner surface for composition |

SQL execution proof: `PgProductionProfilesRead::profiles` in
`adapters/production_profiles_postgres.rs` calls
`sqlx::query(ACCOUNTING_PROFILES_SQL).bind(company).fetch_all(&mut *connection)`
inside a `PgUnitOfWork::run` transaction and maps every returned column into
`AccountingProfileRow`. The handler in `adapters/production_profiles_http.rs`
only validates and calls the injected handler; it contains no SQL and no TS
fallback.

## Payload parity

The retired TypeScript operation returned raw rows from
`SELECT id,status,version,currency,chart_id,inventory_account_id,wip_account_id,overhead_account_id ...`.
The DTO is serialized with the same snake_case keys, so every field the caller
consumed is preserved:

`id`, `status`, `version` (BIGINT → integer), `currency`, `chart_id`,
`inventory_account_id`, `wip_account_id`, `labor_account_id`,
`overhead_account_id`.

The owner SQL is byte-identical to `PLAN_ACCOUNTING_PROFILES.sql` in
`public/native_command.rs` except for the Postgres bind marker (`$1` for `?`),
asserted in the domain and integration tests. A wrapper
`{"data": [ ...rows... ]}` matches the retired route envelope; headers
`Cache-Control: private, no-store`, `Referrer-Policy: no-referrer`,
`X-Content-Type-Options: nosniff` match the lane-28 owner routes.

## Checks (BUILD FREEZE — none executed by the worker)

The lead's dispatch froze `cargo`/`tsc`/build for this iteration, so no checks
were run. The lead must run, in order:

1. `cargo check -p erpx-server-manufacturing --locked`
2. `cargo test -p erpx-server-manufacturing --lib --locked` (new in-crate tests)
3. after adding the `[[test]]` entry below:
   `cargo test -p erpx-server-manufacturing --test manufacturing_production_profiles_http --locked`
4. a scoped Postgres parity test against `erpx_dev_*` (not added here:
   `tests/integration/manufacturing/**` is outside the worker's allowed paths).

## Mount spec (lead-owned)

- Router constructor: `erpx_server_manufacturing::public::production_profiles_http::router`
  (single pool) or `router_for_holdings` / `router_with_reader`.
- Route: `GET /api/v1/holdings/{holding_slug}/production/accounting-profiles`.
- Target identity: exact registered `(host, holding_slug)` pair; Host header must
  match. Company scope comes from the single `x-erpx-context-company` header.
- Composition should add a small module under `src/composition/src/` that builds
  the router from the owner `DatabasePool` and the registered Holding targets,
  alongside the existing Manufacturing routers (`manufacturing_read.rs`,
  `manufacturing_wave3.rs`, `native_lane_28` mounts). No composition or root
  manifest was touched by this worker.
- Test registration needed in `packages/server-manufacturing/Cargo.toml`
  (`autotests = false`):
  `[[test]] name = "manufacturing_production_profiles_http" path = "../../tests/unit/manufacturing/rust-production-profiles-http.test.rs"`

## Route-deletion packet (lead-owned)

1. Native route becomes authoritative for the accounting-profiles projection.
2. Consumer move: `packages/server-planning/src/application/scenarios.ts:79,92`
   (`r.accountingProfiles(a.companyId)` → strict native client transport; no
   local SQL, no 503 fallback).
3. TS authority to delete after the consumer switch:
   - `packages/server-manufacturing/src/adapters/production-command-sources.ts:78`
     (`accountingProfiles` method).
   - `packages/server-manufacturing/src/adapters/native-manufacturing-read.ts`
     (`MANUFACTURING_RD_ACCOUNTING_PROFILES` constant line 30, registry line 64,
     `nativeManufacturingReadAll` / `nativeManufacturingReadFirst` lines 176–194
     once no other kind uses them).
   - `packages/server-manufacturing/src/public/native_command.rs`:
     `RD_ACCOUNTING_PROFILES` line 58, `PLAN_ACCOUNTING_PROFILES` lines 339–354
     and its `PLANS` entry line 370 (remove this one kind only, not the registry).
4. Shared route handler to retire once no non-backend consumer needs the
   production GET: `packages/feature-manufacturing/src/routes/api/v1/holdings/[holdingSlug]/production/route.ts:14-34`
   (the `GET` export). `POST` is unaffected.
5. App re-exports of that shared handler (delete with step 4):
   - `apps/manufacturing/src/app/api/v1/holdings/[holdingSlug]/production/route.ts:2`
   - `apps/compat/src/app/api/v1/holdings/[holdingSlug]/production/route.ts:2`
6. Canonical destination links for the workspace are produced by the existing
   client presentation helper; the deleted backend route supplies none.

## KPI

No TypeScript writer was removed by this read-only slice. Retirement counter
`retirement:ts-statements` (SELECT) must be measured by the lead after the
deletion steps; the owner's TS SELECT count for this operation is one statement
(`production-command-sources.ts:78` via the plan registry). No manifest,
catalogue or generated inventory file was edited.
