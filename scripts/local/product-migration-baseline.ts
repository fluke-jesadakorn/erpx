/** Development-only, read-only baseline. Detailed authority facts stay in a private ignored file. */
import { chmod, mkdir, writeFile } from "node:fs/promises";
import { resolve } from "node:path";
import { createHash } from "node:crypto";
import { activeHoldingBindings, holdingResources, runtimeEnv } from "../../src/server/modules/runtime/public/runtime";
import { isTransactionalSqlDatabase, type SqlDatabase } from "../../src/server/modules/runtime/public/runtime/types";
import { runtimeSourceFingerprint, verificationSourceFingerprint } from "./source-fingerprint";

const root = process.cwd(), directory = resolve(root, ".scratch/dev/product-migration-baseline-2026-10-14");
if (process.argv.slice(2).length) throw new Error("BASELINE_HAS_NO_APPLY_MODE");
const env = await runtimeEnv();
if (process.env.NODE_ENV === "production" || env.ERPX_ENVIRONMENT !== "local") throw new Error("DEVELOPMENT_ONLY");
await mkdir(directory, { recursive: true, mode: 0o700 });
await chmod(directory, 0o700);
const sourceFingerprint = await runtimeSourceFingerprint(root);
async function snapshot(database: SqlDatabase, holding: string | null) {
  if (!isTransactionalSqlDatabase(database)) throw new Error("BASELINE_TRANSACTION_REQUIRED");
  return database.transaction(async tx => {
    await tx.exec("SET TRANSACTION READ ONLY");
    const identity = await tx.prepare("SELECT current_database() AS database,transaction_timestamp()::text AS observed_at,version() AS postgres").first<{ database: string; observed_at: string; postgres: string }>();
    if (!identity || !/^erpx_dev_/.test(identity.database)) throw new Error("DEVELOPMENT_DATABASE_REQUIRED");
    const tables = (await tx.prepare("SELECT table_name FROM information_schema.tables WHERE table_schema='public' AND table_type='BASE TABLE' ORDER BY table_name").all<{ table_name: string }>()).results.map(row => row.table_name);
    const read = async (table: string, columns: string, order: string) => tables.includes(table)
      ? (await tx.prepare(`SELECT ${columns} FROM "${table}" ORDER BY ${order}`).all()).results : [];
    const organizations = await read("organization_nodes", "id,kind,slug,display_name,parent_id,company_id,branch_id,status", "id");
    const instant = "floor(extract(epoch FROM transaction_timestamp())*1000)::bigint";
    const assignments = await read(holding ? "assignments" : "platform_assignments", `id,user_id,role,modules_json,${holding ? "company_id,scope_kind,scope_node_id," : ""}valid_from,valid_until,revoked_at,version,
      iam_assignment_active(id,${instant}) AS assignment_active,iam_principal_active(user_id,${holding ? "company_id" : "'platform'"},${instant}) AS principal_active`, "id");
    const roles = await read("iam_roles", "id,company_id,published_revision,status", "company_id,id");
    const revisions = await read("iam_role_revisions", "company_id,role_id,revision,permissions_json,grantable_json,scopes_json", "company_id,role_id,revision");
    const rollouts = await read("company_feature_rollouts", "company_id,module_id,rollout_state,version,reason", "company_id,module_id");
    const grants = await read("company_package_grants", "company_id,package_id,package_version,state,starts_at,ends_at,version,reason,evidence", "company_id,package_id");
    const integrations = await read("iam_connections", "id,company_id,provider,status,version,tested_version,health", "company_id,id");
    // These owner tables can contain credentials, payment data, or private
    // evidence. Record only their row counts; authority-bearing values above are
    // selected explicitly so no arbitrary row serialization reads secret fields.
    const protectedTables = tables.filter(name => /^(?:billing_|beta_|company_release_|iam_connections$|iam_members$|iam_principals$|iam_revoked_|.*(?:integration|provider_connections)$)/.test(name));
    const protectedState = [];
    for (const table of protectedTables) {
      if (!/^[a-z0-9_]+$/.test(table)) throw new Error("INVALID_BASELINE_TABLE");
      const state = await tx.prepare(`SELECT count(*)::integer AS rows FROM "${table}"`).first();
      protectedState.push({ table, ...state });
    }
    return { holding, identity, organizations, assignments, roles, revisions, rollouts, grants, integrations, protectedState,
      snapshotBoundary: "one read-only transaction per database; no cross-database atomic snapshot claimed" };
  });
}
const snapshots = [await snapshot(env.CONTROL_DB, null)];
for (const binding of await activeHoldingBindings()) snapshots.push(await snapshot((await holdingResources(binding.id)).write, binding.id));
if (await runtimeSourceFingerprint(root) !== sourceFingerprint) throw new Error("BASELINE_SOURCE_CHANGED");
const evidence = { version: 1, observedAt: new Date().toISOString(), sourceFingerprint,
  verificationFingerprint: await verificationSourceFingerprint(root, sourceFingerprint),
  environment: "development", writes: 0, snapshots };
const bytes = JSON.stringify(evidence, null, 2) + "\n";
const path = resolve(directory, `authority-${Date.now()}.json`);
await writeFile(path, bytes, { mode: 0o600, flag: "wx" });
console.log(JSON.stringify({ path, sha256: createHash("sha256").update(bytes).digest("hex"), databases: snapshots.length, writes: 0, sourceFingerprint }));
process.exit(0);
