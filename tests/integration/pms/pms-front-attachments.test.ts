import {afterAll,afterEach,beforeAll,beforeEach,expect,it,vi} from 'vitest';
import {packageGrantStore,readCompanyPackageGrants} from '@erpx/server-commercial/public/package-grants';
import {applyMigrations,env} from "../../support/runtime-env";
import {seedExpenseCompany} from "../../../fixtures/expenses/company";
import {pmsCommand} from "../../../packages/server-pms/src/public/pms";
import {frontPmsAttachmentRequest} from "@erpx/server-pms/public/pms/front-attachments";
import {attachmentContent,scanAttachmentNow} from "@erpx/server-files/public/attachments/service";
import {seedAttachmentSet} from '../../support/helpers/attachment-v2';
import type {FileStore} from '@erpx/server-runtime/public/runtime/types';
import {attachmentApiBase} from "@erpx/feature-files/lib/attachment-client";
vi.mock("@erpx/server-runtime/adapters/runtime",async()=>{const {env}=await import("../../support/runtime-env");return {holdingResources:async()=>({write:env.H001_DB,files:env.H001_FILES}),runtimeEnv:async()=>env};});
// This adapter/security test uses a scanner double; real ClamAV has separate acceptance.
vi.mock("@erpx/server-files/public/attachments/scanner",()=>({attachmentScannerReadiness:async()=>({enabled:true,ready:true,engine:'Scanner double',definitionUpdatedAt:Date.now(),error:null}),scanAttachmentBytes:async()=>({clean:true,signature:null,engine:'Scanner double',definitionUpdatedAt:Date.now()})}));
const db=env.H001_DB;
let fixture:Awaited<ReturnType<typeof seedExpenseCompany>>,propertyId:string,taskId:string;
beforeAll(async()=>{await applyMigrations(db,env.STAFF_TEST_MIGRATIONS);process.env.ERPX_ATTACHMENTS_V2='true';});
afterAll(()=>{delete process.env.ERPX_ATTACHMENTS_V2;});
afterEach(()=>vi.unstubAllEnvs());
async function hotelRows(source:Awaited<ReturnType<typeof seedExpenseCompany>>){
 const propertyId=(await pmsCommand(db,source.owner,{operation:'property.create',input:{name:'Front file fixture',branchId:source.branch,businessDate:'2026-09-01'}},crypto.randomUUID())).id;
 const typeId=crypto.randomUUID(),roomId=crypto.randomUUID();
 await db.batch([
  db.prepare("UPDATE pms_properties SET state='open' WHERE id=?").bind(propertyId),
  db.prepare("INSERT INTO pms_records(id,property_id,kind,data_json,version,updated_at) VALUES(?,?,'room_type',?,1,1)").bind(typeId,propertyId,JSON.stringify({name:'Standard',capacity:2,amenities:''})),
  db.prepare("INSERT INTO pms_records(id,property_id,kind,data_json,version,updated_at) VALUES(?,?,'room',?,1,1)").bind(roomId,propertyId,JSON.stringify({number:'101',typeId,building:'A',floor:'1',active:true,cleanliness:'dirty',cleaningCycle:1,dnd:false,observedOccupied:false})),
 ]);
 const taskId=(await pmsCommand(db,source.owner,{operation:'task.create',input:{roomId,type:'arrival',date:'2026-09-01'},propertyId,expectedVersion:1},crypto.randomUUID())).id;
 return {propertyId,taskId};
}
beforeEach(async()=>{fixture=await seedExpenseCompany(db,env.H001_FILES);({propertyId,taskId}=await hotelRows(fixture));});
function request(path:string,method='GET',body?:unknown){return new Request(`http://127.0.0.1:3104/api/v1/front/acme/hotel/pms/${path}`,{method,headers:{host:'127.0.0.1:3104',origin:'http://127.0.0.1:3104','Content-Type':'application/json','Idempotency-Key':crypto.randomUUID()},...(body===undefined?{}:{body:JSON.stringify(body)})});}
async function purchase(state:'active'|'draining',company=fixture.company){
 const current=(await readCompanyPackageGrants(db,company)).find(grant=>grant.packageId==='pms');
 return packageGrantStore(db,{authorize:async()=>({userId:'isolated-front-file-provider'}),obligations:async()=>{throw Error('No settlement withdrawal witness');}}).apply({expectedVersion:current?.version??0,idempotencyKey:crypto.randomUUID(),grant:{companyId:company,packageId:'pms',packageVersion:1,state,startsAt:0,endsAt:null,version:(current?.version??0)+1,reason:'Declared Front file purchase review',evidence:'test:pms-front-files'}});
}
it('keeps private room evidence on Front through intake, scan and the hotel action binding',async()=>{
 await purchase('active');vi.stubEnv('ERPX_PMS_PACKAGE_ENFORCEMENT','enforced');
 expect(attachmentApiBase('/front/acme/hotel/pms')).toBe('/api/v1/front/acme/hotel/pms');
 expect(()=>attachmentApiBase('/front/acme/hotel/pos')).toThrow();
 const input={targetType:'pms_task',targetId:taskId,componentKind:'action',componentRef:'task.complete',purposeKey:'room_photo',expectedTargetVersion:1};
 const set=(await (await frontPmsAttachmentRequest(request('attachment-sets','POST',input),fixture.owner,['attachment-sets'])).json()).data;
 await purchase('draining');
 const png=Buffer.from('iVBORw0KGgoAAAANSUhEUgAAAAIAAAACCAYAAABytg0kAAAACXBIWXMAAAPoAAAD6AG1e1JrAAAAEUlEQVQImWOQaEj4D8IMMAYAQaQH3RNNL/AAAAAASUVORK5CYII=','base64');
 const file=(await (await frontPmsAttachmentRequest(request(`attachment-sets/${set.id}/files`,'POST',{filename:'room.png',mediaType:'image/png',bytes:png.length}),fixture.owner,['attachment-sets',set.id,'files'])).json()).data;
 expect(file.uploadUrl).toMatch(/^\/api\/v1\/front\/acme\/hotel\/pms\/uploads\/[^/]+\/content$/);
 const upload=new Request(`http://127.0.0.1:3104${file.uploadUrl}`,{method:'PUT',headers:{host:'127.0.0.1:3104',origin:'http://127.0.0.1:3104','Content-Type':'image/png','Content-Length':String(png.length)},body:png});
 const saved=(await (await frontPmsAttachmentRequest(upload,fixture.owner,['uploads',file.uploadIntentId,'content'])).json()).data;
 await scanAttachmentNow(db,env.H001_FILES,saved.sourceId);
 const row=await db.prepare('SELECT data_json FROM pms_records WHERE id=?').bind(taskId).first<{data_json:string}>();
 await pmsCommand(db,fixture.owner,{operation:'task.complete',propertyId,expectedVersion:2,input:{id:taskId,checked:JSON.parse(row!.data_json).checklist,notes:'Fictional test evidence'},attachmentSetIds:[set.id]},crypto.randomUUID());
 const status=(await (await frontPmsAttachmentRequest(request(`attachment-links/${file.attachmentLinkId}/status`),fixture.owner,['attachment-links',file.attachmentLinkId,'status'])).json()).data;
 expect(status.linkState).toBe('bound');
 const content=await frontPmsAttachmentRequest(request(`attachment-links/${file.attachmentLinkId}/content`),fixture.owner,['attachment-links',file.attachmentLinkId,'content']);expect(content.headers.get('Cache-Control')).toBe('private, no-store');expect(Buffer.from(await content.arrayBuffer())).toEqual(png);
 const other=await seedExpenseCompany(db,env.H001_FILES);
 await expect(frontPmsAttachmentRequest(request(`attachment-links/${file.attachmentLinkId}/status`),other.owner,['attachment-links',file.attachmentLinkId,'status'])).rejects.toMatchObject({status:404});
 await expect(frontPmsAttachmentRequest(request(`attachment-links/${file.attachmentLinkId}/status`),{...fixture.owner,branchId:'other-branch'},['attachment-links',file.attachmentLinkId,'status'])).rejects.toMatchObject({status:403});
});
it('denies unpurchased hotel evidence through the actual Front file adapter',async()=>{
 vi.stubEnv('ERPX_PMS_PACKAGE_ENFORCEMENT','enforced');
 const input={targetType:'pms_task',targetId:taskId,componentKind:'action',componentRef:'task.complete',purposeKey:'room_photo',expectedTargetVersion:1};
 await expect(frontPmsAttachmentRequest(request('attachment-sets','POST',input),fixture.owner,['attachment-sets'])).rejects.toMatchObject({status:403,code:'PACKAGE_REQUIRED'});
 expect(await db.prepare('SELECT count(*) n FROM attachment_sets WHERE target_id=?').bind(taskId).first('n')).toBe(0);
});
it('rejects non-hotel targets, publication and administrative actions on Front',async()=>{
 await expect(frontPmsAttachmentRequest(request('attachment-sets','POST',{targetType:'payroll_run',targetId:taskId,componentKind:'record',purposeKey:'room_photo',expectedTargetVersion:1}),fixture.owner,['attachment-sets'])).rejects.toThrow();
 await expect(frontPmsAttachmentRequest(request('media-publications','POST',{}),fixture.owner,['media-publications'])).rejects.toMatchObject({status:404});
 await expect(frontPmsAttachmentRequest(request('attachment-sets','POST',{targetType:'pms_task',targetId:taskId,componentKind:'publication',purposeKey:'room_photo',expectedTargetVersion:1}),fixture.owner,['attachment-sets'])).rejects.toThrow();
});

