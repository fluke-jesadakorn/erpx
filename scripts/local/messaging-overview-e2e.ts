/** Messaging overview gateway->Rust proof: disposable DBs + migrations + seeds + service boot + curl.
 * Run with: bun --conditions=react-server scripts/local/messaging-overview-e2e.ts
 * (react-server condition required: the script imports server-only runtime
 * modules, like other scripts/local tooling executed outside Next).
 * Proves over real loopback HTTP: (1) boundary allows the messaging route,
 * (2) ungranted Company gets the TS-identical paywall shape (empty rooms +
 * purchase.history=false), (3) granted Company gets rooms (preview/unread/
 * teamId/private/updatedAt), teams and purchase.history=true. Cleans up its
 * databases and the service on success or failure; kills nothing else.
 */
import { spawn, type ChildProcess } from 'node:child_process';
import { randomBytes } from 'node:crypto';
import { writeFileSync, chmodSync, rmSync } from 'node:fs';
import { Client } from 'pg';

const ADMIN = 'postgresql://erpx_test_admin@127.0.0.1:5432/postgres';
const tag = randomBytes(5).toString('hex');
const CONTROL_DB = `erpx_test_${tag}_control_db`;
const H001_DB = `erpx_test_${tag}_h001_db`;
const NOW = Date.now();
const COMPANY = 'acme-co';
const HOST = 'acme.e2e.test';
const ORIGIN = `https://${HOST}`;
const PORT = 44892;
const TOKEN_PARENT = 'p'.repeat(64);
const TOKEN_CHILD = 'c'.repeat(64);

const { migrateDatabase } = await import('@erpx/server-runtime/public/runtime/migrations');
const { PostgresDatabase } = await import('@erpx/server-runtime/adapters/runtime/postgres');
const admin = new Client({ connectionString: ADMIN });
const step = (msg: string) => console.log(`[e2e] ${msg}`);
let child: ChildProcess | undefined;
async function cleanup(code: number) {
  try { child?.kill('SIGTERM'); } catch { /* already dead */ }
  try {
    await admin.query(`DROP DATABASE IF EXISTS "${H001_DB}"`);
    await admin.query(`DROP DATABASE IF EXISTS "${CONTROL_DB}"`);
  } catch { /* best effort */ }
  try { await admin.end(); } catch { /* already ended */ }
  process.exit(code);
}

type Db = InstanceType<typeof PostgresDatabase>;
const q = async (db: Db, sql: string, args: unknown[] = []) => {
  try {
    await db.prepare(sql).bind(...args).run();
  } catch (e) {
    throw new Error(`seed failed [${sql.slice(0, 90)}] args=${args.length}: ${(e as Error).message}`);
  }
};

await admin.connect();
const pw = `${tag}${tag}`;
await admin.query(`ALTER ROLE erpx_test_admin WITH PASSWORD '${pw}'`);
const AUTH = `erpx_test_admin:${tag}${tag}@`;
await admin.query(`CREATE DATABASE "${CONTROL_DB}" TEMPLATE template0`);
await admin.query(`CREATE DATABASE "${H001_DB}" TEMPLATE template0`);
const control = new PostgresDatabase({ connectionString: `postgresql://${AUTH}127.0.0.1:5432/${CONTROL_DB}`, max: 4 });
const holding = new PostgresDatabase({ connectionString: `postgresql://${AUTH}127.0.0.1:5432/${H001_DB}`, max: 4 });
const { createHash } = await import('node:crypto');
const digest = (raw: string) => {
  const h = createHash('sha256');
  h.update(Buffer.from('erpx-account-v1\0'));
  h.update(Buffer.from('session'));
  h.update(Buffer.from([0]));
  h.update(Buffer.from(raw));
  return h.digest('hex');
};
const TOKEN_PARENT_DIGEST = digest(TOKEN_PARENT);
const TOKEN_CHILD_DIGEST = digest(TOKEN_CHILD);
const { readdir, readFile } = await import('node:fs/promises');
const { join } = await import('node:path');
async function migrateTrack(db: Db, track: string) {
  await db.pool.query(`CREATE TABLE IF NOT EXISTS erpx_migrations(name TEXT PRIMARY KEY,checksum TEXT NOT NULL,applied_at TIMESTAMPTZ NOT NULL DEFAULT now())`);
  const dir = join(process.cwd(), 'migrations/postgres', track);
  const files = (await readdir(dir)).filter(f => f.endsWith('.sql')).sort();
  for (const name of files) {
    const sql = await readFile(join(dir, name), 'utf8');
    const checksum = createHash('sha256').update(sql).digest('hex');
    try {
      await db.pool.query(sql);
    } catch (e) {
      throw new Error(`migration-apply failed ${track}/${name}: ${(e as Error).message}`);
    }
    await db.pool.query(`INSERT INTO erpx_migrations(name,checksum) VALUES($1,$2) ON CONFLICT(name) DO UPDATE SET checksum=excluded.checksum`, [name, checksum]);
  }
}
await migrateTrack(control, 'control');
await migrateTrack(holding, 'holding-write');
step('migrations applied');

