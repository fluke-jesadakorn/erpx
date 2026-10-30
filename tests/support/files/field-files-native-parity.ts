import {spawn,execFile} from 'node:child_process';
import {request as httpRequest} from 'node:http';
import {promisify} from 'node:util';
import {mkdtemp,readFile,rm,stat,writeFile,rename} from 'node:fs/promises';
import {tmpdir} from 'node:os';
import {join} from 'node:path';
import {randomUUID,createHash} from 'node:crypto';
import {Pool} from 'pg';
import {expect} from 'vitest';
import {PostgresDatabase} from '@erpx/server-runtime/adapters/runtime/postgres';
import {attachmentToolEnvironment} from '../../../scripts/testing/runtime-tooling';

const sleep=(ms:number)=>new Promise(resolve=>setTimeout(resolve,ms));
const key=()=>randomUUID();
type Session={cookie:string;companyId:string;assignmentId:string;assignmentVersion:number;revision:number};
type PrivateFixture={nonce:string;address:string;cookie:string;foreignCookie:string;database:{url:string;leaseToken:string};store:string;sessions?:Record<string,Session>};
export type MixedActivation={companyId:string;actors:{label:string;userId:string;assignmentId:string;assignmentVersion:number;role:string;modules:string[];branchId:string|null;departmentId:string|null}[]};

let build:Promise<unknown>|undefined;

