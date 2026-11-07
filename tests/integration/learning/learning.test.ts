import { afterEach,beforeAll, beforeEach, expect, it, vi } from "vitest";
import { applyMigrations, env } from "../../support/runtime-env";
import {
  expenseDb as db,
  expenseFixture,
  key,
} from "../../support/helpers/expense-fixture";
import {
  changeLearningAssignment,
  changeLearningCourse,
  createLearningAssignment,
  createLearningCourse,
  learningAssignmentDetail,
  learningWork,
  learningWorkspace,
} from "@erpx/server-learning/public/learning";
import {
  attachmentBindingState,
  enableAttachmentV2ForTest,
  seedAttachmentSet,
} from "../../support/helpers/attachment-v2";
import {learningAttachmentPurchaseScope,learningAttachmentSourceReadLocks} from "@erpx/server-learning/public/attachment-purchases";
import {measureReads} from "@erpx/server-runtime/public/runtime/read-metrics";
import {packageGrantStore,readCompanyPackageGrants} from "@erpx/server-commercial/public/package-grants";
import {packageGrantRevision} from "@erpx/server-commercial/public/package-grants";
import {withOwnerPackageObligations,readOwnerPackageObligations,ownerPackageObligations} from "@erpx/server-commercial/public/owner-package-obligations";
import {learningPackageObligations} from "@erpx/server-learning/public/package-obligations";
import {revokeLearningCertificate} from "@erpx/server-learning/public/learning-eligibility";
import {listAttachmentSets,bindRecordAttachmentSet,attachmentContent} from "@erpx/server-files/public/attachments/service";
import type {FileStore} from '@erpx/server-runtime/public/runtime/types';
vi.mock("@erpx/server-runtime/adapters/runtime", () => ({
  holdingResources: async () => ({ write: env.H001_DB, files: env.H001_FILES }),
  runtimeEnv: async () => env,
}));
let f: Awaited<ReturnType<typeof expenseFixture>>;
beforeAll(async () => {
  await applyMigrations(db, env.STAFF_TEST_MIGRATIONS);
  enableAttachmentV2ForTest();
});
beforeEach(async () => {
  f = await expenseFixture();
  for (const a of [f.owner, f.tie, f.claimant, f.head]){
    a.modules = ["learning"];
    await db.prepare("UPDATE assignments SET modules_json='[\"learning\"]' WHERE id=?").bind(a.assignmentId).run();
  }
});
afterEach(()=>vi.unstubAllEnvs());
async function learningGrant(state:'active'|'draining'='active',endsAt:number|null=null,companyId=f.company){
 const before=(await readCompanyPackageGrants(db,companyId)).find(row=>row.packageId==='learn');
 return packageGrantStore(db,{authorize:async()=>({userId:'isolated-learning-provider'}),obligations:async()=>{throw new Error('LEARNING_WITHDRAWAL_NOT_IMPLEMENTED');}}).apply({expectedVersion:before?.version??0,idempotencyKey:key(),grant:{companyId,packageId:'learn',packageVersion:1,state,startsAt:0,endsAt,version:(before?.version??0)+1,reason:'Isolated actual Learning owner workflow',evidence:'test:learning-owner-purchase'}});
}
const learningCourseDraft={title:'Package-only practical course',description:'Actual retained learner assessment and certificate workflow.',provider:'Isolated development academy',durationMinutes:60,requiredFor:'equipment_operator'};
const learningSettlementInput=()=>({companyId:f.company,packageId:'learn',packageVersion:1});
async function learningSettlement(){return withOwnerPackageObligations(db,learningSettlementInput(),tx=>readOwnerPackageObligations(db,tx,{...learningSettlementInput(),now:Date.now()}));}
async function learningWithdrawal(){
 const grants=await readCompanyPackageGrants(db,f.company),current=grants.find(grant=>grant.packageId==='learn')!;
 return {companyId:f.company,expectedRevision:packageGrantRevision(f.company,grants),idempotencyKey:key(),changes:[{expectedVersion:current.version,
  grant:{...current,state:'inactive' as const,version:current.version+1,reason:'Actual original Learning obligations reviewed',evidence:'test:learning-concrete-settlement'}}]};
}
function learningWithdrawalReview(request:Awaited<ReturnType<typeof learningWithdrawal>>,apply=false){
 const store=packageGrantStore(db,{authorize:async()=>({userId:'isolated-learning-settlement-provider'}),obligations:(tx,companyId,packageId)=>ownerPackageObligations(db,tx,companyId,packageId)});
 return withOwnerPackageObligations(db,learningSettlementInput(),tx=>apply?store.batch.applyIn(tx,request,async()=>{}):store.batch.previewIn(tx,request));
}

it('withdraws Learning through both real owners after completion while retaining original certificate, exact replay and Legal-held evidence without buying People',async()=>{
 vi.stubEnv('ERPX_LEARNING_PACKAGE_ENFORCEMENT','enforced');vi.stubEnv('ERPX_PEOPLE_PACKAGE_ENFORCEMENT','enforced');
 await learningGrant();const id=await assignment();await learningGrant('draining');
 const opened=await learningSettlement();expect(opened.observations.map(row=>row.moduleId)).toEqual(['learning','people_payroll']);expect(opened.total).toBeGreaterThan(0);
 expect(JSON.stringify(opened)).not.toMatch(/result_evidence|authority_json|person_id|description/);
 const blocked=await learningWithdrawal();await expect(learningWithdrawalReview(blocked)).rejects.toThrow('PACKAGE_OBLIGATIONS_REMAIN');await expect(learningWithdrawalReview(blocked,true)).rejects.toThrow('PACKAGE_OBLIGATIONS_REMAIN');
 await changeLearningAssignment(f.claimant,id,{action:'start',expectedVersion:1,reason:'Continue the original assigned learner work'},key());
 const evidence=await seedAttachmentSet(db,f.claimant,{targetType:'learning_assignment',targetId:id,componentKind:'action',componentRef:'completion',purposeKey:'certificate',expectedTargetVersion:2});
 const receipt=key(),input={action:'complete',expectedVersion:2,attendance:'attended',scoreBps:9500,evidence:'Observed original completion',reason:'Complete the retained learner obligation',attachmentSetIds:[evidence.setId]};
 const completed=await changeLearningAssignment(f.claimant,id,input,receipt),detail=await learningAssignmentDetail(f.claimant,id);
 expect(detail.certificate).toMatchObject({company_id:f.company,source_assignment_version:3,completion_kind:'completed'});
 const objectKey=String(await db.prepare('SELECT object_key FROM attachments WHERE id=?').bind(evidence.sourceId).first('object_key')),body='Original Learning withdrawal evidence under Legal hold';
 await env.H001_FILES.put(objectKey,body,{httpMetadata:{contentType:'application/pdf'}});await db.prepare('UPDATE attachments SET legal_hold=1 WHERE id=?').bind(evidence.sourceId).run();
 const settled=await learningSettlement();expect(settled.total).toBe(0);expect(settled.observations.every(row=>row.outstanding===0)).toBe(true);
 const request=await learningWithdrawal(),preview=await learningWithdrawalReview(request),applied=await learningWithdrawalReview(request,true);expect(applied).toEqual(preview);expect(await learningWithdrawalReview(request,true)).toEqual(applied);
 expect((await learningAssignmentDetail(f.claimant,id)).certificate).toEqual(detail.certificate);expect(await changeLearningAssignment(f.claimant,id,input,receipt)).toEqual(completed);
 expect(await (await attachmentContent(db,env.H001_FILES,f.claimant,evidence.linkId,'original')).object.text()).toBe(body);
 expect((await readCompanyPackageGrants(db,f.company)).map(grant=>[grant.packageId,grant.state])).toEqual([['learn','inactive']]);
 expect(await db.prepare('SELECT 1 FROM learning_benefit_consumption_receipts WHERE company_id=?').bind(f.company).first()).toBeNull();
 await expect(createLearningCourse(f.owner,learningCourseDraft,key())).rejects.toMatchObject({code:'PACKAGE_INACTIVE'});
 await db.prepare('UPDATE assignments SET revoked_at=1,version=version+1 WHERE id=?').bind(f.claimant.assignmentId).run();
 await expect(changeLearningAssignment(f.claimant,id,input,receipt)).rejects.toMatchObject({code:'LEARNING_ACCESS_CHANGED'});await expect(attachmentContent(db,env.H001_FILES,f.claimant,evidence.linkId,'original')).rejects.toMatchObject({status:404});
 expect((await learningSettlement()).total).toBe(0);expect(await (await env.H001_FILES.get(objectKey))?.text()).toBe(body);
});

it('retains ambiguous legacy Learning outcomes and foreign course lineage as actual withdrawal obligations',async()=>{
 await learningGrant();const id=await assignment();
 await db.prepare("UPDATE learning_assignments SET status='completed',completion_reason='Legacy outcome without original terminal command',version=version+1 WHERE id=?").bind(id).run();
 await learningGrant('draining');const observed=await learningSettlement();expect(observed.observations.find(row=>row.moduleId==='learning')?.outstanding).toBeGreaterThan(0);
 await expect(learningWithdrawalReview(await learningWithdrawal())).rejects.toThrow('PACKAGE_OBLIGATIONS_REMAIN');
 const foreignCourse=key(),foreignAssignment=key(),now=Date.now();
 await db.prepare("INSERT INTO learning_courses(id,company_id,title,description,provider,duration_minutes,status,created_by,created_at,updated_at) VALUES(?,?,'Foreign original course','','Original academy',30,'draft',?,?,?)").bind(foreignCourse,'other-company',f.owner.userId,now,now).run();
 await db.prepare("INSERT INTO learning_assignments(id,company_id,branch_id,department_id,course_id,course_version,learner_id,due_date,created_by,created_at,updated_at) VALUES(?,?,?,?,?,1,?,'2026-10-01',?,?,?)").bind(foreignAssignment,f.company,f.branch,f.department,foreignCourse,f.claimant.userId,f.owner.userId,now,now).run();
 expect((await learningSettlement()).observations.find(row=>row.moduleId==='learning')?.outstanding).toBe((observed.observations.find(row=>row.moduleId==='learning')?.outstanding??0)+1);
});

