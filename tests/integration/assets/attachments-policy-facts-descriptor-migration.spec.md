# `migrate.assets` — `attachments-policy-facts` descriptor SELECT → native `observe_asset_attachment_descriptor`

Status: **owner side already native and verified; Files-side cutover BLOCKED on lead-owned composition; legacy TS descriptor NOT deleted; acceptance deferred.**
This verification changed no owner source and did not mutate the ledger or push. The only new artifact is this spec.

## Identity and source

- Task `migrate.assets`, goal `G3`, owner `assets`, workstream `S06`, plan `docs/production-plan.md#rust-migration-current`.
- Claim held, not modified: `runId=run-20261123-f`, `agentId=wA19`, `generation=158`, lease expires `2026-11-23T17:30:08.102Z`.
- HEAD used for every check: `9e64cfec077852474e7730abd654eca4509efa1b` on `codex/rust-backend-migration`; `executionPlan.revision=6259`; `rust-migration-work.ts validate` → `{"valid":true}` and `status --json` confirmed the claim above.
- Task `allowedPaths` (ledger): `packages/server-assets/src/**`, `packages/server-assets/Cargo.toml`, `packages/feature-assets/src/**`, `tests/unit/assets/**`, `tests/integration/assets/**`, `tests/e2e/assets/**`. `git status --short -- packages/server-assets tests/integration/assets tests/unit/assets` is empty — no concurrent WIP in the claimed paths. Other owners' dirty WIP (globals.css, Cargo.lock, ledger, work_projection, suite manifest) was left untouched.
- Relevant blob SHAs at HEAD:
  - `dd5c758f4039c24e0db302b5d51ef1e6601e4a50` `packages/server-assets/src/adapters/asset_purchase_source.rs`
  - `a062baa66aa95d6cbd70a1e9fffb7c870bf28c1c` `packages/server-assets/src/public/asset_purchase_source.rs`
  - `71e51800d1e147c3bbccbe6b8298e115bdefb21a` `packages/server-assets/src/adapters/attachments-policy-facts.ts`
  - `4c71ca62cf2d5a612679f625c84f177ac222cea9` `packages/server-assets/src/public/attachments-policy-facts.ts`
  - `d71d4de53d6ba26755cc9d9d383c3620e326285f` `tests/integration/assets/rust-asset-register-postgres.test.rs`

## The slice

The legacy owner descriptor `packages/server-assets/src/adapters/attachments-policy-facts.ts:4` exposes exactly one SELECT via `policyAssetsFacts(db).readDescriptor(facts)`:

```sql
SELECT company_id,branch_id AS branch_id,department_id AS department_id,version AS version
FROM operational_assets WHERE id=? AND company_id=?
```

Files consumes it at `packages/server-files/src/adapters/attachments/policy.ts:74`: `case "asset": return policyAssetsFacts(db).readDescriptor(facts);` inside `unit(db)` → `AttachmentPolicyUnit.readDescriptor`.

## Owner side — already native, no new code required

- `observe_asset_attachment_descriptor` in `packages/server-assets/src/adapters/asset_purchase_source.rs:260-285` performs the same SELECT (schema-qualified `public.operational_assets`, `$1/$2` binds) inside the Asset purchase serializable/lock unit, keeping absence and raw `i64` version; `CurrentAssetAttachmentDescriptor::{data,require_current}` (`:286-307`) provide the revalidation witness.
- It is publicly exported at `packages/server-assets/src/public/asset_purchase_source.rs:3`, with the ownership note at `asset_purchase_source.rs:226`: "Raw Assets-owned descriptor for Files `policyAssetsFacts.readDescriptor`. This is not the Files policy, current IAM, purchase, or a target grant."
- Owner-side executable oracle proof already exists at `tests/integration/assets/rust-asset-register-postgres.test.rs:1801-1824` (`asset_files_descriptor_matches_actual_typescript_owner_sql`): it calls the **actual** Bun-imported TS owner adapter with a stub `SqlDatabase`, captures its fixed SQL, and asserts byte-level result parity with the native observation across long/foreign/null/missing ids plus a persisted-version drift savepoint.

### Live parity re-verification of the *policy* oracle (this slice)

