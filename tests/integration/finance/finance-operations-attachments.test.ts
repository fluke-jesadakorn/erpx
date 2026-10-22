import { beforeAll, beforeEach, describe, expect, it,vi,afterEach } from "vitest";
import { applyMigrations, env } from "../../support/runtime-env";
import { expenseDb as db, expenseFixture, key } from "../../support/helpers/expense-fixture";
import { attachmentBindingState, enableAttachmentV2ForTest, seedAttachmentSet } from "../../support/helpers/attachment-v2";
import type { WorkContext } from "@erpx/server-identity_access/public/authorization";
import type { SqlDatabase } from "@erpx/server-runtime/adapters/runtime/types";
import {
  attachmentPolicy,
  authorizeAttachmentTarget,
} from "@erpx/server-files/public/attachments/policy";
import {
  authorizeFinanceOperationsAttachmentTarget,
  financeOperationsAttachmentPolicies,
} from "@erpx/server-files/public/attachments/finance-operations-policy";
import {
  attestAttachmentSetSanitized,
  bindRecordAttachmentSet,
} from "@erpx/server-files/public/attachments/service";

let fixture: Awaited<ReturnType<typeof expenseFixture>>;
vi.mock("@erpx/server-runtime/adapters/runtime",()=>({holdingResources:async()=>({write:env.H001_DB,files:env.H001_FILES}),runtimeEnv:async()=>env}));

function withPermissions(actor: WorkContext, permissions: string[]): WorkContext {
  return {
    ...actor,
    iam: {
      roleRevision: 1,
      identityVersion: 1,
      permissions,
      grantablePermissions: [],
      principalId: `test:${actor.userId}`,
      personId: null,
      freshUntil: null,
    },
  };
}

beforeAll(async () => {
  await applyMigrations(db, env.STAFF_TEST_MIGRATIONS);
  enableAttachmentV2ForTest();
});

afterEach(()=>vi.unstubAllEnvs());
beforeEach(async () => {
  vi.stubEnv("ERPX_FINANCE_PACKAGE_ENFORCEMENT","disabled");
  fixture = await expenseFixture();
});

