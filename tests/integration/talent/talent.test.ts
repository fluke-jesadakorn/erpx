import type {FileStore,SqlDatabase} from "@erpx/server-runtime/public/runtime/types";
import {candidatePrivacyWorkspace} from "@erpx/server-talent/public/candidate-privacy-workspace";
import {talentCandidatePrivacyProductSourceEntry} from "@erpx/server-talent/public/product-entry";
import {measureReads} from "@erpx/server-runtime/public/runtime/read-metrics";
import {hireHandoffWorkspace} from "@erpx/server-people_payroll/public/hire-handoff-workspace";
import {talentHireProductSourceEntry} from "@erpx/server-talent/public/product-entry";
import {packageGrantStore,readCompanyPackageGrants} from "@erpx/server-commercial/public/package-grants";
import { beforeAll, beforeEach, afterEach, expect, it, vi } from "vitest";
import { applyMigrations, env } from "../../support/runtime-env";
import {
  expenseDb as db,
  expenseFixture,
  key,
} from "../../support/helpers/expense-fixture";
import {
  changeTalentRecord,
  createTalentApplication,
  createTalentCandidate,
  createTalentOffer,
  createTalentProposal,
  createTalentRequisition,
  talentDetail,
  talentWork,
  talentWorkspace,
} from "@erpx/server-talent/public/talent";
import type { WorkContext } from "@erpx/server-identity_access/public/authorization";
import { candidatePrivacySchema,changeCandidatePrivacy,consumeHireHandoff,hireConsumptionSchema,hireHandoffQueue } from "@erpx/server-people_payroll/public/people-handoffs";
import {createPerson,createPersonSchema,createPayGroup,payGroupSchema} from "@erpx/server-people_payroll/public/people-payroll";
vi.mock("@erpx/server-runtime/adapters/runtime", () => ({
  holdingResources: async () => ({ write: talentDatabase??env.H001_DB, files: env.H001_FILES }),
  runtimeEnv: async () => env,
}));
let talentDatabase:SqlDatabase|undefined;
afterEach(()=>{talentDatabase=undefined;vi.unstubAllEnvs();});
function recruitmentPurchase(){const store=packageGrantStore(db,{authorize:async()=>({userId:"isolated-recruitment-provider"}),obligations:async()=>{throw Error("No Recruitment inactive witness is invented");}}),grant={companyId:f.company,packageId:"recruit",packageVersion:1,state:"active" as const,startsAt:0,endsAt:null,version:1,reason:"Actual Recruitment-only workflow and accepted People handoff",evidence:"test:recruitment-owner"};return {store,grant};}
let f: Awaited<ReturnType<typeof expenseFixture>>,
  creator: WorkContext,
  reviewer: WorkContext;
