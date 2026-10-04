import {afterAll,beforeAll,describe,expect,it,vi} from "vitest";
import {createHash} from "node:crypto";
import {applyMigrations,env} from "./runtime-env";
import {expenseDb as db,expenseFixture,key} from "./helpers/expense-fixture";
import type {WorkContext} from "../src/server/authorization";
import {type AttachmentOwnerTuple} from "./helpers/engineering-finance-people-attachment-owners";
import {serviceHospitalityAttachmentOwner} from "./helpers/engineering-service-hospitality-attachment-owners";
import ownerMatrix from "../docs/testing/engineering-attachment-owner-cases.json";
import {loadIamAuthority} from "../src/server/iam-store";
import {attachmentBindingState} from "./helpers/attachment-v2";
import {attachmentContent,attachmentLinkStatus,bindRecordAttachmentSet,completeAttachmentUpload,createAttachmentSet,drainAttachmentScans,listAttachmentSets,prepareAttachmentBinding,reserveAttachmentFile} from "../src/server/attachments/service";

vi.mock("../src/server/runtime",()=>({holdingResources:async()=>({write:env.H001_DB,files:env.H001_FILES}),runtimeEnv:async()=>env}));
// Explicit clean scanner fixture; format detection, preview generation, storage,
// upload transactions, permissions and owner commands remain production code.
// The release scanner gate independently proves the real compiled scanner.
vi.mock("../src/server/attachments/scanner",()=>({
 attachmentScannerReadiness:async()=>({enabled:true,ready:true,engine:"Declared clean scanner fixture",definitionUpdatedAt:Date.now(),error:null}),
 scanAttachmentBytes:async()=>({clean:true,signature:null,engine:"Declared clean scanner fixture",definitionUpdatedAt:Date.now()}),
}));
const targetTypes=new Set(["employee_request","leave_request","expense_claim","expense_payment","invoice_payment","staff_task","pos_sale","pos_refund","pos_shift","pms_guest","pms_reservation","pms_stay","pms_task","pms_lost_property","pms_property"]);
const tuples=ownerMatrix.filter(row=>targetTypes.has(row.targetType)) as AttachmentOwnerTuple[];
const png=Buffer.from("iVBORw0KGgoAAAANSUhEUgAAAAIAAAACCAYAAABytg0kAAAACXBIWXMAAAPoAAAD6AG1e1JrAAAAEUlEQVQImWOQaEj4D8IMMAYAQaQH3RNNL/AAAAAASUVORK5CYII=","base64"),checksum=createHash("sha256").update(png).digest("hex"),previousGate=process.env.ERPX_ATTACHMENTS_V2;
let foreignActor:WorkContext;
beforeAll(async()=>{await applyMigrations(db,env.STAFF_TEST_MIGRATIONS);process.env.ERPX_ATTACHMENTS_V2="true";foreignActor=(await expenseFixture()).owner;foreignActor.iam=await loadIamAuthority(db,foreignActor.companyId,foreignActor.userId,foreignActor.role);});
afterAll(()=>{if(previousGate===undefined)delete process.env.ERPX_ATTACHMENTS_V2;else process.env.ERPX_ATTACHMENTS_V2=previousGate;});

