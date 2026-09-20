import {beforeAll,beforeEach,it,expect,vi} from 'vitest';
import {applyMigrations,env} from './runtime-env';
import {expenseDb as db,expenseFixture,key} from './helpers/expense-fixture';
import type {WorkContext} from '../src/server/authorization';
import {saveMaster,saveOrder,orderSchema} from '../src/server/commerce';
import {createSupportTicket,changeSupportTicket,getSupportTicket,supportDetail} from '../src/server/support';
import {revokeSupportEvidence,supportEvidence} from '../src/server/support-evidence';
const session=vi.hoisted(()=>({actor:null as WorkContext|null}));
vi.mock('../src/server/authorization',async original=>({...await original<typeof import('../src/server/authorization')>(),requireWorkContext:async()=>session.actor!}));
vi.mock('../src/server/runtime',()=>({holdingResources:async()=>({write:env.H001_DB,files:env.H001_FILES}),runtimeEnv:async()=>env}));
import {POST as reserve} from '../src/app/api/v1/holdings/[holdingSlug]/uploads/route';
import {PUT as put} from '../src/app/api/v1/holdings/[holdingSlug]/uploads/[id]/content/route';
import {GET as download} from '../src/app/api/v1/holdings/[holdingSlug]/attachments/[id]/content/route';
let f:Awaited<ReturnType<typeof expenseFixture>>,ticket:string;
const bytes=new TextEncoder().encode('%PDF-1.4\nSimulation evidence, no real customer data\n%%EOF');
const ctx=(id:string)=>({params:Promise.resolve({holdingSlug:'test',id})});
beforeAll(async()=>{await applyMigrations(db,env.STAFF_TEST_MIGRATIONS);});
beforeEach(async()=>{
 f=await expenseFixture();f.head.role='support_agent';f.head.assignmentId=await f.grant(f.head,'support_agent');session.actor=f.head;
 const customer=await saveMaster(f.owner,'parties',null,{kind:'customer',code:key(),name:'Evidence customer'},key()),w=await saveMaster(f.owner,'warehouses',null,{branchId:f.branch,code:key(),name:'Evidence warehouse'},key()),p=await saveMaster(f.owner,'products',null,{sku:key(),name:'Evidence product',price:'10'},key()),order=await saveOrder(f.owner,null,orderSchema.parse({kind:'quote',partyId:customer.id,warehouseId:w.id,orderDate:'2026-09-20',dueDate:'2026-09-24',lines:[{productId:p.id,quantity:'1',price:'10',taxBps:0}]}),key());
 ticket=String((await createSupportTicket(f.owner,{orderId:order.id,sourceVersion:1,title:'Evidence support case',issue:'Customer issue with installation',ownerId:f.head.userId,dueDate:'2026-09-24'},key())).id);
});
async function intent(expectedVersion=1,k=key()) {return reserve(new Request('http://localhost:3100/api/v1/holdings/test/uploads',{method:'POST',headers:{'Content-Type':'application/json','Idempotency-Key':k},body:JSON.stringify({ownerType:'support_ticket',ownerId:ticket,expectedVersion,purpose:'support_evidence',filename:'simulation.pdf',mediaType:'application/pdf',bytes:bytes.length})}),{params:Promise.resolve({holdingSlug:'test'})});}
const upload=(id:string,data=bytes)=>put(new Request(`http://localhost:3100/api/v1/holdings/test/uploads/${id}/content`,{method:'PUT',headers:{'Content-Type':'application/pdf','Content-Length':String(data.length)},body:data}),ctx(id));
const get=(id:string)=>download(new Request(`http://localhost:3100/api/v1/holdings/test/attachments/${id}/content`),ctx(id));
it('uploads private evidence through existing routes, safely replays, preserves hashes/history and revokes access',async()=>{
 const k=key(),r=await intent(1,k);expect(r.status).toBe(201);const reserved=(await r.json()).data;expect((await (await intent(1,k)).json()).data.id).toBe(reserved.id);
 const response=await upload(reserved.id);expect(response.status).toBe(201);const result=(await response.json()).data;expect((await (await upload(reserved.id)).json()).data).toEqual(result);
 expect(await getSupportTicket(f.head,ticket)).toMatchObject({version:2,status:'open'});expect((await supportDetail(f.head,ticket)).history[0]).toMatchObject({action:'attachment-add',version:2});
 const content=await get(result.id);expect(content.status).toBe(200);expect(content.headers.get('Cache-Control')).toBe('private, no-store');expect(content.headers.get('Content-Disposition')).toContain('attachment');expect(new Uint8Array(await content.arrayBuffer())).toEqual(bytes);
 session.actor={...f.head,companyId:'other'};expect((await get(result.id)).status).toBe(404);session.actor={...f.head,branchId:'other'};expect((await get(result.id)).status).toBe(404);session.actor={...f.head,role:'employee'};expect((await get(result.id)).status).toBe(403);session.actor=f.head;
 const input={expectedVersion:2,attachmentId:result.id,reason:'Wrong simulated screenshot replaced by corrected evidence'},revokeKey=key(),receipt=await revokeSupportEvidence(f.head,ticket,input,revokeKey);expect(await revokeSupportEvidence(f.head,ticket,input,revokeKey)).toEqual(receipt);
 expect((await get(result.id)).status).toBe(404);expect((await supportEvidence(f.head,ticket)).items).toHaveLength(1);expect((await supportEvidence(f.head,ticket)).items[0].revoked_at).not.toBeNull();
 expect((await supportDetail(f.head,ticket)).history[0]).toMatchObject({action:'attachment-revoke',version:3});
});
it('rechecks source version and current assignment when completing a reserved upload',async()=>{
 const reserved=(await (await intent()).json()).data;
 await changeSupportTicket(f.head,ticket,{action:'start',expectedVersion:1,reason:'Work advanced after evidence selection'},key());
 expect((await upload(reserved.id)).status).toBe(409);expect((await supportEvidence(f.head,ticket)).items).toEqual([]);
 const next=(await (await intent(2)).json()).data;
 await db.prepare('UPDATE assignments SET revoked_at=? WHERE id=?').bind(Date.now(),f.head.assignmentId).run();
 expect((await upload(next.id)).status).toBe(409);expect((await supportEvidence(f.owner,ticket)).items).toEqual([]);
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
it('preserves the existing expense upload contract without a Support source version',async()=>{
 const expense=await f.claim();session.actor=f.claimant;
 const r=await reserve(new Request('http://localhost:3100/api/v1/holdings/test/uploads',{method:'POST',headers:{'Content-Type':'application/json','Idempotency-Key':key()},body:JSON.stringify({ownerType:'expense_claim',ownerId:expense,purpose:'merchant_receipt',filename:'receipt.pdf',mediaType:'application/pdf',bytes:bytes.length})}),{params:Promise.resolve({holdingSlug:'test'})});
 expect(r.status).toBe(201);const reserved=(await r.json()).data;
 expect(await db.prepare('SELECT owner_version FROM upload_intents WHERE id=?').bind(reserved.id).first()).toEqual({owner_version:null});
 const content=await upload(reserved.id);expect(content.status).toBe(201);const attachment=(await content.json()).data;
 expect((await get(attachment.id)).status).toBe(200);
 expect(await db.prepare('SELECT version FROM finance_expenses WHERE id=?').bind(expense).first()).toEqual({version:2});
 expect(await getSupportTicket(f.owner,ticket)).toMatchObject({version:1});
});

it('hides mutation controls for a manager whose current employment no longer permits case work',async()=>{
 expect((await supportEvidence(f.owner,ticket)).canChange).toBe(true);
 await db.prepare("UPDATE hr_engagements SET status='ended' WHERE company_id=? AND person_id IN (SELECT id FROM hr_people WHERE user_id=?)").bind(f.company,f.owner.userId).run();
 expect((await supportEvidence(f.owner,ticket)).canChange).toBe(false);
 session.actor=f.owner;expect((await intent()).status).toBe(409);
});