describe("Finance, hospitality, Organization and system attachment policy", () => {
  it('compiles the exact native Finance private-read owner inventory and its actual child and included-source leases',async()=>{
    const {financeAttachmentReadTables,financeAttachmentReadSourceLocks,financeAttachmentReadSourceImage,isFinanceAttachmentReadTarget}=await import('@erpx/server-finance/public/attachment-read-sources');
    const actual=[...new Set(financeOperationsAttachmentPolicies.map(policy=>policy.targetType).filter(type=>type.startsWith('finance_')&&type!=='finance_tax_identity'))].sort();expect(Object.keys(financeAttachmentReadTables).sort()).toEqual(actual);expect(actual).toHaveLength(29);expect(isFinanceAttachmentReadTarget('finance_tax_identity')).toBe(false);expect(isFinanceAttachmentReadTarget('invoice_payment')).toBe(false);expect(isFinanceAttachmentReadTarget('finance_private_anything')).toBe(false);
    await db.transaction(async tx=>{await tx.batch(financeAttachmentReadSourceLocks(tx,fixture.company));for(const target of Object.keys(financeAttachmentReadTables)){if(!isFinanceAttachmentReadTarget(target))throw new Error('ACTUAL_FINANCE_TARGET_REQUIRED');expect(await financeAttachmentReadSourceImage(tx,fixture.company,target,`absent-${key()}`)).toBeNull();}});
  });




  it("fails closed across permission, Company and Branch scope boundaries", async () => {
    const journalId = `journal-${key()}`;
    await db.prepare(`INSERT INTO finance_journals(
      id,company_id,chart_id,period_id,journal_number,entry_date,currency,memo,status,created_by,created_at,updated_at
    ) VALUES(?,?,?,?,?,'2026-09-27','THB','Attachment authorization fixture','draft',?,?,?)`)
      .bind(journalId, fixture.company, fixture.chart, fixture.period, `ATT-${key()}`, fixture.owner.userId, Date.now(), Date.now()).run();
    await expect(authorizeAttachmentTarget(
      db,
      withPermissions(fixture.owner, []),
      attachmentPolicy("finance_journal", "journal_support", "record"),
      journalId,
      "write",
    )).rejects.toMatchObject({ status: 403, code: "ATTACHMENT_FORBIDDEN" });
    await expect(authorizeAttachmentTarget(
      db,
      { ...fixture.owner, companyId: `foreign-${key()}` },
      attachmentPolicy("finance_journal", "journal_support", "record"),
      journalId,
      "read",
    )).rejects.toMatchObject({ status: 404, code: "ATTACHMENT_TARGET_NOT_FOUND" });

    const scopedActor = withPermissions({ ...fixture.owner, branchId: fixture.branch, assignmentScope: "branch", scope: { kind: "branch", holdingId: fixture.owner.holdingId, holdingSlug: fixture.owner.holdingSlug, holdingName: fixture.owner.holdingName, companyId: fixture.company, branchId: fixture.branch, branchSlug: "finance-fixture", branchName: "Finance fixture" } }, ["pos.read", "pos.sell"]);
    const fakeDb = {
      prepare: () => ({
        bind: () => ({
          first: async () => ({ company_id: fixture.company, branch_id: `other-${key()}`, department_id: null, version: 1 }),
        }),
      }),
    } as unknown as SqlDatabase;
    await expect(authorizeFinanceOperationsAttachmentTarget(
      fakeDb,
      scopedActor,
      attachmentPolicy("pos_sale", "void_evidence", "action"),
      "sale-1",
      "write",
    )).rejects.toMatchObject({ status: 403, code: "ATTACHMENT_SCOPE_FORBIDDEN" });
  });

  it("binds a Finance record atomically and rejects a stale record-document revision", async () => {
    const journalId = `journal-${key()}`;
    await db.prepare(`INSERT INTO finance_journals(
      id,company_id,chart_id,period_id,journal_number,entry_date,currency,memo,status,created_by,created_at,updated_at
    ) VALUES(?,?,?,?,?,'2026-09-27','THB','Attachment binding fixture','draft',?,?,?)`)
      .bind(journalId, fixture.company, fixture.chart, fixture.period, `ATT-${key()}`, fixture.owner.userId, Date.now(), Date.now()).run();
    const first = await seedAttachmentSet(db, fixture.owner, {
      targetType: "finance_journal",
      targetId: journalId,
      componentKind: "record",
      purposeKey: "journal_support",
      expectedTargetVersion: 1,
    });
    await bindRecordAttachmentSet(db, fixture.owner, first.setId, 1, key());
    expect(await attachmentBindingState(db, first.setId)).toMatchObject({
      set_state: "bound",
      link_state: "bound",
      binding_type: "record_document",
      binding_id: journalId,
    });
    expect(await db.prepare("SELECT version FROM finance_journals WHERE id=?").bind(journalId).first("version")).toBe(2);

    const stale = await seedAttachmentSet(db, fixture.owner, {
      targetType: "finance_journal",
      targetId: journalId,
      componentKind: "record",
      purposeKey: "journal_support",
      expectedTargetVersion: 1,
    });
    await expect(bindRecordAttachmentSet(db, fixture.owner, stale.setId, 1, key()))
      .rejects.toMatchObject({ status: 409, code: "ATTACHMENT_TARGET_CHANGED" });
    expect(await attachmentBindingState(db, stale.setId)).toMatchObject({ set_state: "draft", link_state: "draft" });
  });

  it("requires a server-recorded sanitization review before immutable IAM evidence can bind", async () => {
    const reviewId = `iam-review-${key()}`;
    await db.prepare(`INSERT INTO iam_reviews(id,company_id,kind,payload,created_by,created_person,created_at,status,version,reason)
      VALUES(?,?,'access_review','{}'::jsonb,?,?,?,'pending',1,'Attachment sanitization fixture')`)
      .bind(reviewId, fixture.company, fixture.owner.userId, fixture.owner.userId, Date.now()).run();
    const files = await seedAttachmentSet(db, fixture.owner, {
      targetType: "iam_review",
      targetId: reviewId,
      componentKind: "record",
      purposeKey: "sanitized_review_evidence",
      expectedTargetVersion: 1,
    });
    await expect(bindRecordAttachmentSet(db, fixture.owner, files.setId, 1, key()))
      .rejects.toMatchObject({ status: 422, code: "ATTACHMENT_SANITIZATION_ATTESTATION_REQUIRED" });
    const attestation={
      confirmed: true,
      note: "Reviewed final artifact; credentials and system exports were removed.",
    } as const,attestationKey=key();
    const attested=await attestAttachmentSetSanitized(db, fixture.owner, files.setId, attestation, attestationKey);
    await expect(attestAttachmentSetSanitized(db, fixture.owner, files.setId, attestation, attestationKey)).resolves.toEqual(attested);
    await expect(attestAttachmentSetSanitized(db, fixture.owner, files.setId, {...attestation,note:"Reviewed final artifact; a different sanitization statement was recorded."}, attestationKey))
      .rejects.toMatchObject({status:409,code:"IDEMPOTENCY_CONFLICT"});

    const secondReviewId=`iam-review-${key()}`;
    await db.prepare(`INSERT INTO iam_reviews(id,company_id,kind,payload,created_by,created_person,created_at,status,version,reason)
      VALUES(?,?,'access_review','{}'::jsonb,?,?,?,'pending',1,'Second attachment sanitization fixture')`)
      .bind(secondReviewId,fixture.company,fixture.owner.userId,fixture.owner.userId,Date.now()).run();
    const secondFiles=await seedAttachmentSet(db,fixture.owner,{targetType:"iam_review",targetId:secondReviewId,componentKind:"record",purposeKey:"sanitized_review_evidence",expectedTargetVersion:1});
    await expect(attestAttachmentSetSanitized(db,fixture.owner,secondFiles.setId,attestation,attestationKey))
      .rejects.toMatchObject({status:409,code:"IDEMPOTENCY_CONFLICT"});
    await bindRecordAttachmentSet(db, fixture.owner, files.setId, 1, key());
    expect(await attachmentBindingState(db, files.setId)).toMatchObject({ set_state: "bound", link_state: "bound" });
    expect(await db.prepare("SELECT version FROM iam_reviews WHERE id=?").bind(reviewId).first("version")).toBe(2);
  });
});

