import {
  afterAll,
  beforeAll,
  beforeEach,
  describe,
  expect,
  it,
  vi,
} from "vitest";
import { Document, Packer, Paragraph } from "docx";
import { applyMigrations, env } from "../../support/runtime-env";
const attachmentRuntime = vi.hoisted(() => ({
  scannerReady: true,
  previewReady: true,
}));
vi.mock("@erpx/server-runtime/adapters/runtime", () => ({
  holdingResources: async () => ({ write: env.H001_DB, files: env.H001_FILES }),
  runtimeEnv: async () => env,
}));
vi.mock("@erpx/server-files/public/attachments/scanner", () => ({
  attachmentScannerReadiness: async () => ({
    enabled: true,
    ready: attachmentRuntime.scannerReady,
    engine: "ClamAV specialized-adapter test",
    definitionUpdatedAt: Date.now(),
    error: null,
  }),
  scanAttachmentBytes: async () => ({
    clean: true,
    signature: null,
    engine: "ClamAV specialized-adapter test",
    definitionUpdatedAt: Date.now(),
  }),
}));
vi.mock("@erpx/server-files/public/attachments/preview", async () => {
  const actual = await vi.importActual<
    typeof import("@erpx/server-files/public/attachments/preview")
  >("@erpx/server-files/public/attachments/preview");
  return {
    ...actual,
    attachmentPreviewReadiness: async () => ({
      ready: attachmentRuntime.previewReady,
      heifConvert: "/test/heif-convert",
      pdftoppm: "/test/pdftoppm",
      error: null,
    }),
    sanitizedAttachmentPreview: async () => null,
  };
});
import {
  expenseDb as db,
  expenseFixture,
  key,
} from "../../support/helpers/expense-fixture";
import {
  attachmentBindingState,
  seedAttachmentSet,
} from "../../support/helpers/attachment-v2";
import {
  createMessagingTeam,
  messageSchema,
  messagingFile,
  messagingRoom,
  sendMessagingMessage,
} from "@erpx/server-collaboration/public/messaging";
import {
  attachmentContent,
  bindRecordAttachmentSet,
} from "@erpx/server-files/public/attachments/service";
import {
  attachmentPolicy,
} from "@erpx/server-files/public/attachments/policy";
import { getExpense } from "@erpx/server-people_payroll/public/expenses";
import { expenseRequirements } from "@erpx/server-people_payroll/public/expense-drafts";
import { legalCommand, detail as legalDetail } from "@erpx/server-legal/public/legal/service";
import { access as legalAccess } from "@erpx/server-legal/public/legal/access";
import { portalUpload } from "@erpx/server-legal/public/legal/portal";
import {
  documentExport,
  externalSubmission,
  uploadDocument,
} from "@erpx/server-legal/public/legal/documents";

let fixture: Awaited<ReturnType<typeof expenseFixture>>;

beforeAll(async () => {
  await applyMigrations(db, env.STAFF_TEST_MIGRATIONS);
  await applyMigrations(env.CONTROL_DB, env.CONTROL_TEST_MIGRATIONS);
  process.env.ERPX_ATTACHMENTS_V2 = "true";
});

afterAll(() => {
  delete process.env.ERPX_ATTACHMENTS_V2;
});

beforeEach(async () => {
  fixture = await expenseFixture(false);
  for (const actor of [
    fixture.claimant,
    fixture.head,
    fixture.owner,
    fixture.reviewer,
  ])
    await env.CONTROL_DB.prepare(
      'INSERT INTO "user"(id,name,email,emailVerified,createdAt,updatedAt) VALUES(?,?,?,1,1,1)',
    )
      .bind(actor.userId, actor.name, `${actor.userId}@example.invalid`)
      .run();
});

async function messageFile(
  roomId: string,
  sourceState: "available" | "quarantined",
  componentRef?: string,
) {
  const room = await messagingRoom(db, fixture.claimant, roomId);
  const fileId = key();
  const canonical = await seedAttachmentSet(db, fixture.claimant, {
    targetType: "messaging_room",
    targetId: roomId,
    componentKind: "message",
    componentRef: componentRef ?? fileId,
    purposeKey: "message_attachment",
    expectedTargetVersion: Number(room.version),
    sourceState,
    mediaType: "text/csv",
  });
  const source = await db
    .prepare(
      "SELECT object_key,checksum_sha256 FROM attachments WHERE id=?",
    )
    .bind(canonical.sourceId)
    .first<{ object_key: string; checksum_sha256: string }>();
  await db
    .prepare(
      `INSERT INTO messaging_files(
        id,room_id,created_by,original_name,media_type,bytes,object_key,checksum,created_at,
        attachment_set_id,attachment_link_id,attachment_source_id
      ) VALUES(?,?,?,?,?,?,?,?,?,?,?,?)`,
    )
    .bind(
      fileId,
      roomId,
      fixture.claimant.userId,
      "evidence.csv",
      "text/csv",
      8,
      source!.object_key,
      source!.checksum_sha256,
      Date.now(),
      canonical.setId,
      canonical.linkId,
      canonical.sourceId,
    )
    .run();
  return { fileId, ...canonical, objectKey: source!.object_key };
}