it('requires minted Learning source leases for the exact Company, module and supported package version',async()=>{
 const input={...learningSettlementInput(),moduleId:'learning',now:Date.now()};
 await expect(db.transaction(tx=>learningPackageObligations(db,tx,input))).rejects.toThrow('LEARNING_PACKAGE_OBLIGATION_SOURCE_REQUIRED');
 expect(()=>withOwnerPackageObligations(db,{...learningSettlementInput(),packageVersion:2},tx=>learningPackageObligations(db,tx,{...input,packageVersion:2}))).toThrow('PACKAGE_OBLIGATION_SOURCES_REQUIRED');
 await expect(withOwnerPackageObligations(db,learningSettlementInput(),tx=>learningPackageObligations(db,tx,{...input,moduleId:'people_payroll'}))).rejects.toThrow('LEARNING_PACKAGE_OBLIGATION_SCOPE_REQUIRED');
 await expect(withOwnerPackageObligations(db,learningSettlementInput(),tx=>learningPackageObligations(db,tx,{...input,companyId:'foreign-company'}))).rejects.toThrow('LEARNING_PACKAGE_OBLIGATION_SOURCE_REQUIRED');
 expect((await learningSettlement()).total).toBe(0);
});

it('rolls normal Commercial inactivation and its receipt back when a final deferred Learning source creates an obligation',async()=>{
 await learningGrant();await learningGrant('draining');const request=await learningWithdrawal(),courseId=`learning-late-${key()}`,name=`learning_final_settlement_${key().replaceAll('-','')}`,now=Date.now();
 await db.prepare(`CREATE FUNCTION ${name}() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN INSERT INTO learning_courses(id,company_id,title,description,provider,duration_minutes,created_by,created_at,updated_at) VALUES('${courseId}','${f.company}','Actual deferred draft','','Original academy',30,'${f.owner.userId}',${now},${now}); RETURN NEW; END $$`).run();
 await db.prepare(`CREATE CONSTRAINT TRIGGER ${name} AFTER INSERT ON company_package_change_sets DEFERRABLE INITIALLY DEFERRED FOR EACH ROW WHEN(NEW.id='${request.idempotencyKey}') EXECUTE FUNCTION ${name}()`).run();
 try{await expect(learningWithdrawalReview(request,true)).rejects.toThrow('PACKAGE_OBLIGATIONS_REMAIN');}
 finally{await db.prepare(`DROP TRIGGER ${name} ON company_package_change_sets`).run();await db.prepare(`DROP FUNCTION ${name}()`).run();}
 expect(await db.prepare("SELECT state,version FROM company_package_grants WHERE company_id=? AND package_id='learn'").bind(f.company).first()).toEqual({state:'draining',version:2});
 expect(await db.prepare('SELECT 1 FROM company_package_change_sets WHERE id=?').bind(request.idempotencyKey).first()).toBeNull();expect(await db.prepare('SELECT 1 FROM learning_courses WHERE id=?').bind(courseId).first()).toBeNull();
 expect((await learningSettlement()).total).toBe(0);expect((await learningWithdrawalReview(request,true)).after.find(grant=>grant.packageId==='learn')?.state).toBe('inactive');
});

it('waits before the first settlement snapshot and sees a concurrently committed original Learning draft',async()=>{
 await learningGrant();await learningGrant('draining');const request=await learningWithdrawal(),courseId=`learning-concurrent-${key()}`,now=Date.now();
 let inserted!:()=>void,release!:()=>void;const ready=new Promise<void>(resolve=>{inserted=resolve;}),released=new Promise<void>(resolve=>{release=resolve;});
 const writer=db.transaction(async tx=>{
  await tx.prepare('LOCK TABLE company_package_grants IN ROW EXCLUSIVE MODE').run();
  await tx.prepare("INSERT INTO learning_courses(id,company_id,title,description,provider,duration_minutes,created_by,created_at,updated_at) VALUES(?,?,'Actual concurrent draft','','Original academy',30,?,?,?)").bind(courseId,f.company,f.owner.userId,now,now).run();inserted();await released;
 });
 await ready;const review=learningWithdrawalReview(request).then(value=>({value,error:null}),error=>({value:null,error}));
 try{
  let waiting=false;
  for(let attempt=0;attempt<100;attempt++){
   waiting=Boolean(await db.prepare("SELECT 1 FROM pg_locks WHERE database=(SELECT oid FROM pg_database WHERE datname=current_database()) AND relation='company_package_grants'::regclass AND mode='ShareRowExclusiveLock' AND NOT granted LIMIT 1").first());
   if(waiting)break;await new Promise(resolve=>setTimeout(resolve,10));
  }
  expect(waiting).toBe(true);
 }finally{release();await writer;}
 expect((await review).error).toMatchObject({message:'PACKAGE_OBLIGATIONS_REMAIN'});expect((await learningSettlement()).total).toBeGreaterThan(0);
 expect(await db.prepare('SELECT status FROM learning_courses WHERE id=?').bind(courseId).first('status')).toBe('draft');
});

it('denies unpurchased Learning work, preserves shadow owner authority, and settles original learner results and certificates while draining without buying People',async()=>{
 vi.stubEnv('ERPX_LEARNING_PACKAGE_ENFORCEMENT','enforced');vi.stubEnv('ERPX_PEOPLE_PACKAGE_ENFORCEMENT','enforced');
 await expect(createLearningCourse(f.owner,learningCourseDraft,key())).rejects.toMatchObject({code:'PACKAGE_REQUIRED'});
 await expect(learningWorkspace(f.owner)).rejects.toMatchObject({code:'PACKAGE_REQUIRED'});expect(await learningWork(f.claimant)).toEqual([]);
 await learningGrant();const id=await assignment();await learningGrant('draining');
 await expect(createLearningCourse(f.owner,learningCourseDraft,key())).rejects.toMatchObject({status:403});
 const controls=(await learningWorkspace(f.claimant)).purchases;expect(controls?.['learn.assignments']).toMatchObject({newWork:false,settleExisting:true,history:true});
 expect(JSON.stringify(controls)).not.toMatch(/grants|evidence|startsAt|actorId|codes/);
 await changeLearningAssignment(f.claimant,id,{action:'start',expectedVersion:1,reason:'Continue existing assigned course'},key());
 const evidence=await seedAttachmentSet(db,f.claimant,{targetType:'learning_assignment',targetId:id,componentKind:'action',componentRef:'completion',purposeKey:'certificate',expectedTargetVersion:2});
 const receipt=key(),input={action:'complete',expectedVersion:2,attendance:'attended',scoreBps:9500,evidence:'Original observed learner completion evidence',reason:'Existing learning completed while draining',attachmentSetIds:[evidence.setId]};
 const completed=await changeLearningAssignment(f.claimant,id,input,receipt);expect(completed).toMatchObject({id,version:3,status:'completed'});expect(await changeLearningAssignment(f.claimant,id,input,receipt)).toEqual(completed);
 expect(await attachmentBindingState(db,evidence.setId)).toMatchObject({set_state:'bound',link_state:'bound',binding_type:'learning_completion'});
 const detail=await learningAssignmentDetail(f.claimant,id),certificate=String(detail.certificate?.id);expect(detail.certificate).toMatchObject({eligibility_code:'equipment_operator',source_assignment_version:3,completion_kind:'completed'});
 const objectKey=String(await db.prepare('SELECT object_key FROM attachments WHERE id=?').bind(evidence.sourceId).first('object_key')),body='Original learner evidence retained under Legal hold';await env.H001_FILES.put(objectKey,body,{httpMetadata:{contentType:'application/pdf'}});await db.prepare('UPDATE attachments SET legal_hold=1 WHERE id=?').bind(evidence.sourceId).run();expect(await (await attachmentContent(db,env.H001_FILES,f.claimant,evidence.linkId,'original')).object.text()).toBe(body);
 const revokeKey=key(),revokeInput={expectedAssignmentVersion:3,reason:'Independent owner retires this original retained certificate'};expect(await revokeLearningCertificate(f.tie,certificate,revokeInput,revokeKey)).toMatchObject({id:certificate,status:'revoked'});expect(await revokeLearningCertificate(f.tie,certificate,revokeInput,revokeKey)).toMatchObject({id:certificate,status:'revoked'});
 expect((await readCompanyPackageGrants(db,f.company)).map(row=>row.packageId)).toEqual(['learn']);
 vi.stubEnv('ERPX_LEARNING_PACKAGE_ENFORCEMENT','shadow');await expect(changeLearningAssignment(f.owner,id,{action:'start',expectedVersion:3,reason:'Cannot impersonate the assigned learner'},key())).rejects.toMatchObject({code:'LEARNING_LEARNER_REQUIRED'});
 await db.prepare('UPDATE assignments SET revoked_at=1,version=version+1 WHERE id=?').bind(f.claimant.assignmentId).run();await expect(learningWorkspace(f.claimant)).rejects.toMatchObject({code:'LEARNING_ACCESS_CHANGED'});await expect(attachmentContent(db,env.H001_FILES,f.claimant,evidence.linkId,'original')).rejects.toMatchObject({status:404});expect(await (await env.H001_FILES.get(objectKey))?.text()).toBe(body);
});

