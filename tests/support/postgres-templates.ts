import { createHash, randomUUID } from "node:crypto";
import { mkdir, readFile, readdir, writeFile, rename, link, rm } from "node:fs/promises";
import { resolve, join } from "node:path";
import { Pool, type PoolClient } from "pg";
import { PostgresDatabase } from "@erpx/server-runtime/adapters/runtime/postgres";
import { migrateDatabase } from "@erpx/server-runtime/public/runtime/migrations";

export const databaseTracks = {
  CONTROL_DB: "control", CONTROL_READ_DB: "control-read",
  H001_DB: "holding-write", H001_READ_DB: "holding-read",
  H002_DB: "holding-write", H002_READ_DB: "holding-read",
  H003_DB: "holding-write", H003_READ_DB: "holding-read",
} as const;
export type DatabaseSlot = keyof typeof databaseTracks;
export type Template = { database: string; track: string; fingerprint: string };
export type NativeFixturePlan = { version: 1; root: string; pid: number; benchmark?: { mode: "legacy-raw" | "template"; directory: string }; templates: Record<string, Template>; files: Record<string, { mode: "template" | "raw"; slots: DatabaseSlot[]; migratedSlots: DatabaseSlot[] }> };
const directory = (root: string) => resolve(root, ".scratch/dev/postgres-templates");
const json = (value: unknown) => JSON.stringify(value, null, 2) + "\n";
/** Schema templates contain no scenario data. Native and development browser
 * setup share the same fixture/migration-engine identity and PostgreSQL checks. */
