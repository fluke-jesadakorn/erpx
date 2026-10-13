import {beforeAll,beforeEach,it,expect,vi} from 'vitest';
import {applyMigrations,env} from "../../support/runtime-env";
import {expenseDb as db,expenseFixture,key} from "../../support/helpers/expense-fixture";
import type {WorkContext} from "../../../src/server/modules/identity_access/public/authorization";
import {saveMaster,saveOrder,orderSchema} from "../../../src/server/modules/commerce/public/commerce";
import {createSupportTicket,changeSupportTicket,getSupportTicket,supportDetail} from "../../../src/server/modules/support/public/support";
import {bindSupportEvidence,revokeSupportEvidence,supportEvidence} from "../../../src/server/modules/support/public/support-evidence";
import {attachmentBindingState,seedAttachmentSet} from "../../support/helpers/attachment-v2";
import {customerPortalSharedFiles,customerPortalSharedPreview} from "../../../src/server/modules/customer_360/public/customer-portal-attachments";
const session=vi.hoisted(()=>({actor:null as WorkContext|null}));
vi.mock("../../../src/server/modules/identity_access/public/authorization",async original=>({...await original<typeof import("../../../src/server/modules/identity_access/public/authorization")>(),requireWorkContext:async()=>session.actor!}));
vi.mock("../../../src/server/modules/runtime/adapters/runtime",()=>({holdingResources:async()=>({write:env.H001_DB,files:env.H001_FILES}),runtimeEnv:async()=>env}));
vi.mock("../../../src/server/modules/files/public/attachments/scanner",()=>({
 attachmentScannerReadiness:async()=>({enabled:true,ready:true,engine:'ClamAV test',definitionUpdatedAt:Date.now(),error:null}),
 scanAttachmentBytes:async()=>({clean:true,signature:null,engine:'ClamAV test',definitionUpdatedAt:Date.now()}),
}));
vi.mock("../../../src/server/modules/files/public/attachments/preview",()=>({
 attachmentPreviewReadiness:async()=>({ready:true,heifConvert:'/test/heif-convert',pdftoppm:'/test/pdftoppm',error:null}),
 detectedAttachmentMediaType:(value:Uint8Array,declared:string)=>declared==='application/pdf'&&new TextDecoder().decode(value.slice(0,5))!=='%PDF-'?null:declared,
 sanitizedAttachmentPreview:async()=>null,
}));
import {POST as reserve} from "../../../src/app/api/v1/holdings/[holdingSlug]/uploads/route";
import {PUT as put} from "../../../src/app/api/v1/holdings/[holdingSlug]/uploads/[id]/content/route";
import {GET as download} from "../../../src/app/api/v1/holdings/[holdingSlug]/attachments/[id]/content/route";
import {POST as bindRecord} from "../../../src/app/api/v1/holdings/[holdingSlug]/attachment-sets/[setId]/bind/route";
let f:Awaited<ReturnType<typeof expenseFixture>>,ticket:string,orderId:string;
const bytes=new TextEncoder().encode('%PDF-1.4\nSimulation evidence, no real customer data\n%%EOF');
const ctx=(id:string)=>({params:Promise.resolve({holdingSlug:'test',id})});
beforeAll(async()=>{await applyMigrations(db,env.STAFF_TEST_MIGRATIONS);});
beforeEach(async()=>{
 f=await expenseFixture();f.head.role='support_agent';f.head.assignmentId=await f.grant(f.head,'support_agent');session.actor=f.head;
 const customer=await saveMaster(f.owner,'parties',null,{kind:'customer',code:key(),name:'Evidence customer'},key()),w=await saveMaster(f.owner,'warehouses',null,{branchId:f.branch,code:key(),name:'Evidence warehouse'},key()),p=await saveMaster(f.owner,'products',null,{sku:key(),name:'Evidence product',price:'10'},key()),order=await saveOrder(f.owner,null,orderSchema.parse({kind:'quote',partyId:customer.id,warehouseId:w.id,orderDate:'2026-09-20',dueDate:'2026-09-24',lines:[{productId:p.id,quantity:'1',price:'10',taxBps:0}]}),key());
 orderId=String(order.id);ticket=String((await createSupportTicket(f.owner,{orderId,sourceVersion:1,title:'Evidence support case',issue:'Customer issue with installation',ownerId:f.head.userId,dueDate:'2026-09-24'},key())).id);
});
async function intent(expectedVersion=1,k=key()) {return reserve(new Request('http://localhost:3100/api/v1/holdings/test/uploads',{method:'POST',headers:{'Content-Type':'application/json','Idempotency-Key':k},body:JSON.stringify({ownerType:'support_ticket',ownerId:ticket,expectedVersion,purpose:'support_evidence',filename:'simulation.pdf',mediaType:'application/pdf',bytes:bytes.length})}),{params:Promise.resolve({holdingSlug:'test'})});}
const upload=(id:string,data=bytes)=>put(new Request(`http://localhost:3100/api/v1/holdings/test/uploads/${id}/content`,{method:'PUT',headers:{'Content-Type':'application/pdf','Content-Length':String(data.length)},body:data}),ctx(id));
const get=(id:string)=>download(new Request(`http://localhost:3100/api/v1/holdings/test/attachments/${id}/content`),ctx(id));
it('uploads private evidence through existing routes, safely replays, preserves hashes/history and records a correction without mutating bound evidence',async()=>{
 const previous=process.env.ERPX_ATTACHMENTS_V2;process.env.ERPX_ATTACHMENTS_V2='false';delete process.env.ERPX_ATTACHMENTS_READ_ONLY;
 const k=key(),r=await intent(1,k);expect(r.status).toBe(201);const reserved=(await r.json()).data;expect(reserved).toMatchObject({attachmentSetId:expect.any(String),attachmentLinkId:expect.any(String)});expect((await (await intent(1,k)).json()).data.id).toBe(reserved.id);
 const response=await upload(reserved.id);expect(response.status).toBe(201);const result=(await response.json()).data;expect((await (await upload(reserved.id)).json()).data).toEqual(result);
 expect(await db.prepare('SELECT source_state FROM attachments WHERE id=?').bind(result.id).first('source_state')).toBe('available');expect(await db.prepare('SELECT state FROM attachment_links WHERE id=?').bind(reserved.attachmentLinkId).first('state')).toBe('bound');
 expect(await getSupportTicket(f.head,ticket)).toMatchObject({version:2,status:'open'});expect((await supportDetail(f.head,ticket)).history[0]).toMatchObject({action:'attachment-add',version:2});
 const content=await get(result.id);expect(content.status).toBe(200);expect(content.headers.get('Cache-Control')).toBe('private, no-store');expect(content.headers.get('Content-Disposition')).toContain('attachment');expect(new Uint8Array(await content.arrayBuffer())).toEqual(bytes);
 if(previous===undefined)delete process.env.ERPX_ATTACHMENTS_V2;else process.env.ERPX_ATTACHMENTS_V2=previous;
 session.actor={...f.head,companyId:'other'};expect((await get(result.id)).status).toBe(404);session.actor={...f.head,branchId:'other'};expect((await get(result.id)).status).toBe(404);session.actor={...f.head,role:'employee'};expect((await get(result.id)).status).toBe(404);session.actor=f.head;
 const input={expectedVersion:2,attachmentId:result.id,reason:'Wrong simulated screenshot replaced by corrected evidence'},revokeKey=key(),receipt=await revokeSupportEvidence(f.head,ticket,input,revokeKey);expect(await revokeSupportEvidence(f.head,ticket,input,revokeKey)).toEqual(receipt);
 expect((await get(result.id)).status).toBe(200);expect((await supportEvidence(f.head,ticket)).items).toHaveLength(1);expect((await supportEvidence(f.head,ticket)).items[0].revoked_at).not.toBeNull();
 expect((await supportDetail(f.head,ticket)).history[0]).toMatchObject({action:'attachment-revoke',version:3});
});
it('blocks every support-evidence mutation and hides write capabilities in explicit read-only rollback mode',async()=>{
 const previousGate=process.env.ERPX_ATTACHMENTS_V2,previousReadOnly=process.env.ERPX_ATTACHMENTS_READ_ONLY;process.env.ERPX_ATTACHMENTS_V2='false';delete process.env.ERPX_ATTACHMENTS_READ_ONLY;
 try{
  const reservation=(await (await intent()).json()).data,uploaded=(await (await upload(reservation.id)).json()).data;
  const staged=await seedAttachmentSet(db,f.owner,{targetType:'support_ticket',targetId:ticket,purposeKey:'internal_evidence',componentKind:'action',componentRef:'support-evidence:internal_evidence',expectedTargetVersion:2,compatibilityAdapter:true});
  expect((await supportEvidence(f.owner,ticket))).toMatchObject({canChange:true,canShare:true});
  process.env.ERPX_ATTACHMENTS_READ_ONLY='true';
  const intake=await intent(2);expect(intake.status).toBe(503);expect((await intake.json()).error.code).toBe('ATTACHMENTS_READ_ONLY');
  expect((await supportEvidence(f.owner,ticket))).toMatchObject({canChange:false,canShare:false});
  await expect(bindSupportEvidence(f.owner,ticket,{expectedVersion:2,attachmentSetIds:[staged.setId],reason:'Rollback must keep this evidence draft immutable'},key())).rejects.toMatchObject({status:503,code:'ATTACHMENTS_READ_ONLY'});
  await expect(revokeSupportEvidence(f.owner,ticket,{expectedVersion:2,attachmentId:uploaded.id,reason:'Rollback must preserve existing evidence'},key())).rejects.toMatchObject({status:503,code:'ATTACHMENTS_READ_ONLY'});
  expect(await db.prepare('SELECT version FROM support_tickets WHERE id=?').bind(ticket).first()).toEqual({version:2});
  expect(await db.prepare('SELECT revoked_at FROM attachments WHERE id=?').bind(uploaded.id).first()).toEqual({revoked_at:null});
  expect(await attachmentBindingState(db,staged.setId)).toMatchObject({set_state:'draft',link_state:'draft'});
 }
 finally{if(previousGate===undefined)delete process.env.ERPX_ATTACHMENTS_V2;else process.env.ERPX_ATTACHMENTS_V2=previousGate;if(previousReadOnly===undefined)delete process.env.ERPX_ATTACHMENTS_READ_ONLY;else process.env.ERPX_ATTACHMENTS_READ_ONLY=previousReadOnly;}
});
it('fails closed before allocating storage for an unknown legacy purpose',async()=>{
 const before=Number(await db.prepare('SELECT count(*) FROM attachment_sets WHERE target_type=? AND target_id=?').bind('support_ticket',ticket).first('count'));
 const response=await reserve(new Request('http://localhost:3100/api/v1/holdings/test/uploads',{method:'POST',headers:{'Content-Type':'application/json','Idempotency-Key':key()},body:JSON.stringify({ownerType:'support_ticket',ownerId:ticket,expectedVersion:1,purpose:'unregistered_support_export',filename:'simulation.pdf',mediaType:'application/pdf',bytes:bytes.length})}),{params:Promise.resolve({holdingSlug:'test'})});
 expect(response.status).toBe(422);expect((await response.json()).error.code).toBe('ATTACHMENT_PURPOSE_UNSUPPORTED');
 expect(Number(await db.prepare('SELECT count(*) FROM attachment_sets WHERE target_type=? AND target_id=?').bind('support_ticket',ticket).first('count'))).toBe(before);
});
it('rechecks source version and current assignment when completing a reserved upload',async()=>{
 const reserved=(await (await intent()).json()).data;
 await changeSupportTicket(f.head,ticket,{action:'start',expectedVersion:1,reason:'Work advanced after evidence selection'},key());
 expect((await upload(reserved.id)).status).toBe(409);expect((await supportEvidence(f.head,ticket)).items).toEqual([]);
 const next=(await (await intent(2)).json()).data;
 await db.prepare('UPDATE assignments SET revoked_at=? WHERE id=?').bind(Date.now(),f.head.assignmentId).run();
 expect((await upload(next.id)).status).toBe(404);expect((await supportEvidence(f.owner,ticket)).items).toEqual([]);
});
it('freezes evidence after proposed resolution and refuses signature mismatch or legal-hold revocation',async()=>{
 const reserved=(await (await intent()).json()).data;
 expect((await upload(reserved.id,new Uint8Array(bytes.length).fill(65))).status).toBe(422);
 const response=await upload(reserved.id),attachment=(await response.json()).data;
 await db.prepare('UPDATE attachments SET legal_hold=1 WHERE id=?').bind(attachment.id).run();
 await expect(revokeSupportEvidence(f.owner,ticket,{expectedVersion:2,attachmentId:attachment.id,reason:'Held evidence must remain readable'},key())).rejects.toMatchObject({code:'CONFLICT'});
 await changeSupportTicket(f.head,ticket,{action:'start',expectedVersion:2,reason:'Begin support work'},key());await changeSupportTicket(f.head,ticket,{action:'resolve',expectedVersion:3,reason:'Simulated proposed resolution'},key());
 expect((await intent(4)).status).toBe(409);expect((await supportEvidence(f.head,ticket)).canChange).toBe(false);expect((await get(attachment.id)).status).toBe(200);
 await expect(revokeSupportEvidence(f.owner,ticket,{expectedVersion:4,attachmentId:attachment.id,reason:'Cannot change reviewed evidence'},key())).rejects.toMatchObject({code:'SUPPORT_EVIDENCE_LOCKED'});
});
it('recovers the last reservation response at capacity without allocating an eleventh intent',async()=>{
 const keys=Array.from({length:10},()=>key());let lastId='';
 for(const k of keys){const r=await intent(1,k);expect(r.status).toBe(201);lastId=(await r.json()).data.id;}
 expect((await intent()).status).toBe(422);
 const replay=await intent(1,keys[9]);expect(replay.status).toBe(201);expect((await replay.json()).data.id).toBe(lastId);
 expect(await db.prepare("SELECT COUNT(*) n FROM upload_intents WHERE owner_type='support_ticket' AND owner_id=?").bind(ticket).first()).toEqual({n:10});
});
it('stages expense evidence for the domain bind instead of mutating the claim during upload',async()=>{
 const expense=await f.claim();session.actor=f.claimant;
 const r=await reserve(new Request('http://localhost:3100/api/v1/holdings/test/uploads',{method:'POST',headers:{'Content-Type':'application/json','Idempotency-Key':key()},body:JSON.stringify({ownerType:'expense_claim',ownerId:expense,purpose:'merchant_receipt',filename:'receipt.pdf',mediaType:'application/pdf',bytes:bytes.length})}),{params:Promise.resolve({holdingSlug:'test'})});
 expect(r.status).toBe(201);const reserved=(await r.json()).data;
 expect(await db.prepare('SELECT owner_version FROM upload_intents WHERE id=?').bind(reserved.id).first()).toEqual({owner_version:1});
 const content=await upload(reserved.id);expect(content.status).toBe(202);const attachment=(await content.json()).data;
 expect(attachment).toMatchObject({id:expect.any(String),sourceId:expect.any(String),attachmentSetId:reserved.attachmentSetId,state:'available'});
 expect(await db.prepare('SELECT state FROM attachment_links WHERE id=?').bind(reserved.attachmentLinkId).first('state')).toBe('draft');
 expect(await db.prepare('SELECT version FROM finance_expenses WHERE id=?').bind(expense).first()).toEqual({version:1});
 const bound=await bindRecord(new Request(`http://localhost:3100/api/v1/holdings/test/attachment-sets/${reserved.attachmentSetId}/bind`,{method:'POST',headers:{'Content-Type':'application/json','Idempotency-Key':key()},body:JSON.stringify({expectedTargetVersion:reserved.expectedTargetVersion})}),{params:Promise.resolve({holdingSlug:'test',setId:reserved.attachmentSetId})});
 expect(bound.status).toBe(200);expect(await db.prepare('SELECT state FROM attachment_links WHERE id=?').bind(reserved.attachmentLinkId).first('state')).toBe('bound');
 expect((await get(attachment.id)).status).toBe(200);
 expect(await db.prepare('SELECT version FROM finance_expenses WHERE id=?').bind(expense).first()).toEqual({version:2});
 expect(await getSupportTicket(f.owner,ticket)).toMatchObject({version:1});
});

