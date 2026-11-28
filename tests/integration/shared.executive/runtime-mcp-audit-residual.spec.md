# G4 / retire.runtime.residual-writers — executive MCP + outbox residual (precise spec)

Claim: `run-20261123-f` / `wA13`. Read-only worker review; no ledger mutation, no push.
Plan: `docs/production-plan.md#rust-migration-current`. Goal G4, task `retire.runtime.residual-writers`.

## Source binding

- Task baseSha `040a20744dabccf124a8ee37283d84417e8d93d3`; last recorded check SHA `43c8fd81b92b82678f376b5d7d156d0fd42b5c48`; prior review `be30e2e27`.
- Working HEAD during review was moving under concurrent workers (observed `2068f46ca`, `9e64cfec0`, `44c98707`). Use blob SHAs below as the stable source identity.
- `platform/runtime/server/src/adapters/executive/mcp.ts` = `509ebedff7204518c32ad80ef1d00dad23257c4e`
- `platform/runtime/server/src/adapters/native_command_sql.rs` = `54a45b017c643429a72996738348bc82cb1da377`
- `platform/runtime/ui/src/adapters/server/native-runtime-command.ts` = `5a16ab8d683217a44c4cb0a1cc0e030731466346`
- `tests/integration/shared.executive/executive-mcp.test.ts` = `042eac182ade82cabc272f227ff7641c8b183908`

## Trace (complete)

`runtime_stmt_16` (`UPDATE executive_external_tokens SET last_used_at=$1 WHERE id=$2 AND revoked_at IS NULL`, scope `control`) has exactly one producer: `mcp.ts:145`, after a successful MCP response inside `handleExecutiveMcp`.

`runtime_stmt_17` (`INSERT INTO executive_mcp_audit(...)`, scope `control`) has exactly one producer: `mcp.ts:160`, in the terminal `try` of `handleExecutiveMcp`; on write failure the handler returns 503 (`EXECUTIVE_UNAVAILABLE`).

`handleExecutiveMcp` callers:
- production `products/apps/ui/src/routes/api/mcp/executive/route.ts` (POST/GET/DELETE) — outside this task; lead integration;
- barrel `platform/runtime/server/src/public/executive/mcp.ts`;
- `tests/integration/shared.executive/executive-mcp.test.ts` (direct `rpc()` and Streamable-HTTP client via overridden `fetch`);
- e2e `tests/e2e/shared.executive/executive-work-mcp.spec.ts` over HTTP `POST /api/mcp/executive`.

No other TS call site for 16/17. No TypeScript `outbox_events` writer remains in the runtime owner: the receipt/audit/outbox ledger is written by the native command path (`native_command_postgres.rs`, composition `runtime_wave2_native.rs`), and the executive evidence outbox insert is native (`executive_evidence_postgres.rs:102`). The remaining TS outbox SQL in the repo is fixture/seed/test oracle only, out of scope.

Owner KPI (`INSERT INTO|DELETE FROM|UPDATE <table> SET` over `platform/runtime/server/src/**/*.ts`) is **0** at current HEAD; the 36 → 0 check at `43c8fd81` is already satisfied. The residual is not literal SQL but 21 TS `nativeRuntimeStatement` shim call sites that still own write *orchestration*; SQL text already lives in `native_command_sql.rs`. Under G4 these are retained transport code that must be explicitly classified with an expiry task or moved behind a typed native command.

## Baseline checks (passed at review HEAD)

- `cargo check -p erpx-platform-runtime-server --locked` — passed.
- `tsc --noEmit --incremental false -p platform/runtime/server/tsconfig.json` — passed (0 diagnostics).
- `tsc --noEmit --incremental false -p platform/runtime/ui/tsconfig.json` — passed (0 diagnostics).

## Blocker found (must be fixed in the same packet)

`executive-mcp.test.ts` sets `ERPX_NATIVE_EXECUTIVE_CONNECTIONS_CONFIG` (rate-limit oracle) but never sets `ERPX_NATIVE_RUNTIME_CONFIG`. With the current fail-closed shim, `nativeRuntimeStatement('runtime_stmt_16'/'17')` throws `RUNTIME_NATIVE_COMMAND_UNAVAILABLE` (503), so every `handleExecutiveMcp` call in that suite returns 503 and every MCP assertion (including the `executive_mcp_audit` denied-attribution check at line 137) fails. The same gap affects the browser cohort: without native runtime config, all MCP calls are 503. A literal-SQL KPI of 0 therefore does not prove a working MCP path.

