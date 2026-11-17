/** Temporary source-bound differential oracle; never connect outside an owned Rust test lease. */
import {readFileSync} from 'node:fs';
import {PostgresDatabase} from '@erpx/server-runtime/adapters/runtime/postgres';
import {readListPage} from '@erpx/server-work_projection/public/list-page';

async function main() {
const input = JSON.parse(readFileSync(0, 'utf8')) as {database: string; token: string; company: string; cursor?: string};
if (process.version !== 'v26.11.1' || !/^[0-9a-f]{32}$/.test(input.token) || input.database !== `erpx_dev_rust_${input.token}`) throw Error('EXACT_LEASE_REQUIRED');
const admin = new URL(process.env.ERPX_TEST_ADMIN_URL ?? '');
if (admin.hostname !== '127.0.0.1' || admin.port !== '5432' || admin.username !== 'erpx_test_admin' || admin.pathname !== '/postgres') throw Error('LOCAL_TEST_ADMIN_REQUIRED');
admin.pathname = `/${input.database}`;
const db = new PostgresDatabase({connectionString: admin.href, max: 1});
try {
  const identity = await db.prepare("SELECT current_database()::text database,current_user::text role,pg_get_userbyid(d.datdba)::text owner,shobj_description(d.oid,'pg_database') lease,r.rolsuper,r.rolcreaterole FROM pg_database d JOIN pg_roles r ON r.rolname=current_user WHERE d.datname=current_database()").first<{database:string;role:string;owner:string;lease:string;rolsuper:boolean;rolcreaterole:boolean}>();
  if (!identity || identity.database !== input.database || identity.role !== 'erpx_test_admin' || identity.owner !== 'erpx_test_admin' || identity.lease !== `erpx-rust-test:${input.token}` || identity.rolsuper || identity.rolcreaterole) throw Error('EXACT_LEASE_REQUIRED');
  const source = readFileSync('packages/server-people_payroll/src/adapters/people-payroll.ts','utf8');
  const match = source.match(/pagedRuns:\(companyId,cursor\)=>readListPage<[^\n]+?\{sql:"([^"]+)",args:\[companyId\],cursor,descending:true,scope:\[companyId,'payroll'\]\}/);
  if (!match) throw Error('PAYROLL_HISTORY_SOURCE_CHANGED');
  try {
    const page = await readListPage(db, {sql:match[1]!, args:[input.company], scope:[input.company,'payroll'], cursor:input.cursor, descending:true});
    process.stdout.write(JSON.stringify({page}));
  } catch (error) {
    const e=error as Error & {code?:string;status?:number};
    process.stdout.write(JSON.stringify({error:{name:e.name,message:e.message,code:e.code??null,status:e.status??null}}));
  }
} finally { await db.close(); }
}
main().catch(() => { console.error('PAYROLL_HISTORY_ORACLE_FAILED'); process.exitCode = 1; });
