# Manufacturing caller-unit native read plan — follow-up for consumer cutover

Source task: `G4 / migrate.manufacturing.read-plan` (run `run-20261123-g`, agent `wAEman`).
This document is the **plan** deliverable, not a cutover receipt. Acceptance stays deferred.

## Why this plan exists

A prior Manufacturing spec-register slice was rejected because it replaced live
Planning/Records reads with a fail-closed 503 transport: the native read was not
actually able to serve the caller, so a working read became a hard failure. This
slice therefore begins with the callers, not the transport. The owner registry in
`modules/manufacturing/server/src/public/native_command.rs` records the exact
projection/join and authority/scoping facts of each live read; the bind adapter
in `modules/manufacturing/server/src/adapters/native_command_sql.rs` plans binds
on the caller's own transaction. **No route, selector, bridge or consumer is
switched here and no live read returns a 503 marker.** The TypeScript reads stay
authoritative until each caller passes its own cutover gate below.

## Live caller inventory (what must keep working)

| Caller unit | Live source | Exact reads / joins |
| --- | --- | --- |
| `planning.scenarios` | `server-planning/adapters/production-command-sources-read.ts` | `scenarios` (`scenarioSpecification` join), `recommendations`, `executiveShortage`/`executiveRecommendation` (`executiveSpecification`, Commerce output/warehouse/shortage-product, Organization branch, Finance/Inventory facts), `comparison`, `scenario`, `scopedRecommendation` |
| `manufacturing.workspace` | `server-manufacturing/adapters/production-command-sources.ts` | `specifications`, `orders` (spec + Commerce `outputProduct`), `openOrders`, `accountingProfiles` |
| `records.detail` / `production.detail` | same owner source + `adapters/production.ts` | `specification`, `specificationLines`, `detailedOrder`/`scopedOrder`, `orderSpecificationLines` (Commerce `materialProduct`), `orderReservations` (Commerce `reservationProduct`), `orderEvents`, `costSnapshot`, `productGenealogy`, `orderPostings` (Finance `journal`), `orderOperations` (Planning `operationResource`), `lotGenealogy` (Inventory lot joins), `materialReservation` |
| `product.get` | same owner source | `specification`, `specificationLines`, `orderSpecificationLines` |

Atomic guards (`assertion(...)`, `guardScenarioSpecification`, `guardRecommendationSpecification`,
`lockOrder`, `guardOrderVersion`, `guardCurrentMaterialReservation`, etc.) are **not**
read projections. They stay inside the caller's serializable owner unit until a
separate native command guard owns them; they are deliberately absent from this
read registry so a read cutover cannot quietly drop an atomic check.

## Authority and scoping facts (must survive cutover)

From `server-manufacturing/adapters/purchases.ts` (`manufacturingSourceAccess` → `current`):

- Original `production_scope` assignment must still match the actor
  (`assignmentId`, `userId`, `role`, `companyId`, `branchId`, `departmentId`).
- The assignment must still select the `planning` or `manufacturing` module.
- `records.read` is required; `records.review` additionally requires `records.write`.
- Company isolation: `company_id = actor.companyId`. Branch scoping: the optional
  `(?::text IS NULL OR branch_id = ?)` predicate with `actor.branchId`.
- A source witness decides whether `planning.advanced-mrp` applies. On a transient
  source-view miss a read falls back to base capabilities and must **not** become
  a 404 — and certainly not a 503. This is the specific regression to avoid.
- All reads run on the caller's already-open Holding transaction; the owner read
  adds no connection, no commit and no lock.

Each `ReadPlan` restates its caller, permission/module/source witness and
cross-owner fragments. Cross-owner joins (Commerce product, Finance journal,
Planning resource, Inventory lot) remain in the live SQL and in the caller's
transaction until their own owner exposes a native subquery.

## Registered owner read kinds (RD_*)

`specification`, `specification-version`, `specification-lines`,
`order-specification-lines`, `orders`, `order-detail`, `material-reservations`,
`order-events`, `order-postings`, `order-operations`, `lot-genealogy`,
`product-genealogy`, `accounting-profiles`.

The r22 detail-select cutover switched `orderSpecificationLines`,
`orderReservations`, `orderPostings`, `orderOperations` and `lotGenealogy` in
`production-command-sources.ts` to `nativeManufacturingReadAll` on these closed
kinds and deleted `manufacturing-read-tail-r22-sql.ts`. The owner route and
`ERPX_NATIVE_MANUFACTURING_READ_CONFIG` provisioning remain lead integration
steps; this transport fails closed when they are absent.

The SQL literal for every kind is frozen byte-for-byte against the live caller in
the in-crate unit tests (`byte_parity_with_live_caller_sql`) and the bind order is
asserted against the live caller order. A kind with a `None` branch scope binds
SQL NULL and keeps the unscoped predicate intact.

## Cutover plan (one caller at a time; never a global switch)

1. **Native transport, additive.** Add an owner HTTP route
   (`adapters/native_command_http.rs`, `public/native_command_http.rs`) that returns
   only the registered `ReadPlan.sql` for a closed `RD_*` kind, using the existing
   private listener and exact registered Holding target validation. Reject unknown
   kinds with validation errors; do not mount a fallback. Keep the route dormant
   for live callers until step 3.
2. **Caller parity harness.** For each caller, capture the live result set and the
   bound native plan result on the same serializable Holding transaction and assert
   byte/row equality for the full projection, including cross-owner columns and
   the unscoped-branch case. Move the existing `production-oracles-cutover` and
   `planning-fixture-oracle-retirement` assertions onto this harness; do not delete
   an oracle before its native replacement passes.
3. **Per-caller route switch, smallest first.** Recommended order:
   `manufacturing.workspace` (`orders`, `accounting-profiles`) →
   `records.detail`/`production.detail` (`order-detail`, `order-events`,
   `order-postings`, `order-operations`, reservations, genealogy) →
   `product.get` (`specification`, `specification-lines`) →
   `planning.scenarios` (`specification`, `specification-version`). At each step the
   Next/transport caller keeps its permission/module/assignment/source checks;
   only the read SQL source changes. No step may retain a TS fallback as the
   authority once it is marked cut over.
4. **Retire the replaced TS method** in `production-command-sources.ts` /
   `production-command-sources-read.ts` only after its caller's parity gate passes,
   then update `retire.manufacturing.spec-register-read` with a strictly lower
   `retirement:ts-statements` count. Preserve atomic guards until their own native
   command task closes.
5. **Post-cutover audit.** Re-run the scoped tsc, the owner Rust suites and the
   affected Holding page through normal authentication. Record the KPI delta,
   retained legacy consumers with removal conditions, and the verified pushed SHA.
   Full acceptance (`A1`) remains deferred.

## Boundaries and ownership

- Lead owns `platform/composition/**`, root Cargo manifests/lockfile, contracts, status
  registers and the route mount; a worker must not switch a consumer or edit shared
  status to claim cutover.
- Manufacturing owns only its read registry, bind adapter, feature transport and
  the registered Manufacturing tests. Cross-owner SQL fragments stay inside the
  caller unit until the matching owner (Commerce/Finance/Planning/Inventory) exposes
  a native subquery.
- This slice changed no Cargo test registration: runnable unit tests live as
  `#[cfg(test)]` modules in the two new Rust files (`cargo test -p erpx-manufacturing-server --lib`).
  Registering a repo-level suite file for the same assertions needs a shared
  `modules/manufacturing/server/Cargo.toml` `[[test]]` entry and belongs to lead
  integration, outside this task's `allowedPaths`.
