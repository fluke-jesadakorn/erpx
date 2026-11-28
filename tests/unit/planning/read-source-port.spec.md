# Planning read-source native port — G4 / migrate.planning.read-source-port

Source task: `G4 / migrate.planning.read-source-port` (run `run-20261123-i`, agent `wGC3pln`).
This is a **port + parity** deliverable, not a consumer cutover. Acceptance stays deferred.

## Coverage

The two blocked TypeScript read sources contain 23 SELECT tokens:

| Retained source | Tokens | Registered plans |
| --- | --- | --- |
| `modules/planning/server/src/adapters/production-command-sources-read.ts` | 16 | 15 |
| `modules/planning/server/src/adapters/resources-read.ts` | 7 | 7 |

Each executive projection folds its lateral shortage SELECT into the single
statement the caller executes; the recommendation's runtime eligibility branch
is registered as two variants. The closed registry therefore holds 22 plans.

## Deliverables

- `modules/planning/server/src/public/read_source.rs` — owner-local registry.
  Every plan carries the exact retained SQL, ordered bind fields, caller,
  authority and disclosed cross-owner fragments. `include_str!` unit tests bind
  each literal to the retained TypeScript file; cross-owner fragments are bound
  to their owner projection sources.
- `modules/planning/server/src/adapters/read_source.rs` — bind adapter. Maps
  caller scope to ordered binds on the caller's own transaction; a `None`
  branch scope binds SQL NULL and keeps the unscoped predicate. No connection,
  commit, lock or status marker.
- `tests/integration/planning/rust-planning-read-source-parity-postgres.test.rs`
  — same-transaction PG parity for all 22 plans, including the unscoped-branch
  form and an uncommitted write. Nothing commits; no caller is switched.

No TypeScript reader is deleted by this slice.

## Authority facts that must survive cutover

- `records.read` plus the `planning` module; resource guard reads also require
  `records.write`/`records.review` and the native owner-write flush.
- Company isolation via `company_id = actor.companyId`.
- Branch scoping via `(?::text IS NULL OR branch_id = ?)`; retained callers pass
  their single `branchId` into both positions.
- All reads run on the caller's already-open serializable Holding transaction.
- Cross-owner joins (Manufacturing specification, Commerce product/warehouse,
  Organization branch) stay inlined until their own owner exposes a native
  subquery.

## Next action for a later slice

1. Add an additive owner-native HTTP transport for a closed `RD_*` kind.
2. Move one caller at a time onto the parity harness; smallest first
   (`scenarios`, `scenario`, `resources`), guards last because they are atomic
   assertions inside the native Workflow unit.
3. Delete each replaced TypeScript method only after its parity gate passes,
   then record a strictly lower `retirement:ts-statements` count for owner
   `planning` (no writer is removed by this read-only slice).
4. Re-run scoped tsc, the owner Rust suites and the affected Holding page;
   acceptance (`A1`) remains deferred.