beforeAll(async () => {
  await Promise.all([applyMigrations(db, env.STAFF_TEST_MIGRATIONS),applyMigrations(env.CONTROL_DB,env.CONTROL_TEST_MIGRATIONS)]);
});
beforeEach(async () => {
  f = await expenseFixture();
  creator = {
    ...f.owner,
    branchId: f.branch,
    departmentId: f.department,
    modules: ["talent"],
  };
  reviewer = {
    ...f.tie,
    branchId: f.branch,
    departmentId: f.department,
    modules: ["talent"],
  };
});
async function application() {
  const req = await createTalentRequisition(
    creator,
    {
      jobTitle: "Service specialist",
      openings: 1,
      targetStartDate: "2026-11-01",
      description: "Serve customer cases and complete field handoffs.",
    },
    key(),
  );
  await expect(
    changeTalentRecord(
      creator,
      "requisition",
      String(req.id),
      {
        action: "approve",
        expectedVersion: 1,
        reason: "Self approval forbidden",
      },
      key(),
    ),
  ).rejects.toMatchObject({ code: "CONFLICT" });
  await changeTalentRecord(
    reviewer,
    "requisition",
    String(req.id),
    {
      action: "approve",
      expectedVersion: 1,
      reason: "Headcount and scope reviewed",
    },
    key(),
  );
  await changeTalentRecord(
    reviewer,
    "requisition",
    String(req.id),
    {
      action: "open",
      expectedVersion: 2,
      reason: "Approved role is ready for applications",
    },
    key(),
  );
  const candidate = await createTalentCandidate(
    creator,
    {
      legalName: "Candidate One",
      email: `candidate-${key()}@example.test`,
      phone: "0800000000",
      consentEvidence:
        "Candidate consented to recruitment processing in the simulated intake.",
      retentionUntil: "2027-09-30",
    },
    key(),
  );
  const app = await createTalentApplication(
    creator,
    {
      requisitionId: req.id,
      requisitionVersion: 3,
      candidateId: candidate.id,
      ownerId: creator.userId,
    },
    key(),
  );
  return { req, candidate, app };
}
async function actualProductHire(){
 const {app,candidate}=await application(),appId=String(app.id);await changeTalentRecord(creator,'application',appId,{action:'interview',expectedVersion:1,reason:'Original interview before exact product hire handoff'},key());const proposal=await createTalentProposal(creator,{applicationId:appId,applicationVersion:2,recommendation:'hire',scoreBps:9100,evidence:'Independent panel retained original exact candidate and role evidence'},key());await changeTalentRecord(reviewer,'proposal',String(proposal.id),{action:'accept-proposal',expectedVersion:1,reason:'Independent original panel proposal review'},key());await changeTalentRecord(creator,'application',appId,{action:'select',expectedVersion:2,reason:'Original independently accepted panel proposal'},key());const offer=await createTalentOffer(creator,{applicationId:appId,applicationVersion:3,currency:'THB',baseAmount:'36000.00',startDate:'2026-11-01',expiresOn:'2026-11-15',terms:'Reviewed original monthly compensation and identity conditions'},key());await changeTalentRecord(reviewer,'offer',String(offer.id),{action:'approve-offer',expectedVersion:1,reason:'Independent reviewed actual offer and original identity'},key());await changeTalentRecord(creator,'offer',String(offer.id),{action:'send',expectedVersion:2,reason:'Approved original offer before accepted source handoff'},key());await changeTalentRecord(creator,'offer',String(offer.id),{action:'accept-offer',expectedVersion:3,reason:'Actual original candidate acceptance for product handoff',acceptanceEvidence:'Signed isolated candidate acceptance SOURCE-PRODUCT-ENTRY'},key());const detail=await talentDetail(creator,appId);return {candidateId:String(candidate.id),handoffId:String(detail.handoff!.id)};
}
it('productSourceRollout enters only the actual accepted hire independently of Talent enforcement mode',async()=>{
 const {handoffId}=await actualProductHire(),actor={...f.owner,modules:['people']},scope={companyId:f.company,branchId:null,departmentId:null,userId:actor.userId,assignmentId:actor.assignmentId,assignmentVersion:await db.prepare('SELECT version FROM assignments WHERE id=?').bind(actor.assignmentId).first<number>('version')??0,role:actor.role};expect(await talentHireProductSourceEntry(db,scope,handoffId)).toBeNull();expect((await hireHandoffWorkspace(actor,handoffId)).purchase).toBeUndefined();await expect(hireHandoffWorkspace(actor,handoffId,{strictProduct:true})).rejects.toMatchObject({code:'PACKAGE_REQUIRED'});const {store,grant}=recruitmentPurchase();await store.apply({expectedVersion:0,idempotencyKey:key(),grant});for(const mode of ['disabled','shadow','enforced']){vi.stubEnv('ERPX_TALENT_PACKAGE_ENFORCEMENT',mode);expect(await talentHireProductSourceEntry(db,scope,handoffId)).toEqual({capability:'recruit.offers',purchasePurpose:'history'});const workspace=await hireHandoffWorkspace(actor,handoffId,{strictProduct:true});expect(workspace).toMatchObject({handoff:{id:handoffId,legalName:'Candidate One'},canConsume:true,purchase:{newWork:true,settleExisting:true,history:true}});expect(JSON.stringify(workspace)).not.toMatch(/consent_evidence|accepted_evidence|source_digest|assignmentVersion|grants/);}
 for(const changed of [{assignmentVersion:scope.assignmentVersion+1},{companyId:'foreign-company'},{branchId:f.branch}])expect(await talentHireProductSourceEntry(db,{...scope,...changed},handoffId)).toBeNull();expect(await talentHireProductSourceEntry(db,scope,'unrelated-hire')).toBeNull();await store.apply({expectedVersion:1,idempotencyKey:key(),grant:{...grant,state:'draining',version:2}});expect((await hireHandoffWorkspace(actor,handoffId,{strictProduct:true})).purchase).toMatchObject({newWork:false,settleExisting:true,history:true});expect((await readCompanyPackageGrants(db,f.company)).map(row=>row.packageId)).toEqual(['recruit']);vi.stubEnv('NODE_ENV','production');expect(await talentHireProductSourceEntry(db,scope,handoffId)).toBeNull();vi.stubEnv('NODE_ENV','test');await db.prepare('UPDATE assignments SET revoked_at=1,version=version+1 WHERE id=?').bind(actor.assignmentId).run();expect(await talentHireProductSourceEntry(db,scope,handoffId)).toBeNull();await expect(hireHandoffWorkspace(actor,handoffId,{strictProduct:true})).rejects.toMatchObject({code:'TALENT_ACCESS_CHANGED'});
});
it('productSourceRollout enters only original candidate privacy independently of Talent enforcement mode',async()=>{
 const {candidate}=await application(),id=String(candidate.id),actor={...f.tie,role:'legal_manager',assignmentId:await f.grant(f.tie,'legal_manager'),modules:['legal']},scope={companyId:f.company,branchId:null,departmentId:null,userId:actor.userId,assignmentId:actor.assignmentId,assignmentVersion:await db.prepare('SELECT version FROM assignments WHERE id=?').bind(actor.assignmentId).first<number>('version')??0,role:actor.role};expect(await talentCandidatePrivacyProductSourceEntry(db,scope,id)).toBeNull();expect((await candidatePrivacyWorkspace(actor,id)).purchase).toBeUndefined();await expect(candidatePrivacyWorkspace(actor,id,{strictProduct:true})).rejects.toMatchObject({code:'PACKAGE_REQUIRED'});const {store,grant}=recruitmentPurchase();await store.apply({expectedVersion:0,idempotencyKey:key(),grant});for(const mode of ['disabled','shadow','enforced']){vi.stubEnv('ERPX_TALENT_PACKAGE_ENFORCEMENT',mode);expect(await talentCandidatePrivacyProductSourceEntry(db,scope,id)).toEqual({capability:'recruit.candidates',purchasePurpose:'history'});const workspace=await candidatePrivacyWorkspace(actor,id,{strictProduct:true});expect(workspace).toMatchObject({candidate:{id,status:'active'},actions:{hold:true,withdraw:false},purchase:{newWork:true,settleExisting:true,history:true}});expect(JSON.stringify(workspace)).not.toMatch(/Candidate One|@example|0800000000|consent|created_by|privacy_requested_by|grants|source_digest/);}
 for(const changed of [{assignmentVersion:scope.assignmentVersion+1},{companyId:'foreign-company'},{departmentId:f.department}])expect(await talentCandidatePrivacyProductSourceEntry(db,{...scope,...changed},id)).toBeNull();await store.apply({expectedVersion:1,idempotencyKey:key(),grant:{...grant,state:'draining',version:2}});expect((await candidatePrivacyWorkspace(actor,id,{strictProduct:true})).purchase).toMatchObject({newWork:false,settleExisting:true,history:true});vi.stubEnv('NODE_ENV','production');expect(await talentCandidatePrivacyProductSourceEntry(db,scope,id)).toBeNull();vi.stubEnv('NODE_ENV','test');await db.prepare('UPDATE assignments SET revoked_at=1,version=version+1 WHERE id=?').bind(actor.assignmentId).run();expect(await talentCandidatePrivacyProductSourceEntry(db,scope,id)).toBeNull();await expect(candidatePrivacyWorkspace(actor,id,{strictProduct:true})).rejects.toMatchObject({code:'TALENT_ACCESS_CHANGED'});
});
it("moves a consented candidate through proposal, reviewed offer and immutable hire handoff", async () => {
  const {store,grant}=recruitmentPurchase();await store.apply({expectedVersion:0,idempotencyKey:key(),grant});vi.stubEnv("ERPX_TALENT_PACKAGE_ENFORCEMENT","enforced");vi.stubEnv("ERPX_PEOPLE_PACKAGE_ENFORCEMENT","enforced");
  const { app,candidate } = await application();
  await changeTalentRecord(
    creator,
    "application",
    String(app.id),
    {
      action: "interview",
      expectedVersion: 1,
      reason: "Interview scheduled with candidate",
    },
    key(),
  );
  const proposal = await createTalentProposal(
    creator,
    {
      applicationId: app.id,
      applicationVersion: 2,
      recommendation: "hire",
      scoreBps: 8800,
      evidence: "Panel evidence and role rubric recorded.",
    },
    key(),
  );
  await expect(
    changeTalentRecord(
      creator,
      "proposal",
      String(proposal.id),
      {
        action: "accept-proposal",
        expectedVersion: 1,
        reason: "Self review forbidden",
      },
      key(),
    ),
  ).rejects.toMatchObject({ code: "CONFLICT" });
  await changeTalentRecord(
    reviewer,
    "proposal",
    String(proposal.id),
    {
      action: "accept-proposal",
      expectedVersion: 1,
      reason: "Panel evidence reviewed independently",
    },
    key(),
  );
  await changeTalentRecord(
    creator,
    "application",
    String(app.id),
    {
      action: "select",
      expectedVersion: 2,
      reason: "Accepted panel proposal selected",
    },
    key(),
  );
  const offer = await createTalentOffer(
    creator,
    {
      applicationId: app.id,
      applicationVersion: 3,
      currency: "THB",
      baseAmount: "45000.00",
      startDate: "2026-11-01",
      expiresOn: "2026-11-15",
      terms: "Monthly base salary subject to onboarding checks.",
    },
    key(),
  );
  await changeTalentRecord(
    reviewer,
    "offer",
    String(offer.id),
    {
      action: "approve-offer",
      expectedVersion: 1,
      reason: "Compensation and authority reviewed",
    },
    key(),
  );
  await changeTalentRecord(
    creator,
    "offer",
    String(offer.id),
    {
      action: "send",
      expectedVersion: 2,
      reason: "Approved offer delivered to candidate",
    },
    key(),
  );
  await store.apply({expectedVersion:1,idempotencyKey:key(),grant:{...grant,state:"draining",version:2}});
  await changeTalentRecord(
    creator,
    "offer",
    String(offer.id),
    {
      action: "accept-offer",
      expectedVersion: 3,
      reason: "Candidate acceptance recorded",
      acceptanceEvidence: "Signed simulated offer reference SIM-OFFER-1",
    },
    key(),
  );
  const d = await talentDetail(creator, String(app.id));
  expect(d.application.status).toBe("accepted");
  expect(d.offer).toMatchObject({
    status: "accepted",
    base_amount_minor: 4500000,
    version: 4,
  });
  expect(d.handoff).toMatchObject({
    source_offer_version: 4,
    status: "open",
    branch_id: f.branch,
    department_id: f.department,
  });
  await expect(
    db
      .prepare("DELETE FROM talent_hire_handoffs WHERE offer_id=?")
      .bind(offer.id)
      .run(),
  ).rejects.toThrow("TALENT_HISTORY_IMMUTABLE");
  const group=`hire-group-${key()}`;
  await db.prepare("INSERT INTO hr_pay_groups(id,company_id,name,country_code,currency,frequency,timezone,created_by,created_at,updated_at) VALUES(?,?,'Hire monthly group','TH','THB','monthly','Asia/Bangkok',?,1,1)").bind(group,f.company,f.owner.userId).run();
  const hrActor={...f.owner,modules:["people"]},entryScope={companyId:hrActor.companyId,branchId:null,departmentId:null,userId:hrActor.userId,assignmentId:hrActor.assignmentId,assignmentVersion:await db.prepare("SELECT version FROM assignments WHERE id=?").bind(hrActor.assignmentId).first<number>("version")??0,role:hrActor.role},handoffId=String(d.handoff!.id),handoff=await hireHandoffWorkspace(hrActor,handoffId);
  expect(handoff).toMatchObject({canConsume:true,purchase:{newWork:false,settleExisting:true,history:true},people:[],receipt:null,handoff:{id:handoffId,offerVersion:4,candidateVersion:1,baseAmountMinor:4500000}});expect(handoff.payGroups.map(row=>row.id)).toEqual([group]);expect(JSON.stringify(handoff)).not.toMatch(/consent_evidence|accepted_evidence|source_digest|grants/);
  expect(await talentHireProductSourceEntry(db,entryScope,handoffId)).toEqual({capability:"recruit.offers",purchasePurpose:"history"});expect(await talentHireProductSourceEntry(db,{...entryScope,assignmentVersion:entryScope.assignmentVersion+1},handoffId)).toBeNull();expect(await talentHireProductSourceEntry(db,{...entryScope,branchId:f.branch},handoffId)).toBeNull();expect(await talentHireProductSourceEntry(db,entryScope,"missing-hire")).toBeNull();await expect(hireHandoffWorkspace(creator,handoffId)).rejects.toMatchObject({code:"COMPANY_HR_SCOPE_REQUIRED"});
  const intake=hireConsumptionSchema.parse({handoffId:d.handoff!.id,expectedOfferVersion:4,expectedCandidateVersion:1,payGroupId:group,evidenceRef:"HR reconciled candidate acceptance, identity and proposed pay"}),intakeKey=key(),fault=`hire_final_${key().replaceAll('-','')}`,tables=["talent_hire_consumption_receipts","hr_people","hr_engagements","hr_compensation_revisions","money_transaction_snapshots","command_receipts","audit_events","outbox_events"],snapshot=()=>Promise.all(tables.map(async table=>(await db.prepare(`SELECT row_to_json(t)::text row FROM ${table} t ORDER BY row_to_json(t)::text`).all()).results)),before=await snapshot();
  await db.exec(`CREATE FUNCTION ${fault}() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN UPDATE hr_people SET legal_name='Changed after original hire effects' WHERE id='person-hire-${intakeKey}';RETURN NEW;END $$;CREATE CONSTRAINT TRIGGER ${fault} AFTER INSERT ON audit_events DEFERRABLE INITIALLY DEFERRED FOR EACH ROW WHEN(NEW.command_id='${intakeKey}') EXECUTE FUNCTION ${fault}()`);try{await expect(consumeHireHandoff(f.owner,intake,intakeKey)).rejects.toMatchObject({code:"TALENT_SOURCE_CHANGED"});expect(await snapshot()).toEqual(before);}finally{await db.exec(`DROP TRIGGER ${fault} ON audit_events;DROP FUNCTION ${fault}()`);}const consumed=await consumeHireHandoff(hrActor,intake,intakeKey);
  expect(await hireHandoffWorkspace(hrActor,handoffId)).toMatchObject({canConsume:false,payGroups:[],people:[],receipt:{id:consumed.id,person_id:consumed.personId}});
  expect(await consumeHireHandoff(f.owner,intake,intakeKey)).toEqual(consumed);
  expect(await db.prepare("SELECT p.legal_name,e.status,e.job_title,e.start_date,c.status compensation_status,c.base_amount_minor FROM hr_people p JOIN hr_engagements e ON e.person_id=p.id JOIN hr_compensation_revisions c ON c.engagement_id=e.id WHERE e.id=?").bind(consumed.engagementId).first()).toMatchObject({legal_name:"Candidate One",status:"onboarding",job_title:"Service specialist",start_date:"2026-11-01",compensation_status:"draft",base_amount_minor:4500000});
  expect((await hireHandoffQueue(f.owner)).find(row=>row.id===d.handoff!.id)).toMatchObject({receipt_id:consumed.id,person_id:consumed.personId,engagement_id:consumed.engagementId});
  expect(await db.prepare("SELECT status FROM talent_hire_handoffs WHERE id=?").bind(d.handoff!.id).first("status")).toBe("open");
  await expect(consumeHireHandoff(f.owner,intake,key())).rejects.toMatchObject({code:"HIRE_HANDOFF_ALREADY_CONSUMED"});
  await expect(db.prepare("DELETE FROM talent_hire_consumption_receipts WHERE handoff_id=?").bind(d.handoff!.id).run()).rejects.toThrow("PEOPLE_CONSUMPTION_EVIDENCE_IMMUTABLE");
  expect(await db.prepare("SELECT COUNT(*) n FROM hr_people WHERE id=?").bind(consumed.personId).first("n")).toBe(1);
  const other=await expenseFixture();await expect(consumeHireHandoff(other.owner,{...intake,handoffId:String(d.handoff!.id)},key())).rejects.toMatchObject({code:"HIRE_HANDOFF_NOT_FOUND"});
  expect(await db.prepare("SELECT id FROM talent_candidates WHERE id=?").bind(candidate.id).first("id")).toBe(candidate.id);
  expect((await readCompanyPackageGrants(db,f.company)).map(row=>[row.packageId,row.state])).toEqual([["recruit","draining"]]);
  const {listPeople}=await import("@erpx/server-people_payroll/public/people-payroll");await expect(listPeople(f.owner)).rejects.toMatchObject({code:"PACKAGE_REQUIRED"});
});
it("reconciles an accepted hire into one reviewed existing HR identity without duplicating the source person",async()=>{
  const {app,candidate}=await application();await changeTalentRecord(creator,"application",String(app.id),{action:"interview",expectedVersion:1,reason:"Interview source recorded before reviewed selection"},key());
  const proposal=await createTalentProposal(creator,{applicationId:app.id,applicationVersion:2,recommendation:"hire",scoreBps:9100,evidence:"Independent interview panel retained the recommended identity and role evidence."},key());await changeTalentRecord(reviewer,"proposal",String(proposal.id),{action:"accept-proposal",expectedVersion:1,reason:"Panel proposal checked by a different reviewer"},key());await changeTalentRecord(creator,"application",String(app.id),{action:"select",expectedVersion:2,reason:"Independent proposal authorizes offer preparation"},key());
  const offer=await createTalentOffer(creator,{applicationId:app.id,applicationVersion:3,currency:"THB",baseAmount:"36000.00",startDate:"2026-11-01",expiresOn:"2026-11-15",terms:"Reviewed monthly salary and onboarding conditions."},key());await changeTalentRecord(reviewer,"offer",String(offer.id),{action:"approve-offer",expectedVersion:1,reason:"Different reviewer checked compensation and identity evidence"},key());await changeTalentRecord(creator,"offer",String(offer.id),{action:"send",expectedVersion:2,reason:"Reviewed offer sent to candidate"},key());await changeTalentRecord(creator,"offer",String(offer.id),{action:"accept-offer",expectedVersion:3,reason:"Candidate acceptance recorded against reviewed offer",acceptanceEvidence:"Signed synthetic rehire offer reference SIM-REHIRE-1"},key());
  const candidateEmail=String(await db.prepare("SELECT email FROM talent_candidates WHERE id=?").bind(candidate.id).first("email")),existing=await createPerson(f.owner,createPersonSchema.parse({legalName:"Candidate One",email:candidateEmail,countryCode:"TH"}),key()),group=await createPayGroup(f.owner,payGroupSchema.parse({name:"Reconciled hire monthly group",countryCode:"TH",currency:"THB",frequency:"monthly",timezone:"Asia/Bangkok"}),key()),detail=await talentDetail(creator,String(app.id));
  const intake=hireConsumptionSchema.parse({handoffId:detail.handoff!.id,expectedOfferVersion:4,expectedCandidateVersion:1,payGroupId:group.id,evidenceRef:"HR reviewed signed acceptance and reconciled the existing Company identity."});
  const {store,grant}=recruitmentPurchase();await store.apply({expectedVersion:0,idempotencyKey:key(),grant});vi.stubEnv("ERPX_TALENT_PACKAGE_ENFORCEMENT","enforced");vi.stubEnv("ERPX_PEOPLE_PACKAGE_ENFORCEMENT","enforced");
  await expect(consumeHireHandoff(f.owner,intake,key())).rejects.toMatchObject({code:"HIRE_IDENTITY_RECONCILIATION_REQUIRED"});await expect(consumeHireHandoff(f.owner,{...intake,reconcilePersonId:existing.id,expectedPersonVersion:99},key())).rejects.toMatchObject({code:"HIRE_PERSON_CHANGED"});
  const command=key(),reconciledInput={...intake,reconcilePersonId:existing.id,expectedPersonVersion:1},consumed=await consumeHireHandoff(f.owner,reconciledInput,command);expect(consumed).toMatchObject({personId:existing.id,reconciled:true,status:"consumed"});expect(await consumeHireHandoff(f.owner,reconciledInput,command)).toEqual(consumed);
  expect(await db.prepare("SELECT COUNT(*) n FROM hr_people WHERE company_id=? AND lower(email)=lower(?)").bind(f.company,candidateEmail).first("n")).toBe(1);expect(await db.prepare("SELECT person_id,status FROM hr_engagements WHERE id=?").bind(consumed.engagementId).first()).toEqual({person_id:existing.id,status:"onboarding"});expect(await db.prepare("SELECT base_amount_minor,status FROM hr_compensation_revisions WHERE id=?").bind(consumed.compensationId).first()).toEqual({base_amount_minor:3600000,status:"draft"});
  expect(await db.prepare("SELECT source_offer_version,status FROM talent_hire_handoffs WHERE id=?").bind(detail.handoff!.id).first()).toEqual({source_offer_version:4,status:"open"});expect(await db.prepare("SELECT COUNT(*) n FROM talent_hire_consumption_receipts WHERE handoff_id=? AND person_id=?").bind(detail.handoff!.id,existing.id).first("n")).toBe(1);await expect(consumeHireHandoff(f.owner,reconciledInput,key())).rejects.toMatchObject({code:"HIRE_HANDOFF_ALREADY_CONSUMED"});
});
it("preserves candidate privacy holds, requires independent erasure after withdrawal and retains immutable privacy decisions",async()=>{
  const {store,grant}=recruitmentPurchase();await store.apply({expectedVersion:0,idempotencyKey:key(),grant});vi.stubEnv("ERPX_TALENT_PACKAGE_ENFORCEMENT","enforced");
  const {candidate}=await application();await store.apply({expectedVersion:1,idempotencyKey:key(),grant:{...grant,state:"draining",version:2}});const id=String(candidate.id),legal={...f.tie,role:"legal_manager",assignmentId:await f.grant(f.tie,"legal_manager")};
  legal.modules=["legal"];const privacyScope={companyId:f.company,branchId:null,departmentId:null,userId:legal.userId,assignmentId:legal.assignmentId,assignmentVersion:await db.prepare("SELECT version FROM assignments WHERE id=?").bind(legal.assignmentId).first<number>("version")??0,role:legal.role},before=await candidatePrivacyWorkspace(legal,id);
  expect(before).toMatchObject({candidate:{id,status:"active",privacyHold:false,version:1},actions:{hold:true,release:false,withdraw:false,erase:false},purchase:{newWork:false,settleExisting:true,history:true}});expect(JSON.stringify(before)).not.toMatch(/Candidate One|@example|0800000000|consent|created_by|privacy_requested_by|grants|reason/);expect(await talentCandidatePrivacyProductSourceEntry(db,privacyScope,id)).toEqual({capability:"recruit.candidates",purchasePurpose:"history"});expect(await talentCandidatePrivacyProductSourceEntry(db,{...privacyScope,assignmentVersion:privacyScope.assignmentVersion+1},id)).toBeNull();expect(await talentCandidatePrivacyProductSourceEntry(db,{...privacyScope,branchId:f.branch},id)).toBeNull();
  await changeCandidatePrivacy(f.owner,id,candidatePrivacySchema.parse({action:"hold",expectedVersion:1,reason:"Preserve source for an active synthetic legal matter"}),key());
  await changeCandidatePrivacy(f.owner,id,candidatePrivacySchema.parse({action:"withdraw",expectedVersion:2,reason:"Candidate withdrew recruitment processing in this fixture"}),key());
  await expect(changeCandidatePrivacy(legal,id,candidatePrivacySchema.parse({action:"erase",expectedVersion:3,reason:"Independent erasure review"}),key())).rejects.toMatchObject({code:"CANDIDATE_ERASURE_BLOCKED"});
  await changeCandidatePrivacy(legal,id,candidatePrivacySchema.parse({action:"release",expectedVersion:3,reason:"Legal matter closed and preservation duty released"}),key());
  expect((await candidatePrivacyWorkspace(legal,id)).actions.erase).toBe(true);expect((await candidatePrivacyWorkspace(f.owner,id)).actions.erase).toBe(false);
  await expect(changeCandidatePrivacy(f.owner,id,candidatePrivacySchema.parse({action:"erase",expectedVersion:4,reason:"Self destruction review is prohibited"}),key())).rejects.toMatchObject({code:"SELF_APPROVAL"});
  const request=candidatePrivacySchema.parse({action:"erase",expectedVersion:4,reason:"Independent reviewer verified recruitment withdrawal and no hold"}),command=key(),erased=await changeCandidatePrivacy(legal,id,request,command);expect(await changeCandidatePrivacy(legal,id,request,command)).toEqual(erased);
  expect(await db.prepare("SELECT legal_name,email,phone,status,version,privacy_hold FROM talent_candidates WHERE id=?").bind(id).first()).toMatchObject({legal_name:"Erased recruitment contact",email:`${id}@erased.invalid`,phone:null,status:"erased",version:5,privacy_hold:false});
  expect((await db.prepare("SELECT action FROM talent_candidate_privacy_events WHERE candidate_id=? ORDER BY candidate_version").bind(id).all<{action:string}>()).results.map(e=>e.action)).toEqual(["hold","withdraw","release","erase"]);
  await expect(db.prepare("DELETE FROM talent_candidate_privacy_events WHERE candidate_id=?").bind(id).run()).rejects.toThrow("PEOPLE_CONSUMPTION_EVIDENCE_IMMUTABLE");
  const retained=await candidatePrivacyWorkspace(legal,id);expect(retained.candidate).toMatchObject({status:"erased",privacyHold:false,version:5});expect(retained.history.map(event=>event.action)).toEqual(["hold","withdraw","release","erase"]);expect(Object.values(retained.actions).some(Boolean)).toBe(false);expect(JSON.stringify(retained)).not.toMatch(/@erased|Contact|contact|source_digest|reason/);await db.prepare("UPDATE assignments SET revoked_at=1,version=version+1 WHERE id=?").bind(legal.assignmentId).run();await expect(candidatePrivacyWorkspace(legal,id)).rejects.toMatchObject({code:"TALENT_ACCESS_CHANGED"});expect(await talentCandidatePrivacyProductSourceEntry(db,privacyScope,id)).toBeNull();
});
it("restricts candidate workspace to reviewers and protects requisition source versions", async () => {
  const { req } = await application();
  await expect(
    talentWorkspace({ ...f.claimant, modules: ["talent"] }),
  ).rejects.toMatchObject({ code: "TALENT_PRIVACY_FORBIDDEN" });
  await expect(
    createTalentApplication(
      creator,
      {
        requisitionId: req.id,
        requisitionVersion: 2,
        candidateId: "missing",
        ownerId: creator.userId,
      },
      key(),
    ),
  ).rejects.toMatchObject({ code: "TALENT_SOURCE_CHANGED" });
});
it("withdraws an interviewed application, blocks withdrawn candidate processing and recovers lost command responses",async()=>{
 const {app,candidate}=await application(),input={action:"interview",expectedVersion:1,reason:"Candidate and recruiter scheduled an interview"},command=key();
 await expect(changeTalentRecord({...creator,departmentId:f.accountsDepartment},"application",String(app.id),input,key())).rejects.toMatchObject({status:404});
 const interviewed=await changeTalentRecord(creator,"application",String(app.id),input,command);
 await changeCandidatePrivacy(f.owner,String(candidate.id),candidatePrivacySchema.parse({action:"withdraw",expectedVersion:1,reason:"Candidate withdrew recruitment processing consent"}),key());
 await expect(changeTalentRecord(creator,"application",String(app.id),{action:"select",expectedVersion:2,reason:"Selection cannot use withdrawn consent"},key())).rejects.toMatchObject({code:"TALENT_CANDIDATE_PROCESSING_BLOCKED"});
 const withdrawal={action:"withdraw",expectedVersion:2,reason:"Recruiter completes requested application withdrawal"},withdrawalKey=key(),withdrawn=await changeTalentRecord(creator,"application",String(app.id),withdrawal,withdrawalKey);
 expect(withdrawn).toMatchObject({status:"withdrawn",version:3});
 expect(await changeTalentRecord(creator,"application",String(app.id),withdrawal,withdrawalKey)).toEqual(withdrawn);
 expect(await changeTalentRecord(creator,"application",String(app.id),input,command)).toEqual(interviewed);
 const detail=await talentDetail(creator,String(app.id));expect(detail.application.status).toBe("withdrawn");expect(detail.history.map(event=>event.action)).toEqual(["apply","interview","withdraw"]);
});
it("rechecks Branch and Department scope when opening a Talent application", async () => {
  const { app } = await application();
  await expect(talentDetail({ ...creator, departmentId: f.accountsDepartment }, String(app.id)))
    .rejects.toMatchObject({ status: 404, code: "TALENT_APPLICATION_NOT_FOUND" });
  await expect(talentDetail({ ...creator, branchId: `other-${key()}` }, String(app.id)))
    .rejects.toMatchObject({ status: 404, code: "TALENT_APPLICATION_NOT_FOUND" });
  expect((await talentDetail(creator, String(app.id))).application.id).toBe(app.id);
});
it("surfaces an ineligible application owner for reviewed reassignment", async () => {
  const { app } = await application();
  expect(
    (await talentWork(creator)).find((x) => x.id === app.id),
  ).toMatchObject({ queue: "todo", canAct: true });
  await db
    .prepare(
      "UPDATE assignments SET revoked_at=? WHERE company_id=? AND user_id=?",
    )
    .bind(Date.now(), f.company, creator.userId)
    .run();
  expect(
    (await talentWork(reviewer)).find((x) => x.id === app.id)?.blocker,
  ).toContain("no longer eligible");
  await changeTalentRecord(
    reviewer,
    "application",
    String(app.id),
    {
      action: "reassign",
      expectedVersion: 1,
      reason: "Original recruiter access was revoked",
      ownerId: reviewer.userId,
    },
    key(),
  );
  expect(
    (await talentDetail(reviewer, String(app.id))).application.owner_id,
  ).toBe(reviewer.userId);
});

