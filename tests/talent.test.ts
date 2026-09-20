import { beforeAll, beforeEach, expect, it, vi } from "vitest";
import { applyMigrations, env } from "./runtime-env";
import {
  expenseDb as db,
  expenseFixture,
  key,
} from "./helpers/expense-fixture";
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
} from "../src/server/talent";
import type { WorkContext } from "../src/server/authorization";
vi.mock("../src/server/runtime", () => ({
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
  const { app } = await application();
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
