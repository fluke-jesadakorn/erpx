# G3 / retire.legal.destruction — mount verification (still blocked)

Status of this worker pass (`run-20261123-e` / `worker-legalD3`): the native
authority exists and compiles, but the lead-owned composition mount is still
ABSENT. No production code was changed, no oracle was moved, no ledger or remote
was mutated. Acceptance stays deferred.

## Verified state (re-checked this pass)

- Working copy: branch `codex/rust-backend-migration`, HEAD
  `9e63520b096f86c6d230e05dd319bfb9145afa0f`; claim is held by
  `worker-legalD3` (generation 155). `validate` returns `{"valid": true}`;
  `status --task=retire.legal.destruction --json` returns revision 6112 with the
  claim active and `baseSha` `4b1723460963f08f7c236e7fb223b3c1d56d269f`.
  Working tree has NO diff in any of this packet's `allowedPaths` (Legal source,
  feature-legal, the isolated rehearsal fixture, or legal tests).
- Earlier spec HEAD `cb6d2d22e86b5f4dd603ba953b2ca3b48758dc90` and the native
  authority commit `ef6639fa6ed7b0765a352ab204b6b8045cb7faf9` are both ancestors
  of current HEAD; no later commit touches Legal composition sources.
- Native authority is additive and exported:
  `packages/server-legal/src/adapters/retention_execution.rs:324` defines
  `execute_with_sources<B: PgLegalDestructionBridge>(pool, bridge, company,
  destruction, now, identifiers: &mut impl IdentifierSource) ->
  Result<LegalDestructionExecution, Error>`;
  `packages/server-legal/src/public/mod.rs:126-134` re-exports it with
  `LegalRetentionUnit`, `AuthorizedLegalDestructionBridge`,
  `PgIndependentReview`, `LegalDestructionExecution` and the journal/manifest
  functions. `LegalDestructionExecution` variants are `NoCandidate`, `Blocked`,
  `Journaled`, `Completed` (`:299`).
- Mount is ABSENT: `src/composition/src/legal.rs` has NO destruction port and NO
  non-deferred `execute_with_sources` call — only seven unrelated
  `execute_with_sources_deferred` mounts (lines 337, 382, 428, 476, 532, 740,
  869). `src/composition/src/legal.rs:84-90` still has the no-op `LegalSources`
  (`prepare` returns `Ok(())`), with no destruction source lease.
- Real bridge is ABSENT: the ONLY implementation of `PgLegalDestructionBridge`
  in the repo is a test fake at
  `tests/integration/legal/rust-auxiliary-postgres.test.rs:1016` (and
  `PgLegalDestructionManifestBridge` at `:1096`). The trait
  (`packages/server-legal/src/adapters/postgres_destruction.rs:26-87`) requires
  transaction-bound `prepare`, `assert_current`, `locked_source`, `link_current`,
  `source_shared`, `retained_ledger`, `object_shared`, `destroy`, `seal`. Stored
  actor/manifest JSON is never an authority witness (trait doc `:19-21`).
- Rehearsal still uses the TS writer:
  `scripts/testing/fixtures/legal-isolated-destruction-uat.ts:7,49` imports and
  calls `drainLegalDestruction` via
  `packages/server-legal/src/public/legal/retention.ts:1` →
  `packages/server-legal/src/adapters/legal/retention.ts:17` →
  `packages/server-legal/src/adapters/legal/destruction-worker.ts:46`. The same
  oracle is used by `tests/integration/legal/legal-retention-sources.test.ts:9,43,45,48,73,126`
  and `tests/integration/legal/legal.test.ts:745,837`.
- The `@erpx/server-collaboration/adapters/native-legal-destruction-worker`
  dependency inside `destruction-worker.ts` is NOT the Rust authority port: it
  is a 29-line TS helper wrapping `nativeSystemNotificationStatement` SQL
  statements. It does not implement `PgLegalDestructionBridge` and cannot
  substitute for the composition mount.
- `destruction` is deliberately not a recurring TS job
  (`packages/server-runtime/src/adapters/runtime/jobs.ts:207-208`); it is invoked
  explicitly by isolated rehearsals. The native `legal-worker` CLI
  (`src/composition/src/main.rs:19-69`,
  `src/composition/src/legal_runtime_worker.rs:436`) currently owns only
  processing/OCR, not destruction.
- Checks run on current HEAD (passed): `cargo check -p erpx-server-legal --lib`
  (Legal crate clean; only pre-existing warnings in
  `erpx-server-identity-access`); `bunx tsc --noEmit --project
  packages/server-legal/tsconfig.json` (exit 0). The instruction's `-p legal`
  is not a package name; the crate is `erpx-server-legal`.
