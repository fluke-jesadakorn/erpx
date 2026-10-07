import {afterAll,beforeAll,describe,expect,it,vi} from "vitest";
import {createHash} from "node:crypto";
import {applyMigrations,env} from "./runtime-env";
import {expenseDb as db,expenseFixture,key} from "./helpers/expense-fixture";
import type {WorkContext} from "../src/server/authorization";
import {financePeopleAttachmentOwner,type AttachmentOwnerTuple} from "./helpers/engineering-finance-people-attachment-owners";
import ownerMatrix from "../docs/testing/engineering-attachment-owner-cases.json";
import {loadIamAuthority} from "../src/server/iam-store";
import {attachmentBindingState} from "./helpers/attachment-v2";
import {attachmentContent,attachmentLinkStatus,bindRecordAttachmentSet,completeAttachmentUpload,createAttachmentSet,drainAttachmentScans,listAttachmentSets,prepareAttachmentBinding,reserveAttachmentFile} from "../src/server/attachments/service";

vi.mock("../src/server/runtime",()=>({holdingResources:async()=>({write:env.H001_DB,files:env.H001_FILES}),runtimeEnv:async()=>env}));
// Explicit clean scanner fixture; format detection, preview generation, storage,
// upload transactions, permissions and owner commands remain production code.
// The release scanner gate independently proves the real compiled scanner.
vi.mock("../src/server/modules/files/public/attachments/scanner",()=>({
 attachmentScannerReadiness:async()=>({enabled:true,ready:true,engine:"Declared clean scanner fixture",definitionUpdatedAt:Date.now(),error:null}),
 scanAttachmentBytes:async()=>({clean:true,signature:null,engine:"Declared clean scanner fixture",definitionUpdatedAt:Date.now()}),
}));
const tuples=ownerMatrix.filter(row=>/^(finance_|hr_|people_lifecycle_case$|people_clearance_task$|payroll_|talent_|learning_|benefit_|time_entry$|overtime_request$)/.test(row.targetType)) as AttachmentOwnerTuple[];
const png=Buffer.from("iVBORw0KGgoAAAANSUhEUgAAAAIAAAACCAYAAABytg0kAAAACXBIWXMAAAPoAAAD6AG1e1JrAAAAEUlEQVQImWOQaEj4D8IMMAYAQaQH3RNNL/AAAAAASUVORK5CYII=","base64"),checksum=createHash("sha256").update(png).digest("hex"),previousGate=process.env.ERPX_ATTACHMENTS_V2;
let foreignActor:WorkContext;
beforeAll(async()=>{await applyMigrations(db,env.STAFF_TEST_MIGRATIONS);process.env.ERPX_ATTACHMENTS_V2="true";foreignActor=(await expenseFixture()).owner;foreignActor.iam=await loadIamAuthority(db,foreignActor.companyId,foreignActor.userId,foreignActor.role);});
afterAll(()=>{if(previousGate===undefined)delete process.env.ERPX_ATTACHMENTS_V2;else process.env.ERPX_ATTACHMENTS_V2=previousGate;});