it('rolls complete Learning owner and attachment effects back after deferred source or IAM changes, then retries the unchanged receipt once',async()=>{
 await learningGrant();vi.stubEnv('ERPX_LEARNING_PACKAGE_ENFORCEMENT','enforced');vi.stubEnv('ERPX_PEOPLE_PACKAGE_ENFORCEMENT','enforced');
 const id=await assignment();await changeLearningAssignment(f.claimant,id,{action:'start',expectedVersion:1,reason:'Start the original learner assignment'},key());
 const evidence=await seedAttachmentSet(db,f.claimant,{targetType:'learning_assignment',targetId:id,componentKind:'action',componentRef:'completion',purposeKey:'certificate',expectedTargetVersion:2});
 const receipt=key(),input={action:'complete',expectedVersion:2,attendance:'attended',scoreBps:9400,evidence:'Actual source evidence retained for atomic retry',reason:'Complete original assignment with final source validation',attachmentSetIds:[evidence.setId]},name=`learning_final_iam_${key().replaceAll('-','')}`;
 await db.prepare(`CREATE FUNCTION ${name}() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN UPDATE assignments SET revoked_at=1,version=version+1 WHERE id='${f.claimant.assignmentId}'; RETURN NEW; END $$`).run();
 await db.prepare(`CREATE CONSTRAINT TRIGGER ${name} AFTER INSERT ON command_receipts DEFERRABLE INITIALLY DEFERRED FOR EACH ROW WHEN (NEW.command_id='${receipt}') EXECUTE FUNCTION ${name}()`).run();
 try{await expect(changeLearningAssignment(f.claimant,id,input,receipt)).rejects.toMatchObject({code:'LEARNING_SOURCE_CHANGED'});}finally{await db.prepare(`DROP TRIGGER ${name} ON command_receipts`).run();await db.prepare(`DROP FUNCTION ${name}()`).run();}
 expect((await learningAssignmentDetail(f.claimant,id)).assignment).toMatchObject({version:2,status:'in_progress'});expect(await attachmentBindingState(db,evidence.setId)).toMatchObject({set_state:'draft',link_state:'draft'});
 for(const table of ['command_receipts','audit_events','outbox_events'])expect(await db.prepare(`SELECT 1 FROM ${table} WHERE command_id=?`).bind(receipt).first()).toBeNull();
 expect(await db.prepare('SELECT 1 FROM learning_certificates WHERE assignment_id=?').bind(id).first()).toBeNull();expect(await db.prepare('SELECT revoked_at FROM assignments WHERE id=?').bind(f.claimant.assignmentId).first()).toEqual({revoked_at:null});
 expect(await changeLearningAssignment(f.claimant,id,input,receipt)).toMatchObject({id,version:3,status:'completed'});expect(await changeLearningAssignment(f.claimant,id,input,receipt)).toMatchObject({id,version:3,status:'completed'});
});

it('rejects a new course after final purchase expiry without leaving its receipt or draft, then accepts the original retry under a normal renewed grant',async()=>{
 await learningGrant('active',Date.now()+1800);vi.stubEnv('ERPX_LEARNING_PACKAGE_ENFORCEMENT','enforced');
 const receipt=key(),name=`learning_final_expiry_${key().replaceAll('-','')}`;
 await db.prepare(`CREATE FUNCTION ${name}() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN PERFORM pg_sleep(1.9); RETURN NEW; END $$`).run();await db.prepare(`CREATE CONSTRAINT TRIGGER ${name} AFTER INSERT ON command_receipts DEFERRABLE INITIALLY DEFERRED FOR EACH ROW WHEN (NEW.command_id='${receipt}') EXECUTE FUNCTION ${name}()`).run();
 try{await expect(createLearningCourse(f.owner,learningCourseDraft,receipt)).rejects.toMatchObject({code:'PACKAGE_CAPABILITY_CHANGED'});}finally{await db.prepare(`DROP TRIGGER ${name} ON command_receipts`).run();await db.prepare(`DROP FUNCTION ${name}()`).run();}
 expect(await db.prepare('SELECT 1 FROM learning_courses WHERE id=?').bind(`learning-course-${receipt}`).first()).toBeNull();for(const table of ['command_receipts','audit_events','outbox_events'])expect(await db.prepare(`SELECT 1 FROM ${table} WHERE command_id=?`).bind(receipt).first()).toBeNull();
 await learningGrant();const result=await createLearningCourse(f.owner,learningCourseDraft,receipt);expect(await createLearningCourse(f.owner,learningCourseDraft,receipt)).toEqual(result);
});

it('retains Learning material bytes and original sensitive file policy while fencing new content and deferred binding authority',async()=>{
 const c=await createLearningCourse(f.owner,learningCourseDraft,key()),id=String(c.id),material=await seedAttachmentSet(db,f.owner,{targetType:'learning_course',targetId:id,componentKind:'record',purposeKey:'course_material',expectedTargetVersion:1});
 vi.stubEnv('ERPX_LEARNING_PACKAGE_ENFORCEMENT','enforced');vi.stubEnv('ERPX_PEOPLE_PACKAGE_ENFORCEMENT','enforced');
 await expect(listAttachmentSets(db,f.owner,{targetType:'learning_course',targetId:id,purposeKey:'course_material',componentKind:'record'})).rejects.toMatchObject({code:'PACKAGE_REQUIRED'});
 await expect(bindRecordAttachmentSet(db,f.owner,material.setId,1,key())).rejects.toMatchObject({code:'PACKAGE_REQUIRED'});await learningGrant();
 const first=key();await bindRecordAttachmentSet(db,f.owner,material.setId,1,first);await bindRecordAttachmentSet(db,f.owner,material.setId,1,first);
 const objectKey=String(await db.prepare('SELECT object_key FROM attachments WHERE id=?').bind(material.sourceId).first('object_key')),body='Reviewed course material retained under Legal hold';await env.H001_FILES.put(objectKey,body,{httpMetadata:{contentType:'application/pdf'}});await db.prepare('UPDATE attachments SET legal_hold=1 WHERE id=?').bind(material.sourceId).run();
 const second=await seedAttachmentSet(db,f.owner,{targetType:'learning_course',targetId:id,componentKind:'record',purposeKey:'course_material',expectedTargetVersion:2});await learningGrant('draining');
 expect(await (await attachmentContent(db,env.H001_FILES,f.owner,material.linkId,'original')).object.text()).toBe(body);await expect(bindRecordAttachmentSet(db,f.owner,second.setId,2,key())).rejects.toMatchObject({status:403});
 await learningGrant();const receipt=key(),name=`learning_file_iam_${key().replaceAll('-','')}`;
 await db.prepare(`CREATE FUNCTION ${name}() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN UPDATE assignments SET revoked_at=1,version=version+1 WHERE id='${f.owner.assignmentId}'; RETURN NEW; END $$`).run();await db.prepare(`CREATE CONSTRAINT TRIGGER ${name} AFTER INSERT ON command_receipts DEFERRABLE INITIALLY DEFERRED FOR EACH ROW WHEN (NEW.command_id='${receipt}') EXECUTE FUNCTION ${name}()`).run();
 try{await expect(bindRecordAttachmentSet(db,f.owner,second.setId,2,receipt)).rejects.toMatchObject({status:409});}finally{await db.prepare(`DROP TRIGGER ${name} ON command_receipts`).run();await db.prepare(`DROP FUNCTION ${name}()`).run();}
 expect(await attachmentBindingState(db,second.setId)).toMatchObject({set_state:'draft',link_state:'draft'});expect(await db.prepare('SELECT version FROM learning_courses WHERE id=?').bind(id).first()).toEqual({version:2});expect(await db.prepare('SELECT revoked_at FROM assignments WHERE id=?').bind(f.owner.assignmentId).first()).toEqual({revoked_at:null});for(const table of ['command_receipts','audit_events','outbox_events'])expect(await db.prepare(`SELECT 1 FROM ${table} WHERE command_id=?`).bind(receipt).first()).toBeNull();
 await bindRecordAttachmentSet(db,f.owner,second.setId,2,receipt);expect(await attachmentBindingState(db,second.setId)).toMatchObject({set_state:'bound',link_state:'bound'});expect(await (await env.H001_FILES.get(objectKey))?.text()).toBe(body);
});