type NativeSet={id:string;state:string};
type NativeReservation={uploadIntentId:string;attachmentLinkId:string;uploadUrl:string};
type NativeData=NativeSet & NativeReservation & {sourceId:string};
type NativeResponse<T=NativeData>={status:number;raw:Buffer;headers:Headers;json:{data:T;error:{code:string;message:string}}};
type DatabaseRow={owner:string;lease:string;blocked:boolean;source_state:string;quarantine_key:string;object_key:string;variant:string;bytes:string|number;state:string;n:string|number;is_called:boolean;[key:string]:unknown};
export type NativeFieldFixture={base:string;store:string;query:(sql:string,args?:unknown[])=>Promise<DatabaseRow[]>;request:<T=NativeData>(method:string,path:string,data?:unknown,id?:string,foreign?:boolean)=>Promise<NativeResponse<T>>;startScan:()=>void;assignmentFor:(label:string)=>string;advanceClock:()=>Promise<void>;requestAs:<T=NativeData>(label:string,method:string,path:string,data?:unknown,id?:string)=>Promise<NativeResponse<T>>;committedWriter:<T>(sql:string,args:unknown[],read:()=>Promise<T>)=>Promise<T>};
/** One native process and owned database lease per reviewed case. */
export async function withNativeFieldFixture<T>(run:(fixture:NativeFieldFixture)=>Promise<T>,prepareMixed?:(db:PostgresDatabase)=>Promise<MixedActivation>,profile?:'parts'):Promise<T> {
 const directory=await mkdtemp(join(tmpdir(),'erpx-field-parity-')), output=join(directory,'private.json'),nonce=randomUUID();
 const allowed=['PATH','HOME','NODE_ENV','TMPDIR','LANG','LC_ALL','LC_CTYPE','TZ','CARGO_HOME','RUSTUP_HOME','RUSTUP_TOOLCHAIN','CARGO_TARGET_DIR','CARGO_BUILD_TARGET','CARGO_INCREMENTAL','CARGO_PROFILE_DEV_DEBUG','CARGO_PROFILE_TEST_DEBUG','CARGO_ENCODED_RUSTFLAGS','CC','CXX','AR','CFLAGS','CXXFLAGS','LDFLAGS','PKG_CONFIG_PATH','ERPX_NATIVE_VIPS_PREFIX','ERPX_NATIVE_VIPS_LIBDIR','ERPX_TEST_ADMIN_URL','ERPX_FILES_PREVIEW_BIN'];
 const environment=Object.fromEntries(allowed.flatMap(name=>process.env[name]===undefined?[]:[[name,process.env[name]]])) as NodeJS.ProcessEnv;
 Object.assign(environment,attachmentToolEnvironment(process.cwd()),{ERPX_FIELD_FILES_FIXTURE_OUTPUT:output,ERPX_FIELD_FILES_FIXTURE_NONCE:nonce,...(prepareMixed?{ERPX_FIELD_FILES_MIXED_PROFILE:'1'}:{}),...(profile==='parts'?{ERPX_FIELD_PARTS_MIXED_PROFILE:'1'}:{})});
 build??=promisify(execFile)('cargo',['build','--locked','-p','erpx-service','--example','field_files_vitest_held_provider','--no-default-features','--features','private-field-api,files-scan-worker'],{cwd:process.cwd(),env:environment,maxBuffer:10*1024*1024});
 try{await build;}catch{await rm(directory,{recursive:true,force:true});throw new Error('Native Field fixture build failed');}
 const binary=join(process.env.CARGO_TARGET_DIR??join(process.cwd(),'target'),'debug/examples/field_files_vitest_held_provider');
 const child=spawn(binary,[],{cwd:process.cwd(),env:environment,stdio:['pipe','ignore','pipe']});
 let diagnostics='',fixturePanicked=false;child.stderr.on('data',(chunk:Buffer)=>{if(diagnostics.length<16384)diagnostics+=chunk.toString().slice(0,16384-diagnostics.length);fixturePanicked=diagnostics.includes('panicked at ');});
 let spawnFailed=false,finished=false,failed=true;child.once('error',()=>{spawnFailed=true;});child.stdin.on('error',()=>{});
 const closed=new Promise<void>(resolve=>child.once('close',()=>{finished=true;resolve();}));
 let pool:Pool|undefined,mixedDb:PostgresDatabase|undefined;
 try {
  async function readDescriptor(path:string):Promise<PrivateFixture>{
   let value:PrivateFixture|undefined;for(let n=0;n<600;n++){try{value=JSON.parse(await readFile(path,'utf8'));break;}catch{if(spawnFailed||fixturePanicked||child.exitCode!==null)throw new Error('Native Field fixture exited during setup');await sleep(500);}}
   if(!value)throw new Error('Native Field fixture did not become ready');const info=await stat(path);if(!info.isFile()||(info.mode&0o777)!==0o600||info.size>65536||value.nonce!==nonce)throw new Error('Invalid private fixture output');
   if(!/^[0-9a-f]{32}$/.test(value.database.leaseToken))throw new Error('Invalid native fixture lease');const url=new URL(value.database.url),admin=new URL(environment.ERPX_TEST_ADMIN_URL!);
   const invalid=[url.protocol!=='postgresql:'&&url.protocol!=='postgres:'&&'protocol',url.hostname!=='127.0.0.1'&&'host',url.port!=='5432'&&'port',url.username!=='erpx_test_admin'&&'user',url.pathname!==`/erpx_dev_rust_${value.database.leaseToken}`&&'database',url.username!==admin.username&&'admin-user',!!url.search&&'query',!!url.hash&&'fragment'].filter(Boolean);if(invalid.length)throw new Error(`Invalid native fixture database: ${invalid.join(',')}`);
   return value;
  }
  const seed=await readDescriptor(prepareMixed?`${output}.seed`:output);
  pool=new Pool({connectionString:seed.database.url,max:2});
  const query=async(sql:string,args:unknown[]=[]):Promise<DatabaseRow[]>=> (await pool!.query(sql,args)).rows as DatabaseRow[];
  const ownership=await query("SELECT pg_get_userbyid(datdba)::text owner,shobj_description(oid,'pg_database') lease FROM pg_database WHERE datname=current_database()");
  if(ownership.length!==1||ownership[0].owner!=='erpx_test_admin'||ownership[0].lease!==`erpx-rust-test:${seed.database.leaseToken}`)throw new Error('Native fixture lease ownership invalid');
  if(prepareMixed){mixedDb=new PostgresDatabase({connectionString:seed.database.url,max:4});const activation=await prepareMixed(mixedDb);child.stdin.write(`${JSON.stringify(activation)}\n`);}
  const fixture=prepareMixed?await readDescriptor(output):seed;
  if(fixture.database.url!==seed.database.url||fixture.database.leaseToken!==seed.database.leaseToken||fixture.store!==seed.store)throw new Error('Native fixture activation mismatch');
  if(!/^127\.0\.0\.1:[1-9][0-9]{0,4}$/.test(fixture.address))throw new Error('Invalid native fixture listener');
  const base='/api/v1/holdings/acme/field-service/attachment-sets';
  const headers={'host':'acme.example.test','cookie':fixture.cookie,'origin':'https://acme.example.test','x-erpx-context-company':'company','x-erpx-context-assignment':'field-fixture-a','x-erpx-context-assignment-version':'3','x-erpx-context-revision':'8'};
  async function request<T=NativeData>(method:string,path:string,data?:unknown,id:string=key(),foreign=false,actor?:string):Promise<NativeResponse<T>>{
   const payload=data===undefined?undefined:Buffer.isBuffer(data)?data:Buffer.from(JSON.stringify(data));
   const response=await new Promise<{status:number;raw:Buffer;headers:Headers}>((resolve,reject)=>{const req=httpRequest({host:'127.0.0.1',port:Number(fixture!.address.split(':').at(-1)),path,method,headers:{...headers,...(foreign?{'cookie':fixture.foreignCookie,'x-erpx-context-company':'foreign','x-erpx-context-assignment':'field-foreign'}:{}),...(actor?{'cookie':fixture.sessions![actor].cookie,'x-erpx-context-company':fixture.sessions![actor].companyId,'x-erpx-context-assignment':fixture.sessions![actor].assignmentId,'x-erpx-context-assignment-version':String(fixture.sessions![actor].assignmentVersion),'x-erpx-context-revision':String(fixture.sessions![actor].revision)}:{}),'idempotency-key':id,...(payload?{'content-type':Buffer.isBuffer(data)?'image/png':'application/json','content-length':payload.length}:{})}},res=>{const chunks:Buffer[]=[];res.on('data',(chunk:Buffer)=>chunks.push(chunk));res.on('end',()=>resolve({status:res.statusCode!,raw:Buffer.concat(chunks),headers:new Headers(Object.entries(res.headers).flatMap(([k,v])=>v===undefined?[]:[[k,Array.isArray(v)?v.join(', '):v]]))}));});req.on('error',reject);req.end(payload);});
   let json:NativeResponse<T>['json']|undefined;try{json=JSON.parse(response.raw.toString()) as NativeResponse<T>['json'];}catch{}return {...response,json:json as NativeResponse<T>['json']};
  }
  async function committedWriter<T>(sql:string,args:unknown[],read:()=>Promise<T>):Promise<T>{
   const writer=await pool!.connect();let inTransaction=false;
   try{await writer.query('BEGIN');inTransaction=true;await writer.query(sql,args);const pid=(await writer.query('SELECT pg_backend_pid() pid')).rows[0].pid;let completed=false;const observation=read().then(value=>({value}),cause=>({cause})).finally(()=>{completed=true;});let blocked=false;
    for(let n=0;n<200;n++){blocked=(await query('SELECT EXISTS(SELECT 1 FROM pg_stat_activity a WHERE $1::int=ANY(pg_blocking_pids(a.pid))) blocked',[pid]))[0].blocked;if(blocked||completed)break;await sleep(10);}
    await writer.query('COMMIT');inTransaction=false;const result=await observation;if(!blocked)throw new Error('Native reader did not wait for committed source writer');if('cause' in result)throw result.cause;return result.value;
   }finally{if(inTransaction)await writer.query('ROLLBACK');writer.release();}
  }
  async function advanceClock(){if(!prepareMixed)throw new Error('Mixed fixture clock command required');const path=`${output}.clock`;child.stdin.write('advance-clock\n');let acknowledgement:{nonce:string}|undefined;for(let n=0;n<200;n++){try{acknowledgement=JSON.parse(await readFile(path,'utf8'));break;}catch{if(fixturePanicked||child.exitCode!==null)throw new Error('Native fixture clock advance failed');await sleep(10);}}const info=await stat(path);if(!acknowledgement||acknowledgement.nonce!==nonce||!info.isFile()||(info.mode&0o777)!==0o600||info.size>1024)throw new Error('Invalid native fixture clock acknowledgement');}
  const result=await run({base,store:fixture.store,query,request,advanceClock,assignmentFor:label=>{if(!fixture.sessions?.[label])throw new Error('Unknown native fixture actor');return fixture.sessions[label].assignmentId;},requestAs:<T=NativeData>(label:string,method:string,path:string,data?:unknown,id?:string)=>{if(!fixture.sessions?.[label])throw new Error('Unknown native fixture actor');return request<T>(method,path,data,id,false,label);},startScan:()=>child.stdin.write('scan\n'),committedWriter});
  failed=false;
  return result;
 } finally {
  await mixedDb?.close();await pool?.end();child.stdin.end('quit\n');
  if(!finished)await Promise.race([closed,sleep(30_000)]);
  if(!finished){child.kill('SIGTERM');await Promise.race([closed,sleep(5_000)]);}
  if(!finished){child.kill('SIGKILL');await Promise.race([closed,sleep(5_000)]);}
  await rm(output,{force:true});await rm(`${output}.seed`,{force:true});await rm(`${output}.clock`,{force:true});
  if(failed||!finished||child.exitCode!==0){await writeFile(join(directory,'failure.log'),`Native Field fixture failed; spawn=${spawnFailed}, closed=${finished}, exit=${child.exitCode}, signal=${child.signalCode}\n${diagnostics.split('\n').filter(line=>/panicked at .*:[0-9]+:[0-9]+:/.test(line)).map(line=>line.replace(/^.*panicked at /,'panicked at ')).join('\n')}\n`,{mode:0o600});}else await rm(directory,{recursive:true,force:true});
  if(!failed&&(!finished||child.exitCode!==0))throw new Error('Native Field fixture cleanup failed; private diagnostic retained');
 }
}

