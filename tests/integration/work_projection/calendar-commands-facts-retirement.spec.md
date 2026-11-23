# `retire.work_projection.calendar-commands-facts` — precise spec (facts verified; shim retained; acceptance deferred)

Status: **verified; the SQL writer is already fully retired; the residual TypeScript transport shim is NOT deleted; acceptance deferred.**
This verification performed no source change, did not mutate the ledger, and did not push.

## Identity and source

- Task `retire.work_projection.calendar-commands-facts`, goal `G3`, owner `work_projection`, workstream `S05`, plan `docs/production-plan.md#rust-migration-current`.
  Bounded workflow IDs: `quality/inspections`, `quality/results`, `quality/release-gates`, `quality/feature-cancellation`.
- Claim held for this verification: `runId=run-20261123-f`, `agentId=wA18` (generation 158). No claim, heartbeat, ledger field or run state was modified.
- HEAD used for every recorded check: `2373c3c4b591238a614d3041ac1dbb9da9e3afde` on `codex/rust-backend-migration`.
  The lead-referenced baseline `2068f46ca22aa4fa73ea6994be3abb2ba6625a8e` (native work-report HTTP + calendar read ports) was also inspected; the intervening commits are test-registration/catalogue/docs only and do not change the owner source.
- `bun scripts/testing/rust-migration-work.ts validate` → `{"valid": true}`; `status --json` shows coordinator `run-20261123-f` and my claim on this task.
- Claimed paths were clean at verification time: `git status --short -- packages/server-work_projection packages/feature-work_projection tests/unit/work_projection tests/integration/work_projection` was empty (the prior dirty `Cargo.toml` `work_report_read_http` test target was committed by the lead in `2373c3c4b`). Unrelated dirty WIP in `apps/*/globals.css`, `apps/*/turbo.json`, `docs/status/rust-delivery-bundles.json`, `Cargo.lock` and `packages/feature-pms/**` belongs to other writers and was not touched.

## What "remaining calendar-commands facts" actually are — verification verdict

The task scope said `adapters/calendar-commands-facts.ts` held 26 retained SQL write statements "with no native counterpart" at the step-0 audit (`46ea9d1c`, `before=41 / after=41`). That state no longer exists. At current source:

1. **Zero SQL remains in the TypeScript owner.** `git grep -I -o -E "INSERT INTO|DELETE FROM|UPDATE [a-z_]+ SET" HEAD -- ':(glob)packages/server-work_projection/src/**/*.ts' | wc -l` = **0** (same in the working tree). The owner write KPI was already driven `41 → 0` by `43ffc6a62c43bf89428606c6cb880370b17e0646` ("land axum command cutover for 15 owner waves, TS write KPI 994 → 0") and the read statements `12 → 0` by `1cfea241a4100be2e02346621f01d17558fae717` ("native calendar_read port 12→0, fail-closed + parity").
2. **All 26 write facts are native-backed.** `packages/server-work_projection/src/adapters/calendar-commands-facts.ts` is a pure fail-closed transport shim: `write14,15,16,18,19,20,21,22,24,29,31,32,33,34,35,36,39,40,45,48,50,52,54,56,57` dispatch through `nativeOwnerWrite('<wp_cal_*>', …)` and `read44` through `nativeOwnerWriteRow('wp_cal_attendee_reminder_update', …)` to the private `/work/owner-write` axum route. Every one of the 25 distinct `wp_cal_*` kinds has (a) a constant in `domain/owner_write_registry.rs`, (b) a real SQL statement in `adapters/owner_write_postgres.rs`, and (c) an allow-listed, arity-fenced entry in the frozen oracle `tests/unit/work_projection/rust-work-projection-calendar-spec.test.rs`. No SQL mirror, JSON SQL fallback or TS writer remains in the file; `noop = () => db.prepare("SELECT 1")` is an inert placeholder that only keeps caller batches structurally identical.
3. **All 12 calendar read facts are native-backed.** `calendarCommandsUnit` binds `read1,3,4,12,13,23,26,30,37,38,42,53` to `nativeCalendarRead` (`packages/feature-work_projection/src/lib/native-calendar-read.ts`), which posts allow-listed `wp_cal_read_*` kinds to `/work/owner-read`; the SQL is in `adapters/calendar_read.rs` and fenced by `rust-work-projection-calendar-read-spec.test.rs` plus the Postgres parity suite.
4. **The work-report HTTP read port also landed and is green.** `work_report_read_http.rs` / `public/work_report_read_http.rs` are covered by `tests/unit/shared.work/rust-work-report-http.test.rs` (3/3), now registered as a Cargo test target in `packages/server-work_projection/Cargo.toml`.

**Verdict:** there are no unmigrated calendar-commands facts. The only residue is the *transport shim file itself* plus its public barrel. Per the plan's classification, retained native-transport code is allowed presentation/transport with an explicit removal task; this spec is that task's precise removal plan. Deleting the shim is structural cleanup with **no KPI delta** (owner writes already `after: 0`), so the retirement KPI gate can be closed independently of the deletion.

## Preconditions still required before deleting `calendar-commands-facts.ts` (precise bounded slice)