it("requires the actual Recruitment package for new work and keeps bounded application withdrawal and source history while draining",async()=>{
 vi.stubEnv("ERPX_TALENT_PACKAGE_ENFORCEMENT","enforced");await expect(application()).rejects.toMatchObject({code:"PACKAGE_REQUIRED"});expect(await talentWork(creator)).toEqual([]);const {store,grant}=recruitmentPurchase();await store.apply({expectedVersion:0,idempotencyKey:key(),grant});const {app}=await application();await store.apply({expectedVersion:1,idempotencyKey:key(),grant:{...grant,state:"draining",version:2}});await expect(createTalentCandidate(creator,{legalName:"New unpurchased intake",email:`candidate-${key()}@example.test`,consentEvidence:"New recruitment consent must require active work",retentionUntil:"2027-09-30"},key())).rejects.toMatchObject({code:"PACKAGE_DRAINING"});expect((await talentWork(creator)).find(item=>item.id===app.id)).toMatchObject({canAct:false,queue:"waiting"});
 const command=key(),input={action:"withdraw",expectedVersion:1,reason:"Complete the retained recruitment withdrawal"},result=await changeTalentRecord(creator,"application",String(app.id),input,command);expect(await changeTalentRecord(creator,"application",String(app.id),input,command)).toEqual(result);expect((await talentDetail(creator,String(app.id))).purchases?.["recruit.applications"]).toMatchObject({newWork:false,settleExisting:true,history:true});
});
it("rolls Recruitment source and selected-role changes back after deferred effects, preserving original receipts and exact retry",async()=>{
 const {store,grant}=recruitmentPurchase();await store.apply({expectedVersion:0,idempotencyKey:key(),grant});vi.stubEnv("ERPX_TALENT_PACKAGE_ENFORCEMENT","enforced");const {app,candidate}=await application(),input={action:"interview",expectedVersion:1,reason:"Original current recruitment interview source"},tables=["talent_requisitions","talent_candidates","talent_applications","talent_application_events","assignments","command_receipts","audit_events","outbox_events"],snapshot=()=>Promise.all(tables.map(async table=>(await db.prepare(`SELECT row_to_json(t)::text row FROM ${table} t ORDER BY row_to_json(t)::text`).all()).results));
 for(const fault of ["source","authority"] as const){const command=key(),name=`talent_final_${key().replaceAll('-','')}`,before=await snapshot(),effect=fault==="source"?`UPDATE talent_candidates SET retention_until='2020-01-01' WHERE id='${candidate.id}'`:`UPDATE assignments SET revoked_at=1,version=version+1 WHERE id='${creator.assignmentId}'`;await db.exec(`CREATE FUNCTION ${name}() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN ${effect};RETURN NEW;END $$;CREATE CONSTRAINT TRIGGER ${name} AFTER INSERT ON audit_events DEFERRABLE INITIALLY DEFERRED FOR EACH ROW WHEN(NEW.command_id='${command}') EXECUTE FUNCTION ${name}()`);try{await expect(changeTalentRecord(creator,"application",String(app.id),input,command)).rejects.toMatchObject({code:fault==="source"?"TALENT_SOURCE_CHANGED":"TALENT_ACCESS_CHANGED"});expect(await snapshot()).toEqual(before);}finally{await db.exec(`DROP TRIGGER ${name} ON audit_events;DROP FUNCTION ${name}()`);}}
 const command=key(),result=await changeTalentRecord(creator,"application",String(app.id),input,command);expect(await changeTalentRecord(creator,"application",String(app.id),input,command)).toEqual(result);const {withTalentPurchasedCommand}=await import("@erpx/server-talent/adapters/purchases");await expect(db.transaction(tx=>withTalentPurchasedCommand(creator,{key:key(),operation:"talent.application.interview",capability:"recruit.applications",purpose:"new-work",target:{kind:"application",id:String(app.id)},permissions:["records.write","records.review"]},async()=>true,tx))).rejects.toMatchObject({code:"TALENT_COMPOSITION_REQUIRED"});await db.prepare("UPDATE assignments SET revoked_at=1,version=version+1 WHERE id=?").bind(creator.assignmentId).run();await expect(changeTalentRecord(creator,"application",String(app.id),input,command)).rejects.toMatchObject({code:"TALENT_ACCESS_CHANGED"});
});