export async function proveNativeFieldFiles(purposeKey:string,bytes:Buffer) {
 return withNativeFieldFixture(async ({base,store,query,request,startScan})=>{
  const input={targetType:'field_service_visit',targetId:'issue-order',componentKind:'record',purposeKey,expectedTargetVersion:3};
  const stale=await request('POST',base,{...input,expectedTargetVersion:4});expect(stale.status,JSON.stringify(stale.json)).toBe(409);expect(stale.json.error.code).toBe('ATTACHMENT_TARGET_CHANGED');
  const createKey=key(),created=await request('POST',base,input,createKey);expect(created.status).toBe(201);const set=created.json.data;
  expect((await request('POST',base,input,createKey)).json.data).toMatchObject({id:set.id,state:'draft'});
  const file={filename:'synthetic-business-owner-evidence.png',mediaType:'image/png',bytes:bytes.length,caption:'Declared fictional native owner and clean scanner fixtures.'},reserveKey=key(),reserve=await request('POST',`${base}/${set.id}/files`,file,reserveKey);expect(reserve.status).toBe(201);const reserved=reserve.json.data;
  expect((await request('POST',`${base}/${set.id}/files`,file,reserveKey)).json.data).toEqual(reserved);
  const conflict=await request('POST',`${base}/${set.id}/files`,{...file,caption:'Changed file payload'},reserveKey);expect(conflict.status).toBe(409);expect(conflict.json.error.code).toBe('IDEMPOTENCY_CONFLICT');
  async function pending(ref:string){const s=await request('POST',base,{...input,componentRef:ref});expect(s.status).toBe(201);const r=await request('POST',`${base}/${s.json.data.id}/files`,file);expect(r.status).toBe(201);return r.json.data;}
  const staleReservation=await pending('record:stale-intake'),revokedReservation=await pending('record:revocation-intake');
  const upload=await request('PUT',reserved.uploadUrl,bytes);expect(upload.status).toBe(202);const uploaded=upload.json.data;expect(uploaded).toMatchObject({state:'quarantined',status:'scan_pending'});
  expect((await request('PUT',reserved.uploadUrl,bytes)).json.data.sourceId).toBe(uploaded.sourceId);
  const link=`/api/v1/holdings/acme/field-service/attachment-links/${reserved.attachmentLinkId}`,original=`${link}/content?variant=original`,preview=`${link}/content?variant=preview`;
  const locked=await request('GET',original);expect(locked.status).toBe(423);expect(locked.json.error.code).toBe('ATTACHMENT_NOT_AVAILABLE');startScan();
  let available=false;for(let n=0;n<120;n++){const rows=await query('SELECT source_state FROM attachments WHERE id=$1',[uploaded.sourceId]);if(rows[0]?.source_state==='available'){available=true;break;}await sleep(500);}expect(available).toBe(true);
  expect((await request('GET',`${link}/status`)).json.data).toMatchObject({sourceState:'available',linkState:'draft',bindable:true,previewAvailable:true,scan:{lastResult:'clean',lastError:null}});
  const digest=createHash('sha256').update(bytes).digest('hex');
  expect((await query('SELECT company_id,owner_type,owner_id,purpose,checksum_sha256,bytes,detected_media_type,source_state FROM attachments WHERE id=$1',[uploaded.sourceId]))[0]).toMatchObject({company_id:'company',owner_type:'field_service_visit',owner_id:'issue-order',purpose:purposeKey,checksum_sha256:digest,bytes:bytes.length,detected_media_type:'image/png',source_state:'available'});
  expect((await query('SELECT status,checksum_sha256,actual_bytes FROM upload_intents WHERE id=$1',[reserved.uploadIntentId]))[0]).toEqual({status:'ready',checksum_sha256:digest,actual_bytes:bytes.length});
  const bindPath=`/api/v1/holdings/acme/attachment-sets/${set.id}/bind`,command=key(),binding={expectedTargetVersion:3};
  const invalidBind=await request('POST',bindPath,{expectedTargetVersion:4});expect(invalidBind.status).toBe(409);expect(invalidBind.json.error.code).toBe('ATTACHMENT_SET_STALE');
  const bound=await request('POST',bindPath,binding,command);expect(bound.status).toBe(200);expect((await request('POST',bindPath,binding,command)).json).toEqual(bound.json);
  expect(Number((await query("SELECT version FROM field_service_orders WHERE id='issue-order'"))[0].version)).toBe(4);
  expect((await query('SELECT s.state set_state,l.state link_state,l.binding_type,l.binding_id,s.bound_command_id FROM attachment_sets s JOIN attachment_links l ON l.set_id=s.id WHERE s.id=$1',[set.id]))[0]).toMatchObject({set_state:'bound',link_state:'bound',binding_type:'record_document',binding_id:'issue-order',bound_command_id:command});
  expect((await request('PUT',reserved.uploadUrl,bytes)).json.data).toMatchObject({sourceId:uploaded.sourceId,state:'available'});
  const staleUpload=await request('PUT',staleReservation.uploadUrl,bytes);expect(staleUpload.status).toBe(409);expect(staleUpload.json.error.code).toBe('ATTACHMENT_TARGET_CHANGED');
  const objectDirectory=(objectKey:string)=>join(store,createHash('sha256').update(objectKey).digest('hex'));
  async function pendingUntouched(reservation:NativeReservation){const row=(await query('SELECT status,actual_bytes,quarantine_key FROM upload_intents WHERE id=$1',[reservation.uploadIntentId]))[0];expect(row).toMatchObject({status:'pending',actual_bytes:null});await expect(stat(join(objectDirectory(row.quarantine_key),'current.json'))).rejects.toMatchObject({code:'ENOENT'});}
  await pendingUntouched(staleReservation);
  const listed=await request<NativeSet[]>('GET',`${base}?targetType=field_service_visit&targetId=issue-order&purposeKey=${purposeKey}&componentKind=record`);expect(listed.status).toBe(200);expect(listed.json.data.filter((s)=>s.state==='bound')).toHaveLength(1);expect(listed.json.data.find((s)=>s.id===set.id)).toMatchObject({state:'bound',expectedTargetVersion:3,links:[expect.objectContaining({id:reserved.attachmentLinkId,sourceId:uploaded.sourceId,linkState:'bound',sourceState:'available',bytes:bytes.length})]});
  expect((await request('GET',original)).raw).toEqual(bytes);const image=await request('GET',preview);expect(image.headers.get('content-type')).toBe('image/webp');expect(image.raw.length).toBeGreaterThan(0);
  const ledger=await query('SELECT variant,object_key,bytes,state FROM attachment_storage_ledger WHERE attachment_id=$1 ORDER BY variant',[uploaded.sourceId]);expect(ledger.map(r=>r.variant)).toEqual(['original','preview']);expect(ledger.every(r=>r.state==='stored')).toBe(true);for(const row of ledger){const directory=objectDirectory(row.object_key),manifest=JSON.parse(await readFile(join(directory,'current.json'),'utf8'));expect(manifest.key).toBe(row.object_key);const stored=await readFile(join(directory,manifest.version));expect(stored.length).toBe(Number(row.bytes));expect(createHash('sha256').update(stored).digest('hex')).toBe(manifest.etag);}
  expect(Number((await query("SELECT COALESCE(sum(bytes),0) bytes FROM attachment_storage_ledger WHERE company_id='company' AND state='stored'"))[0].bytes)).toBe(ledger.reduce((sum,r)=>sum+Number(r.bytes),0));expect(Number((await query("SELECT COALESCE(sum(declared_bytes),0) bytes FROM upload_intents WHERE company_id='company' AND status='pending'"))[0].bytes)).toBe(2*bytes.length);
  const foreignList=await request('GET',`${base}?targetType=field_service_visit&targetId=issue-order`,undefined,key(),true);expect(foreignList.status).toBe(200);expect(foreignList.json.data).toEqual([]);const foreignContent=await request('GET',original,undefined,key(),true);expect(foreignContent.status).toBe(404);expect(foreignContent.json.error.code).toBe('ATTACHMENT_LINK_NOT_FOUND');const foreignReserve=await request('POST',`${base}/${set.id}/files`,file,key(),true);expect(foreignReserve.status).toBe(404);expect(foreignReserve.json.error.code).toBe('ATTACHMENT_SET_NOT_FOUND');
  await query("UPDATE assignments SET revoked_at=$1,version=version+1 WHERE id='field-fixture-a'",[Date.now()]);
  const effectsBefore=await query("SELECT (SELECT count(*) FROM attachments) sources,(SELECT count(*) FROM attachment_storage_ledger) ledger,(SELECT count(*) FROM command_receipts) receipts,(SELECT count(*) FROM audit_events) audits,(SELECT count(*) FROM outbox_events) outbox");
  // The private HTTP entry resolves the opaque session's live work context first.
  // A revoked role is a 409 here; the legacy cached-actor service returned 404.
  const denials=[];for(const [method,path,data,id] of [['GET',base+'?targetType=field_service_visit&targetId=issue-order',undefined,key()],['GET',original,undefined,key()],['POST',base,input,key()],['POST',bindPath,binding,command],['PUT',reserved.uploadUrl,bytes,key()],['PUT',revokedReservation.uploadUrl,bytes,key()]] as const){const denial=await request(method,path,data,id);denials.push({method,status:denial.status,code:denial.json?.error?.code});}expect(denials).toEqual(Array.from({length:6},()=>expect.objectContaining({status:409,code:'WORK_ROLE_UNAVAILABLE'})));
  expect(await query("SELECT (SELECT count(*) FROM attachments) sources,(SELECT count(*) FROM attachment_storage_ledger) ledger,(SELECT count(*) FROM command_receipts) receipts,(SELECT count(*) FROM audit_events) audits,(SELECT count(*) FROM outbox_events) outbox")).toEqual(effectsBefore);
  await pendingUntouched(revokedReservation);expect(Number((await query("SELECT count(*) n FROM attachments WHERE company_id='company'"))[0].n)).toBe(1);
  for(const table of ['audit_events','outbox_events'])expect(Number((await query(`SELECT count(*) n FROM ${table} WHERE command_id=$1`,[command]))[0].n)).toBe(1);
  expect((await query('SELECT s.state set_state,l.state link_state,s.bound_command_id FROM attachment_sets s JOIN attachment_links l ON l.set_id=s.id WHERE s.id=$1',[set.id]))[0]).toMatchObject({set_state:'bound',link_state:'bound',bound_command_id:command});
 });
}

