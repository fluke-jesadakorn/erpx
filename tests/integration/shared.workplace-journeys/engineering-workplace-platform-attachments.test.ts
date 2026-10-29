import {afterAll,beforeAll,describe,expect,it,vi} from 'vitest';
import {createHash} from 'node:crypto';
import sharp from 'sharp';
import {PDFDocument} from 'pdf-lib';
import {applyMigrations,env} from "../../support/runtime-env";
import {expenseDb as db,expenseFixture,key} from "../../support/helpers/expense-fixture";
import {workplaceAttachmentOwner,legalAttachmentOwner,controlAccount,type WorkplaceAttachmentTuple} from "../../support/helpers/engineering-workplace-platform-attachment-owners";
import ownerMatrix from "../../catalogue/engineering-attachment-owner-cases.json";
import type {WorkContext} from "@erpx/server-identity_access/public/authorization";
import {loadIamAuthority} from "@erpx/server-identity_access/public/iam-store";
import {attachmentBindingState} from "../../support/helpers/attachment-v2";
import {requiresSanitizedEvidenceAttestation} from "@erpx/server-files/public/attachments/finance-operations-policy";
import {attachmentContent,attachmentLinkStatus,attestAttachmentSetSanitized,bindRecordAttachmentSet,completeAttachmentUpload,createAttachmentSet,drainAttachmentScans,listAttachmentSets,prepareAttachmentBinding,reserveAttachmentFile,createAttachmentPublication,approveAttachmentPublication,publicAttachmentPublication} from "@erpx/server-files/public/attachments/service";
import {uploadDocument,documentExport} from "@erpx/server-legal/public/legal/documents";
import {legalCommand,detail as legalDetail} from "@erpx/server-legal/public/legal/service";
import {portalUpload,portalDetail} from "@erpx/server-legal/public/legal/portal";

