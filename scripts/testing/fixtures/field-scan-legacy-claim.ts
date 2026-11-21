/** Current legacy repository claimant, disposable leased PostgreSQL only. */
import {createInterface} from 'node:readline';
const input=createInterface({input:process.stdin,crlfDelay:Infinity})[Symbol.asyncIterator]();
const first=await input.next();if(first.done)throw Error('REQUEST_REQUIRED');
const request=JSON.parse(first.value) as {database:string;source:string};
const token=/^erpx_dev_rust_([a-f0-9]{32})$/.exec(request.database)?.[1];if(!token||!request.source)throw Error('OWNED_TEST_DATABASE_REQUIRED');
const raw=process.env.ERPX_TEST_ADMIN_URL;if(!raw)throw Error('TEST_ADMIN_REQUIRED');const url=new URL(raw);
if(!['postgres:','postgresql:'].includes(url.protocol)||url.hostname!=='127.0.0.1'||url.port!=='5432'||url.username!=='erpx_test_admin'||url.pathname!=='/postgres')throw Error('LOCAL_TEST_ADMIN_REQUIRED');url.pathname=`/${request.database}`;
Bun.plugin({name:'owned-test-server-only',setup(build){build.module('server-only',()=>({exports:{},loader:'object'}));}});
const {PostgresDatabase}=await import('@erpx/server-runtime/adapters/runtime/postgres');const db=new PostgresDatabase({connectionString:url.href,max:1});
try{
 const lease=await db.prepare("SELECT pg_get_userbyid(datdba)::text owner,shobj_description(oid,'pg_database') lease FROM pg_database WHERE datname=?").bind(request.database).first<{owner:string;lease:string|null}>();if(lease?.owner!=='erpx_test_admin'||lease.lease!==`erpx-rust-test:${token}`)throw Error('OWNED_TEST_LEASE_REQUIRED');
 const {repository}=await import('@erpx/server-files/adapters/attachments/service-repository');
 process.stdout.write('READY\n');const command=await input.next();if(command.done||command.value!=='GO')throw Error('START_REQUIRED');
 const result=await repository(db).readAttachmentScanAttempts106({now:1000,arg2:61000,lease:'legacy-bun-lease',arg4:request.source,arg5:request.source});
 process.stdout.write(JSON.stringify({claimed:!!result}));
}finally{await db.pool.end();}