describe("specialized canonical attachment adapters", () => {
  it('keeps ordinary module-empty Expense file reuse and actual writer visibility without borrowing Legal or Finance authority or hiding cross-Company retries',async()=>{
    const {createAttachmentSet,reserveAttachmentFile,completeAttachmentUpload,scanAttachmentNow,linkExistingAttachment}=await import('@erpx/server-files/public/attachments/service'),{measureReads}=await import('@erpx/server-runtime/public/runtime/read-metrics'),{committedSourceVisibility}=await import('../../support/helpers/source-writer-visibility');
    const bytes=Buffer.from('iVBORw0KGgoAAAANSUhEUgAAAAIAAAACCAYAAABytg0kAAAACXBIWXMAAAPoAAAD6AG1e1JrAAAAEUlEQVQImWOQaEj4D8IMMAYAQaQH3RNNL/AAAAAASUVORK5CYII=','base64');
    async function receipt(source:typeof fixture,id:string){
      const actor=source.claimant,set=await createAttachmentSet(db,actor,{targetType:'expense_claim',targetId:id,componentKind:'record',purposeKey:'receipt',expectedTargetVersion:1},key()),reserved=await reserveAttachmentFile(db,actor,set.id,{filename:'original-business-receipt.png',mediaType:'image/png',bytes:bytes.byteLength},key()),uploaded=await completeAttachmentUpload(db,env.H001_FILES,actor,reserved.uploadIntentId,Uint8Array.from(bytes).buffer);expect(await scanAttachmentNow(db,env.H001_FILES,uploaded.sourceId)).toBe('available');await bindRecordAttachmentSet(db,actor,set.id,1,key());return uploaded;
    }
    const originalClaim=await fixture.claim(),original=await receipt(fixture,originalClaim),reusedClaim=await fixture.claim(),target=await createAttachmentSet(db,fixture.claimant,{targetType:'expense_claim',targetId:reusedClaim,componentKind:'record',purposeKey:'receipt',expectedTargetVersion:1},key()),reused=await linkExistingAttachment(db,fixture.claimant,target.id,{sourceLinkId:original.attachmentLinkId,caption:'Original receipt legitimately reused through the normal File owner command'},key());await bindRecordAttachmentSet(db,fixture.claimant,target.id,1,key());
    expect(fixture.claimant.role).toBe('employee');expect(await db.prepare('SELECT jsonb_array_length(modules_json::jsonb) n FROM assignments WHERE id=?').bind(fixture.claimant.assignmentId).first('n')).toBe(0);
    expect(await db.prepare('SELECT a.owner_id,s.target_id FROM attachment_links l JOIN attachments a ON a.id=l.attachment_id JOIN attachment_sets s ON s.id=l.set_id WHERE l.id=?').bind(reused.attachmentLinkId).first()).toEqual({owner_id:originalClaim,target_id:reusedClaim});expect(Buffer.from(await(await attachmentContent(db,env.H001_FILES,fixture.claimant,reused.attachmentLinkId,'original')).object.arrayBuffer())).toEqual(bytes);
    await (await import('../../support/helpers/private-file-unavailable-audits')).retainedUnavailableFileAudits(db,env.H001_FILES,fixture.claimant,reused.attachmentLinkId,{targetType:'expense_claim',targetId:reusedClaim,purposeKey:'receipt',componentKind:'record',expectedTargetVersion:2});
    const observer=await fixture.member(`ordinary-unrelated-reader-${key()}`,'employee');await expect(attachmentContent(db,env.H001_FILES,observer,reused.attachmentLinkId,'original')).rejects.toMatchObject({code:'EXPENSE_NOT_FOUND'});expect(await db.prepare("SELECT reason FROM attachment_access_events WHERE link_id=? AND actor_id=? AND outcome='denied'").bind(reused.attachmentLinkId,observer.userId).first('reason')).toBe('EXPENSE_NOT_FOUND');
    await db.prepare('UPDATE assignments SET revoked_at=1,version=version+1 WHERE id=?').bind(observer.assignmentId).run();await expect(attachmentContent(db,env.H001_FILES,observer,reused.attachmentLinkId,'original')).rejects.toMatchObject({code:'ATTACHMENT_TARGET_NOT_FOUND'});
    const invalidDenials=(await db.prepare("SELECT id,reason FROM attachment_access_events WHERE link_id=? AND actor_id=? AND outcome='denied' ORDER BY id").bind(reused.attachmentLinkId,observer.userId).all()).results;expect(invalidDenials).toHaveLength(2);const invalidVersion=await db.prepare('SELECT version FROM assignments WHERE id=?').bind(observer.assignmentId).first('version'),invalidFault=`ordinary_invalid_${key().replaceAll('-','')}`;
    await db.exec(`CREATE FUNCTION ${invalidFault}() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN UPDATE assignments SET version=version+1 WHERE id='${observer.assignmentId}';RETURN NEW;END $$;CREATE CONSTRAINT TRIGGER ${invalidFault} AFTER INSERT ON attachment_access_events DEFERRABLE INITIALLY DEFERRED FOR EACH ROW WHEN(NEW.link_id='${reused.attachmentLinkId}' AND NEW.outcome='denied') EXECUTE FUNCTION ${invalidFault}()`);
    try{await expect(attachmentContent(db,env.H001_FILES,observer,reused.attachmentLinkId,'original')).rejects.toMatchObject({code:'ATTACHMENT_FILE_READ_CHANGED'});}finally{await db.exec(`DROP TRIGGER ${invalidFault} ON attachment_access_events;DROP FUNCTION ${invalidFault}()`);}expect(await db.prepare('SELECT version FROM assignments WHERE id=?').bind(observer.assignmentId).first('version')).toBe(invalidVersion);expect((await db.prepare("SELECT id,reason FROM attachment_access_events WHERE link_id=? AND actor_id=? AND outcome='denied' ORDER BY id").bind(reused.attachmentLinkId,observer.userId).all()).results).toEqual(invalidDenials);
    const visibility=`employee_source_visible_${key().replaceAll('-','')}`;
    await db.exec(`CREATE FUNCTION ${visibility}() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN IF NOT EXISTS(SELECT 1 FROM finance_expenses WHERE id='${reusedClaim}' AND business_purpose='Actual committed native source before bytes' AND version=3) THEN RAISE EXCEPTION 'STALE_EMPLOYEE_FILE_SOURCE_SNAPSHOT';END IF;RETURN NEW;END $$;CREATE CONSTRAINT TRIGGER ${visibility} AFTER INSERT ON attachment_access_events DEFERRABLE INITIALLY DEFERRED FOR EACH ROW WHEN(NEW.link_id='${reused.attachmentLinkId}' AND NEW.outcome='allowed') EXECUTE FUNCTION ${visibility}()`);
    try{const visible=await committedSourceVisibility(db,tx=>tx.prepare("UPDATE finance_expenses SET business_purpose='Actual committed native source before bytes',version=version+1 WHERE id=?").bind(reusedClaim).run(),()=>measureReads(()=>attachmentContent(db,env.H001_FILES,fixture.claimant,reused.attachmentLinkId,'original')));expect(Buffer.from(await visible.result.object.arrayBuffer())).toEqual(bytes);expect(visible.metrics.deadlockFailures??0).toBe(0);expect(visible.metrics.serializationFailures??0).toBe(0);expect(visible.metrics.transactionRetries??0).toBe(0);}finally{await db.exec(`DROP TRIGGER ${visibility} ON attachment_access_events;DROP FUNCTION ${visibility}()`);}
    const other=await expenseFixture(false),otherClaim=await other.claim(),otherFile=await receipt(other,otherClaim),cases=[{actor:fixture.claimant,linkId:reused.attachmentLinkId,claimId:reusedClaim},{actor:other.claimant,linkId:otherFile.attachmentLinkId,claimId:otherClaim}],overlap=`employee_files_overlap_${key().replaceAll('-','')}`;
    await db.exec(`CREATE FUNCTION ${overlap}() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN PERFORM pg_sleep(0.1);UPDATE finance_expenses SET business_purpose=business_purpose WHERE id=CASE WHEN NEW.link_id='${reused.attachmentLinkId}' THEN '${reusedClaim}' ELSE '${otherClaim}' END;RETURN NEW;END $$;CREATE CONSTRAINT TRIGGER ${overlap} AFTER INSERT ON attachment_access_events DEFERRABLE INITIALLY DEFERRED FOR EACH ROW WHEN(NEW.link_id IN ('${reused.attachmentLinkId}','${otherFile.attachmentLinkId}') AND NEW.outcome='allowed') EXECUTE FUNCTION ${overlap}()`);
    try{const measured=await measureReads(()=>Promise.all(cases.map(source=>attachmentContent(db,env.H001_FILES,source.actor,source.linkId,'original'))));expect(measured.result).toHaveLength(2);for(const file of measured.result)expect(Buffer.from(await file.object.arrayBuffer())).toEqual(bytes);expect(measured.metrics.deadlockFailures??0).toBe(0);expect(measured.metrics.serializationFailures??0).toBe(0);expect(measured.metrics.transactionRetries??0).toBe(0);}finally{await db.exec(`DROP TRIGGER ${overlap} ON attachment_access_events;DROP FUNCTION ${overlap}()`);}
  });
  it('retains the actual standalone Legal file denial while late source or current authority effects roll its audit back',async()=>{
    const manager=await fixture.member(`legal-denied-reader-${key()}`,'legal_manager');await legalCommand(db,manager,{action:'settings',enabled:true},key());const legal=await legalCommand(db,manager,{action:'create',item:{kind:'contract',title:'Native Legal denied read',ownerId:manager.userId,data:{contractType:'Evidence',parties:'Original Company'}}},key());
    const bytes=Buffer.from('iVBORw0KGgoAAAANSUhEUgAAAAIAAAACCAYAAABytg0kAAAACXBIWXMAAAPoAAAD6AG1e1JrAAAAEUlEQVQImWOQaEj4D8IMMAYAQaQH3RNNL/AAAAAASUVORK5CYII=','base64');await uploadDocument(db,env.H001_FILES,manager,String(legal.id),1,'denied-original.png','image/png',bytes,key());
    const document=await db.prepare('SELECT attachment_link_id FROM legal_documents WHERE item_id=?').bind(legal.id).first<{attachment_link_id:string}>();if(!document)throw new Error('ACTUAL_LEGAL_FILE_REQUIRED');const linkId=document.attachment_link_id,reader=fixture.claimant;
    await (await import('../../support/helpers/private-file-unavailable-audits')).retainedUnavailableFileAudits(db,env.H001_FILES,manager,linkId,{targetType:'legal_item',targetId:String(legal.id),purposeKey:'legal_document',componentKind:'record',expectedTargetVersion:2});
    await expect(attachmentContent(db,env.H001_FILES,reader,linkId,'original')).rejects.toMatchObject({code:'PERMISSION_REQUIRED'});
    const original=(await db.prepare("SELECT id,reason FROM attachment_access_events WHERE link_id=? AND actor_id=? AND outcome='denied'").bind(linkId,reader.userId).all()).results;expect(original).toHaveLength(1);expect(original[0].reason).toBe('PERMISSION_REQUIRED');
    for(const change of ['source','authority'] as const){const fault=`legal_denied_${key().replaceAll('-','')}`,effect=change==='source'?`UPDATE legal_items SET version=version+1 WHERE id='${legal.id}'`:`UPDATE assignments SET revoked_at=1,version=version+1 WHERE id='${reader.assignmentId}'`;
      await db.exec(`CREATE FUNCTION ${fault}() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN ${effect};RETURN NEW;END $$;CREATE CONSTRAINT TRIGGER ${fault} AFTER INSERT ON attachment_access_events DEFERRABLE INITIALLY DEFERRED FOR EACH ROW WHEN(NEW.link_id='${linkId}' AND NEW.outcome='denied') EXECUTE FUNCTION ${fault}()`);
      try{await expect(attachmentContent(db,env.H001_FILES,reader,linkId,'original')).rejects.toMatchObject({code:change==='source'?'ATTACHMENT_FILE_READ_CHANGED':'QUEUED_AUTHORITY_CHANGED'});}finally{await db.exec(`DROP TRIGGER ${fault} ON attachment_access_events;DROP FUNCTION ${fault}()`);}
      expect((await db.prepare("SELECT id,reason FROM attachment_access_events WHERE link_id=? AND actor_id=? AND outcome='denied'").bind(linkId,reader.userId).all()).results).toEqual(original);expect(await db.prepare('SELECT version FROM legal_items WHERE id=?').bind(legal.id).first('version')).toBe(2);expect(await db.prepare('SELECT revoked_at FROM assignments WHERE id=?').bind(reader.assignmentId).first('revoked_at')).toBeNull();
    }
    await db.prepare('UPDATE assignments SET revoked_at=1,version=version+1 WHERE id=?').bind(reader.assignmentId).run();await expect(attachmentContent(db,env.H001_FILES,reader,linkId,'original')).rejects.toMatchObject({code:'ATTACHMENT_TARGET_NOT_FOUND'});
 const invalidRetained=(await db.prepare("SELECT id,reason FROM attachment_access_events WHERE link_id=? AND actor_id=? AND outcome='denied' ORDER BY id").bind(linkId,reader.userId).all()).results;expect(invalidRetained).toHaveLength(2);expect(invalidRetained.some(event=>event.reason==='ATTACHMENT_TARGET_NOT_FOUND')).toBe(true);
 const invalidVersion=await db.prepare('SELECT version FROM assignments WHERE id=?').bind(reader.assignmentId).first('version'),invalidFault=`legal_invalid_denial_${key().replaceAll('-','')}`;
 await db.exec(`CREATE FUNCTION ${invalidFault}() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN UPDATE assignments SET version=version+1 WHERE id='${reader.assignmentId}';RETURN NEW;END $$;CREATE CONSTRAINT TRIGGER ${invalidFault} AFTER INSERT ON attachment_access_events DEFERRABLE INITIALLY DEFERRED FOR EACH ROW WHEN(NEW.link_id='${linkId}' AND NEW.outcome='denied') EXECUTE FUNCTION ${invalidFault}()`);
 try{await expect(attachmentContent(db,env.H001_FILES,reader,linkId,'original')).rejects.toMatchObject({code:'ATTACHMENT_FILE_READ_CHANGED'});}finally{await db.exec(`DROP TRIGGER ${invalidFault} ON attachment_access_events;DROP FUNCTION ${invalidFault}()`);}
 expect(await db.prepare('SELECT version FROM assignments WHERE id=?').bind(reader.assignmentId).first('version')).toBe(invalidVersion);expect((await db.prepare("SELECT id,reason FROM attachment_access_events WHERE link_id=? AND actor_id=? AND outcome='denied' ORDER BY id").bind(linkId,reader.userId).all()).results).toEqual(invalidRetained);

  });
  it('observes a committed native Legal reader revocation before the first snapshot and composes two Company exports without transaction retries',async()=>{
    const {measureReads}=await import('@erpx/server-runtime/public/runtime/read-metrics'),{committedSourceVisibility}=await import('../../support/helpers/source-writer-visibility');
    const cases:{actor:Awaited<ReturnType<typeof expenseFixture>>['claimant'];itemId:string;documentId:string;body:string}[]=[];
    for(let index=0;index<2;index++){
      const source=index===0?fixture:await expenseFixture(false),actor=await source.member(`legal-visible-reader-${key()}`,'legal_manager');
      await legalCommand(db,actor,{action:'settings',enabled:true},key());const item=await legalCommand(db,actor,{action:'create',item:{kind:'contract',title:'Native source-first retained text',ownerId:actor.userId,data:{contractType:'Evidence',parties:'Original Company'}}},key());
      const body=`Original Company ${source.company} retained bytes`;await legalCommand(db,actor,{action:'document',id:item.id,expectedVersion:1,name:'Retained text',body},key());
      const document=await db.prepare('SELECT id FROM legal_documents WHERE item_id=?').bind(item.id).first<{id:string}>();if(!document)throw new Error('ACTUAL_LEGAL_DOCUMENT_REQUIRED');cases.push({actor,itemId:String(item.id),documentId:document.id,body});
    }
    const fault=`legal_exports_overlap_${key().replaceAll('-','')}`;
    await db.exec(`CREATE FUNCTION ${fault}() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN PERFORM pg_sleep(0.1);RETURN NEW;END $$;CREATE CONSTRAINT TRIGGER ${fault} AFTER INSERT ON legal_access_events DEFERRABLE INITIALLY DEFERRED FOR EACH ROW WHEN(NEW.item_id IN ('${cases[0].itemId}','${cases[1].itemId}')) EXECUTE FUNCTION ${fault}()`);
    try{const measured=await measureReads(()=>Promise.all(cases.map(source=>documentExport(db,env.H001_FILES,source.actor,source.documentId,null))));expect(measured.result).toHaveLength(2);for(let index=0;index<2;index++)expect(Buffer.from(await measured.result[index].arrayBuffer()).toString()).toBe(cases[index].body);expect(measured.metrics.deadlockFailures??0).toBe(0);expect(measured.metrics.serializationFailures??0).toBe(0);expect(measured.metrics.transactionRetries??0).toBe(0);}finally{await db.exec(`DROP TRIGGER ${fault} ON legal_access_events;DROP FUNCTION ${fault}()`);}
    const source=cases[0],auditBefore=Number(await db.prepare('SELECT count(*) n FROM legal_access_events WHERE item_id=?').bind(source.itemId).first('n'));
    const visible=await committedSourceVisibility(db,tx=>tx.prepare('UPDATE assignments SET revoked_at=1,version=version+1 WHERE id=?').bind(source.actor.assignmentId).run(),()=>measureReads(async()=>{try{return await documentExport(db,env.H001_FILES,source.actor,source.documentId,null);}catch(error){return error;}}));
    expect(visible.result).toMatchObject({code:'QUEUED_AUTHORITY_CHANGED'});expect(visible.metrics.deadlockFailures??0).toBe(0);expect(visible.metrics.serializationFailures??0).toBe(0);expect(visible.metrics.transactionRetries??0).toBe(0);expect(Number(await db.prepare('SELECT count(*) n FROM legal_access_events WHERE item_id=?').bind(source.itemId).first('n'))).toBe(auditBefore);
  });
  it.each(['canonical-file','stored-text'] as const)('keeps %s Legal export bytes inside the original complete source and authority boundary',async kind=>{
    const manager=await fixture.member(`legal-own-read-${key()}`,'legal_manager');await legalCommand(db,manager,{action:'settings',enabled:true},key());
    const legal=await legalCommand(db,manager,{action:'create',item:{kind:'contract',title:'Original retained Legal read',ownerId:manager.userId,data:{contractType:'Evidence',parties:'Original Company'}}},key());
    const bytes=kind==='canonical-file'?Buffer.from('iVBORw0KGgoAAAANSUhEUgAAAAIAAAACCAYAAABytg0kAAAACXBIWXMAAAPoAAAD6AG1e1JrAAAAEUlEQVQImWOQaEj4D8IMMAYAQaQH3RNNL/AAAAAASUVORK5CYII=','base64'):Buffer.from('Original retained Legal text');
    if(kind==='canonical-file')await uploadDocument(db,env.H001_FILES,manager,String(legal.id),1,'retained.png','image/png',bytes,key());
    else await legalCommand(db,manager,{action:'document',id:legal.id,expectedVersion:1,name:'Retained text',body:bytes.toString()},key());
    const document=await db.prepare('SELECT id,attachment_link_id FROM legal_documents WHERE item_id=?').bind(legal.id).first<{id:string;attachment_link_id:string|null}>();expect(document).toBeTruthy();
    const auditBefore=Number(await db.prepare('SELECT count(*) n FROM legal_access_events WHERE item_id=?').bind(legal.id).first('n')),fileBefore=Number(await db.prepare('SELECT count(*) n FROM attachment_access_events WHERE company_id=?').bind(fixture.company).first('n'));
    for(const effect of ['source','authority'] as const){
      const fault=`legal_document_read_${key().replaceAll('-','')}`,change=effect==='source'?`UPDATE legal_items SET version=version+1 WHERE id='${legal.id}'`:`UPDATE assignments SET revoked_at=1,version=version+1 WHERE id='${manager.assignmentId}'`;
      await db.exec(`CREATE FUNCTION ${fault}() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN ${change};RETURN NEW;END $$;CREATE CONSTRAINT TRIGGER ${fault} AFTER INSERT ON legal_access_events DEFERRABLE INITIALLY DEFERRED FOR EACH ROW WHEN(NEW.item_id='${legal.id}') EXECUTE FUNCTION ${fault}()`);
      try{await expect(documentExport(db,env.H001_FILES,manager,document!.id,null)).rejects.toMatchObject({code:effect==='source'?'ATTACHMENT_FILE_READ_CHANGED':'QUEUED_AUTHORITY_CHANGED'});}finally{await db.exec(`DROP TRIGGER ${fault} ON legal_access_events;DROP FUNCTION ${fault}()`);}
      expect(await db.prepare('SELECT version FROM legal_items WHERE id=?').bind(legal.id).first('version')).toBe(2);expect(await db.prepare('SELECT revoked_at FROM assignments WHERE id=?').bind(manager.assignmentId).first('revoked_at')).toBeNull();
      expect(Number(await db.prepare('SELECT count(*) n FROM legal_access_events WHERE item_id=?').bind(legal.id).first('n'))).toBe(auditBefore);expect(Number(await db.prepare('SELECT count(*) n FROM attachment_access_events WHERE company_id=?').bind(fixture.company).first('n'))).toBe(fileBefore);
    }
    const exported=await documentExport(db,env.H001_FILES,manager,document!.id,null);expect(Buffer.from(await exported.arrayBuffer())).toEqual(bytes);
    await expect(db.transaction(async tx=>{await tx.prepare('SELECT 1').first();return documentExport(tx,env.H001_FILES,manager,document!.id,null);})).rejects.toMatchObject({code:'LEGAL_TRANSACTION_REQUIRED'});
    if(document!.attachment_link_id){
      const linkId=document!.attachment_link_id;expect(Buffer.from(await(await attachmentContent(db,env.H001_FILES,manager,linkId,'original')).object.arrayBuffer())).toEqual(bytes);
      await expect(db.transaction(async tx=>{await tx.prepare('SELECT 1').first();return attachmentContent(tx,env.H001_FILES,manager,linkId,'original');})).rejects.toMatchObject({code:'LEGAL_CANONICAL_READ_PARENT_REQUIRED'});
    }
  });
  it.each(['employee_request','expense_claim','leave_request','expense_payment','invoice_payment'] as const)('preserves actual %s bytes and rolls back a late original source change in the complete Legal parent',async kind=>{
    const {createHash}=await import('node:crypto'),{preserveAttachment}=await import('@erpx/server-legal/public/legal/sources'),{createAttachmentSet,reserveAttachmentFile,completeAttachmentUpload,scanAttachmentNow}=await import('@erpx/server-files/public/attachments/service');
    // Each source is prepared by its original normal owner API, with the
    // independent accounting/treasury steps retained for payment evidence.
    fixture=await expenseFixture();
    let sourceId:string,version:number,creator=fixture.claimant,table:string;
    if(kind==='employee_request'){
      const {saveEmployeeRequest,employeeRequestSchema}=await import('@erpx/server-records/public/employee-requests');
      const result=await saveEmployeeRequest(db,creator,null,employeeRequestSchema.parse({kind:'service',title:'Actual employee evidence for Legal',description:'Preserve the original employee request',branchId:fixture.branch,departmentId:fixture.department}),key());sourceId=result.id;version=1;table='employee_requests';
    }else if(kind==='leave_request'){
      const {createLeave}=await import('@erpx/server-people_payroll/public/leave');
      const result=await createLeave(creator,{startDate:'2026-10-15',days:1,handoverNote:'Original handover evidence'},key());if(typeof result.id!=='string')throw new Error('ACTUAL_LEAVE_ID_REQUIRED');sourceId=result.id;version=1;table='leave_requests';
    }else if(kind==='invoice_payment'){
      const {createParty,partySchema,createInvoice,invoiceSchema,postInvoice}=await import('@erpx/server-finance/public/finance'),{resolveFlow,assignFlowPrimary}=await import('@erpx/server-workflow/public/flow-routing');
      const party=await createParty(fixture.reviewer,partySchema.parse({kind:'supplier',code:key(),name:'Actual Legal evidence supplier',currency:'THB'}),key());
      const invoice=await createInvoice(fixture.reviewer,invoiceSchema.parse({kind:'payable',partyId:party.id,invoiceNumber:key(),invoiceDate:'2026-09-01',dueDate:'2026-09-30',currency:'THB',subtotal:'100',tax:'0',debitAccountId:fixture.accounts.expense,creditAccountId:fixture.accounts.payable}),key());
      const flow=await resolveFlow(db,fixture.owner,'invoice',invoice.id);
      await assignFlowPrimary(db,fixture.owner,'invoice',invoice.id,{stepKey:'draft',userId:fixture.manager.userId,reason:'Independent original payable posting',saveDefault:false,expectedVersion:flow.version,recordVersion:flow.recordVersion},key());
      await postInvoice(fixture.manager,invoice.id,{expectedVersion:1,periodId:fixture.period,entryDate:'2026-09-02'},key());sourceId=invoice.id;version=2;creator=fixture.payer;table='finance_invoices';
    }else{
      sourceId=await fixture.claim();version=1;table='finance_expenses';
      if(kind==='expense_payment'){
        const {postExpense}=await import('@erpx/server-people_payroll/public/expenses'),{createExpensePayment}=await import('@erpx/server-people_payroll/public/expense-payments');
        const receipt=await seedAttachmentSet(db,fixture.claimant,{targetType:'expense_claim',targetId:sourceId,componentKind:'record',purposeKey:'receipt',expectedTargetVersion:1});await bindRecordAttachmentSet(db,fixture.claimant,receipt.setId,1,key());
        await fixture.throughAccounts(sourceId);await fixture.action(fixture.manager,sourceId,'approve');const approved=await getExpense(fixture.manager,sourceId);
        await postExpense(fixture.manager,sourceId,{expectedVersion:Number(approved.version),periodId:fixture.period,entryDate:'2026-09-02'},key());const posted=await getExpense(fixture.payer,sourceId);
        const payment=await createExpensePayment(fixture.payer,sourceId,Number(posted.version),key());sourceId=String(payment.draftId);version=1;creator=fixture.payer;table='expense_payment_drafts';
      }
    }
    const purposeKey=kind==='employee_request'?'request_evidence':kind==='leave_request'?'leave_supporting_photo':kind==='expense_claim'?'receipt':'payment_slip',componentKind=kind==='expense_claim'||kind==='leave_request'?'record':'action';
    const bytes=Buffer.from('iVBORw0KGgoAAAANSUhEUgAAAAIAAAACCAYAAABytg0kAAAACXBIWXMAAAPoAAAD6AG1e1JrAAAAEUlEQVQImWOQaEj4D8IMMAYAQaQH3RNNL/AAAAAASUVORK5CYII=','base64'),checksum=createHash('sha256').update(bytes).digest('hex');
    const set=await createAttachmentSet(db,creator,{targetType:kind,targetId:sourceId,componentKind,componentRef:'legal-preservation',purposeKey,expectedTargetVersion:version},key()),reserved=await reserveAttachmentFile(db,creator,set.id,{filename:`${kind}.png`,mediaType:'image/png',bytes:bytes.byteLength},key()),uploaded=await completeAttachmentUpload(db,env.H001_FILES,creator,reserved.uploadIntentId,Uint8Array.from(bytes).buffer);
    expect(await scanAttachmentNow(db,env.H001_FILES,uploaded.sourceId)).toBe('available');
    let consumed=0;const files:import("@erpx/server-runtime/public/runtime/types").FileStore={put:(...args)=>env.H001_FILES.put(...args),head:(...args)=>env.H001_FILES.head(...args),delete:(...args)=>env.H001_FILES.delete(...args),list:(...args)=>env.H001_FILES.list(...args),get:async(...args)=>{const object=await env.H001_FILES.get(...args);return object?{...object,arrayBuffer:async()=>{consumed++;return object.arrayBuffer();}}:null;}};
    const ordinary=await attachmentContent(db,files,creator,uploaded.attachmentLinkId,'original');expect(consumed).toBe(1);expect(Buffer.from(await ordinary.object.arrayBuffer())).toEqual(bytes);expect(consumed).toBe(1);
    const auditBefore=Number(await db.prepare("SELECT count(*) n FROM attachment_access_events WHERE link_id=? AND outcome='allowed'").bind(uploaded.attachmentLinkId).first('n'));
    for(const change of ['source','authority'] as const){const late=`employee_bytes_${key().replaceAll('-','')}`,effect=change==='source'?`UPDATE ${table} SET version=version+1 WHERE id='${sourceId}'`:`UPDATE assignments SET revoked_at=1,version=version+1 WHERE id='${creator.assignmentId}'`;
      await db.exec(`CREATE FUNCTION ${late}() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN ${effect};RETURN NEW;END $$;CREATE CONSTRAINT TRIGGER ${late} AFTER INSERT ON attachment_access_events DEFERRABLE INITIALLY DEFERRED FOR EACH ROW WHEN(NEW.link_id='${uploaded.attachmentLinkId}' AND NEW.outcome='allowed') EXECUTE FUNCTION ${late}()`);
      try{await expect(attachmentContent(db,files,creator,uploaded.attachmentLinkId,'original')).rejects.toMatchObject({code:'ATTACHMENT_FILE_READ_CHANGED'});}finally{await db.exec(`DROP TRIGGER ${late} ON attachment_access_events;DROP FUNCTION ${late}()`);}
      expect(await db.prepare(`SELECT version FROM ${table} WHERE id=?`).bind(sourceId).first('version')).toBe(version);expect(await db.prepare('SELECT revoked_at FROM assignments WHERE id=?').bind(creator.assignmentId).first('revoked_at')).toBeNull();expect(Number(await db.prepare("SELECT count(*) n FROM attachment_access_events WHERE link_id=? AND outcome='allowed'").bind(uploaded.attachmentLinkId).first('n'))).toBe(auditBefore);
    }
    expect(consumed).toBe(3);await expect(db.transaction(async tx=>{await tx.prepare('SELECT 1').first();return attachmentContent(tx,files,creator,uploaded.attachmentLinkId,'original');})).rejects.toMatchObject({code:'EMPLOYEE_EVIDENCE_READ_TRANSACTION_REQUIRED'});expect(consumed).toBe(3);
    const actor={...creator,role:'holding_owner',assignmentId:await fixture.grant(creator,'holding_owner')};
    await legalCommand(db,actor,{action:'settings',enabled:true},key());const legal=await legalCommand(db,actor,{action:'create',item:{kind:'contract',title:`Actual ${kind} preservation`,ownerId:actor.userId,data:{contractType:'Evidence',parties:'Original Company'}}},key());
    const command=key(),body={action:'source.attach',id:String(legal.id),expectedVersion:1,attachmentId:uploaded.sourceId},fault=`evidence_legal_${key().replaceAll('-','')}`;
    await db.exec(`CREATE FUNCTION ${fault}() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN UPDATE ${table} SET version=version+1 WHERE id='${sourceId}';RETURN NEW;END $$;CREATE CONSTRAINT TRIGGER ${fault} AFTER INSERT ON legal_file_preservations DEFERRABLE INITIALLY DEFERRED FOR EACH ROW WHEN(NEW.item_id='${legal.id}') EXECUTE FUNCTION ${fault}()`);
    try{await expect(preserveAttachment(db,env.H001_FILES,actor,body,command)).rejects.toMatchObject({code:'LEGAL_EVIDENCE_FILE_READ_CHANGED'});}finally{await db.exec(`DROP TRIGGER ${fault} ON legal_file_preservations;DROP FUNCTION ${fault}()`);}
    expect(await db.prepare(`SELECT version FROM ${table} WHERE id=?`).bind(sourceId).first('version')).toBe(version);
    expect(await db.prepare('SELECT id FROM legal_documents WHERE item_id=?').bind(legal.id).first()).toBeNull();expect(await db.prepare('SELECT item_id FROM legal_file_preservations WHERE item_id=?').bind(legal.id).first()).toBeNull();expect(await db.prepare('SELECT command_id FROM command_receipts WHERE command_id=?').bind(command).first()).toBeNull();
    if(kind==='employee_request'){
      const currentRole=await db.prepare("SELECT company_id,published_revision FROM iam_roles WHERE id=? AND company_id IN ('*',?) ORDER BY company_id DESC LIMIT 1").bind(actor.role,actor.companyId).first<{company_id:string;published_revision:number}>();if(!currentRole)throw new Error('ACTUAL_ORIGINAL_ROLE_REQUIRED');
      for(const change of ['destination','permission'] as const){
        const late=`preservation_final_${key().replaceAll('-','')}`,effect=change==='destination'?`UPDATE legal_items SET version=version+1 WHERE id='${legal.id}'`:`INSERT INTO iam_role_revisions SELECT company_id,role_id,revision+1,permissions_json-'legal.edit',grantable_json,scopes_json,created_by,created_at FROM iam_role_revisions WHERE company_id='${currentRole.company_id}' AND role_id='${actor.role}' AND revision=${currentRole.published_revision};UPDATE iam_roles SET published_revision=published_revision+1 WHERE company_id='${currentRole.company_id}' AND id='${actor.role}'`;
        await db.exec(`CREATE FUNCTION ${late}() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN ${effect};RETURN NEW;END $$;CREATE CONSTRAINT TRIGGER ${late} AFTER INSERT ON legal_file_preservations DEFERRABLE INITIALLY DEFERRED FOR EACH ROW WHEN(NEW.item_id='${legal.id}') EXECUTE FUNCTION ${late}()`);
        try{await expect(preserveAttachment(db,env.H001_FILES,actor,body,command)).rejects.toMatchObject({code:change==='destination'?'LEGAL_SOURCE_CHANGED':'QUEUED_AUTHORITY_CHANGED'});}finally{await db.exec(`DROP TRIGGER ${late} ON legal_file_preservations;DROP FUNCTION ${late}()`);}
        expect(await db.prepare('SELECT version FROM legal_items WHERE id=?').bind(legal.id).first('version')).toBe(1);expect(await db.prepare('SELECT published_revision FROM iam_roles WHERE company_id=? AND id=?').bind(currentRole.company_id,actor.role).first('published_revision')).toBe(currentRole.published_revision);expect(await db.prepare('SELECT id FROM legal_documents WHERE item_id=?').bind(legal.id).first()).toBeNull();expect(await db.prepare('SELECT command_id FROM command_receipts WHERE command_id=?').bind(command).first()).toBeNull();
      }
    }
    const result=await preserveAttachment(db,env.H001_FILES,actor,body,command);expect(result).toMatchObject({id:legal.id,version:2});expect(await preserveAttachment(db,env.H001_FILES,actor,body,command)).toEqual(result);
    const document=await db.prepare('SELECT checksum,object_key FROM legal_documents WHERE item_id=?').bind(legal.id).first<{checksum:string;object_key:string}>();expect(document?.checksum).toBe(checksum);expect(Buffer.from(await(await env.H001_FILES.get(document!.object_key))!.arrayBuffer())).toEqual(bytes);
    const {prepareLegalEvidenceAttachmentParentRead}=await import('@erpx/server-legal/adapters/evidence-attachment-read');
    await expect(db.transaction(async tx=>{await tx.prepare('SELECT 1').first();return prepareLegalEvidenceAttachmentParentRead(tx,actor,{attachmentId:uploaded.sourceId,ownerType:kind,ownerId:sourceId});})).rejects.toMatchObject({code:'LEGAL_ATTACHMENT_READ_PREPARATION_REQUIRED'});
    if(kind==='employee_request'){
      const {inLegalAttachmentPreservationUnit}=await import('@erpx/server-legal/adapters/purchases');
      await expect(inLegalAttachmentPreservationUnit(db,actor,tx=>attachmentContent(tx,env.H001_FILES,creator,uploaded.attachmentLinkId,'original'),{attachmentId:uploaded.sourceId,ownerType:kind,ownerId:sourceId})).rejects.toMatchObject({code:'LEGAL_EVIDENCE_READ_PARENT_REQUIRED'});
      await expect(inLegalAttachmentPreservationUnit(db,actor,tx=>attachmentContent(tx,env.H001_FILES,actor,uploaded.attachmentLinkId,'original'),{attachmentId:uploaded.sourceId,ownerType:kind,ownerId:key()})).rejects.toMatchObject({code:'ATTACHMENT_FILE_READ_CHANGED'});
    }
  });
  it.each(['disabled','enforced'] as const)('preserves actual clean Task bytes through the complete Legal owner transaction with Work enforcement %s',async mode=>{
    const {createHash}=await import('node:crypto'),{saveTask,taskSchema}=await import('@erpx/server-operations/public/staff-tasks'),{preserveAttachment}=await import('@erpx/server-legal/public/legal/sources'),{createAttachmentSet,reserveAttachmentFile,completeAttachmentUpload,scanAttachmentNow}=await import('@erpx/server-files/public/attachments/service'),{packageGrantStore}=await import('@erpx/server-commercial/public/package-grants');
    const manager=await fixture.member(`legal-task-reader-${key()}`,'legal_manager');
    await legalCommand(db,manager,{action:'settings',enabled:true},key());
    const legal=await legalCommand(db,manager,{action:'create',item:{kind:'contract',title:'Original Task evidence preservation',ownerId:manager.userId,data:{contractType:'Evidence',parties:'Original Company'}}},key());
    const task=await saveTask(db,fixture.owner,null,taskSchema.parse({title:'Canonical Task evidence for Legal',assigneeUserId:manager.userId,startApprovers:[],completionApprovers:[]}),key());
    for(const packageId of ['work','legal'])await packageGrantStore(db,{authorize:async()=>({userId:'isolated-legal-preservation-provider'}),obligations:async()=>{throw Error('PRESERVATION_FIXTURE_CANNOT_INACTIVATE');}}).apply({expectedVersion:0,idempotencyKey:key(),grant:{companyId:fixture.company,packageId,packageVersion:1,state:'active',startsAt:0,endsAt:null,version:1,reason:'Actual independent owner preservation fixture',evidence:'test:task-legal-owner'}});
    const previous={work:process.env.ERPX_WORK_PACKAGE_ENFORCEMENT,legal:process.env.ERPX_LEGAL_PACKAGE_ENFORCEMENT};
    process.env.ERPX_WORK_PACKAGE_ENFORCEMENT=mode;process.env.ERPX_LEGAL_PACKAGE_ENFORCEMENT=mode;
    try{
      const bytes=Buffer.from('iVBORw0KGgoAAAANSUhEUgAAAAIAAAACCAYAAABytg0kAAAACXBIWXMAAAPoAAAD6AG1e1JrAAAAEUlEQVQImWOQaEj4D8IMMAYAQaQH3RNNL/AAAAAASUVORK5CYII=','base64'),checksum=createHash('sha256').update(bytes).digest('hex');
      const set=await createAttachmentSet(db,manager,{targetType:'staff_task',targetId:String(task.id),componentKind:'action',componentRef:'legal-preservation',purposeKey:'task_evidence',expectedTargetVersion:1},key()),reserved=await reserveAttachmentFile(db,manager,set.id,{filename:'original-task.png',mediaType:'image/png',bytes:bytes.byteLength},key()),uploaded=await completeAttachmentUpload(db,env.H001_FILES,manager,reserved.uploadIntentId,Uint8Array.from(bytes).buffer);
      expect(await scanAttachmentNow(db,env.H001_FILES,uploaded.sourceId)).toBe('available');
      const command=key(),body={action:'source.attach',id:String(legal.id),expectedVersion:1,attachmentId:uploaded.sourceId};
      const fault=`task_legal_parent_${key().replaceAll('-','')}`,effect=mode==='disabled'?`UPDATE staff_tasks SET title='Late original Task source change' WHERE id='${task.id}'`:`UPDATE assignments SET revoked_at=1,version=version+1 WHERE id='${manager.assignmentId}'`;
      // The effect occurs after bytes have been copied by the original Legal
      // workflow. Only the complete parent's final check can reject that copy.
      await db.exec(`CREATE FUNCTION ${fault}() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN ${effect};RETURN NEW;END $$;CREATE CONSTRAINT TRIGGER ${fault} AFTER INSERT ON legal_file_preservations DEFERRABLE INITIALLY DEFERRED FOR EACH ROW WHEN(NEW.item_id='${legal.id}') EXECUTE FUNCTION ${fault}()`);
      try{await expect(preserveAttachment(db,env.H001_FILES,manager,body,command)).rejects.toMatchObject({code:'WORK_TASK_FILE_READ_CHANGED'});}finally{await db.exec(`DROP TRIGGER ${fault} ON legal_file_preservations;DROP FUNCTION ${fault}()`);}
      expect(await db.prepare('SELECT id FROM legal_documents WHERE item_id=?').bind(legal.id).first()).toBeNull();expect(await db.prepare('SELECT item_id FROM legal_file_preservations WHERE item_id=?').bind(legal.id).first()).toBeNull();expect(await db.prepare('SELECT command_id FROM command_receipts WHERE command_id=?').bind(command).first()).toBeNull();
      expect(await db.prepare('SELECT title FROM staff_tasks WHERE id=?').bind(task.id).first('title')).toBe('Canonical Task evidence for Legal');expect(await db.prepare('SELECT revoked_at FROM assignments WHERE id=?').bind(manager.assignmentId).first('revoked_at')).toBeNull();
      const result=await preserveAttachment(db,env.H001_FILES,manager,body,command);
      expect(result).toMatchObject({id:legal.id,version:2});expect(await preserveAttachment(db,env.H001_FILES,manager,body,command)).toEqual(result);
      const preserved=await db.prepare('SELECT source_type,source_id,checksum FROM legal_file_preservations WHERE item_id=?').bind(legal.id).all();expect(preserved.results).toEqual([{source_type:'attachment',source_id:uploaded.sourceId,checksum}]);
      const document=await db.prepare('SELECT checksum,object_key,attachment_link_id FROM legal_documents WHERE item_id=?').bind(legal.id).first<{checksum:string;object_key:string;attachment_link_id:string}>();expect(document?.checksum).toBe(checksum);expect(Buffer.from(await(await env.H001_FILES.get(document!.object_key))!.arrayBuffer())).toEqual(bytes);
      await expect(db.transaction(async tx=>{await tx.prepare('SELECT 1').first();return attachmentContent(tx,env.H001_FILES,manager,uploaded.attachmentLinkId,'original');})).rejects.toMatchObject({code:'WORK_TASK_FILE_READ_TRANSACTION_REQUIRED'});
      const {prepareWorkTaskAttachmentParentRead}=await import('@erpx/server-operations/public/task-attachment-read');await expect(db.transaction(async tx=>{await tx.prepare('SELECT 1').first();return prepareWorkTaskAttachmentParentRead(tx,manager);})).rejects.toMatchObject({code:'LEGAL_ATTACHMENT_READ_PREPARATION_REQUIRED'});
    }finally{for(const [name,value] of [['ERPX_WORK_PACKAGE_ENFORCEMENT',previous.work],['ERPX_LEGAL_PACKAGE_ENFORCEMENT',previous.legal]] as const){if(value===undefined)delete process.env[name];else process.env[name]=value;}}
  });
  it("keeps Messaging CSV pending until clean, then binds it atomically and hides guessed links", async () => {
    const wrongContextTeam = await createMessagingTeam(
      db,
      fixture.claimant,
      { name: "Wrong context evidence", members: [fixture.head.userId] },
      key(),
    );
    const wrongContext = await messageFile(wrongContextTeam.roomId, "available", `another-file-${key()}`);
    await expect(
      sendMessagingMessage(
        db,
        fixture.claimant,
        wrongContextTeam.roomId,
        messageSchema.parse({ body: "", files: [wrongContext.fileId] }),
        key(),
      ),
    ).rejects.toMatchObject({ code: "ATTACHMENT_BINDING_CONTEXT" });
    expect(await attachmentBindingState(db, wrongContext.setId)).toMatchObject({ set_state: "draft", link_state: "draft" });

    const pendingTeam = await createMessagingTeam(
      db,
      fixture.claimant,
      { name: "Pending evidence", members: [fixture.head.userId] },
      key(),
    );
    const pending = await messageFile(pendingTeam.roomId, "quarantined");
    await expect(
      sendMessagingMessage(
        db,
        fixture.claimant,
        pendingTeam.roomId,
        messageSchema.parse({ body: "", files: [pending.fileId] }),
        key(),
      ),
    ).rejects.toMatchObject({ code: "ATTACHMENT_SCAN_PENDING" });
    expect((await attachmentBindingState(db, pending.setId))?.set_state).toBe(
      "draft",
    );

    const team = await createMessagingTeam(
      db,
      fixture.claimant,
      { name: "Clean evidence", members: [fixture.head.userId] },
      key(),
    );
    const clean = await messageFile(team.roomId, "available");
    await env.H001_FILES.put(clean.objectKey, "a,b\n1,2\n", {
      onlyIf: { etagDoesNotMatch: "*" },
      httpMetadata: { contentType: "text/csv" },
    });
    const sent = await sendMessagingMessage(
      db,
      fixture.claimant,
      team.roomId,
      messageSchema.parse({ body: "", files: [clean.fileId] }),
      key(),
    );
    expect(sent.id).toMatch(/^message-/);
    expect(await attachmentBindingState(db, clean.setId)).toMatchObject({
      set_state: "bound",
      link_state: "bound",
      binding_type: "message_attachment",
      binding_id: sent.id,
    });
    expect(Number((await messagingRoom(db, fixture.claimant, team.roomId)).version)).toBe(
      2,
    );
    expect((await messagingFile(db, fixture.head, clean.fileId)).id).toBe(
      clean.fileId,
    );
    const download = await attachmentContent(
      db,
      env.H001_FILES,
      fixture.head,
      clean.linkId,
      "original",
    );
    expect(await download.object.text()).toBe("a,b\n1,2\n");
    await expect(
      attachmentContent(
        db,
        env.H001_FILES,
        fixture.owner,
        clean.linkId,
        "original",
      ),
    ).rejects.toMatchObject({ status: 404 });
    await expect(
      attachmentContent(
        db,
        env.H001_FILES,
        fixture.head,
        `guessed-${key()}`,
        "original",
      ),
    ).rejects.toMatchObject({
      status: 404,
      code: "ATTACHMENT_LINK_NOT_FOUND",
    });
    expect(
      Number(
        await db
          .prepare(
            "SELECT count(*) FROM attachment_access_events WHERE link_id=? AND outcome='denied'",
          )
          .bind(clean.linkId)
          .first("count"),
      ),
    ).toBe(1);
  });

  it("counts only clean bound Expense CSV evidence", async () => {
    const claimId = await fixture.claim();
    const claim = await getExpense(fixture.claimant, claimId);
    const clean = await seedAttachmentSet(db, fixture.claimant, {
      targetType: "expense_claim",
      targetId: claimId,
      componentKind: "record",
      purposeKey: "receipt",
      expectedTargetVersion: Number(claim.version),
      mediaType: "text/csv",
    });
    expect((await expenseRequirements(db, claim)).join(" ")).toContain(
      "Attach the receipt",
    );
    await bindRecordAttachmentSet(
      db,
      fixture.claimant,
      clean.setId,
      Number(claim.version),
      key(),
    );
    const after = await getExpense(fixture.claimant, claimId);
    expect((await expenseRequirements(db, after)).join(" ")).not.toContain(
      "Attach the receipt",
    );

    const secondId = await fixture.claim();
    const second = await getExpense(fixture.claimant, secondId);
    const pending = await seedAttachmentSet(db, fixture.claimant, {
      targetType: "expense_claim",
      targetId: secondId,
      componentKind: "record",
      purposeKey: "receipt",
      expectedTargetVersion: Number(second.version),
      sourceState: "quarantined",
      mediaType: "text/csv",
    });
    await expect(
      bindRecordAttachmentSet(
        db,
        fixture.claimant,
        pending.setId,
        Number(second.version),
        key(),
      ),
    ).rejects.toMatchObject({ code: "ATTACHMENT_SCAN_PENDING" });
  });



  it("scans and binds staff and authenticated-portal Legal DOCX files", async () => {
    const manager = await fixture.member(
      `legal-adapter-${key()}`,
      "legal_manager",
    );
    await legalCommand(
      db,
      manager,
      { action: "settings", enabled: true },
      key(),
    );
    const created = await legalCommand(
      db,
      manager,
      {
        action: "create",
        item: {
          kind: "contract",
          title: "Portal evidence",
          ownerId: manager.userId,
          data: { contractType: "Supply", parties: "Company and supplier" },
        },
      },
      key(),
    );
    let item = await legalAccess(db, manager, String(created.id));
    await legalCommand(
      db,
      manager,
      {
        action: "document",
        id: created.id,
        expectedVersion: item.version,
        name: "Shared terms",
        body: "Terms shared with the authenticated counterparty.",
      },
      key(),
    );
    item = await legalAccess(db, manager, String(created.id));
    const internalWord = await Packer.toBuffer(
      new Document({
        sections: [{ children: [new Paragraph("Staff legal evidence")] }],
      }),
    );
    const mediaType =
      "application/vnd.openxmlformats-officedocument.wordprocessingml.document";
    await uploadDocument(
      db,
      env.H001_FILES,
      manager,
      String(created.id),
      Number(item.version),
      "staff-evidence.docx",
      mediaType,
      internalWord,
      key(),
    );
    const internal = await db
      .prepare(
        "SELECT id,attachment_link_id,attachment_source_id FROM legal_documents WHERE item_id=? AND name='staff-evidence.docx'",
      )
      .bind(created.id)
      .first<{
        id: string;
        attachment_link_id: string;
        attachment_source_id: string;
      }>();
    expect(internal?.attachment_link_id).toBeTruthy();
    expect(
      await db
        .prepare("SELECT state FROM attachment_links WHERE id=?")
        .bind(internal!.attachment_link_id)
        .first("state"),
    ).toBe("bound");
    const staffDownload = await documentExport(
      db,
      env.H001_FILES,
      manager,
      internal!.id,
      null,
    );
    expect(staffDownload.headers.get("Content-Disposition")).toMatch(
      /^attachment;/,
    );
    expect(Buffer.from(await staffDownload.arrayBuffer())).toEqual(
      Buffer.from(internalWord),
    );

    const document = (
      await legalDetail(db, manager, String(created.id))
    ).documents.find((candidate) => candidate.name === "Shared terms")!;
    item = await legalAccess(db, manager, String(created.id));
    const email = `portal-${key()}@example.invalid`;
    await legalCommand(
      db,
      manager,
      {
        action: "share",
        id: created.id,
        expectedVersion: item.version,
        documentId: document.id,
        email,
        expiresAt: Date.now() + 86_400_000,
      },
      key(),
    );
    const share = await db
      .prepare(
        "SELECT id FROM legal_shares WHERE item_id=? AND lower(email)=? AND revoked_at IS NULL",
      )
      .bind(created.id, email)
      .first<{ id: string }>();
    const actor = {
      userId: `portal-user-${key()}`,
      email,
      sessionId: `portal-session-${key()}`,
    };
    const word = await Packer.toBuffer(
      new Document({
        sections: [{ children: [new Paragraph("Counterparty evidence")] }],
      }),
    );
    const command = key();
    const before = await legalAccess(db, manager, String(created.id));
    const previousGate = process.env.ERPX_ATTACHMENTS_V2;
    const previousReadOnly = process.env.ERPX_ATTACHMENTS_READ_ONLY;
    const previousFreeze = process.env.ERPX_ATTACHMENT_UPLOAD_FREEZE;
    const sourceCount = Number(
      await db.prepare("SELECT count(*) FROM attachments").first("count"),
    );
    process.env.ERPX_ATTACHMENTS_V2 = "false";
    process.env.ERPX_ATTACHMENTS_READ_ONLY = "true";
    await expect(
      portalUpload(
        db,
        env.H001_FILES,
        actor,
        share!.id,
        "counterparty.docx",
        mediaType,
        word,
        key(),
      ),
    ).rejects.toMatchObject({ code: "ATTACHMENTS_READ_ONLY", status: 503 });
    delete process.env.ERPX_ATTACHMENTS_READ_ONLY;
    process.env.ERPX_ATTACHMENT_UPLOAD_FREEZE = "true";
    await expect(
      portalUpload(
        db,
        env.H001_FILES,
        actor,
        share!.id,
        "counterparty.docx",
        mediaType,
        word,
        key(),
      ),
    ).rejects.toMatchObject({ code: "ATTACHMENT_UPLOAD_FREEZE", status: 503 });
    delete process.env.ERPX_ATTACHMENT_UPLOAD_FREEZE;
    attachmentRuntime.scannerReady = false;
    const unavailableCommand = key();
    await expect(
      portalUpload(
        db,
        env.H001_FILES,
        actor,
        share!.id,
        "counterparty.docx",
        mediaType,
        word,
        unavailableCommand,
      ),
    ).rejects.toMatchObject({
      code: "ATTACHMENT_SCANNER_UNAVAILABLE",
      status: 503,
    });
    expect(
      await env.H001_FILES.head(
        `legal/${fixture.claimant.companyId}/${created.id}/quarantine/attachment-upload-legal-portal-${unavailableCommand}/original`,
      ),
    ).toBeNull();
    attachmentRuntime.scannerReady = true;
    expect(
      Number(await db.prepare("SELECT count(*) FROM attachments").first("count")),
    ).toBe(sourceCount);
    let uploaded!: Awaited<ReturnType<typeof portalUpload>>;
    try {
      uploaded = await portalUpload(
        db,
        env.H001_FILES,
        actor,
        share!.id,
        "counterparty.docx",
        mediaType,
        word,
        command,
      );
      expect(
        await portalUpload(
          db,
          env.H001_FILES,
          actor,
          share!.id,
          "counterparty.docx",
          mediaType,
          word,
          command,
        ),
      ).toEqual(uploaded);
    } finally {
      attachmentRuntime.scannerReady = true;
      attachmentRuntime.previewReady = true;
      if (previousGate === undefined) delete process.env.ERPX_ATTACHMENTS_V2;
      else process.env.ERPX_ATTACHMENTS_V2 = previousGate;
      if (previousReadOnly === undefined)
        delete process.env.ERPX_ATTACHMENTS_READ_ONLY;
      else process.env.ERPX_ATTACHMENTS_READ_ONLY = previousReadOnly;
      if (previousFreeze === undefined)
        delete process.env.ERPX_ATTACHMENT_UPLOAD_FREEZE;
      else process.env.ERPX_ATTACHMENT_UPLOAD_FREEZE = previousFreeze;
    }
    const row = await db
      .prepare(
        "SELECT attachment_link_id,attachment_source_id FROM legal_external_uploads WHERE id=?",
      )
      .bind(uploaded.id)
      .first<{
        attachment_link_id: string;
        attachment_source_id: string;
      }>();
    expect(row?.attachment_link_id).toBeTruthy();
    expect(
      await db
        .prepare("SELECT source_state FROM attachments WHERE id=?")
        .bind(row!.attachment_source_id)
        .first("source_state"),
    ).toBe("available");
    expect(
      await db
        .prepare("SELECT state FROM attachment_links WHERE id=?")
        .bind(row!.attachment_link_id)
        .first("state"),
    ).toBe("bound");
    expect(
      Number((await legalAccess(db, manager, String(created.id))).version),
    ).toBe(Number(before.version) + 1);
    const downloaded = await externalSubmission(
      db,
      env.H001_FILES,
      manager,
      String(uploaded.id),
    );
    expect(Buffer.from(downloaded.bytes)).toEqual(Buffer.from(word));
    expect(
      Number(
        await db
          .prepare(
            "SELECT count(*) FROM attachment_access_events WHERE link_id=? AND outcome='allowed' AND variant='original'",
          )
          .bind(row!.attachment_link_id)
          .first("count"),
      ),
    ).toBe(1);
  });
});