export async function postgresFixtureFingerprint(root:string){
 const hash=createHash('sha256').update('schema-only-fixtures/v3');
 for(const directory of ['fixtures','tests/support','scripts/seed']){
  const files=(await readdir(join(root,directory),{recursive:true,withFileTypes:true})).filter(entry=>entry.isFile()).map(entry=>join(entry.parentPath,entry.name)).filter(file=>/\.[cm]?[jt]sx?$/.test(file)).sort();
  for(const file of files)hash.update(file.slice(root.length)).update('\0').update(await readFile(file)).update('\0');
 }
 for(const file of ['packages/server-runtime/src/adapters/runtime/migrations.ts','packages/server-runtime/src/adapters/runtime/postgres.ts','packages/server-runtime/src/adapters/runtime/resources.ts','packages/server-runtime/src/adapters/runtime/read-metrics.ts','bun.lock'])hash.update(file).update('\0').update(await readFile(join(root,file))).update('\0');
 return hash.digest('hex');
}
async function atomic(path: string, value: unknown, exclusive=false) {
  const temporary = `${path}.${process.pid}.${randomUUID()}.tmp`;
  await writeFile(temporary, json(value), { mode: 0o600, flag:'wx' });
  try{if(exclusive)await link(temporary,path);else await rename(temporary,path);}
  finally{await rm(temporary,{force:true});}
}
export function testAdministratorUrl(value = process.env.ERPX_TEST_ADMIN_URL) {
  if (!value) throw new Error("TEST_DATABASE_ADMINISTRATOR_REQUIRED");
  const url = new URL(value);
  if (url.hostname !== "127.0.0.1" || url.port !== "5432" || url.username !== "erpx_test_admin" || url.pathname !== "/postgres") throw new Error("LOCAL_TEST_DATABASE_REQUIRED");
  return value;
}
export async function assertTestAdministrator(admin: Pool) {
  const role = (await admin.query("SELECT current_user name,rolsuper,rolcreatedb,rolcreaterole FROM pg_roles WHERE rolname=current_user")).rows[0];
  if (role?.name !== "erpx_test_admin" || role.rolsuper || !role.rolcreatedb || role.rolcreaterole) throw new Error("UNSAFE_TEST_DATABASE_ROLE");
  const forbidden = await admin.query("SELECT datname FROM pg_database WHERE (datname LIKE 'epsx\\_%' ESCAPE '\\' OR datname IN ('n8n','plf_webapp_dev')) AND has_database_privilege(current_user,datname,'CONNECT')");
  if (forbidden.rowCount) throw new Error("ERPX_TEST_ROLE_CAN_CONNECT_TO_PROJECT_DATABASE");
}
export async function schemaFingerprint(root: string, track: string, version: string, fixtureFingerprint: string) {
  if (!["control", "control-read", "holding-write", "holding-read"].includes(track)) throw new Error("UNKNOWN_TEMPLATE_TRACK");
  const hash = createHash("sha256").update(`erpx-schema-template/v1:${track}:${version}:${fixtureFingerprint}`);
  for (const name of (await readdir(join(root, "migrations/postgres", track))).filter(name => name.endsWith(".sql")).sort())
    hash.update(name).update("\0").update(await readFile(join(root, "migrations/postgres", track, name))).update("\0");
  return hash.digest("hex");
}
export async function ensureSchemaTemplate(root: string, track: string, fixtureFingerprint: string): Promise<Template> {
  const connectionString = testAdministratorUrl(), admin = new Pool({ connectionString, max: 1 });
  let lock: number | undefined;
  try {
    await assertTestAdministrator(admin);
    const version = (await admin.query("SELECT current_setting('server_version') version,current_setting('TimeZone') timezone,current_setting('server_encoding') encoding,current_setting('DateStyle') datestyle,current_setting('standard_conforming_strings') conforming,datcollate,datctype,datcollversion FROM pg_database WHERE datname='template0'")).rows[0];
    if (!String(version.version).startsWith("18.6")) throw new Error("POSTGRES_18_6_REQUIRED");
    const fingerprint = await schemaFingerprint(root, track, JSON.stringify(version), fixtureFingerprint);
    const database = `erpx_test_tpl_${fingerprint.slice(0, 24)}`, descriptor = { database, track, fingerprint };
    lock = Buffer.from(fingerprint.slice(0, 8), "hex").readInt32BE();
    await admin.query("SELECT pg_advisory_lock(714928002,$1)", [lock]);
    const previous = (await admin.query("SELECT pg_get_userbyid(datdba) owner,datallowconn,shobj_description(oid,'pg_database') description FROM pg_database WHERE datname=$1", [database])).rows[0];
    if (previous && previous.owner !== "erpx_test_admin") throw new Error("UNSAFE_TEMPLATE_OWNER");
    if (previous?.description === json(descriptor).trim() && previous.datallowconn === false) return descriptor;
    if (previous) {
      if ((await admin.query("SELECT 1 FROM pg_stat_activity WHERE datname=$1", [database])).rowCount) throw new Error("INCOMPLETE_TEMPLATE_BUSY");
      await admin.query(`DROP DATABASE "${database}"`);
    }
    await admin.query(`CREATE DATABASE "${database}" TEMPLATE template0`);
    await admin.query(`REVOKE CONNECT ON DATABASE "${database}" FROM PUBLIC`);
    const url = new URL(connectionString); url.pathname = "/" + database;
    const db = new PostgresDatabase({ connectionString: url.href, max: 1, application_name: "erpx-schema-template" });
    try { await migrateDatabase(db, track, join(root, 'migrations/postgres')); } finally { await db.close(); }
    const comment = json(descriptor).trim().replaceAll("'", "''");
    await admin.query(`COMMENT ON DATABASE "${database}" IS '${comment}'`);
    await admin.query(`ALTER DATABASE "${database}" ALLOW_CONNECTIONS false`);
    await mkdir(directory(root), { recursive: true, mode: 0o700 });
    await atomic(join(directory(root), `${fingerprint}.json`), { ...descriptor, createdAt: Date.now() });
    return descriptor;
  } finally {
    if (lock !== undefined) await admin.query("SELECT pg_advisory_unlock(714928002,$1)", [lock]).catch(() => {});
    await admin.end();
  }
}
/** Clone only an exact immutable broker template into a new test-owned name.
 * The caller records its suite/browser lease before allocation and owns cleanup. */
