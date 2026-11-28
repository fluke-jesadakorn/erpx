# `retire.manufacturing.accounting-dead-writers` — retirement spec

Status: **owner deletion and unit oracle landed; fixture repoint blocked at runtime; acceptance deferred.**

This worker did **not** mutate the ledger or push. The lead owns the tracked record and the integration/push.

## Identity and source

- Task `retire.manufacturing.accounting-dead-writers`, goal `G4`, owner `manufacturing`,
  plan `docs/production-plan.md#rust-migration-current`.
- Claim held (not modified): `runId=run-20261123-f`, `agentId=wU03`, `generation=158`.
- Shared coordinator read-only `status --json` and `validate` returned `{"valid":true}`.
- HEAD at first read: `a9ac2176b253712370174b7b0008afccd4a8d83d`; the lead advanced HEAD during
  the work to `ec6caf0158473857beceedbca3f72caf4ed251c4`. All checks below were re-run on
  `ec6caf01`. `git status --short` on the claimed paths was clean of concurrent WIP at first read.

## Owner deletion (all inside the held paths)

- `modules/manufacturing/server/src/adapters/accounting.ts`
  — deleted the exported `createManufacturingAccountingProfile` / `reviewManufacturingAccountingProfile`
  writers, the `profile` (`SELECT *`) and `guardProfileVersion` (`EXISTS(SELECT 1 …)`) reads,
  the now-dead `prepareProfile` / `prepareProfileReview` fail-closed stubs and their
  `nativeProfileUnavailable` helper, and the `holdingResources`, `profileSchema`, `reviewSchema`
  and `withManufacturingPurchasedCommand` imports. Kept `approvedProfile`, `guardApprovedProfile`,
  `prepareManufacturingPosting` and the native posting-profile entrypoints
  (`prepareManufacturingPostingProfile`, `decideManufacturingPostingProfile`).
- `modules/manufacturing/server/src/application/accounting.ts`
  — deleted both application writers, the `profile` / `guardProfileVersion` repository members,
  the `prepareProfile` / `prepareProfileReview` members, the `companyFinance` helper and the
  `profileSchema` / `reviewSchema` / `manufacturingAccountKind` import. Kept `approvedProfile`,
  `guardApprovedProfile` and `prepareManufacturingPosting`; the factory now returns only
  `{prepareManufacturingPosting}`.
- `modules/manufacturing/server/src/public/production-accounting.ts`
  — now exports only `prepareManufacturingPosting`. Existing `prepareManufacturingPosting` type
  consumers (`adapters/production.ts`, `application/production.ts`,
  `tests/support/manufacturing/legacy-production/**`) are unaffected.

## Oracles

- `tests/unit/manufacturing/native-manufacturing-bom-transport.test.ts`
  — the owner SELECT KPI oracle asserted `156` but the code at HEAD was already `155`
  (stale from an earlier committed drop); after removing the two profile SELECTs the measured
  count is **153**. The oracle name and assertion were moved to `153`.
- `tests/integration/manufacturing/production.test.ts`
  — the missing-mapping fail-closed case now calls the native
  `posting-profiles.prepareManufacturingPostingProfile` entrypoint (with a scheme-valid
  context key) instead of the deleted TS writer; the "no orphan effects" image assertion is
  preserved. All other profile cases in that file already used the native posting-profile
  entrypoints.

## Fixture — blocked, follow-up recorded

`fixtures/shared/builders/engineering-business-browser.ts` no longer imports the deleted writer.
The native HTTP entrypoint (`nativeManufacturingProfile`) needs a configured
`ERPX_NATIVE_MANUFACTURING_ACCOUNTING_CONFIG` transport and uses `holdingResources(actor.holdingId)`,
i.e. the shared Holding database — not the disposable `erpx_browser_<run>_h001_db` handle this
fixture seeds through. Calling it here would fail closed with 503 and write to the wrong database.
The fixture therefore seeds the exact approved native-owned profile row with its own `db` handle
(status `approved`, version `2`, independent `created_by` / `reviewed_by`), matching the pattern
already used in `tests/integration/manufacturing/production.test.ts`.

