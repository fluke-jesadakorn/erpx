# `migrate.manufacturing.production-oracles` — precise cutover / blocker spec

Status: **implementation committed and pushed; migration gate blocked; acceptance deferred.**
This slice is a test-oracle relocation only — it does not change a production writer,
a native route or a presenter. The gate cannot close until the owner's TypeScript
writes are retired by `retire.manufacturing`.

## Identity and source

- Task: `migrate.manufacturing.production-oracles`, goal `G3`, owner `manufacturing`,
  workstream `S08`, plan `docs/production-plan.md#rust-migration-current`.
- Claim held for this verification: `runId=run-20261123-e`, `agentId=worker-manuO`
  (do not mutate ledger or push; no claim was modified in this run).
- Implemented and pushed SHA (identical): `d5318121503beef1a515acf009f60a6fe40f3e0b`
  (`test(manufacturing): native-order production oracles`, 10 files, 752 insertions,
  13 deletions). It is an ancestor of HEAD.
- HEAD at first read: `8785f0f3f801919cbad372acdc6145b1f487184a`; the lead
  advanced HEAD and the verified remote to
  `a5598fdd59a8a172530955a6fd31a9f81cdeac8c` on `codex/rust-backend-migration`
  before this spec was finished (no source change by this worker).
- Ledger revision 6037→6041 during reads; `validate` returned `{"valid":true}`.

## What the slice changed (all inside the held paths)

- `tests/integration/manufacturing/production.test.ts` — rewired the production
  integration cases to import test-only oracle copies instead of live owner TS
  packages: `changeManufacturingOrder` now comes from
  `../../support/manufacturing/legacy-production`; inventory count and initial
  physical stock imports now come from
  `../../support/manufacturing/legacy-count/*`.
- `tests/support/manufacturing/legacy-production/{application-production,adapters-production,lots-application,lots-adapters,lots-domain,manufacturing-lot-sources,index}.ts`
  and `tests/support/manufacturing/legacy-count/{inventory-counts,manufacturing-initial-physical-stock-count}.ts`
  — test-only copies added so the suite keeps exercising the original legacy
  behavior without importing the production TS modules.

No file under `modules/manufacturing/ui/**` or
`modules/manufacturing/server/src/**` was changed by this slice.

## Four cutover layers — not applicable to this slice

The production route was already cut over to the native order command by
`migrate.manufacturing.order-material-lifecycle` and
`migrate.manufacturing.production-lots` (see
`tests/unit/manufacturing/production-lots-cutover.spec.md`): the Next route POST
dispatches issue/pass-QC/cancel/scrap to the native axum command and fails closed
with 503; no TypeScript lot-writer fallback remains. This packet only relocates
test oracles, so it introduces no new owner package, native client, route or
presenter change. No workplace presenter reads the relocated sources.

## Blocker: retirement KPI cannot go strictly lower

The coordinator migration gate rejects `migrated` without a passed retirement
check whose `after < before` (`RETIREMENT_KPI_REQUIRED`,
`scripts/testing/rust-migration-work.ts:384`); a non-zero `after` also requires a
covering `retire.<owner>*` task (`RETIRE_TASK_REQUIRED`, line 408).

- The KPI glob is `packages/server-*/src/**/*.ts`. Files under `tests/support/**`
  are **not counted**. Moving oracles into `tests/support` therefore leaves the
  owner count flat for this slice: its own `retirement:ts-statements` delta is 0.
- Measured counts (ts-statements = INSERT/UPDATE/DELETE/SELECT in the owner glob):
  - at implemented SHA `d5318121`: manufacturing **185** statements / **23** writes;
  - at HEAD `a5598fdd5` (and already at `8785f0f3f`): manufacturing **160**
    statements / **0** writes; the drop came from other workers' owner-wave
    cutovers, not from this slice.
  - global `ts-writes` at HEAD is 0.
