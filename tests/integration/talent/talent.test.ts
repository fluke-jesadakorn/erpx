import { beforeAll, beforeEach, expect, it, vi } from "vitest";
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
} from "../../../src/server/modules/talent/public/talent";
import type { WorkContext } from "../../../src/server/modules/identity_access/public/authorization";
import { candidatePrivacySchema,changeCandidatePrivacy,consumeHireHandoff,hireConsumptionSchema,hireHandoffQueue } from "../../../src/server/modules/people_payroll/public/people-handoffs";
import {createPerson,createPersonSchema,createPayGroup,payGroupSchema} from "../../../src/server/modules/people_payroll/public/people-payroll";
vi.mock("../../../src/server/modules/runtime/adapters/runtime", () => ({
  holdingResources: async () => ({ write: env.H001_DB, files: env.H001_FILES }),
  runtimeEnv: async () => env,
}));
let f: Awaited<ReturnType<typeof expenseFixture>>,
  creator: WorkContext,
  reviewer: WorkContext;
beforeAll(async () => {
  await applyMigrations(db, env.STAFF_TEST_MIGRATIONS);
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
it("moves a consented candidate through proposal, reviewed offer and immutable hire handoff", async () => {
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
  const intake=hireConsumptionSchema.parse({handoffId:d.handoff!.id,expectedOfferVersion:4,expectedCandidateVersion:1,payGroupId:group,evidenceRef:"HR reconciled candidate acceptance, identity and proposed pay"}),intakeKey=key(),consumed=await consumeHireHandoff(f.owner,intake,intakeKey);
  expect(await consumeHireHandoff(f.owner,intake,intakeKey)).toEqual(consumed);
  expect(await db.prepare("SELECT p.legal_name,e.status,e.job_title,e.start_date,c.status compensation_status,c.base_amount_minor FROM hr_people p JOIN hr_engagements e ON e.person_id=p.id JOIN hr_compensation_revisions c ON c.engagement_id=e.id WHERE e.id=?").bind(consumed.engagementId).first()).toMatchObject({legal_name:"Candidate One",status:"onboarding",job_title:"Service specialist",start_date:"2026-11-01",compensation_status:"draft",base_amount_minor:4500000});
  expect((await hireHandoffQueue(f.owner)).find(row=>row.id===d.handoff!.id)).toMatchObject({receipt_id:consumed.id,person_id:consumed.personId,engagement_id:consumed.engagementId});
  expect(await db.prepare("SELECT status FROM talent_hire_handoffs WHERE id=?").bind(d.handoff!.id).first("status")).toBe("open");
  await expect(consumeHireHandoff(f.owner,intake,key())).rejects.toMatchObject({code:"HIRE_HANDOFF_ALREADY_CONSUMED"});
  await expect(db.prepare("DELETE FROM talent_hire_consumption_receipts WHERE handoff_id=?").bind(d.handoff!.id).run()).rejects.toThrow("PEOPLE_CONSUMPTION_EVIDENCE_IMMUTABLE");
  expect(await db.prepare("SELECT COUNT(*) n FROM hr_people WHERE id=?").bind(consumed.personId).first("n")).toBe(1);
  const other=await expenseFixture();await expect(consumeHireHandoff(other.owner,{...intake,handoffId:String(d.handoff!.id)},key())).rejects.toMatchObject({code:"HIRE_HANDOFF_NOT_FOUND"});
  expect(await db.prepare("SELECT id FROM talent_candidates WHERE id=?").bind(candidate.id).first("id")).toBe(candidate.id);
});
it("reconciles an accepted hire into one reviewed existing HR identity without duplicating the source person",async()=>{
  const {app,candidate}=await application();await changeTalentRecord(creator,"application",String(app.id),{action:"interview",expectedVersion:1,reason:"Interview source recorded before reviewed selection"},key());
  const proposal=await createTalentProposal(creator,{applicationId:app.id,applicationVersion:2,recommendation:"hire",scoreBps:9100,evidence:"Independent interview panel retained the recommended identity and role evidence."},key());await changeTalentRecord(reviewer,"proposal",String(proposal.id),{action:"accept-proposal",expectedVersion:1,reason:"Panel proposal checked by a different reviewer"},key());await changeTalentRecord(creator,"application",String(app.id),{action:"select",expectedVersion:2,reason:"Independent proposal authorizes offer preparation"},key());
  const offer=await createTalentOffer(creator,{applicationId:app.id,applicationVersion:3,currency:"THB",baseAmount:"36000.00",startDate:"2026-11-01",expiresOn:"2026-11-15",terms:"Reviewed monthly salary and onboarding conditions."},key());await changeTalentRecord(reviewer,"offer",String(offer.id),{action:"approve-offer",expectedVersion:1,reason:"Different reviewer checked compensation and identity evidence"},key());await changeTalentRecord(creator,"offer",String(offer.id),{action:"send",expectedVersion:2,reason:"Reviewed offer sent to candidate"},key());await changeTalentRecord(creator,"offer",String(offer.id),{action:"accept-offer",expectedVersion:3,reason:"Candidate acceptance recorded against reviewed offer",acceptanceEvidence:"Signed synthetic rehire offer reference SIM-REHIRE-1"},key());
  const candidateEmail=String(await db.prepare("SELECT email FROM talent_candidates WHERE id=?").bind(candidate.id).first("email")),existing=await createPerson(f.owner,createPersonSchema.parse({legalName:"Candidate One",email:candidateEmail,countryCode:"TH"}),key()),group=await createPayGroup(f.owner,payGroupSchema.parse({name:"Reconciled hire monthly group",countryCode:"TH",currency:"THB",frequency:"monthly",timezone:"Asia/Bangkok"}),key()),detail=await talentDetail(creator,String(app.id));
  const intake=hireConsumptionSchema.parse({handoffId:detail.handoff!.id,expectedOfferVersion:4,expectedCandidateVersion:1,payGroupId:group.id,evidenceRef:"HR reviewed signed acceptance and reconciled the existing Company identity."});
  await expect(consumeHireHandoff(f.owner,intake,key())).rejects.toMatchObject({code:"HIRE_IDENTITY_RECONCILIATION_REQUIRED"});await expect(consumeHireHandoff(f.owner,{...intake,reconcilePersonId:existing.id,expectedPersonVersion:99},key())).rejects.toMatchObject({code:"HIRE_PERSON_CHANGED"});
  const command=key(),reconciledInput={...intake,reconcilePersonId:existing.id,expectedPersonVersion:1},consumed=await consumeHireHandoff(f.owner,reconciledInput,command);expect(consumed).toMatchObject({personId:existing.id,reconciled:true,status:"consumed"});expect(await consumeHireHandoff(f.owner,reconciledInput,command)).toEqual(consumed);
  expect(await db.prepare("SELECT COUNT(*) n FROM hr_people WHERE company_id=? AND lower(email)=lower(?)").bind(f.company,candidateEmail).first("n")).toBe(1);expect(await db.prepare("SELECT person_id,status FROM hr_engagements WHERE id=?").bind(consumed.engagementId).first()).toEqual({person_id:existing.id,status:"onboarding"});expect(await db.prepare("SELECT base_amount_minor,status FROM hr_compensation_revisions WHERE id=?").bind(consumed.compensationId).first()).toEqual({base_amount_minor:3600000,status:"draft"});
  expect(await db.prepare("SELECT source_offer_version,status FROM talent_hire_handoffs WHERE id=?").bind(detail.handoff!.id).first()).toEqual({source_offer_version:4,status:"open"});expect(await db.prepare("SELECT COUNT(*) n FROM talent_hire_consumption_receipts WHERE handoff_id=? AND person_id=?").bind(detail.handoff!.id,existing.id).first("n")).toBe(1);await expect(consumeHireHandoff(f.owner,reconciledInput,key())).rejects.toMatchObject({code:"HIRE_HANDOFF_ALREADY_CONSUMED"});
});
it("preserves candidate privacy holds, requires independent erasure after withdrawal and retains immutable privacy decisions",async()=>{
  const {candidate}=await application(),id=String(candidate.id),legal={...f.tie,role:"legal_manager",assignmentId:await f.grant(f.tie,"legal_manager")};
  await changeCandidatePrivacy(f.owner,id,candidatePrivacySchema.parse({action:"hold",expectedVersion:1,reason:"Preserve source for an active synthetic legal matter"}),key());
  await changeCandidatePrivacy(f.owner,id,candidatePrivacySchema.parse({action:"withdraw",expectedVersion:2,reason:"Candidate withdrew recruitment processing in this fixture"}),key());
  await expect(changeCandidatePrivacy(legal,id,candidatePrivacySchema.parse({action:"erase",expectedVersion:3,reason:"Independent erasure review"}),key())).rejects.toMatchObject({code:"CANDIDATE_ERASURE_BLOCKED"});
  await changeCandidatePrivacy(legal,id,candidatePrivacySchema.parse({action:"release",expectedVersion:3,reason:"Legal matter closed and preservation duty released"}),key());
  await expect(changeCandidatePrivacy(f.owner,id,candidatePrivacySchema.parse({action:"erase",expectedVersion:4,reason:"Self destruction review is prohibited"}),key())).rejects.toMatchObject({code:"SELF_APPROVAL"});
  const request=candidatePrivacySchema.parse({action:"erase",expectedVersion:4,reason:"Independent reviewer verified recruitment withdrawal and no hold"}),command=key(),erased=await changeCandidatePrivacy(legal,id,request,command);expect(await changeCandidatePrivacy(legal,id,request,command)).toEqual(erased);
  expect(await db.prepare("SELECT legal_name,email,phone,status,version,privacy_hold FROM talent_candidates WHERE id=?").bind(id).first()).toMatchObject({legal_name:"Erased recruitment contact",email:`${id}@erased.invalid`,phone:null,status:"erased",version:5,privacy_hold:false});
  expect((await db.prepare("SELECT action FROM talent_candidate_privacy_events WHERE candidate_id=? ORDER BY candidate_version").bind(id).all<{action:string}>()).results.map(e=>e.action)).toEqual(["hold","withdraw","release","erase"]);
  await expect(db.prepare("DELETE FROM talent_candidate_privacy_events WHERE candidate_id=?").bind(id).run()).rejects.toThrow("PEOPLE_CONSUMPTION_EVIDENCE_IMMUTABLE");
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