async function recruitmentFileSources(){const {app,candidate}=await application();await changeTalentRecord(creator,"application",String(app.id),{action:"interview",expectedVersion:1,reason:"Native actual candidate document preparation"},key());const proposal=await createTalentProposal(creator,{applicationId:app.id,applicationVersion:2,recommendation:"hire",scoreBps:9000,evidence:"Native independently reviewed application evidence"},key());await changeTalentRecord(reviewer,"proposal",String(proposal.id),{action:"accept-proposal",expectedVersion:1,reason:"Different reviewer checked actual source"},key());await changeTalentRecord(creator,"application",String(app.id),{action:"select",expectedVersion:2,reason:"Native accepted proposal before offer documents"},key());const offer=await createTalentOffer(creator,{applicationId:app.id,applicationVersion:3,currency:"THB",baseAmount:"32000.00",startDate:"2026-11-01",expiresOn:"2026-11-15",terms:"Actual monthly offer subject to onboarding verification"},key());return [{targetType:"talent_candidate",targetId:String(candidate.id),purposeKey:"cv"},{targetType:"talent_offer",targetId:String(offer.id),purposeKey:"offer_document"}] as const;}

it.each(["disabled","enforced"] as const)("preserves the exact accepted hire source until HR delivery before allowing new Offer Files in %s mode",async mode=>{
 const group=await createPayGroup(f.owner,payGroupSchema.parse({name:"Original accepted hire delivery group",countryCode:"TH",currency:"THB",frequency:"monthly",timezone:"Asia/Bangkok"}),key()),{store,grant}=recruitmentPurchase();await store.apply({expectedVersion:0,idempotencyKey:key(),grant});vi.stubEnv("ERPX_TALENT_PACKAGE_ENFORCEMENT",mode);vi.stubEnv("ERPX_PEOPLE_PACKAGE_ENFORCEMENT","enforced");
 const sources=await recruitmentFileSources(),offer=sources[1].targetId,appId=String(await db.prepare("SELECT application_id FROM talent_offers WHERE id=?").bind(offer).first("application_id")),{seedAttachmentSet,enableAttachmentV2ForTest,attachmentBindingState}=await import("../../support/helpers/attachment-v2"),{bindRecordAttachmentSet,attachmentContent}=await import("@erpx/server-files/public/attachments/service");enableAttachmentV2ForTest();const prior=await seedAttachmentSet(db,creator,{targetType:"talent_offer",targetId:offer,componentKind:"record",purposeKey:"offer_document",expectedTargetVersion:1}),priorCommand=key();await bindRecordAttachmentSet(db,creator,prior.setId,1,priorCommand);const objectKey=String(await db.prepare("SELECT object_key FROM attachments WHERE id=?").bind(prior.sourceId).first("object_key")),body="Exact independently reviewed original Offer evidence";await env.H001_FILES.put(objectKey,body,{httpMetadata:{contentType:"application/pdf"}});await db.prepare("UPDATE attachments SET legal_hold=1 WHERE id=?").bind(prior.sourceId).run();
 await changeTalentRecord(reviewer,"offer",offer,{action:"approve-offer",expectedVersion:2,reason:"Independent review includes the original bound Offer evidence"},key());await changeTalentRecord(creator,"offer",offer,{action:"send",expectedVersion:3,reason:"Original source document sent before acceptance"},key());await changeTalentRecord(creator,"offer",offer,{action:"accept-offer",expectedVersion:4,reason:"Candidate accepted this exact original offer",acceptanceEvidence:"Original signed candidate acceptance and identity source"},key());const detail=await talentDetail(creator,appId),handoff=detail.handoff!,image=await db.prepare("SELECT row_to_json(h)::text image FROM talent_hire_handoffs h WHERE id=?").bind(handoff.id).first("image"),next=await seedAttachmentSet(db,creator,{targetType:"talent_offer",targetId:offer,componentKind:"record",purposeKey:"offer_document",expectedTargetVersion:5}),command=key();
 expect(detail.offerDocumentsEditable).toBe(false);await expect(bindRecordAttachmentSet(db,creator,next.setId,5,command)).rejects.toMatchObject({code:"HIRE_HANDOFF_PENDING"});expect(await db.prepare("SELECT version FROM talent_offers WHERE id=?").bind(offer).first("version")).toBe(5);expect(await attachmentBindingState(db,next.setId)).toMatchObject({set_state:"draft",link_state:"draft"});for(const table of ["command_receipts","audit_events","outbox_events"])expect(await db.prepare(`SELECT 1 FROM ${table} WHERE command_id=?`).bind(command).first()).toBeNull();expect(await (await attachmentContent(db,env.H001_FILES,creator,prior.linkId,"original")).object.text()).toBe(body);await bindRecordAttachmentSet(db,creator,prior.setId,1,priorCommand);
 const intake=hireConsumptionSchema.parse({handoffId:handoff.id,expectedOfferVersion:5,expectedCandidateVersion:1,payGroupId:group.id,evidenceRef:"Original Company HR reconciled accepted exact source before document updates"}),consumeKey=key(),hr={...f.owner,modules:["people"]},receipt=await consumeHireHandoff(hr,intake,consumeKey);expect((await talentDetail(creator,appId)).offerDocumentsEditable).toBe(true);await bindRecordAttachmentSet(db,creator,next.setId,5,command);expect(await attachmentBindingState(db,next.setId)).toMatchObject({set_state:"bound",link_state:"bound"});expect(await consumeHireHandoff(hr,intake,consumeKey)).toEqual(receipt);expect(await db.prepare("SELECT row_to_json(h)::text image FROM talent_hire_handoffs h WHERE id=?").bind(handoff.id).first("image")).toBe(image);expect(await db.prepare("SELECT source_offer_version FROM talent_hire_consumption_receipts WHERE handoff_id=?").bind(handoff.id).first("source_offer_version")).toBe(5);expect((await readCompanyPackageGrants(db,f.company)).map(row=>row.packageId)).toEqual(["recruit"]);expect(await (await attachmentContent(db,env.H001_FILES,creator,prior.linkId,"original")).object.text()).toBe(body);
});
it("keeps actual Recruitment private document bytes and Legal holds while draining and denies missing purchases and current-role revocation",async()=>{
 const {seedAttachmentSet,enableAttachmentV2ForTest,attachmentBindingState}=await import("../../support/helpers/attachment-v2"),{listAttachmentSets,bindRecordAttachmentSet,attachmentContent}=await import("@erpx/server-files/public/attachments/service");enableAttachmentV2ForTest();const sources=await recruitmentFileSources(),files=await Promise.all(sources.map(source=>seedAttachmentSet(db,creator,{...source,componentKind:"record",expectedTargetVersion:1})));vi.stubEnv("ERPX_TALENT_PACKAGE_ENFORCEMENT","enforced");for(const [index,source] of sources.entries()){await expect(listAttachmentSets(db,creator,{...source,componentKind:"record"})).rejects.toMatchObject({code:"PACKAGE_REQUIRED"});await expect(bindRecordAttachmentSet(db,creator,files[index].setId,1,key())).rejects.toMatchObject({code:"PACKAGE_REQUIRED"});}
 const {store,grant}=recruitmentPurchase();await store.apply({expectedVersion:0,idempotencyKey:key(),grant});await store.apply({expectedVersion:1,idempotencyKey:key(),grant:{...grant,state:"draining",version:2}});for(const [index,source] of sources.entries()){const file=files[index],command=key(),body=`Private retained ${source.targetType} document`;await bindRecordAttachmentSet(db,creator,file.setId,1,command);await bindRecordAttachmentSet(db,creator,file.setId,1,command);expect(await attachmentBindingState(db,file.setId)).toMatchObject({set_state:"bound",link_state:"bound"});const objectKey=String(await db.prepare("SELECT object_key FROM attachments WHERE id=?").bind(file.sourceId).first("object_key"));await env.H001_FILES.put(objectKey,body,{httpMetadata:{contentType:"application/pdf"}});await db.prepare("UPDATE attachments SET legal_hold=1 WHERE id=?").bind(file.sourceId).run();expect(await (await attachmentContent(db,env.H001_FILES,creator,file.linkId,"original")).object.text()).toBe(body);}
 await changeTalentRecord(creator,"offer",sources[1].targetId,{action:"withdraw",expectedVersion:2,reason:"Discharge existing offer after retained document review"},key());expect(await (await attachmentContent(db,env.H001_FILES,creator,files[1].linkId,"original")).object.text()).toContain("Private retained talent_offer");await db.prepare("UPDATE assignments SET revoked_at=1,version=version+1 WHERE id=?").bind(creator.assignmentId).run();for(const file of files)await expect(attachmentContent(db,env.H001_FILES,creator,file.linkId,"original")).rejects.toMatchObject({status:404});
});
it("rolls actual candidate and offer File sources and selected authority back after deferred effects before exact retry",async()=>{
 const {seedAttachmentSet,enableAttachmentV2ForTest,attachmentBindingState}=await import("../../support/helpers/attachment-v2"),{bindRecordAttachmentSet}=await import("@erpx/server-files/public/attachments/service");enableAttachmentV2ForTest();const {store,grant}=recruitmentPurchase();await store.apply({expectedVersion:0,idempotencyKey:key(),grant});vi.stubEnv("ERPX_TALENT_PACKAGE_ENFORCEMENT","enforced");const sources=await recruitmentFileSources();
 for(const source of sources){const file=await seedAttachmentSet(db,creator,{...source,componentKind:"record",expectedTargetVersion:1});for(const fault of ["source","authority"] as const){const command=key(),name=`talent_file_${key().replaceAll('-','')}`,tables=["talent_candidates","talent_offers","attachment_sets","attachment_links","attachment_events","assignments","command_receipts","audit_events","outbox_events"],snapshot=()=>Promise.all(tables.map(async table=>(await db.prepare(`SELECT row_to_json(t)::text row FROM ${table} t ORDER BY row_to_json(t)::text`).all()).results)),before=await snapshot(),effect=fault==="source"?`UPDATE talent_candidates SET retention_until='2020-01-01' WHERE id='${sources[0].targetId}'`:`UPDATE assignments SET revoked_at=1,version=version+1 WHERE id='${creator.assignmentId}'`;await db.exec(`CREATE FUNCTION ${name}() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN ${effect};RETURN NEW;END $$;CREATE CONSTRAINT TRIGGER ${name} AFTER INSERT ON audit_events DEFERRABLE INITIALLY DEFERRED FOR EACH ROW WHEN(NEW.command_id='${command}') EXECUTE FUNCTION ${name}()`);try{await expect(bindRecordAttachmentSet(db,creator,file.setId,1,command)).rejects.toMatchObject(fault==="source"?{code:"TALENT_ATTACHMENT_CHANGED"}:{status:409});expect(await snapshot()).toEqual(before);}finally{await db.exec(`DROP TRIGGER ${name} ON audit_events;DROP FUNCTION ${name}()`);}}
 const command=key();await bindRecordAttachmentSet(db,creator,file.setId,1,command);await bindRecordAttachmentSet(db,creator,file.setId,1,command);expect(await attachmentBindingState(db,file.setId)).toMatchObject({set_state:"bound",link_state:"bound"});}
});

