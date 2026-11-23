# G3 / retire.legal.destruction — composition mount spec (blocked)

Status of this worker pass: the native authority exists and compiles, but the
lead-owned composition mount does not. No production code was changed, no
oracles were moved, no ledger or remote was mutated. Acceptance stays deferred.

## Verified state

- Claim: `run-20261123-e` / `worker-legalD2` (generation 155), task
  `retire.legal.destruction`; ledger `status --json` revision 6040 and `validate`
  both pass. Working tree has no diff in this packet's allowed paths.
- Source: HEAD `cb6d2d22e86b5f4dd603ba953b2ca3b48758dc90`; native authority
  landed at `ef6639fa6ed7b0765a352ab204b6b8045cb7faf9` and is an ancestor of
  HEAD. No later commit touches Legal composition sources.
- Native authority is additive and exported:
  `packages/server-legal/src/adapters/retention_execution.rs:324` defines
  `execute_with_sources<B: PgLegalDestructionBridge>(pool, bridge, company,
  destruction, now, identifiers) -> Result<LegalDestructionExecution, Error>`;
  `packages/server-legal/src/public/mod.rs:131-134` re-exports it with
  `LegalRetentionUnit`, `AuthorizedLegalDestructionBridge`, `PgIndependentReview`
  and `LegalDestructionExecution`. The landed comment states TS orchestration is
  retained pending composition mount.
- Mount is ABSENT: `src/composition/src/legal.rs` has no destruction port or
  `execute_with_sources` call (only the seven unrelated `execute_with_sources_deferred`
  mounts at lines 337, 382, 428, 476, 532, 740, 869). `src/composition/src/legal.rs:84-90`
  still has the no-op `LegalSources`, with no destruction source lease.
- Real bridge is ABSENT: the only `PgLegalDestructionBridge` implementations in
  the repo are test fakes at `tests/integration/legal/rust-auxiliary-postgres.test.rs:959`
  and `:1039`. The trait (`packages/server-legal/src/adapters/postgres_destruction.rs:26-87`)
  requires transaction-bound `prepare`, `assert_current`, source/link/ledger reads,
  `destroy` and `seal`; the manifest extension is at `:93-112`.
- Rehearsal still uses the TS writer: `scripts/testing/fixtures/legal-isolated-destruction-uat.ts:7,49`
  imports and calls `drainLegalDestruction` via
  `packages/server-legal/src/public/legal/retention.ts:1` →
  `packages/server-legal/src/adapters/legal/retention.ts:17` →
  `packages/server-legal/src/adapters/legal/destruction-worker.ts:46`. The same
  oracle is used by `tests/integration/legal/legal-retention-sources.test.ts` and
  `tests/integration/legal/legal.test.ts:745,837`.
- `destruction` is deliberately not a recurring TS job
  (`packages/server-runtime/src/adapters/runtime/jobs.ts:207-208`); it is invoked
  explicitly by isolated rehearsals. The native `legal-worker` CLI
  (`src/composition/src/main.rs:19-69`, `src/composition/src/legal_runtime_worker.rs:436`)
  currently owns only processing/OCR, not destruction.

## Lead prerequisite (lead-owned; outside this packet's allowedPaths)

1. Implement a real composition `PgLegalDestructionBridge` plus
   `PgLegalDestructionManifestBridge` supplying current IAM (`iam_same_person`
   and assignment/role authority), Commercial purchase/history checks, and Files
   destroy/seal with the same transaction-bound authorization the TS path uses
   (`packages/server-legal/src/adapters/legal/destruction-worker.ts:1-44`). Stored
   actor/manifest JSON alone must never be an authority witness.
2. Add a destruction source-preparation type beside `LegalSources` in
   `src/composition/src/legal.rs` that acquires the exact same-unit lease used by
   the native tests (Control + Legal/Files sources), then mount
   `execute_with_sources` with `WorkAuthorizationRequest` permission
   `legal.destroy`, module `legal`, destination `legal/retention`, `api: true`,
   mirroring `src/composition/src/legal_processing_status.rs:58-82`.
3. Expose one native invocation path for the isolated rehearsal: either a
   `legal-worker --destruction` one-shot CLI using the existing private runtime
   config, or an authorized native route with a full native client in
   `packages/feature-legal/**`. Fail closed when the bridge/config is absent; no
   TS fallback. Report the exact command/route and payload so the oracle can be
   repointed.

## Worker slice to execute immediately after the mount lands

All paths below are already in this packet's allowedPaths; no split is needed.

1. Repoint `scripts/testing/fixtures/legal-isolated-destruction-uat.ts` from
   `drainLegalDestruction` to the native invocation, preserving the frozen
   attestation/fingerprint/enrollment gates (lines 13-19, 25, 36), the exact
   queue-head check (line 39), and the fault/resume/hold assertions (lines 26-30,
   44-57). The native path must produce the same evidence JSON shape at lines 31
   and 59.
2. Move the destruction oracles in `tests/integration/legal/legal-retention-sources.test.ts`
   (crash/retry/hold/compat cases) and `tests/integration/legal/legal.test.ts:745,837`
   to native seeding/execution, keeping the injected post-byte-deletion fault and
   replay assertions.
3. Delete `packages/server-legal/src/adapters/legal/destruction-worker.ts` and
   `packages/server-legal/src/application/destruction-worker.ts`; remove the
   re-exports at `packages/server-legal/src/adapters/legal/retention.ts:17` and
   `packages/server-legal/src/public/legal/retention.ts:1`. Keep shared source
   validation/manifest helpers, which native paths still consume. Confirm no live
   TS caller remains (`drainLegalDestruction` references must reach zero).
4. Record `retirement:ts-writes` before/after for owner legal; current scoped
   count is already 0 write statements (the 14 former writers were retired by
   `retire.legal.destruction-worker` at `c0a24c8136`). The delta here is deletion
   of the 8 remaining SELECT/orchestration statements in the worker plus its 81
   line application driver; record a `retirement:ts-statements` pair with a
   strictly lower `after` if the write count cannot decrease, and clear the
   retainedLegacy entry once the file is gone.
5. Checks to run on the resulting committed source: `cargo check -p erpx-server-legal --lib`,
   `cargo test -p erpx-server-legal --lib`, the native destruction PostgreSQL tests
   (`--test auxiliary_postgres destruction:: -- --test-threads=1`), scoped
   `bunx tsc --noEmit --project packages/server-legal/tsconfig.json`, and the
   burn-in TS legal suites. Do not flip catalogue certification; acceptance
   remains deferred.

## Checks already run on current HEAD `cb6d2d22e86b`

- `cargo check -p erpx-server-legal --lib` — passed (only pre-existing warnings in
  `erpx-server-collaboration`; Legal itself clean). The instruction's `-p legal`
  is not a package name; the crate is `erpx-server-legal`.
- `bunx tsc --noEmit --project packages/server-legal/tsconfig.json` — passed, exit 0.
- `bun scripts/testing/rust-migration-work.ts validate` — `{"valid": true}`;
  `status --json` confirms the claim and shows `retire.legal.destruction` open.
- Scoped write KPI: 0 INSERT/UPDATE/DELETE and 352 total SQL statements under
  `packages/server-legal/src/**/*.ts`; 8 SELECTs and 0 writes in the destruction
  worker itself.

## Next action

Lead: land the three prerequisites above in composition (paths `src/composition/src/**`,
outside this claim) and report the exact native invocation plus the new source
SHA. Then re-dispatch this packet; the worker slice in the previous section is
ready to execute without further scope refinement.
