import {calendarQuerySchema} from "../src/lib/calendar";
import {beforeAll,beforeEach,expect,it,vi} from "vitest";
import {PDFDocument} from "pdf-lib";
import {env,applyMigrations} from "./runtime-env";
import {seedExpenseCompany} from "../fixtures/expenses/company";
import {legalCommand,detail,legalData} from "../src/server/legal/service";
import {access} from "../src/server/legal/access";
import {legalPeople} from "../src/server/legal/assignees";
import {importExecutedCopy,inspectDocument} from "../src/server/legal/documents";
import {executedCopies,reviewExecutedCopy} from "../src/server/legal/executed-copies";
import {legalCalendar,legalWork} from "../src/server/legal/integrations";
vi.mock("../src/server/modules/files/public/attachments/scanner",()=>({
  attachmentScannerReadiness:async()=>({enabled:true,ready:true,engine:"simulated scanner",definitionUpdatedAt:Date.now(),error:null}),
  scanAttachmentBytes:async(bytes:Uint8Array)=>({clean:!Buffer.from(bytes).includes(Buffer.from("EICAR")),signature:Buffer.from(bytes).includes(Buffer.from("EICAR"))?"EICAR":null,engine:"simulated scanner",definitionUpdatedAt:Date.now()}),
}));
vi.mock("../src/server/modules/files/public/attachments/preview",()=>({attachmentPreviewReadiness:async()=>({ready:true,error:null}),detectedAttachmentMediaType:(_bytes:Uint8Array,type:string)=>type,sanitizedAttachmentPreview:async()=>null}));
const db=env.H001_DB,key=()=>crypto.randomUUID();
let f:Awaited<ReturnType<typeof seedExpenseCompany>>;
beforeAll(()=>applyMigrations(db,env.STAFF_TEST_MIGRATIONS));
beforeEach(async()=>{f=await seedExpenseCompany(db,env.H001_FILES);f.manager=await f.member("legal-manager","legal_manager");f.tie=await f.member("legal-reviewer","legal_counsel");await legalCommand(db,f.manager,{action:"settings",enabled:true},key());});
const input=(extra={})=>({kind:"contract",title:"Thai fixture contract",ownerId:f.manager.userId,data:{contractType:"Supply",parties:"Company and Supplier"},...extra});
async function command(id:string,action:string,extra={},actor=f.manager){return legalCommand(db,actor,{id,action,expectedVersion:(await access(db,actor,id)).version,...extra},key());}
async function approved(extra={}){
  const r=await legalCommand(db,f.manager,{action:"create",item:input(extra)},key());
  await command(r.id,"document",{name:"Authority",body:"Fictional authority evidence"});
  const authorityId=(await detail(db,f.manager,r.id)).documents[0].id;
  await command(r.id,"document",{name:"Approved draft",body:"Mutually approved Thai terms"});
  const approvedDocumentId=(await detail(db,f.manager,r.id)).documents[0].id;
  await command(r.id,"member",{userId:f.tie.userId,access:"read"});
  await command(r.id,"transition",{state:"review",reason:"Review fictional terms"});
  await command(r.id,"transition",{state:"approval",reason:"Independent approval"});
  await command(r.id,"approve",{documentId:approvedDocumentId,signerIds:[f.manager.userId],reason:"Approve fictional terms"},f.tie);
  const pdf=await PDFDocument.create();pdf.addPage();const bytes=await pdf.save();
  const metadata={approvedDocumentId,signedDate:"2026-01-01",parties:["Company","Supplier"],signers:[{name:"Fictional representative",party:"Company",authorityDocumentIds:[authorityId]}],declaredPageCount:1,notes:"Fictional externally signed scan"};
  return {id:r.id,bytes,metadata};
}
const checks={contentMatches:true,pagesComplete:true,partiesVerified:true,signaturesComplete:true,authorityVerified:true};
it("returns the persisted inspection identity and checksum for human text review",async()=>{
  const r=await legalCommand(db,f.manager,{action:"create",item:input()},key());
  await command(r.id,"document",{name:"Thai evidence",body:"หลักฐานภาษาไทยสำหรับการทบทวน"});
  const d=(await detail(db,f.manager,r.id)).documents[0];
  const result=await inspectDocument(db,env.H001_FILES,f.manager,d.id,"ocr");
  expect(result).toMatchObject({checksum:d.checksum,requiresReview:true,pageMapping:false,pages:[{page:1,text:d.body}]});
  expect(typeof result.inspectionId).toBe("string");
  expect(JSON.parse((await db.prepare("SELECT result_json FROM legal_document_inspections WHERE id=?").bind(result.inspectionId).first<string>("result_json"))!)).toEqual(result);
});
async function importCopy(p:Awaited<ReturnType<typeof approved>>,k=key()){
  return importExecutedCopy(db,env.H001_FILES,f.manager,p.id,(await access(db,f.manager,p.id)).version,p.metadata,"Signed original.pdf","application/pdf",p.bytes,k);
}
async function review(id:string,executedCopyId:string,extra={},actor=f.tie){return reviewExecutedCopy(db,actor,{action:"executed.review",id,expectedVersion:(await access(db,actor,id)).version,executedCopyId,decision:"confirm",checks,reason:"Independently inspected all pages and signing authority",...extra},key());}
it("imports a different-hash signed original, reviews independently, preserves revisions and replays atomically",async()=>{
  const p=await approved(),version=(await access(db,f.manager,p.id)).version,k=key();
  const imported=await importExecutedCopy(db,env.H001_FILES,f.manager,p.id,version,p.metadata,"Signed original.pdf","application/pdf",p.bytes,k);
  expect(await importExecutedCopy(db,env.H001_FILES,f.manager,p.id,version,p.metadata,"Signed original.pdf","application/pdf",p.bytes,k)).toEqual(imported);
  let copies=await executedCopies(db,f.manager,p.id);
  expect(copies).toHaveLength(1);expect(copies[0].state).toBe("pending_review");expect(copies[0].approved_hash).not.toBe(copies[0].executed_hash);
  await expect(db.prepare("UPDATE legal_executed_copies SET state='confirmed',reviewed_by=?,reviewed_at=? WHERE id=?").bind(f.tie.userId,Date.now(),copies[0].id).run()).rejects.toThrow("LEGAL_EXECUTED_REVIEW_REQUIRED");
  await expect(review(p.id,copies[0].id,{},f.manager)).rejects.toMatchObject({code:"LEGAL_INDEPENDENT_REVIEW"});
  const body={action:"executed.review",id:p.id,expectedVersion:(await access(db,f.tie,p.id)).version,executedCopyId:copies[0].id,decision:"confirm",checks,reason:"Independently inspected all pages and signing authority"},reviewKey=key();
  const fault=`legal_executed_late_${crypto.randomUUID().replaceAll("-","")}`;
  await db.exec(`CREATE FUNCTION ${fault}() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'LEGAL_EXECUTED_LATE_OUTBOX' USING ERRCODE='23514';END;$$;CREATE TRIGGER ${fault} AFTER INSERT ON outbox_events FOR EACH ROW WHEN (NEW.event_type='legal.executed.review') EXECUTE FUNCTION ${fault}();`);
  try {
    await expect(reviewExecutedCopy(db,f.tie,body,reviewKey)).rejects.toMatchObject({code:"23514"});
    expect((await access(db,f.manager,p.id))).toMatchObject({version:body.expectedVersion,state:"approved"});
    expect((await executedCopies(db,f.manager,p.id))[0]).toMatchObject({state:"pending_review",reviewed_by:null,version:1,reviews:[]});
    expect(await db.prepare("SELECT command_id FROM command_receipts WHERE command_id=?").bind(reviewKey).first()).toBeNull();
    expect(await db.prepare("SELECT id FROM audit_events WHERE command_id=?").bind(reviewKey).first()).toBeNull();
    expect(await db.prepare("SELECT id FROM outbox_events WHERE command_id=?").bind(reviewKey).first()).toBeNull();
  } finally {await db.exec(`DROP TRIGGER ${fault} ON outbox_events;DROP FUNCTION ${fault}();`);}
  const accepted=await reviewExecutedCopy(db,f.tie,body,reviewKey);
  expect(await reviewExecutedCopy(db,f.tie,body,reviewKey)).toEqual(accepted);
  expect((await access(db,f.manager,p.id)).state).toBe("executed");
  await command(p.id,"transition",{state:"active",reason:"Track approved contractual obligations"});
  copies=await executedCopies(db,f.manager,p.id);expect(copies[0].state).toBe("confirmed");expect(copies[0].reviews).toHaveLength(1);
  await expect(db.prepare("UPDATE legal_executed_copies SET metadata='{}' WHERE id=?").bind(copies[0].id).run()).rejects.toThrow("LEGAL_EXECUTED_COPY_IMMUTABLE");
  await importCopy(p);expect(await executedCopies(db,f.manager,p.id)).toHaveLength(2);
  expect((await executedCopies(db,f.manager,p.id)).filter(c=>c.state==="confirmed")).toHaveLength(1);
  expect(await db.prepare("SELECT count(*) n FROM audit_events WHERE record_id=? AND action='legal.executed.review'").bind(p.id).first("n")).toBe(1);
  expect(await db.prepare("SELECT count(*) n FROM outbox_events WHERE aggregate_id=? AND event_type='legal.executed.review'").bind(p.id).first("n")).toBe(1);
});
it("keeps incomplete or changed signed copies awaiting correction and excludes cross-company review",async()=>{
  const p=await approved();await importCopy(p);const e=(await executedCopies(db,f.manager,p.id))[0];
  await expect(review(p.id,e.id,{checks:{...checks,pagesComplete:false}})).rejects.toMatchObject({code:"LEGAL_EXECUTED_INCOMPLETE"});
  await review(p.id,e.id,{decision:"changes_required",checks:{...checks,contentMatches:false},reason:"The scanned version changes the approved payment terms"});
  expect((await executedCopies(db,f.manager,p.id))[0].state).toBe("changes_required");expect((await access(db,f.manager,p.id)).state).toBe("approved");
  const other=await seedExpenseCompany(db,env.H001_FILES);
  await expect(executedCopies(db,other.owner,p.id)).rejects.toMatchObject({code:"LEGAL_NOT_FOUND"});
  await command(p.id,"member",{userId:f.tie.userId,access:"remove"});
  await expect(review(p.id,e.id)).rejects.toMatchObject({code:"LEGAL_NOT_FOUND"});
});
it.each(["membership","assignment"] as const)("refuses an executed review replay after current %s is withdrawn on an internal contract",async(withdrawal)=>{
  const p=await approved({classification:"internal"});await importCopy(p);
  const e=(await executedCopies(db,f.tie,p.id))[0];
  const body={action:"executed.review",id:p.id,expectedVersion:(await access(db,f.tie,p.id)).version,executedCopyId:e.id,decision:"confirm",checks,reason:"Independently inspected all pages and signing authority"},reviewKey=key();
  await reviewExecutedCopy(db,f.tie,body,reviewKey);
  if(withdrawal==="membership")await command(p.id,"member",{userId:f.tie.userId,access:"remove"});
  else await db.prepare("UPDATE assignments SET revoked_at=? WHERE id=?").bind(Date.now(),f.tie.assignmentId).run();
  await expect(reviewExecutedCopy(db,f.tie,body,reviewKey)).rejects.toMatchObject({code:withdrawal==="membership"?"LEGAL_REVIEWER_UNAVAILABLE":"LEGAL_PERSON_UNAVAILABLE"});
  expect(await db.prepare("SELECT count(*) n FROM legal_executed_reviews WHERE executed_copy_id=?").bind(e.id).first("n")).toBe(1);
});
it("refuses malformed, oversized, infected and stale executed imports before accepting evidence",async()=>{
  const p=await approved();const version=(await access(db,f.manager,p.id)).version;
  const attempt=(bytes:Uint8Array,metadata:unknown=p.metadata)=>importExecutedCopy(db,env.H001_FILES,f.manager,p.id,version,metadata,"Scan.pdf","application/pdf",bytes,key());
  await expect(attempt(new Uint8Array(10*1024*1024+1))).rejects.toMatchObject({code:"LEGAL_INVALID"});
  await expect(attempt(Buffer.from("not a PDF"))).rejects.toMatchObject({code:"LEGAL_INVALID"});
  await expect(attempt(Buffer.from("%PDF- EICAR"))).rejects.toMatchObject({code:"LEGAL_UPLOAD_REJECTED"});
  await expect(attempt(p.bytes,{...p.metadata,approvedDocumentId:"other-version"})).rejects.toMatchObject({code:"LEGAL_APPROVAL_STALE"});
  expect(await executedCopies(db,f.manager,p.id)).toHaveLength(0);
});
it("filters assignees by active Company and exact unit and repeats the check on save",async()=>{
  const branchReviewer=await f.member("branch-reviewer","legal_counsel");
  await db.prepare("UPDATE assignments SET scope_kind='branch',scope_node_id=? WHERE id=?").bind(f.branch,branchReviewer.assignmentId).run();
  expect((await legalPeople(db,f.manager)).map(p=>p.id)).not.toContain(branchReviewer.userId);
  expect((await legalPeople(db,{...f.manager,branchId:f.branch},{purpose:"reviewer"})).map(p=>p.id)).toContain(branchReviewer.userId);
  await expect(legalCommand(db,f.manager,{action:"create",item:input({ownerId:branchReviewer.userId})},key())).rejects.toMatchObject({code:"LEGAL_PERSON_UNAVAILABLE"});
  await db.prepare("INSERT INTO iam_members(company_id,user_id,principal_id,status) VALUES(?,?,?,'suspended')").bind(f.manager.companyId,branchReviewer.userId,branchReviewer.userId).run();
  expect((await legalPeople(db,{...f.manager,branchId:f.branch})).map(p=>p.id)).not.toContain(branchReviewer.userId);
  await expect(legalCommand(db,{...f.manager,branchId:f.branch},{action:"create",item:input({ownerId:branchReviewer.userId})},key())).rejects.toMatchObject({code:"LEGAL_PERSON_UNAVAILABLE"});
});
it("lets requesters submit and reply to their own routed request without legal operator powers",async()=>{
  const requester=await f.member("requester","legal_requester");
  const item={kind:"request",title:"Question about a supplier",dueDate:"2026-10-02",ownerId:requester.userId,data:{requestType:"Advice",question:"Please review this supplier condition"}};
  const r=await legalCommand(db,requester,{action:"create",item},key());
  expect((await detail(db,f.manager,r.id)).members.some(m=>m.user_id===f.manager.userId&&m.access==="edit")).toBe(true);
  const submit={id:r.id,action:"transition",expectedVersion:(await access(db,requester,r.id)).version,state:"review",reason:"Please triage my question"},submitKey=key();
  const submitted=await legalCommand(db,requester,submit,submitKey);
  expect(await legalCommand(db,requester,submit,submitKey)).toEqual(submitted);
  await command(r.id,"assign",{ownerId:f.tie.userId,reason:"Counsel assigned after triage"});
  expect((await access(db,f.manager,r.id)).owner_id).toBe(f.tie.userId);
  await command(r.id,"comment",{body:"Additional information requested by Legal"},requester);
  await expect(command(r.id,"member",{userId:f.tie.userId,access:"edit"},requester)).rejects.toMatchObject({status:403});
  await expect(command(r.id,"assign",{ownerId:requester.userId,reason:"Unauthorized reassignment"},requester)).rejects.toMatchObject({code:"LEGAL_REQUESTER_SCOPE"});
  await expect(command(r.id,"update",{item:{...item,data:{...item.data,advice:"Self-approved legal advice"}}},requester)).rejects.toMatchObject({status:403});
  const other=await legalCommand(db,f.manager,{action:"create",item:input({classification:"internal"})},key());
  await expect(detail(db,requester,other.id)).rejects.toMatchObject({code:"LEGAL_NOT_FOUND"});
  expect((await legalData(db,requester)).total).toBe(1);
  expect((await legalWork(db,requester)).map(w=>w.id)).toEqual([r.id]);
  expect((await legalCalendar(db,requester,calendarQuerySchema.parse({mode:"team",search:""}),"2026-10-01","2026-11-01",100)).map(w=>w.sourceId)).toEqual([r.id]);
  await expect(legalCommand(db,requester,{action:"create",item:input({ownerId:requester.userId})},key())).rejects.toMatchObject({code:"LEGAL_REQUESTER_SCOPE"});
  await db.prepare("UPDATE assignments SET revoked_at=? WHERE id=?").bind(Date.now(),f.manager.assignmentId).run();
  await expect(legalCommand(db,requester,{action:"create",item},key())).rejects.toMatchObject({code:"LEGAL_ROUTING_UNAVAILABLE"});
});