export async function cloneSchemaTemplate(admin:Pool,target:{database:string;slot:DatabaseSlot},template:Template){
 const pattern=/^erpx_(?:test_[a-f0-9]{10}|browser_[a-f0-9]{12})_(?:control(?:_read)?|h00[1-3](?:_read)?)_db$/;
 if(!pattern.test(target.database)||!(target.slot in databaseTracks)||!target.database.endsWith('_'+target.slot.toLowerCase())
   ||!template||!/^erpx_test_tpl_[a-f0-9]{24}$/.test(template.database)||!/^[a-f0-9]{64}$/.test(template.fingerprint)||template.track!==databaseTracks[target.slot])throw new Error('EXACT_SCHEMA_TEMPLATE_REQUIRED');
 await assertTestAdministrator(admin);
 const identity=(await admin.query("SELECT pg_get_userbyid(datdba) owner,datallowconn,shobj_description(oid,'pg_database') description FROM pg_database WHERE datname=$1",[template.database])).rows[0];
 if(identity?.owner!=='erpx_test_admin'||identity.datallowconn!==false||identity.description!==json(template).trim())throw new Error('TEMPLATE_IDENTITY_CHANGED');
 await admin.query(`CREATE DATABASE "${target.database}" TEMPLATE "${template.database}"`);
 await admin.query(`REVOKE CONNECT ON DATABASE "${target.database}" FROM PUBLIC`);
}
type SuiteLease = { version: 2; root: string; prefix: string; pid: number; token: string; databases: string[]; completed: boolean };
const databasePattern = /^erpx_test_[a-f0-9]{10}_(?:control(?:_read)?|h00[1-3](?:_read)?)_db$/;
const suiteMarker = (lease: SuiteLease) => JSON.stringify({version:2,root:lease.root,prefix:lease.prefix,pid:lease.pid,token:lease.token});
const sqlLiteral = (value: string) => "'" + value.replace(/'/g,"''") + "'";
function validateSuiteLease(lease: SuiteLease, root: string, prefix: string) {
  if(lease.version!==2 || lease.root!==root || lease.prefix!==prefix || !Number.isSafeInteger(lease.pid) || lease.pid<=1 || !/^[a-f0-9-]{36}$/.test(lease.token) || !Array.isArray(lease.databases) || new Set(lease.databases).size!==lease.databases.length)
    throw new Error('TEST_LEASE_IDENTITY_CHANGED');
  for(const database of lease.databases)if(!databasePattern.test(database)||!database.startsWith(prefix+'_'))throw new Error('UNSAFE_ABANDONED_DATABASE');
}
async function dropLeasedDatabases(admin: Pool, lease: SuiteLease) {
  const owned: string[]=[];
  // Preflight all ownership markers before deleting any database.
  for(const database of lease.databases){
    const row=(await admin.query("SELECT pg_get_userbyid(datdba) owner,shobj_description(oid,'pg_database') description FROM pg_database WHERE datname=$1",[database])).rows[0];
    if(!row)continue;
    if(row.owner!=='erpx_test_admin'||row.description!==suiteMarker(lease))throw new Error('TEST_DATABASE_OWNERSHIP_CHANGED');
    // pool.end() can finish before PostgreSQL removes the terminated backend
    // from pg_stat_activity. Wait for that acknowledgement; never force-drop
    // or terminate a still-live connection.
    const deadline=Date.now()+1000;
    while((await admin.query('SELECT 1 FROM pg_stat_activity WHERE datname=$1',[database])).rowCount){
      if(Date.now()>=deadline)throw new Error('OWNED_TEST_DATABASE_STILL_BUSY');
      await new Promise(resolve=>setTimeout(resolve,20));
    }
    owned.push(database);
  }
  for(const database of owned)await admin.query(`DROP DATABASE "${database}"`);
}
export async function createSuiteDatabases(root: string, prefix: string, slots: readonly DatabaseSlot[], templates: Record<string, Template>, mode: "template" | "raw", migratedSlots: readonly DatabaseSlot[]) {
  if (!/^erpx_test_[a-f0-9]{10}$/.test(prefix) || new Set(slots).size !== slots.length || slots.some(slot => !(slot in databaseTracks)) || migratedSlots.some(slot => !slots.includes(slot)) || mode === 'raw' && migratedSlots.length > 0) throw new Error("INVALID_TEST_DATABASE_LEASE");
  const admin = new Pool({ connectionString: testAdministratorUrl(), max: 3 });
  const path = join(directory(root), `${prefix}.lease.json`), created: string[] = [];
  const lease: SuiteLease={version:2,root,prefix,pid:process.pid,token:randomUUID(),databases:slots.map(slot=>`${prefix}_${slot.toLowerCase()}`),completed:false};
  let owner: PoolClient | undefined, recorded=false;
  await mkdir(directory(root), { recursive: true, mode: 0o700 });
  try {
    await assertTestAdministrator(admin);
    owner=await admin.connect();
    const key=createHash('sha256').update(prefix).digest().readInt32BE();
    await owner.query('SELECT pg_advisory_lock(714928003,$1)',[key]);
    if((await owner.query("SELECT 1 FROM pg_database WHERE starts_with(datname,$1)",[prefix+'_'])).rowCount)throw new Error('TEST_DATABASE_PREFIX_ALREADY_IN_USE');
    await atomic(path,lease,true);recorded=true;
    const allocate = async (slot: DatabaseSlot) => {
      const name = `${prefix}_${slot.toLowerCase()}`, template = templates[databaseTracks[slot]];
      if (mode === "template" && migratedSlots.includes(slot)) {
        await cloneSchemaTemplate(admin,{database:name,slot},template);
      } else await admin.query(`CREATE DATABASE "${name}" TEMPLATE template0`);
      created.push(name);
      await admin.query(`COMMENT ON DATABASE "${name}" IS ${sqlLiteral(suiteMarker(lease))}`);
      await admin.query(`REVOKE CONNECT ON DATABASE "${name}" FROM PUBLIC`);
    };
    const pending=[...slots];
    const worker=async()=>{while(pending.length)await allocate(pending.shift()!);};
    const legacyBenchmark=process.env.ERPX_PERFORMANCE_BENCHMARK==='workspace-fixtures-v1'&&process.env.ERPX_FIXTURE_BENCHMARK_MODE==='legacy-raw';
    const outcomes=await Promise.allSettled(legacyBenchmark?[worker()]:[worker(),worker()]);
    const failure=outcomes.find((outcome):outcome is PromiseRejectedResult=>outcome.status==='rejected');
    if(failure)throw failure.reason;
    return lease;
  } catch(error) {
    const failures:unknown[]=[];
    // Only names whose CREATE succeeded in this invocation enter this list.
    for(const database of created.reverse())try{await admin.query(`DROP DATABASE "${database}"`);}catch(cleanup){failures.push(cleanup);}
    if(recorded&&!failures.length)await atomic(path,{...lease,completed:true,creationFailed:true});
    if(failures.length)throw new AggregateError([error,...failures],'TEST_DATABASE_CREATION_CLEANUP_FAILED');
    throw error;
  } finally {owner?.release();await admin.end();}
}
export async function completeSuiteLease(root: string, prefix: string) {
  const path = join(directory(root), `${prefix}.lease.json`);
  const lease:SuiteLease = JSON.parse(await readFile(path, "utf8"));
  validateSuiteLease(lease,root,prefix);
  if(lease.pid!==process.pid)throw new Error('TEST_LEASE_IDENTITY_CHANGED');
  await atomic(path, { ...lease, completed: true, completedAt: Date.now() });
}
export async function cleanupSuiteDatabases(root: string, prefix: string) {
  const lease:SuiteLease=JSON.parse(await readFile(join(directory(root),prefix+'.lease.json'),'utf8'));
  validateSuiteLease(lease,root,prefix);
  if(lease.pid!==process.pid)throw new Error('TEST_LEASE_IDENTITY_CHANGED');
  const admin=new Pool({connectionString:testAdministratorUrl(),max:1});
  try{await assertTestAdministrator(admin);await dropLeasedDatabases(admin,lease);await completeSuiteLease(root,prefix);}finally{await admin.end();}
}
/** Dead owners and exact generation markers are required; never FORCE connections. */
export async function recoverAbandonedSuiteLeases(root: string) {
  await mkdir(directory(root), { recursive: true, mode: 0o700 });
  const admin = new Pool({ connectionString: testAdministratorUrl(), max: 1 });
  try {
    await assertTestAdministrator(admin);
    for (const name of (await readdir(directory(root))).filter(name => /^erpx_test_[a-f0-9]{10}\.lease\.json$/.test(name))) {
      const path = join(directory(root), name), lease = JSON.parse(await readFile(path, "utf8"));
      if (lease.completed || lease.root !== root || !/^erpx_test_[a-f0-9]{10}$/.test(lease.prefix)) continue;
      try { process.kill(lease.pid, 0); continue; } catch (error) { if ((error as NodeJS.ErrnoException).code !== "ESRCH") continue; }
      if(lease.version!==2){
        // Pre-marker leases keep their historical scope. Never adopt a database
        // solely because an older file names it; retain it for explicit review.
        await atomic(path,{...lease,requiresManualReview:true});continue;
      }
      validateSuiteLease(lease,root,lease.prefix);
      await dropLeasedDatabases(admin,lease);
      await atomic(path, { ...lease, completed: true, recoveredAt: Date.now() });
    }
  } finally { await admin.end(); }
}
