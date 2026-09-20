import { beforeAll, beforeEach, expect, it, vi } from "vitest";
import { applyMigrations, env } from "./runtime-env";
import {
  expenseDb as db,
  expenseFixture,
  key,
} from "./helpers/expense-fixture";
import {
  changeLearningAssignment,
  changeLearningCourse,
  createLearningAssignment,
  createLearningCourse,
  learningAssignmentDetail,
  learningWork,
} from "../src/server/learning";
vi.mock("../src/server/runtime", () => ({
  holdingResources: async () => ({ write: env.H001_DB, files: env.H001_FILES }),
  runtimeEnv: async () => env,
}));
let f: Awaited<ReturnType<typeof expenseFixture>>;
beforeAll(async () => {
  await applyMigrations(db, env.STAFF_TEST_MIGRATIONS);
});
beforeEach(async () => {
  f = await expenseFixture();
  for (const a of [f.owner, f.tie, f.claimant, f.head])
    a.modules = ["learning"];
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
    },
    key(),
  );
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