it("keeps My learning limited to the actor's assignments even for a workspace manager",async()=>{
  const id=await assignment();
  expect((await learningWorkspace(f.owner)).assignments.map(row=>row.id)).toContain(id);
  expect((await learningWorkspace(f.owner,true)).assignments).toEqual([]);
  expect((await learningWorkspace(f.claimant,true)).assignments.map(row=>row.id)).toContain(id);
});
async function course(requiredFor: string | null = "equipment_operator") {
  const x = await createLearningCourse(
    f.owner,
    {
      title: "Safe equipment operation",
      description: "Practical operating and emergency-stop procedure.",
      provider: "Internal academy",
      durationMinutes: 90,
      requiredFor,
    },
    key(),
  );
  await expect(
    changeLearningCourse(
      f.owner,
      String(x.id),
      {
        action: "publish",
        expectedVersion: 1,
        reason: "Self review is forbidden",
      },
      key(),
    ),
  ).rejects.toMatchObject({ code: "CONFLICT" });
  await changeLearningCourse(
    f.tie,
    String(x.id),
    {
      action: "publish",
      expectedVersion: 1,
      reason: "Independent reviewer checked content and audience",
    },
    key(),
  );
  return String(x.id);
}
async function assignment() {
  const c = await course();
  return String(
    (
      await createLearningAssignment(
        f.owner,
        {
          courseId: c,
          courseVersion: 2,
          learnerId: f.claimant.userId,
          dueDate: "2026-10-01",
          reason: "Required before operating production equipment",
        },
        key(),
      )
    ).id,
  );
}
it.each(['learning_course','learning_assignment'] as const)('buffers actual %s private bytes with native learner/course guards and no full People purchase',async target=>{
 enableAttachmentV2ForTest();await learningGrant();vi.stubEnv('ERPX_LEARNING_PACKAGE_ENFORCEMENT','enforced');vi.stubEnv('ERPX_PEOPLE_PACKAGE_ENFORCEMENT','enforced');const assignmentId=await assignment(),courseId=await db.prepare('SELECT course_id FROM learning_assignments WHERE id=?').bind(assignmentId).first<string>('course_id');if(!courseId)throw Error('ACTUAL_LEARNER_COURSE_REQUIRED');
 const targetId=target==='learning_course'?courseId:assignmentId,actor=target==='learning_course'?f.owner:f.claimant,evidence=await seedAttachmentSet(db,actor,{targetType:target,targetId,componentKind:'record',purposeKey:target==='learning_course'?'course_material':'certificate',expectedTargetVersion:target==='learning_course'?2:1});
 if(target==='learning_course')await bindRecordAttachmentSet(db,f.owner,evidence.setId,2,key());
 const objectKey=await db.prepare('SELECT object_key FROM attachments WHERE id=?').bind(evidence.sourceId).first<string>('object_key');if(!objectKey)throw Error('ACTUAL_LEARNING_FILE_REQUIRED');const bytes='Original included learner evidence and course material';await env.H001_FILES.put(objectKey,bytes);let consumed=0;
 const files:FileStore={put:(...args)=>env.H001_FILES.put(...args),get:async(...args)=>{const result=await env.H001_FILES.get(...args);return result?{...result,arrayBuffer:async()=>{consumed++;return result.arrayBuffer();}}:null;},head:(...args)=>env.H001_FILES.head(...args),delete:(...args)=>env.H001_FILES.delete(...args),list:(...args)=>env.H001_FILES.list(...args)};
 for(const kind of ['course','learner','file','iam'] as const){
  const fault=`learning_private_${key().replaceAll('-','')}`,effect=kind==='course'?`UPDATE learning_courses SET title='Late native course content' WHERE id='${courseId}'`:kind==='learner'?`UPDATE learning_assignments SET due_date='2027-01-01' WHERE id='${assignmentId}'`:kind==='file'?`UPDATE attachments SET legal_hold=1 WHERE id='${evidence.sourceId}'`:`UPDATE assignments SET revoked_at=1,version=version+1 WHERE id='${f.claimant.assignmentId}'`;
  await db.exec(`CREATE FUNCTION ${fault}() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN ${effect};RETURN NEW;END $$;CREATE CONSTRAINT TRIGGER ${fault} AFTER INSERT ON attachment_access_events DEFERRABLE INITIALLY DEFERRED FOR EACH ROW WHEN(NEW.link_id='${evidence.linkId}' AND NEW.outcome='allowed') EXECUTE FUNCTION ${fault}()`);
  try{await expect(attachmentContent(db,files,f.claimant,evidence.linkId,'original')).rejects.toMatchObject({code:'LEARNING_FILE_READ_CHANGED'});}finally{await db.exec(`DROP TRIGGER ${fault} ON attachment_access_events;DROP FUNCTION ${fault}()`);}
  expect(await db.prepare("SELECT count(*)::int n FROM attachment_access_events WHERE link_id=? AND outcome='allowed'").bind(evidence.linkId).first('n')).toBe(0);
 }
 expect(consumed).toBe(4);await learningGrant('draining');expect(await(await attachmentContent(db,files,f.claimant,evidence.linkId,'original')).object.text()).toBe(bytes);expect(consumed).toBe(5);expect((await readCompanyPackageGrants(db,f.company)).map(row=>[row.packageId,row.state])).toEqual([['learn','draining']]);
 await expect(db.transaction(tx=>attachmentContent(tx,files,f.claimant,evidence.linkId,'original'))).rejects.toMatchObject({code:'LEARNING_FILE_READ_TRANSACTION_REQUIRED'});expect(consumed).toBe(5);
 await db.prepare('UPDATE assignments SET revoked_at=1,version=version+1 WHERE id=?').bind(f.claimant.assignmentId).run();await expect(attachmentContent(db,files,f.claimant,evidence.linkId,'original')).rejects.toMatchObject({status:404});expect(await db.prepare("SELECT count(*)::int n FROM attachment_access_events WHERE link_id=? AND actor_id=? AND outcome='denied'").bind(evidence.linkId,f.claimant.userId).first('n')).toBe(1);expect(consumed).toBe(5);
});
it('observes committed native Learning sources before the byte snapshot and isolates two Companies without hidden retries',async()=>{
 const {committedSourceVisibility}=await import('../../support/helpers/source-writer-visibility'),{companyPurchaseReadLocks}=await import('@erpx/server-commercial/public/purchase-command-boundary');enableAttachmentV2ForTest();const other=await expenseFixture();other.owner.modules=['learning'];await db.prepare("UPDATE assignments SET modules_json='[\"learning\"]' WHERE id=?").bind(other.owner.assignmentId).run();await learningGrant();await learningGrant('active',null,other.company);vi.stubEnv('ERPX_LEARNING_PACKAGE_ENFORCEMENT','enforced');vi.stubEnv('ERPX_PEOPLE_PACKAGE_ENFORCEMENT','enforced');const rows:{fixture:typeof f;id:string;evidence:Awaited<ReturnType<typeof seedAttachmentSet>>;bytes:string}[]=[];
 for(const fixture of [f,other]){const course=await createLearningCourse(fixture.owner,learningCourseDraft,key()),evidence=await seedAttachmentSet(db,fixture.owner,{targetType:'learning_course',targetId:String(course.id),componentKind:'record',purposeKey:'course_material',expectedTargetVersion:1});await bindRecordAttachmentSet(db,fixture.owner,evidence.setId,1,key());const objectKey=await db.prepare('SELECT object_key FROM attachments WHERE id=?').bind(evidence.sourceId).first<string>('object_key');if(!objectKey)throw Error('ACTUAL_LEARNING_PRIVATE_FILE_REQUIRED');const bytes=`Retained Learning ${fixture.company}`;await env.H001_FILES.put(objectKey,bytes);rows.push({fixture,id:String(course.id),evidence,bytes});}
 const first=rows[0],fault=`learning_bytes_visible_${key().replaceAll('-','')}`;await db.exec(`CREATE FUNCTION ${fault}() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN IF (SELECT title FROM learning_courses WHERE id='${first.id}')<>'Committed Learning material source' THEN RAISE EXCEPTION 'STALE_LEARNING_FILE_SOURCE';END IF;RETURN NEW;END $$;CREATE CONSTRAINT TRIGGER ${fault} AFTER INSERT ON attachment_access_events DEFERRABLE INITIALLY DEFERRED FOR EACH ROW WHEN(NEW.link_id='${first.evidence.linkId}' AND NEW.outcome='allowed') EXECUTE FUNCTION ${fault}()`);
 try{const observed=await committedSourceVisibility(db,async tx=>{await tx.batch(companyPurchaseReadLocks(tx,f.company));await tx.prepare("UPDATE learning_courses SET title='Committed Learning material source',version=version+1 WHERE id=?").bind(first.id).run();},()=>measureReads(()=>attachmentContent(db,env.H001_FILES,f.owner,first.evidence.linkId,'original')));expect(await observed.result.object.text()).toBe(first.bytes);expect(observed.metrics.deadlockFailures??0).toBe(0);expect(observed.metrics.serializationFailures??0).toBe(0);expect(observed.metrics.transactionRetries??0).toBe(0);}finally{await db.exec(`DROP TRIGGER ${fault} ON attachment_access_events;DROP FUNCTION ${fault}()`);}
 const parallel=`learning_bytes_parallel_${key().replaceAll('-','')}`;await db.exec(`CREATE FUNCTION ${parallel}() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN PERFORM pg_sleep(0.08);UPDATE learning_courses SET title=title WHERE id=CASE WHEN NEW.company_id='${f.company}' THEN '${first.id}' ELSE '${rows[1].id}' END;RETURN NEW;END $$;CREATE CONSTRAINT TRIGGER ${parallel} AFTER INSERT ON attachment_access_events DEFERRABLE INITIALLY DEFERRED FOR EACH ROW WHEN(NEW.link_id IN ('${first.evidence.linkId}','${rows[1].evidence.linkId}') AND NEW.outcome='allowed') EXECUTE FUNCTION ${parallel}()`);
 try{const observed=await measureReads(()=>Promise.all(rows.map(row=>attachmentContent(db,env.H001_FILES,row.fixture.owner,row.evidence.linkId,'original'))));for(let index=0;index<rows.length;index++)expect(await observed.result[index].object.text()).toBe(rows[index].bytes);expect(observed.metrics.deadlockFailures??0).toBe(0);expect(observed.metrics.serializationFailures??0).toBe(0);expect(observed.metrics.transactionRetries??0).toBe(0);}finally{await db.exec(`DROP TRIGGER ${parallel} ON attachment_access_events;DROP FUNCTION ${parallel}()`);}
});
it("publishes a reviewed course and completes attendance/results into an eligibility handoff", async () => {
  const id = await assignment();
  expect(
    (await learningWork(f.claimant)).find((x) => x.id === id),
  ).toMatchObject({ queue: "todo", canAct: true });
  await changeLearningAssignment(
    f.claimant,
    id,
    {
      action: "start",
      expectedVersion: 1,
      reason: "Learner opened the assigned course",
    },
    key(),
  );
  const certificate = await seedAttachmentSet(db, f.claimant, {
    targetType: "learning_assignment",
    targetId: id,
    componentKind: "action",
    componentRef: "completion",
    purposeKey: "certificate",
    expectedTargetVersion: 2,
  });
  const completeKey = key();
  await changeLearningAssignment(
    f.claimant,
    id,
    {
      action: "complete",
      expectedVersion: 2,
      attendance: "attended",
      scoreBps: 9200,
      evidence: "Provider result SIM-9200 and observed practical check.",
      reason: "Course and assessment completed",
      attachmentSetIds: [certificate.setId],
    },
    completeKey,
  );
  expect(await attachmentBindingState(db, certificate.setId)).toMatchObject({
    set_state: "bound",
    link_state: "bound",
    binding_type: "learning_completion",
    binding_id: id,
  });
  const d = await learningAssignmentDetail(f.claimant, id);
  expect(d.assignment).toMatchObject({
    status: "completed",
    attendance: "attended",
    score_bps: 9200,
    version: 3,
  });
  expect(d.handoff).toMatchObject({
    eligibility_code: "equipment_operator",
    source_assignment_version: 3,
    status: "open",
  });
  expect(d.history.map((x) => x.action)).toEqual([
    "assign",
    "start",
    "complete",
  ]);
  await changeLearningCourse(f.tie,String(d.assignment.course_id),{action:"retire",expectedVersion:2,reason:"Course revision retired after all original evidence was retained"},key());
  expect(await changeLearningAssignment(f.claimant,id,{action:"complete",expectedVersion:2,attendance:"attended",scoreBps:9200,evidence:"Provider result SIM-9200 and observed practical check.",reason:"Course and assessment completed",attachmentSetIds:[certificate.setId]},completeKey)).toMatchObject({status:"completed",version:3});
  expect((await learningAssignmentDetail(f.claimant,id)).history).toHaveLength(3);
  expect((await learningAssignmentDetail(f.claimant,id)).certificate).toMatchObject({source_assignment_version:3,completion_kind:"completed",eligibility_code:"equipment_operator"});
  await expect(createLearningAssignment(f.owner,{courseId:d.assignment.course_id,courseVersion:2,learnerId:f.claimant.userId,dueDate:"2026-11-01",reason:"Cannot assign a retired course"},key())).rejects.toMatchObject({code:"CONFLICT"});
});
it("requires the learner for results and permits a reviewer waiver with immutable evidence", async () => {
  const id = await assignment();
  await expect(
    changeLearningAssignment(
      f.owner,
      id,
      {
        action: "start",
        expectedVersion: 1,
        reason: "Manager cannot impersonate learner",
      },
      key(),
    ),
  ).rejects.toMatchObject({ code: "LEARNING_LEARNER_REQUIRED" });
  await changeLearningAssignment(
    f.tie,
    id,
    {
      action: "waive",
      expectedVersion: 1,
      reason: "Equivalent external certification verified",
    },
    key(),
  );
  expect((await learningAssignmentDetail(f.owner, id)).assignment.status).toBe(
    "waived",
  );
  await expect(
    db
      .prepare("DELETE FROM learning_assignment_events WHERE assignment_id=?")
      .bind(id)
      .run(),
  ).rejects.toThrow("LEARNING_HISTORY_IMMUTABLE");
});
it("surfaces current learner ineligibility and protects source versions", async () => {
  const id = await assignment();
  await db
    .prepare("UPDATE assignments SET revoked_at=? WHERE id=?")
    .bind(Date.now(), f.claimant.assignmentId)
    .run();
  expect(
    (await learningWork(f.owner)).find((x) => x.id === id)?.blocker,
  ).toContain("no longer eligible");
  await expect(
    changeLearningAssignment(
      f.claimant,
      id,
      {
        action: "start",
        expectedVersion: 1,
        reason: "Revoked learner cannot start",
      },
      key(),
    ),
  ).rejects.toMatchObject({ code: "LEARNING_LEARNER_REQUIRED" });
  await expect(
    db
      .prepare("UPDATE learning_assignments SET course_version=99 WHERE id=?")
      .bind(id)
      .run(),
  ).rejects.toThrow("LEARNING_ASSIGNMENT_SOURCE_IMMUTABLE");
});


