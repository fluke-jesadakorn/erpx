/** Current legacy cleanup, disposable leased PostgreSQL only. */
import {createInterface} from 'node:readline';
const input=createInterface({input:process.stdin,crlfDelay:Infinity})[Symbol.asyncIterator]();
const first=await input.next();if(first.done)throw Error('REQUEST_REQUIRED');
const request=JSON.parse(first.value) as {database:string;source:string;mode?:string};
const token=/^erpx_dev_rust_([a-f0-9]{32})$/.exec(request.database)?.[1];if(!token||!request.source)throw Error('OWNED_TEST_DATABASE_REQUIRED');
const raw=process.env.ERPX_TEST_ADMIN_URL;if(!raw)throw Error('TEST_ADMIN_REQUIRED');const url=new URL(raw);
if(!['postgres:','postgresql:'].includes(url.protocol)||url.hostname!=='127.0.0.1'||url.port!=='5432'||url.username!=='erpx_test_admin'||url.pathname!=='/postgres')throw Error('LOCAL_TEST_ADMIN_REQUIRED');url.pathname=`/${request.database}`;
Bun.plugin({name:'owned-test-server-only',setup(build){build.module('server-only',()=>({exports:{},loader:'object'}));}});
const {PostgresDatabase}=await import('@erpx/server-runtime/adapters/runtime/postgres');const db=new PostgresDatabase({connectionString:url.href,max:1});
try{
 const lease=await db.prepare("SELECT pg_get_userbyid(datdba)::text owner,shobj_description(oid,'pg_database') lease FROM pg_database WHERE datname=?").bind(request.database).first<{owner:string;lease:string|null}>();if(lease?.owner!=='erpx_test_admin'||lease.lease!==`erpx-rust-test:${token}`)throw Error('OWNED_TEST_LEASE_REQUIRED');
 process.env.ERPX_ATTACHMENTS_V2='true';delete process.env.ERPX_ATTACHMENT_UPLOAD_FREEZE;
 const {cleanupAttachmentDrafts}=await import('@erpx/server-files/adapters/attachments/service');
 process.stdout.write('READY\n');const command=await input.next();if(command.done||command.value!=='GO')throw Error('START_REQUIRED');
 if(request.mode==='writer'||request.mode==='upload') {const service=await import('@erpx/server-files/adapters/attachments/service');let code='';let puts=0;try{if(request.mode==='writer')await service.reserveAttachmentFile(db,{companyId:'company-owned'} as never,request.source,{filename:'x.csv',mediaType:'text/csv',bytes:1},'fixture-key');else await service.completeAttachmentUpload(db,{put:async()=>{puts++;throw Error('UNEXPECTED_PUT');}} as never,{companyId:'company-owned'} as never,request.source,new Uint8Array([1]).buffer);}catch(error){if(error&&typeof error==='object'&&'code' in error)code=String(error.code);}process.stdout.write(JSON.stringify({code,puts}));} else if(request.mode==='cancel') {await db.withObjectLocks('company-owned',['fixture-cancel'],async()=>{process.stdout.write('LOCKED\n');await new Promise<void>(()=>{});});} else if(request.mode==='failure') {let attempts=0;try{await db.withObjectLocks('company-owned',['fixture-key'],async()=>{attempts++;throw Error('TEST_ABORT');});}catch(error){if(!(error instanceof Error)||error.message!=='TEST_ABORT')throw error;}const locks=await db.prepare("SELECT count(*) count FROM pg_locks WHERE pid=pg_backend_pid() AND locktype='advisory'").first<{count:number}>();process.stdout.write(JSON.stringify({attempts,locks:locks?.count}));} else {
 let deleted=0;const files={supportsTransactionDeletion:false,delete:async()=>{deleted++;},put:async()=>{throw Error('UNEXPECTED_PUT');},get:async()=>null,head:async()=>null};
 const result=await cleanupAttachmentDrafts(db,files as never);
 process.stdout.write(JSON.stringify({result,deleted}));
 }
}finally{await db.pool.end();}