/** Publish a scanned original through the actual private ingress for a native case. */
export async function prepareNativeFieldFile(fixture:NativeFieldFixture,purposeKey:string,targetId='issue-order',expectedTargetVersion=3,foreign=false) {
 const bytes=Buffer.from('iVBORw0KGgoAAAANSUhEUgAAAAIAAAACCAYAAABytg0kAAAACXBIWXMAAAPoAAAD6AG1e1JrAAAAEUlEQVQImWOQaEj4D8IMMAYAQaQH3RNNL/AAAAAASUVORK5CYII=','base64');
 const create=await fixture.request('POST',fixture.base,{targetType:'field_service_visit',targetId,componentKind:'record',purposeKey,expectedTargetVersion},key(),foreign);expect(create.status).toBe(201);
 const reserve=await fixture.request('POST',`${fixture.base}/${create.json.data.id}/files`,{filename:'native-service-evidence.png',mediaType:'image/png',bytes:bytes.length},key(),foreign);expect(reserve.status).toBe(201);
 const upload=await fixture.request('PUT',reserve.json.data.uploadUrl,bytes,key(),foreign);expect(upload.status).toBe(202);fixture.startScan();
 let ready=false;for(let n=0;n<120;n++){const row=(await fixture.query('SELECT source_state FROM attachments WHERE id=$1',[upload.json.data.sourceId]))[0];if(row?.source_state==='available'){ready=true;break;}await sleep(500);}expect(ready).toBe(true);
 return {setId:create.json.data.id as string,linkId:reserve.json.data.attachmentLinkId as string,sourceId:upload.json.data.sourceId as string,bytes};
}