it.each(['disabled','enforced'] as const)('buffers private Hotel bytes and repeats actual property record route and account guards after deferred audits (%s)',async mode=>{
 await purchase('active');vi.stubEnv('ERPX_PMS_PACKAGE_ENFORCEMENT',mode);
 const evidence=await seedAttachmentSet(db,fixture.owner,{targetType:'pms_task',targetId:taskId,componentKind:'action',componentRef:'task.complete',purposeKey:'room_photo',expectedTargetVersion:1,mediaType:'image/png'}),objectKey=await db.prepare('SELECT object_key FROM attachments WHERE id=?').bind(evidence.sourceId).first<string>('object_key');if(!objectKey)throw Error('ACTUAL_HOTEL_FILE_SOURCE_REQUIRED');
 const bytes=new Uint8Array([137,80,78,71,13,10,26,10]);await env.H001_FILES.put(objectKey,bytes);
 let consumed=0;const files:FileStore={put:(...args)=>env.H001_FILES.put(...args),get:async(...args)=>{const value=await env.H001_FILES.get(...args);return value?{...value,arrayBuffer:async()=>{consumed++;return value.arrayBuffer();}}:null;},head:(...args)=>env.H001_FILES.head(...args),delete:(...args)=>env.H001_FILES.delete(...args),list:(...args)=>env.H001_FILES.list(...args)};
 for(const kind of ['record','property','iam','route'] as const){
  const fault=`hotel_read_${crypto.randomUUID().replaceAll('-','')}`,effect=kind==='record'?`UPDATE pms_records SET version=version+1 WHERE id='${taskId}'`:kind==='property'?`UPDATE pms_properties SET name='Late hotel evidence owner' WHERE id='${propertyId}'`:kind==='iam'?`UPDATE assignments SET revoked_at=1,version=version+1 WHERE id='${fixture.owner.assignmentId}'`:`UPDATE attachment_sets SET component_ref='changed-after-file-audit' WHERE id='${evidence.setId}'`;
  await db.exec(`CREATE FUNCTION ${fault}() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN ${effect};RETURN NEW;END $$;CREATE CONSTRAINT TRIGGER ${fault} AFTER INSERT ON attachment_access_events DEFERRABLE INITIALLY DEFERRED FOR EACH ROW WHEN(NEW.link_id='${evidence.linkId}' AND NEW.outcome='allowed') EXECUTE FUNCTION ${fault}()`);
  try{await expect(attachmentContent(db,files,fixture.owner,evidence.linkId,'original')).rejects.toMatchObject({status:409});}finally{await db.exec(`DROP TRIGGER ${fault} ON attachment_access_events;DROP FUNCTION ${fault}()`);}
  expect(Number(await db.prepare("SELECT count(*) n FROM attachment_access_events WHERE link_id=? AND outcome='allowed'").bind(evidence.linkId).first('n'))).toBe(0);
 }
 expect(consumed).toBe(4);expect(new Uint8Array(await(await attachmentContent(db,files,fixture.owner,evidence.linkId,'original')).object.arrayBuffer())).toEqual(bytes);expect(consumed).toBe(5);
 expect(await db.prepare('SELECT revoked_at,version FROM assignments WHERE id=?').bind(fixture.owner.assignmentId).first()).toEqual({revoked_at:null,version:1});
 await expect(db.transaction(async tx=>{await tx.prepare('SELECT 1').first();return attachmentContent(tx,files,fixture.owner,evidence.linkId,'original');})).rejects.toMatchObject({code:'HOSPITALITY_FILE_READ_TRANSACTION_REQUIRED'});expect(consumed).toBe(5);
});