| Required before deleting the adapter + barrel | Observed at `2373c3c4b` | Verdict |
| --- | --- | --- |
| No live TS caller of `calendarCommandsWorkProjectionFacts` | `packages/server-work_projection/src/adapters/calendar/commands.ts:15` imports it and `calendarCommandsUnit` binds 26 `write*` + `read44` through it. This is inside this task's `allowedPaths` (`packages/server-work_projection/src/**`). | **MET by the slice below (lead or next worker)** |
| Native counterpart complete | All kinds execute real SQL in Rust; frozen oracles pass. | **MET** |
| Remove the retired package export | `packages/server-work_projection/package.json` still exports `./adapters/calendar-commands-facts` and `./public/calendar-commands-facts`. | **MET by the slice below** |
| Regenerate derived cross-app config | `@source` lines in ~24 `apps/*/src/app/globals.css` and inputs in `apps/*/turbo.json` still name both deleted paths. Those files are lead/shared-owned and dirty with other writers' WIP; they are outside this task's `allowedPaths`. | **BLOCKED — lead integration step** |
| Native write/read transport provisioned in authorized dev | `ERPX_NATIVE_WORK_PROJECTION_OWNER_WRITE_CONFIG` is absent from `.env`, `.env.local` and `.scratch/dev/v5.json`; `nativeCalendarRead` can reuse the write config but both absent means fail-closed. This is expected during migration and is not a reason to keep a TS writer, but calendar commands cannot be browser-smoked until the lead provisions a registered loopback Holding config. | **LEAD/ENV — note, not a code blocker** |

No file was deleted because the derived-config regeneration cannot stay inside one worker's claimed paths; doing it now would leave stale generated references across shared app files owned by other writers.

## Exact next actions (ordered; lead executes or assigns)

1. **Close the KPI gate now if desired.** Record `retirement:ts-writes -- ':(glob)packages/server-work_projection/src/**/*.ts'` with `before=41, after=0` at implementation SHA `43ffc6a62c43bf89428606c6cb880370b17e0646` (already present in the task's `checks`), and `retirement:ts-statements before=12, after=0` at `1cfea241a4100be2e02346621f01d17558fae717`. Both are strictly decreasing and already pass; no new writer removal is needed for the KPI.
2. **Add a native write bridge.** In `packages/server-work_projection/src/adapters/native-calendar-write.ts` (or `packages/feature-work_projection/src/lib/native-calendar-write.ts`, mirroring `native-calendar-read.ts`), export a `nativeCalendarWrite(db)` factory that reproduces the exact 26 dispatchers from the current shim using `nativeOwnerWrite` / `nativeOwnerWriteRow` and the same kind names, positional arities, idempotency keys and company scoping. Keep the file fail-closed with no SQL.
3. **Rewire the one caller.** In `packages/server-work_projection/src/adapters/calendar/commands.ts`, change the import at line 15 and the 26 bindings at lines 40–75 from `calendarCommandsWorkProjectionFacts` to `nativeCalendarWrite`. Preserve `read44` through the row-returning variant.
4. **Delete the shim and barrel.** Remove `packages/server-work_projection/src/adapters/calendar-commands-facts.ts` and `packages/server-work_projection/src/public/calendar-commands-facts.ts`, and delete their two `package.json` `exports` entries. Prove with `git grep -n "calendarCommandsWorkProjectionFacts\|public/calendar-commands-facts" -- '*.ts' '*.tsx'` that no importer remains.
5. **Regenerate derived config (lead-owned).** Re-run the workspace style/ Turbo input generators so the deleted `@source` and `$TURBO_ROOT$` inputs disappear from all `apps/*/globals.css` and `apps/*/turbo.json`. Do not hand-edit those dirty files in this worker's slice.
6. **Re-verify and record.** Rerun the scoped checks below, confirm the owner KPI stays `0`, and confirm the frozen oracles still pass. Leave `acceptance:false`; the lead integrates, stages only the claimed slice, verifies the push and records the evidence.

## Checks performed at `2373c3c4b591238a614d3041ac1dbb9da9e3afde`

- `bun scripts/testing/rust-migration-work.ts validate` → `{"valid": true}`.
- Owner write KPI (`git grep … ':(glob)packages/server-work_projection/src/**/*.ts' | wc -l`) → **0**.
- `cargo check -p erpx-server-work-projection --offline` → **passed** (`Finished dev profile`; 2 pre-existing warnings in `work_snapshot_sources.rs` only).
- `cargo test -p erpx-server-work-projection --test work_projection_calendar_spec --test work_projection_calendar_read_spec --test work_report_read_http --offline` → **4/4 + 3/3 + 3/3 passed** (native owner command table, RETURNING set/reply envelope, read table, HTTP mount for registered holding, work-report route/target). No database or network required.
- `bunx tsc --noEmit -p packages/server-work_projection/tsconfig.json` → **fails on 2 errors outside the claimed owner**: `packages/feature-pms/src/adapters/server/native-pms.ts(713,67)` and `(726,24)`, both `TS18046 'value' is of type 'unknown'`. That file is dirty WIP from another writer (`git status --short` shows ` M`), not part of this task; no error is reported in `packages/server-work_projection/**` or `packages/feature-work_projection/**`. Owner-local TypeScript is therefore clean; the lead should re-run this once the PMS writer resolves its errors.
- Environment note (informational only; no dev/staging/private service was modified): `ERPX_NATIVE_WORK_PROJECTION_OWNER_WRITE_CONFIG` is absent from the checked `.env`/`.env.local`/v5 config, so a configured-but-missing native calendar write fails closed by design. No browser smoke was attempted; browser/provider verification remains deferred to acceptance.