vi.mock("@erpx/server-runtime/adapters/runtime",()=>({holdingResources:async()=>({write:env.H001_DB,files:env.H001_FILES}),runtimeEnv:async()=>env}));
// Declared clean scanner fixture only. Bytes, signature detection, real preview
// rendering, private storage and all owner authorization/binding are native.
// The compiled release scanner is independently required by the release gate.
vi.mock("@erpx/server-files/public/attachments/scanner",()=>({
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
  const input={targetType:tuple.targetType,targetId:owner.targetId,componentKind:tuple.componentKind,purposeKey:tuple.purposeKey,componentRef:owner.componentRef??undefined,expectedTargetVersion:owner.version},setKey=key(),fileInput={filename:'synthetic-reviewed-evidence.png',mediaType:'image/png',bytes:png.byteLength,caption:'Reviewed sanitized synthetic business artifact only.'};
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
  if(owner.bindAction&&['iam_review','iam_migration','people_provision_operation','company_release_acceptance'].includes(tuple.targetType)){
   const iam=tuple.targetType!=='company_release_acceptance',fault='platform_evidence_'+key().replace(/[^a-z0-9]/g,''),table=iam?'iam_audit':'outbox_events',column=iam?'target_id':'command_id';
   await db.exec(`CREATE FUNCTION ${fault}() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'SYSTEM_EVIDENCE_LATE_COMMIT' USING ERRCODE='23514';END;$$;CREATE TRIGGER ${fault} AFTER INSERT ON ${table} FOR EACH ROW WHEN (NEW.${column}='${command}') EXECUTE FUNCTION ${fault}();`);
   try{
    await expect(bind()).rejects.toThrow('SYSTEM_EVIDENCE_LATE_COMMIT');
    expect(await db.prepare(`SELECT * FROM ${owner.table} WHERE ${owner.idColumn??'id'}=?`).bind(rowId).first()).toEqual(ownerBefore);
    expect(await attachmentBindingState(db,set.id)).toMatchObject({set_state:'draft',link_state:'draft',bound_command_id:null});
    expect(await db.prepare(iam?'SELECT count(*) n FROM iam_receipts WHERE id=?':'SELECT count(*) n FROM command_receipts WHERE command_id=?').bind(command).first('n')).toBe(0);
    expect(await db.prepare(`SELECT count(*) n FROM ${table} WHERE ${column}=?`).bind(command).first('n')).toBe(0);
   }finally{await db.exec(`DROP TRIGGER ${fault} ON ${table};DROP FUNCTION ${fault}();`);}
  }
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
   if(!uploaded.documentId)throw new Error("Canonical legal upload must return its document receipt");
   const document=(await legalDetail(db,actor,owner.id)).documents.find(row=>row.id===uploaded.documentId)!;sourceId=document.attachment_source_id!;linkId=document.attachment_link_id!;setId=String(await db.prepare('SELECT set_id FROM attachment_links WHERE id=?').bind(linkId).first('set_id'));version=2;
   const exported=await documentExport(db,env.H001_FILES,actor,document.id,null);expect(Buffer.from(await exported.arrayBuffer())).toEqual(Buffer.from(bytes));
  }else{
   await legalCommand(db,actor,{action:'document',id:owner.id,expectedVersion:1,name:'Terms shared with one counterparty',body:'Synthetic original terms; no personal data.'},key());const document=(await legalDetail(db,actor,owner.id)).documents[0],email=`counterparty-${key()}@example.test`;
   await legalCommand(db,actor,{action:'share',id:owner.id,expectedVersion:2,documentId:document.id,email,expiresAt:Date.now()+86400000},key());version=3;
   const shareId=String(await db.prepare('SELECT id FROM legal_shares WHERE item_id=? AND email=?').bind(owner.id,email).first('id')),portal={userId:key(),email,sessionId:key()};
   await expect(portalUpload(db,env.H001_FILES,{...portal,email:'other-'+email},shareId,'synthetic-counterparty.pdf','application/pdf',bytes,key())).rejects.toMatchObject({status:404});
   const fault=`legal_portal_late_${key().replaceAll('-','')}`;
   await db.exec(`CREATE FUNCTION ${fault}() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'LEGAL_PORTAL_LATE_OUTBOX' USING ERRCODE='23514';END;$$;CREATE TRIGGER ${fault} AFTER INSERT ON outbox_events FOR EACH ROW WHEN (NEW.event_type='legal.portal.upload') EXECUTE FUNCTION ${fault}();`);
   try {
    await expect(portalUpload(db,env.H001_FILES,portal,shareId,'synthetic-counterparty.pdf','application/pdf',bytes,command)).rejects.toMatchObject({code:'23514'});
    expect(await db.prepare('SELECT id FROM legal_external_uploads WHERE id=?').bind(command).first()).toBeNull();
    expect(await db.prepare('SELECT id FROM legal_portal_receipts WHERE id=?').bind(command).first()).toBeNull();
    expect(await db.prepare('SELECT id FROM audit_events WHERE command_id=?').bind(command).first()).toBeNull();
    expect(await db.prepare('SELECT id FROM outbox_events WHERE command_id=?').bind(command).first()).toBeNull();
    expect(await db.prepare('SELECT version FROM legal_items WHERE id=?').bind(owner.id).first('version')).toBe(3);
    expect(await attachmentBindingState(db,`attachment-set-legal-portal-${command}`)).toMatchObject({set_state:'draft',link_state:'draft',bound_command_id:null});
   } finally {await db.exec(`DROP TRIGGER ${fault} ON outbox_events;DROP FUNCTION ${fault}();`);}
   const receipt=await portalUpload(env.H001_DB,env.H001_FILES,portal,shareId,'synthetic-counterparty.pdf','application/pdf',bytes,command);expect(await portalUpload(env.H001_DB,env.H001_FILES,portal,shareId,'synthetic-counterparty.pdf','application/pdf',bytes,command)).toEqual(receipt);
   const external=(await db.prepare('SELECT attachment_source_id,attachment_link_id,checksum FROM legal_external_uploads WHERE id=?').bind(command).first())!;sourceId=String(external.attachment_source_id);linkId=String(external.attachment_link_id);setId=String(await db.prepare('SELECT set_id FROM attachment_links WHERE id=?').bind(linkId).first('set_id'));expect(external.checksum).toBe(createHash('sha256').update(bytes).digest('hex'));expect((await portalDetail(db,portal,shareId)).doc).toMatchObject({id:document.id});
   version=(await legalDetail(db,actor,owner.id)).item.version;expect(version).toBe(4);await legalCommand(db,actor,{action:'share.revoke',id:owner.id,expectedVersion:version,shareId},key());version++;
   await expect(portalDetail(db,portal,shareId)).rejects.toMatchObject({status:404});await expect(portalUpload(env.H001_DB,env.H001_FILES,portal,shareId,'synthetic-counterparty.pdf','application/pdf',bytes,command)).rejects.toMatchObject({status:404});
  }
  await storedEvidence(sourceId,actor,tuple.targetType,owner.id,tuple.purposeKey,bytes);expect(await attachmentBindingState(db,setId)).toMatchObject({set_state:'bound',link_state:'bound',binding_type:tuple.componentKind==='record'?'legal_document':'legal_portal_submission',bound_command_id:command});expect(await db.prepare('SELECT version FROM legal_items WHERE id=?').bind(owner.id).first('version')).toBe(version);
  expect((await listAttachmentSets(db,actor,{targetType:'legal_item',targetId:owner.id,purposeKey:tuple.purposeKey})).find(row=>row.id===setId)).toMatchObject({state:'bound',links:[expect.objectContaining({sourceId,linkState:'bound',sourceState:'available'})]});expect(Buffer.from(await (await attachmentContent(db,env.H001_FILES,actor,linkId,'original')).object.arrayBuffer())).toEqual(Buffer.from(bytes));
  expect(await listAttachmentSets(db,foreignActor,{targetType:'legal_item',targetId:owner.id})).toEqual([]);await expect(attachmentContent(db,env.H001_FILES,foreignActor,linkId,'original')).rejects.toMatchObject({status:404});await db.prepare('UPDATE assignments SET revoked_at=?,version=version+1 WHERE id=?').bind(Date.now(),actor.assignmentId).run();await expect(attachmentContent(db,env.H001_FILES,actor,linkId,'original')).rejects.toMatchObject({status:404});await expect(listAttachmentSets(db,actor,{targetType:'legal_item',targetId:owner.id})).rejects.toMatchObject({status:404});expect(await attachmentBindingState(db,setId)).toMatchObject({set_state:'bound',link_state:'bound',bound_command_id:command});
 });
});