it('keeps the actual Hotel denial audit while a true late account change rolls the denial and source effects back',async()=>{
 const evidence=await seedAttachmentSet(db,fixture.owner,{targetType:'pms_task',targetId:taskId,componentKind:'action',purposeKey:'room_photo',expectedTargetVersion:1,mediaType:'image/png'}),fault=`hotel_denied_${crypto.randomUUID().replaceAll('-','')}`;
 vi.stubEnv('ERPX_PMS_PACKAGE_ENFORCEMENT','disabled');
 await db.exec(`CREATE FUNCTION ${fault}() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN UPDATE assignments SET revoked_at=1,version=version+1 WHERE id='${fixture.claimant.assignmentId}';RETURN NEW;END $$;CREATE CONSTRAINT TRIGGER ${fault} AFTER INSERT ON attachment_access_events DEFERRABLE INITIALLY DEFERRED FOR EACH ROW WHEN(NEW.link_id='${evidence.linkId}' AND NEW.outcome='denied') EXECUTE FUNCTION ${fault}()`);
 try{await expect(attachmentContent(db,env.H001_FILES,fixture.claimant,evidence.linkId,'original')).rejects.toMatchObject({status:409});}finally{await db.exec(`DROP TRIGGER ${fault} ON attachment_access_events;DROP FUNCTION ${fault}()`);}
 expect(Number(await db.prepare("SELECT count(*) n FROM attachment_access_events WHERE link_id=? AND outcome='denied'").bind(evidence.linkId).first('n'))).toBe(0);
 await expect(attachmentContent(db,env.H001_FILES,fixture.claimant,evidence.linkId,'original')).rejects.toMatchObject({code:'ATTACHMENT_LINK_NOT_FOUND'});
 expect(Number(await db.prepare("SELECT count(*) n FROM attachment_access_events WHERE link_id=? AND outcome='denied'").bind(evidence.linkId).first('n'))).toBe(1);
 await db.prepare('UPDATE assignments SET revoked_at=1,version=version+1 WHERE id=?').bind(fixture.claimant.assignmentId).run();
 await expect(attachmentContent(db,env.H001_FILES,fixture.claimant,evidence.linkId,'original')).rejects.toMatchObject({code:'ATTACHMENT_LINK_NOT_FOUND'});
 expect(Number(await db.prepare("SELECT count(*) n FROM attachment_access_events WHERE link_id=? AND outcome='denied'").bind(evidence.linkId).first('n'))).toBe(2);
});

