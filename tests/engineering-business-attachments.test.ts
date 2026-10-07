import {afterAll,beforeAll,describe,expect,it,vi} from 'vitest';
import {createHash} from 'node:crypto';
import {applyMigrations,env} from './runtime-env';
import {expenseDb as db,expenseFixture,key} from './helpers/expense-fixture';
import {businessFileOwners,businessFileTarget} from './helpers/business-file-targets';
import {attachmentBindingState} from './helpers/attachment-v2';
import ownerMatrix from '../docs/testing/engineering-attachment-owner-cases.json';
import type {WorkContext} from '../src/server/authorization';
import type {AttachmentComponentKind} from '../src/server/attachments/types';
import {attachmentPolicy} from '../src/server/attachments/policy';
import {attachmentContent,attachmentLinkStatus,bindRecordAttachmentSet,completeAttachmentUpload,createAttachmentSet,drainAttachmentScans,listAttachmentSets,prepareAttachmentBinding,reserveAttachmentFile} from '../src/server/attachments/service';
import {loadIamAuthority} from '../src/server/iam-store';

vi.mock('../src/server/runtime',()=>({holdingResources:async()=>({write:env.H001_DB,files:env.H001_FILES}),runtimeEnv:async()=>env}));
// The clean scanner is a declared fixture. Upload bytes, sniffing, sanitized
// preview creation, quota, storage, owner policy and transactions are real.
// Root verifies ClamAV with the compiled scanner separately on this source.
vi.mock('../src/server/modules/files/public/attachments/scanner',()=>({
 attachmentScannerReadiness:async()=>({enabled:true,ready:true,engine:'Declared clean scanner fixture',definitionUpdatedAt:Date.now(),error:null}),
 scanAttachmentBytes:async()=>({clean:true,signature:null,engine:'Declared clean scanner fixture',definitionUpdatedAt:Date.now()}),
}));
const tuples=ownerMatrix.filter(row=>(businessFileOwners as readonly string[]).includes(row.targetType));
const png=Buffer.from('iVBORw0KGgoAAAANSUhEUgAAAAIAAAACCAYAAABytg0kAAAACXBIWXMAAAPoAAAD6AG1e1JrAAAAEUlEQVQImWOQaEj4D8IMMAYAQaQH3RNNL/AAAAAASUVORK5CYII=','base64');
const checksum=createHash('sha256').update(png).digest('hex'),body=png.buffer.slice(png.byteOffset,png.byteOffset+png.byteLength);
const previousGate=process.env.ERPX_ATTACHMENTS_V2;
let foreignActor:WorkContext;
beforeAll(async()=>{await applyMigrations(db,env.STAFF_TEST_MIGRATIONS);process.env.ERPX_ATTACHMENTS_V2='true';foreignActor=(await expenseFixture()).owner;foreignActor.iam=await loadIamAuthority(db,foreignActor.companyId,foreignActor.userId,foreignActor.role);});
afterAll(()=>{if(previousGate===undefined)delete process.env.ERPX_ATTACHMENTS_V2;else process.env.ERPX_ATTACHMENTS_V2=previousGate;});

