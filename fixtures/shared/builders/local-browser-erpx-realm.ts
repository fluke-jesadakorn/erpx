import { readFile } from "node:fs/promises";
import { join, resolve } from "node:path";
import type { RuntimeEnv } from "../../../src/server/modules/runtime/adapters/runtime/env";
import { PostgresDatabase } from "../../../src/server/modules/runtime/adapters/runtime/postgres";
import type { SqlDatabase } from "../../../src/server/modules/runtime/adapters/runtime/types";
import { loadIamAuthority } from "../../../src/server/modules/identity_access/public/iam-store";
import { assertAcmeBrowserSeedTarget } from "./acme-browser-seed-target";
import registry from "../../../src/data/module-registry.json";

type Manifest = { databases: Record<string, string> };
type State = Parameters<typeof assertAcmeBrowserSeedTarget>[1];
export function assertLocalBrowserRealmTarget(manifest: Manifest, state: State, run: string | undefined, targets: Record<string, { name: string; owner: string } | null>) {
  assertAcmeBrowserSeedTarget(manifest, state, run);
  if (state.profile !== "local-seed" || JSON.stringify(Object.keys(targets).sort()) !== JSON.stringify(Object.keys(manifest.databases).sort()) || Object.entries(targets).some(([binding, target]) => target?.name !== `erpx_browser_${run}_${binding.toLowerCase()}` || target.owner !== "erpx_test_admin")) throw new Error("EXACT_LOCAL_BROWSER_REALM_DATABASE_REQUIRED");
}

/** Only prerequisites: reuse the existing fictional account and its Platform identity. */
export async function prepareLocalBrowserRealm(control: SqlDatabase, holding: SqlDatabase, now = Date.now()) {
  const userId = "usr_jesadakorn", companyId = "browser-erpx-realm", assignmentId = "browser_usr_jesadakorn_erpx_holding_owner";
  const identity = await control.prepare(`SELECT u.id FROM "user" u JOIN platform_assignments a ON a.user_id=u.id WHERE u.id=? AND a.role='platform_admin' AND a.revoked_at IS NULL AND a.valid_from<=? AND a.valid_until>? AND iam_assignment_active(a.id,?) AND iam_principal_active(u.id,'platform',?)`).bind(userId, now, now, now, now).first();
  if (!identity) throw new Error("EXISTING_LOCAL_BROWSER_PLATFORM_IDENTITY_REQUIRED");
  const binding = await control.prepare("SELECT slug,write_binding,read_binding,files_binding FROM holding_bindings WHERE holding_id='h003'").first<{ slug: string; write_binding: string; read_binding: string; files_binding: string }>();
  if (binding && (binding.slug !== "erpx" || binding.write_binding !== "H003_DB" || binding.read_binding !== "H003_READ_DB" || binding.files_binding !== "H003_FILES")) throw new Error("LOCAL_BROWSER_REALM_ALREADY_OWNED");
  await holding.batch([
    holding.prepare("INSERT INTO organization_nodes(id,kind,slug,display_name,company_id) VALUES(?,'company','erpx','ERPX browser realm',?) ON CONFLICT(id) DO NOTHING").bind(companyId, companyId),
    holding.prepare("INSERT INTO beta_bootstrap(singleton,holding_id,owner_user_id,created_at) VALUES(1,'h003',?,?) ON CONFLICT(singleton) DO NOTHING").bind(userId, now),
    holding.prepare("INSERT INTO assignments(id,user_id,role,modules_json,company_id,branch_id,valid_from,valid_until,version,scope_kind,scope_node_id) VALUES(?,?,'holding_owner',?,?,NULL,?,?,1,'holding',NULL) ON CONFLICT(id) DO NOTHING").bind(assignmentId, userId, JSON.stringify(registry.map(module => module.id)), companyId, now - 1_000, now + 365 * 86_400_000),
  ]);
  await control.prepare("INSERT INTO holding_bindings(holding_id,slug,display_name,write_binding,read_binding,files_binding,status) VALUES('h003','erpx','ERPX browser realm','H003_DB','H003_READ_DB','H003_FILES','active') ON CONFLICT(holding_id) DO NOTHING").run();
  const assignment = await holding.prepare("SELECT company_id,role,scope_kind,iam_assignment_active(id,?) active FROM assignments WHERE id=? AND user_id=?").bind(now, assignmentId, userId).first<{ company_id: string; role: string; scope_kind: string; active: boolean }>();
  const authority = await loadIamAuthority(holding, companyId, userId, "holding_owner");
  if (!assignment || assignment.company_id !== companyId || assignment.role !== "holding_owner" || assignment.scope_kind !== "holding" || !assignment.active || !authority.permissions.includes("manage_erpx.access")) throw new Error("NORMAL_LOCAL_BROWSER_REALM_AUTHORITY_REQUIRED");
  return { userId, companyId, assignmentId };
}

export async function seedLocalBrowserRealm(env: RuntimeEnv, directory: string) {
  const run = process.env.ERPX_BROWSER_SIMULATION_RUN_ID, manifestPath = process.env.ERPX_DATABASE_MANIFEST;
  if (!run || !manifestPath || resolve(directory) !== resolve(".runtime/browser", run) || env.ERPX_ENVIRONMENT !== "local") throw new Error("EXACT_ACME_BROWSER_SEED_COHORT_REQUIRED");
  const manifest: Manifest = JSON.parse(await readFile(manifestPath, "utf8")), state: State = JSON.parse(await readFile(join(directory, "state.json"), "utf8"));
  assertAcmeBrowserSeedTarget(manifest, state, run);
  const targets: Record<string, { name: string; owner: string } | null> = {};
  for (const binding of Object.keys(manifest.databases)) {
    const database = env[binding];
    if (!(database instanceof PostgresDatabase)) throw new Error("EXACT_ACME_BROWSER_NATIVE_DATABASE_REQUIRED");
    targets[binding] = await database.prepare("SELECT current_database() name,current_user owner").first<{ name: string; owner: string }>();
  }
  assertLocalBrowserRealmTarget(manifest, state, run, targets);
  return prepareLocalBrowserRealm(env.CONTROL_DB, env.H003_DB);
}