it("commits concurrent Recruitment intake for separate Companies without cross-owner table lock upgrades or mixed identities",async()=>{
 const other=await expenseFixture(),otherActor={...other.owner,branchId:other.branch,departmentId:other.department,modules:["talent"]};const {store,grant}=recruitmentPurchase();await store.apply({expectedVersion:0,idempotencyKey:key(),grant});await store.apply({expectedVersion:0,idempotencyKey:key(),grant:{...grant,companyId:other.company}});vi.stubEnv("ERPX_TALENT_PACKAGE_ENFORCEMENT","enforced");const input={legalName:"Concurrent Recruitment intake",email:`concurrent-${key()}@example.test`,consentEvidence:"Actual separately scoped concurrent intake consent",retentionUntil:"2027-09-30"},first=key(),second=key(),measured=await measureReads(()=>Promise.all([createTalentCandidate(creator,input,first),createTalentCandidate(otherActor,input,second)])),results=measured.result;expect(measured.metrics.deadlockFailures??0).toBe(0);expect(results.map(result=>result.id)).toEqual([`talent-candidate-${first}`,`talent-candidate-${second}`]);for(const [id,company] of [[results[0].id,f.company],[results[1].id,other.company]])expect(await db.prepare("SELECT company_id FROM talent_candidates WHERE id=?").bind(id).first("company_id")).toBe(company);
});

it("filters actual Recruitment Records, search and both notification limits before pagination with current private-source authority",async()=>{
 const {req,app}=await application(),actor=creator,now=Date.now(),otherReq=key(),foreign=key();
 await db.prepare("INSERT INTO talent_requisitions SELECT ?,company_id,branch_id,NULL,job_title,openings,target_start_date,description,status,created_by,reviewed_by,version,created_at,updated_at FROM talent_requisitions WHERE id=?").bind(otherReq,req.id).run();
 await db.prepare("INSERT INTO talent_applications(id,company_id,requisition_id,requisition_version,candidate_id,owner_id,status,version,created_by,created_at,updated_at) SELECT ?,company_id,?,requisition_version,candidate_id,owner_id,status,version,created_by,created_at,updated_at FROM talent_applications WHERE id=?").bind(foreign,otherReq,app.id).run();
 for(const [id,section] of [[String(req.id),"requisitions"],[String(app.id),"applications"],[foreign,"applications"]])await db.prepare("INSERT INTO workspace_records(module_id,id,section_id,title,description,status,fields_json,company_id,branch_id,department_id,created_by,updated_by,created_at,updated_at) VALUES('talent',?,?,'Actual Recruitment source projected','','draft','[]',?,?,?,?,?,?,?)").bind(id,section,f.company,f.branch,f.department,actor.userId,actor.userId,now,now).run();
 const {listRecords,recordsPage,getRecord,searchReferences}=await import("@erpx/server-records/public/records"),{listStaffNotifications}=await import("@erpx/server-collaboration/public/staff-notifications"),{publishInboxEvent,inboxPage}=await import("@erpx/server-collaboration/public/inbox");
 for(let index=0;index<106;index++){const missing=`missing-talent-${key()}`;await db.prepare("INSERT INTO staff_notifications(id,company_id,user_id,record_type,record_id,record_version,message,created_at) VALUES(?,?,?,'talent_application',?,1,'Missing private source must not consume limit',?)").bind(key(),f.company,actor.userId,missing,now+1000+index).run();await publishInboxEvent(db,{key:key(),companyId:f.company,sourceType:"talent_application",sourceId:missing,sourceVersion:1,category:"work",preview:"Missing Recruitment source",recipientIds:[actor.userId],createdAt:now+1000+index});}
 await db.prepare("INSERT INTO staff_notifications(id,company_id,user_id,record_type,record_id,record_version,message,created_at) VALUES(?,?,?,'talent_application',?,1,'Review actual private Recruitment source',?)").bind(key(),f.company,actor.userId,String(app.id),now).run();
 await publishInboxEvent(db,{key:key(),companyId:f.company,sourceType:"talent_application",sourceId:String(app.id),sourceVersion:1,category:"work",preview:"Review actual Recruitment application",recipientIds:[actor.userId],createdAt:now});
 vi.stubEnv("ERPX_TALENT_PACKAGE_ENFORCEMENT","enforced");expect(await listRecords(actor,"talent","requisitions")).toEqual([]);expect((await recordsPage(actor,"talent","applications",{})).items).toEqual([]);expect(await searchReferences(actor,"Actual Recruitment source")).toEqual([]);expect(await listStaffNotifications(db,actor)).toEqual([]);expect((await inboxPage(db,actor,{view:"all"})).items).toEqual([]);await expect(getRecord(actor,"talent",String(app.id))).rejects.toMatchObject({code:"PACKAGE_REQUIRED"});
 const {store,grant}=recruitmentPurchase();await store.apply({expectedVersion:0,idempotencyKey:key(),grant});await store.apply({expectedVersion:1,idempotencyKey:key(),grant:{...grant,state:"draining",version:2}});
 expect((await listRecords(actor,"talent","requisitions")).map(row=>row.id)).toEqual([req.id]);expect((await recordsPage(actor,"talent","applications",{})).items.map(row=>row.id)).toEqual([app.id]);expect((await searchReferences(actor,"Actual Recruitment source")).map(row=>{if(!("id" in row)||typeof row.id!=="string")throw Error("Actual Recruitment reference ID is missing");return row.id;}).sort()).toEqual([req.id,app.id].sort());await expect(getRecord(actor,"talent",foreign)).rejects.toMatchObject({code:"TALENT_SOURCE_NOT_FOUND"});expect((await listStaffNotifications(db,actor)).map(row=>row.record_id)).toContain(app.id);expect((await inboxPage(db,actor,{view:"all"})).items.map(row=>row.sourceId)).toEqual([app.id,app.id]);
 await db.prepare("UPDATE assignments SET revoked_at=1,version=version+1 WHERE id=?").bind(actor.assignmentId).run();expect(await listStaffNotifications(db,actor)).toEqual([]);expect((await inboxPage(db,actor,{view:"all"})).items).toEqual([]);expect(await db.prepare("SELECT COUNT(*)::int count FROM staff_notifications WHERE company_id=? AND record_type='talent_application'").bind(f.company).first("count")).toBe(107);
});
it("aligns Recruitment metric catalogue, drilldown and metadata evidence with actual purchased history and rechecks a captured source callback",async()=>{
 const {app}=await application(),actor={...f.owner,modules:["talent"]},filters={from:"2026-01-01",to:"2026-12-31",currency:"THB"},ids=["open-requisitions","application-stage-counts"] as const,{metricCatalogFor,queryMetric}=await import("@erpx/server-runtime/public/executive/metrics"),{readExecutiveMetricProjection}=await import("@erpx/server-talent/public/executive-metric-projections"),{searchExecutiveEvidence,readExecutiveEvidence}=await import("@erpx/server-runtime/public/executive/evidence");
 vi.stubEnv("ERPX_TALENT_PACKAGE_ENFORCEMENT","enforced");for(const id of ids){expect((await metricCatalogFor(db,actor)).find(row=>row.id===id)?.available).toBe(false);await expect(queryMetric(db,actor,id,filters)).rejects.toMatchObject({status:403});await expect(searchExecutiveEvidence(db,actor,{metric:id,filters,limit:1})).rejects.toMatchObject({status:403});}
 const {store,grant}=recruitmentPurchase();await store.apply({expectedVersion:0,idempotencyKey:key(),grant});await store.apply({expectedVersion:1,idempotencyKey:key(),grant:{...grant,state:"draining",version:2}});for(const id of ids){const metric=await queryMetric(db,actor,id,filters);expect(metric.total).toBe("1");const source=await queryMetric(db,actor,id,filters,"default",{label:metric.rows[0].label,page:1});expect(source.rows).toHaveLength(1);expect(JSON.stringify(source.rows)).not.toMatch(/candidate_name|consent_evidence|0800000000|@example/);const evidence=await searchExecutiveEvidence(db,actor,{metric:id,filters,limit:1});expect(evidence.items).toHaveLength(1);const detail=await readExecutiveEvidence(db,actor,id,evidence.items[0].id,filters);expect(detail.metric).toBe(id);expect(JSON.stringify(detail)).not.toMatch(/consent_evidence|0800000000|@example/);}
 const snapshot=await readExecutiveMetricProjection(db,actor,"application-stage-counts",filters,"default");expect(snapshot.raw).toHaveLength(1);await db.prepare("UPDATE assignments SET revoked_at=1,version=version+1 WHERE id=?").bind(actor.assignmentId).run();await expect(snapshot.source({label:"applied",page:1})).rejects.toMatchObject({code:"METRIC_SOURCE_UNAVAILABLE"});expect((await metricCatalogFor(db,actor)).find(row=>row.id==="application-stage-counts")?.available).toBe(false);expect(await db.prepare("SELECT id FROM talent_applications WHERE id=?").bind(app.id).first("id")).toBe(app.id);
});
it("denies unpurchased, foreign and unauthorized privacy readers without exposing candidate contacts through an HR or Legal source entry",async()=>{
 const {candidate}=await application(),id=String(candidate.id),actor={...f.owner,modules:["people"]},scope={companyId:f.company,branchId:null,departmentId:null,userId:actor.userId,assignmentId:actor.assignmentId,assignmentVersion:await db.prepare("SELECT version FROM assignments WHERE id=?").bind(actor.assignmentId).first<number>("version")??0,role:actor.role};vi.stubEnv("ERPX_TALENT_PACKAGE_ENFORCEMENT","enforced");await expect(candidatePrivacyWorkspace(actor,id)).rejects.toMatchObject({code:"PACKAGE_REQUIRED"});expect(await talentCandidatePrivacyProductSourceEntry(db,scope,id)).toBeNull();const {store,grant}=recruitmentPurchase();await store.apply({expectedVersion:0,idempotencyKey:key(),grant});await expect(candidatePrivacyWorkspace({...f.claimant,modules:["talent"]},id)).rejects.toMatchObject({code:"CANDIDATE_PRIVACY_FORBIDDEN"});await expect(candidatePrivacyWorkspace(creator,id)).rejects.toMatchObject({code:"COMPANY_HR_SCOPE_REQUIRED"});await expect(candidatePrivacyWorkspace(actor,"missing-candidate")).rejects.toMatchObject({code:"CANDIDATE_NOT_FOUND"});const other=await expenseFixture();await store.apply({expectedVersion:0,idempotencyKey:key(),grant:{...grant,companyId:other.company}});await expect(candidatePrivacyWorkspace(other.owner,id)).rejects.toMatchObject({code:"CANDIDATE_NOT_FOUND"});expect(await talentCandidatePrivacyProductSourceEntry(db,{...scope,companyId:other.company},id)).toBeNull();const data=await candidatePrivacyWorkspace(actor,id);expect(data.actions.withdraw).toBe(true);expect(Object.keys(data.candidate).sort()).toEqual(["id","privacyHold","retentionUntil","status","version"]);expect(JSON.stringify(data)).not.toMatch(/Candidate One|@example|0800000000|consent|privacy_requested_by|reason/);
});