it.each(['disabled','enforced'] as const)('preserves original Hotel quarantine and missing-object denial audit without committing late source changes (%s)',async mode=>{
 await purchase('active');vi.stubEnv('ERPX_PMS_PACKAGE_ENFORCEMENT',mode);
 for(const unavailable of [{state:'quarantined',status:423,code:'ATTACHMENT_NOT_AVAILABLE'},{state:'available',status:503,code:'FILE_UNAVAILABLE'}] as const){
  const evidence=await seedAttachmentSet(db,fixture.owner,{targetType:'pms_task',targetId:taskId,componentKind:'action',componentRef:`unavailable:${unavailable.state}`,purposeKey:'room_photo',expectedTargetVersion:1,mediaType:'image/png',sourceState:unavailable.state}),objectKey=await db.prepare('SELECT object_key FROM attachments WHERE id=?').bind(evidence.sourceId).first<string>('object_key');if(!objectKey)throw Error('ACTUAL_HOTEL_OBJECT_REQUIRED');await env.H001_FILES.delete(objectKey);
  const fault=`hotel_unavailable_${crypto.randomUUID().replaceAll('-','')}`,count=()=>db.prepare("SELECT count(*)::int n FROM attachment_access_events WHERE link_id=? AND outcome='denied' AND reason=?").bind(evidence.linkId,unavailable.code).first<number>('n');
  await db.exec(`CREATE FUNCTION ${fault}() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN UPDATE pms_records SET version=version+1 WHERE id='${taskId}';RETURN NEW;END $$;CREATE CONSTRAINT TRIGGER ${fault} AFTER INSERT ON attachment_access_events DEFERRABLE INITIALLY DEFERRED FOR EACH ROW WHEN(NEW.link_id='${evidence.linkId}' AND NEW.outcome='denied') EXECUTE FUNCTION ${fault}()`);
  try{await expect(attachmentContent(db,env.H001_FILES,fixture.owner,evidence.linkId,'original')).rejects.toMatchObject({code:'HOSPITALITY_FILE_READ_CHANGED'});}finally{await db.exec(`DROP TRIGGER ${fault} ON attachment_access_events;DROP FUNCTION ${fault}()`);}
  expect(await count()).toBe(0);expect(await db.prepare('SELECT version FROM pms_records WHERE id=?').bind(taskId).first('version')).toBe(1);
  await expect(attachmentContent(db,env.H001_FILES,fixture.owner,evidence.linkId,'original')).rejects.toMatchObject({status:unavailable.status,code:unavailable.code});expect(await count()).toBe(1);
  expect(await db.prepare("SELECT count(*)::int n FROM attachment_access_events WHERE link_id=? AND outcome='allowed'").bind(evidence.linkId).first('n')).toBe(0);
 }
});