it('hard-denies a guessed raw source without an authorized canonical link regardless of the UI gate',async()=>{
 const previous=process.env.ERPX_ATTACHMENTS_V2;process.env.ERPX_ATTACHMENTS_V2='false';
 try{
  const suffix=key(),uploadId=`raw-upload-${suffix}`,attachmentId=`raw-source-${suffix}`,objectKey=`h001/raw-regression/${suffix}`,now=Date.now(),checksum='a'.repeat(64);
  await env.H001_FILES.put(objectKey,bytes,{httpMetadata:{contentType:'application/pdf'},customMetadata:{sha256:checksum}});
  await db.batch([
   db.prepare("INSERT INTO upload_intents(id,owner_type,owner_id,purpose,original_name,media_type,declared_bytes,actual_bytes,checksum_sha256,staging_key,final_key,status,created_by,expires_at,created_at,updated_at,owner_version,company_id) VALUES(?,?,?,?,?,?,?,?,?,?,?,'ready',?,?,?,?,1,?)").bind(uploadId,'support_ticket',ticket,'internal_evidence','raw.pdf','application/pdf',bytes.length,bytes.length,checksum,objectKey,objectKey,f.head.userId,now+60_000,now,now,f.company),
   db.prepare("INSERT INTO attachments(id,upload_id,owner_type,owner_id,purpose,object_key,revision,original_name,media_type,bytes,checksum_sha256,created_by,created_at,company_id,detected_media_type,source_state,scanned_at,scanner_definition_at) VALUES(?,?,?,?,?,?,1,?,?,?,?,?,?,?,?, 'available',?,?)").bind(attachmentId,uploadId,'support_ticket',ticket,'internal_evidence',objectKey,'raw.pdf','application/pdf',bytes.length,checksum,f.head.userId,now,f.company,'application/pdf',now,now),
  ]);
  expect((await get(attachmentId)).status).toBe(404);
 }finally{if(previous===undefined)delete process.env.ERPX_ATTACHMENTS_V2;else process.env.ERPX_ATTACHMENTS_V2=previous;}
});