it("binds separate Company Learning materials through the owner source lease without deadlock retries or crossed receipts",async()=>{
 const other=await expenseFixture();other.owner.modules=["learning"];
 await db.prepare("UPDATE assignments SET modules_json='[\"learning\"]' WHERE id=?").bind(other.owner.assignmentId).run();
 const first=await createLearningCourse(f.owner,learningCourseDraft,key()),second=await createLearningCourse(other.owner,learningCourseDraft,key());
 const material=await seedAttachmentSet(db,f.owner,{targetType:"learning_course",targetId:String(first.id),componentKind:"record",purposeKey:"course_material",expectedTargetVersion:1});
 const otherMaterial=await seedAttachmentSet(db,other.owner,{targetType:"learning_course",targetId:String(second.id),componentKind:"record",purposeKey:"course_material",expectedTargetVersion:1});
 await learningGrant();await learningGrant("active",null,other.company);vi.stubEnv("ERPX_LEARNING_PACKAGE_ENFORCEMENT","enforced");
 const source=await learningAttachmentPurchaseScope(db,f.owner,"learning_course",String(first.id)),otherSource=await learningAttachmentPurchaseScope(db,other.owner,"learning_course",String(second.id));expect(source).not.toBeNull();expect(otherSource).not.toBeNull();
 const receipt=key(),otherReceipt=key(),name=`learning_parallel_files_${key().replaceAll("-","")}`;
 // Widen the real pre-effect overlap in two complete source-leased Holding
 // transactions. Automatic SERIALIZABLE retry must not conceal a table-lock
 // upgrade deadlock between unrelated Companies.
 await db.prepare(`CREATE FUNCTION ${name}() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN IF NEW.command_id LIKE 'learning-file-before:%' THEN PERFORM pg_sleep(0.1); END IF; RETURN NEW; END $$`).run();
 await db.prepare(`CREATE TRIGGER ${name} INSTEAD OF INSERT ON command_assertions FOR EACH ROW EXECUTE FUNCTION ${name}()`).run();
 try{
  const measured=await measureReads(()=>Promise.all([
   db.transaction(async tx=>{await tx.batch(await learningAttachmentSourceReadLocks(tx,[source!]));return bindRecordAttachmentSet(tx,f.owner,material.setId,1,receipt);}),
   db.transaction(async tx=>{await tx.batch(await learningAttachmentSourceReadLocks(tx,[otherSource!]));return bindRecordAttachmentSet(tx,other.owner,otherMaterial.setId,1,otherReceipt);}),
  ]));
  expect(measured.result).toHaveLength(2);expect(measured.metrics.deadlockFailures??0).toBe(0);
 }finally{await db.prepare(`DROP TRIGGER ${name} ON command_assertions`).run();await db.prepare(`DROP FUNCTION ${name}()`).run();}
 for(const [actor,courseId,evidence,commandId] of [[f.owner,String(first.id),material,receipt],[other.owner,String(second.id),otherMaterial,otherReceipt]] as const){
  expect(await db.prepare("SELECT company_id,version FROM learning_courses WHERE id=?").bind(courseId).first()).toEqual({company_id:actor.companyId,version:2});
  expect(await attachmentBindingState(db,evidence.setId)).toMatchObject({set_state:"bound",link_state:"bound",binding_type:"record_document"});
  expect(await db.prepare("SELECT count(*)::int count FROM command_receipts WHERE command_id=? AND actor_id=? AND authority_json::jsonb->>'companyId'=?").bind(commandId,actor.userId,actor.companyId).first("count")).toBe(1);
  await bindRecordAttachmentSet(db,actor,evidence.setId,1,commandId);
 }
});

it.each(['picker','create'] as const)('keeps the Learn-only %s learner reference inside the current reviewer placement',async surface=>{
 await learningGrant();vi.stubEnv('ERPX_LEARNING_PACKAGE_ENFORCEMENT','enforced');vi.stubEnv('ERPX_PEOPLE_PACKAGE_ENFORCEMENT','enforced');
 const courseId=await course(),reviewer={...f.head,branchId:f.branch,departmentId:f.department,assignmentScope:'department' as const,
  scope:{kind:'department' as const,holdingId:f.head.holdingId,holdingSlug:f.head.holdingSlug,holdingName:f.head.holdingName,companyId:f.company,branchId:f.branch,departmentId:f.department,departmentSlug:'team',departmentName:'Team'}};
 await db.prepare("UPDATE assignments SET scope_kind='department',scope_node_id=?,modules_json='[\"learning\"]',version=version+1 WHERE id=?").bind(f.department,reviewer.assignmentId).run();
 if(surface==='picker'){
  const people=(await learningWorkspace(reviewer)).people;
  expect(people.map(row=>row.id)).toContain(f.claimant.userId);expect(people.map(row=>row.id)).not.toContain(f.reviewer.userId);
  expect(people.every(row=>row.branch_id===f.branch&&row.department_id===f.department)).toBe(true);
  expect(Object.keys(people[0]).sort()).toEqual(['branch_id','department_id','id','name']);
 }else{
  const input={courseId,courseVersion:2,learnerId:f.reviewer.userId,dueDate:'2026-10-01',reason:'Out of placement learner cannot borrow a reviewer assignment'},receipt=key();
  await expect(createLearningAssignment(reviewer,input,receipt)).rejects.toMatchObject({code:'LEARNING_LEARNER_INELIGIBLE'});
  expect(await db.prepare('SELECT 1 FROM command_receipts WHERE command_id=?').bind(receipt).first()).toBeNull();
  expect(await db.prepare('SELECT 1 FROM learning_assignments WHERE id=?').bind(`learning-assignment-${receipt}`).first()).toBeNull();
  expect(await createLearningAssignment(reviewer,{...input,learnerId:f.claimant.userId},key())).toMatchObject({status:'assigned',version:1});
 }
 expect((await readCompanyPackageGrants(db,f.company)).map(row=>row.packageId)).toEqual(['learn']);
});

