import {seedExpenseCompany} from '../../../fixtures/expenses/company';
import type {PostgresDatabase} from '@erpx/server-runtime/adapters/runtime/postgres';
import {holdingResources} from '@erpx/server-runtime/public/runtime';
import {afterAll,afterEach,beforeAll,describe,expect,it,vi} from 'vitest';
import {createHash} from 'node:crypto';
import {LocalFileStore} from '@erpx/server-runtime/adapters/runtime/filesystem';
import {applyMigrations,env} from "../../support/runtime-env";
import {expenseDb as db,expenseFixture,key} from "../../support/helpers/expense-fixture";
import {businessFileOwners,businessFileTarget} from "../../support/helpers/business-file-targets";
import {attachmentBindingState} from "../../support/helpers/attachment-v2";
import {proveNativeFieldFiles,withNativeFieldFixture,prepareNativeFieldFile,verifyNativeCommittedWithdrawal,verifyNativeLateContentFaults} from "../../support/files/field-files-native-parity";
import ownerMatrix from "../../catalogue/engineering-attachment-owner-cases.json";
import type {WorkContext} from "@erpx/server-identity_access/public/authorization";
import type {AttachmentComponentKind} from "@erpx/server-files/public/attachments/types";
import {attachmentPolicy} from "@erpx/server-files/public/attachments/policy";
import {attachmentContent,attachmentLinkStatus,bindRecordAttachmentSet,completeAttachmentUpload,createAttachmentSet,drainAttachmentScans,listAttachmentSets,prepareAttachmentBinding,reserveAttachmentFile} from "@erpx/server-files/public/attachments/service";
import {loadIamAuthority} from "@erpx/server-identity_access/public/iam-store";
import {packageGrantStore,readCompanyPackageGrants} from "@erpx/server-commercial/public/package-grants";
import {attachmentDestructionEligibility} from "@erpx/server-files/public/attachments/service";