/** Observe a real competing writer before the native metadata snapshot. */
export async function verifyNativeCommittedWithdrawal(){
 await withNativeFieldFixture(async fixture=>{
  const file=await prepareNativeFieldFile(fixture,'technician_report'),row=(await fixture.query('SELECT object_key FROM attachments WHERE id=$1',[file.sourceId]))[0],directory=join(fixture.store,createHash('sha256').update(row.object_key).digest('hex')),manifest=JSON.parse(await readFile(join(directory,'current.json'),'utf8')),physical=join(directory,manifest.version),held=join(directory,`${manifest.version}.test-held`);
  // If content I/O is reached, the actual store must fail because the original
  // is temporarily unavailable. Restore it even when the assertion fails.
  await rename(physical,held);
  try{
   const response=await fixture.committedWriter("UPDATE attachment_links SET state='withdrawn',withdrawn_by=created_by,withdrawn_at=1,withdrawal_reason='Actual committed original evidence withdrawal' WHERE id=$1 AND state='draft'",[file.linkId],()=>fixture.request('GET',`/api/v1/holdings/acme/field-service/attachment-links/${file.linkId}/content?variant=original`));
   expect(response.status).toBe(404);expect(response.json.error.code).toBe('ATTACHMENT_LINK_NOT_FOUND');
   expect(await fixture.query('SELECT outcome,reason FROM attachment_access_events WHERE link_id=$1',[file.linkId])).toEqual([{outcome:'denied',reason:'ATTACHMENT_LINK_NOT_FOUND'}]);
   // Denial audit is best effort: late audit/authority faults must roll its
   // transaction back without changing the original 404 or fabricating audit.
   for(const fault of ['delete','alter','iam'] as const){const fn=`field_deny_fault_${key().replaceAll('-','')}`,effect=fault==='delete'?"DELETE FROM attachment_access_events WHERE id=NEW.id;":fault==='alter'?"UPDATE attachment_access_events SET reason='altered' WHERE id=NEW.id;":"UPDATE assignments SET revoked_at=1,version=version+1 WHERE id='field-fixture-a';";
    await fixture.query(`CREATE SEQUENCE ${fn}_attempt`);await fixture.query(`CREATE FUNCTION ${fn}() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN PERFORM nextval('${fn}_attempt');${effect} RETURN NEW;END $$`);await fixture.query(`CREATE CONSTRAINT TRIGGER ${fn} AFTER INSERT ON attachment_access_events DEFERRABLE INITIALLY DEFERRED FOR EACH ROW WHEN(NEW.link_id='${file.linkId}' AND NEW.outcome='denied') EXECUTE FUNCTION ${fn}()`);
    try{const denied=await fixture.request('GET',`/api/v1/holdings/acme/field-service/attachment-links/${file.linkId}/content?variant=original`);expect((await fixture.query(`SELECT is_called FROM ${fn}_attempt`))[0].is_called).toBe(true);expect(denied.status).toBe(404);expect(denied.json.error.code).toBe('ATTACHMENT_LINK_NOT_FOUND');}finally{await fixture.query(`DROP TRIGGER ${fn} ON attachment_access_events`);await fixture.query(`DROP FUNCTION ${fn}()`);await fixture.query(`DROP SEQUENCE ${fn}_attempt`);}
    expect(await fixture.query('SELECT outcome,reason FROM attachment_access_events WHERE link_id=$1',[file.linkId])).toEqual([{outcome:'denied',reason:'ATTACHMENT_LINK_NOT_FOUND'}]);expect((await fixture.query("SELECT revoked_at,version FROM assignments WHERE id='field-fixture-a'"))[0]).toEqual({revoked_at:null,version:3});
   }
  }finally{await rename(held,physical);}
 });
}

