import { afterEach,beforeAll, beforeEach, expect, it, vi } from "vitest";
import { applyMigrations, env } from "../../support/runtime-env";
import {
  expenseDb as db,
  expenseFixture,
  key,
} from "../../support/helpers/expense-fixture";
import {
  attachmentBindingState,
  enableAttachmentV2ForTest,
  seedAttachmentSet,
} from "../../support/helpers/attachment-v2";
import {
  attachmentPolicy,
  authorizeAttachmentTarget,
} from "@erpx/server-files/public/attachments/policy";
import {
  bindRecordAttachmentSet,
  listAttachmentSets,
} from "@erpx/server-files/public/attachments/service";
import { sensitiveAttachmentPolicies } from "@erpx/server-files/public/attachments/sensitive-policy";
import type { WorkContext } from "@erpx/server-identity_access/public/authorization";

vi.mock("@erpx/server-runtime/adapters/runtime", () => ({
  holdingResources: async () => ({ write: env.H001_DB, files: env.H001_FILES }),
  runtimeEnv: async () => env,
}));

let f: Awaited<ReturnType<typeof expenseFixture>>;

function withPermissions(actor: WorkContext, permissions: string[]): WorkContext {
  return {
    ...actor,
    iam: {
      roleRevision: 1,
      identityVersion: 1,
      permissions,
      grantablePermissions: [],
      principalId: `local:${actor.userId}`,
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
  f = await expenseFixture();
});



it("allows only self-visible employee documents and returns no bound metadata to a broad records role", async () => {
  const personId = `person-${f.claimant.userId}`;
  await expect(
    authorizeAttachmentTarget(
      db,
      f.claimant,
      attachmentPolicy("hr_person", "employment_contract", "record"),
      personId,
      "read",
    ),
  ).resolves.toMatchObject({ companyId: f.company, version: 1 });
  await expect(
    authorizeAttachmentTarget(
      db,
      f.claimant,
      attachmentPolicy("hr_person", "professional_certificate", "record"),
      personId,
      "write",
    ),
  ).resolves.toMatchObject({ editable: true });
  await expect(
    authorizeAttachmentTarget(
      db,
      f.claimant,
      attachmentPolicy("hr_person", "identity_document", "record"),
      personId,
      "read",
    ),
  ).rejects.toMatchObject({ status: 404, code: "ATTACHMENT_TARGET_NOT_FOUND" });
  await expect(
    authorizeAttachmentTarget(
      db,
      f.claimant,
      attachmentPolicy("hr_person", "employment_contract", "record"),
      personId,
      "write",
    ),
  ).rejects.toMatchObject({ status: 404, code: "ATTACHMENT_TARGET_NOT_FOUND" });

  const files = await seedAttachmentSet(db, f.owner, {
    targetType: "hr_person",
    targetId: personId,
    componentKind: "record",
    purposeKey: "employment_contract",
    expectedTargetVersion: 1,
  });
  await db.batch([
    db
      .prepare(
        "UPDATE attachment_sets SET state='bound',bound_at=?,bound_command_id=? WHERE id=?",
      )
      .bind(Date.now(), key(), files.setId),
    db
      .prepare(
        "UPDATE attachment_links SET state='bound',bound_at=?,binding_type='record_document',binding_id=? WHERE id=?",
      )
      .bind(Date.now(), personId, files.linkId),
  ]);
  expect(
    await listAttachmentSets(db, f.claimant, {
      targetType: "hr_person",
      targetId: personId,
      purposeKey: "employment_contract",
      componentKind: "record",
    }),
  ).toHaveLength(1);

  const broadRecordsActor = withPermissions(f.reviewer, [
    "records.read",
    "records.write",
    "records.review",
    "people.read",
    "people.manage",
  ]);
  await expect(
    listAttachmentSets(db, broadRecordsActor, {
      targetType: "hr_person",
      targetId: personId,
      purposeKey: "employment_contract",
      componentKind: "record",
    }),
  ).rejects.toMatchObject({ status: 404, code: "ATTACHMENT_TARGET_NOT_FOUND" });
  expect(await attachmentBindingState(db, files.setId)).toMatchObject({
    set_state: "bound",
    link_state: "bound",
  });
});

it("binds an employee-owned certificate and advances the canonical person version atomically", async () => {
  const personId = `person-${f.claimant.userId}`;
  const files = await seedAttachmentSet(db, f.claimant, {
    targetType: "hr_person",
    targetId: personId,
    componentKind: "record",
    purposeKey: "professional_certificate",
    expectedTargetVersion: 1,
  });
  await bindRecordAttachmentSet(db, f.claimant, files.setId, 1, key());
  expect(await attachmentBindingState(db, files.setId)).toMatchObject({
    set_state: "bound",
    link_state: "bound",
    binding_type: "record_document",
    binding_id: personId,
  });
  expect(
    await db.prepare("SELECT version FROM hr_people WHERE id=?").bind(personId).first("version"),
  ).toBe(2);
});

it("limits candidate files to assigned Talent ownership despite the broad Talent read role", async () => {
  const requisitionId = `talent-req-${key()}`;
  const candidateId = `talent-candidate-${key()}`;
  const applicationId = `talent-application-${key()}`;
  await db.batch([
    db
      .prepare(
        `INSERT INTO talent_requisitions(id,company_id,branch_id,department_id,job_title,openings,target_start_date,description,status,created_by,version,created_at,updated_at)
        VALUES(?,?,?,?, 'Analyst',1,'2027-01-15','Sensitive attachment access fixture','open',?,1,1,1)`,
      )
      .bind(requisitionId, f.company, f.branch, f.department, f.owner.userId),
    db
      .prepare(
        `INSERT INTO talent_candidates(id,company_id,legal_name,email,consent_evidence,consented_at,retention_until,status,created_by,version,created_at,updated_at)
        VALUES(?,?, 'Candidate One',?,'Candidate consented to this fixture',1,'2027-12-31','active',?,1,1,1)`,
      )
      .bind(candidateId, f.company, `${candidateId}@example.test`, f.owner.userId),
    db
      .prepare(
        `INSERT INTO talent_applications(id,company_id,requisition_id,requisition_version,candidate_id,status,owner_id,created_by,version,created_at,updated_at)
        VALUES(?,?,?,?,?,'applied',?,?,1,1,1)`,
      )
      .bind(
        applicationId,
        f.company,
        requisitionId,
        1,
        candidateId,
        f.head.userId,
        f.owner.userId,
      ),
  ]);

  await expect(
    authorizeAttachmentTarget(
      db,
      f.head,
      attachmentPolicy("talent_candidate", "cv", "record"),
      candidateId,
      "read",
    ),
  ).resolves.toMatchObject({ companyId: f.company, version: 1 });
  await expect(
    authorizeAttachmentTarget(
      db,
      f.tie,
      attachmentPolicy("talent_candidate", "cv", "record"),
      candidateId,
      "read",
    ),
  ).rejects.toMatchObject({ status: 404, code: "ATTACHMENT_TARGET_NOT_FOUND" });
  await expect(
    authorizeAttachmentTarget(
      db,
      f.owner,
      attachmentPolicy("talent_candidate", "background_document", "record"),
      candidateId,
      "write",
    ),
  ).resolves.toMatchObject({ editable: true });
});

it("keeps retirement clearance evidence scoped to the assigned task owner", async () => {
  const caseId = `clearance-case-${key()}`;
  const taskId = `${caseId}-equipment`;
  const engagementId = `eng-${f.claimant.userId}`;
  await db.batch([
    db
      .prepare(
        `INSERT INTO people_lifecycle_cases(id,company_id,person_id,engagement_id,kind,status,version,effective_at,definition,created_by,updated_at)
        VALUES(?,?,?,?,'offboarding','scheduled',1,1,'{}'::jsonb,?,1)`,
      )
      .bind(
        caseId,
        f.company,
        `person-${f.claimant.userId}`,
        engagementId,
        f.owner.userId,
      ),
    db
      .prepare(
        `INSERT INTO people_preparation_tasks(id,company_id,case_id,kind,title,status,assignee_id,version,updated_at,responsibility,clearance_type)
        VALUES(?,?,?,'lifecycle','Return equipment','pending',?,1,1,'equipment','equipment')`,
      )
      .bind(taskId, f.company, caseId, f.claimant.userId),
  ]);

  await expect(
    authorizeAttachmentTarget(
      db,
      f.claimant,
      attachmentPolicy("people_clearance_task", "clearance_evidence", "action"),
      taskId,
      "bind",
    ),
  ).resolves.toMatchObject({ companyId: f.company, version: 1 });
  await expect(
    authorizeAttachmentTarget(
      db,
      f.owner,
      attachmentPolicy("people_clearance_task", "clearance_evidence", "action"),
      taskId,
      "read",
    ),
  ).rejects.toMatchObject({ status: 404, code: "ATTACHMENT_TARGET_NOT_FOUND" });
});

it('compiles all ten exact native People Payroll evidence source images and query-free leases without inventing a Roster target',async()=>{
 const {peopleAttachmentReadTables,isPeopleAttachmentReadTarget,peopleAttachmentReadSourceLocks,peopleAttachmentReadSourceImage}=await import('@erpx/server-people_payroll/public/attachment-read-sources'),{financeOperationsAttachmentPolicies}=await import('@erpx/server-files/public/attachments/finance-operations-policy');
 const registered=[...new Set([...sensitiveAttachmentPolicies,...financeOperationsAttachmentPolicies].map(policy=>policy.targetType).filter(target=>['hr_person','hr_evaluation_assessment','people_lifecycle_case','people_clearance_task','payroll_run','payroll_advance','payroll_final_pay','overtime_request','time_entry','people_provision_operation'].includes(target)))].sort();expect(Object.keys(peopleAttachmentReadTables).sort()).toEqual(registered);expect(registered).toHaveLength(10);expect(isPeopleAttachmentReadTarget('roster_shift')).toBe(false);expect(isPeopleAttachmentReadTarget('leave_request')).toBe(false);expect(isPeopleAttachmentReadTarget('benefit_plan')).toBe(false);
 await db.transaction(async tx=>{await tx.batch(peopleAttachmentReadSourceLocks(tx));for(const target of Object.keys(peopleAttachmentReadTables)){if(!isPeopleAttachmentReadTarget(target))throw Error('EXACT_PEOPLE_TARGET_REQUIRED');expect(await peopleAttachmentReadSourceImage(tx,f.company,target,`missing-native-${key()}`)).toBeNull();}});
});

const privatePeoplePng=Buffer.from('iVBORw0KGgoAAAANSUhEUgAAAAIAAAACCAYAAABytg0kAAAACXBIWXMAAAPoAAAD6AG1e1JrAAAAEUlEQVQImWOQaEj4D8IMMAYAQaQH3RNNL/AAAAAASUVORK5CYII=','base64');
async function nativePeopleFile(actor:WorkContext,input:Parameters<typeof import("@erpx/server-files/public/attachments/service").createAttachmentSet>[2]){
 const service=await import('@erpx/server-files/public/attachments/service');vi.stubEnv('ERPX_ATTACHMENTS_V2','true');vi.stubEnv('ERPX_ATTACHMENT_UPLOAD_FREEZE','false');const set=await service.createAttachmentSet(db,actor,input,key()),reserved=await service.reserveAttachmentFile(db,actor,set.id,{filename:'private-native-owner.png',mediaType:'image/png',bytes:privatePeoplePng.byteLength},key()),uploaded=await service.completeAttachmentUpload(db,env.H001_FILES,actor,reserved.uploadIntentId,Uint8Array.from(privatePeoplePng).buffer);expect(await service.drainAttachmentScans(db,env.H001_FILES,uploaded.sourceId)).toBe(true);if(input.componentKind==='record')await service.bindRecordAttachmentSet(db,actor,set.id,input.expectedTargetVersion,key());return {linkId:reserved.attachmentLinkId,setId:set.id};
}
async function privatePeoplePurchase(companyId:string,packageId:'projects'|'people'|'people-payroll'|'people-workforce',state:'active'|'draining'='active'){
 const {packageGrantStore,readCompanyPackageGrants}=await import('@erpx/server-commercial/public/package-grants'),prior=(await readCompanyPackageGrants(db,companyId)).find(grant=>grant.packageId===packageId);return packageGrantStore(db,{authorize:async()=>({userId:'isolated-owner-file-review'}),obligations:async()=>{throw Error('Actual withdrawal requires owner witnesses');}}).apply({expectedVersion:prior?.version??0,idempotencyKey:key(),grant:{companyId,packageId,packageVersion:1,state,startsAt:0,endsAt:null,version:(prior?.version??0)+1,reason:'Reviewed isolated original owner file history',evidence:'test:private-native-people'}});
}
it('preserves Projects worker foundations and real module-empty employee documents and rolls back late Person placement or IAM effects before bytes escape',async()=>{
 const {attachmentContent}=await import('@erpx/server-files/public/attachments/service'),personId=`person-${f.claimant.userId}`,file=await nativePeopleFile(f.owner,{targetType:'hr_person',targetId:personId,componentKind:'record',purposeKey:'employment_contract',expectedTargetVersion:1});expect(f.claimant.modules).toEqual([]);await privatePeoplePurchase(f.company,'projects');vi.stubEnv('ERPX_PEOPLE_PACKAGE_ENFORCEMENT','enforced');await expect(attachmentContent(db,env.H001_FILES,f.claimant,file.linkId,'original')).rejects.toMatchObject({code:'PACKAGE_REQUIRED'});const {peopleReferenceRows}=await import('@erpx/server-people_payroll/public/people-purchases');expect((await peopleReferenceRows(f.owner,'worker',db)).some(row=>row.id===personId)).toBe(true);await privatePeoplePurchase(f.company,'people');
 const native=env.H001_FILES.get.bind(env.H001_FILES);let consumed=0;const get=vi.spyOn(env.H001_FILES,'get').mockImplementation(async(...args)=>{const object=await native(...args);return object?{...object,arrayBuffer:async()=>{consumed++;return object.arrayBuffer();}}:null;});
 try{const result=await attachmentContent(db,env.H001_FILES,f.claimant,file.linkId,'original');expect(consumed).toBe(1);expect(Buffer.from(await result.object.arrayBuffer())).toEqual(privatePeoplePng);expect(consumed).toBe(1);}finally{get.mockRestore();}
 for(const change of ['person','placement','authority'] as const){const fault=`people_read_final_${key().replaceAll('-','')}`,effect=change==='person'?`UPDATE hr_people SET legal_name='Late private source name' WHERE id='${personId}'`:change==='placement'?`UPDATE hr_engagements SET job_title='Late placement source' WHERE id='eng-${f.claimant.userId}'`:`UPDATE assignments SET revoked_at=1,version=version+1 WHERE id='${f.claimant.assignmentId}'`,source=await db.prepare('SELECT to_jsonb(p)::text image FROM hr_people p WHERE id=?').bind(personId).first('image'),placement=await db.prepare('SELECT to_jsonb(e)::text image FROM hr_engagements e WHERE id=?').bind(`eng-${f.claimant.userId}`).first('image'),before=Number(await db.prepare('SELECT count(*) n FROM attachment_access_events WHERE link_id=?').bind(file.linkId).first('n'));
  await db.exec(`CREATE FUNCTION ${fault}() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN ${effect};RETURN NEW;END $$;CREATE CONSTRAINT TRIGGER ${fault} AFTER INSERT ON attachment_access_events DEFERRABLE INITIALLY DEFERRED FOR EACH ROW WHEN(NEW.link_id='${file.linkId}' AND NEW.outcome='allowed') EXECUTE FUNCTION ${fault}()`);
  try{await expect(attachmentContent(db,env.H001_FILES,f.claimant,file.linkId,'original')).rejects.toMatchObject({code:'ATTACHMENT_FILE_READ_CHANGED'});}finally{await db.exec(`DROP TRIGGER ${fault} ON attachment_access_events;DROP FUNCTION ${fault}()`);}
  expect(await db.prepare('SELECT to_jsonb(p)::text image FROM hr_people p WHERE id=?').bind(personId).first('image')).toBe(source);expect(await db.prepare('SELECT to_jsonb(e)::text image FROM hr_engagements e WHERE id=?').bind(`eng-${f.claimant.userId}`).first('image')).toBe(placement);expect(await db.prepare('SELECT revoked_at FROM assignments WHERE id=?').bind(f.claimant.assignmentId).first('revoked_at')).toBeNull();expect(Number(await db.prepare('SELECT count(*) n FROM attachment_access_events WHERE link_id=?').bind(file.linkId).first('n'))).toBe(before);
 }
 await privatePeoplePurchase(f.company,'people','draining');expect(Buffer.from(await(await attachmentContent(db,env.H001_FILES,f.claimant,file.linkId,'original')).object.arrayBuffer())).toEqual(privatePeoplePng);await expect(db.transaction(async tx=>{await tx.prepare('SELECT 1').first();return attachmentContent(tx,env.H001_FILES,f.claimant,file.linkId,'original');})).rejects.toMatchObject({code:'PEOPLE_FILE_READ_TRANSACTION_REQUIRED'});
});
it('keeps real newly prepared Payroll run and advance evidence private through draining without granting Finance or a broad records role',async()=>{
 const payroll=await import('@erpx/server-people_payroll/public/people-payroll'),{attachmentContent}=await import('@erpx/server-files/public/attachments/service');const group=await payroll.createPayGroup(f.owner,payroll.payGroupSchema.parse({name:'Actual private file payroll group',countryCode:'TH',currency:'THB',frequency:'monthly',timezone:'Asia/Bangkok'}),key()),run=await payroll.createPayrollRun(f.owner,payroll.payrollRunSchema.parse({payGroupId:group.id,periodStart:'2026-09-01',periodEnd:'2026-09-30',payDate:'2026-10-02',currency:'THB'}),key()),runFile=await nativePeopleFile(f.owner,{targetType:'payroll_run',targetId:String(run.id),componentKind:'action',componentRef:'correction-request',purposeKey:'correction_support',expectedTargetVersion:1});
 await payroll.assignPayGroup(f.owner,`eng-${f.claimant.userId}`,{payGroupId:group.id,expectedVersion:Number(await db.prepare('SELECT version FROM hr_engagements WHERE id=?').bind(`eng-${f.claimant.userId}`).first('version'))},key());
 const advance=await payroll.requestPayrollAdvance(f.claimant,payroll.payrollAdvanceRequestSchema.parse({engagementId:`eng-${f.claimant.userId}`,amount:'100',installmentCount:1,firstDeductionPeriod:'2026-10-01',currency:'THB',reason:'Actual employee-requested advance support'}),key()),advanceFile=await nativePeopleFile(f.claimant,{targetType:'payroll_advance',targetId:String(advance.id),componentKind:'record',purposeKey:'advance_support',expectedTargetVersion:1});
 await privatePeoplePurchase(f.company,'people');await privatePeoplePurchase(f.company,'people-payroll');await privatePeoplePurchase(f.company,'people-payroll','draining');vi.stubEnv('ERPX_PEOPLE_PACKAGE_ENFORCEMENT','enforced');for(const item of [{actor:f.owner,file:runFile},{actor:f.claimant,file:advanceFile}])expect(Buffer.from(await(await attachmentContent(db,env.H001_FILES,item.actor,item.file.linkId,'original')).object.arrayBuffer())).toEqual(privatePeoplePng);
 await expect(attachmentContent(db,env.H001_FILES,f.reviewer,runFile.linkId,'original')).rejects.toMatchObject({code:'ATTACHMENT_LINK_NOT_FOUND'});const before=Number(await db.prepare('SELECT count(*) n FROM attachment_access_events WHERE link_id=?').bind(advanceFile.linkId).first('n')),fault=`payroll_file_source_${key().replaceAll('-','')}`;
 await db.exec(`CREATE FUNCTION ${fault}() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN UPDATE hr_payroll_advances SET reason='Late payroll source' WHERE id='${advance.id}';RETURN NEW;END $$;CREATE CONSTRAINT TRIGGER ${fault} AFTER INSERT ON attachment_access_events DEFERRABLE INITIALLY DEFERRED FOR EACH ROW WHEN(NEW.link_id='${advanceFile.linkId}' AND NEW.outcome='allowed') EXECUTE FUNCTION ${fault}()`);
 try{await expect(attachmentContent(db,env.H001_FILES,f.claimant,advanceFile.linkId,'original')).rejects.toMatchObject({code:'ATTACHMENT_FILE_READ_CHANGED'});}finally{await db.exec(`DROP TRIGGER ${fault} ON attachment_access_events;DROP FUNCTION ${fault}()`);}expect(Number(await db.prepare('SELECT count(*) n FROM attachment_access_events WHERE link_id=?').bind(advanceFile.linkId).first('n'))).toBe(before);expect(await db.prepare('SELECT reason FROM hr_payroll_advances WHERE id=?').bind(advance.id).first('reason')).toBe('Actual employee-requested advance support');
});
it('preserves real People policy denial unavailable audits and rejects initially-invalid authority changes without retrieving bytes',async()=>{
 const {attachmentContent}=await import('@erpx/server-files/public/attachments/service'),personId=`person-${f.claimant.userId}`,file=await nativePeopleFile(f.owner,{targetType:'hr_person',targetId:personId,componentKind:'record',purposeKey:'employment_contract',expectedTargetVersion:1}),reader=await f.member(`unrelated-private-document-${key()}`,'employee');await (await import('../../support/helpers/private-file-unavailable-audits')).retainedUnavailableFileAudits(db,env.H001_FILES,f.claimant,file.linkId,{targetType:'hr_person',targetId:personId,componentKind:'record',purposeKey:'employment_contract',expectedTargetVersion:2});
 await expect(attachmentContent(db,env.H001_FILES,reader,file.linkId,'original')).rejects.toMatchObject({code:'ATTACHMENT_TARGET_NOT_FOUND'});await db.prepare('UPDATE assignments SET revoked_at=1,version=version+1 WHERE id=?').bind(reader.assignmentId).run();await expect(attachmentContent(db,env.H001_FILES,reader,file.linkId,'original')).rejects.toMatchObject({code:'ATTACHMENT_TARGET_NOT_FOUND'});const before=(await db.prepare("SELECT id,reason FROM attachment_access_events WHERE link_id=? AND actor_id=? ORDER BY id").bind(file.linkId,reader.userId).all()).results;expect(before).toHaveLength(2);const version=await db.prepare('SELECT version FROM assignments WHERE id=?').bind(reader.assignmentId).first('version'),fault=`people_invalid_${key().replaceAll('-','')}`,get=vi.spyOn(env.H001_FILES,'get');await db.exec(`CREATE FUNCTION ${fault}() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN UPDATE assignments SET version=version+1 WHERE id='${reader.assignmentId}';RETURN NEW;END $$;CREATE CONSTRAINT TRIGGER ${fault} AFTER INSERT ON attachment_access_events DEFERRABLE INITIALLY DEFERRED FOR EACH ROW WHEN(NEW.link_id='${file.linkId}' AND NEW.outcome='denied') EXECUTE FUNCTION ${fault}()`);
 try{await expect(attachmentContent(db,env.H001_FILES,reader,file.linkId,'original')).rejects.toMatchObject({code:'ATTACHMENT_FILE_READ_CHANGED'});expect(get).not.toHaveBeenCalled();}finally{get.mockRestore();await db.exec(`DROP TRIGGER ${fault} ON attachment_access_events;DROP FUNCTION ${fault}()`);}expect((await db.prepare("SELECT id,reason FROM attachment_access_events WHERE link_id=? AND actor_id=? ORDER BY id").bind(file.linkId,reader.userId).all()).results).toEqual(before);expect(await db.prepare('SELECT version FROM assignments WHERE id=?').bind(reader.assignmentId).first('version')).toBe(version);
});
it('observes a committed original People writer before its read snapshot and preserves two Company private reads with zero retries',async()=>{
 const {attachmentContent}=await import('@erpx/server-files/public/attachments/service'),{committedSourceVisibility}=await import('../../support/helpers/source-writer-visibility'),{measureReads}=await import('@erpx/server-runtime/public/runtime/read-metrics'),{companyPurchaseReadLocks}=await import('@erpx/server-commercial/public/purchase-command-boundary'),other=await expenseFixture(),personId=`person-${f.claimant.userId}`,otherPerson=`person-${other.claimant.userId}`,first=await nativePeopleFile(f.owner,{targetType:'hr_person',targetId:personId,purposeKey:'employment_contract',componentKind:'record',expectedTargetVersion:1}),second=await nativePeopleFile(other.owner,{targetType:'hr_person',targetId:otherPerson,purposeKey:'employment_contract',componentKind:'record',expectedTargetVersion:1}),fault=`people_visible_${key().replaceAll('-','')}`;
 await db.exec(`CREATE FUNCTION ${fault}() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN IF NOT EXISTS(SELECT 1 FROM hr_people WHERE id='${personId}' AND legal_name='Actual committed native reader source') THEN RAISE EXCEPTION 'STALE_PEOPLE_PRIVATE_FILE_SNAPSHOT';END IF;RETURN NEW;END $$;CREATE CONSTRAINT TRIGGER ${fault} AFTER INSERT ON attachment_access_events DEFERRABLE INITIALLY DEFERRED FOR EACH ROW WHEN(NEW.link_id='${first.linkId}' AND NEW.outcome='allowed') EXECUTE FUNCTION ${fault}()`);
 try{const measured=await committedSourceVisibility(db,async tx=>{await tx.batch(companyPurchaseReadLocks(tx,f.company));await tx.prepare("UPDATE hr_people SET legal_name='Actual committed native reader source' WHERE id=?").bind(personId).run();},()=>measureReads(()=>attachmentContent(db,env.H001_FILES,f.claimant,first.linkId,'original')));expect(Buffer.from(await measured.result.object.arrayBuffer())).toEqual(privatePeoplePng);expect(measured.metrics.deadlockFailures??0).toBe(0);expect(measured.metrics.serializationFailures??0).toBe(0);expect(measured.metrics.transactionRetries??0).toBe(0);}finally{await db.exec(`DROP TRIGGER ${fault} ON attachment_access_events;DROP FUNCTION ${fault}()`);}
 const overlap=`people_private_overlap_${key().replaceAll('-','')}`;await db.exec(`CREATE FUNCTION ${overlap}() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN PERFORM pg_sleep(.1);UPDATE hr_people SET legal_name=legal_name WHERE id=CASE WHEN NEW.link_id='${first.linkId}' THEN '${personId}' ELSE '${otherPerson}' END;RETURN NEW;END $$;CREATE CONSTRAINT TRIGGER ${overlap} AFTER INSERT ON attachment_access_events DEFERRABLE INITIALLY DEFERRED FOR EACH ROW WHEN(NEW.link_id IN ('${first.linkId}','${second.linkId}') AND NEW.outcome='allowed') EXECUTE FUNCTION ${overlap}()`);
 try{const measured=await measureReads(()=>Promise.all([{actor:f.claimant,file:first},{actor:other.claimant,file:second}].map(item=>attachmentContent(db,env.H001_FILES,item.actor,item.file.linkId,'original'))));for(const result of measured.result)expect(Buffer.from(await result.object.arrayBuffer())).toEqual(privatePeoplePng);expect(measured.result).toHaveLength(2);expect(measured.metrics.deadlockFailures??0).toBe(0);expect(measured.metrics.serializationFailures??0).toBe(0);expect(measured.metrics.transactionRetries??0).toBe(0);}finally{await db.exec(`DROP TRIGGER ${overlap} ON attachment_access_events;DROP FUNCTION ${overlap}()`);}
});
it('reads a real module-empty employee Overtime request evidence through retained Workforce history and fences its native request source',async()=>{
 const {createOvertime}=await import('@erpx/server-people_payroll/public/overtime'),{overtimeDetailsSchema}=await import('@erpx/feature-people_payroll/lib/overtime'),{attachmentContent}=await import('@erpx/server-files/public/attachments/service');await f.grant(f.it,'hr_admin');const overtime=await createOvertime(f.claimant,overtimeDetailsSchema.parse({mode:'advance',startLocal:'2026-11-10T18:00',endLocal:'2026-11-10T20:00',timezone:'Asia/Bangkok',reason:'Original employee planned shift evidence'}),key()),file=await nativePeopleFile(f.claimant,{targetType:'overtime_request',targetId:String(overtime.id),componentKind:'action',componentRef:'request-submission',purposeKey:'planned_evidence',expectedTargetVersion:1});await privatePeoplePurchase(f.company,'people');await privatePeoplePurchase(f.company,'people-workforce');await privatePeoplePurchase(f.company,'people-workforce','draining');vi.stubEnv('ERPX_PEOPLE_PACKAGE_ENFORCEMENT','enforced');expect(Buffer.from(await(await attachmentContent(db,env.H001_FILES,f.claimant,file.linkId,'original')).object.arrayBuffer())).toEqual(privatePeoplePng);const before=Number(await db.prepare('SELECT count(*) n FROM attachment_access_events WHERE link_id=?').bind(file.linkId).first('n')),fault=`overtime_private_${key().replaceAll('-','')}`;
 await db.exec(`CREATE FUNCTION ${fault}() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN UPDATE overtime_requests SET reason='Late native overtime source' WHERE id='${overtime.id}';RETURN NEW;END $$;CREATE CONSTRAINT TRIGGER ${fault} AFTER INSERT ON attachment_access_events DEFERRABLE INITIALLY DEFERRED FOR EACH ROW WHEN(NEW.link_id='${file.linkId}' AND NEW.outcome='allowed') EXECUTE FUNCTION ${fault}()`);
 try{await expect(attachmentContent(db,env.H001_FILES,f.claimant,file.linkId,'original')).rejects.toMatchObject({code:'ATTACHMENT_FILE_READ_CHANGED'});}finally{await db.exec(`DROP TRIGGER ${fault} ON attachment_access_events;DROP FUNCTION ${fault}()`);}expect(await db.prepare('SELECT reason FROM overtime_requests WHERE id=?').bind(overtime.id).first('reason')).toBe('Original employee planned shift evidence');expect(Number(await db.prepare('SELECT count(*) n FROM attachment_access_events WHERE link_id=?').bind(file.linkId).first('n'))).toBe(before);
});