const financeReadPng=Buffer.from('iVBORw0KGgoAAAANSUhEUgAAAAIAAAACCAYAAABytg0kAAAACXBIWXMAAAPoAAAD6AG1e1JrAAAAEUlEQVQImWOQaEj4D8IMMAYAQaQH3RNNL/AAAAAASUVORK5CYII=','base64');
async function nativeJournalEvidence(f:Awaited<ReturnType<typeof expenseFixture>>){
 const {createJournal,createJournalSchema}=await import('@erpx/server-finance/public/finance'),files=await import('@erpx/server-files/public/attachments/service');vi.stubEnv('ERPX_ATTACHMENTS_V2','true');vi.stubEnv('ERPX_ATTACHMENT_UPLOAD_FREEZE','false');
 const journal=await createJournal(f.owner,createJournalSchema.parse({chartId:f.chart,periodId:f.period,entryDate:'2026-09-01',currency:'THB',memo:'Actual Finance private-byte source',lines:[{accountId:f.accounts.expense,debit:'100',credit:'0'},{accountId:f.accounts.cash,debit:'0',credit:'100'}]}),key()),set=await files.createAttachmentSet(db,f.owner,{targetType:'finance_journal',targetId:journal.id,componentKind:'record',purposeKey:'journal_support',expectedTargetVersion:1},key()),reserved=await files.reserveAttachmentFile(db,f.owner,set.id,{filename:'original-journal.png',mediaType:'image/png',bytes:financeReadPng.byteLength},key()),upload=await files.completeAttachmentUpload(db,env.H001_FILES,f.owner,reserved.uploadIntentId,financeReadPng.buffer.slice(financeReadPng.byteOffset,financeReadPng.byteOffset+financeReadPng.byteLength));expect(await files.drainAttachmentScans(db,env.H001_FILES,upload.sourceId)).toBe(true);await files.bindRecordAttachmentSet(db,f.owner,set.id,1,key());return {id:journal.id,link:reserved.attachmentLinkId};
}
it('buffers a real Finance journal private read before final native header child authority effects and retains draining purchased history',async()=>{
 const source=await nativeJournalEvidence(fixture),{attachmentContent}=await import('@erpx/server-files/public/attachments/service'),{reviewedFinanceGrant}=await import('../../support/helpers/finance-purchase-fixture');await reviewedFinanceGrant(db,fixture.company,'finance');vi.stubEnv('ERPX_FINANCE_PACKAGE_ENFORCEMENT','enforced');
 const rawGet=env.H001_FILES.get.bind(env.H001_FILES);let consumed=0;const getter=vi.spyOn(env.H001_FILES,'get').mockImplementation(async(...args)=>{const object=await rawGet(...args);return object?{...object,arrayBuffer:async()=>{consumed++;return object.arrayBuffer();}}:null;});
 try{
  const original=await attachmentContent(db,env.H001_FILES,fixture.owner,source.link,'original');expect(consumed).toBe(1);expect(Buffer.from(await original.object.arrayBuffer())).toEqual(financeReadPng);expect(consumed).toBe(1);
  const lines=await db.prepare('SELECT id,ordinal FROM finance_journal_lines WHERE journal_id=? ORDER BY id').bind(source.id).all(),grant=await db.prepare("SELECT to_jsonb(g)::text image FROM company_package_grants g WHERE company_id=? AND package_id='finance'").bind(fixture.company).first('image');
  for(const change of ['source','child','authority'] as const){const fault=`finance_file_final_${key().replaceAll('-','')}`,effect=change==='source'?`UPDATE finance_journals SET memo='Late source evidence' WHERE id='${source.id}'`:change==='child'?`UPDATE finance_journal_lines SET ordinal=ordinal+10 WHERE journal_id='${source.id}'`:`UPDATE assignments SET revoked_at=1,version=version+1 WHERE id='${fixture.owner.assignmentId}'`,before=Number(await db.prepare("SELECT count(*) n FROM attachment_access_events WHERE link_id=?").bind(source.link).first('n'));
   await db.exec(`CREATE FUNCTION ${fault}() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN ${effect};RETURN NEW;END $$;CREATE CONSTRAINT TRIGGER ${fault} AFTER INSERT ON attachment_access_events DEFERRABLE INITIALLY DEFERRED FOR EACH ROW WHEN(NEW.link_id='${source.link}' AND NEW.outcome='allowed') EXECUTE FUNCTION ${fault}()`);
   try{await expect(attachmentContent(db,env.H001_FILES,fixture.owner,source.link,'original')).rejects.toMatchObject({code:'ATTACHMENT_FILE_READ_CHANGED'});}finally{await db.exec(`DROP TRIGGER ${fault} ON attachment_access_events;DROP FUNCTION ${fault}()`);}
   expect(Number(await db.prepare("SELECT count(*) n FROM attachment_access_events WHERE link_id=?").bind(source.link).first('n'))).toBe(before);expect(await db.prepare('SELECT memo FROM finance_journals WHERE id=?').bind(source.id).first('memo')).toBe('Actual Finance private-byte source');expect((await db.prepare('SELECT id,ordinal FROM finance_journal_lines WHERE journal_id=? ORDER BY id').bind(source.id).all()).results).toEqual(lines.results);expect(await db.prepare('SELECT revoked_at FROM assignments WHERE id=?').bind(fixture.owner.assignmentId).first('revoked_at')).toBeNull();expect(await db.prepare("SELECT to_jsonb(g)::text image FROM company_package_grants g WHERE company_id=? AND package_id='finance'").bind(fixture.company).first('image')).toBe(grant);
  }
  await reviewedFinanceGrant(db,fixture.company,'finance','draining');expect(Buffer.from(await(await attachmentContent(db,env.H001_FILES,fixture.owner,source.link,'original')).object.arrayBuffer())).toEqual(financeReadPng);
  await expect(db.transaction(async tx=>{await tx.prepare('SELECT 1').first();return attachmentContent(tx,env.H001_FILES,fixture.owner,source.link,'original');})).rejects.toMatchObject({code:'FINANCE_FILE_READ_TRANSACTION_REQUIRED'});
 }finally{getter.mockRestore();}
});
it('sees the committed original Finance source before its first snapshot and completes two Company reads with zero transaction retries',async()=>{
 const first=await nativeJournalEvidence(fixture),other=await expenseFixture(),second=await nativeJournalEvidence(other),{attachmentContent}=await import('@erpx/server-files/public/attachments/service'),{committedSourceVisibility}=await import('../../support/helpers/source-writer-visibility'),{measureReads}=await import('@erpx/server-runtime/public/runtime/read-metrics'),{companyPurchaseReadLocks}=await import('@erpx/server-commercial/public/purchase-command-boundary'),fault=`finance_snapshot_${key().replaceAll('-','')}`;
 await db.exec(`CREATE FUNCTION ${fault}() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN IF NOT EXISTS(SELECT 1 FROM finance_journals WHERE id='${first.id}' AND memo='Committed native Finance source') THEN RAISE EXCEPTION 'STALE_FINANCE_PRIVATE_SOURCE_SNAPSHOT';END IF;RETURN NEW;END $$;CREATE CONSTRAINT TRIGGER ${fault} AFTER INSERT ON attachment_access_events DEFERRABLE INITIALLY DEFERRED FOR EACH ROW WHEN(NEW.link_id='${first.link}' AND NEW.outcome='allowed') EXECUTE FUNCTION ${fault}()`);
 try{const measured=await committedSourceVisibility(db,async tx=>{await tx.batch(companyPurchaseReadLocks(tx,fixture.company));await tx.prepare("UPDATE finance_journals SET memo='Committed native Finance source' WHERE id=?").bind(first.id).run();},()=>measureReads(()=>attachmentContent(db,env.H001_FILES,fixture.owner,first.link,'original')));expect(Buffer.from(await measured.result.object.arrayBuffer())).toEqual(financeReadPng);expect(measured.metrics.deadlockFailures??0).toBe(0);expect(measured.metrics.serializationFailures??0).toBe(0);expect(measured.metrics.transactionRetries??0).toBe(0);}finally{await db.exec(`DROP TRIGGER ${fault} ON attachment_access_events;DROP FUNCTION ${fault}()`);}
 const overlap=`finance_read_overlap_${key().replaceAll('-','')}`;await db.exec(`CREATE FUNCTION ${overlap}() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN PERFORM pg_sleep(.12);UPDATE finance_journals SET memo=memo WHERE id=CASE WHEN NEW.link_id='${first.link}' THEN '${first.id}' ELSE '${second.id}' END;RETURN NEW;END $$;CREATE CONSTRAINT TRIGGER ${overlap} AFTER INSERT ON attachment_access_events DEFERRABLE INITIALLY DEFERRED FOR EACH ROW WHEN(NEW.link_id IN ('${first.link}','${second.link}') AND NEW.outcome='allowed') EXECUTE FUNCTION ${overlap}()`);
 try{const measured=await measureReads(()=>Promise.all([{actor:fixture.owner,file:first},{actor:other.owner,file:second}].map(item=>attachmentContent(db,env.H001_FILES,item.actor,item.file.link,'original'))));expect(measured.result).toHaveLength(2);for(const result of measured.result)expect(Buffer.from(await result.object.arrayBuffer())).toEqual(financeReadPng);expect(measured.metrics.deadlockFailures??0).toBe(0);expect(measured.metrics.serializationFailures??0).toBe(0);expect(measured.metrics.transactionRetries??0).toBe(0);}finally{await db.exec(`DROP TRIGGER ${overlap} ON attachment_access_events;DROP FUNCTION ${overlap}()`);}
});
it('keeps truthful Finance denied audits for current and already-revoked principals and rolls back any late invalid-authority change',async()=>{
 const source=await nativeJournalEvidence(fixture),reader=fixture.claimant,{attachmentContent}=await import('@erpx/server-files/public/attachments/service');await expect(attachmentContent(db,env.H001_FILES,reader,source.link,'original')).rejects.toMatchObject({code:'ATTACHMENT_FORBIDDEN'});await db.prepare('UPDATE assignments SET revoked_at=1,version=version+1 WHERE id=?').bind(reader.assignmentId).run();await expect(attachmentContent(db,env.H001_FILES,reader,source.link,'original')).rejects.toMatchObject({code:'ATTACHMENT_TARGET_NOT_FOUND'});
 const denied=(await db.prepare("SELECT id,reason FROM attachment_access_events WHERE link_id=? AND actor_id=? AND outcome='denied' ORDER BY id").bind(source.link,reader.userId).all()).results;expect(denied).toHaveLength(2);const version=await db.prepare('SELECT version FROM assignments WHERE id=?').bind(reader.assignmentId).first('version'),fault=`finance_invalid_${key().replaceAll('-','')}`,get=vi.spyOn(env.H001_FILES,'get');
 await db.exec(`CREATE FUNCTION ${fault}() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN UPDATE assignments SET version=version+1 WHERE id='${reader.assignmentId}';RETURN NEW;END $$;CREATE CONSTRAINT TRIGGER ${fault} AFTER INSERT ON attachment_access_events DEFERRABLE INITIALLY DEFERRED FOR EACH ROW WHEN(NEW.link_id='${source.link}' AND NEW.outcome='denied') EXECUTE FUNCTION ${fault}()`);
 try{await expect(attachmentContent(db,env.H001_FILES,reader,source.link,'original')).rejects.toMatchObject({code:'ATTACHMENT_FILE_READ_CHANGED'});expect(get).not.toHaveBeenCalled();}finally{get.mockRestore();await db.exec(`DROP TRIGGER ${fault} ON attachment_access_events;DROP FUNCTION ${fault}()`);}
 expect(await db.prepare('SELECT version FROM assignments WHERE id=?').bind(reader.assignmentId).first('version')).toBe(version);expect((await db.prepare("SELECT id,reason FROM attachment_access_events WHERE link_id=? AND actor_id=? AND outcome='denied' ORDER BY id").bind(source.link,reader.userId).all()).results).toEqual(denied);
});