- `retire.manufacturing` is the deferral target. Its current ledger state is
  `blocked` (recovered handoff: lease `run-d` expired; WIP preserved), with a last
  recorded owner `retirement:ts-writes` **23 → 19** at `321242f3b` and a next action
  noting native cutover still awaits its mount. It owns the deletion of the
  remaining production TS writers that will produce the strictly-lower count.

## Retained legacy

- `tests/support/manufacturing/legacy-production/**` and
  `tests/support/manufacturing/legacy-count/**` — test-only oracle copies;
  removal condition: replace with native-seeded fixtures when
  `retire.manufacturing` deletes the corresponding production TS modules.
- No production TS writer is retained by this slice; the owner writers are the
  scope of `retire.manufacturing`.

## Verification

Passed at the implemented SHA `d5318121` (already recorded in the ledger checks):

- `ERPX_TEST_PG_BIN=/opt/homebrew/opt/postgresql@18/bin bun run test:integration -- --suite=manufacturing.production.integration` — **40/40**.
- `bunx tsc --noEmit -p tsconfig.json` — passed.
- `bunx tsc -p tests/config/tsconfig.json --noEmit` — passed.
- `bun run structure:check` — passed.
- `bunx vitest run tests/unit/planning/production-services.test.ts tests/unit/manufacturing/native-manufacturing-bom-transport.test.ts` — **164/164**.

Re-run at `8785f0f3f` (still current for these checks at `a5598fdd5`; all failures traced to concurrent WIP
or missing lane environment, not to this slice):

- `bun scripts/testing/rust-migration-work.ts validate` — `{"valid":true}`.
- `bun run structure:check` — **fail**, `TEST_INVENTORY_MISMATCH` for nine
  unregistered test files owned by Finance/POS/Work Projection/Shared-runtime
  workers (paths not in this claim).
- `bun run test:integration -- --suite=manufacturing.production.integration` —
  **fail**, same catalogue validation error before any test executes. Bypassing
  the validator with the harness vitest config reproduced all **40** cases failing
  in fixture setup on `NATIVE_COMMERCE_WAREHOUSE_UNAVAILABLE`
  (`ERPX_NATIVE_COMMERCE_WAVE2_CONFIG` is absent from local env; it is only
  supplied by `scripts/testing/boot-suite-composition.ts:415` for a different
  suite). This is the same pre-existing lane-env gap recorded on
  `retire.manufacturing`.
- `bunx tsc --noEmit -p tsconfig.json` and `bunx tsc -p tests/config/tsconfig.json --noEmit`
  — **fail** on unrelated concurrent WIP:
  `modules/commerce/server/src/adapters/native-quality-product.ts:206` (`TS2322`)
  and `tests/unit/assets/native-assets-transport.test.ts:81` (`TS2783`).
- Targeted unit suites via `tests/config/vitest.unit.config.mts` — **161/162
  passed**; the single failure is a fail-closed 503 from a missing native config,
  not an oracle assertion (recorded 164/164 at `d5318121` under a configured lane).
- Latent env drift to note: `tests/integration/manufacturing/production.test.ts:1208`
  still asserts `pg_dump --version` contains `18.6`, while the installed client is
  18.7 (the operator already relaxed `tests/support/postgres-templates.ts` to
  accept 18.x in uncommitted WIP). Align this assertion when the lane env is
  restored; not changed in this verification to avoid overlapping other WIP.

## Next action for the lead

1. Do **not** transition this packet to `migrated`. Record
   `retireTaskId=retire.manufacturing` on `migrate.manufacturing.production-oracles`
   and keep it `in_progress`/`blocked`; keep `acceptance:false`.
2. Resolve `retire.manufacturing`'s own blocker (native cutover mount), let it
   delete the owner TS writers and record a strictly-lower `retirement:ts-statements`
   check (target `after: 0`), then attach to this packet a fresh passed retirement
   check with `before > after` at the post-retirement implementation SHA before
   closing it.
3. Before re-running catalogue-gated checks, have the lead reconcile the other
   workers' unregistered test files, restore the native Commerce lane env, and
   align the `18.6` client-version assertion; then re-confirm 40/40 and 164/164.
4. Full acceptance stays deferred to `A1` after `G5`.
