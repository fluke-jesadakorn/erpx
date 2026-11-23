# `retire.agent.residual-writers` — precise spec (all 22 residual writes already native-backed; KPI 22→0; structural residue lead-owned; acceptance deferred)

Status: **verified; every residual Agent SQL writer is retired in source; only structural transport/barrel cleanup and environment provisioning remain; acceptance deferred.**
This verification changed no owner source, did not mutate the ledger, and did not push. Its only artifact is this spec inside the claimed path `tests/integration/agent/**`.

## Identity and source

- Task `retire.agent.residual-writers`, goal `G3`, owner `agent`, workstream `S05`, plan `docs/production-plan.md#rust-migration-current`.
  Bounded workflow IDs: `agent/agent-catalog`, `agent/versions`, `agent/assignments`, `agent/feature-pools-routing`, `agent/feature-private-sessions`, `agent/feature-invocation-evidence`, `agent/feature-field-suggestions`.
- Claim held for this verification: `runId=run-20261123-f`, `agentId=wC03`, generation 158 (claimed `2026-11-23T17:08:19.842Z`). No claim, heartbeat, ledger field or run state was modified.
- Source identity at the moment the checks below ran: HEAD `1dea2dc464236e3c34ede85e22a6e547c8a79cd8` on `codex/rust-backend-migration`. This is a shared, concurrently committing checkout, so HEAD advanced (`1ef22319efa726c53ed26606a921fa03124f0d11`) while this spec was written; the owner source named here is unchanged by those intervening commits.
  - Step-0 audit baseline: `5f73271e62037a8b15a1f5baf9125cb8885fdaaf` (before 22 / after 22).
  - AI-gateway command packet: `8962a90b6aa67a0954480beb53f1ec864c605f9f` (before 22 / after 16).
  - 15-owner-wave native cutover: `43ffc6a62c43bf89428606c6cb880370b17e0646` (16 → 0; the implementation commit for the residual slice).
- `bun scripts/testing/rust-migration-work.ts validate` → `{"valid": true}`; `status --json` shows coordinator `run-20261123-f` and my held claim on this task.
- Claimed paths were clean: `git status --short -- packages/server-agent packages/feature-agent 'packages/feature-admin/src/routes/api/v1/holdings/[holdingSlug]/agents' tests/integration/agent tests/unit/agent fixtures tests/integration/shared.requests` was empty. Unrelated dirty WIP elsewhere (`Cargo.lock`, `apps/*/globals.css`, `docs/status/rust-delivery-bundles.json`, `packages/feature-brand/**`, `packages/feature-pms/**`, the untracked `tests/integration/work_projection/*.spec.md`) belongs to other writers and was not touched.

## Audit verdict — what the "22 remaining writes" are now

The 2026-11-13 decision (`fc2cd18ae`) recorded the blocker "no complete native counterpart for any of the 22 writes" at `5f73271e6`. That state no longer exists. The gateway native-port decision has since been implemented as a single allow-listed, fail-closed native owner-write port (`ERPX_NATIVE_AGENT_OWNER_WRITE_CONFIG` → private `/agent/owner-write`, with a Control-side `/agent/owner-write-control` variant), and every residual write was converted onto it.

Owner write KPI (`git grep -I -o -E "INSERT INTO|DELETE FROM|UPDATE [a-z_]+ SET" <sha> -- ':(glob)packages/server-agent/src/**/*.ts' | wc -l`), re-measured for this spec:

| SHA | Count | Cause |
| --- | --- | --- |
| `5f73271e6` | 22 | step-0 audit baseline |
| `8962a90b6` | 16 | 6 AI-gateway policy/statement/cost writers retired (`migrate.agent.ai-gateway-commands`) |
| `43ffc6a62` | 0 | final 16 retired by the 15-owner-wave axum cutover |
| checked HEAD | 0 | current committed and working-tree source |

The 16 residual writes recorded in `8962a90b6`'s `residual[]` and their current dispatch (all are native clients now; none is a TS SQL writer):

| File (original residual) | Original writes | Current dispatch |
| --- | --- | --- |
| `adapters/ai-gateway.ts` | 2 | `nativeReserveQuota` / `nativeFinishQuota` → native ai-quota commands (`native-quota-command.ts`) |
| `adapters/workspace-assistant.ts` | 7 | `nativeOwnerWrite('ag_workspace_turn_*')` × 7 kinds |
| `adapters/operations-guided-intake-facts.ts` | 3 | `nativeOwnerWrite('ag_workspace_turn_insert/ready/failed')` |
| `adapters/agent-governance.ts` | 3 | `nativeOwnerWrite('ag_private_session_insert', 'ag_session_controls_insert', 'ag_invocation_evidence_insert')` |
| `adapters/ai-runtime.ts` | 1 | `nativeOwnerWrite('ag_provider_usage_insert')` |

All ten distinct `nativeOwnerWrite` kinds have a complete three-layer native counterpart:

| Kind (`domain/owner_write_registry.rs`) | SQL in `adapters/owner_write_postgres.rs` | HTTP authority |
| --- | --- | --- |
| `ag_private_session_insert` | `INSERT INTO ai_private_sessions …` | `src/composition/src/agent_wave2.rs::agent_owner_write_router`, mounted in `private_development_api.rs` |
| `ag_session_controls_insert` | `INSERT INTO ai_private_session_controls …` | same |
| `ag_invocation_evidence_insert` | `INSERT INTO ai_invocation_evidence …` | same |
| `ag_provider_usage_insert` | `INSERT INTO ai_provider_usage …` | same |
| `ag_workspace_turn_insert` | `INSERT INTO ai_workspace_turns …` | same |
| `ag_workspace_turn_ready` | `UPDATE ai_workspace_turns SET state='ready' …` | same |
| `ag_workspace_turn_failed` | `UPDATE ai_workspace_turns SET state='failed' …` | same |
| `ag_workspace_turn_english` | `UPDATE ai_workspace_turns SET english_answer_json …` | same |
| `ag_workspace_turn_translation_ready` | `UPDATE ai_workspace_turns SET state='ready' …` | same |
| `ag_workspace_turn_translation_failed` | `UPDATE ai_workspace_turns SET state='failed' …` | same |

`PgOwnerWrite::execute` also inserts the `command_receipts`, `audit_events` and `outbox_events` ledger rows in the same transaction and requires an open transaction (`AGENT_OWNER_WRITE_TRANSACTION_REQUIRED`). The registry is an exact, arity-fenced allow list (`validate(kind, value_count)`), and `native-owner-write.ts` fails closed on missing/invalid config, bad port ranges (reserved ports and 3200–3299 rejected), malformed kind/key and any non-200/non-`{data:{ok:true,kind}}` reply. `noop = () => db.prepare("SELECT 1")` is an inert statement that only preserves caller batch shape; it carries no SQL write.

The governance route is already native-authoritative for writes: `packages/feature-admin/src/routes/api/v1/holdings/[holdingSlug]/agents/route.ts` POST fails closed (`503 NATIVE_AGENT_UNAVAILABLE`) before calling `nativeAgentCommand`; its remaining `agentWorkspace` GET branch is a read-tail item for G4, not a writer.

**Verdict: there is no unmigrated residual Agent writer and no dual-authority TS fallback. No new bounded native slice is required for this task.** The retirement KPI can be closed now at `before=22, after=0`.

## Remaining structural residue — exact next actions (lead executes or assigns)

These are cleanup with **no KPI delta** (owner writes are already `0`) and cross the task's `pathPolicy`, so they were deliberately not performed in this worker slice:

1. **Close the KPI gate.** Record `retirement:ts-writes -- ':(glob)packages/server-agent/src/**/*.ts'` with `before=22, after=0`, implementation SHA `43ffc6a62c43bf89428606c6cb880370b17e0646`; the intermediate `22→16` at `8962a90b6` stays in history.
2. **Decide on shim deletion (optional structural cleanup).** The four adapter files above are large read/application hosts, not standalone shims; deleting them wholesale is not possible. If the lead wants the `nativeOwnerWrite` dispatch extracted, add an owner-local bridge under `packages/server-agent/src/adapters/` and rewire the callers, then remove any now-dead exports from `packages/server-agent/package.json`. This does not change the KPI and is not required to close retirement.
3. **Regenerate derived config (lead-owned).** Any removed `@source`/Turbo inputs in `apps/*/src/app/globals.css` and `apps/*/turbo.json` must be regenerated by the lead, not hand-edited in this slice (those files are dirty with other writers' WIP).
4. **Provision the native port in authorized dev (environment, not source).** `ERPX_NATIVE_AGENT_OWNER_WRITE_CONFIG` is absent from the checked `.env`, `.env.local` and `.scratch/dev/v5.json`, so a configured-but-missing native owner write correctly fails closed. Browser/gateway smoke of assistant turns, governance pools/sessions, guided intake and provider usage requires the lead to provision a loopback Holding config on the shared native listener; no staging/production access is authorized.
5. **Re-verify and record.** Re-run the checks below, confirm owner KPI stays `0` and no retired export remains, then hand off for lead integration. Keep `acceptance:false`; acceptance stays deferred under G5/A1.

## Checks performed at the checked HEAD

- `bun scripts/testing/rust-migration-work.ts validate` → `{"valid": true}`.
- `bun scripts/testing/rust-migration-work.ts status --task=retire.agent.residual-writers --json` → task in `in_progress`, claim `run-20261123-f / wC03`, `retirement` block `targetValue=0`.
- Owner write KPI on committed source and working tree → **0** (history `22→16→0` verified at the SHAs above).
- `cargo check -p erpx-server-agent --quiet --offline` → **passed** (`cargo_exit=0`; only pre-existing dead-code warnings in `packages/server-workflow/src/domain/flow_advance.rs`).
- `bunx tsc --noEmit -p packages/server-agent/tsconfig.json` → **passed** (`tsc_exit=0`, no diagnostics).
- `git diff --check` on the claimed paths → clean (this spec is the only new file).

No database, gateway, staging or private production service was contacted; no ledger or remote state was mutated; nothing was pushed. Acceptance remains deferred.