const recruitSettlementInput=()=>({companyId:f.company,packageId:"recruit",packageVersion:1});
async function recruitSettlement(){const {withOwnerPackageObligations,readOwnerPackageObligations}=await import("@erpx/server-commercial/public/owner-package-obligations");return withOwnerPackageObligations(db,recruitSettlementInput(),tx=>readOwnerPackageObligations(db,tx,{...recruitSettlementInput(),now:Date.now()}));}
async function recruitWithdrawal(){const {readCompanyPackageGrants,packageGrantRevision}=await import("@erpx/server-commercial/public/package-grants"),grants=await readCompanyPackageGrants(db,f.company),current=grants.find(row=>row.packageId==="recruit")!;return {companyId:f.company,expectedRevision:packageGrantRevision(f.company,grants),idempotencyKey:key(),changes:[{expectedVersion:current.version,grant:{...current,state:"inactive" as const,version:current.version+1,reason:"Actual Recruitment owners discharged processing and accepted-hire obligations",evidence:"test:recruitment-native-settlement"}}]};}
async function recruitWithdrawalReview(request:Awaited<ReturnType<typeof recruitWithdrawal>>,apply=false,after?:()=>Promise<void>){const {withOwnerPackageObligations,ownerPackageObligations}=await import("@erpx/server-commercial/public/owner-package-obligations"),store=packageGrantStore(db,{authorize:async()=>({userId:"isolated-recruitment-settlement-provider"}),obligations:(tx,companyId,packageId)=>ownerPackageObligations(db,tx,companyId,packageId)});return withOwnerPackageObligations(db,recruitSettlementInput(),async tx=>{const result=apply?await store.batch.applyIn(tx,request,async()=>{}):await store.batch.previewIn(tx,request);if(after){talentDatabase=tx;try{await after();}finally{talentDatabase=undefined;}}return result;});}
it("withdraws Recruit only after actual recruitment processing and accepted HR delivery settle while preserving Legal-held bytes, private history and exact replay",async()=>{
 const group=await createPayGroup(f.owner,payGroupSchema.parse({name:"Existing accepted hire pay group",countryCode:"TH",currency:"THB",frequency:"monthly",timezone:"Asia/Bangkok"}),key()),{store,grant}=recruitmentPurchase();await store.apply({expectedVersion:0,idempotencyKey:key(),grant});vi.stubEnv("ERPX_TALENT_PACKAGE_ENFORCEMENT","enforced");vi.stubEnv("ERPX_PEOPLE_PACKAGE_ENFORCEMENT","enforced");const sources=await recruitmentFileSources(),offer=sources[1].targetId,appId=String(await db.prepare("SELECT application_id FROM talent_offers WHERE id=?").bind(offer).first("application_id"));await changeTalentRecord(reviewer,"offer",offer,{action:"approve-offer",expectedVersion:1,reason:"Independent retained source offer approval"},key());await changeTalentRecord(creator,"offer",offer,{action:"send",expectedVersion:2,reason:"Reviewed original offer sent to candidate"},key());await store.apply({expectedVersion:1,idempotencyKey:key(),grant:{...grant,state:"draining",version:2}});const request=await recruitWithdrawal();await expect(recruitWithdrawalReview(request)).rejects.toThrow("PACKAGE_OBLIGATIONS_REMAIN");await expect(recruitWithdrawalReview(request,true)).rejects.toThrow("PACKAGE_OBLIGATIONS_REMAIN");
 const acceptKey=key(),accept={action:"accept-offer",expectedVersion:3,reason:"Candidate accepted original retained offer",acceptanceEvidence:"Original signed acceptance and identity evidence"};await changeTalentRecord(creator,"offer",offer,accept,acceptKey);const detail=await talentDetail(creator,appId),id=String(detail.handoff!.id),observed=await recruitSettlement();expect(observed.observations.map(row=>row.moduleId)).toEqual(["talent","people_payroll"]);expect(observed.observations.find(row=>row.moduleId==="people_payroll")?.outstanding).toBeGreaterThan(0);expect(JSON.stringify(observed)).not.toMatch(/Candidate One|@example|consent|source_digest|person_id|base_amount/);
 const hr={...f.owner,modules:["people"]},intake=hireConsumptionSchema.parse({handoffId:id,expectedOfferVersion:4,expectedCandidateVersion:1,payGroupId:group.id,evidenceRef:"Original Company HR received and reconciled accepted source"}),consumeKey=key(),consumed=await consumeHireHandoff(hr,intake,consumeKey);expect((await recruitSettlement()).observations.find(row=>row.moduleId==="people_payroll")?.outstanding).toBe(0);expect((await recruitSettlement()).total).toBeGreaterThan(0);
 const {seedAttachmentSet,enableAttachmentV2ForTest}=await import("../../support/helpers/attachment-v2"),{bindRecordAttachmentSet,attachmentContent}=await import("@erpx/server-files/public/attachments/service");enableAttachmentV2ForTest();const candidateId=sources[0].targetId,document=await seedAttachmentSet(db,creator,{targetType:"talent_candidate",targetId:candidateId,componentKind:"record",purposeKey:"cv",expectedTargetVersion:1});await bindRecordAttachmentSet(db,creator,document.setId,1,key());const objectKey=String(await db.prepare("SELECT object_key FROM attachments WHERE id=?").bind(document.sourceId).first("object_key"));await env.H001_FILES.put(objectKey,"Original private candidate CV retained under Legal hold",{httpMetadata:{contentType:"application/pdf"}});await db.prepare("UPDATE attachments SET legal_hold=1 WHERE id=?").bind(document.sourceId).run();await changeCandidatePrivacy(hr,candidateId,candidatePrivacySchema.parse({action:"hold",expectedVersion:2,reason:"Preserve original candidate evidence for a Legal obligation"}),key());const withdrawKey=key(),withdraw=candidatePrivacySchema.parse({action:"withdraw",expectedVersion:3,reason:"Explicitly end Company recruitment processing without erasing retained evidence"});await changeCandidatePrivacy(hr,candidateId,withdraw,withdrawKey);const reqId=String(detail.application.requisition_id);await changeTalentRecord(creator,"requisition",reqId,{action:"close",expectedVersion:3,reason:"Original accepted recruitment need has been discharged"},key());expect((await recruitSettlement()).total).toBe(0);
 const tables=["talent_requisitions","talent_candidates","talent_applications","talent_proposals","talent_offers","talent_hire_handoffs","talent_hire_consumption_receipts","talent_candidate_privacy_events","hr_people","hr_engagements","hr_compensation_revisions","money_transaction_snapshots","attachments","attachment_links"],history=()=>Promise.all(tables.map(async table=>(await db.prepare(`SELECT row_to_json(t)::text row FROM ${table} t ORDER BY row_to_json(t)::text`).all()).results)),before=await history();expect((await recruitWithdrawalReview(request)).after.find(row=>row.packageId==="recruit")?.state).toBe("inactive");const applied=await recruitWithdrawalReview(request,true);expect(await recruitWithdrawalReview(request,true)).toEqual(applied);expect(await history()).toEqual(before);expect(await consumeHireHandoff(hr,intake,consumeKey)).toEqual(consumed);await changeTalentRecord(creator,"offer",offer,accept,acceptKey);await changeCandidatePrivacy(hr,candidateId,withdraw,withdrawKey);expect((await candidatePrivacyWorkspace(hr,candidateId)).candidate).toMatchObject({status:"withdrawn",privacyHold:true,version:4});expect((await talentDetail(creator,appId)).application.status).toBe("accepted");expect(await (await attachmentContent(db,env.H001_FILES,creator,document.linkId,"original")).object.text()).toBe("Original private candidate CV retained under Legal hold");await expect(createTalentCandidate(creator,{legalName:"New forbidden inactive intake",email:`inactive-${key()}@example.test`,consentEvidence:"Inactive cannot create recruitment activity",retentionUntil:"2027-09-30"},key())).rejects.toMatchObject({code:"PACKAGE_INACTIVE"});await expect(changeCandidatePrivacy(hr,candidateId,candidatePrivacySchema.parse({action:"erase",expectedVersion:4,reason:"An active hold cannot be bypassed by commercial inactivity"}),key())).rejects.toMatchObject({code:"CANDIDATE_ERASURE_BLOCKED"});
 await db.prepare("UPDATE assignments SET revoked_at=1,version=version+1 WHERE id=?").bind(hr.assignmentId).run();await expect(consumeHireHandoff(hr,intake,consumeKey)).rejects.toMatchObject({code:"TALENT_ACCESS_CHANGED"});await expect(changeTalentRecord(creator,"offer",offer,accept,acceptKey)).rejects.toMatchObject({code:"TALENT_ACCESS_CHANGED"});await expect(attachmentContent(db,env.H001_FILES,creator,document.linkId,"original")).rejects.toMatchObject({status:404});expect((await recruitSettlement()).total).toBe(0);expect(await db.prepare("SELECT count(*)::int count FROM command_receipts WHERE command_id=?").bind(consumeKey).first("count")).toBe(1);
});
it("settles an actually declined offer without rewriting its original offered application and keeps ambiguous legacy sources outstanding",async()=>{
 const {store,grant}=recruitmentPurchase();await store.apply({expectedVersion:0,idempotencyKey:key(),grant});vi.stubEnv("ERPX_TALENT_PACKAGE_ENFORCEMENT","enforced");const sources=await recruitmentFileSources(),offer=sources[1].targetId,appId=String(await db.prepare("SELECT application_id FROM talent_offers WHERE id=?").bind(offer).first("application_id"));await changeTalentRecord(reviewer,"offer",offer,{action:"approve-offer",expectedVersion:1,reason:"Independent original offer approval"},key());await changeTalentRecord(creator,"offer",offer,{action:"send",expectedVersion:2,reason:"Original sent offer awaits a candidate decision"},key());await store.apply({expectedVersion:1,idempotencyKey:key(),grant:{...grant,state:"draining",version:2}});await changeTalentRecord(creator,"offer",offer,{action:"decline",expectedVersion:3,reason:"Candidate declined the retained reviewed offer"},key());await changeCandidatePrivacy(f.owner,sources[0].targetId,candidatePrivacySchema.parse({action:"withdraw",expectedVersion:1,reason:"End candidate processing after actual offer decline"}),key());const detail=await talentDetail(creator,appId);await changeTalentRecord(creator,"requisition",String(detail.application.requisition_id),{action:"close",expectedVersion:3,reason:"Close original headcount after actual candidate decline"},key());expect(detail.application.status).toBe("offered");expect((await recruitSettlement()).total).toBe(0);
 const request=await recruitWithdrawal();await recruitWithdrawalReview(request);const legacy=key();await db.prepare("INSERT INTO talent_offers(id,company_id,application_id,application_version,currency,base_amount_minor,start_date,expires_on,terms,status,created_by,version,created_at,updated_at) VALUES(?,?,?,?, 'THB',10000,'2026-11-01','2026-11-15','Legacy ambiguous source','accepted',?,1,1,1)").bind(legacy,f.company,`missing-application-${key()}`,1,creator.userId).run().then(()=>{throw Error("Expected original foreign key to reject missing application");},cause=>expect(cause).toMatchObject({code:"23503"}));
 await db.prepare("UPDATE talent_offers SET status='accepted',version=version+1 WHERE id=?").bind(offer).run();expect((await recruitSettlement()).total).toBeGreaterThan(0);await expect(recruitWithdrawalReview(request,true)).rejects.toThrow("PACKAGE_OBLIGATIONS_REMAIN");expect(await db.prepare("SELECT state,version FROM company_package_grants WHERE company_id=? AND package_id='recruit'").bind(f.company).first()).toEqual({state:"draining",version:2});
});
it("requires exact fresh Recruit source leases and locks every native obligation table including absent future rows",async()=>{
 const {withOwnerPackageObligations}=await import("@erpx/server-commercial/public/owner-package-obligations"),{talentPackageObligations}=await import("@erpx/server-talent/public/package-obligations"),{talentPeoplePackageObligations}=await import("@erpx/server-people_payroll/public/talent-package-obligations"),input={...recruitSettlementInput(),moduleId:"talent",now:Date.now()};await expect(db.transaction(tx=>talentPackageObligations(db,tx,input))).rejects.toThrow("TALENT_PACKAGE_OBLIGATION_SOURCE_REQUIRED");await expect(db.transaction(tx=>talentPeoplePackageObligations(db,tx,{...input,moduleId:"people_payroll"}))).rejects.toThrow("TALENT_PEOPLE_PACKAGE_OBLIGATION_SOURCE_REQUIRED");expect(()=>withOwnerPackageObligations(db,{...recruitSettlementInput(),packageVersion:2},tx=>talentPackageObligations(db,tx,{...input,packageVersion:2}))).toThrow("PACKAGE_OBLIGATION_SOURCES_REQUIRED");await expect(withOwnerPackageObligations(db,recruitSettlementInput(),tx=>talentPackageObligations(db,tx,{...input,companyId:"foreign-company"}))).rejects.toThrow("TALENT_PACKAGE_OBLIGATION_SOURCE_REQUIRED");await expect(withOwnerPackageObligations(db,recruitSettlementInput(),tx=>talentPackageObligations(db,tx,{...input,moduleId:"people_payroll"}))).rejects.toThrow("TALENT_PACKAGE_OBLIGATION_SCOPE_REQUIRED");
 await withOwnerPackageObligations(db,recruitSettlementInput(),async tx=>{expect((await talentPackageObligations(db,tx,input)).outstanding).toBe(0);for(const table of ["talent_requisitions","talent_candidates","talent_applications","talent_proposals","talent_offers","talent_hire_handoffs","talent_hire_consumption_receipts","talent_candidate_privacy_events","hr_people","hr_engagements","hr_compensation_revisions","hr_pay_groups","money_transaction_snapshots","command_receipts","organization_nodes"]){await expect(db.transaction(async competing=>{await competing.prepare("SET LOCAL lock_timeout='100ms'").run();await competing.prepare(`LOCK TABLE ${table} IN ROW EXCLUSIVE MODE`).run();})).rejects.toMatchObject({code:"55P03"});}});
});
it("acquires Recruit withdrawal sources before the first snapshot and observes a concurrently committed actual candidate processing obligation",async()=>{
 const {store,grant}=recruitmentPurchase();await store.apply({expectedVersion:0,idempotencyKey:key(),grant});await store.apply({expectedVersion:1,idempotencyKey:key(),grant:{...grant,state:"draining",version:2}});const request=await recruitWithdrawal(),command=key();let ready:()=>void=()=>{},failed:(cause:unknown)=>void=()=>{},release:()=>void=()=>{};const sourceReady=new Promise<void>((resolve,reject)=>{ready=resolve;failed=reject;}),finish=new Promise<void>(resolve=>{release=resolve;});
 const writer=db.transaction(async tx=>{talentDatabase=tx;try{await createTalentCandidate(creator,{legalName:"Concurrent real candidate",email:`source-${key()}@example.test`,consentEvidence:"Actual legacy compatible owner intake while Commercial sources wait",retentionUntil:"2027-09-30"},command);}finally{talentDatabase=undefined;}ready();await finish;}).catch(cause=>{failed(cause);throw cause;});try{await sourceReady;let completed=false;const review=recruitWithdrawalReview(request).then(()=>null,cause=>cause).finally(()=>{completed=true;});await new Promise(resolve=>setTimeout(resolve,100));expect(completed).toBe(false);release();await writer;expect(await review).toMatchObject({message:"PACKAGE_OBLIGATIONS_REMAIN"});}finally{release();talentDatabase=undefined;await writer.catch(()=>{});}expect(await db.prepare("SELECT status FROM talent_candidates WHERE id=?").bind(`talent-candidate-${command}`).first("status")).toBe("active");
});
it("rolls Recruit withdrawal and late normal or deferred owner activity back together after the original Commercial Apply",async()=>{
 const {store,grant}=recruitmentPurchase();await store.apply({expectedVersion:0,idempotencyKey:key(),grant});await store.apply({expectedVersion:1,idempotencyKey:key(),grant:{...grant,state:"draining",version:2}});const candidate=await createTalentCandidate(creator,{legalName:"Original retained closed candidate",email:`retained-${key()}@example.test`,consentEvidence:"Original retained private processing consent",retentionUntil:"2027-09-30"},key());await changeCandidatePrivacy(f.owner,String(candidate.id),candidatePrivacySchema.parse({action:"withdraw",expectedVersion:1,reason:"Original Company privacy withdrawal before Commercial review"}),key());expect((await recruitSettlement()).total).toBe(0);const request=await recruitWithdrawal(),command=key();await expect(recruitWithdrawalReview(request,true,()=>createTalentCandidate(creator,{legalName:"Late normal legacy intake",email:`late-${key()}@example.test`,consentEvidence:"Actual normal owner command in same leased transaction",retentionUntil:"2027-09-30"},command).then(()=>{}))).rejects.toThrow("PACKAGE_OBLIGATIONS_REMAIN");expect(await db.prepare("SELECT id FROM talent_candidates WHERE id=?").bind(`talent-candidate-${command}`).first()).toBeNull();for(const table of ["command_receipts","audit_events","outbox_events"])expect(await db.prepare(`SELECT 1 FROM ${table} WHERE command_id=?`).bind(command).first()).toBeNull();
 const name=`recruit_deferred_${key().replaceAll('-','')}`,before=await db.prepare("SELECT * FROM talent_candidates WHERE id=?").bind(candidate.id).first();await db.exec(`CREATE SEQUENCE ${name}_attempt;CREATE FUNCTION ${name}() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN PERFORM nextval('${name}_attempt');UPDATE talent_candidates SET status='active',version=version+1 WHERE id='${candidate.id}';RETURN NEW;END $$;CREATE CONSTRAINT TRIGGER ${name} AFTER INSERT ON company_package_change_sets DEFERRABLE INITIALLY DEFERRED FOR EACH ROW WHEN(NEW.company_id='${f.company}') EXECUTE FUNCTION ${name}()`);try{await expect(recruitWithdrawalReview(request,true)).rejects.toThrow("PACKAGE_OBLIGATIONS_REMAIN");expect(await db.prepare(`SELECT is_called FROM ${name}_attempt`).first("is_called")).toBe(true);expect(await db.prepare("SELECT * FROM talent_candidates WHERE id=?").bind(candidate.id).first()).toEqual(before);expect(await db.prepare("SELECT state,version FROM company_package_grants WHERE company_id=? AND package_id='recruit'").bind(f.company).first()).toEqual({state:"draining",version:2});expect(await db.prepare("SELECT id FROM company_package_change_sets WHERE id=?").bind(request.idempotencyKey).first()).toBeNull();}finally{await db.exec(`DROP TRIGGER ${name} ON company_package_change_sets;DROP FUNCTION ${name}();DROP SEQUENCE ${name}_attempt`);}
});