- Scoped KPI at HEAD: 0 INSERT/UPDATE/DELETE write statements and 350 total SQL
  verbs under `packages/server-legal/src/**/*.ts`; the destruction worker itself
  holds 8 SELECTs and 0 writes. Legal writes already reached 0 at
  `retire.legal.destruction-worker` / `c0a24c8136`; this packet's delta is the
  deletion of those 8 SELECT/orchestration statements and the 81-line
  application driver.

## Lead prerequisite (lead-owned; outside this packet's allowedPaths)

1. Implement a real composition `PgLegalDestructionBridge` (and
   `PgLegalDestructionManifestBridge` where manifest minting is needed) supplying
   current IAM (`iam_same_person` and assignment/role authority), Commercial
   purchase/history checks, and Files destroy/seal with the same transaction-bound
   authorization the TS path uses
   (`packages/server-legal/src/adapters/legal/destruction-worker.ts:1-44`, trait
   contract `postgres_destruction.rs:19-112`). Stored actor/manifest JSON alone
   must never be an authority witness; `destroy` must revalidate within the unit
   and be replay-safe after partial deletion.
2. Add a destruction source-preparation type beside `LegalSources` in
   `src/composition/src/legal.rs` that acquires the exact same-unit lease used by
   the native tests (Control + Legal/Files sources, modeled on `StatusSources`
   at `src/composition/src/legal_processing_status.rs:32-40`), then mount
   `execute_with_sources` with `WorkAuthorizationRequest` permission
   `legal.destroy`, module `legal`, destination `legal/retention`, `api: true`,
   mirroring `legal_processing_status.rs:58-82`. Supply a real `IdentifierSource`
   (`packages/server-shared/src/application/ports.rs:9`) for journal/event IDs.
3. Expose exactly ONE native invocation path for the isolated rehearsal: either
   a `legal-worker --destruction` one-shot CLI using the existing private runtime
   config, or an authorized native route with a full native client in
   `packages/feature-legal/**`. Fail closed when bridge/config is absent; no TS
   fallback. Report the exact command/route and payload plus the resulting new
   source SHA so the oracle can be repointed.

## Worker slice to execute immediately after the mount lands

All paths below are already in this packet's `allowedPaths`; no split is needed.

1. Repoint `scripts/testing/fixtures/legal-isolated-destruction-uat.ts` from
   `drainLegalDestruction` to the native invocation, preserving the frozen
   attestation/fingerprint/enrollment gates (lines 12-22), the exact queue-head
   check (line 39), and the fault/resume/hold assertions (lines 34-56). The native
   path must produce the same evidence JSON shape at lines 33 and 58.
2. Move the destruction oracles in
   `tests/integration/legal/legal-retention-sources.test.ts` (crash/retry/hold/
   compat cases at lines 43, 45, 48, 73, 126) and
   `tests/integration/legal/legal.test.ts:745,837` to native seeding/execution,
   keeping the injected post-byte-deletion fault and replay assertions.
3. Delete `packages/server-legal/src/adapters/legal/destruction-worker.ts` and
   `packages/server-legal/src/application/destruction-worker.ts`; remove the
   re-exports at `packages/server-legal/src/adapters/legal/retention.ts:17` and
   `packages/server-legal/src/public/legal/retention.ts:1`. Keep shared source
   validation/manifest helpers, which native paths still consume. Confirm no live
   TS caller remains (`drainLegalDestruction` references must reach zero, and the
   `@erpx/server-collaboration/adapters/native-legal-destruction-worker` import
   must reach zero).
4. Record `retirement:ts-writes` before/after for owner legal (0 -> 0) and a
   `retirement:ts-statements` pair (350 -> 342, strictly lower) for the 8 removed
   SELECT/orchestration statements plus the 81-line driver. Clear the
   `retainedLegacy` entry once the file is gone.
5. Checks on the resulting source: `cargo check -p erpx-server-legal --lib`,
   `cargo test -p erpx-server-legal --lib`, native destruction PostgreSQL tests
   (`--test auxiliary_postgres destruction:: -- --test-threads=1`), scoped
   `bunx tsc --noEmit --project packages/server-legal/tsconfig.json`, and the
   burn-in TS legal suites. Do not flip catalogue certification; acceptance
   remains deferred.

## Next action

Lead: land the three prerequisites in composition (`src/composition/src/**`,
outside this claim) and report the exact native invocation plus the new source
SHA. Then re-dispatch this packet; the worker slice above is ready to execute
without further scope refinement. Workers must NOT mutate the ledger or push —
only the lead records the handoff/release and integrates.
