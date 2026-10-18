import {expect,vi} from "vitest";
import {expenseDb as db,key,type expenseFixture} from "./expense-fixture";
import {env,applyMigrations} from "../runtime-env";
import {packageGrantStore} from "../../../src/server/modules/commercial/public/package-grants";
import {getRecord,recordsPage,searchReferences} from "../../../src/server/modules/records/public/records";
import {listStaffNotifications,notificationsPage} from "../../../src/server/modules/collaboration/public/staff-notifications";
import {publishInboxEvent,inboxPage} from "../../../src/server/modules/collaboration/public/inbox";
import {calendarData} from "../../../src/server/modules/work_projection/public/calendar/index";
import {attachmentBindingState,seedAttachmentSet} from "./attachment-v2";
import {attachmentContent,bindRecordAttachmentSet,createAttachmentSet} from "../../../src/server/modules/files/public/attachments/service";
type Fixture=Awaited<ReturnType<typeof expenseFixture>>;
function purchase(f:Fixture){
 const store=packageGrantStore(db,{authorize:async()=>({userId:"isolated-rfq-source-provider"}),obligations:async()=>{throw Error("No Procurement inactive witness is invented");}}),grant={companyId:f.company,packageId:"supply",packageVersion:1,state:"active" as const,startsAt:0,endsAt:null,version:1,reason:"Actual Procurement source history",evidence:"test:procurement-sources"};
 return {store,grant};
}
export async function verifyProcurementProjections(f:Fixture,rfqId:string,orderId:string){
 await applyMigrations(env.CONTROL_DB,env.CONTROL_TEST_MIGRATIONS);const recordsActor={...f.owner,modules:["procurement"]};
 const {store,grant}=purchase(f),shared=key(),now=Date.now();
 for(const [id,title,at] of [[shared,"Shared supplier reference",now],[rfqId,"Private original RFQ",now+1000]] as const)await db.prepare("INSERT INTO workspace_records(module_id,id,section_id,title,description,status,fields_json,company_id,created_by,updated_by,created_at,updated_at) VALUES('procurement',?,'purchase-requests',?,'','draft','[]',?,?,?,?,?)").bind(id,title,f.company,f.owner.userId,f.owner.userId,at,at).run();
 for(let index=0;index<106;index++){
  const id=index===0?rfqId:index===1?orderId:key(),type=index===1?"supplier_promise":"procurement_rfq",message=index<2?"Actual purchased source":"Missing RFQ must not consume the cursor";
  await db.prepare("INSERT INTO staff_notifications(id,company_id,user_id,record_type,record_id,record_version,message,created_at) VALUES(?,?,?,?,?,?,?,?)").bind(key(),f.company,f.owner.userId,type,id,index+100,message,now+index).run();
  await publishInboxEvent(db,{key:key(),companyId:f.company,sourceType:type,sourceId:id,sourceVersion:index+100,category:"work",preview:message,recipientIds:[f.owner.userId]});
 }
 const calendar=()=>calendarData(db,f.owner,{view:"month",date:"2026-10-01",mode:"team",layers:"procurement_rfqs,supplier_promises",timezone:"Asia/Bangkok"});
 vi.stubEnv("ERPX_COMMERCE_PACKAGE_ENFORCEMENT","enforced");
 expect((await recordsPage(recordsActor,"procurement","purchase-requests",{})).items.map(row=>row.id)).toEqual([shared]);expect((await searchReferences(recordsActor,"Private original RFQ")).some(row=>"id" in row&&row.id===rfqId)).toBe(false);await expect(getRecord(recordsActor,"procurement",rfqId)).rejects.toMatchObject({code:"PACKAGE_REQUIRED"});
 expect(await listStaffNotifications(db,f.owner)).toEqual([]);expect((await inboxPage(db,f.owner,{view:"all"})).items).toEqual([]);expect((await calendar()).items).toEqual([]);
 await store.apply({expectedVersion:0,idempotencyKey:key(),grant});await store.apply({expectedVersion:1,idempotencyKey:key(),grant:{...grant,state:"draining",version:2}});
 expect((await getRecord(recordsActor,"procurement",rfqId)).id).toBe(rfqId);expect((await searchReferences(recordsActor,"Private original RFQ")).flatMap(row=>"id" in row?[row.id]:[])).toContain(rfqId);
 const notices=await listStaffNotifications(db,f.owner);expect(notices.some(row=>row.record_type==="procurement_rfq"&&row.record_id===rfqId)).toBe(true);expect(notices.some(row=>row.record_type==="supplier_promise"&&row.record_id===orderId)).toBe(true);expect(notices.some(row=>row.message.includes("Missing RFQ"))).toBe(false);expect((await notificationsPage(db,f.owner,{view:"all"})).items.some(row=>row.record_id===rfqId)).toBe(true);
 const inbox=await inboxPage(db,f.owner,{view:"all"});expect(inbox.items.some(row=>row.sourceId===rfqId)).toBe(true);expect(inbox.items.some(row=>row.sourceId===orderId)).toBe(true);expect(inbox.items.some(row=>row.message.includes("Missing RFQ"))).toBe(false);expect((await calendar()).items.map(row=>row.sourceId)).toEqual(expect.arrayContaining([rfqId,orderId]));
 await db.prepare("UPDATE assignments SET revoked_at=1,version=version+1 WHERE id=?").bind(f.owner.assignmentId).run();expect(await listStaffNotifications(db,f.owner)).toEqual([]);expect((await inboxPage(db,f.owner,{view:"all"})).items).toEqual([]);await expect(getRecord(recordsActor,"procurement",rfqId)).rejects.toMatchObject({status:404});
}
export async function verifyRfqFiles(f:Fixture,rfqId:string){
 const {store,grant}=purchase(f),files=await seedAttachmentSet(db,f.owner,{targetType:"procurement_rfq",targetId:rfqId,purposeKey:"supplier_quotation",componentKind:"record",expectedTargetVersion:1}),objectKey=String(await db.prepare("SELECT object_key FROM attachments WHERE id=?").bind(files.sourceId).first("object_key")),bytes="%PDF-1.7 retained supplier quotation";
 await env.H001_FILES.put(objectKey,bytes,{httpMetadata:{contentType:"application/pdf"}});await db.prepare("UPDATE attachments SET legal_hold=1 WHERE id=?").bind(files.sourceId).run();vi.stubEnv("ERPX_COMMERCE_PACKAGE_ENFORCEMENT","enforced");
 await expect(bindRecordAttachmentSet(db,f.owner,files.setId,1,key())).rejects.toMatchObject({code:"PACKAGE_REQUIRED"});await expect(attachmentContent(db,env.H001_FILES,f.owner,files.linkId,"original")).rejects.toMatchObject({code:"PACKAGE_REQUIRED"});await store.apply({expectedVersion:0,idempotencyKey:key(),grant});
 const tables=["procurement_rfqs","assignments","attachments","attachment_sets","attachment_links","attachment_events","command_receipts","audit_events","outbox_events"],snapshot=()=>Promise.all(tables.map(async table=>(await db.prepare(`SELECT row_to_json(t)::text row FROM ${table} t ORDER BY row_to_json(t)::text`).all()).results));
 for(const change of ["source","authority"] as const){
  const command=key(),fault=`rfq_file_${key().replaceAll('-','')}`,before=await snapshot(),effect=change==="source"?`UPDATE procurement_rfqs SET status='cancelled' WHERE id='${rfqId}'`:`UPDATE assignments SET revoked_at=1,version=version+1 WHERE id='${f.owner.assignmentId}'`;
  await db.exec(`CREATE FUNCTION ${fault}() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN ${effect};RETURN NEW;END $$;CREATE CONSTRAINT TRIGGER ${fault} AFTER INSERT ON audit_events DEFERRABLE INITIALLY DEFERRED FOR EACH ROW WHEN(NEW.command_id='${command}') EXECUTE FUNCTION ${fault}()`);
  try{let failure:unknown;try{await bindRecordAttachmentSet(db,f.owner,files.setId,1,command);}catch(error){failure=error;}expect(failure).toBeInstanceOf(Error);expect((failure as {cause?:unknown}).cause??failure).toMatchObject({code:"23514",detail:expect.stringContaining(change==="source"?"rfq-file-owner":"iam-current-authority")});expect(await snapshot()).toEqual(before);}finally{await db.exec(`DROP TRIGGER ${fault} ON audit_events;DROP FUNCTION ${fault}()`);}
 }
 const command=key(),bound=await bindRecordAttachmentSet(db,f.owner,files.setId,1,command);expect(await bindRecordAttachmentSet(db,f.owner,files.setId,1,command)).toEqual(bound);expect(await attachmentBindingState(db,files.setId)).toMatchObject({set_state:"bound",link_state:"bound"});
 await store.apply({expectedVersion:1,idempotencyKey:key(),grant:{...grant,state:"draining",version:2}});expect(await (await attachmentContent(db,env.H001_FILES,f.owner,files.linkId,"original")).object.text()).toBe(bytes);await expect(createAttachmentSet(db,f.owner,{targetType:"procurement_rfq",targetId:rfqId,purposeKey:"supplier_quotation",componentKind:"record",expectedTargetVersion:2},key())).rejects.toMatchObject({code:"PACKAGE_DRAINING"});
 await db.prepare("UPDATE assignments SET revoked_at=1,version=version+1 WHERE id=?").bind(f.owner.assignmentId).run();await expect(attachmentContent(db,env.H001_FILES,f.owner,files.linkId,"original")).rejects.toMatchObject({status:404});expect(await (await env.H001_FILES.get(objectKey))!.text()).toBe(bytes);expect(await db.prepare("SELECT legal_hold,revoked_at FROM attachments WHERE id=?").bind(files.sourceId).first()).toEqual({legal_hold:1,revoked_at:null});
}

