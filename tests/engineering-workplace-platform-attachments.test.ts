import {afterAll,beforeAll,describe,expect,it,vi} from 'vitest';
import {createHash} from 'node:crypto';
import sharp from 'sharp';
import {PDFDocument} from 'pdf-lib';
import {applyMigrations,env} from './runtime-env';
import {expenseDb as db,expenseFixture,key} from './helpers/expense-fixture';
import {workplaceAttachmentOwner,legalAttachmentOwner,controlAccount,type WorkplaceAttachmentTuple} from './helpers/engineering-workplace-platform-attachment-owners';
import ownerMatrix from '../docs/testing/engineering-attachment-owner-cases.json';
import type {WorkContext} from '../src/server/authorization';
import {loadIamAuthority} from '../src/server/iam-store';
import {attachmentBindingState} from './helpers/attachment-v2';
import {requiresSanitizedEvidenceAttestation} from '../src/server/attachments/finance-operations-policy';
import {attachmentContent,attachmentLinkStatus,attestAttachmentSetSanitized,bindRecordAttachmentSet,completeAttachmentUpload,createAttachmentSet,drainAttachmentScans,listAttachmentSets,prepareAttachmentBinding,reserveAttachmentFile,createAttachmentPublication,approveAttachmentPublication,publicAttachmentPublication} from '../src/server/attachments/service';
import {uploadDocument,documentExport} from '../src/server/legal/documents';
import {legalCommand,detail as legalDetail} from '../src/server/legal/service';
import {portalUpload,portalDetail} from '../src/server/legal/portal';

vi.mock('../src/server/runtime',()=>({holdingResources:async()=>({write:env.H001_DB,files:env.H001_FILES}),runtimeEnv:async()=>env}));
// Declared clean scanner fixture only. Bytes, signature detection, real preview
// rendering, private storage and all owner authorization/binding are native.
// The compiled release scanner is independently required by the release gate.
vi.mock('../src/server/attachments/scanner',()=>({
 attachmentScannerReadiness:async()=>({enabled:true,ready:true,engine:'Declared engineering scanner fixture',definitionUpdatedAt:Date.now(),error:null}),
 scanAttachmentBytes:async()=>({clean:true,signature:null,engine:'Declared engineering scanner fixture',definitionUpdatedAt:Date.now()}),
}));
const targetTypes=new Set(['holding_brand','branch_brand','company_storefront_brand','company_provisioning','company_release_acceptance','iam_migration','iam_review','organization_change','organization_node','people_provision_operation','customer_access_binding','legal_item','messaging_room','social_draft','social_post','support_incident','support_ticket']);
const tuples=ownerMatrix.filter(row=>targetTypes.has(row.targetType)) as WorkplaceAttachmentTuple[],previousGate=process.env.ERPX_ATTACHMENTS_V2;
let png:Buffer,checksum:string,foreignActor:WorkContext;
beforeAll(async()=>{
 await applyMigrations(db,env.STAFF_TEST_MIGRATIONS);await applyMigrations(env.CONTROL_DB,env.CONTROL_TEST_MIGRATIONS);process.env.ERPX_ATTACHMENTS_V2='true';
 png=await sharp({create:{width:64,height:64,channels:4,background:{r:47,g:105,b:245,alpha:.8}}}).png().toBuffer();checksum=createHash('sha256').update(png).digest('hex');foreignActor=(await expenseFixture(false)).owner;await controlAccount(foreignActor);
});
afterAll(()=>{if(previousGate===undefined)delete process.env.ERPX_ATTACHMENTS_V2;else process.env.ERPX_ATTACHMENTS_V2=previousGate;});