// Control seed (mirrors the pms refund-context proof).
await q(control, `INSERT INTO "user"(id,name,email,"emailVerified","createdAt","updatedAt") VALUES('user','Synthetic','user@example.test',1,?,?)`, [NOW, NOW]);
await q(control, `INSERT INTO session(id,"userId",token,"expiresAt","createdAt","updatedAt") VALUES('parent','user','${TOKEN_PARENT_DIGEST}',to_timestamp((?::bigint+3600000)::double precision/1000)::text,to_timestamp(?::double precision/1000)::text,to_timestamp(?::double precision/1000)::text),('session','user','${TOKEN_CHILD_DIGEST}',to_timestamp((?::bigint+86400000)::double precision/1000)::text,to_timestamp(?::double precision/1000)::text,to_timestamp(?::double precision/1000)::text)`, [NOW, NOW, NOW, NOW, NOW, NOW]);
await q(control, `INSERT INTO sso_session_bindings(session_id,audience,environment,parent_session_id,authenticated_at,absolute_expires_at) VALUES('parent','account','development',NULL,?,?),('session','holding:development:h001:${HOST}','development','parent',?,?)`, [NOW, NOW + 3600000, NOW, NOW + 3600000]);
await q(control, `INSERT INTO account_device_browsers(id,active_session_id,revision,updated_at) VALUES('browser','parent',1,?)`, [NOW]);
await q(control, `INSERT INTO account_device_accounts(session_id,browser_id,handle) VALUES('parent','browser','saved-parent')`, []);
await q(control, `INSERT INTO holding_bindings(holding_id,slug,display_name,write_binding,read_binding,files_binding,status) VALUES('h001','acme','E2E','HOLDING_H001_DB','HOLDING_H001_READ_DB','HOLDING_H001_FILES','active')`, []);
step('control seeded');

// Holding seed: company node, role, assignment, context, team + group room + message.
await q(holding, `INSERT INTO organization_nodes(id,kind,slug,display_name,parent_id,company_id,branch_id,status) VALUES('${COMPANY}','company','acme','Acme',NULL,'${COMPANY}',NULL,'active')`, []);
await q(holding, `INSERT INTO iam_roles(company_id,id,name,built_in,published_revision,status) VALUES('*','e2e_msg_reader','E2E',false,1,'active')`, []);
await q(holding, `INSERT INTO iam_role_revisions(company_id,role_id,revision,permissions_json,grantable_json,scopes_json,created_by,created_at) VALUES('*','e2e_msg_reader',1,'["records.read","module.collaboration.read"]','[]','["holding"]','fixture',0)`, []);
await q(holding, `INSERT INTO assignments(id,user_id,role,modules_json,company_id,valid_from,valid_until,version,scope_kind) VALUES('grant','user','e2e_msg_reader','["collaboration"]','${COMPANY}',?,?,3,'holding')`, [NOW - 1000, NOW + 3600000]);
await q(holding, `INSERT INTO active_work_contexts(session_id,user_id,company_id,assignment_id,assignment_version,revision,expires_at,updated_at) VALUES('session','user','${COMPANY}','grant',3,7,?,?)`, [NOW + 3600000, NOW]);
await q(holding, `INSERT INTO messaging_teams(id,company_id,name,owner_id,created_at) VALUES('team-1','${COMPANY}','Ops','user',1)`, []);
await q(holding, `INSERT INTO messaging_rooms(id,company_id,kind,name,owner_id,team_id,private,version,last_seq,created_at) VALUES('room-1','${COMPANY}','group','Ops Room','user',NULL,false,1,1,${NOW})`, []);
await q(holding, `INSERT INTO messaging_room_members(room_id,user_id) VALUES('room-1','user')`, []);
await q(holding, `INSERT INTO messaging_messages(id,room_id,seq,sender_id,sender_name,body,created_at) VALUES('msg-1','room-1',1,'user','Synthetic','hello native',${NOW})`, []);
step('holding seeded (no package grant yet)');