it('hides mutation controls for a manager whose current employment no longer permits case work',async()=>{
 expect((await supportEvidence(f.owner,ticket)).canChange).toBe(true);
 await db.prepare("UPDATE hr_engagements SET status='ended' WHERE company_id=? AND person_id IN (SELECT id FROM hr_people WHERE user_id=?)").bind(f.company,f.owner.userId).run();
 expect((await supportEvidence(f.owner,ticket)).canChange).toBe(false);
 session.actor=f.owner;expect((await intent()).status).toBe(409);
});
it('keeps internal files out of the portal and exposes only sanitized customer-shareable metadata',async()=>{
 const previous=process.env.ERPX_ATTACHMENTS_V2;process.env.ERPX_ATTACHMENTS_V2='true';
 try {
  const internal=await seedAttachmentSet(db,f.head,{targetType:'support_ticket',targetId:ticket,purposeKey:'internal_evidence',componentKind:'action',componentRef:'support-evidence:internal_evidence',expectedTargetVersion:1});
  await bindSupportEvidence(f.head,ticket,{expectedVersion:1,attachmentSetIds:[internal.setId],reason:'Internal investigation evidence for assigned Support staff'},key());
  const previewKey=`test/customer-preview/${crypto.randomUUID()}`,preview=new TextEncoder().encode('sanitized preview');
  await env.H001_FILES.put(previewKey,preview,{httpMetadata:{contentType:'image/webp',cacheControl:'private, no-store'},customMetadata:{sanitized:'true'}});
  const shared=await seedAttachmentSet(db,f.owner,{targetType:'support_ticket',targetId:ticket,purposeKey:'customer_shareable',componentKind:'action',componentRef:'support-evidence:customer_shareable',expectedTargetVersion:2,sourceState:'scanning'});
  await db.prepare("UPDATE attachments SET source_state='available',scanned_at=?,scanner_definition_at=?,preview_object_key=?,preview_media_type='image/webp',preview_checksum_sha256=?,preview_bytes=? WHERE id=?").bind(Date.now(),Date.now(),'test/customer-preview/'+previewKey.split('/').pop(),'0'.repeat(64),preview.byteLength,shared.sourceId).run();
  await bindSupportEvidence(f.owner,ticket,{expectedVersion:2,attachmentSetIds:[shared.setId],reason:'Manager reviewed this document for customer sharing'},key());
  expect(await attachmentBindingState(db,shared.setId)).toMatchObject({set_state:'bound',link_state:'bound',binding_type:'support_action'});
  await db.prepare("UPDATE commerce_orders SET state='confirmed' WHERE id=?").bind(orderId).run();
  const source=await db.prepare('SELECT company_id,party_id FROM commerce_orders WHERE id=?').bind(orderId).first<{company_id:string;party_id:string}>(),binding=`customer-share-${key()}`,email=`shared-${key()}@example.com`,userId=`portal-${key()}`,now=Date.now();
  await db.batch([
   db.prepare("INSERT INTO customer_access_bindings(id,company_id,order_id,customer_id,email,expires_at,status,reason,created_by,approved_by,reviewed_at,version,created_at,updated_at) VALUES(?,?,?,?,?,?,'approved',?,?,?,?,2,?,?)").bind(binding,source!.company_id,orderId,source!.party_id,email,now+86400000,'Reviewed customer document access',f.head.userId,f.owner.userId,now,now,now),
   db.prepare('INSERT INTO customer_access_identities(binding_id,user_id,email,bound_at) VALUES(?,?,?,?)').bind(binding,userId,email,now),
  ]);
  const visible=await customerPortalSharedFiles(db,{userId,email},[binding]);
  expect(visible.get(binding)).toEqual([{linkId:shared.linkId,label:'Shared support document',previewAvailable:true}]);
  expect(Object.keys(visible.get(binding)![0]).sort()).toEqual(['label','linkId','previewAvailable']);
  await expect(customerPortalSharedPreview(db,env.H001_FILES,{userId,email},internal.linkId)).rejects.toMatchObject({code:'CUSTOMER_SHARED_FILE_NOT_FOUND'});
  const result=await customerPortalSharedPreview(db,env.H001_FILES,{userId,email},shared.linkId);
  expect(result.mediaType).toBe('image/webp');expect(new Uint8Array(await result.object.arrayBuffer())).toEqual(preview);
  await expect(customerPortalSharedPreview(db,env.H001_FILES,{userId:'guessed-user',email},shared.linkId)).rejects.toMatchObject({code:'CUSTOMER_SHARED_FILE_NOT_FOUND'});
  await db.prepare("UPDATE customer_access_bindings SET status='revoked',version=version+1,updated_at=? WHERE id=?").bind(Date.now(),binding).run();
  await expect(customerPortalSharedPreview(db,env.H001_FILES,{userId,email},shared.linkId)).rejects.toMatchObject({code:'CUSTOMER_SHARED_FILE_NOT_FOUND'});
 } finally {if(previous===undefined)delete process.env.ERPX_ATTACHMENTS_V2;else process.env.ERPX_ATTACHMENTS_V2=previous;}
});