async function storedEvidence(sourceId:string,actor:WorkContext,targetType:string,targetId:string,purposeKey:string,expectedBytes:Uint8Array){
 const digest=createHash('sha256').update(expectedBytes).digest('hex'),source=await db.prepare('SELECT company_id,owner_type,owner_id,purpose,checksum_sha256,bytes,source_state,object_key FROM attachments WHERE id=?').bind(sourceId).first();
 expect(source).toMatchObject({company_id:actor.companyId,owner_type:targetType,owner_id:targetId,purpose:purposeKey,checksum_sha256:digest,bytes:expectedBytes.byteLength,source_state:'available'});
 const ledger=(await db.prepare("SELECT variant,object_key,bytes,state FROM attachment_storage_ledger WHERE attachment_id=? ORDER BY variant").bind(sourceId).all<{variant:string;object_key:string;bytes:number;state:string}>()).results;
 expect(ledger.some(row=>row.variant==='original'&&row.state==='stored')).toBe(true);
 for(const row of ledger){expect(row.state).toBe('stored');const object=await env.H001_FILES.get(row.object_key);expect(object).not.toBeNull();expect((await object!.arrayBuffer()).byteLength).toBe(Number(row.bytes));}
 expect(await db.prepare("SELECT count(*) n FROM attachment_scan_attempts WHERE attachment_id=? AND state='clean' AND result_code='OK'").bind(sourceId).first('n')).toBe(1);
 return source!;
}