Follow-up for the lead: when the browser lane has a configured native Manufacturing transport and
a native-seeded fixture builder that targets the disposable browser database, replace the direct
row seed with `manufacturingPostingProfiles` + `prepareManufacturingPostingProfile` +
`decideManufacturingPostingProfile` and assert the native receipt/version parity.

## KPI (measured, not recorded in the ledger by this worker)

- Command: `git grep -I -o -E "INSERT INTO|DELETE FROM|UPDATE [a-z_]+ SET|SELECT " -- ':(glob)modules/manufacturing/server/src/**/*.ts' | wc -l`
- Before (at `ec6caf01` with this worker's deletion unapplied): **155**. After: **153**.
- The unit oracle's `\bSELECT\b` word count gives the same 155 → 153.
- The task's `156 -> 154` figure is the prior ledger check's stale baseline (last recorded
  `156 -> 155` at `b6ae3759b`); the source had already dropped one statement. The real delta for
  this slice is two statements, 155 → 153. The lead should record the actual measured values so
  the check matches the code and does not regress the latest recorded `after: 155`.

## Checks (all at `ec6caf01`)

- `cargo check -p erpx-manufacturing-server --offline` — passed (two pre-existing unused-import
  warnings in `accounting_profile_wave3.rs` and `owner_write_wave3.rs`, untouched by this slice).
- `ERPX_TEST_ADMIN_URL` from local `.env.local` (not printed) `cargo test -p erpx-manufacturing-server --offline`
  — passed: 63 tests across lib and all unit/PG integration targets.
- `bunx tsc --noEmit -p modules/manufacturing/server/tsconfig.json` — passed (exit 0) at
  `ec6caf01` earlier in this run. A later re-run at the same HEAD is blocked by a concurrent
  worker's uncommitted `modules/projects/server/src/adapters/{structure,native-project-graph}.ts`
  edits (`TS2307` then `TS2554`), which this owner's tsconfig resolves as a dependency; neither
  error is in a claimed path.
- `bunx vitest run tests/unit/manufacturing/native-manufacturing-bom-transport.test.ts --config tests/config/vitest.unit.config.mts`
  — passed 8/8 (KPI oracle included).
- `bunx tsc -p tests/config/tsconfig.json --noEmit` — **fails only in
  `tests/unit/planning/production-services.test.ts`**: five errors for the removed `profile`
  repository member and the removed `createManufacturingAccountingProfile` /
  `reviewManufacturingAccountingProfile` application methods. That file is at
  `tests/unit/planning/**`, outside this task's `allowedPaths`.
- `bunx vitest run tests/unit/planning/production-services.test.ts --config tests/config/vitest.unit.config.mts`
  — 151 passed / 4 failed: three assertions call the deleted application methods, one is the
  pre-existing fail-closed native-config 503 recorded in earlier specs.

## Next actions for the lead

1. Reassign or extend a claim to `tests/unit/planning/production-services.test.ts` and delete its
   three retired create/review assertions (lines 47, 48, 50) and the `profile` repository fact in
   the `accounting()` helper (line 20). The WIP-separation, THB-chart and independent-review rules
   are covered by the Rust owner tests (`domain/accounting_profile_wave3::tests`); the retained
   `approvedProfile` posting assertions at lines 51–55 stay.
2. Record the retirement check with the measured `before: 155, after: 153` at the implementation
   SHA, keep `retire.manufacturing` residual classification current, and leave `acceptance:false`.
3. The integration suite still needs the lead's catalogue reconciliation and native lane config
   before `manufacturing.production.integration` can be re-verified end-to-end; this slice's
   fail-closed assertion is byte-for-byte consistent with the existing native-entrypoint case.
4. Full acceptance stays deferred to `A1` after `G5`.