it.each(['immediate','deferred'] as const)('rolls the complete Learn-only assignment back after an %s native worker reference change',async timing=>{
 await learningGrant();vi.stubEnv('ERPX_LEARNING_PACKAGE_ENFORCEMENT','enforced');vi.stubEnv('ERPX_PEOPLE_PACKAGE_ENFORCEMENT','enforced');
 const courseId=await course(),receipt=key(),id=`learning-assignment-${receipt}`,fn=`learning_reference_${key().replaceAll('-','')}`,
  input={courseId,courseVersion:2,learnerId:f.claimant.userId,dueDate:'2026-10-01',reason:'Assign only against the original captured worker source'},
  before=await db.prepare('SELECT to_jsonb(e) image FROM hr_engagements e WHERE person_id=? AND company_id=?').bind(`person-${f.claimant.userId}`,f.company).first('image');
 await db.exec(`CREATE FUNCTION ${fn}() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN UPDATE hr_engagements SET department_id='${f.accountsDepartment}',version=version+1 WHERE person_id='person-${f.claimant.userId}' AND company_id='${f.company}';RETURN NEW;END $$;
 CREATE ${timing==='deferred'?'CONSTRAINT ':''}TRIGGER ${fn} AFTER INSERT ON command_receipts ${timing==='deferred'?'DEFERRABLE INITIALLY DEFERRED ':''}FOR EACH ROW WHEN(NEW.command_id='${receipt}') EXECUTE FUNCTION ${fn}()`);
 try{await expect(createLearningAssignment(f.owner,input,receipt)).rejects.toMatchObject({code:'LEARNING_SOURCE_CHANGED'});}
 finally{await db.exec(`DROP TRIGGER ${fn} ON command_receipts;DROP FUNCTION ${fn}()`);}
 expect(await db.prepare('SELECT to_jsonb(e) image FROM hr_engagements e WHERE person_id=? AND company_id=?').bind(`person-${f.claimant.userId}`,f.company).first('image')).toEqual(before);
 expect(await db.prepare('SELECT 1 FROM learning_assignments WHERE id=?').bind(id).first()).toBeNull();expect(await db.prepare('SELECT 1 FROM learning_assignment_events WHERE assignment_id=?').bind(id).first()).toBeNull();
 expect(await db.prepare('SELECT 1 FROM command_receipts WHERE command_id=?').bind(receipt).first()).toBeNull();expect(await db.prepare('SELECT 1 FROM audit_events WHERE record_id=?').bind(id).first()).toBeNull();
 const saved=await createLearningAssignment(f.owner,input,receipt);expect(saved).toMatchObject({id,version:1,status:'assigned'});expect(await createLearningAssignment(f.owner,input,receipt)).toEqual(saved);
});

it('rechecks the native Learning module before references, new assignments and exact prior retries',async()=>{
 await learningGrant();vi.stubEnv('ERPX_LEARNING_PACKAGE_ENFORCEMENT','enforced');vi.stubEnv('ERPX_PEOPLE_PACKAGE_ENFORCEMENT','enforced');
 const courseId=await course(),receipt=key(),input={courseId,courseVersion:2,learnerId:f.claimant.userId,dueDate:'2026-10-01',reason:'Actual currently licensed learner reference'},saved=await createLearningAssignment(f.owner,input,receipt);
 expect((await learningWorkspace(f.owner)).people.map(row=>row.id)).toContain(f.claimant.userId);
 await db.prepare("UPDATE assignments SET modules_json='[]',version=version+1 WHERE id=?").bind(f.owner.assignmentId).run();
 await expect(learningWorkspace(f.owner)).rejects.toMatchObject({code:'LEARNING_ACCESS_CHANGED'});await expect(createLearningAssignment(f.owner,input,receipt)).rejects.toMatchObject({code:'LEARNING_ACCESS_CHANGED'});
 await expect(createLearningAssignment(f.owner,input,key())).rejects.toMatchObject({code:'LEARNING_ACCESS_CHANGED'});
 expect(await db.prepare('SELECT status,version FROM learning_assignments WHERE id=?').bind(saved.id).first()).toEqual({status:'assigned',version:1});
 expect(await db.prepare('SELECT count(*)::int n FROM command_receipts WHERE command_id=?').bind(receipt).first('n')).toBe(1);
});

it('takes the learner reference source lease before its first workspace and assignment snapshot without hidden retries',async()=>{
 const {committedSourceVisibility}=await import('../../support/helpers/source-writer-visibility');await learningGrant();vi.stubEnv('ERPX_LEARNING_PACKAGE_ENFORCEMENT','enforced');vi.stubEnv('ERPX_PEOPLE_PACKAGE_ENFORCEMENT','enforced');
 const courseId=await course(),observed=await committedSourceVisibility(db,async tx=>{await tx.prepare("UPDATE hr_people SET legal_name='Committed learner identity',version=version+1 WHERE company_id=? AND user_id=?").bind(f.company,f.claimant.userId).run();},()=>measureReads(()=>learningWorkspace(f.owner)));
 expect(observed.result.people.find(row=>row.id===f.claimant.userId)?.name).toBe('Committed learner identity');
 for(const field of ['deadlockFailures','serializationFailures','transactionRetries'] as const)expect(observed.metrics[field]??0).toBe(0);
 const reviewer={...f.head,branchId:f.branch,departmentId:f.department,assignmentScope:'department' as const,
  scope:{kind:'department' as const,holdingId:f.head.holdingId,holdingSlug:f.head.holdingSlug,holdingName:f.head.holdingName,companyId:f.company,branchId:f.branch,departmentId:f.department,departmentSlug:'team',departmentName:'Team'}};
 await db.prepare("UPDATE assignments SET scope_kind='department',scope_node_id=?,version=version+1 WHERE id=?").bind(f.department,reviewer.assignmentId).run();
 const receipt=key(),created=await committedSourceVisibility(db,async tx=>{await tx.prepare('UPDATE hr_engagements SET department_id=?,version=version+1 WHERE company_id=? AND person_id=?').bind(f.accountsDepartment,f.company,`person-${f.claimant.userId}`).run();},()=>measureReads(async()=>{
  try{return {value:await createLearningAssignment(reviewer,{courseId,courseVersion:2,learnerId:f.claimant.userId,dueDate:'2026-10-01',reason:'A committed placement move invalidates this original reviewer scope'},receipt),error:null};}catch(error){return {value:null,error};}
 }));
 expect(created.result.error).toMatchObject({code:'LEARNING_LEARNER_INELIGIBLE'});expect(created.result.value).toBeNull();for(const field of ['deadlockFailures','serializationFailures','transactionRetries'] as const)expect(created.metrics[field]??0).toBe(0);
 expect(await db.prepare('SELECT 1 FROM command_receipts WHERE command_id=?').bind(receipt).first()).toBeNull();
});

