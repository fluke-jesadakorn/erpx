# `retire.planning.fixture-oracle` — precise retirement spec (deletion precondition unmet)

Status: **verified; legacy oracle NOT deleted; acceptance deferred.**
This verification performed no source change and did not mutate the ledger or push.

## Identity and source

- Task `retire.planning.fixture-oracle`, goal `G4`, owner `planning`, workstream `S08`, plan `docs/production-plan.md#rust-migration-current`.
- Claim held for this verification: `runId=run-20261123-e`, `agentId=worker-planF`, `generation=155`, lease expires `2026-11-23T14:55:01.903Z`. No claim was modified.
- HEAD read and used for every check: `ab4d98bb1ba585d092a103a4891bff0ca79df385` on `codex/rust-backend-migration`; `executionPlan.revision=6051`; `rust-migration-work.ts validate` → `{"valid":true}`.
- Working tree carries unrelated dirty WIP from other owners. `git status --short -- modules/planning/server tests/support/planning tests/integration/manufacturing` is empty — no concurrent WIP in the claimed paths.
- Crate correction: the Cargo package is `erpx-server-planning`. `cargo check -p planning` is invalid (`package ID specification planning did not match any packages`); use `cargo check -p erpx-planning-server --offline`.

## Precondition verdict

| Required before deleting `tests/support/planning/legacy` | Observed at HEAD | Verdict |
| --- | --- | --- |
| Native Planning service configured in authorized dev | Native binary is up (`erpx-server private-api --development-config .scratch/dev/v5.json`, `127.0.0.1:3198`, `/api/health` 200). But `ERPX_NATIVE_PLANNING_CONFIG` is **absent** from `.env`, `.env.local` and the dev gateway process on `127.0.0.1:3100` (which has many other `ERPX_NATIVE_*` configs). The Next Planning transport/presenter therefore stays unset/fail-closed, not native-authoritative. | **NOT MET** |
| Full `manufacturing.production.integration` passes with native-seeded fixtures | `tests/integration/manufacturing/production.test.ts:14-18` and `:26` still import `createPlanningScenario`, `cancelPlanningScenario`, `reviewPlanningRecommendation`, `productionWorkspace`, `saveProductionResource`, `saveProductionResourceCalendar` from `../../support/planning` (the fixture-only SQL oracle). Suite execution fails before any test at `TEST_INVENTORY_MISMATCH` for 11 unregistered test files owned by other workers; the recorded lane-env failure is `NATIVE_COMMERCE_WAREHOUSE_UNAVAILABLE` (missing `ERPX_NATIVE_COMMERCE_WAVE2_CONFIG`). | **NOT MET** |
| Deletion stays inside this worker's narrowed paths | The legacy barrel is also imported by `fixtures/shared/builders/business-worker-sources.ts:17`, `fixtures/shared/builders/engineering-business-browser.ts:15` and `tests/unit/planning/production-services.test.ts:6-7`. Those paths are outside `server-planning`, `tests/support/planning`, `tests/integration/manufacturing`. Deleting the barrel now breaks all three. | **BLOCKED — needs lead action** |

Consequence: deleting the legacy tree today would neither prove a native-seeded suite nor compile the three external consumers. No file was changed.

## Exact next actions (ordered; lead executes or assigns)

1. **Configure dev lane.** Set `ERPX_NATIVE_PLANNING_CONFIG` in the authorized dev environment with a registered loopback private base and the exact Holding entry, e.g. `{"privateBaseUrl":"http://127.0.0.1:<owned-port>","timeoutMs":10000,"holdings":[{"origin":"https://acme.jesadakorn.com","holdingSlug":"acme","holdingId":"h001"}]}`. Set `ERPX_NATIVE_MANUFACTURING_CONFIG` and `ERPX_NATIVE_COMMERCE_WAVE2_CONFIG` for the same lane. Do not use reserved ports (3000/3001/3100/3101/3103/8080, 5432/5433/5435, 3200-3299). Confirm a configured-but-invalid native read fails closed rather than falling back (pattern in `tests/unit/planning/native-planning-workspace-transport.test.ts`).
2. **Reconcile the catalogue.** Lead registers or removes the 11 unregistered files named by `TEST_INVENTORY_MISMATCH` so `bun run test:integration -- --suite=manufacturing.production.integration` reaches fixture setup. Restore the native Commerce lane env.
3. **Extend ownership or reassign.** Either extend this task's claim to `fixtures/shared/**` and `tests/unit/planning/**` (both are in the ledger `allowedPaths` but not this worker's narrowed remit), or assign those three consumers to their owners. They must switch to native-seeded builders before the barrel can go.
4. **Rewire the integration oracle.** In `tests/integration/manufacturing/production.test.ts`, replace the six imported planning helpers with a native-seeded fixture helper whose seed path is the native Planning command/effect proven by `tests/integration/planning/rust-planning-command-postgres.test.rs` and `tests/unit/planning/rust-planning-*.test.rs`. Preserve every existing assertion (scenario feasibility/allocations/comparison, capacity-release conflict, independent review replay, event log, calendar immutability). Remove the `from "../../support/planning"` imports at lines 18 and 26.
5. **Delete the legacy tree.** Remove `tests/support/planning/index.ts` and all seven files under `tests/support/planning/legacy/`. Prove with `git grep -n "support/planning" -- '*.ts' '*.tsx'` that no importer remains.
6. **Re-verify and record.** Re-run the full suite (`40/40` native-seeded), the scoped checks below, and a retirement check whose `after` reaches 0, e.g. `grep -rI -o -E "INSERT INTO|DELETE FROM|UPDATE [a-z_]+ SET|SELECT " tests/support/planning | wc -l` = **41 before → 0 after** at HEAD. Note these fixture files are outside the KPI glob `packages/server-*/src/**/*.ts`, so owner KPI is unchanged: at HEAD planning `writes=0`, `statements=51` (51 residual classified under `retire.planning.read-sources`), global `writes=0`. Keep `acceptance:false`; the lead integrates, verifies the push and records the check.

## Checks performed at `ab4d98bb1b`

- `bun scripts/testing/rust-migration-work.ts validate` → `{"valid":true}`.
- `cargo check -p erpx-planning-server --offline` → **passed** (`Finished dev profile` after a file-lock wait).
- `bunx tsc --noEmit -p modules/planning/server/tsconfig.json` → **passed** (exit 0).
- `cargo check -p planning --offline` → fails as invalid package name (see crate correction above).
- `bun run test:integration -- --suite=manufacturing.production.integration` → **fails before execution** on `TEST_INVENTORY_MISMATCH` (11 files: finance 2, pos 2, work_projection 3, workflow 2, shared.runtime 1, unit/work_projection 2 — all other owners' WIP).
- `bun run structure:check` → fails on the same pre-existing 11-file catalogue mismatch; this spec's `.spec.md` is not discovered as a test and is not in the mismatch list.
- Environment note: `http://127.0.0.1:3100/api/health` → 200; `https://console-dev-erpx.jesadakorn.com/api/health` → 530; `http://127.0.0.1:3101/api/health` → connection failed. These dev/private-service issues are outside the claimed paths and were not modified.
