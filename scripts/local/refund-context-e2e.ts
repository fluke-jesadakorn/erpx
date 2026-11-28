/** Manual refund-context e2e setup: disposable DBs + migrations + seeds + service boot + curl.
 * Run with: bun scripts/local/refund-context-e2e.ts
 * Cleans up: drops the two databases and kills the service on exit/failure.
 */
import { spawnSync, spawn, type ChildProcess } from 'node:child_process';
import { randomBytes, createHash } from 'node:crypto';
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
const PORT = 44891;
const TOKEN_PARENT = 'p'.repeat(64);
const TOKEN_CHILD = 'c'.repeat(64);
const sessionDigest = (raw: string) => createHash('sha256')
  .update(Buffer.concat([Buffer.from('erpx-account-v1\0'), Buffer.from('session'), Buffer.from([0]), Buffer.from(raw, 'utf8')]))
  .digest('hex');
const PARENT_DIGEST = sessionDigest(TOKEN_PARENT);
const CHILD_DIGEST = sessionDigest(TOKEN_CHILD);

const admin = new Client({ connectionString: ADMIN });
const step = (msg: string) => console.log(`[e2e] ${msg}`);
let child: ChildProcess | undefined;
async function cleanup(code: number) {
  try { child?.kill('SIGTERM'); } catch { /* already dead */ }
  try {
    await admin.query(`DROP DATABASE IF EXISTS "${H001_DB}"`);
    await admin.query(`DROP DATABASE IF EXISTS "${CONTROL_DB}"`);
  } catch { /* best effort */ }
  await admin.end().catch(() => undefined);
  process.exit(code);
}
process.on('SIGINT', () => void cleanup(130));

const { migrateDatabase } = await import('@erpx/platform-runtime-server/public/runtime/migrations');
const { PostgresDatabase } = await import('@erpx/platform-runtime-server/adapters/runtime/postgres');

await admin.connect();
await admin.query(`CREATE DATABASE "${CONTROL_DB}" TEMPLATE template0`);
await admin.query(`CREATE DATABASE "${H001_DB}" TEMPLATE template0`);
step(`databases created: ${CONTROL_DB}, ${H001_DB}`);

const control = new PostgresDatabase({ connectionString: `postgresql://erpx_test_admin@127.0.0.1:5432/${CONTROL_DB}`, max: 4 });
const holding = new PostgresDatabase({ connectionString: `postgresql://erpx_test_admin@127.0.0.1:5432/${H001_DB}`, max: 4 });
await migrateDatabase(control as never, 'control');
await migrateDatabase(holding as never, 'holding-write');
step('migrations applied');

// Control seed: user, parent+child sessions, bindings, device, holding_bindings.
const q = async (db: InstanceType<typeof PostgresDatabase>, sql: string, args: unknown[] = []) => db.prepare(sql).bind(...args).run();
await q(control, `INSERT INTO "user"(id,name,email,"emailVerified","createdAt","updatedAt") VALUES('user','Synthetic','user@example.test',1,$1,$1)`, [NOW]);
await q(control, `INSERT INTO session(id,"userId",token,"expiresAt","createdAt","updatedAt") VALUES('parent','user','${PARENT_DIGEST}',to_timestamp(($1::bigint+3600000)::double precision/1000)::text,to_timestamp($1::double precision/1000)::text,to_timestamp($1::double precision/1000)::text),('session','user','${CHILD_DIGEST}',to_timestamp(($1::bigint+86400000)::double precision/1000)::text,to_timestamp($1::double precision/1000)::text,to_timestamp($1::double precision/1000)::text)`, [NOW]);
await q(control, `INSERT INTO sso_session_bindings(session_id,audience,environment,parent_session_id,authenticated_at,absolute_expires_at) VALUES('parent','account','development',NULL,$1,$2),('session','holding:development:h001:${HOST}','development','parent',$1,$2)`, [NOW, NOW + 3600000]);
await q(control, `INSERT INTO account_device_browsers(id,active_session_id,revision,updated_at) VALUES('browser','parent',1,$1)`, [NOW]);
await q(control, `INSERT INTO account_device_accounts(session_id,browser_id,handle) VALUES('parent','browser','saved-parent')`, []);
await q(control, `INSERT INTO holding_bindings(holding_id,slug,display_name,write_binding,read_binding,files_binding,status) VALUES('h001','acme','E2E','HOLDING_H001_DB','HOLDING_H001_READ_DB','HOLDING_H001_FILES','active')`, []);
step('control seeded');