// Actual native identity/Organization fixture reused from current Projects,
// with only Learning purchased; no positive worker/Finance/Stock SQL.
type LearningWorkerActor=import("@erpx/server-identity_access/public/authorization").WorkContext;
async function zeroWorkerLearningCompany(purchased=true){
 // The pre-existing Learning suite migrates only Holding; this new identity
 // fixture also needs the normal Control schema, as the Projects fixture does.
 await applyMigrations(env.CONTROL_DB,env.CONTROL_TEST_MIGRATIONS);
 const company=`learning-workers-${key()}`,branch=`${company}-branch`,department=`${company}-team`;
 await db.batch([
  db.prepare("INSERT INTO organization_nodes(id,kind,slug,display_name,company_id) VALUES(?,'company',?,'Zero-worker Learning Company',?)").bind(company,company,company),
  db.prepare("INSERT INTO organization_nodes(id,kind,slug,display_name,parent_id,company_id,branch_id) VALUES(?,'branch',?,'Real Learning Branch',?,?,?)").bind(branch,branch,company,company,branch),
  db.prepare("INSERT INTO organization_nodes(id,kind,slug,display_name,parent_id,company_id,branch_id) VALUES(?,'department',?,'Real Learning team',?,?,?)").bind(department,department,branch,company,branch),
 ]);
 const {loadIamAuthority}=await import('@erpx/server-identity_access/public/iam-store'),{selectWorkRole}=await import('@erpx/server-work_projection/public/work-role-store');
 async function account(name:string,role='employee'):Promise<LearningWorkerActor>{
  const userId=`${company}-${name}`,assignmentId=`assignment-${userId}`,email=`${userId}@example.test`,modules=['learning',...(role==='holding_owner'?['people_payroll']:[])];
  // Explicit original identity/account fixture, before any worker exists. The
  // feature under test creates neither account, assignment, access nor role.
  await env.CONTROL_DB.prepare('INSERT INTO "user"(id,name,email,"emailVerified","createdAt","updatedAt") VALUES(?,?,?,1,1,1)').bind(userId,name,email).run();
  await db.prepare("INSERT INTO assignments(id,user_id,role,modules_json,company_id,valid_from,valid_until,scope_kind) VALUES(?,?,?,?,?,0,9999999999999,'holding')").bind(assignmentId,userId,role,JSON.stringify(modules),company).run();
  const sessionId=key(),expiresAt=Date.now()+86400000;await selectWorkRole(db,{sessionId,userId,companyId:company,assignmentId,assignmentVersion:1,revision:0,expiresAt});
  const actor:LearningWorkerActor={userId,assignmentId,role,companyId:company,branchId:null,departmentId:null,holdingId:'h001',holdingSlug:'test',holdingName:'Test',name,email,assignmentScope:'holding',inherited:false,modules,assignments:[],workspaces:[],scope:{kind:'holding',holdingId:'h001',holdingSlug:'test',holdingName:'Test',companyId:company},authority:{sessionId,revision:1,assignmentVersion:1}};
  actor.iam=await loadIamAuthority(db,company,userId,role,sessionId);return actor;
 }
 const owner=await account('company-hr-owner','holding_owner'),contributor=await account('learning-participant');
 const {installStaffLevels,savePosition,positionSchema}=await import('@erpx/server-organization/public/organization');await installStaffLevels(db,owner,key());
 const ceo=String((await savePosition(db,owner,null,positionSchema.parse({title:'Actual first Company leader',levelId:`org-level-${company}-1`,branchId:null,departmentId:null,isCeo:true,reportsToPositionId:null,expectedVersion:0,reason:'Normal Organization prepares the first worker position'}),key())).id);
 const worker=String((await savePosition(db,owner,null,positionSchema.parse({title:'Actual Learning participant',levelId:`org-level-${company}-6`,branchId:branch,departmentId:department,isCeo:false,reportsToPositionId:ceo,expectedVersion:0,reason:'Normal Organization prepares the explicit Learning worker position'}),key())).id);
 const store=packageGrantStore(db,{authorize:async(_tx,id)=>{if(id!==company)throw Error('PROJECT_WORKER_FIXTURE_COMPANY');return {userId:'isolated-learning-worker-provider'};},obligations:async()=>{throw Error('PROJECT_WORKER_FIXTURE_CANNOT_INACTIVATE');}});
 async function purchase(state:'active'|'draining'='active'){const prior=(await readCompanyPackageGrants(db,company)).find(row=>row.packageId==='learn');return store.apply({expectedVersion:prior?.version??0,idempotencyKey:key(),grant:{companyId:company,packageId:'learn',packageVersion:1,state,startsAt:0,endsAt:null,version:(prior?.version??0)+1,reason:'Actual Learning-only worker-reference foundation',evidence:'test:learning-zero-worker-native'}});}
 if(purchased)await purchase();
 vi.stubEnv('ERPX_LEARNING_PACKAGE_ENFORCEMENT','enforced');vi.stubEnv('ERPX_PEOPLE_PACKAGE_ENFORCEMENT','enforced');vi.stubEnv('ERPX_APPS_PACKAGE_ENFORCEMENT','enforced');
 const api=await import('@erpx/server-people_payroll/public/learning-worker-startup'),jobs=await import('@erpx/server-people_payroll/public/people-lifecycle-jobs');
 const counts=()=>Promise.all(['hr_people','hr_engagements','employees','people_lifecycle_cases','people_learning_worker_starters','people_learning_worker_placements','staff_projects','finance_charts','finance_accounts','finance_journals','finance_payments','hr_pay_groups','hr_payroll_runs'].map(table=>db.prepare(`SELECT count(*) n FROM ${table} WHERE company_id=?`).bind(company).first<number>('n')));
 expect(await counts()).toEqual(Array(13).fill(0));
 const accountImage=async()=>({assignments:(await db.prepare('SELECT to_jsonb(a)::text source FROM assignments a WHERE a.company_id=? ORDER BY a.id').bind(company).all()).results,members:(await db.prepare('SELECT to_jsonb(m)::text source FROM iam_members m WHERE m.company_id=? ORDER BY m.user_id').bind(company).all()).results,accounts:(await env.CONTROL_DB.prepare('SELECT to_jsonb(u)::text source FROM "user" u WHERE id LIKE ? ORDER BY id').bind(`${company}-%`).all()).results});
 const nonWorkerImage=async()=>({accounts:await accountImage(),compensation:(await db.prepare('SELECT to_jsonb(c)::text source FROM hr_compensation_revisions c JOIN hr_engagements e ON e.id=c.engagement_id WHERE e.company_id=? ORDER BY c.id').bind(company).all()).results,leave:(await db.prepare('SELECT to_jsonb(l)::text source FROM leave_policy_assignments l WHERE l.company_id=? ORDER BY l.id').bind(company).all()).results,money:await Promise.all(['finance_charts','finance_accounts','finance_periods','company_fx_rates','finance_journals','finance_payments','hr_pay_groups','hr_payroll_runs'].map(table=>db.prepare(`SELECT count(*) n FROM ${table} WHERE company_id=?`).bind(company).first<number>('n')))});
 async function prepare(who:LearningWorkerActor,positionId:string,managerPersonId:string|null,profileName=who.name){const view=await api.learningWorkerStartupWorkspace(owner,{accountId:who.userId});expect(view.canPrepare).toBe(true);if(!view.contextKey)throw Error('Native worker context required');const command=key(),input=api.learningWorkerPrepareSchema.parse({mode:'create',accountId:who.userId,expectedVersion:0,contextKey:view.contextKey,input:{legalName:profileName,email:who.email,countryCode:'TH',branchId:branch,departmentId:department,jobTitle:positionId===ceo?'Company leader':'Learning participant',workerType:'employee',employmentType:'salaried',startDate:new Date().toISOString().slice(0,10),currency:'THB',managerPersonId,levelId:`org-level-${company}-${positionId===ceo?1:6}`,positionId,acceptRecommendedPay:false,acceptRecommendedLeave:false,reason:'Explicitly prepare this selected existing Company account'}});const result=await api.prepareLearningWorkerStartup(owner,input,command);expect(result).toEqual({id:`person-${command}`,version:1});return {result,input,command};}
 async function submit(personId:string,positionId:string,managerPersonId:string|null,effectiveAt=Date.now()-1000){const view=await api.learningWorkerStartupWorkspace(owner,{personId});expect(view.canSubmitPlacement).toBe(true);if(!view.contextKey)throw Error('Native placement context required');const input=api.learningWorkerPlacementSchema.parse({positionId,managerPersonId,effectiveAt,timezone:'Asia/Bangkok',expectedVersion:0,contextKey:view.contextKey,reason:'Reviewed original HR-only Learning worker placement'}),preview=await api.previewLearningWorkerPlacement(owner,personId,input);expect(preview.ready).toBe(true);expect(preview.blockers).toEqual([]);const body={...input,token:preview.token},command=key(),result=await api.submitLearningWorkerPlacement(owner,personId,body,command);expect(result).toEqual({id:`lifecycle-${command}`,version:1,status:'scheduled'});return {result,body,command};}
 async function settle(){return withOwnerPackageObligations(db,{companyId:company,packageId:'learn',packageVersion:1},tx=>readOwnerPackageObligations(db,tx,{companyId:company,packageId:'learn',packageVersion:1,now:Date.now()}));}
 async function pendingPeople(){return (await settle()).observations.find(row=>row.moduleId==='people_payroll')?.outstanding??0;}
 return {company,branch,department,owner,contributor,ceo,worker,account,api,jobs,counts,accountImage,nonWorkerImage,prepare,submit,purchase,pendingPeople};
}

it('prepares a zero-worker Learning-only Company through original reviewed HR placement before real course assignment and learner start',async()=>{
 const s=await zeroWorkerLearningCompany(),before=await s.nonWorkerImage(),{activeStaff}=await import('@erpx/server-operations/public/staff-policy'),{saveDirectoryPerson,directoryUpdateSchema}=await import('@erpx/server-people_payroll/public/people-directory');
 expect((await readCompanyPackageGrants(db,s.company)).map(row=>row.packageId)).toEqual(['learn']);await expect(activeStaff(db,s.company,s.contributor.userId)).rejects.toMatchObject({code:'STAFF_LINK_REQUIRED'});
 await expect(saveDirectoryPerson(s.owner,null,directoryUpdateSchema.parse({legalName:'Generic People remains separate',email:s.owner.email,countryCode:'TH',branchId:s.branch,departmentId:s.department,jobTitle:'Company leader',workerType:'employee',employmentType:'salaried',startDate:new Date().toISOString().slice(0,10),currency:'THB',managerPersonId:null,levelId:`org-level-${s.company}-1`,positionId:s.ceo,expectedVersion:1}),key())).rejects.toMatchObject({code:'PACKAGE_REQUIRED'});
 expect(await s.counts()).toEqual(Array(13).fill(0));expect((await learningWorkspace(s.owner)).people).toEqual([]);
 const reviewer=await s.account('independent-course-reviewer','holding_owner'),authorityBefore=await s.accountImage(),course=await createLearningCourse(s.owner,{title:'Actual Learning-only first course',description:'Original course publication and first real active learner assignment',provider:'Native Company academy',durationMinutes:45,requiredFor:null},key());
 await expect(changeLearningCourse(s.owner,String(course.id),{action:'publish',expectedVersion:1,reason:'Original independent course review still required'},key())).rejects.toMatchObject({code:'CONFLICT'});await changeLearningCourse(reviewer,String(course.id),{action:'publish',expectedVersion:1,reason:'Different native Learning reviewer validates actual course'},key());
 const notReadyKey=key();await expect(createLearningAssignment(s.owner,{courseId:String(course.id),courseVersion:2,learnerId:s.contributor.userId,dueDate:new Date(Date.now()+86400000).toISOString().slice(0,10),reason:'Original assignment still requires the real active worker'},notReadyKey)).rejects.toMatchObject({code:'LEARNING_LEARNER_INELIGIBLE'});for(const table of ['command_receipts','audit_events','outbox_events'])expect(await db.prepare('SELECT 1 FROM '+table+' WHERE command_id=?').bind(notReadyKey).first()).toBeNull();
 const owner=await s.prepare(s.owner,s.ceo,null),ownerPlacement=await s.submit(owner.result.id,s.ceo,null);expect(await s.pendingPeople()).toBe(1);expect(await s.jobs.drainPeopleLifecycle(db)).toBe(1);expect(await s.pendingPeople()).toBe(0);
 const learner=await s.prepare(s.contributor,s.worker,owner.result.id),placement=await s.submit(learner.result.id,s.worker,owner.result.id);expect(await s.pendingPeople()).toBe(1);expect(await s.jobs.drainPeopleLifecycle(db)).toBe(1);expect(await s.pendingPeople()).toBe(0);expect((await activeStaff(db,s.company,s.contributor.userId)).person_id).toBe(learner.result.id);
 const workspace=await learningWorkspace(s.owner);expect(workspace.people).toEqual(expect.arrayContaining([{id:s.contributor.userId,name:s.contributor.name,branch_id:s.branch,department_id:s.department}]));
 const command=key(),body={courseId:String(course.id),courseVersion:2,learnerId:s.contributor.userId,dueDate:new Date(Date.now()+86400000).toISOString().slice(0,10),reason:'Assign only after the original reviewed worker became active'},assigned=await createLearningAssignment(s.owner,body,command);expect(assigned).toEqual({id:`learning-assignment-${command}`,version:1,status:'assigned'});expect(await createLearningAssignment(s.owner,body,command)).toEqual(assigned);
 const started=await changeLearningAssignment(s.contributor,String(assigned.id),{action:'start',expectedVersion:1,reason:'The actual assigned Company learner starts the published course'},key());expect(started).toMatchObject({id:assigned.id,status:'in_progress',version:2});expect((await learningAssignmentDetail(s.contributor,String(assigned.id))).assignment).toMatchObject({learner_id:s.contributor.userId,status:'in_progress',learner_eligible:1});
 expect(await s.api.prepareLearningWorkerStartup(s.owner,owner.input,owner.command)).toEqual(owner.result);expect(await s.api.submitLearningWorkerPlacement(s.owner,owner.result.id,ownerPlacement.body,ownerPlacement.command)).toEqual(ownerPlacement.result);expect(await s.api.prepareLearningWorkerStartup(s.owner,learner.input,learner.command)).toEqual(learner.result);expect(await s.api.submitLearningWorkerPlacement(s.owner,learner.result.id,placement.body,placement.command)).toEqual(placement.result);
 if(!s.owner.authority)throw Error('Native selected Company HR authority required');const scope={companyId:s.company,branchId:null,departmentId:null,userId:s.owner.userId,assignmentId:s.owner.assignmentId,assignmentVersion:s.owner.authority.assignmentVersion,role:s.owner.role,sessionId:s.owner.authority.sessionId};expect(await s.api.learningWorkerStartupProductEntry(db,scope,{personId:learner.result.id})).toMatchObject({revision:expect.stringMatching(/^[a-f0-9]{64}$/),purchasePurpose:'history'});expect(await s.api.learningWorkerStartupProductEntry(db,{...scope,assignmentVersion:scope.assignmentVersion+1})).toBeNull();expect(await s.api.learningWorkerStartupProductEntry(db,{...scope,beta:true})).toBeNull();
 const after=await s.nonWorkerImage();expect(after.accounts).toEqual(authorityBefore);expect(after.compensation).toEqual(before.compensation);expect(after.leave).toEqual(before.leave);expect(after.money).toEqual(before.money);expect(await db.prepare('SELECT count(*)::int n FROM staff_projects WHERE company_id=?').bind(s.company).first('n')).toBe(0);expect(await db.prepare('SELECT count(*)::int n FROM people_project_worker_starters WHERE company_id=?').bind(s.company).first('n')).toBe(0);expect((await readCompanyPackageGrants(db,s.company)).map(row=>row.packageId)).toEqual(['learn']);
},120000);