/** Exercise the native post-I/O source/authority fence with actual deferred faults. */
export async function verifyNativeLateContentFaults(){
 await withNativeFieldFixture(async fixture=>{
  const {query,request}=fixture,file=await prepareNativeFieldFile(fixture,'technician_report'),path=`/api/v1/holdings/acme/field-service/attachment-links/${file.linkId}/content?variant=original`;
  await query('UPDATE attachments SET legal_hold=1 WHERE id=$1',[file.sourceId]);const first=await request('GET',path);expect(first.status).toBe(200);expect(first.raw).toEqual(file.bytes);
  for(const fault of ['customer','iam'] as const){const fn=`field_content_fault_${key().replaceAll('-','')}`,effect=fault==='customer'?"UPDATE finance_parties SET status='blocked' WHERE id='customer';":"UPDATE assignments SET revoked_at=1,version=version+1 WHERE id='field-fixture-a';";
   await query(`CREATE SEQUENCE ${fn}_attempt`);await query(`CREATE FUNCTION ${fn}() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN PERFORM nextval('${fn}_attempt');${effect} RETURN NEW;END $$`);await query(`CREATE CONSTRAINT TRIGGER ${fn} AFTER INSERT ON attachment_access_events DEFERRABLE INITIALLY DEFERRED FOR EACH ROW WHEN(NEW.link_id='${file.linkId}' AND NEW.outcome='allowed') EXECUTE FUNCTION ${fn}()`);
   try{const result=await request('GET',path);expect((await query(`SELECT is_called FROM ${fn}_attempt`))[0].is_called).toBe(true);expect(result.status).toBe(409);expect(result.json.error.code).toBe(fault==='customer'?'FIELD_SERVICE_CUSTOMERS_CHANGED':'CONFLICT');}finally{await query(`DROP TRIGGER ${fn} ON attachment_access_events`);await query(`DROP FUNCTION ${fn}()`);await query(`DROP SEQUENCE ${fn}_attempt`);}
   expect((await query("SELECT status FROM finance_parties WHERE id='customer'"))[0].status).toBe('active');expect((await query("SELECT revoked_at,version FROM assignments WHERE id='field-fixture-a'"))[0]).toEqual({revoked_at:null,version:3});expect(Number((await query("SELECT count(*) n FROM attachment_access_events WHERE link_id=$1 AND outcome='allowed'",[file.linkId]))[0].n)).toBe(1);
  }
  const retry=await request('GET',path);expect(retry.status).toBe(200);expect(retry.raw).toEqual(file.bytes);expect((await query('SELECT legal_hold,revoked_at FROM attachments WHERE id=$1',[file.sourceId]))[0]).toEqual({legal_hold:1,revoked_at:null});
  await query("UPDATE assignments SET revoked_at=1,version=version+1 WHERE id='field-fixture-a'");const revoked=await request('GET',path);expect(revoked.status).toBe(409);expect(revoked.json.error.code).toBe('WORK_ROLE_UNAVAILABLE');expect(Number((await query("SELECT count(*) n FROM attachment_access_events WHERE link_id=$1 AND outcome='allowed'",[file.linkId]))[0].n)).toBe(2);
 });
}