describe('Workplace and Platform File Sources owner intake',()=>{
 it.each(tuples.filter(row=>row.targetType!=='legal_item').map(row=>[row.id,row] as const))('proves authorized intake scan binding reload and revocation for %s',async(_id,tuple)=>{
  const owner=await workplaceAttachmentOwner(tuple),actor=owner.actor;if(actor.role!=='external_customer')actor.iam=await loadIamAuthority(db,actor.companyId,actor.userId,actor.role,actor.authority?.sessionId);
  const input={targetType:tuple.targetType,targetId:owner.targetId,componentKind:tuple.componentKind,purposeKey:tuple.purposeKey,componentRef:owner.componentRef,expectedTargetVersion:owner.version},setKey=key(),fileInput={filename:'synthetic-reviewed-evidence.png',mediaType:'image/png',bytes:png.byteLength,caption:'Reviewed sanitized synthetic business artifact only.'};
  await expect(createAttachmentSet(db,actor,{...input,expectedTargetVersion:owner.version+1},key())).rejects.toMatchObject({code:'ATTACHMENT_TARGET_CHANGED'});
  const set=await createAttachmentSet(db,actor,input,setKey);expect(await createAttachmentSet(db,actor,input,setKey)).toMatchObject({id:set.id,state:'draft'});
  const reservationKey=key(),reserved=await reserveAttachmentFile(db,actor,set.id,fileInput,reservationKey);expect(await reserveAttachmentFile(db,actor,set.id,fileInput,reservationKey)).toEqual(reserved);
  await expect(reserveAttachmentFile(db,actor,set.id,{...fileInput,caption:'Different reviewed payload'},reservationKey)).rejects.toMatchObject({code:'IDEMPOTENCY_CONFLICT'});
  const extraSet=await createAttachmentSet(db,actor,{...input,componentRef:`${owner.componentRef??'record'}:revocation-intake`},key()),extra=await reserveAttachmentFile(db,actor,extraSet.id,fileInput,key()),body=Uint8Array.from(png).buffer;
  const uploaded=await completeAttachmentUpload(db,env.H001_FILES,actor,reserved.uploadIntentId,body);expect(uploaded).toMatchObject({state:'quarantined',status:'scan_pending'});expect((await completeAttachmentUpload(db,env.H001_FILES,actor,reserved.uploadIntentId,body)).sourceId).toBe(uploaded.sourceId);
  await expect(attachmentContent(db,env.H001_FILES,actor,reserved.attachmentLinkId,'original')).rejects.toMatchObject({code:'ATTACHMENT_NOT_AVAILABLE'});expect(await drainAttachmentScans(db,env.H001_FILES,uploaded.sourceId)).toBe(true);
  expect(await attachmentLinkStatus(db,actor,reserved.attachmentLinkId)).toMatchObject({sourceState:'available',linkState:'draft',bindable:true,previewAvailable:true,scan:{lastResult:'clean',lastError:null}});
  await storedEvidence(uploaded.sourceId,actor,tuple.targetType,owner.targetId,tuple.purposeKey,png);expect(await db.prepare('SELECT status,checksum_sha256,actual_bytes FROM upload_intents WHERE id=?').bind(reserved.uploadIntentId).first()).toEqual({status:'ready',checksum_sha256:checksum,actual_bytes:png.byteLength});
  const command=key(),bind=()=>owner.bindAction?owner.bindAction(set.id,command,uploaded.sourceId):bindRecordAttachmentSet(db,actor,set.id,owner.version,command);
  await expect(prepareAttachmentBinding(db,actor,{attachmentSetIds:[set.id],bindingContract:{targetType:tuple.targetType,bindingType:owner.bindingType,contexts:[{purposeKey:tuple.purposeKey,componentKind:tuple.componentKind,componentRef:owner.componentRef}]},targetId:owner.targetId,expectedTargetVersion:owner.version+1,commandId:key(),bindingId:owner.targetId})).rejects.toMatchObject({code:'ATTACHMENT_SET_STALE'});
  if(tuple.componentKind==='action'||tuple.componentKind==='message')await expect(bindRecordAttachmentSet(db,actor,set.id,owner.version,key())).rejects.toMatchObject({code:'ACTION_EVIDENCE_DOMAIN_BIND_REQUIRED'});
  if(requiresSanitizedEvidenceAttestation(tuple.targetType)){
   await expect(bind()).rejects.toMatchObject({code:'ATTACHMENT_SANITIZATION_ATTESTATION_REQUIRED'});
   const attestationKey=key(),attestation={confirmed:true,note:'I reviewed this synthetic artifact: it contains no passwords, private keys, secrets, configuration exports or database dumps.'};
   await attestAttachmentSetSanitized(db,actor,set.id,attestation,attestationKey);await attestAttachmentSetSanitized(db,actor,set.id,attestation,attestationKey);expect(await db.prepare("SELECT count(*) n FROM attachment_events WHERE set_id=? AND action='sanitization.attested'").bind(set.id).first('n')).toBe(1);
  }
  const rowId=tuple.targetType==='holding_brand'?actor.companyId:owner.targetId,ownerBefore=(await db.prepare(`SELECT * FROM ${owner.table} WHERE ${owner.idColumn??'id'}=?`).bind(rowId).first())!;
  const result=await bind();expect(await bind()).toEqual(result);expect(await attachmentBindingState(db,set.id)).toMatchObject({set_state:'bound',link_state:'bound',binding_type:owner.bindingType,bound_command_id:command});
  const after=(await db.prepare(`SELECT * FROM ${owner.table} WHERE ${owner.idColumn??'id'}=?`).bind(rowId).first())!;expect(Number(after[owner.versionColumn])).toBe(owner.version+(owner.versionIncrement??1));
  if(['iam_review','iam_migration','people_provision_operation'].includes(tuple.targetType))expect(after.status).toBe(ownerBefore.status);
  if(tuple.targetType==='company_release_acceptance'){expect(after.evidence_sha256).toBe(ownerBefore.evidence_sha256);expect(after.evidence_json).toBe(ownerBefore.evidence_json);expect(await db.prepare('SELECT count(*) n FROM company_release_gate_reviews WHERE release_id=?').bind(owner.targetId).first('n')).toBe(0);}
  if(tuple.targetType==='organization_change')expect(after.state).toBe('applied');if(tuple.targetType==='support_incident')expect(after.status).toBe('investigating');
  if(tuple.targetType==='messaging_room')expect(await db.prepare('SELECT body FROM messaging_messages WHERE id=?').bind(`message-${command}`).first('body')).toBe('Synthetic scanned evidence attached to this real message');
  if(tuple.targetType==='customer_access_binding')expect(await db.prepare('SELECT action,status FROM customer_portal_actions WHERE binding_id=?').bind(owner.targetId).first()).toEqual({action:'request_support',status:'recorded'});
  const reloaded=await listAttachmentSets(db,actor,{targetType:tuple.targetType,targetId:owner.targetId,purposeKey:tuple.purposeKey,componentKind:tuple.componentKind});expect(reloaded.filter(row=>row.state==='bound')).toHaveLength(1);expect(reloaded.find(row=>row.id===set.id)).toMatchObject({state:'bound',links:[expect.objectContaining({id:reserved.attachmentLinkId,sourceId:uploaded.sourceId,sourceState:'available',linkState:'bound'})]});
  const original=await attachmentContent(db,env.H001_FILES,actor,reserved.attachmentLinkId,'original'),preview=await attachmentContent(db,env.H001_FILES,actor,reserved.attachmentLinkId,'preview');expect(Buffer.from(await original.object.arrayBuffer())).toEqual(png);expect(preview.mediaType).toBe('image/webp');expect((await preview.object.arrayBuffer()).byteLength).toBeGreaterThan(0);
  if(tuple.componentKind==='publication'){
   const publication=await createAttachmentPublication(db,env.H001_FILES,actor,{sourceLinkId:reserved.attachmentLinkId,targetType:tuple.targetType,targetId:owner.targetId,altText:'Synthetic reviewed Company identity',ordinal:0},key());expect(publication!.state).toBe('draft');await expect(publicAttachmentPublication(db,env.H001_FILES,String(publication!.id))).rejects.toMatchObject({status:404});await expect(approveAttachmentPublication(db,actor,String(publication!.id),key())).rejects.toMatchObject({code:'INDEPENDENT_PUBLICATION_REVIEW'});
   const reviewer={...actor,userId:actor.userId+'-brand-reviewer',assignmentId:key()};await db.prepare("INSERT INTO assignments(id,user_id,role,modules_json,company_id,valid_from,valid_until,scope_kind) VALUES(?,?,'holding_owner','[]',?,0,9999999999999,'holding')").bind(reviewer.assignmentId,reviewer.userId,actor.companyId).run();reviewer.iam=await loadIamAuthority(db,reviewer.companyId,reviewer.userId,reviewer.role);await approveAttachmentPublication(db,reviewer,String(publication!.id),key());const publicFile=await publicAttachmentPublication(db,env.H001_FILES,String(publication!.id));expect((await publicFile.object.arrayBuffer()).byteLength).toBeGreaterThan(0);
  }
  expect(await listAttachmentSets(db,foreignActor,{targetType:tuple.targetType,targetId:owner.targetId})).toEqual([]);await expect(attachmentContent(db,env.H001_FILES,foreignActor,reserved.attachmentLinkId,'original')).rejects.toMatchObject({status:404});await expect(reserveAttachmentFile(db,foreignActor,set.id,fileInput,key())).rejects.toMatchObject({status:404});
  if(owner.revoke)await owner.revoke();else await db.prepare('UPDATE assignments SET revoked_at=?,version=version+1 WHERE id=?').bind(Date.now(),actor.assignmentId).run();
  await expect(listAttachmentSets(db,actor,{targetType:tuple.targetType,targetId:owner.targetId})).rejects.toMatchObject({status:404});await expect(attachmentContent(db,env.H001_FILES,actor,reserved.attachmentLinkId,'original')).rejects.toMatchObject({status:404});await expect(createAttachmentSet(db,actor,input,key())).rejects.toMatchObject({status:404});await expect(completeAttachmentUpload(db,env.H001_FILES,actor,reserved.uploadIntentId,body)).rejects.toMatchObject({status:404});await expect(completeAttachmentUpload(db,env.H001_FILES,actor,extra.uploadIntentId,body)).rejects.toMatchObject({status:404});
  expect(await db.prepare('SELECT status,actual_bytes FROM upload_intents WHERE id=?').bind(extra.uploadIntentId).first()).toEqual({status:'pending',actual_bytes:null});const quarantine=String(await db.prepare('SELECT quarantine_key FROM upload_intents WHERE id=?').bind(extra.uploadIntentId).first('quarantine_key'));expect(await env.H001_FILES.get(quarantine)).toBeNull();expect(await attachmentBindingState(db,set.id)).toMatchObject({set_state:'bound',link_state:'bound',bound_command_id:command});
 });

 it.each(tuples.filter(row=>row.targetType==='legal_item').map(row=>[row.id,row] as const))('proves authorized intake scan binding reload and revocation for %s',async(_id,tuple)=>{
  const owner=await legalAttachmentOwner(),actor=owner.actor,pdf=await PDFDocument.create();pdf.addPage([100,100]);const bytes=await pdf.save(),command=key();let version=1,sourceId:string,linkId:string,setId:string;
  if(tuple.componentKind==='record'){
   await expect(uploadDocument(db,env.H001_FILES,actor,owner.id,2,'synthetic-original.pdf','application/pdf',bytes,key())).rejects.toMatchObject({code:'LEGAL_VERSION_CONFLICT'});
   const uploaded=await uploadDocument(db,env.H001_FILES,actor,owner.id,1,'synthetic-original.pdf','application/pdf',bytes,command);expect(await uploadDocument(db,env.H001_FILES,actor,owner.id,1,'synthetic-original.pdf','application/pdf',bytes,command)).toEqual(uploaded);
   const document=(await legalDetail(db,actor,owner.id)).documents.find(row=>row.id===uploaded.documentId)!;sourceId=document.attachment_source_id!;linkId=document.attachment_link_id!;setId=String(await db.prepare('SELECT set_id FROM attachment_links WHERE id=?').bind(linkId).first('set_id'));version=2;
   const exported=await documentExport(db,env.H001_FILES,actor,document.id,null);expect(Buffer.from(await exported.arrayBuffer())).toEqual(Buffer.from(bytes));
  }else{
   await legalCommand(db,actor,{action:'document',id:owner.id,expectedVersion:1,name:'Terms shared with one counterparty',body:'Synthetic original terms; no personal data.'},key());const document=(await legalDetail(db,actor,owner.id)).documents[0],email=`counterparty-${key()}@example.test`;
   await legalCommand(db,actor,{action:'share',id:owner.id,expectedVersion:2,documentId:document.id,email,expiresAt:Date.now()+86400000},key());version=3;
   const shareId=String(await db.prepare('SELECT id FROM legal_shares WHERE item_id=? AND email=?').bind(owner.id,email).first('id')),portal={userId:key(),email,sessionId:key()};
   await expect(portalUpload(db,env.H001_FILES,{...portal,email:'other-'+email},shareId,'synthetic-counterparty.pdf','application/pdf',bytes,key())).rejects.toMatchObject({status:404});const receipt=await portalUpload(env.H001_DB,env.H001_FILES,portal,shareId,'synthetic-counterparty.pdf','application/pdf',bytes,command);expect(await portalUpload(env.H001_DB,env.H001_FILES,portal,shareId,'synthetic-counterparty.pdf','application/pdf',bytes,command)).toEqual(receipt);
   const external=(await db.prepare('SELECT attachment_source_id,attachment_link_id,checksum FROM legal_external_uploads WHERE id=?').bind(command).first())!;sourceId=String(external.attachment_source_id);linkId=String(external.attachment_link_id);setId=String(await db.prepare('SELECT set_id FROM attachment_links WHERE id=?').bind(linkId).first('set_id'));expect(external.checksum).toBe(createHash('sha256').update(bytes).digest('hex'));expect((await portalDetail(db,portal,shareId)).doc).toMatchObject({id:document.id});
   version=(await legalDetail(db,actor,owner.id)).item.version;expect(version).toBe(4);await legalCommand(db,actor,{action:'share.revoke',id:owner.id,expectedVersion:version,shareId},key());version++;
   await expect(portalDetail(db,portal,shareId)).rejects.toMatchObject({status:404});await expect(portalUpload(env.H001_DB,env.H001_FILES,portal,shareId,'synthetic-counterparty.pdf','application/pdf',bytes,command)).rejects.toMatchObject({status:404});
  }
  await storedEvidence(sourceId,actor,tuple.targetType,owner.id,tuple.purposeKey,bytes);expect(await attachmentBindingState(db,setId)).toMatchObject({set_state:'bound',link_state:'bound',binding_type:tuple.componentKind==='record'?'legal_document':'legal_portal_submission',bound_command_id:command});expect(await db.prepare('SELECT version FROM legal_items WHERE id=?').bind(owner.id).first('version')).toBe(version);
  expect((await listAttachmentSets(db,actor,{targetType:'legal_item',targetId:owner.id,purposeKey:tuple.purposeKey})).find(row=>row.id===setId)).toMatchObject({state:'bound',links:[expect.objectContaining({sourceId,linkState:'bound',sourceState:'available'})]});expect(Buffer.from(await (await attachmentContent(db,env.H001_FILES,actor,linkId,'original')).object.arrayBuffer())).toEqual(Buffer.from(bytes));
  expect(await listAttachmentSets(db,foreignActor,{targetType:'legal_item',targetId:owner.id})).toEqual([]);await expect(attachmentContent(db,env.H001_FILES,foreignActor,linkId,'original')).rejects.toMatchObject({status:404});await db.prepare('UPDATE assignments SET revoked_at=?,version=version+1 WHERE id=?').bind(Date.now(),actor.assignmentId).run();await expect(attachmentContent(db,env.H001_FILES,actor,linkId,'original')).rejects.toMatchObject({status:404});await expect(listAttachmentSets(db,actor,{targetType:'legal_item',targetId:owner.id})).rejects.toMatchObject({status:404});expect(await attachmentBindingState(db,setId)).toMatchObject({set_state:'bound',link_state:'bound',bound_command_id:command});
 });
});