it('retains exact original quarantined and missing Finance byte audits while late source or authority effects roll them back',async()=>{
 const source=await nativeJournalEvidence(fixture),{attachmentContent}=await import('@erpx/server-files/public/attachments/service'),quarantined=await seedAttachmentSet(db,fixture.owner,{targetType:'finance_journal',targetId:source.id,purposeKey:'journal_support',componentKind:'record',expectedTargetVersion:2,sourceState:'quarantined'}),get=vi.spyOn(env.H001_FILES,'get');
 try{await expect(attachmentContent(db,env.H001_FILES,fixture.owner,quarantined.linkId,'original')).rejects.toMatchObject({status:423,code:'ATTACHMENT_NOT_AVAILABLE'});expect(get).not.toHaveBeenCalled();expect(await db.prepare("SELECT reason FROM attachment_access_events WHERE link_id=? AND outcome='denied'").bind(quarantined.linkId).first('reason')).toBe('ATTACHMENT_NOT_AVAILABLE');}finally{get.mockRestore();}
 const missing=vi.spyOn(env.H001_FILES,'get').mockResolvedValue(null);
 try{
  await expect(attachmentContent(db,env.H001_FILES,fixture.owner,source.link,'original')).rejects.toMatchObject({status:503,code:'FILE_UNAVAILABLE'});const before=(await db.prepare("SELECT id,reason FROM attachment_access_events WHERE link_id=? ORDER BY id").bind(source.link).all()).results;expect(before).toHaveLength(1);expect(before[0].reason).toBe('FILE_UNAVAILABLE');
  for(const change of ['source','authority'] as const){const fault=`finance_unavailable_${key().replaceAll('-','')}`,effect=change==='source'?`UPDATE finance_journals SET memo='Late unavailable source' WHERE id='${source.id}'`:`UPDATE assignments SET revoked_at=1,version=version+1 WHERE id='${fixture.owner.assignmentId}'`;
   await db.exec(`CREATE FUNCTION ${fault}() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN ${effect};RETURN NEW;END $$;CREATE CONSTRAINT TRIGGER ${fault} AFTER INSERT ON attachment_access_events DEFERRABLE INITIALLY DEFERRED FOR EACH ROW WHEN(NEW.link_id='${source.link}' AND NEW.outcome='denied') EXECUTE FUNCTION ${fault}()`);
   try{await expect(attachmentContent(db,env.H001_FILES,fixture.owner,source.link,'original')).rejects.toMatchObject({code:'ATTACHMENT_FILE_READ_CHANGED'});}finally{await db.exec(`DROP TRIGGER ${fault} ON attachment_access_events;DROP FUNCTION ${fault}()`);}
   expect((await db.prepare("SELECT id,reason FROM attachment_access_events WHERE link_id=? ORDER BY id").bind(source.link).all()).results).toEqual(before);expect(await db.prepare('SELECT memo FROM finance_journals WHERE id=?').bind(source.id).first('memo')).toBe('Actual Finance private-byte source');expect(await db.prepare('SELECT revoked_at FROM assignments WHERE id=?').bind(fixture.owner.assignmentId).first('revoked_at')).toBeNull();
  }
 }finally{missing.mockRestore();}
});