/** Actual consolidated supplier bills share the same retained Supply source on
 * generic records and calendar, with Finance authority selected independently. */
export async function verifySupplierBillProjections(f:Fixture,billId:string){
 await applyMigrations(env.CONTROL_DB,env.CONTROL_TEST_MIGRATIONS);
 const actor={...f.reviewer,modules:["procurement"]},now=Date.now();
 await db.prepare("INSERT INTO workspace_records(module_id,id,section_id,title,description,status,fields_json,company_id,created_by,updated_by,created_at,updated_at) VALUES('procurement',?,'purchase-requests','Private consolidated supplier bill','','draft','[]',?,?,?,?,?)").bind(billId,f.company,actor.userId,actor.userId,now,now).run();
 expect((await getRecord(actor,"procurement",billId)).id).toBe(billId);
 expect((await searchReferences(actor,"Private consolidated supplier bill")).flatMap(row=>"id" in row?[row.id]:[])).toContain(billId);
 expect((await calendarData(db,actor,{view:"month",date:"2026-11-01",mode:"team",layers:"supplier_bills",timezone:"Asia/Bangkok"})).items.map(row=>row.sourceId)).toContain(billId);
}
export async function verifySupplierBankProjections(f:Fixture,billId:string,batchId:string){
 const actor=f.owner,now=Date.now();
 for(let index=0;index<106;index++){
  const type=index%2?"supplier_batch":"supplier_bill",id=index===0?billId:index===1?batchId:key(),message=index<2?"Actual retained supplier source":"Missing supplier source must not consume cursor";
  await db.prepare("INSERT INTO staff_notifications(id,company_id,user_id,record_type,record_id,record_version,message,created_at) VALUES(?,?,?,?,?,?,?,?)").bind(key(),f.company,actor.userId,type,id,index+100,message,now+index).run();
  await publishInboxEvent(db,{key:key(),companyId:f.company,sourceType:type,sourceId:id,sourceVersion:index+100,category:"work",preview:message,recipientIds:[actor.userId]});
 }
 const notices=await listStaffNotifications(db,actor);expect(notices.some(row=>row.record_id===billId)).toBe(true);expect(notices.some(row=>row.record_id===batchId)).toBe(true);expect(notices.some(row=>row.message.includes("Missing supplier"))).toBe(false);
 const inbox=await inboxPage(db,actor,{view:"all"});expect(inbox.items.some(row=>row.sourceId===billId)).toBe(true);expect(inbox.items.some(row=>row.sourceId===batchId)).toBe(true);expect(inbox.items.some(row=>row.message.includes("Missing supplier"))).toBe(false);
 expect((await calendarData(db,actor,{view:"month",date:"2026-11-01",mode:"team",layers:"supplier_batches",timezone:"Asia/Bangkok"})).items.map(row=>row.sourceId)).toContain(batchId);
 await db.prepare("UPDATE assignments SET revoked_at=1,version=version+1 WHERE id=?").bind(actor.assignmentId).run();
 expect((await listStaffNotifications(db,actor)).some(row=>[billId,batchId].includes(row.record_id))).toBe(false);expect((await inboxPage(db,actor,{view:"all"})).items.some(row=>[billId,batchId].includes(row.sourceId))).toBe(false);
 await expect(getRecord({...actor,modules:["procurement"]},"procurement",billId)).rejects.toMatchObject({status:404});
}
export async function verifySupplierBillFiles(f:Fixture,billId:string){
 const version=Number(await db.prepare("SELECT version FROM procurement_bill_matches WHERE id=?").bind(billId).first("version")),files=await seedAttachmentSet(db,f.reviewer,{targetType:"supplier_bill_match",targetId:billId,purposeKey:"invoice_document",componentKind:"record",expectedTargetVersion:version}),objectKey=String(await db.prepare("SELECT object_key FROM attachments WHERE id=?").bind(files.sourceId).first("object_key")),bytes="%PDF-1.7 retained consolidated supplier invoice";
 await env.H001_FILES.put(objectKey,bytes,{httpMetadata:{contentType:"application/pdf"}});await db.prepare("UPDATE attachments SET legal_hold=1 WHERE id=?").bind(files.sourceId).run();
 const command=key(),fault=`supplier_bill_file_${key().replaceAll('-','')}`,tables=["commerce_fulfillments","procurement_bill_matches","attachments","attachment_sets","attachment_links","attachment_events","command_receipts","audit_events","outbox_events"],snapshot=()=>Promise.all(tables.map(async table=>(await db.prepare(`SELECT row_to_json(t)::text row FROM ${table} t ORDER BY row_to_json(t)::text`).all()).results)),before=await snapshot();
 await db.exec(`CREATE FUNCTION ${fault}() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN UPDATE commerce_fulfillments SET invoice_id=NULL WHERE invoice_id=(SELECT invoice_id FROM procurement_bill_matches WHERE id='${billId}');RETURN NEW;END $$;CREATE CONSTRAINT TRIGGER ${fault} AFTER INSERT ON audit_events DEFERRABLE INITIALLY DEFERRED FOR EACH ROW WHEN(NEW.command_id='${command}') EXECUTE FUNCTION ${fault}()`);
 try{let failure:unknown;try{await bindRecordAttachmentSet(db,f.reviewer,files.setId,version,command);}catch(error){failure=error;}expect(failure).toBeInstanceOf(Error);expect((failure as {cause?:unknown}).cause??failure).toMatchObject({code:"23514",detail:expect.stringContaining("bill-file-owner")});expect(await snapshot()).toEqual(before);}finally{await db.exec(`DROP TRIGGER ${fault} ON audit_events;DROP FUNCTION ${fault}()`);}
 const bound=await bindRecordAttachmentSet(db,f.reviewer,files.setId,version,command);expect(await bindRecordAttachmentSet(db,f.reviewer,files.setId,version,command)).toEqual(bound);expect(await attachmentBindingState(db,files.setId)).toMatchObject({set_state:"bound",link_state:"bound"});expect(await (await attachmentContent(db,env.H001_FILES,f.reviewer,files.linkId,"original")).object.text()).toBe(bytes);expect(await db.prepare("SELECT legal_hold,revoked_at FROM attachments WHERE id=?").bind(files.sourceId).first()).toEqual({legal_hold:1,revoked_at:null});
}