it('keeps Learning worker preparation source-bound through deferred rollback, draining history and current-member cancellation without Projects or People purchases',async()=>{
 const s=await zeroWorkerLearningCompany(false),image=async()=>Object.fromEntries(await Promise.all(['assignments','hr_people','hr_engagements','employees','people_lifecycle_cases','people_preparation_tasks','people_learning_worker_starters','people_learning_worker_placements','organization_positions','staff_organization_history','staff_level_history','command_receipts','command_assertions','audit_events','outbox_events','iam_audit'].map(async table=>[table,await db.prepare(`SELECT COALESCE(jsonb_agg(to_jsonb(actual) ORDER BY to_jsonb(actual)::text),'[]'::jsonb)::text image FROM ${table} actual`).first<string>('image')]))),initial=await image();
 vi.stubEnv('ERPX_PEOPLE_PACKAGE_ENFORCEMENT','disabled');vi.stubEnv('ERPX_LEARNING_PACKAGE_ENFORCEMENT','disabled');await expect(s.api.prepareLearningWorkerStartup(s.owner,{mode:'create',accountId:s.contributor.userId,expectedVersion:0,contextKey:'a'.repeat(64),input:{legalName:s.contributor.name,email:s.contributor.email,countryCode:'TH',branchId:s.branch,departmentId:s.department,jobTitle:'Learning participant',workerType:'employee',employmentType:'salaried',startDate:new Date().toISOString().slice(0,10),currency:'THB',managerPersonId:null,levelId:`org-level-${s.company}-6`,positionId:s.worker,acceptRecommendedPay:false,acceptRecommendedLeave:false,reason:'Unpurchased strict named preparation remains denied'}},key())).rejects.toMatchObject({status:403,code:'PACKAGE_REQUIRED'});expect(await image()).toEqual(initial);vi.stubEnv('ERPX_PEOPLE_PACKAGE_ENFORCEMENT','enforced');vi.stubEnv('ERPX_LEARNING_PACKAGE_ENFORCEMENT','enforced');await s.purchase();
 const owner=await s.prepare(s.owner,s.ceo,null);await s.submit(owner.result.id,s.ceo,null);expect(await s.jobs.drainPeopleLifecycle(db)).toBe(1);
 const seed=await s.api.learningWorkerStartupWorkspace(s.owner,{accountId:s.contributor.userId});if(!seed.contextKey)throw Error('Actual Learning worker context required');const body=s.api.learningWorkerPrepareSchema.parse({mode:'create',accountId:s.contributor.userId,expectedVersion:0,contextKey:seed.contextKey,input:{legalName:s.contributor.name,email:s.contributor.email,countryCode:'TH',branchId:s.branch,departmentId:s.department,jobTitle:'Learning participant',workerType:'employee',employmentType:'salaried',startDate:new Date().toISOString().slice(0,10),currency:'THB',managerPersonId:owner.result.id,levelId:`org-level-${s.company}-6`,positionId:s.worker,acceptRecommendedPay:false,acceptRecommendedLeave:false,reason:'Actual named source must remain unchanged through original deferred effects'}}),command=key(),before=await image(),fault=`learning_worker_source_${key().replaceAll('-','')}`;
 await db.exec(`CREATE FUNCTION ${fault}() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN IF NEW.company_id='${s.company}' THEN UPDATE organization_positions SET title=title||' late native source drift' WHERE id='${s.worker}';END IF;RETURN NULL;END $$;CREATE CONSTRAINT TRIGGER ${fault} AFTER INSERT ON people_learning_worker_starters DEFERRABLE INITIALLY DEFERRED FOR EACH ROW EXECUTE FUNCTION ${fault}()`);
 try{await expect(s.api.prepareLearningWorkerStartup(s.owner,body,command)).rejects.toMatchObject({status:409,code:'LEARNING_WORKER_SOURCE_CHANGED'});expect(await image()).toEqual(before);}finally{await db.exec(`DROP TRIGGER ${fault} ON people_learning_worker_starters;DROP FUNCTION ${fault}()`);}
 const prepared=await s.api.prepareLearningWorkerStartup(s.owner,body,command);expect(prepared).toEqual({id:`person-${command}`,version:1});expect(await s.api.prepareLearningWorkerStartup(s.owner,body,command)).toEqual(prepared);const future=await s.submit(prepared.id,s.worker,owner.result.id,Date.now()+86400000);expect(await s.pendingPeople()).toBe(1);
 // A distinct original reviewed case is already due before Commercial drains.
 // It settles normally while the other reviewed case remains future/cancellable.
 const dueAccount=await s.account('reviewed-due-participant'),due=await s.prepare(dueAccount,s.worker,owner.result.id),dueCase=await s.submit(due.result.id,s.worker,owner.result.id);expect(await s.pendingPeople()).toBe(2);
 await s.purchase('draining');expect(await s.api.prepareLearningWorkerStartup(s.owner,due.input,due.command)).toEqual(due.result);expect(await s.api.submitLearningWorkerPlacement(s.owner,due.result.id,dueCase.body,dueCase.command)).toEqual(dueCase.result);expect(await s.jobs.drainPeopleLifecycle(db)).toBe(1);expect(await s.pendingPeople()).toBe(1);expect(await db.prepare('SELECT status,version FROM people_lifecycle_cases WHERE id=?').bind(dueCase.result.id).first()).toEqual({status:'completed',version:2});const {activeStaff}=await import('@erpx/server-operations/public/staff-policy');expect((await activeStaff(db,s.company,dueAccount.userId)).person_id).toBe(due.result.id);
 expect(await s.api.prepareLearningWorkerStartup(s.owner,body,command)).toEqual(prepared);expect(await s.api.submitLearningWorkerPlacement(s.owner,prepared.id,future.body,future.command)).toEqual(future.result);const beforeDenied=await image();await expect(s.api.prepareLearningWorkerStartup(s.owner,body,key())).rejects.toMatchObject({status:403});expect(await image()).toEqual(beforeDenied);
 // Adverse native membership revocation is a diagnostic fact. It does not
 // authorize a due job, but cannot strand retained future HR cancellation.
 await db.prepare('UPDATE assignments SET revoked_at=?,version=version+1 WHERE id=?').bind(Date.now(),s.contributor.assignmentId).run();expect((await s.api.learningWorkerStartupWorkspace(s.owner,{personId:prepared.id})).canCancelPlacement).toBe(true);const cancelKey=key(),cancel=await s.api.cancelLearningWorkerPlacement(s.owner,prepared.id,future.result.id,{expectedVersion:1},cancelKey);expect(cancel).toEqual({id:future.result.id,status:'cancelled'});expect(Object.hasOwn(cancel,'version')).toBe(false);expect(await s.api.cancelLearningWorkerPlacement(s.owner,prepared.id,future.result.id,{expectedVersion:1},cancelKey)).toEqual(cancel);expect(await s.pendingPeople()).toBe(0);
 const retained=await image();await db.prepare('UPDATE assignments SET revoked_at=?,version=version+1 WHERE id=?').bind(Date.now(),s.owner.assignmentId).run();await expect(s.api.prepareLearningWorkerStartup(s.owner,body,command)).rejects.toMatchObject({code:'QUEUED_AUTHORITY_CHANGED'});const {assignments:oldAssignments,...oldBusiness}=retained,{assignments:newAssignments,...newBusiness}=await image();expect(newBusiness).toEqual(oldBusiness);expect(newAssignments).not.toBe(oldAssignments);expect((await readCompanyPackageGrants(db,s.company)).map(row=>[row.packageId,row.state])).toEqual([['learn','draining']]);
},120000);