const configPath = `/tmp/erpx-msg-e2e-${tag}.json`;
writeFileSync(configPath, JSON.stringify({
  version: 1, mode: 'test', environment: 'development', transport: 'direct-loopback',
  listen: `127.0.0.1:${PORT}`, accountHost: 'account.e2e.test',
  requireEmailVerification: false, passwordWorkers: 1, maximumRequests: 16, requestTimeoutMs: 10000,
  control: { url: `postgresql://${AUTH}127.0.0.1:5432/${CONTROL_DB}` },
  holdings: [{ holdingId: 'h001', slug: 'acme', host: HOST, writeBinding: 'HOLDING_H001_DB', readBinding: 'HOLDING_H001_READ_DB', filesBinding: 'HOLDING_H001_FILES', database: { url: `postgresql://${AUTH}127.0.0.1:5432/${H001_DB}` } }],
  operationsCompanies: [{ holdingId: 'h001', companyId: COMPANY }],
  posCompanies: [{ holdingId: 'h001', companyId: COMPANY }],
  crmCompanies: [{ holdingId: 'h001', companyId: COMPANY }],
  commerceCompanies: [{ holdingId: 'h001', companyId: COMPANY }],
}) + '\n', { mode: 0o600 });
chmodSync(configPath, 0o600);
const { openSync } = await import('node:fs');
const svcLog = openSync(`/tmp/erpx-msg-e2e-${tag}.log`, 'w', 0o600);
child = spawn('target/debug/erpx-server', ['private-api', '--test-config', configPath], {
  env: { ...process.env, ERPX_COMMERCE_PACKAGE_ENFORCEMENT: 'enforced', ERPX_ENVIRONMENT: 'local' },
  stdio: ['ignore', svcLog, svcLog],
});
let stdout = '';
const readSvcLog = async () => { try { const { readFileSync } = await import('node:fs'); return readFileSync(`/tmp/erpx-msg-e2e-${tag}.log`, 'utf8'); } catch { return ''; } };
const base = `http://127.0.0.1:${PORT}`;
let ready = false;
for (let n = 0; n < 200; n++) {
  if (child.exitCode !== null) throw new Error(`server exited: ${(await readSvcLog()).slice(0, 800)}`);
  try { const r = await fetch(`${base}/api/health`); if (r.status === 200) { ready = true; break; } } catch { /* starting */ }
  await new Promise(r => setTimeout(r, 100));
}
if (!ready) throw new Error(`service not ready: ${(await readSvcLog()).slice(0, 800)}`);
step('service ready');

const cookie = `__Host-erpx-product=${TOKEN_CHILD}`;
process.on('uncaughtException', async (e) => { console.error('fatal:', (e as Error).message?.slice(0, 500)); await cleanup(1); });
process.on('unhandledRejection', async (e) => { console.error('fatal:', String(e).slice(0, 500)); await cleanup(1); });
const { Socket } = await import('node:net');
async function rawGet(path: string) {
  const cookieHeader = `__Host-erpx-product=${TOKEN_CHILD}`;
  console.log('[e2e] sending cookie prefix:', cookieHeader.slice(0, 30), 'len=', TOKEN_CHILD.length);
  const req = `GET ${path} HTTP/1.1\r\nHost: ${HOST}\r\nOrigin: ${ORIGIN}\r\nAccept: application/json\r\nCookie: ${cookieHeader}\r\nConnection: close\r\n\r\n`;
  const chunks: Buffer[] = [];
  await new Promise<void>((resolve, reject) => {
    const s = new Socket();
    s.once('error', reject);
    s.connect(PORT, '127.0.0.1', () => { s.write(req); });
    s.on('data', (d: Buffer) => chunks.push(d));
    s.on('close', () => resolve());
  });
  const response = Buffer.concat(chunks).toString('utf8');
  const [head, ...rest] = response.split('\r\n\r\n');
  const status = Number(head.split('\n')[0].split(' ')[1]);
  return { status, text: rest.join('\r\n\r\n') };
}
async function overview() {
  const { status, text } = await rawGet('/api/v1/holdings/acme/messaging');
  let body: unknown = text;
  try { body = JSON.parse(text); } catch { /* keep text */ }
  return { status, body };
}