it.each(['disabled','enforced'] as const)('buffers original Recruitment candidate and offer private bytes with final privacy, ownership and current authority (%s)',async mode=>{
 const {seedAttachmentSet,enableAttachmentV2ForTest}=await import('../../support/helpers/attachment-v2'),{bindRecordAttachmentSet,attachmentContent}=await import('@erpx/server-files/public/attachments/service');enableAttachmentV2ForTest();const {store,grant}=recruitmentPurchase();await store.apply({expectedVersion:0,idempotencyKey:key(),grant});vi.stubEnv('ERPX_TALENT_PACKAGE_ENFORCEMENT',mode);const sources=await recruitmentFileSources();
 for(const source of sources){const document=await seedAttachmentSet(db,creator,{...source,componentKind:'record',expectedTargetVersion:1});await bindRecordAttachmentSet(db,creator,document.setId,1,key());const objectKey=String(await db.prepare('SELECT object_key FROM attachments WHERE id=?').bind(document.sourceId).first('object_key')),bytes=`Exact retained private ${source.targetType}`;await env.H001_FILES.put(objectKey,bytes);let consumed=0;
  const files:FileStore={put:(...args)=>env.H001_FILES.put(...args),get:async(...args)=>{const object=await env.H001_FILES.get(...args);return object?{...object,arrayBuffer:async()=>{consumed++;return object.arrayBuffer();}}:null;},head:(...args)=>env.H001_FILES.head(...args),delete:(...args)=>env.H001_FILES.delete(...args),list:(...args)=>env.H001_FILES.list(...args)};
  for(const effect of [`UPDATE talent_candidates SET retention_until='2020-01-01' WHERE id='${sources[0].targetId}'`,`UPDATE talent_requisitions SET description='Late original application source' WHERE id=(SELECT requisition_id FROM talent_applications WHERE candidate_id='${sources[0].targetId}')`,`UPDATE attachments SET legal_hold=1 WHERE id='${document.sourceId}'`,`UPDATE assignments SET revoked_at=1,version=version+1 WHERE id='${creator.assignmentId}'`]){
   const fault=`talent_private_${key().replaceAll('-','')}`;await db.exec(`CREATE FUNCTION ${fault}() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN ${effect};RETURN NEW;END $$;CREATE CONSTRAINT TRIGGER ${fault} AFTER INSERT ON attachment_access_events DEFERRABLE INITIALLY DEFERRED FOR EACH ROW WHEN(NEW.link_id='${document.linkId}' AND NEW.outcome='allowed') EXECUTE FUNCTION ${fault}()`);try{await expect(attachmentContent(db,files,creator,document.linkId,'original')).rejects.toMatchObject({code:'TALENT_FILE_READ_CHANGED'});}finally{await db.exec(`DROP TRIGGER ${fault} ON attachment_access_events;DROP FUNCTION ${fault}()`);}expect(await db.prepare("SELECT count(*)::int n FROM attachment_access_events WHERE link_id=? AND outcome='allowed'").bind(document.linkId).first('n')).toBe(0);
  }
  expect(consumed).toBe(4);await db.prepare('UPDATE attachments SET legal_hold=1 WHERE id=?').bind(document.sourceId).run();expect(await(await attachmentContent(db,files,creator,document.linkId,'original')).object.text()).toBe(bytes);expect(consumed).toBe(5);await expect(db.transaction(tx=>attachmentContent(tx,files,creator,document.linkId,'original'))).rejects.toMatchObject({code:'TALENT_FILE_READ_TRANSACTION_REQUIRED'});expect(consumed).toBe(5);
 }
});
it('preserves original masked Recruitment and exact unavailable denial audits only while native evidence and denied authority stay unchanged',async()=>{
 const {seedAttachmentSet,enableAttachmentV2ForTest}=await import('../../support/helpers/attachment-v2'),{attachmentContent}=await import('@erpx/server-files/public/attachments/service');enableAttachmentV2ForTest();vi.stubEnv('ERPX_TALENT_PACKAGE_ENFORCEMENT','disabled');const source=(await recruitmentFileSources())[0],document=await seedAttachmentSet(db,creator,{...source,componentKind:'record',expectedTargetVersion:1}),objectKey=String(await db.prepare('SELECT object_key FROM attachments WHERE id=?').bind(document.sourceId).first('object_key'));
 await expect(attachmentContent(db,env.H001_FILES,creator,document.linkId,'original')).rejects.toMatchObject({status:503,code:'FILE_UNAVAILABLE'});await env.H001_FILES.put(objectKey,'Original masked candidate privacy');const scanning=await seedAttachmentSet(db,creator,{...source,componentKind:'record',componentRef:'scanning-cv',expectedTargetVersion:1,sourceState:'scanning'});await expect(attachmentContent(db,env.H001_FILES,creator,scanning.linkId,'original')).rejects.toMatchObject({status:423,code:'ATTACHMENT_NOT_AVAILABLE'});await db.prepare('UPDATE assignments SET revoked_at=1,version=version+1 WHERE id=?').bind(creator.assignmentId).run();await expect(attachmentContent(db,env.H001_FILES,creator,document.linkId,'original')).rejects.toMatchObject({status:404});const before=Number(await db.prepare("SELECT count(*) n FROM attachment_access_events WHERE link_id=? AND outcome='denied'").bind(document.linkId).first('n'));expect(before).toBe(2);
 const fault=`talent_denied_${key().replaceAll('-','')}`;await db.exec(`CREATE FUNCTION ${fault}() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN UPDATE assignments SET version=version+1 WHERE id='${creator.assignmentId}';RETURN NEW;END $$;CREATE CONSTRAINT TRIGGER ${fault} AFTER INSERT ON attachment_access_events DEFERRABLE INITIALLY DEFERRED FOR EACH ROW WHEN(NEW.link_id='${document.linkId}' AND NEW.outcome='denied') EXECUTE FUNCTION ${fault}()`);try{await expect(attachmentContent(db,env.H001_FILES,creator,document.linkId,'original')).rejects.toMatchObject({code:'TALENT_FILE_READ_CHANGED'});}finally{await db.exec(`DROP TRIGGER ${fault} ON attachment_access_events;DROP FUNCTION ${fault}()`);}expect(Number(await db.prepare("SELECT count(*) n FROM attachment_access_events WHERE link_id=? AND outcome='denied'").bind(document.linkId).first('n'))).toBe(before);
});
it('observes actual native candidate Legal preservation before its byte snapshot and isolates two Company source-writing private reads',async()=>{
 const {seedAttachmentSet,enableAttachmentV2ForTest}=await import('../../support/helpers/attachment-v2'),{bindRecordAttachmentSet,attachmentContent}=await import('@erpx/server-files/public/attachments/service'),{committedSourceVisibility}=await import('../../support/helpers/source-writer-visibility');enableAttachmentV2ForTest();vi.stubEnv('ERPX_TALENT_PACKAGE_ENFORCEMENT','disabled');const source=(await recruitmentFileSources())[0],document=await seedAttachmentSet(db,creator,{...source,componentKind:'record',expectedTargetVersion:1});await bindRecordAttachmentSet(db,creator,document.setId,1,key());const objectKey=String(await db.prepare('SELECT object_key FROM attachments WHERE id=?').bind(document.sourceId).first('object_key'));await env.H001_FILES.put(objectKey,'Native candidate evidence retained under original Legal hold');const fault=`talent_committed_${key().replaceAll('-','')}`;
 await db.exec(`CREATE FUNCTION ${fault}() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN IF NOT(SELECT privacy_hold FROM talent_candidates WHERE id='${source.targetId}') THEN RAISE EXCEPTION 'STALE_NATIVE_CANDIDATE_PRIVACY';END IF;RETURN NEW;END $$;CREATE CONSTRAINT TRIGGER ${fault} AFTER INSERT ON attachment_access_events DEFERRABLE INITIALLY DEFERRED FOR EACH ROW WHEN(NEW.link_id='${document.linkId}' AND NEW.outcome='allowed') EXECUTE FUNCTION ${fault}()`);
 try{const observed=await committedSourceVisibility(db,async tx=>{talentDatabase=tx;try{await changeCandidatePrivacy(f.owner,source.targetId,candidatePrivacySchema.parse({action:'hold',expectedVersion:2,reason:'Original Legal preservation is committed before the private candidate byte snapshot'}),key());}finally{talentDatabase=undefined;}},()=>measureReads(()=>attachmentContent(db,env.H001_FILES,creator,document.linkId,'original')));expect(await observed.result.object.text()).toBe('Native candidate evidence retained under original Legal hold');expect(observed.metrics.deadlockFailures??0).toBe(0);expect(observed.metrics.serializationFailures??0).toBe(0);expect(observed.metrics.transactionRetries??0).toBe(0);}finally{talentDatabase=undefined;await db.exec(`DROP TRIGGER ${fault} ON attachment_access_events;DROP FUNCTION ${fault}()`);}
 const other=await expenseFixture(),second={...other.owner,modules:['talent']},rows:{actor:WorkContext;id:string;file:Awaited<ReturnType<typeof seedAttachmentSet>>;bytes:string}[]=[];
 for(const actor of [{...f.owner,modules:['talent']},second]){const id=String((await createTalentCandidate(actor,{legalName:'Separate Company retained candidate',email:`private-${key()}@example.test`,consentEvidence:'Original explicit native isolated candidate intake consent',retentionUntil:'2027-09-30'},key())).id),file=await seedAttachmentSet(db,actor,{targetType:'talent_candidate',targetId:id,componentKind:'record',purposeKey:'cv',expectedTargetVersion:1});await bindRecordAttachmentSet(db,actor,file.setId,1,key());const objectKey=String(await db.prepare('SELECT object_key FROM attachments WHERE id=?').bind(file.sourceId).first('object_key')),bytes=`Native private Company ${actor.companyId}`;await env.H001_FILES.put(objectKey,bytes);rows.push({actor,id,file,bytes});}
 const parallel=`talent_parallel_${key().replaceAll('-','')}`;await db.exec(`CREATE FUNCTION ${parallel}() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN PERFORM pg_sleep(0.08);UPDATE talent_candidates SET legal_name=legal_name WHERE id=CASE WHEN NEW.company_id='${f.company}' THEN '${rows[0].id}' ELSE '${rows[1].id}' END;RETURN NEW;END $$;CREATE CONSTRAINT TRIGGER ${parallel} AFTER INSERT ON attachment_access_events DEFERRABLE INITIALLY DEFERRED FOR EACH ROW WHEN(NEW.link_id IN('${rows[0].file.linkId}','${rows[1].file.linkId}') AND NEW.outcome='allowed') EXECUTE FUNCTION ${parallel}()`);
 try{const observed=await measureReads(()=>Promise.all(rows.map(row=>attachmentContent(db,env.H001_FILES,row.actor,row.file.linkId,'original'))));for(let index=0;index<rows.length;index++)expect(await observed.result[index].object.text()).toBe(rows[index].bytes);expect(observed.metrics.deadlockFailures??0).toBe(0);expect(observed.metrics.serializationFailures??0).toBe(0);expect(observed.metrics.transactionRetries??0).toBe(0);}finally{await db.exec(`DROP TRIGGER ${parallel} ON attachment_access_events;DROP FUNCTION ${parallel}()`);}
});