/** Prove the real native record bind's immutable Field and Support projection. */
export async function verifyNativeBindProjection(){
 await withNativeFieldFixture(async fixture=>{
  const {query,request}=fixture,file=await prepareNativeFieldFile(fixture,'technician_report','reserve-order',1),command=key(),path=`/api/v1/holdings/acme/attachment-sets/${file.setId}/bind`;
  const result=await request('POST',path,{expectedTargetVersion:1},command);expect(result.status).toBe(200);expect((await request('POST',path,{expectedTargetVersion:1},command)).json).toEqual(result.json);
  expect((await query('SELECT s.state set_state,l.state link_state FROM attachment_sets s JOIN attachment_links l ON l.set_id=s.id WHERE s.id=$1',[file.setId]))[0]).toEqual({set_state:'bound',link_state:'bound'});
  expect((await query("SELECT version,status FROM field_service_orders WHERE id='reserve-order'"))[0]).toEqual({version:2,status:'scheduled'});
  expect((await query("SELECT version,action,actor_id,status,details_json->>'attachmentCommandId' attachment_command FROM field_service_order_events WHERE id=$1",[`service-file-event-${command}`]))[0]).toEqual({version:2,action:'attach-evidence',actor_id:'fixture-a',status:'scheduled',attachment_command:command});
  expect((await query("SELECT service_order_version,version,status FROM support_tickets WHERE id='ticket-reserve-order'"))[0]).toEqual({service_order_version:2,version:2,status:'open'});
  expect((await query("SELECT action,actor_id FROM support_ticket_events WHERE ticket_id='ticket-reserve-order' AND version=2"))[0]).toEqual({action:'service-attach-evidence',actor_id:'fixture-a'});
  expect(Number((await query('SELECT count(*) n FROM command_receipts WHERE command_id=$1',[command]))[0].n)).toBe(1);
  const content=await request('GET',`/api/v1/holdings/acme/field-service/attachment-links/${file.linkId}/content?variant=original`);expect(content.status).toBe(200);expect(content.raw).toEqual(file.bytes);
 });
}