vi.mock("@erpx/server-runtime/adapters/runtime",()=>({holdingResources:vi.fn(async()=>({write:env.H001_DB,read:env.H001_DB,files:env.H001_FILES})),runtimeEnv:async()=>env}));
// The clean scanner is a declared fixture. Upload bytes, sniffing, sanitized
// preview creation, quota, storage, owner policy and transactions are real.
// Root verifies ClamAV with the compiled scanner separately on this source.
vi.mock("@erpx/server-files/public/attachments/scanner",()=>({
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
afterEach(()=>vi.unstubAllEnvs());
async function commerceGrant(company:string,packageId:string,state:'active'|'draining'='active'){
 const store=packageGrantStore(db,{authorize:async(_tx,current)=>{if(current!==company)throw new Error('FICTIONAL_PROVIDER_SCOPE');return {userId:'fictional-commerce-files-provider'};},obligations:async()=>{throw new Error('NO_INACTIVE_FILE_CUTOVER');}}),previous=(await readCompanyPackageGrants(db,company)).find(row=>row.packageId===packageId);
 return store.apply({expectedVersion:previous?.version??0,idempotencyKey:key(),grant:{companyId:company,packageId,packageVersion:1,state,startsAt:0,endsAt:null,version:(previous?.version??0)+1,reason:'Isolated Commerce file source fixture',evidence:'test:commerce-files-owner-boundary'}});
}

describe('Business File Sources owner intake',()=>{
 it.each(['stock_count','inventory_stock_count'] as const)('Physical Count Files preserve original action binding, history bytes and Legal holds for %s',async(targetType)=>{
  const policy=attachmentPolicy(targetType,'count_evidence','action'),owner=await businessFileTarget(db,await expenseFixture(),policy,'action'),actor=owner.actor;actor.iam=await loadIamAuthority(db,actor.companyId,actor.userId,actor.role);
  const input={targetType,targetId:owner.targetId,purposeKey:'count_evidence',componentKind:'action' as const,componentRef:owner.componentRef??undefined,expectedTargetVersion:owner.version};
  vi.stubEnv('ERPX_COMMERCE_PACKAGE_ENFORCEMENT','enforced');await expect(createAttachmentSet(db,actor,input,key())).rejects.toMatchObject({code:'PACKAGE_REQUIRED'});
  await commerceGrant(actor.companyId,'supply');
  const set=await createAttachmentSet(db,actor,input,key()),reserved=await reserveAttachmentFile(db,actor,set.id,{filename:'retained-count-source.png',mediaType:'image/png',bytes:png.byteLength},key()),uploaded=await completeAttachmentUpload(db,env.H001_FILES,actor,reserved.uploadIntentId,body);await drainAttachmentScans(db,env.H001_FILES,uploaded.sourceId);
  const command=key(),bound=await owner.bindAction!(set.id,command);if(owner.assertNative)await owner.assertNative();
  await commerceGrant(actor.companyId,'supply','draining');expect(await owner.bindAction!(set.id,command)).toEqual(bound);
  expect(await attachmentBindingState(db,set.id)).toMatchObject({set_state:'bound',link_state:'bound',bound_command_id:command});
  expect(Buffer.from(await (await attachmentContent(db,env.H001_FILES,actor,reserved.attachmentLinkId,'original')).object.arrayBuffer())).toEqual(png);
  await db.prepare('UPDATE attachments SET legal_hold=1 WHERE id=?').bind(uploaded.sourceId).run();expect(await attachmentDestructionEligibility(db,uploaded.sourceId,Date.now()+400*86_400_000)).toMatchObject({eligible:false,reason:'legal_hold'});
  await db.prepare('UPDATE assignments SET revoked_at=?,version=version+1 WHERE id=?').bind(Date.now(),actor.assignmentId).run();await expect(attachmentContent(db,env.H001_FILES,actor,reserved.attachmentLinkId,'original')).rejects.toMatchObject({status:404,code:'ATTACHMENT_TARGET_NOT_FOUND'});
  expect(await db.prepare('SELECT source_state,legal_hold FROM attachments WHERE id=?').bind(uploaded.sourceId).first()).toEqual({source_state:'available',legal_hold:1});
 });
 it.each(['source','authority'] as const)('Physical Count Files roll back late %s changes in their original atomic intake',async(change)=>{
  const policy=attachmentPolicy('stock_count','count_evidence','action'),owner=await businessFileTarget(db,await expenseFixture(),policy,'action'),actor=owner.actor;actor.iam=await loadIamAuthority(db,actor.companyId,actor.userId,actor.role);
  vi.stubEnv('ERPX_COMMERCE_PACKAGE_ENFORCEMENT','enforced');await commerceGrant(actor.companyId,'supply');
  const command=key(),name=`count_files_${key().replaceAll('-','')}`,before=await db.prepare('SELECT reason,version FROM inventory_counts WHERE id=?').bind(owner.targetId).first(),assignment=await db.prepare('SELECT revoked_at,version FROM assignments WHERE id=?').bind(actor.assignmentId).first();
  const update=change==='source'?`UPDATE inventory_counts SET reason=reason||' changed' WHERE id='${owner.targetId}'`:`UPDATE assignments SET revoked_at=1,version=version+1 WHERE id='${actor.assignmentId}'`;
  await db.prepare(`CREATE FUNCTION ${name}() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN ${update}; RETURN NEW; END $$`).run();
  await db.prepare(`CREATE TRIGGER ${name} AFTER INSERT ON attachment_sets FOR EACH ROW WHEN (NEW.target_id='${owner.targetId}') EXECUTE FUNCTION ${name}()`).run();
  const input={targetType:'stock_count',targetId:owner.targetId,purposeKey:'count_evidence',componentKind:'action' as const,componentRef:owner.componentRef??undefined,expectedTargetVersion:owner.version};
  try{await expect(createAttachmentSet(db,actor,input,command)).rejects.toMatchObject({status:409,code:'COUNT_ATTACHMENT_CHANGED'});}finally{await db.prepare(`DROP TRIGGER ${name} ON attachment_sets`).run();await db.prepare(`DROP FUNCTION ${name}()`).run();}
  expect(await db.prepare('SELECT reason,version FROM inventory_counts WHERE id=?').bind(owner.targetId).first()).toEqual(before);expect(await db.prepare('SELECT revoked_at,version FROM assignments WHERE id=?').bind(actor.assignmentId).first()).toEqual(assignment);
  expect(await db.prepare('SELECT 1 FROM attachment_sets WHERE target_id=?').bind(owner.targetId).first()).toBeNull();
  for(const table of ['command_receipts','audit_events','outbox_events'])expect(await db.prepare(`SELECT 1 FROM ${table} WHERE command_id=?`).bind(command).first()).toBeNull();
  const created=await createAttachmentSet(db,actor,input,command);expect(await createAttachmentSet(db,actor,input,command)).toEqual(created);
 });
 it.each([
  ['commerce_order','customer_brief','record','sales'],
  ['commerce_fulfillment','return_condition','action','supply'],
  ['commerce_return','condition_photo','action','supply'],
 ] as const)('Commerce purchase Files retain exact bytes, original binding and Legal holds for %s',async(targetType,purposeKey,componentKind,packageId)=>{
  const policy=attachmentPolicy(targetType,purposeKey,componentKind),owner=await businessFileTarget(db,await expenseFixture(),policy,componentKind),actor=owner.actor;actor.iam=await loadIamAuthority(db,actor.companyId,actor.userId,actor.role);
  const input={targetType,targetId:owner.targetId,purposeKey,componentKind,componentRef:owner.componentRef??undefined,expectedTargetVersion:owner.version};
  vi.stubEnv('ERPX_COMMERCE_PACKAGE_ENFORCEMENT','enforced');await expect(createAttachmentSet(db,actor,input,key())).rejects.toMatchObject({code:'PACKAGE_REQUIRED'});
  await commerceGrant(actor.companyId,packageId);await commerceGrant(actor.companyId,packageId,'draining');const command=key(),set=await createAttachmentSet(db,actor,input,key()),reserved=await reserveAttachmentFile(db,actor,set.id,{filename:'retained-commerce-source.png',mediaType:'image/png',bytes:png.byteLength},key()),uploaded=await completeAttachmentUpload(db,env.H001_FILES,actor,reserved.uploadIntentId,body);await drainAttachmentScans(db,env.H001_FILES,uploaded.sourceId);
  const bind=()=>componentKind==='record'?bindRecordAttachmentSet(db,actor,set.id,owner.version,command):owner.bindAction!(set.id,command),bound=await bind();expect(await bind()).toEqual(bound);if(owner.assertNative)await owner.assertNative();
  expect(await attachmentBindingState(db,set.id)).toMatchObject({set_state:'bound',link_state:'bound',bound_command_id:command});const content=await attachmentContent(db,env.H001_FILES,actor,reserved.attachmentLinkId,'original');expect(Buffer.from(await content.object.arrayBuffer())).toEqual(png);expect(await db.prepare('SELECT checksum_sha256,bytes FROM attachments WHERE id=?').bind(uploaded.sourceId).first()).toEqual({checksum_sha256:checksum,bytes:png.byteLength});
  await db.prepare('UPDATE attachments SET legal_hold=1 WHERE id=?').bind(uploaded.sourceId).run();expect(await attachmentDestructionEligibility(db,uploaded.sourceId,Date.now()+400*86_400_000)).toMatchObject({eligible:false,reason:'legal_hold'});expect(Buffer.from(await (await attachmentContent(db,env.H001_FILES,actor,reserved.attachmentLinkId,'original')).object.arrayBuffer())).toEqual(png);
  const revoke=await db.prepare('SELECT version FROM assignments WHERE id=?').bind(actor.assignmentId).first('version');await db.prepare('UPDATE assignments SET revoked_at=?,version=version+1 WHERE id=?').bind(Date.now(),actor.assignmentId).run();await expect(attachmentContent(db,env.H001_FILES,actor,reserved.attachmentLinkId,'original')).rejects.toMatchObject({status:404,code:'ATTACHMENT_TARGET_NOT_FOUND'});expect(await db.prepare('SELECT source_state,legal_hold FROM attachments WHERE id=?').bind(uploaded.sourceId).first()).toEqual({source_state:'available',legal_hold:1});expect(revoke).toBeGreaterThan(0);
 });
 it.each(['source','authority'] as const)('Commerce purchase Files roll back late %s changes with record, binding, receipt, audit and outbox',async(change)=>{
  const policy=attachmentPolicy('commerce_order','customer_brief','record'),owner=await businessFileTarget(db,await expenseFixture(),policy),actor=owner.actor;actor.iam=await loadIamAuthority(db,actor.companyId,actor.userId,actor.role);vi.stubEnv('ERPX_COMMERCE_PACKAGE_ENFORCEMENT','enforced');await commerceGrant(actor.companyId,'sales');
  const set=await createAttachmentSet(db,actor,{targetType:'commerce_order',targetId:owner.targetId,purposeKey:'customer_brief',componentKind:'record',expectedTargetVersion:1},key()),reserved=await reserveAttachmentFile(db,actor,set.id,{filename:'atomic-commerce-source.png',mediaType:'image/png',bytes:png.byteLength},key()),uploaded=await completeAttachmentUpload(db,env.H001_FILES,actor,reserved.uploadIntentId,body);await drainAttachmentScans(db,env.H001_FILES,uploaded.sourceId);
  const command=key(),suffix=key().replaceAll('-',''),fn=`commerce_files_late_${suffix}`,trigger=`commerce_files_late_${suffix}`,original=await db.prepare('SELECT kind,version FROM commerce_orders WHERE id=?').bind(owner.targetId).first(),assignment=await db.prepare('SELECT revoked_at,version FROM assignments WHERE id=?').bind(actor.assignmentId).first();
  // This declared native trigger changes the owner after Files' binding effect,
  // before its final checks. Only the isolated fixture set can activate it.
  const update=change==='source'?`UPDATE commerce_orders SET kind='purchase' WHERE id='${owner.targetId}'`:`UPDATE assignments SET revoked_at=1,version=version+1 WHERE id='${actor.assignmentId}'`;
  await db.exec(`CREATE FUNCTION ${fn}() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN IF NEW.set_id='${set.id}' AND NEW.action='set.bound' THEN ${update}; END IF; RETURN NEW; END $$; CREATE TRIGGER ${trigger} AFTER INSERT ON attachment_events FOR EACH ROW EXECUTE FUNCTION ${fn}()`);
  try{await expect(bindRecordAttachmentSet(db,actor,set.id,1,command)).rejects.toMatchObject({status:409,code:change==='source'?'COMMERCE_ATTACHMENT_SOURCE_CHANGED':'COMMERCE_ATTACHMENT_AUTHORITY_CHANGED'});}finally{await db.exec(`DROP TRIGGER ${trigger} ON attachment_events; DROP FUNCTION ${fn}()`);}
  expect(await db.prepare('SELECT kind,version FROM commerce_orders WHERE id=?').bind(owner.targetId).first()).toEqual(original);expect(await db.prepare('SELECT revoked_at,version FROM assignments WHERE id=?').bind(actor.assignmentId).first()).toEqual(assignment);expect(await attachmentBindingState(db,set.id)).toMatchObject({set_state:'draft',link_state:'draft',bound_command_id:null});
  for(const table of ['command_receipts','audit_events','outbox_events'])expect(await db.prepare(`SELECT COUNT(*) n FROM ${table} WHERE command_id=?`).bind(command).first('n')).toBe(0);expect(Buffer.from(await (await attachmentContent(db,env.H001_FILES,actor,reserved.attachmentLinkId,'original')).object.arrayBuffer())).toEqual(png);
 });
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
  if(tuple.targetType==='field_service_visit'){await proveNativeFieldFiles(tuple.purposeKey,png);return;}
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
 }, 360_000);
});

// Original Field compatibility/enforcement variants now use new native sources;
// these unrelated TS environment flags cannot re-enable a retired Field writer.
it('binds fresh native Field evidence with exact Support projection, module denial and retained compatibility history',async()=>{
 let prepared:{db:PostgresDatabase;f:Awaited<ReturnType<typeof seedExpenseCompany>>;customer:string}|undefined;
 try{await withNativeFieldFixture(async native=>{
  if(!prepared)throw new Error('NATIVE_FIELD_FILES_COMPANY_REQUIRED');const {db,f,customer}=prepared,root='/api/v1/holdings/acme/field-service';
  const agreement=await native.requestAs<{id:string}>('owner','POST',root,{entity:'agreement',branchId:f.branch,departmentId:f.department,customerId:customer,title:'Actual Field evidence source',startDate:'2026-01-01',endDate:'2027-12-31',currency:'THB',visitRateMinor:2500,warrantyDiscountBps:0},key());expect(agreement.status,JSON.stringify(agreement.json)).toBe(200);
  const approved=await native.requestAs('reviewer','POST',root,{entity:'agreement',id:agreement.json.data.id,action:'activate',expectedVersion:1,reason:'Independent actual evidence source review'},key());expect(approved.status,JSON.stringify(approved.json)).toBe(200);
  const bound=[];
  for(const mode of ['disabled','enforced']as const)for(const compatibility of [false,true]){
   vi.stubEnv('ERPX_SERVICE_PACKAGE_ENFORCEMENT',mode);
   const created=await native.requestAs<{id:string;ticketId:string}>('owner','POST',root,{entity:'request',agreementId:agreement.json.data.id,agreementVersion:2,title:'Fresh original Field evidence consumer',issue:'Inspect original equipment',site:'Original customer reception',technicianId:f.claimant.userId,scheduledAt:Date.now()+60000},key());expect(created.status,JSON.stringify(created.json)).toBe(200);const {id:order,ticketId:ticket}=created.json.data;
   const file=await prepareNativeFieldFile(native,'technician_report',order,1,false,'technician');
   if(compatibility)await db.prepare("INSERT INTO attachment_events(id,company_id,set_id,actor_id,action,details_json,created_at)VALUES(?,?,?,?,'set.created',?,?)").bind(key(),f.company,file.setId,f.claimant.userId,JSON.stringify({compatibilityAdapter:true}),Date.now()).run();
   const snapshot=async()=>({order:await db.prepare('SELECT * FROM field_service_orders WHERE id=?').bind(order).first(),ticket:await db.prepare('SELECT * FROM support_tickets WHERE id=?').bind(ticket).first(),events:(await db.prepare('SELECT * FROM field_service_order_events WHERE order_id=? ORDER BY version').bind(order).all()).results,supportEvents:(await db.prepare('SELECT * FROM support_ticket_events WHERE ticket_id=? ORDER BY version').bind(ticket).all()).results});
   const before=await snapshot(),unauthorizedKey=key(),bindPath=`/api/v1/holdings/acme/attachment-sets/${file.setId}/bind`;
   const denied=await native.requestAs('restricted','POST',bindPath,{expectedTargetVersion:1},unauthorizedKey);expect(denied.status,JSON.stringify(denied.json)).toBe(403);expect(denied.json.error.code).toBe('MODULE_UNAVAILABLE');expect(await snapshot()).toEqual(before);expect(await attachmentBindingState(db,file.setId)).toMatchObject({set_state:'draft',link_state:'draft'});for(const table of ['command_receipts','audit_events','outbox_events'])expect(await db.prepare(`SELECT count(*)::int n FROM ${table} WHERE command_id=?`).bind(unauthorizedKey).first('n')).toBe(0);
   const command=key(),receipt=await native.requestAs('technician','POST',bindPath,{expectedTargetVersion:1},command);expect(receipt.status,JSON.stringify(receipt.json)).toBe(200);expect((await native.requestAs('technician','POST',bindPath,{expectedTargetVersion:1},command)).json).toEqual(receipt.json);
   expect(await attachmentBindingState(db,file.setId)).toMatchObject({set_state:'bound',link_state:'bound'});
   const detail=await native.requestAs<{canEditVisitFiles:boolean}>('technician','GET',`${root}/orders/${order}/detail`);expect(detail.status,JSON.stringify(detail.json)).toBe(200);expect(detail.json.data.canEditVisitFiles).toBe(true);
   expect(await db.prepare('SELECT version,status FROM field_service_orders WHERE id=?').bind(order).first()).toEqual({version:2,status:'scheduled'});
   expect(await db.prepare("SELECT version,action,actor_id,status,details_json->>'attachmentCommandId' attachment_command FROM field_service_order_events WHERE id=?").bind(`service-file-event-${command}`).first()).toEqual({version:2,action:'attach-evidence',actor_id:f.claimant.userId,status:'scheduled',attachment_command:command});
   expect(await db.prepare('SELECT service_order_version,version,status FROM support_tickets WHERE id=?').bind(ticket).first()).toEqual({service_order_version:2,version:2,status:'open'});expect(await db.prepare('SELECT action,actor_id FROM support_ticket_events WHERE ticket_id=? AND version=2').bind(ticket).first()).toEqual({action:'service-attach-evidence',actor_id:f.claimant.userId});expect(await db.prepare('SELECT count(*)::int n FROM command_receipts WHERE command_id=?').bind(command).first('n')).toBe(1);
   bound.push({order,file,bindPath,command,receipt});
  }
  // Declared fixture transition; normal Commercial withdrawal is qualified by
  // its existing native source/authority component, not by this SQL fact.
  await db.transaction(async tx=>{const before=await tx.prepare("SELECT jsonb_build_object('companyId',company_id,'packageId',package_id,'packageVersion',package_version,'state',state,'startsAt',starts_at,'endsAt',ends_at,'version',version,'reason',reason,'evidence',evidence) image FROM company_package_grants WHERE company_id=? AND package_id='service'").bind(f.company).first('image');await tx.prepare("UPDATE company_package_grants SET state='draining',version=version+1 WHERE company_id=? AND package_id='service'").bind(f.company).run();await tx.prepare("INSERT INTO company_package_grant_events(id,company_id,package_id,actor_id,request_hash,before_json,after_json,created_at)SELECT ?,company_id,package_id,?,'native-fresh-files-draining',?::jsonb,jsonb_build_object('companyId',company_id,'packageId',package_id,'packageVersion',package_version,'state',state,'startsAt',starts_at,'endsAt',ends_at,'version',version,'reason',reason,'evidence',evidence),? FROM company_package_grants WHERE company_id=? AND package_id='service'").bind(key(),f.owner.userId,typeof before==='string'?before:JSON.stringify(before),Date.now(),f.company).run();});
  for(const {order,file,bindPath,command,receipt}of bound){
   expect((await native.requestAs('technician','POST',bindPath,{expectedTargetVersion:1},command)).json).toEqual(receipt.json);await db.prepare('UPDATE attachments SET legal_hold=1 WHERE id=?').bind(file.sourceId).run();
   const cancelled=await native.requestAs('owner','POST',root,{id:order,action:'cancel',expectedVersion:2,reason:'Original customer cancelled the documented existing visit'},key());expect(cancelled.status,JSON.stringify(cancelled.json)).toBe(200);
   const detail=await native.requestAs<{canEditVisitFiles:boolean}>('technician','GET',`${root}/orders/${order}/detail`);expect(detail.status).toBe(200);expect(detail.json.data.canEditVisitFiles).toBe(false);
   const content=await native.requestAs('technician','GET',`${root}/attachment-links/${file.linkId}/content?variant=original`);expect(content.status,JSON.stringify(content.json)).toBe(200);expect(content.raw).toEqual(file.bytes);
   const history=await native.requestAs('technician','POST',native.base,{targetType:'field_service_visit',targetId:order,componentKind:'record',purposeKey:'technician_report',expectedTargetVersion:3});expect(history.status,JSON.stringify(history.json)).toBe(409);expect(history.json.error.code).toBe('SERVICE_ATTACHMENT_HISTORY_ONLY');expect((await native.requestAs('technician','POST',bindPath,{expectedTargetVersion:1},command)).json).toEqual(receipt.json);expect(await db.prepare('SELECT version,status FROM field_service_orders WHERE id=?').bind(order).first()).toEqual({version:3,status:'cancelled'});
  }
 },async db=>{
  vi.mocked(holdingResources).mockResolvedValue({write:db,read:db,files:env.H001_FILES});const f=await seedExpenseCompany(db,env.H001_FILES);const {saveMaster}=await import('@erpx/server-commerce/public/commerce');const customer=String((await saveMaster(f.owner,'parties',null,{kind:'customer',code:key(),name:'Fresh native Files customer'},key())).id);
  await db.exec(`INSERT INTO iam_role_revisions(company_id,role_id,revision,permissions_json,grantable_json,scopes_json,created_by,created_at)VALUES('*','custom_field_parts',2,'["records.read","records.write","records.review","module.field_service.read"]','[]','["holding","branch","department"]','synthetic-native-files',0);UPDATE iam_roles SET published_revision=2 WHERE company_id='*' AND id='custom_field_parts';INSERT INTO iam_roles(company_id,id,name,built_in,published_revision,status)VALUES('*','native_field_no_module','Original no-Field-module role',false,1,'active');INSERT INTO iam_role_revisions(company_id,role_id,revision,permissions_json,grantable_json,scopes_json,created_by,created_at)VALUES('*','native_field_no_module',1,'["records.read","records.write"]','[]','["holding"]','synthetic-native-files',0)`);
  const actors=[];for(const [label,actor]of [['owner',f.owner],['reviewer',f.tie],['technician',f.claimant],['restricted',f.claimant]]as const){const role=label==='restricted'?'native_field_no_module':'custom_field_parts',assignmentId=await f.grant(actor,role),modules=label==='restricted'?['finance']:['field_service'];await db.prepare('UPDATE assignments SET modules_json=? WHERE id=?').bind(JSON.stringify(modules),assignmentId).run();const version=Number(await db.prepare('SELECT version FROM assignments WHERE id=?').bind(assignmentId).first('version'));actors.push({label,userId:actor.userId,assignmentId,assignmentVersion:version,role,modules,branchId:null,departmentId:null});}
  prepared={db,f,customer};return{companyId:f.company,actors};
 },'service');}finally{vi.mocked(holdingResources).mockResolvedValue({write:env.H001_DB,read:env.H001_DB,files:env.H001_FILES});}
},180_000);

// Existing native Field assertions moved verbatim from the legacy-producer suite.
it("Service Files final source and selected authority fences roll original visit event case projection sensitive set and command effects back before exact retry",async()=>{
 await withNativeFieldFixture(async ({query,request,...fixture})=>{
  const native={query,request,...fixture},file=await prepareNativeFieldFile(native,'signed_job_sheet'),command=key(),fn=`service_file_final_${key().replaceAll('-','')}`,bindPath=`/api/v1/holdings/acme/attachment-sets/${file.setId}/bind`,before=(await query("SELECT * FROM field_service_orders WHERE id='issue-order'"))[0],caseBefore=(await query("SELECT * FROM support_tickets WHERE id='ticket-issue-order'"))[0],authorityBefore=(await query("SELECT * FROM assignments WHERE id='field-fixture-a'"))[0];
  for(const fault of ['source','authority'] as const){
   await query(`CREATE SEQUENCE ${fn}_attempt`);await query(`CREATE FUNCTION ${fn}() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN PERFORM nextval('${fn}_attempt');${fault==='source'?"UPDATE field_service_orders SET site=site||' changed late' WHERE id='issue-order';":"UPDATE assignments SET revoked_at=1,version=version+1 WHERE id='field-fixture-a';"} RETURN NEW;END $$`);await query(`CREATE CONSTRAINT TRIGGER ${fn} AFTER INSERT ON outbox_events DEFERRABLE INITIALLY DEFERRED FOR EACH ROW WHEN(NEW.command_id='${command}') EXECUTE FUNCTION ${fn}()`);
   try{
    const rejected=await request('POST',bindPath,{expectedTargetVersion:3},command);expect(rejected.status).toBe(409);expect(rejected.json.error.code).toBe(fault==='source'?'SERVICE_ATTACHMENT_CHANGED':'CONFLICT');
    expect((await query(`SELECT is_called FROM ${fn}_attempt`))[0].is_called).toBe(true);expect((await query("SELECT * FROM field_service_orders WHERE id='issue-order'"))[0]).toEqual(before);expect((await query("SELECT * FROM support_tickets WHERE id='ticket-issue-order'"))[0]).toEqual(caseBefore);expect((await query("SELECT * FROM assignments WHERE id='field-fixture-a'"))[0]).toEqual(authorityBefore);
    expect((await query('SELECT s.state set_state,l.state link_state FROM attachment_sets s JOIN attachment_links l ON l.set_id=s.id WHERE s.id=$1',[file.setId]))[0]).toMatchObject({set_state:'draft',link_state:'draft'});expect(await query('SELECT id FROM field_service_order_events WHERE id=$1',[`service-file-event-${command}`])).toEqual([]);for(const table of ['command_receipts','audit_events','outbox_events'])expect(await query(`SELECT 1 FROM ${table} WHERE command_id=$1`,[command])).toEqual([]);
   }finally{await query(`DROP TRIGGER ${fn} ON outbox_events`);await query(`DROP FUNCTION ${fn}()`);await query(`DROP SEQUENCE ${fn}_attempt`);}
  }
  const result=await request('POST',bindPath,{expectedTargetVersion:3},command);expect(result.status).toBe(200);expect((await request('POST',bindPath,{expectedTargetVersion:3},command)).json).toEqual(result.json);expect((await query("SELECT service_order_version,version FROM support_tickets WHERE id='ticket-issue-order'"))[0]).toEqual({service_order_version:4,version:2});
 });
},360_000);

it("Service visit Files bind in two actual Companies with zero hidden writer lease deadlocks and isolated source event and receipt identities",async()=>{
 await withNativeFieldFixture(async fixture=>{
  const {query,request}=fixture;
  // A second synthetic historical visit is seeded in the leased database; it is
  // not presented as a migrated Service create/dispatch business journey.
  await query(`BEGIN;
   INSERT INTO organization_nodes(id,kind,slug,display_name,parent_id,company_id,branch_id,status) VALUES('foreign-branch','branch','foreign-branch','Synthetic foreign branch','foreign','foreign','foreign-branch','active');
   INSERT INTO finance_parties SELECT (jsonb_populate_record(NULL::finance_parties,to_jsonb(p)||'{"id":"foreign-customer","company_id":"foreign","code":"FOREIGN"}'::jsonb)).* FROM finance_parties p WHERE id='customer';
   INSERT INTO field_service_agreements SELECT (jsonb_populate_record(NULL::field_service_agreements,to_jsonb(g)||'{"id":"foreign-agreement","company_id":"foreign","branch_id":"foreign-branch","customer_id":"foreign-customer","created_by":"fixture-b","reviewed_by":"fixture-a"}'::jsonb)).* FROM field_service_agreements g WHERE id='agreement';
   INSERT INTO field_service_agreement_events SELECT (jsonb_populate_record(NULL::field_service_agreement_events,to_jsonb(e)||'{"id":"foreign-agreement-review","agreement_id":"foreign-agreement","actor_id":"fixture-a"}'::jsonb)).* FROM field_service_agreement_events e WHERE id='agreement-review';
   INSERT INTO support_tickets SELECT (jsonb_populate_record(NULL::support_tickets,to_jsonb(t)||'{"id":"foreign-ticket","company_id":"foreign","branch_id":"foreign-branch","customer_id":"foreign-customer","service_agreement_id":"foreign-agreement","service_order_id":"foreign-order","owner_id":"fixture-b","created_by":"fixture-b"}'::jsonb)).* FROM support_tickets t WHERE id='ticket-reserve-order';
   INSERT INTO field_service_orders SELECT (jsonb_populate_record(NULL::field_service_orders,to_jsonb(o)||'{"id":"foreign-order","company_id":"foreign","branch_id":"foreign-branch","ticket_id":"foreign-ticket","agreement_id":"foreign-agreement","technician_id":"fixture-b","created_by":"fixture-b"}'::jsonb)).* FROM field_service_orders o WHERE id='reserve-order';
   INSERT INTO field_service_order_events SELECT (jsonb_populate_record(NULL::field_service_order_events,to_jsonb(e)||'{"id":"foreign-order-created","order_id":"foreign-order","actor_id":"fixture-b","technician_id":"fixture-b"}'::jsonb)).* FROM field_service_order_events e WHERE id='historical-reserve-order-1';
   INSERT INTO company_package_grants SELECT 'foreign',package_id,package_version,state,starts_at,ends_at,version,reason,evidence,updated_at FROM company_package_grants WHERE company_id='company' AND package_id IN ('service','service-parts-assets');
   INSERT INTO company_package_grant_events SELECT 'foreign-'||id,'foreign',package_id,actor_id,request_hash,before_json,after_json||jsonb_build_object('companyId','foreign'),created_at FROM company_package_grant_events WHERE company_id='company' AND package_id IN ('service','service-parts-assets');
   COMMIT`);
  const first=await prepareNativeFieldFile(fixture,'technician_report','reserve-order',1),second=await prepareNativeFieldFile(fixture,'technician_report','foreign-order',1,true),firstKey=key(),secondKey=key(),fn=`service_file_parallel_${key().replaceAll('-','')}`;
  await query(`CREATE SEQUENCE ${fn}_attempt`);await query(`CREATE FUNCTION ${fn}() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN PERFORM nextval('${fn}_attempt');PERFORM pg_sleep(0.1);RETURN NEW;END $$`);await query(`CREATE TRIGGER ${fn} BEFORE UPDATE ON field_service_orders FOR EACH ROW WHEN(NEW.id IN ('reserve-order','foreign-order') AND NEW.version=2 AND OLD.version=1) EXECUTE FUNCTION ${fn}()`);
  try{
   await new Promise(resolve=>setTimeout(resolve,1500));await query('SELECT pg_stat_clear_snapshot()');const deadlocksBefore=Number((await query('SELECT deadlocks FROM pg_stat_database WHERE datname=current_database()'))[0].deadlocks);
   const responses=await Promise.all([request('POST',`/api/v1/holdings/acme/attachment-sets/${first.setId}/bind`,{expectedTargetVersion:1},firstKey),request('POST',`/api/v1/holdings/acme/attachment-sets/${second.setId}/bind`,{expectedTargetVersion:1},secondKey,true)]);expect(responses.map(r=>r.status)).toEqual([200,200]);expect(Number((await query(`SELECT last_value FROM ${fn}_attempt`))[0].last_value)).toBe(2);await new Promise(resolve=>setTimeout(resolve,1500));await query('SELECT pg_stat_clear_snapshot()');expect(Number((await query('SELECT deadlocks FROM pg_stat_database WHERE datname=current_database()'))[0].deadlocks)).toBe(deadlocksBefore);
  }finally{await query(`DROP TRIGGER ${fn} ON field_service_orders`);await query(`DROP FUNCTION ${fn}()`);await query(`DROP SEQUENCE ${fn}_attempt`);}
  for(const [company,user,order,command] of [['company','fixture-a','reserve-order',firstKey],['foreign','fixture-b','foreign-order',secondKey]] as const){expect((await query('SELECT company_id,version FROM field_service_orders WHERE id=$1',[order]))[0]).toEqual({company_id:company,version:2});expect((await query('SELECT actor_id FROM field_service_order_events WHERE id=$1 AND order_id=$2',[`service-file-event-${command}`,order]))[0].actor_id).toBe(user);expect((await query("SELECT actor_id,authority_json::jsonb->>'companyId' company FROM command_receipts WHERE command_id=$1",[command]))[0]).toEqual({actor_id:user,company});}
 });
},360_000);

it('Service private byte read consumes lazy bytes inside its fresh actual source unit and rolls access audits back after late customer or selected IAM changes',async()=>{
 // The native contract commits initial authorization before private I/O, then
 // commits a fresh checked finish before delivery. This real HTTP check retains
 // original byte equality, legal hold, late customer/IAM rollback, no-byte
 // release, allowed-audit counts and live-role revocation observations.
 // The registered `files_reservation_postgres` owner test additionally proves
 // zero reads after initial authority denial and exactly one physical read when
 // a real 40001 retries finalization. That replaces the old inside-unit lazy
 // callback and transaction-required implementation assertions.
 await verifyNativeLateContentFaults();
},360_000);

it('Service private byte leases observe a committed original withdrawn-link writer before the first owned metadata snapshot without retrying or opening the blob',async()=>{
 // The real native ingress waits for the owned writer, returns the original
 // link-not-found denial, and records the correct denial audit. The registered
 // `files_reservation_postgres` owner test uses that same real source lock and
 // a counting real store to establish one initial attempt and no physical read.
 // Its forced 40001 finalization test separately proves I/O is never replayed.
 await verifyNativeCommittedWithdrawal();
},360_000);

// Original unpurchased private bytes are explicit retained fixture data. Fresh
// production/intake is never authorised before both required purchases exist.
it('keeps retained native Field Files behind actual Service and addon, current module and draining authority',async()=>{
 await withNativeFieldFixture(async native=>{
  const {query,request}=native,token=key(),set=`history-set-${token}`,source=`history-source-${token}`,link=`history-link-${token}`,upload=`history-upload-${token}`,object=`test/retained-native/${token}`,bytes=Buffer.from('Actual private Service technician evidence'),hash=createHash('sha256').update(bytes).digest('hex'),now=Date.now();
  await new LocalFileStore(native.store).put(object,bytes,{httpMetadata:{contentType:'application/pdf'}});
  await query("INSERT INTO upload_intents(id,owner_type,owner_id,purpose,original_name,media_type,declared_bytes,actual_bytes,checksum_sha256,staging_key,final_key,status,created_by,expires_at,created_at,updated_at,owner_version,company_id,detected_media_type)VALUES($1,'field_service_visit','reserve-order','technician_report','evidence.pdf','application/pdf',$2,$2,$3,$4,$5,'ready','fixture-a',$6,$7,$7,1,'company','application/pdf')",[upload,bytes.length,hash,`test/staging/${token}`,object,now+900000,now]);
  await query("INSERT INTO attachments(id,upload_id,owner_type,owner_id,purpose,object_key,revision,original_name,media_type,bytes,checksum_sha256,created_by,created_at,company_id,detected_media_type,source_state,scanned_at,scanner_definition_at,legal_hold)VALUES($1,$2,'field_service_visit','reserve-order','technician_report',$3,1,'evidence.pdf','application/pdf',$4,$5,'fixture-a',$6,'company','application/pdf','available',$6,$6,1)",[source,upload,object,bytes.length,hash,now]);
  await query("INSERT INTO attachment_sets(id,company_id,branch_id,department_id,target_type,target_id,component_kind,component_ref,purpose_key,expected_target_version,policy_version,classification,audience_json,required_count,max_files,state,created_by,created_at,updated_at)VALUES($1,'company',NULL,NULL,'field_service_visit','reserve-order','record',NULL,'technician_report',1,1,'internal','{}',0,10,'draft','fixture-a',$2,$2)",[set,now]);
  await query("INSERT INTO attachment_links(id,set_id,attachment_id,state,created_by,created_at,expires_at)VALUES($1,$2,$3,'draft','fixture-a',$4,$5)",[link,set,source,now,now+7*86400000]);
  const input={targetType:'field_service_visit',targetId:'reserve-order',componentKind:'record',purposeKey:'technician_report',expectedTargetVersion:1},content=`/api/v1/holdings/acme/field-service/attachment-links/${link}/content?variant=original`;
  expect((await query("SELECT count(*)::int n FROM company_package_grants WHERE company_id='company'"))[0].n).toBe(0);
  for(const [method,path,body]of [['POST',native.base,input],['GET',content,undefined]]as const){const result=await request(method,path,body);expect(result.status,JSON.stringify(result.json)).toBe(403);expect(result.json.error.code).toBe('PACKAGE_REQUIRED');}
  // Fixed synthetic bootstrap facts, not Commercial rollout acceptance.
  const purchase=async(packageId:'service'|'service-parts-assets')=>{
   await query(`BEGIN;INSERT INTO company_package_grants(company_id,package_id,package_version,state,starts_at,version,reason,evidence,updated_at)VALUES('company','${packageId}',1,'active',0,1,'Retained source prerequisite','Public synthetic fixture',${now});INSERT INTO company_package_grant_events(id,company_id,package_id,actor_id,request_hash,before_json,after_json,created_at)SELECT 'native-history-${packageId}',company_id,package_id,'fixture-a','native-history-purchase',NULL,jsonb_build_object('companyId',company_id,'packageId',package_id,'packageVersion',package_version,'state',state,'startsAt',starts_at,'endsAt',ends_at,'version',version,'reason',reason,'evidence',evidence),${now} FROM company_package_grants WHERE company_id='company' AND package_id='${packageId}';COMMIT`);
  };
  await purchase('service');const missingAddon=await request('GET',content);expect(missingAddon.status,JSON.stringify(missingAddon.json)).toBe(403);expect(missingAddon.json.error.code).toBe('PACKAGE_REQUIRED');
  await purchase('service-parts-assets');const allowed=await request('GET',content);expect(allowed.status,JSON.stringify(allowed.json)).toBe(200);expect(allowed.raw).toEqual(bytes);
  await query(`BEGIN;INSERT INTO iam_role_revisions(company_id,role_id,revision,permissions_json,grantable_json,scopes_json,created_by,created_at)VALUES('*','custom_field_parts',2,'["records.read","records.write"]','[]','["holding","branch","department"]','synthetic-history-source',0);UPDATE iam_roles SET published_revision=2 WHERE company_id='*' AND id='custom_field_parts';UPDATE assignments SET modules_json='["finance"]' WHERE id='field-fixture-a';COMMIT`);
  const noModule=await request('GET',content);expect(noModule.status,JSON.stringify(noModule.json)).toBe(403);expect(noModule.json.error.code).toBe('MODULE_UNAVAILABLE');
  await query(`BEGIN;UPDATE iam_roles SET published_revision=1 WHERE company_id='*' AND id='custom_field_parts';UPDATE assignments SET modules_json='["field_service"]' WHERE id='field-fixture-a';WITH old AS MATERIALIZED(SELECT company_id,package_id,jsonb_build_object('companyId',company_id,'packageId',package_id,'packageVersion',package_version,'state',state,'startsAt',starts_at,'endsAt',ends_at,'version',version,'reason',reason,'evidence',evidence) image FROM company_package_grants WHERE company_id='company' FOR UPDATE),changed AS(UPDATE company_package_grants g SET state='draining',version=g.version+1 FROM old o WHERE g.company_id=o.company_id AND g.package_id=o.package_id RETURNING g.*,o.image before_image)INSERT INTO company_package_grant_events(id,company_id,package_id,actor_id,request_hash,before_json,after_json,created_at)SELECT 'native-history-drain-'||package_id,company_id,package_id,'fixture-a','native-history-draining',before_image,jsonb_build_object('companyId',company_id,'packageId',package_id,'packageVersion',package_version,'state',state,'startsAt',starts_at,'endsAt',ends_at,'version',version,'reason',reason,'evidence',evidence),${now} FROM changed;COMMIT`);
  const draining=await request('POST',native.base,{...input,componentRef:'Existing visit settlement'});expect(draining.status,JSON.stringify(draining.json)).toBe(201);expect(draining.json.data).toMatchObject({targetType:'field_service_visit',targetId:'reserve-order'});
  await query("UPDATE assignments SET revoked_at=1,version=version+1 WHERE id='field-fixture-a'");const revoked=await request('GET',content);expect(revoked.status,JSON.stringify(revoked.json)).toBe(409);expect(revoked.json.error.code).toBe('WORK_ROLE_UNAVAILABLE');
 },undefined,'files-history');
},90_000);