// Diagnostic fixture starts with no Company pay group. Every positive business
// effect below uses an original owner; this is not normal Company creation proof.
import {createHireHandoffPayGroup} from '@erpx/server-people_payroll/public/hire-handoff-pay-group';
async function acceptedHireWithoutPayGroup(){
 expect(Number(await db.prepare('SELECT count(*) n FROM hr_pay_groups WHERE company_id=?').bind(f.company).first('n'))).toBe(0);
 const {store,grant}=recruitmentPurchase();await store.apply({expectedVersion:0,idempotencyKey:key(),grant});vi.stubEnv('ERPX_TALENT_PACKAGE_ENFORCEMENT','enforced');vi.stubEnv('ERPX_PEOPLE_PACKAGE_ENFORCEMENT','enforced');
 const sources=await recruitmentFileSources(),offer=sources[1].targetId;await changeTalentRecord(reviewer,'offer',offer,{action:'approve-offer',expectedVersion:1,reason:'Independent reviewer checked the original accepted-hire source'},key());await changeTalentRecord(creator,'offer',offer,{action:'send',expectedVersion:2,reason:'Original reviewed offer sent for actual candidate acceptance'},key());await changeTalentRecord(creator,'offer',offer,{action:'accept-offer',expectedVersion:3,reason:'Candidate accepted the original terms',acceptanceEvidence:'Recorded candidate acceptance and identity before HR intake'},key());
 const appId=String(await db.prepare('SELECT application_id FROM talent_offers WHERE id=?').bind(offer).first('application_id')),handoff=(await talentDetail(creator,appId)).handoff!,handoffId=String(handoff.id);
 return {store,grant,handoffId,input:{expectedOfferVersion:4,expectedCandidateVersion:1,input:payGroupSchema.parse({name:'Accepted-hire monthly Company group',countryCode:'TH',currency:'THB',frequency:'monthly',timezone:'Asia/Bangkok'})}};
}
it('bootstraps only the missing accepted-hire pay group with Recruit alone, then consumes original onboarding and replays without Payroll access',async()=>{
 const s=await acceptedHireWithoutPayGroup(),before=await hireHandoffWorkspace(f.owner,s.handoffId,{strictProduct:true});expect(before).toMatchObject({companyId:f.company,canCreatePayGroup:true,payGroups:[],canConsume:true});
 await expect(createPayGroup(f.owner,s.input.input,key())).rejects.toMatchObject({code:'PACKAGE_REQUIRED'});expect(Number(await db.prepare('SELECT count(*) n FROM hr_pay_groups WHERE company_id=?').bind(f.company).first('n'))).toBe(0);
 await expect(createHireHandoffPayGroup(f.owner,s.handoffId,{...s.input,input:{...s.input.input,currency:'USD'}},key())).rejects.toMatchObject({code:'HIRE_PAY_GROUP_INVALID'});
 const command=key(),created=await createHireHandoffPayGroup(f.owner,s.handoffId,s.input,command);expect(created).toMatchObject({handoffId:s.handoffId,currency:'THB',status:'active'});expect(created.id).toMatch(/^paygrp-/);expect(await createHireHandoffPayGroup(f.owner,s.handoffId,s.input,command)).toEqual(created);
 const current=await hireHandoffWorkspace(f.owner,s.handoffId,{strictProduct:true});expect(current.canCreatePayGroup??false).toBe(false);expect(current.payGroups).toEqual([{id:created.id,name:s.input.input.name,currency:'THB',frequency:'monthly'}]);await expect(createHireHandoffPayGroup(f.owner,s.handoffId,s.input,key())).rejects.toMatchObject({code:'HIRE_PAY_GROUP_ALREADY_AVAILABLE'});
 const intake=hireConsumptionSchema.parse({handoffId:s.handoffId,expectedOfferVersion:4,expectedCandidateVersion:1,payGroupId:created.id,evidenceRef:'Company HR reviewed accepted candidate identity and original pay before onboarding'}),receipt=await consumeHireHandoff(f.owner,intake,key());expect(receipt).toMatchObject({handoffId:s.handoffId,status:'consumed'});expect(await db.prepare('SELECT pay_group_id FROM hr_engagements WHERE id=? AND company_id=?').bind(receipt.engagementId,f.company).first('pay_group_id')).toBe(created.id);expect(await createHireHandoffPayGroup(f.owner,s.handoffId,s.input,command)).toEqual(created);expect((await readCompanyPackageGrants(db,f.company)).map(row=>row.packageId)).toEqual(['recruit']);
 await expect(createPayGroup(f.owner,{...s.input.input,name:'General Payroll remains unavailable'},key())).rejects.toMatchObject({code:'PACKAGE_REQUIRED'});expect(Number(await db.prepare('SELECT count(*) n FROM hr_pay_groups WHERE company_id=?').bind(f.company).first('n'))).toBe(1);
});
it('accepted-hire pay-group late source changes roll back original group, receipts, audit and outbox and preserve same-key recovery',async()=>{
 const s=await acceptedHireWithoutPayGroup(),command=key(),fault=`hire_group_${key().replaceAll('-','')}`,tables=['hr_pay_groups','hr_command_receipts','command_receipts','audit_events','outbox_events','talent_candidates','talent_hire_handoffs','talent_hire_consumption_receipts'],snapshot=()=>Promise.all(tables.map(async table=>(await db.prepare(`SELECT row_to_json(t)::text image FROM ${table} t ORDER BY row_to_json(t)::text`).all()).results)),before=await snapshot();
 await db.exec(`CREATE FUNCTION ${fault}() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN UPDATE talent_candidates SET consent_evidence='Changed inside the original deferred effect diagnostic' WHERE id=(SELECT candidate_id FROM talent_hire_handoffs WHERE id='${s.handoffId}');RETURN NEW;END $$;CREATE CONSTRAINT TRIGGER ${fault} AFTER INSERT ON command_receipts DEFERRABLE INITIALLY DEFERRED FOR EACH ROW WHEN(NEW.command_id='${command}') EXECUTE FUNCTION ${fault}()`);
 try{await expect(createHireHandoffPayGroup(f.owner,s.handoffId,s.input,command)).rejects.toMatchObject({code:'HIRE_PAY_GROUP_SOURCE_CHANGED'});expect(await snapshot()).toEqual(before);}finally{await db.exec(`DROP TRIGGER ${fault} ON command_receipts;DROP FUNCTION ${fault}()`);}
 const result=await createHireHandoffPayGroup(f.owner,s.handoffId,s.input,command);expect(await createHireHandoffPayGroup(f.owner,s.handoffId,s.input,command)).toEqual(result);
});