it('waits for an actual fenced Hotel source writer before its first snapshot and reads two Companies without hidden transaction retries',async()=>{
 const {measureReads}=await import('@erpx/server-runtime/public/runtime/read-metrics'),{committedSourceVisibility}=await import('../../support/helpers/source-writer-visibility'),{companyPurchaseReadLocks}=await import('@erpx/server-commercial/public/purchase-command-boundary');
 const other=await seedExpenseCompany(db,env.H001_FILES),second=await hotelRows(other);await purchase('active');await purchase('active',other.company);vi.stubEnv('ERPX_PMS_PACKAGE_ENFORCEMENT','enforced');
 const sources=[{actor:fixture.owner,propertyId,taskId},{actor:other.owner,...second}],files:{actor:typeof fixture.owner;propertyId:string;taskId:string;linkId:string}[]=[];
 for(const source of sources){const evidence=await seedAttachmentSet(db,source.actor,{targetType:'pms_task',targetId:source.taskId,componentKind:'action',purposeKey:'room_photo',expectedTargetVersion:1,mediaType:'image/png'}),objectKey=await db.prepare('SELECT object_key FROM attachments WHERE id=?').bind(evidence.sourceId).first<string>('object_key');if(!objectKey)throw Error('ACTUAL_HOTEL_FILE_REQUIRED');await env.H001_FILES.put(objectKey,'Retained private room evidence');files.push({...source,linkId:evidence.linkId});}
 const fault=`hotel_visible_${crypto.randomUUID().replaceAll('-','')}`;
 await db.exec(`CREATE FUNCTION ${fault}() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN IF (SELECT name FROM pms_properties WHERE id='${propertyId}')<>'Committed hotel source before bytes' THEN RAISE EXCEPTION 'STALE_HOTEL_SOURCE_SNAPSHOT';END IF;RETURN NEW;END $$;CREATE CONSTRAINT TRIGGER ${fault} AFTER INSERT ON attachment_access_events DEFERRABLE INITIALLY DEFERRED FOR EACH ROW WHEN(NEW.link_id='${files[0].linkId}' AND NEW.outcome='allowed') EXECUTE FUNCTION ${fault}()`);
 try{const observed=await committedSourceVisibility(db,async tx=>{await tx.batch(companyPurchaseReadLocks(tx,fixture.company));await tx.prepare("UPDATE pms_properties SET name='Committed hotel source before bytes',version=version+1 WHERE id=?").bind(propertyId).run();},()=>measureReads(()=>attachmentContent(db,env.H001_FILES,fixture.owner,files[0].linkId,'original')));expect(await observed.result.object.text()).toBe('Retained private room evidence');expect(observed.metrics.transactionRetries??0).toBe(0);expect(observed.metrics.deadlockFailures??0).toBe(0);expect(observed.metrics.serializationFailures??0).toBe(0);}finally{await db.exec(`DROP TRIGGER ${fault} ON attachment_access_events;DROP FUNCTION ${fault}()`);}
 const parallel=`hotel_parallel_${crypto.randomUUID().replaceAll('-','')}`;
 await db.exec(`CREATE FUNCTION ${parallel}() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN PERFORM pg_sleep(0.1);UPDATE pms_records SET version=version WHERE id=CASE WHEN NEW.link_id='${files[0].linkId}' THEN '${taskId}' ELSE '${second.taskId}' END;RETURN NEW;END $$;CREATE CONSTRAINT TRIGGER ${parallel} AFTER INSERT ON attachment_access_events DEFERRABLE INITIALLY DEFERRED FOR EACH ROW WHEN(NEW.link_id IN ('${files[0].linkId}','${files[1].linkId}') AND NEW.outcome='allowed') EXECUTE FUNCTION ${parallel}()`);
 try{const observed=await measureReads(()=>Promise.all(files.map(file=>attachmentContent(db,env.H001_FILES,file.actor,file.linkId,'original'))));expect(observed.result).toHaveLength(2);for(const result of observed.result)expect(await result.object.text()).toBe('Retained private room evidence');expect(observed.metrics.transactionRetries??0).toBe(0);expect(observed.metrics.deadlockFailures??0).toBe(0);expect(observed.metrics.serializationFailures??0).toBe(0);}finally{await db.exec(`DROP TRIGGER ${parallel} ON attachment_access_events;DROP FUNCTION ${parallel}()`);}
});
