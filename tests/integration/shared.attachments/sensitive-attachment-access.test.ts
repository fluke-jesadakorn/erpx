import { beforeAll, beforeEach, expect, it, vi } from "vitest";
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
} from "../../../src/server/modules/files/public/attachments/policy";
import {
  bindRecordAttachmentSet,
  listAttachmentSets,
} from "../../../src/server/modules/files/public/attachments/service";
import { sensitiveAttachmentPolicies } from "../../../src/server/modules/files/public/attachments/sensitive-policy";
import type { WorkContext } from "../../../src/server/modules/identity_access/public/authorization";

vi.mock("../../../src/server/modules/runtime/adapters/runtime", () => ({
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

beforeEach(async () => {
  f = await expenseFixture();
});

it("keeps the sensitive workforce registry exact and fails closed for hidden HR purposes", () => {
  expect(
    sensitiveAttachmentPolicies
      .map((policy) => `${policy.targetType}:${policy.purposeKey}:${policy.componentKinds.join(",")}`)
      .sort(),
  ).toEqual(
    [
      "benefit_dependent:dependent_proof:record,action",
      "benefit_election:election_document:record,action",
      "benefit_election:provider_proof:record,action",
      "benefit_plan:benefit_booklet:record",
      "hr_evaluation_assessment:evaluation_evidence:record",
      "hr_person:bank_document:record",
      "hr_person:compensation_authorization:record",
      "hr_person:employment_amendment:record",
      "hr_person:employment_contract:record",
      "hr_person:identity_document:record",
      "hr_person:professional_certificate:record",
      "hr_person:tax_document:record",
      "hr_person:work_permit:record",
      "learning_assignment:certificate:record,action",
      "learning_course:course_material:record",
      "payroll_advance:advance_support:record",
      "payroll_final_pay:final_pay_support:record",
      "payroll_run:correction_support:action",
      "people_clearance_task:clearance_evidence:action",
      "people_lifecycle_case:offer_document:record",
      "people_lifecycle_case:offboarding_document:record",
      "people_lifecycle_case:transfer_document:record",
      "talent_candidate:background_document:record",
      "talent_candidate:cv:record",
      "talent_candidate:portfolio:record",
      "talent_candidate:work_sample:record",
      "talent_offer:offer_document:record",
    ].sort(),
  );

  for (const purpose of ["evaluation", "investigation", "internal_hr_memo"])
    expect(() => attachmentPolicy("hr_person", purpose, "record")).toThrow(
      "This file purpose is not registered",
    );
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