describe('Business File Sources owner intake',()=>{
 it('rechecks selected grant expiry, suspended principal, live modules and published role permissions while retaining a narrower request snapshot',async()=>{
  const policy=attachmentPolicy('asset','nameplate_photo','record'),owner=await businessFileTarget(db,await expenseFixture(),policy),actor=owner.actor;
  actor.iam=await loadIamAuthority(db,actor.companyId,actor.userId,actor.role);
  const input={targetType:'asset',targetId:owner.targetId,componentKind:'record' as const,purposeKey:'nameplate_photo',expectedTargetVersion:1};
  await createAttachmentSet(db,actor,input,key());
  await db.prepare("UPDATE assignments SET modules_json='[]',version=version+1 WHERE id=?").bind(actor.assignmentId).run();
  await expect(createAttachmentSet(db,actor,input,key())).rejects.toMatchObject({status:403,code:'ATTACHMENT_FORBIDDEN'});expect(actor.modules).toContain('assets');
  await db.prepare("UPDATE assignments SET modules_json=?,valid_until=? WHERE id=?").bind(JSON.stringify(actor.modules),Date.now()-1,actor.assignmentId).run();
  await expect(listAttachmentSets(db,actor,{targetType:'asset',targetId:owner.targetId})).rejects.toMatchObject({status:404,code:'ATTACHMENT_TARGET_NOT_FOUND'});
  await db.prepare('UPDATE assignments SET valid_until=? WHERE id=?').bind(Date.now()+86_400_000,actor.assignmentId).run();
  await db.prepare("INSERT INTO iam_members(company_id,user_id,principal_id,status,login_policy) VALUES(?,?,?,'suspended','local') ON CONFLICT(company_id,user_id) DO UPDATE SET status='suspended',version=iam_members.version+1").bind(actor.companyId,actor.userId,`local:${actor.userId}`).run();
  await expect(listAttachmentSets(db,actor,{targetType:'asset',targetId:owner.targetId})).rejects.toMatchObject({status:404,code:'ATTACHMENT_TARGET_NOT_FOUND'});

  const customOwner=await businessFileTarget(db,await expenseFixture(),policy),role=`custom_business_files_${key().replaceAll('-','')}`,permissions=['records.read','records.write','module.assets.read'];
  await db.prepare("INSERT INTO iam_roles(company_id,id,name,built_in,published_revision) VALUES(?,?,?,false,1)").bind(customOwner.actor.companyId,role,'Fictional file owner reviewer').run();
  async function revision(version:number,allowed:string[]){await db.prepare("INSERT INTO iam_role_revisions(company_id,role_id,revision,permissions_json,grantable_json,scopes_json,created_by,created_at) VALUES(?,?,?,?::jsonb,'[]'::jsonb,'[\"holding\"]'::jsonb,?,?)").bind(customOwner.actor.companyId,role,version,JSON.stringify(allowed),customOwner.actor.userId,Date.now()).run();await db.prepare('UPDATE iam_roles SET published_revision=? WHERE company_id=? AND id=?').bind(version,customOwner.actor.companyId,role).run();}
  await revision(1,permissions);await db.prepare('UPDATE assignments SET role=? WHERE id=?').bind(role,customOwner.actor.assignmentId).run();
  const customActor:WorkContext={...customOwner.actor,role,iam:undefined};customActor.iam=await loadIamAuthority(db,customActor.companyId,customActor.userId,role);
  const customInput={...input,targetId:customOwner.targetId},set=await createAttachmentSet(db,customActor,customInput,key()),reserved=await reserveAttachmentFile(db,customActor,set.id,{filename:'live-role-source.png',mediaType:'image/png',bytes:png.byteLength},key());
  await revision(2,['module.assets.read']);
  await expect(createAttachmentSet(db,customActor,customInput,key())).rejects.toMatchObject({status:403,code:'ATTACHMENT_FORBIDDEN'});await expect(listAttachmentSets(db,customActor,{targetType:'asset',targetId:customOwner.targetId})).rejects.toMatchObject({status:403,code:'ATTACHMENT_FORBIDDEN'});await expect(completeAttachmentUpload(db,env.H001_FILES,customActor,reserved.uploadIntentId,body)).rejects.toMatchObject({status:403,code:'ATTACHMENT_FORBIDDEN'});
  const quarantine=String(await db.prepare('SELECT quarantine_key FROM upload_intents WHERE id=?').bind(reserved.uploadIntentId).first('quarantine_key'));expect(await env.H001_FILES.get(quarantine)).toBeNull();expect(customActor.iam.permissions).toContain('records.write');
  await revision(3,permissions);
  const narrowed={...customActor,iam:{...customActor.iam,permissions:customActor.iam.permissions.filter(permission=>permission!=='records.write')}};
  await expect(createAttachmentSet(db,narrowed,customInput,key())).rejects.toMatchObject({status:403,code:'ATTACHMENT_FORBIDDEN'});expect((await listAttachmentSets(db,narrowed,{targetType:'asset',targetId:customOwner.targetId})).map(row=>row.id)).toEqual([set.id]);expect(narrowed.iam.permissions).not.toContain('records.write');
 });
 it.each(tuples.map(row=>[row.id,row] as const))('proves authorized intake scan binding reload and revocation for %s',async(_id,tuple)=>{
  const componentKind=tuple.componentKind as AttachmentComponentKind,policy=attachmentPolicy(tuple.targetType,tuple.purposeKey,componentKind),owner=await businessFileTarget(db,await expenseFixture(),policy,componentKind),actor=owner.actor;
  actor.iam=await loadIamAuthority(db,actor.companyId,actor.userId,actor.role);
  const componentRef=owner.componentRef??undefined;
  const input={targetType:tuple.targetType,targetId:owner.targetId,componentKind,purposeKey:tuple.purposeKey,componentRef,expectedTargetVersion:owner.version};
  await expect(createAttachmentSet(db,actor,{...input,expectedTargetVersion:owner.version+1},key())).rejects.toMatchObject({status:409,code:'ATTACHMENT_TARGET_CHANGED'});
  const setKey=key(),set=await createAttachmentSet(db,actor,input,setKey);expect(await createAttachmentSet(db,actor,input,setKey)).toMatchObject({id:set.id,state:'draft'});
  const fileInput={filename:'synthetic-business-owner-evidence.png',mediaType:'image/png',bytes:png.byteLength,caption:'Declared fictional native owner and clean scanner fixtures.'},reserveKey=key(),reserved=await reserveAttachmentFile(db,actor,set.id,fileInput,reserveKey);
  expect(await reserveAttachmentFile(db,actor,set.id,fileInput,reserveKey)).toEqual(reserved);
  await expect(reserveAttachmentFile(db,actor,set.id,{...fileInput,caption:'Changed file payload'},reserveKey)).rejects.toMatchObject({code:'IDEMPOTENCY_CONFLICT'});
  const staleSet=await createAttachmentSet(db,actor,{...input,componentRef:`${componentRef??'record'}:stale-intake`},key()),staleReservation=await reserveAttachmentFile(db,actor,staleSet.id,fileInput,key());
  const revokedSet=await createAttachmentSet(db,actor,{...input,componentRef:`${componentRef??'record'}:revocation-intake`},key()),revokedReservation=await reserveAttachmentFile(db,actor,revokedSet.id,fileInput,key());
  const uploaded=await completeAttachmentUpload(db,env.H001_FILES,actor,reserved.uploadIntentId,body);
  expect(uploaded).toMatchObject({state:'quarantined',status:'scan_pending'});expect((await completeAttachmentUpload(db,env.H001_FILES,actor,reserved.uploadIntentId,body)).sourceId).toBe(uploaded.sourceId);
  await expect(attachmentContent(db,env.H001_FILES,actor,reserved.attachmentLinkId,'original')).rejects.toMatchObject({status:423,code:'ATTACHMENT_NOT_AVAILABLE'});
  expect(await drainAttachmentScans(db,env.H001_FILES,uploaded.sourceId)).toBe(true);
  expect(await attachmentLinkStatus(db,actor,reserved.attachmentLinkId)).toMatchObject({sourceState:'available',linkState:'draft',bindable:true,previewAvailable:true,scan:{lastResult:'clean',lastError:null}});
  expect(await db.prepare('SELECT company_id,owner_type,owner_id,purpose,checksum_sha256,bytes,detected_media_type,source_state FROM attachments WHERE id=?').bind(uploaded.sourceId).first()).toMatchObject({company_id:actor.companyId,owner_type:tuple.targetType,owner_id:owner.targetId,purpose:tuple.purposeKey,checksum_sha256:checksum,bytes:png.byteLength,detected_media_type:'image/png',source_state:'available'});
  expect(await db.prepare('SELECT status,checksum_sha256,actual_bytes FROM upload_intents WHERE id=?').bind(reserved.uploadIntentId).first()).toEqual({status:'ready',checksum_sha256:checksum,actual_bytes:png.byteLength});
  const command=key(),bindingType=owner.bindingType??null;
  const bind=()=>componentKind==='record'?bindRecordAttachmentSet(db,actor,set.id,owner.version,command):owner.bindAction?owner.bindAction(set.id,command):Promise.reject(new Error(`Native owner attachment command missing for ${tuple.id}`));
  await expect(prepareAttachmentBinding(db,actor,{attachmentSetIds:[set.id],bindingContract:{targetType:tuple.targetType,bindingType,contexts:[{purposeKey:tuple.purposeKey,componentKind,componentRef:componentRef??null}]},targetId:owner.targetId,expectedTargetVersion:owner.version+1,commandId:key(),bindingId:owner.targetId})).rejects.toMatchObject({status:409,code:'ATTACHMENT_SET_STALE'});
  if(componentKind!=='record')await expect(bindRecordAttachmentSet(db,actor,set.id,owner.version,key())).rejects.toMatchObject({code:'ACTION_EVIDENCE_DOMAIN_BIND_REQUIRED'});
  const result=await bind();expect(await bind()).toEqual(result);if(owner.assertNative)await owner.assertNative();
  expect(await db.prepare(`SELECT version FROM ${owner.table} WHERE id=?`).bind(owner.targetId).first('version')).toBe(owner.version+1);
  expect(await attachmentBindingState(db,set.id)).toMatchObject({set_state:'bound',link_state:'bound',binding_type:bindingType,bound_command_id:command});
  expect(await completeAttachmentUpload(db,env.H001_FILES,actor,reserved.uploadIntentId,body)).toMatchObject({sourceId:uploaded.sourceId,state:'available'});
  await expect(completeAttachmentUpload(db,env.H001_FILES,actor,staleReservation.uploadIntentId,body)).rejects.toMatchObject(tuple.purposeKey==='direct_receipt_evidence'?{status:403,code:'ATTACHMENT_FORBIDDEN'}:{status:409,code:'ATTACHMENT_TARGET_CHANGED'});
  expect(await db.prepare('SELECT status,actual_bytes FROM upload_intents WHERE id=?').bind(staleReservation.uploadIntentId).first()).toEqual({status:'pending',actual_bytes:null});
  expect(await env.H001_FILES.get(String(await db.prepare('SELECT quarantine_key FROM upload_intents WHERE id=?').bind(staleReservation.uploadIntentId).first('quarantine_key')))).toBeNull();
  const reloaded=await listAttachmentSets(db,actor,{targetType:tuple.targetType,targetId:owner.targetId,purposeKey:tuple.purposeKey,componentKind});expect(reloaded.filter(row=>row.state==='bound')).toHaveLength(1);expect(reloaded.find(row=>row.id===set.id)).toMatchObject({state:'bound',expectedTargetVersion:owner.version,links:[expect.objectContaining({id:reserved.attachmentLinkId,sourceId:uploaded.sourceId,linkState:'bound',sourceState:'available',bytes:png.byteLength})]});
  const original=await attachmentContent(db,env.H001_FILES,actor,reserved.attachmentLinkId,'original'),preview=await attachmentContent(db,env.H001_FILES,actor,reserved.attachmentLinkId,'preview');expect(Buffer.from(await original.object.arrayBuffer())).toEqual(png);expect(preview.mediaType).toBe('image/webp');expect((await preview.object.arrayBuffer()).byteLength).toBeGreaterThan(0);
  const ledger=(await db.prepare('SELECT variant,object_key,bytes,state FROM attachment_storage_ledger WHERE attachment_id=? ORDER BY variant').bind(uploaded.sourceId).all<{variant:string;object_key:string;bytes:number;state:string}>()).results;expect(ledger.map(row=>row.variant)).toEqual(['original','preview']);expect(ledger.every(row=>row.state==='stored')).toBe(true);for(const row of ledger){const file=await env.H001_FILES.get(row.object_key);expect(file).not.toBeNull();expect((await file!.arrayBuffer()).byteLength).toBe(Number(row.bytes));}
  expect(await db.prepare("SELECT COALESCE(sum(bytes),0) bytes FROM attachment_storage_ledger WHERE company_id=? AND state='stored'").bind(actor.companyId).first('bytes')).toBe(ledger.reduce((sum,row)=>sum+Number(row.bytes),0));
  expect(await db.prepare("SELECT COALESCE(sum(declared_bytes),0) bytes FROM upload_intents WHERE company_id=? AND status='pending'").bind(actor.companyId).first('bytes')).toBe(2*png.byteLength);
  expect(await listAttachmentSets(db,foreignActor,{targetType:tuple.targetType,targetId:owner.targetId})).toEqual([]);await expect(attachmentContent(db,env.H001_FILES,foreignActor,reserved.attachmentLinkId,'original')).rejects.toMatchObject({status:404,code:'ATTACHMENT_LINK_NOT_FOUND'});await expect(reserveAttachmentFile(db,foreignActor,set.id,fileInput,key())).rejects.toMatchObject({status:404,code:'ATTACHMENT_SET_NOT_FOUND'});
  await db.prepare('UPDATE assignments SET revoked_at=?,version=version+1 WHERE id=?').bind(Date.now(),actor.assignmentId).run();
  const revoked={status:404,code:'ATTACHMENT_TARGET_NOT_FOUND'};
  await expect(listAttachmentSets(db,actor,{targetType:tuple.targetType,targetId:owner.targetId})).rejects.toMatchObject(revoked);await expect(attachmentContent(db,env.H001_FILES,actor,reserved.attachmentLinkId,'original')).rejects.toMatchObject(revoked);await expect(createAttachmentSet(db,actor,input,key())).rejects.toMatchObject(revoked);if(componentKind==='record')await expect(bind()).rejects.toMatchObject(revoked);
  await expect(completeAttachmentUpload(db,env.H001_FILES,actor,reserved.uploadIntentId,body)).rejects.toMatchObject(revoked);await expect(completeAttachmentUpload(db,env.H001_FILES,actor,revokedReservation.uploadIntentId,body)).rejects.toMatchObject(revoked);
  expect(await db.prepare('SELECT status,actual_bytes FROM upload_intents WHERE id=?').bind(revokedReservation.uploadIntentId).first()).toEqual({status:'pending',actual_bytes:null});expect(await env.H001_FILES.get(String(await db.prepare('SELECT quarantine_key FROM upload_intents WHERE id=?').bind(revokedReservation.uploadIntentId).first('quarantine_key')))).toBeNull();expect(await db.prepare('SELECT COUNT(*) n FROM attachments WHERE company_id=?').bind(actor.companyId).first('n')).toBe(1);
  expect(await db.prepare('SELECT COUNT(*) n FROM audit_events WHERE command_id=?').bind(command).first('n')).toBe(1);expect(await db.prepare('SELECT COUNT(*) n FROM outbox_events WHERE command_id=?').bind(command).first('n')).toBe(1);expect(await attachmentBindingState(db,set.id)).toMatchObject({set_state:'bound',link_state:'bound',bound_command_id:command});
 });
});