// Holding seed: org nodes, role with pms.read, assignment, active context, property+payment+request.
await q(holding, `INSERT INTO organization_nodes(id,kind,slug,display_name,parent_id,company_id,branch_id,status) VALUES('${COMPANY}','company','acme','Acme',NULL,'${COMPANY}',NULL,'active')`, []);
await q(holding, `INSERT INTO iam_roles(company_id,id,name,built_in,published_revision,status) VALUES('*','e2e_pms_reader','E2E',false,1,'active')`, []);
await q(holding, `INSERT INTO iam_role_revisions(company_id,role_id,revision,permissions_json,grantable_json,scopes_json,created_by,created_at) VALUES('*','e2e_pms_reader',1,'["pms.read","module.pms.read"]','[]','["holding"]','fixture',0)`, []);
await q(holding, `INSERT INTO assignments(id,user_id,role,modules_json,company_id,valid_from,valid_until,version,scope_kind) VALUES('grant','user','e2e_pms_reader','["pms"]','${COMPANY}',$1,$2,3,'holding')`, [NOW - 1000, NOW + 3600000]);
await q(holding, `INSERT INTO active_work_contexts(session_id,user_id,company_id,assignment_id,assignment_version,revision,expires_at,updated_at) VALUES('session','user','${COMPANY}','grant',3,7,$1,$2)`, [NOW + 3600000, NOW]);
await q(holding, `INSERT INTO pms_properties(id,company_id,branch_id,name,business_date,state,config_json,version,created_by,updated_at) VALUES('prop-1','${COMPANY}','branch','Riverside','2026-11-04','open','{}',1,'maker',1)`, []);
await q(holding, `INSERT INTO pms_records(id,property_id,kind,data_json,version,updated_at) VALUES('res-1','prop-1','reservation','{"number":"BK-1"}',1,1)`, []);
await q(holding, `INSERT INTO pms_booking_payments(id,property_id,reservation_id,company_id,holding_slug,account_id,livemode,amount_minor,currency,checkout_id,checkout_url,status,expires_at,created_at,updated_at) VALUES('pay-1','prop-1','res-1','${COMPANY}','acme','acct-1',false,5000,'THB','cs-1','https://pay.example/cs-1','confirmed',99,1,1)`, []);
await q(holding, `INSERT INTO pms_booking_refund_requests(id,payment_id,amount_minor,allocations_json,reason,requested_by,state,generation,created_at,updated_at) VALUES('req-1','pay-1',2000,'[]','Guest asked','user-1','failed',0,1,1)`, []);
step('holding seeded (incl failed req-1)');

// Config + boot.
const configPath = `/tmp/erpx-e2e-${tag}.json`;
writeFileSync(configPath, JSON.stringify({
  version: 1, mode: 'test', environment: 'development', transport: 'direct-loopback',
  listen: `127.0.0.1:${PORT}`, accountHost: 'account.e2e.test',
  requireEmailVerification: false, passwordWorkers: 1, maximumRequests: 16, requestTimeoutMs: 10000,
  control: { url: `postgresql://erpx_test_admin@127.0.0.1:5432/${CONTROL_DB}` },
  holdings: [{
    holdingId: 'h001', slug: 'acme', host: HOST,
    writeBinding: 'HOLDING_H001_DB', readBinding: 'HOLDING_H001_READ_DB', filesBinding: 'HOLDING_H001_FILES',
    database: { url: `postgresql://erpx_test_admin@127.0.0.1:5432/${H001_DB}` },
  }],
  operationsCompanies: [{ holdingId: 'h001', companyId: COMPANY }],
  posCompanies: [{ holdingId: 'h001', companyId: COMPANY }],
  crmCompanies: [{ holdingId: 'h001', companyId: COMPANY }],
  commerceCompanies: [{ holdingId: 'h001', companyId: COMPANY }],
}) + '\n', { mode: 0o600 });
chmodSync(configPath, 0o600);
child = spawn('target/debug/erpx-server', ['private-api', '--test-config', configPath], { stdio: ['ignore', 'ignore', 'pipe'] });
let stderr = '';
child.stderr!.on('data', (c: Buffer) => { stderr += c.toString().slice(0, 4000); });
const base = `http://127.0.0.1:${PORT}`;
let ready = false;
for (let n = 0; n < 200; n++) {
  if (child.exitCode !== null) throw new Error(`server exited: ${stderr.slice(0, 800)}`);
  try {
    const r = await fetch(`${base}/api/health`, { redirect: 'manual' });
    if (r.status === 200) { ready = true; break; }
  } catch { /* starting */ }
  await new Promise(r => setTimeout(r, 100));
}
if (!ready) throw new Error(`service not ready: ${stderr.slice(0, 800)}`);
step('service ready');

// Curl the refund-context route as the seeded session.
const cookie = `__Host-erpx-product=${TOKEN_CHILD}`;
async function get(path: string, headers: Record<string, string> = {}) {
  const r = await fetch(`${base}${path}`, { headers: { host: HOST, origin: ORIGIN, accept: 'application/json', cookie, ...headers }, redirect: 'error' });
  const text = await r.text();
  let body: unknown = null;
  try { body = JSON.parse(text); } catch { /* non-JSON */ }
  return { status: r.status, headers: r.headers, body };
}
const hit = await get(`/api/v1/holdings/acme/pms/booking-refunds/context?request=req-1`);
console.log('HIT status:', hit.status);
console.log('HIT body:', JSON.stringify(hit.body).slice(0, 600));
console.log('HIT cache-control:', hit.headers.get('cache-control'));
const miss = await get(`/api/v1/holdings/acme/pms/booking-refunds/context?request=nope`);
console.log('MISS status:', miss.status, 'code:', JSON.stringify((miss.body as { error?: { code?: string } } | null)?.error?.code));
const bad = await get(`/api/v1/holdings/acme/pms/booking-refunds/context?unknown=1`);
console.log('BAD status:', bad.status);

const ok = hit.status === 200
  && (hit.body as { data?: { id?: string; property_id?: string } })?.data?.id === 'req-1'
  && (hit.body as { data?: { property_id?: string } })?.data?.property_id === 'prop-1'
  && miss.status === 404
  && bad.status === 422;
console.log(ok ? 'E2E PASS' : 'E2E FAIL');
rmSync(configPath, { force: true });
await cleanup(ok ? 0 : 1);