describe("Finance and workforce File Sources owner intake",()=>{
 it.each(tuples.map(row=>[row.id,row] as const))("proves authorized intake scan binding reload and revocation for %s",async(_id,tuple)=>{
  const owner=await financePeopleAttachmentOwner(tuple),actor=owner.actor;
  actor.iam=await loadIamAuthority(db,actor.companyId,actor.userId,actor.role);
  const input={targetType:tuple.targetType,targetId:owner.targetId,componentKind:tuple.componentKind,purposeKey:tuple.purposeKey,componentRef:owner.componentRef,expectedTargetVersion:owner.version},setKey=key();
  await expect(createAttachmentSet(db,actor,{...input,expectedTargetVersion:owner.version+1},key())).rejects.toMatchObject({code:"ATTACHMENT_TARGET_CHANGED"});
  const set=await createAttachmentSet(db,actor,input,setKey);expect(await createAttachmentSet(db,actor,input,setKey)).toMatchObject({id:set.id,state:"draft"});
  const fileInput={filename:"synthetic-owner-evidence.png",mediaType:"image/png",bytes:png.byteLength,caption:"Synthetic owner evidence; no real employee or payment data."},reserveKey=key(),reserved=await reserveAttachmentFile(db,actor,set.id,fileInput,reserveKey);
  const revokedIntakeSet=await createAttachmentSet(db,actor,{...input,componentRef:`${owner.componentRef??"record"}:revocation-intake`},key()),revokedReservation=await reserveAttachmentFile(db,actor,revokedIntakeSet.id,fileInput,key());
  expect(await reserveAttachmentFile(db,actor,set.id,fileInput,reserveKey)).toEqual(reserved);
  await expect(reserveAttachmentFile(db,actor,set.id,{...fileInput,caption:"Changed evidence payload"},reserveKey)).rejects.toMatchObject({code:"IDEMPOTENCY_CONFLICT"});
  const body=png.buffer.slice(png.byteOffset,png.byteOffset+png.byteLength),uploaded=await completeAttachmentUpload(db,env.H001_FILES,actor,reserved.uploadIntentId,body);
  expect(uploaded).toMatchObject({state:"quarantined",status:"scan_pending"});
  expect((await completeAttachmentUpload(db,env.H001_FILES,actor,reserved.uploadIntentId,body)).sourceId).toBe(uploaded.sourceId);
  await expect(attachmentContent(db,env.H001_FILES,actor,reserved.attachmentLinkId,"original")).rejects.toMatchObject({code:"ATTACHMENT_NOT_AVAILABLE"});
  expect(await drainAttachmentScans(db,env.H001_FILES,uploaded.sourceId)).toBe(true);
  const status=await attachmentLinkStatus(db,actor,reserved.attachmentLinkId);expect(status).toMatchObject({sourceState:"available",linkState:"draft",bindable:true,previewAvailable:true,scan:{lastResult:"clean",lastError:null}});
  const source=await db.prepare("SELECT company_id,owner_type,owner_id,purpose,checksum_sha256,bytes,detected_media_type,source_state,object_key,preview_object_key FROM attachments WHERE id=?").bind(uploaded.sourceId).first();
  expect(source).toMatchObject({company_id:actor.companyId,owner_type:tuple.targetType,owner_id:owner.targetId,purpose:tuple.purposeKey,checksum_sha256:checksum,bytes:png.byteLength,detected_media_type:"image/png",source_state:"available"});
  expect(await db.prepare("SELECT status,checksum_sha256,actual_bytes FROM upload_intents WHERE id=?").bind(reserved.uploadIntentId).first()).toMatchObject({status:"ready",checksum_sha256:checksum,actual_bytes:png.byteLength});
  const command=key(),bind=()=>owner.bindAction?owner.bindAction(set.id,command):bindRecordAttachmentSet(db,actor,set.id,owner.version,command);
  await expect(prepareAttachmentBinding(db,actor,{attachmentSetIds:[set.id],bindingContract:{targetType:tuple.targetType,bindingType:owner.bindingType,contexts:[{purposeKey:tuple.purposeKey,componentKind:tuple.componentKind,componentRef:owner.componentRef??null}]},targetId:owner.targetId,expectedTargetVersion:owner.version+1,commandId:key(),bindingId:owner.targetId})).rejects.toMatchObject({code:"ATTACHMENT_SET_STALE"});
  if(tuple.componentKind==="action")await expect(bindRecordAttachmentSet(db,actor,set.id,owner.version,key())).rejects.toMatchObject({code:"ACTION_EVIDENCE_DOMAIN_BIND_REQUIRED"});
  const result=await bind();expect(await bind()).toEqual(result);
  expect(await completeAttachmentUpload(db,env.H001_FILES,actor,reserved.uploadIntentId,body)).toMatchObject({sourceId:uploaded.sourceId,state:"available"});
  expect(await attachmentBindingState(db,set.id)).toMatchObject({set_state:"bound",link_state:"bound",binding_type:owner.bindingType,bound_command_id:command});
  const ownerRow=(await db.prepare(`SELECT * FROM ${owner.table} WHERE id=?`).bind(owner.targetId).first())!;expect(Number(ownerRow[owner.versionColumn])).toBe(owner.version+(tuple.targetType==="payroll_run"?0:1));
  if(tuple.componentKind==="action"){
   if(tuple.targetType==="finance_bank_transaction")expect(ownerRow).toMatchObject({match_status:"matched",matched_type:"payment"});
   if(tuple.targetType==="finance_period")expect(ownerRow.state).toBe("closed");
   if(tuple.targetType==="finance_payment")expect(ownerRow).toMatchObject({status:"settled",outstanding_minor:0,journal_id:expect.any(String)});
   if(tuple.targetType==="finance_refund_obligation")expect(ownerRow).toMatchObject({status:"settled",outstanding_minor:0,base_outstanding_minor:0});
   if(tuple.targetType==="finance_supplier_batch"){expect(ownerRow.status).toBe("exported");expect(await db.prepare("SELECT status,payment_id,bank_transaction_id FROM finance_supplier_instructions WHERE batch_id=?").bind(owner.targetId).first()).toMatchObject({status:"confirmed",payment_id:expect.any(String),bank_transaction_id:expect.any(String)});}
   if(tuple.targetType==="learning_assignment")expect(ownerRow).toMatchObject({status:"completed",attendance:"attended",score_bps:9200});
   if(tuple.targetType==="benefit_dependent")expect(ownerRow.status).toBe("verified");
   if(tuple.targetType==="benefit_election")expect(ownerRow.status).toBe(tuple.purposeKey==="provider_proof"?"acknowledged":"submitted");
   if(tuple.targetType==="time_entry")expect(ownerRow).toMatchObject({status:"submitted",regular_hours:8,overtime_hours:0});
   if(tuple.targetType==="people_clearance_task")expect(ownerRow).toMatchObject({status:"completed",completed_by:actor.userId});
   if(tuple.targetType==="overtime_request")expect(ownerRow.state).toBe(tuple.purposeKey==="planned_evidence"?"pending_authorization":"pending_verification");
   if(tuple.targetType==="payroll_run"){expect(ownerRow.status).toBe("approved");expect(await db.prepare("SELECT status,run_version FROM hr_payroll_corrections WHERE run_id=?").bind(owner.targetId).first()).toEqual({status:"pending",run_version:owner.version});}
  }
  const reloaded=await listAttachmentSets(db,actor,{targetType:tuple.targetType,targetId:owner.targetId,purposeKey:tuple.purposeKey,componentKind:tuple.componentKind});
  expect(reloaded.filter(row=>row.state==="bound")).toHaveLength(1);expect(reloaded.find(row=>row.id===set.id)).toMatchObject({id:set.id,state:"bound",expectedTargetVersion:owner.version,links:[expect.objectContaining({id:reserved.attachmentLinkId,sourceId:uploaded.sourceId,linkState:"bound",sourceState:"available",bytes:png.byteLength})]});
  const original=await attachmentContent(db,env.H001_FILES,actor,reserved.attachmentLinkId,"original"),preview=await attachmentContent(db,env.H001_FILES,actor,reserved.attachmentLinkId,"preview");expect(Buffer.from(await original.object.arrayBuffer())).toEqual(png);expect(preview.mediaType).toBe("image/webp");expect((await preview.object.arrayBuffer()).byteLength).toBeGreaterThan(0);
  const stored=(await db.prepare("SELECT variant,object_key,bytes,state FROM attachment_storage_ledger WHERE attachment_id=? ORDER BY variant").bind(uploaded.sourceId).all<{variant:string;object_key:string;bytes:number;state:string}>()).results;expect(stored.map(row=>row.variant)).toEqual(["original","preview"]);expect(stored.every(row=>row.state==="stored")).toBe(true);for(const row of stored){const object=await env.H001_FILES.get(row.object_key);expect(object).not.toBeNull();expect((await object!.arrayBuffer()).byteLength).toBe(Number(row.bytes));}
  const foreignListing=await listAttachmentSets(db,foreignActor,{targetType:tuple.targetType,targetId:owner.targetId});expect(foreignListing).toEqual([]);await expect(attachmentContent(db,env.H001_FILES,foreignActor,reserved.attachmentLinkId,"original")).rejects.toMatchObject({status:404,code:"ATTACHMENT_LINK_NOT_FOUND"});await expect(reserveAttachmentFile(db,foreignActor,set.id,fileInput,key())).rejects.toMatchObject({status:404,code:"ATTACHMENT_SET_NOT_FOUND"});
  // Real role revocation against the cached role/permission snapshot. Normal
  // request loading must also reject this assignment; no fake empty IAM list.
  await db.prepare("UPDATE assignments SET revoked_at=?,version=version+1 WHERE id=?").bind(Date.now(),actor.assignmentId).run();
  const revoked={status:404,code:"ATTACHMENT_TARGET_NOT_FOUND"};await expect(listAttachmentSets(db,actor,{targetType:tuple.targetType,targetId:owner.targetId})).rejects.toMatchObject(revoked);await expect(attachmentContent(db,env.H001_FILES,actor,reserved.attachmentLinkId,"original")).rejects.toMatchObject(revoked);await expect(createAttachmentSet(db,actor,input,key())).rejects.toMatchObject(revoked);
  await expect(completeAttachmentUpload(db,env.H001_FILES,actor,reserved.uploadIntentId,body)).rejects.toMatchObject(revoked);await expect(completeAttachmentUpload(db,env.H001_FILES,actor,revokedReservation.uploadIntentId,body)).rejects.toMatchObject(revoked);expect(await db.prepare("SELECT status,actual_bytes FROM upload_intents WHERE id=?").bind(revokedReservation.uploadIntentId).first()).toEqual({status:"pending",actual_bytes:null});const quarantine=String(await db.prepare("SELECT quarantine_key FROM upload_intents WHERE id=?").bind(revokedReservation.uploadIntentId).first("quarantine_key"));expect(await env.H001_FILES.get(quarantine)).toBeNull();expect(await db.prepare("SELECT COUNT(*) n FROM attachments WHERE company_id=?").bind(actor.companyId).first("n")).toBe(1);
  expect(await attachmentBindingState(db,set.id)).toMatchObject({set_state:"bound",link_state:"bound",bound_command_id:command});expect(await db.prepare("SELECT COUNT(*) n FROM attachment_links WHERE attachment_id=? AND state='bound'").bind(uploaded.sourceId).first("n")).toBe(1);
  expect(await db.prepare("SELECT COUNT(*) n FROM attachment_access_events WHERE attachment_id=? AND outcome='denied' AND reason='ATTACHMENT_TARGET_NOT_FOUND'").bind(uploaded.sourceId).first("n")).toBe(1);expect(await db.prepare("SELECT COUNT(*) n FROM attachment_access_events WHERE attachment_id=? AND outcome='denied' AND reason='ATTACHMENT_NOT_AVAILABLE'").bind(uploaded.sourceId).first("n")).toBe(1);
 });
});
