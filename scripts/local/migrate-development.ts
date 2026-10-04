/** Upgrade an already provisioned development workspace with its schema-owner identities. */
import { readFile, readdir } from "node:fs/promises";
import { dirname, join } from "node:path";
import { assertHospitalityDevelopmentTarget } from "../../src/lib/hospitality-seed-target";
import { PostgresDatabase } from "../../src/server/runtime/postgres";
import { migrateDatabase } from "../../src/server/runtime/migrations";
import type { DatabaseManifest } from "../../src/server/runtime/env";

const path = process.env.ERPX_DATABASE_MANIFEST;
if (!path) throw new Error("DEVELOPMENT_MANIFEST_REQUIRED");
const directory = dirname(path);
const [migration, app, jobs] = await Promise.all([
  join(directory,"databases-migrations.json"), path, join(directory,"databases-jobs.json"),
].map(async file => JSON.parse(await readFile(file,"utf8")) as DatabaseManifest));
for (const manifest of [migration,app,jobs]) assertHospitalityDevelopmentTarget(manifest,process.env.ERPX_ENVIRONMENT);
const reports = [];
for (const [binding, connectionString] of Object.entries(migration.databases)) {
  if (!Object.hasOwn(app.databases,binding) || !Object.hasOwn(jobs.databases,binding)) throw new Error("DEVELOPMENT_MANIFEST_BINDING_MISMATCH");
  const url = new URL(connectionString);
  if (!/^erpx_dev_[a-z0-9_]+_migration$/.test(url.username)) throw new Error("DEVELOPMENT_MIGRATION_IDENTITY_REQUIRED");
  const readOnly = binding.includes("READ"), control = binding.startsWith("CONTROL");
  const track = control ? (readOnly ? "control-read" : "control") : (readOnly ? "holding-read" : "holding-write");
  const db = new PostgresDatabase({connectionString,max:1});
  try {
    const current = await db.prepare("SELECT current_database() name,current_user identity").first<{name:string;identity:string}>();
    if (!current || !/^erpx_dev_/.test(current.name) || current.identity!==url.username) throw new Error("DEVELOPMENT_DATABASE_IDENTITY_MISMATCH");
    if (track==="holding-write") {
      const duplicates = await db.prepare(`SELECT asset_id FROM maintenance_work_orders WHERE status NOT IN ('completed','cancelled')
        GROUP BY asset_id HAVING count(*)>1`).all();
      if (duplicates.results.length) throw new Error("ACTIVE_MAINTENANCE_RECONCILIATION_REQUIRED");
    }
    const receiptNames = async () => (await db.prepare("SELECT name FROM erpx_migrations ORDER BY name").all<{name:string}>()).results.map(row=>row.name);
    const before = await receiptNames();
    await migrateDatabase(db,track);
    for (const [manifest,kind] of [[app,"app"],[jobs,"jobs"]] as const) {
      const role = new URL(manifest.databases[binding]!).username;
      if (!new RegExp(`^erpx_dev_[a-z0-9_]+_${kind}$`).test(role)) throw new Error("DEVELOPMENT_RUNTIME_IDENTITY_REQUIRED");
      const privileges = readOnly && kind==="app" ? "SELECT" : "SELECT,INSERT,UPDATE,DELETE";
      await db.exec(`ALTER DEFAULT PRIVILEGES IN SCHEMA public GRANT ${privileges} ON TABLES TO "${role}"`);
      await db.exec(`GRANT ${privileges} ON ALL TABLES IN SCHEMA public TO "${role}"`);
      await db.exec(`REVOKE ALL ON erpx_migrations FROM "${role}"`);
      if (!readOnly || kind==="jobs") await db.exec(`GRANT USAGE,SELECT ON ALL SEQUENCES IN SCHEMA public TO "${role}"`);
    }
    const after = await receiptNames(), files = (await readdir(join(process.cwd(),"migrations/postgres",track))).filter(name=>name.endsWith(".sql"));
    reports.push({binding,database:current.name,track,applied:after.filter(name=>!before.includes(name)),receiptCount:after.length,remaining:files.filter(name=>!after.includes(name))});
  } finally { await db.close(); }
}
console.log(JSON.stringify({environment:"development",reports},null,2));