Existing test uses the structurally identical `attachments-operational-facts.ts` oracle. This verification invoked the actual **policy** oracle with pinned Bun 1.4.3 and `--conditions=react-server`:

```
{"sql":"SELECT company_id,branch_id AS branch_id,department_id AS department_id,version AS version FROM operational_assets WHERE id=$1 AND company_id=$2","args":["sample","company"]}
```

This is byte-identical to the native SQL at `asset_purchase_source.rs:247` except for the `public.` schema qualification. The native `AssetAttachmentDescriptor {company_id, branch_id, department_id, version}` (derives `serde::Serialize`) is therefore a drop-in projection of the policy descriptor row. No owner-side gap remains; the native port is ready for composition binding.

## Blocker — lead-owned, not editable under this claim

| Required for cutover | Observed | Verdict |
| --- | --- | --- |
| Native observation bound in Files policy unit instead of TS `policyAssetsFacts` | `packages/server-files/src/adapters/attachments/policy.ts:48` still imports `@erpx/server-assets/public/attachments-policy-facts`; `:74` still delegates `case "asset"` to the TS descriptor. The Files service composition (`packages/server-files/src/adapters/attachments/service.ts:20` `createAttachmentPolicy(ports)` / `unit(db)`) and the production mount under `src/composition/src/**` are lead-owned and outside `migrate.assets` `allowedPaths`. | **BLOCKED — lead action required** |
| Delete replaced TS writer/branch and move test oracle (G3 cutover layers 3–4) | `attachments-policy-facts.ts` and its `public/` re-export are still the live authority for `case "asset"`. Deleting them now breaks the Files import and typecheck; no replacement is bound. | **NOT MET** |
| `retirement:ts-statements` delta for the removed SELECT | Legacy TS adapter retained. Assets TS writes at HEAD = `0`; statements = `59` (the descriptor SELECT is among them). No delta can be claimed until the Files consumer is switched and the adapter deleted. | **DEFERRED** |

## Exact next actions (lead executes or assigns)

1. Bind `erpx_server_assets::public::asset_purchase_source::observe_asset_attachment_descriptor` into the Files attachment policy unit in the lead-owned Files composition/mount, replacing the `case "asset"` TS delegation at `packages/server-files/src/adapters/attachments/policy.ts:74` and its import at `:48`. Keep the same serializable unit/lock ordering already used by the Assets purchase sources; do not call it outside the owner transaction.
2. Prove full Next transport parity for the returned descriptor (`company_id`, `branch_id`, `department_id`, `version` as an integer) and make native the primary payload, not an overlay.
3. Add a policy-oracle parity assertion beside `asset_files_descriptor_matches_actual_typescript_owner_sql` using `policyAssetsFacts(db).readDescriptor(...)` on `src/adapters/attachments-policy-facts.ts` before deleting the oracle (same fixed-SQL capture pattern; SQL was verified identical here).
4. Delete `packages/server-assets/src/adapters/attachments-policy-facts.ts`, its `public/attachments-policy-facts.ts` re-export, and the two `package.json` subpath exports; prove no importer remains (`git grep -n "policyAssetsFacts" -- '*.ts' '*.tsx'`).
5. Re-run `cargo check -p erpx-server-assets --offline`, scoped `tsc`, the assets native integration suite, and a `retirement:ts-statements` check for assets (before `59` → after `58` or lower). Record the push and keep `acceptance:false`.

## Checks performed at `9e64cfec077852474e7730abd654eca4509efa1b`

- `bun scripts/testing/rust-migration-work.ts validate` → `{"valid":true}`; `status --json` → claim `wA19` held, no mutate.
- `cargo check -p erpx-server-assets --offline` → **passed** (`Finished dev profile`).
- `bunx tsc --noEmit -p packages/server-assets/tsconfig.json` → **passed** (exit 0).
- `bunx tsc --noEmit -p packages/feature-assets/tsconfig.json` → **passed** (exit 0).
- Pinned-Bun live invocation of the actual policy TS oracle → SQL identical to native observation (above).
- Owner KPI at HEAD: assets TS writes `0`, statements `59`; no deletion performed in this blocked slice.