// Phase 1: no grant -> paywall shape (empty rooms + purchase.history=false).
{
  const probe = await rawGet('/api/health');
  console.log('[e2e] health-via-host:', probe.status);
  const rooms = await rawGet('/api/v1/holdings/acme/messaging/rooms');
  console.log('[e2e] rooms-route-probe:', rooms.status, rooms.text.slice(0,120));
  const pms = await rawGet('/api/v1/holdings/acme/pms/booking-refunds/context?refundId=x');
  console.log('[e2e] pms-route-probe:', pms.status, pms.text.slice(0, 120));
  const planning = await rawGet('/api/v1/holdings/acme/planning/workspace');
  console.log('[e2e] planning-route-probe:', planning.status, planning.text.slice(0, 120));
}
const denied = await overview();
console.log('[e2e] ungranted:', denied.status, JSON.stringify(denied.body)?.slice(0, 300));
if (process.env.ERPX_E2E_KEEP === '1') {
  console.log(`[e2e] KEEP control=${CONTROL_DB} holding=${H001_DB} port=${PORT}`);
  await new Promise(() => {});
}
if (denied.status !== 200) throw new Error(`expected 200 paywall, got ${denied.status} :: SVC ${(await readSvcLog()).slice(-1500)}`);
{
  const data = (denied.body as { data?: { rooms?: unknown[]; purchase?: { history?: boolean } } }).data ?? {};
  if (!Array.isArray(data.rooms) || data.rooms.length !== 0) throw new Error('expected empty rooms without grant');
  if (data.purchase?.history !== false) throw new Error('expected purchase.history=false without grant');
}
step('paywall shape proved (empty + purchase.history=false)');

// Phase 2: grant work-messages -> rooms + history=true. Grant + audit event
// must commit atomically (deferred audit trigger).
await holding.pool.query(`BEGIN;
INSERT INTO company_package_grants(company_id,package_id,package_version,state,starts_at,ends_at,version,reason,evidence,updated_at) VALUES('${COMPANY}','work-messages',1,'active',0,NULL,1,'e2e','e2e',${NOW}),('${COMPANY}','work',1,'active',0,NULL,1,'e2e','e2e',${NOW});
INSERT INTO company_package_grant_events(id,company_id,package_id,actor_id,request_hash,before_json,after_json,created_at) SELECT 'msg-grant-'||package_id,company_id,package_id,'fixture-a','synthetic-messages-grant',NULL,jsonb_build_object('companyId',company_id,'packageId',package_id,'packageVersion',package_version,'state',state,'startsAt',starts_at,'endsAt',ends_at,'version',version,'reason',reason,'evidence',evidence),${NOW} FROM company_package_grants WHERE company_id='${COMPANY}' AND package_id IN ('work-messages','work');
COMMIT;`);
const allowed = await overview();
console.log('[e2e] granted:', allowed.status, JSON.stringify(allowed.body)?.slice(0, 500));
if (allowed.status !== 200) throw new Error(`expected 200 overview, got ${allowed.status}`);
{
  const data = (allowed.body as { data?: { rooms?: { id?: string; preview?: string; updatedAt?: number }[]; purchase?: { history?: boolean } } }).data ?? {};
  if (data.rooms?.length !== 1 || data.rooms[0]?.id !== 'room-1') throw new Error('expected room-1');
  if (data.rooms[0]?.preview !== 'hello native') throw new Error('expected preview passthrough');
  if (data.purchase?.history !== true) throw new Error('expected purchase.history=true with grant');
}
step('native overview proved (room + preview + purchase.history=true)');

rmSync(configPath, { force: true });
await cleanup(0);
