# Native production order-events owner read — parity and handoff

Task: `G4 / exec.manufacturing.routes.2` (worker `w2-manufacturing`, lead run
`run-20261126-i`). Route family: `production` detail history. Bounded operation:
the `RD_ORDER_EVENTS` projection currently served by TypeScript plan/execute at
`production-command-sources.ts:142`. Acceptance stays deferred; the lead
verifies, mounts and cuts over.

## What is now native

| Layer | File | Responsibility |
| --- | --- | --- |
| domain | `packages/server-manufacturing/src/domain/order_events.rs` | frozen SQL, bounded order id, complete `OrderEventRow` DTO |
| application | `packages/server-manufacturing/src/application/order_events.rs` | read-only query, validation, `OrderEventsRead` owner port, handler |
| adapter (SQL) | `packages/server-manufacturing/src/adapters/order_events_postgres.rs` | **real sqlx execution** via `PgUnitOfWork` on `DatabasePool`; nine-column history row mapping |
| adapter (HTTP) | `packages/server-manufacturing/src/adapters/order_events_http.rs` | exact Holding-target axum ingress, no SQL/fallback in the handler |
| public | `packages/server-manufacturing/src/public/order_events.rs`, `public/order_events_http.rs` | stable owner surface for composition |
| tests | `tests/unit/manufacturing/rust-order-events-http.test.rs` | SQL parity, full payload, host scope, bounded id |

SQL execution proof: `PgOrderEventsRead::events` in
`adapters/order_events_postgres.rs` calls
`sqlx::query(ORDER_EVENTS_SQL).bind(order_id).fetch_all(&mut *connection)`
inside a `PgUnitOfWork::run` transaction and maps every returned column into
`OrderEventRow`. The handler in `adapters/order_events_http.rs` only resolves the
registered target, validates the path id and calls the injected handler; it
contains no SQL and no TypeScript fallback.

## Payload parity

The retired TypeScript operation returned raw rows from
`SELECT * FROM manufacturing_order_events WHERE order_id=? ORDER BY version`
through `nativeManufacturingReadAll<Row>(MANUFACTURING_RD_ORDER_EVENTS, …)`.
The DTO is serialized with the same snake_case keys, so every field the caller
consumed is preserved and in the same version order:

`id`, `order_id`, `version` (BIGINT → integer), `action`, `actor_id`, `reason`,
`status`, `details_json`, `created_at` (BIGINT → integer).

The nine columns are fixed by `migrations/postgres/holding-write/0233_planning_manufacturing.sql`
and no later migration alters the table. The owner SQL is byte-identical to
`PLAN_ORDER_EVENTS.sql` in `public/native_command.rs` except for the Postgres
bind marker (`$1` for `?`), asserted in the domain and repo-level tests. The
wrapper `{"data": [ …rows… ]}` matches the retired owner envelope; headers
`Cache-Control: private, no-store`, `Referrer-Policy: no-referrer`,
`X-Content-Type-Options: nosniff` match the lane-28 owner routes.

## Checks (BUILD FREEZE — none executed by this worker)

The lead froze `cargo`/`tsc`/build/test/server commands for this iteration, so
no checks were run. The lead must run, in order:

1. `cargo check -p erpx-server-manufacturing --locked`
2. `cargo test -p erpx-server-manufacturing --lib --locked` (new in-crate tests)
3. after adding the `[[test]]` entry below:
   `cargo test -p erpx-server-manufacturing --test manufacturing_order_events_http --locked`
4. a scoped Postgres parity test against `erpx_dev_*` (not added here:
   `tests/integration/manufacturing/**` is outside the worker's allowed paths).

## Mount spec (lead-owned)

- Router constructor: `erpx_server_manufacturing::public::order_events_http::router`
  (single pool) or `router_for_holdings` / `router_with_reader`.
- Route: `GET /api/v1/holdings/{holding_slug}/production/orders/{order_id}/events`.
- Target identity: exact registered `(host, holding_slug)` pair; the Host header
  must match and the slug must be a bounded lowercase identifier. The order id
  comes from the path and is revalidated as a `RecordId` (nonempty, ≤200 UTF-16
  code units, no control characters). No Company header is bound because the
  frozen owner SQL scopes by `order_id` only; composition keeps the existing
  WorkContext, module, purchase and assignment authority checks.
- Composition should add a small module under `src/composition/src/` that builds
  the router from the owner `DatabasePool` and the registered Holding targets,
  alongside the existing Manufacturing routers (`manufacturing_read.rs`,
  `manufacturing_wave3.rs`, `native_lane_28` mounts). No composition or root
  manifest was touched by this worker.
- Test registration needed in `packages/server-manufacturing/Cargo.toml`
  (`autotests = false`), which is outside this task's allowed paths:
  `[[test]] name = "manufacturing_order_events_http" path = "../../tests/unit/manufacturing/rust-order-events-http.test.rs"`

## Route-deletion packet (lead-owned)

1. Native route becomes authoritative for the order-events projection.
2. Consumer move: `packages/server-manufacturing/src/application/production.ts:229`
   (`r.orderEvents(id)` inside `productionDetail`) must obtain the event list
   from the strict native client transport instead of the TS plan/execute
   method; no local SQL and no 503 fallback may remain. Its public surface is
   `packages/server-manufacturing/src/adapters/production.ts:9` and
   `packages/server-manufacturing/src/public/production.ts:1`.
3. Downstream callers that must keep receiving the same `history` field:
   - `packages/server-runtime/src/adapters/executive/evidence.ts:105` and
     `packages/server-runtime/src/application/executive/evidence.ts:1670`
   - `packages/server-collaboration/src/adapters/staff-notifications.ts:93` and
     `packages/server-collaboration/src/application/staff-notifications.ts:232`
   - `packages/workplace/src/routes/holdings/[holdingSlug]/_presenters/manufacturing/[id]/screen.tsx:20`
4. TS authority to delete after the consumer switch (inside this task's allowed
   paths, deletion reserved for the lead's integration):
   - `packages/server-manufacturing/src/adapters/production-command-sources.ts:142`
     (`orderEvents` method).
   - `packages/server-manufacturing/src/adapters/native-manufacturing-read.ts:26`
     (`MANUFACTURING_RD_ORDER_EVENTS` constant) and its registry entry at line 59.
   - `packages/server-manufacturing/src/public/native_command.rs:49` (`RD_ORDER_EVENTS`),
     lines 251–266 (`PLAN_ORDER_EVENTS`) and its `PLANS` entry at line 364
     (retire this one kind only, not the registry).
   - `nativeManufacturingReadAll` / `nativeManufacturingReadFirst` in
     `native-manufacturing-read.ts:176–194` only once no other RD_* kind uses them.
5. There is no dedicated shared API route handler or `apps/*/src/app/api/**`
   re-export for order events: the read is reached through `productionDetail`
   only. The generic production `GET`/`POST` route at
   `packages/feature-manufacturing/src/routes/api/v1/holdings/[holdingSlug]/production/route.ts`
   must remain because it serves other production operations.
6. Canonical destination links are unaffected: this read returns no links.

## KPI

No TypeScript writer was removed by this read-only slice. Retirement counter
`retirement:ts-statements` (SELECT) decreases by one statement after step 4
(`production-command-sources.ts:142` via the plan registry). No manifest,
catalogue, ledger, composition file or generated inventory file was edited.