const foundationTargets=['organization_node','organization_change','iam_review','iam_migration','company_release_acceptance','company_provisioning'] as const;
async function foundationPrivateEvidence(target:typeof foundationTargets[number]){
 const tuple=tuples.find(row=>row.targetType===target&&(row.componentKind==='record'||target==='organization_change'))!;
 const owner=await workplaceAttachmentOwner(tuple),actor=owner.actor;
 // These are Company foundation records. A valid original Company assignment
 // with no product modules or purchases still retains its own evidence.
 await db.prepare("UPDATE assignments SET modules_json='[]' WHERE id=?").bind(actor.assignmentId).run();actor.modules=[];actor.iam=await loadIamAuthority(db,actor.companyId,actor.userId,actor.role,actor.authority?.sessionId);
 const set=await createAttachmentSet(db,actor,{targetType:target,targetId:owner.targetId,purposeKey:tuple.purposeKey,componentKind:tuple.componentKind,componentRef:owner.componentRef??undefined,expectedTargetVersion:owner.version},key()),reserved=await reserveAttachmentFile(db,actor,set.id,{filename:'sanitized-foundation-evidence.png',mediaType:'image/png',bytes:png.byteLength,caption:'Reviewed synthetic Company foundation artifact.'},key()),uploaded=await completeAttachmentUpload(db,env.H001_FILES,actor,reserved.uploadIntentId,Uint8Array.from(png).buffer);
 await drainAttachmentScans(db,env.H001_FILES,uploaded.sourceId);
 if(requiresSanitizedEvidenceAttestation(target))await attestAttachmentSetSanitized(db,actor,set.id,{confirmed:true,note:'Reviewed actual synthetic artifact: no secrets, credentials, private keys or configuration/database dumps.'},key());
 const command=key(),bind=async(transaction:import("@erpx/server-runtime/public/runtime/types").SqlDatabase)=>{
  if(target==='organization_change'){const {decideOrganizationNodeChange}=await import('@erpx/server-organization/public/organization');return decideOrganizationNodeChange(transaction,actor,owner.targetId,{expectedVersion:owner.version,decision:'approve',reason:'Independent original Company review of the synthetic branch',attachmentSetIds:[set.id]},command);}
  return bindRecordAttachmentSet(transaction,actor,set.id,owner.version,command);
 };
 return {owner,actor,target,tuple,set,reserved,sourceId:uploaded.sourceId,linkId:reserved.attachmentLinkId,bind};
}
for(const target of foundationTargets){
 it(`fresh foundation private bytes preserve ${target} Company authority without products and reject late native source or IAM changes`,async()=>{
  const file=await foundationPrivateEvidence(target),{owner,actor}=file;await file.bind(db);await db.prepare('UPDATE attachments SET legal_hold=1 WHERE id=?').bind(file.sourceId).run();
  const identity=owner.idColumn??'id',nativeBefore=await db.prepare(`SELECT * FROM ${owner.table} WHERE ${identity}=?`).bind(owner.targetId).first(),objectKey=String(await db.prepare('SELECT object_key FROM attachments WHERE id=?').bind(file.sourceId).first('object_key'));let consumed=0;
  const files:import("@erpx/server-runtime/public/runtime/types").FileStore={get:async(...args)=>{const object=await env.H001_FILES.get(...args);return object?{...object,arrayBuffer:async()=>{consumed++;return object.arrayBuffer();}}:null;},put:(...args)=>env.H001_FILES.put(...args),head:(...args)=>env.H001_FILES.head(...args),delete:(...args)=>env.H001_FILES.delete(...args),list:(...args)=>env.H001_FILES.list(...args)};
  const code=target.startsWith('organization_')?'ORGANIZATION_FILE_READ_CHANGED':target.startsWith('iam_')?'IAM_FILE_READ_CHANGED':target==='company_release_acceptance'?'COMPANY_RELEASE_FILE_READ_CHANGED':'COMPANY_PROVISIONING_FILE_READ_CHANGED',transactionCode=code.replace('_CHANGED','_TRANSACTION_REQUIRED');
  try{
   for(const mode of ['disabled','enforced']){
    for(const flag of ['ERPX_APPS_PACKAGE_ENFORCEMENT','ERPX_PEOPLE_PACKAGE_ENFORCEMENT','ERPX_FINANCE_PACKAGE_ENFORCEMENT'])vi.stubEnv(flag,mode);
    expect(Buffer.from(await(await attachmentContent(db,files,actor,file.linkId,'original')).object.arrayBuffer())).toEqual(png);
    await expect(db.transaction(transaction=>attachmentContent(transaction,files,actor,file.linkId,'original'))).rejects.toMatchObject({code:transactionCode});
    for(const fault of ['native','iam'] as const){
     const trigger=`foundation_byte_${key().replaceAll('-','')}`,effect=fault==='iam'?`UPDATE assignments SET revoked_at=1,version=version+1 WHERE id='${actor.assignmentId}'`:target==='iam_migration'?`UPDATE iam_connections SET name=name||' late mutation' WHERE id=(SELECT connection_id FROM iam_migrations WHERE id='${owner.targetId}')`:target==='company_release_acceptance'?`UPDATE company_release_revisions SET revision=revision+1 WHERE company_id='${actor.companyId}'`:`UPDATE ${owner.table} SET ${owner.versionColumn}=${owner.versionColumn}+1 WHERE ${identity}='${owner.targetId}'`;
     const beforeAudits=Number(await db.prepare("SELECT count(*) n FROM attachment_access_events WHERE link_id=? AND outcome='allowed'").bind(file.linkId).first('n'));
     await db.exec(`CREATE FUNCTION ${trigger}() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN ${effect};RETURN NEW;END $$;CREATE CONSTRAINT TRIGGER ${trigger} AFTER INSERT ON attachment_access_events DEFERRABLE INITIALLY DEFERRED FOR EACH ROW WHEN(NEW.link_id='${file.linkId}' AND NEW.outcome='allowed') EXECUTE FUNCTION ${trigger}()`);
     try{await expect(attachmentContent(db,files,actor,file.linkId,'original')).rejects.toMatchObject({code});}finally{await db.exec(`DROP TRIGGER ${trigger} ON attachment_access_events;DROP FUNCTION ${trigger}()`);}
     expect(await db.prepare(`SELECT * FROM ${owner.table} WHERE ${identity}=?`).bind(owner.targetId).first()).toEqual(nativeBefore);expect(await db.prepare('SELECT revoked_at,version FROM assignments WHERE id=?').bind(actor.assignmentId).first()).toEqual({revoked_at:null,version:1});expect(Number(await db.prepare("SELECT count(*) n FROM attachment_access_events WHERE link_id=? AND outcome='allowed'").bind(file.linkId).first('n'))).toBe(beforeAudits);
    }
   }
  }finally{vi.unstubAllEnvs();}
  expect(consumed).toBe(6);expect(await db.prepare('SELECT count(*) n FROM company_package_grants WHERE company_id=?').bind(actor.companyId).first('n')).toBe(0);expect(actor.modules).toEqual([]);
  await env.H001_FILES.delete(objectKey);await expect(attachmentContent(db,files,actor,file.linkId,'original')).rejects.toMatchObject({status:503,code:'FILE_UNAVAILABLE'});expect(await db.prepare("SELECT count(*) n FROM attachment_access_events WHERE link_id=? AND outcome='denied' AND reason='FILE_UNAVAILABLE'").bind(file.linkId).first('n')).toBe(1);await env.H001_FILES.put(objectKey,png,{httpMetadata:{contentType:'image/png'}});
  await db.prepare('UPDATE assignments SET revoked_at=1,version=version+1 WHERE id=?').bind(actor.assignmentId).run();await expect(attachmentContent(db,files,actor,file.linkId,'original')).rejects.toMatchObject({status:404,code:'ATTACHMENT_TARGET_NOT_FOUND'});expect(await db.prepare("SELECT count(*) n FROM attachment_access_events WHERE link_id=? AND outcome='denied' AND reason='ATTACHMENT_TARGET_NOT_FOUND'").bind(file.linkId).first('n')).toBe(1);expect(consumed).toBe(6);expect(await db.prepare('SELECT legal_hold,revoked_at FROM attachments WHERE id=?').bind(file.sourceId).first()).toEqual({legal_hold:1,revoked_at:null});expect(Buffer.from(await(await env.H001_FILES.get(objectKey))!.arrayBuffer())).toEqual(png);
 });
 it(`fresh foundation private bytes see actual disabled ${target} native File binding before the first source snapshot`,async()=>{
  const file=await foundationPrivateEvidence(target),{owner,actor}=file,{committedSourceVisibility}=await import('../../support/helpers/source-writer-visibility'),{measureReads}=await import('@erpx/server-runtime/public/runtime/read-metrics'),trigger=`foundation_snapshot_${key().replaceAll('-','')}`;
  await db.exec(`CREATE FUNCTION ${trigger}() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN IF(SELECT ${owner.versionColumn} FROM ${owner.table} WHERE ${owner.idColumn??'id'}='${owner.targetId}')<>${owner.version+1} THEN RAISE EXCEPTION 'FOUNDATION_FILE_STALE_FIRST_SNAPSHOT';END IF;RETURN NEW;END $$;CREATE CONSTRAINT TRIGGER ${trigger} AFTER INSERT ON attachment_access_events DEFERRABLE INITIALLY DEFERRED FOR EACH ROW WHEN(NEW.link_id='${file.linkId}' AND NEW.outcome='allowed') EXECUTE FUNCTION ${trigger}()`);
  try{const observed=await measureReads(()=>committedSourceVisibility(db,transaction=>file.bind(transaction),()=>attachmentContent(db,env.H001_FILES,actor,file.linkId,'original')));expect(Buffer.from(await observed.result.object.arrayBuffer())).toEqual(png);expect(observed.metrics.deadlockFailures??0).toBe(0);expect(observed.metrics.serializationFailures??0).toBe(0);expect(observed.metrics.transactionRetries??0).toBe(0);}finally{await db.exec(`DROP TRIGGER ${trigger} ON attachment_access_events;DROP FUNCTION ${trigger}()`);}
 });
}