## Bounded native slice (packet for lead integration)

Goal: move the MCP audit writer group (16/17) behind one typed, Rust-owned native command; delete the two TS shim calls; restore a self-contained test oracle. No composition change is required — the generic native command router already admits any registered statement kind and selects the control pool from `NativeStatement.scope`.

1. `platform/runtime/server/src/adapters/native_command_sql.rs`
   - Replace kinds `runtime_stmt_16` and `runtime_stmt_17` with one control-scoped compound kind `runtime_mcp_request_audit` implemented as a single data-modifying CTE over `executive_external_tokens` and `executive_mcp_audit`, or keep both kinds but add a Rust application function `record_mcp_request` that composes them on the caller connection. The compound statement must retain current semantics: audit is written for success/denied/failed, including denied attempts whose token is expired/revoked (do not gate the audit CTE on a successful token touch).
   - Extend `every_statement_is_well_formed` to accept a leading `WITH` for the compound kind; keep `bind_count` exact.
   - Add a unit test for the new kind's arity and scope.
2. `platform/runtime/server/src/adapters/executive/mcp.ts` (within allowed path)
   - Replace lines 145 and 160 with one typed client call carrying `(token_id, user_id, company_id, assignment_id, branch_id, department_id, action, outcome, code, started_at, duration_ms, now)`.
   - Keep fail-closed behavior: native command failure still produces 503 `EXECUTIVE_UNAVAILABLE` at the terminal catch; do not reintroduce any SQL literal or TS fallback.
   - Drop the `randomUUID` import only if the native command owns audit id generation (prefer `gen_random_uuid()` in Rust SQL so TS stops minting the audit row identity).
3. `platform/runtime/ui/src/adapters/server/native-executive-mcp-audit.ts` (new, within allowed path)
   - Typed wrapper over `RUNTIME_NATIVECommandSystem(companyId, 'runtime_mcp_request_audit', values, key, now)`. Validate token/company identifier shapes, bound all strings, derive the idempotency key from `(token_id, started_at, action)` so a retried request cannot duplicate the audit row. Fail closed when config is absent; no TS fallback.
4. `tests/integration/shared.executive/executive-mcp.test.ts` (within allowed path)
   - Extend the existing loopback oracle to also serve `POST /api/v1/companies/{company}/runtime/native-command`, mapping `runtime_stmt_1..15,18..22` and the new MCP audit kind to the same SQL registry and executing on `env.CONTROL_DB` for `scope == "control"` and `env.H001_DB` otherwise. Set `ERPX_NATIVE_RUNTIME_CONFIG` to that oracle (same private base URL, one holding with `companyId`, no reserved port) and tear it down in `afterAll`.
   - Keep the existing denied-audit assertion (line 137) and add an assertion that the new command writes exactly one `executive_mcp_audit` row and touches `last_used_at` for a successful call.
5. Residual classification (required to close a `retire.*` task at `after: 0`): the other retained `nativeRuntimeStatement` call sites (`reports.ts` 1–11, `generation.ts` 12–15/20/21, `jobs.ts` 19, `beta-onboarding-effects.ts` 18) are not in this packet. Record them in the retirement check `residual` array with `removalTaskId` and `removalCondition` pointing at per-capability native command packets (report/generation/job/lease), each summing to the recorded `after`. Do not relabel them acceptance-only.
6. Verification: `cargo check -p erpx-platform-runtime-server --locked`; scoped `tsc` for `server-runtime` and `feature-runtime`; `migration_receipt_contract 1/1 + outbox 5/5` unit set; `executive-mcp.integration`; owner KPI pair (still 0 → 0 for literal SQL; record the shim call-site delta 21 → 19 as `retirement:ts-statements` evidence). Acceptance stays `false`.

## Next action

Lead: refine this into a bounded packet limited to stmt_16/17 (MCP audit) as above, grant the exact allowed paths already on the task, and coordinate the test-oracle/config change with the executive-work browser cohort. The production `feature-apps` MCP route and composition stay lead-owned; no ledger write or push was performed by this worker.