describe("Service and hospitality File Sources owner intake",()=>{
 it.each(tuples.map(row=>[row.id,row] as const))("proves authorized intake scan binding reload and revocation for %s",async(_id,tuple)=>{
  const owner=await serviceHospitalityAttachmentOwner(tuple),actor=owner.actor;
  actor.iam=await loadIamAuthority(db,actor.companyId,actor.userId,actor.role);
  const initialSourceCount=Number(await db.prepare("SELECT COUNT(*) n FROM attachments WHERE company_id=?").bind(actor.companyId).first("n"));
  const input={targetType:tuple.targetType,targetId:owner.targetId,componentKind:tuple.componentKind,purposeKey:tuple.purposeKey,componentRef:owner.componentRef,expectedTargetVersion:owner.version},setKey=key();
  await expect(createAttachmentSet(db,actor,{...input,expectedTargetVersion:owner.version+1},key())).rejects.toMatchObject({code:"ATTACHMENT_TARGET_CHANGED"});
  const set=await createAttachmentSet(db,actor,input,setKey);expect(await createAttachmentSet(db,actor,input,setKey)).toMatchObject({id:set.id,state:"draft"});
  const fileInput={filename:"synthetic-owner-evidence.png",mediaType:"image/png",bytes:png.byteLength,caption:"Synthetic owner evidence; no real employee or payment data."},reserveKey=key(),reserved=await reserveAttachmentFile(db,actor,set.id,fileInput,reserveKey);
  const revokedIntakeSet=await createAttachmentSet(db,actor,{...input,targetId:owner.pendingTargetId??input.targetId,componentRef:`${owner.componentRef??"record"}:revocation-intake`},key()),revokedReservation=await reserveAttachmentFile(db,actor,revokedIntakeSet.id,fileInput,key());
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
  await expect(prepareAttachmentBinding(db,tuple.targetType==="leave_request"?owner.fixture.head:actor,{attachmentSetIds:[set.id],bindingContract:{targetType:tuple.targetType,bindingType:owner.bindingType,contexts:[{purposeKey:tuple.purposeKey,componentKind:tuple.componentKind,componentRef:owner.componentRef??null}]},targetId:owner.targetId,expectedTargetVersion:owner.version+1,commandId:key(),bindingId:owner.targetId})).rejects.toMatchObject({code:tuple.targetType==="leave_request"?"ATTACHMENT_TARGET_CHANGED":"ATTACHMENT_SET_STALE"});
  if(tuple.componentKind==="action")await expect(bindRecordAttachmentSet(db,actor,set.id,owner.version,key())).rejects.toMatchObject({code:"ACTION_EVIDENCE_DOMAIN_BIND_REQUIRED"});
  const result=await bind();expect(await bind()).toEqual(result);
  expect(await completeAttachmentUpload(db,env.H001_FILES,actor,reserved.uploadIntentId,body)).toMatchObject({sourceId:uploaded.sourceId,state:"available"});
  expect(await attachmentBindingState(db,set.id)).toMatchObject({set_state:"bound",link_state:"bound",binding_type:owner.bindingType,bound_command_id:command});
  const ownerRow=(await db.prepare(`SELECT * FROM ${owner.table} WHERE id=?`).bind(owner.targetId).first())!;expect(Number(ownerRow[owner.versionColumn])).toBe(owner.version+(tuple.targetType==="pos_refund"?2:1));
  if(tuple.targetType==="expense_payment")expect(ownerRow.status).toBe("confirmed");
  if(tuple.targetType==="invoice_payment")expect(ownerRow).toMatchObject({status:"paid",outstanding_minor:0});
  if(tuple.targetType==="pos_sale")expect(ownerRow.state).toBe("cancelled");
  if(tuple.targetType==="pos_refund")expect(ownerRow.state).toBe("completed");
  if(tuple.targetType==="pos_shift")expect(ownerRow.state).toBe("reviewed");
  if(tuple.targetType==="leave_request")expect(ownerRow.state).toBe("pending_approval");
  if(tuple.targetType==="employee_request"&&tuple.componentKind==="action")expect(ownerRow.state).toBe(tuple.purposeKey==="fulfillment"?"resolved":"queued");
  if(tuple.targetType==="pms_task")expect(JSON.parse(String(ownerRow.data_json)).state).toBe("clean");
  if(tuple.targetType==="pms_lost_property"&&tuple.componentKind==="action")expect(JSON.parse(String(ownerRow.data_json)).state).toBe("returned");
  const reloaded=await listAttachmentSets(db,actor,{targetType:tuple.targetType,targetId:owner.targetId,purposeKey:tuple.purposeKey,componentKind:tuple.componentKind});
  expect(reloaded.filter(row=>row.state==="bound")).toHaveLength(1);expect(reloaded.find(row=>row.id===set.id)).toMatchObject({id:set.id,state:"bound",expectedTargetVersion:owner.version,links:[expect.objectContaining({id:reserved.attachmentLinkId,sourceId:uploaded.sourceId,linkState:"bound",sourceState:"available",bytes:png.byteLength})]});
  const original=await attachmentContent(db,env.H001_FILES,actor,reserved.attachmentLinkId,"original"),preview=await attachmentContent(db,env.H001_FILES,actor,reserved.attachmentLinkId,"preview");expect(Buffer.from(await original.object.arrayBuffer())).toEqual(png);expect(preview.mediaType).toBe("image/webp");expect((await preview.object.arrayBuffer()).byteLength).toBeGreaterThan(0);
  const stored=(await db.prepare("SELECT variant,object_key,bytes,state FROM attachment_storage_ledger WHERE attachment_id=? ORDER BY variant").bind(uploaded.sourceId).all<{variant:string;object_key:string;bytes:number;state:string}>()).results;expect(stored.map(row=>row.variant)).toEqual(["original","preview"]);expect(stored.every(row=>row.state==="stored")).toBe(true);for(const row of stored){const object=await env.H001_FILES.get(row.object_key);expect(object).not.toBeNull();expect((await object!.arrayBuffer()).byteLength).toBe(Number(row.bytes));}
  const foreignListing=await listAttachmentSets(db,foreignActor,{targetType:tuple.targetType,targetId:owner.targetId});expect(foreignListing).toEqual([]);await expect(attachmentContent(db,env.H001_FILES,foreignActor,reserved.attachmentLinkId,"original")).rejects.toMatchObject({status:404,code:"ATTACHMENT_LINK_NOT_FOUND"});await expect(reserveAttachmentFile(db,foreignActor,set.id,fileInput,key())).rejects.toMatchObject({status:404,code:"ATTACHMENT_SET_NOT_FOUND"});
  // Real role revocation against the cached role/permission snapshot. Normal
  // request loading must also reject this assignment; no fake empty IAM list.
  await db.prepare("UPDATE assignments SET revoked_at=?,version=version+1 WHERE id=?").bind(Date.now(),actor.assignmentId).run();
  const revoked={status:404,code:"ATTACHMENT_TARGET_NOT_FOUND"};await expect(listAttachmentSets(db,actor,{targetType:tuple.targetType,targetId:owner.targetId})).rejects.toMatchObject(revoked);await expect(attachmentContent(db,env.H001_FILES,actor,reserved.attachmentLinkId,"original")).rejects.toMatchObject(revoked);await expect(createAttachmentSet(db,actor,input,key())).rejects.toMatchObject(revoked);
  await expect(completeAttachmentUpload(db,env.H001_FILES,actor,reserved.uploadIntentId,body)).rejects.toMatchObject(revoked);await expect(completeAttachmentUpload(db,env.H001_FILES,actor,revokedReservation.uploadIntentId,body)).rejects.toMatchObject(revoked);expect(await db.prepare("SELECT status,actual_bytes FROM upload_intents WHERE id=?").bind(revokedReservation.uploadIntentId).first()).toEqual({status:"pending",actual_bytes:null});const quarantine=String(await db.prepare("SELECT quarantine_key FROM upload_intents WHERE id=?").bind(revokedReservation.uploadIntentId).first("quarantine_key"));expect(await env.H001_FILES.get(quarantine)).toBeNull();expect(await db.prepare("SELECT COUNT(*) n FROM attachments WHERE company_id=?").bind(actor.companyId).first("n")).toBe(initialSourceCount+1);
  expect(await attachmentBindingState(db,set.id)).toMatchObject({set_state:"bound",link_state:"bound",bound_command_id:command});expect(await db.prepare("SELECT COUNT(*) n FROM attachment_links WHERE attachment_id=? AND state='bound'").bind(uploaded.sourceId).first("n")).toBe(1);
  expect(await db.prepare("SELECT COUNT(*) n FROM attachment_access_events WHERE attachment_id=? AND outcome='denied' AND reason='ATTACHMENT_TARGET_NOT_FOUND'").bind(uploaded.sourceId).first("n")).toBe(1);expect(await db.prepare("SELECT COUNT(*) n FROM attachment_access_events WHERE attachment_id=? AND outcome='denied' AND reason='ATTACHMENT_NOT_AVAILABLE'").bind(uploaded.sourceId).first("n")).toBe(1);
 });
});
