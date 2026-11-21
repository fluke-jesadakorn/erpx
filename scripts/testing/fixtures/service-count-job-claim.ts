/** Actual TypeScript outbox claimant against a verified disposable Holding. */
import {createInterface} from 'node:readline';
const lines=createInterface({input:process.stdin,crlfDelay:Infinity})[Symbol.asyncIterator]();
const input=await lines.next();if(input.done)throw Error('RACE_REQUEST_REQUIRED');
const {database}=JSON.parse(input.value) as {database:string};
const token=/^erpx_dev_rust_([a-f0-9]{32})$/.exec(database)?.[1];if(!token)throw Error('OWNED_TEST_DATABASE_REQUIRED');
const admin=process.env.ERPX_TEST_ADMIN_URL;if(!admin)throw Error('TEST_ADMIN_REQUIRED');
const url=new URL(admin);if(!['postgresql:','postgres:'].includes(url.protocol)||url.hostname!=='127.0.0.1'||url.port!=='5432'||url.username!=='erpx_test_admin'||url.pathname!=='/postgres')throw Error('LOCAL_TEST_ADMIN_REQUIRED');url.pathname=`/${database}`;
Bun.plugin({name:'job-claim-test-server-only',setup(build){build.module('server-only',()=>({exports:{},loader:'object'}));}});
const {PostgresDatabase}=await import('@erpx/server-runtime/adapters/runtime/postgres');
const {runtimeJobEffects}=await import('@erpx/server-workflow/public/runtime-job-effects');
const {serviceCountOutboxQuarantine}=await import('@erpx/server-inventory/public/service-count-outbox-predicate');
const owner=new PostgresDatabase({connectionString:url.href,max:1});
try {
 const lease=await owner.prepare("SELECT pg_get_userbyid(datdba)::text owner,shobj_description(oid,'pg_database') lease FROM pg_database WHERE datname=?").bind(database).first<{owner:string;lease:string|null}>();
 if(lease?.owner!=='erpx_test_admin'||lease.lease!==`erpx-rust-test:${token}`)throw Error('OWNED_TEST_LEASE_REQUIRED');
 process.stdout.write('READY\n');const go=await lines.next();if(go.done||go.value!=='GO')throw Error('RACE_START_REQUIRED');
 const event=await runtimeJobEffects(owner,serviceCountOutboxQuarantine).claim('typescript-fixture',1_090_000,1_000_000,1_000_000);
 process.stdout.write(JSON.stringify({id:event?.id??null}));
} finally {await owner.pool.end();}
