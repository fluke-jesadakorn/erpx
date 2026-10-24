import { beforeAll, beforeEach, expect, it, vi } from "vitest";
import { env, applyMigrations } from "../../support/runtime-env";
import { seedExpenseCompany } from "../../../fixtures/expenses/company";
import { loadPeopleMove, validatePeopleMoveTarget } from "@erpx/server-people_payroll/public/people-move";
import {installStaffLevels} from '@erpx/server-organization/public/organization';
import { createHash } from "node:crypto";
import { placementBatchInput, previewPlacementBatch, submitPlacementBatch } from "@erpx/server-people_payroll/public/people-placement-batch";

vi.mock("@erpx/server-runtime/adapters/runtime", () => ({
  holdingBindingBySlug: async (slug: string) => {
    if (slug !== "test") throw new Error("HOLDING_NOT_FOUND");
    return { id: "h001", slug, displayName: "Test Holding" };
  },
  holdingResources: async (holdingId: string) => {
    if (holdingId !== "h001") throw new Error("HOLDING_NOT_FOUND");
    return { write: env.H001_DB, files: env.H001_FILES };
  },
}));

const db = env.H001_DB;
let first: Awaited<ReturnType<typeof seedExpenseCompany>>;
let second: Awaited<ReturnType<typeof seedExpenseCompany>>;
let currentPosition: string;
let targetPosition: string;
let otherBranch: string;
let headquarters: string;

beforeAll(async () => { await applyMigrations(db, env.STAFF_TEST_MIGRATIONS); });
beforeEach(async () => {
  first = await seedExpenseCompany(db, env.H001_FILES);
  second = await seedExpenseCompany(db, env.H001_FILES);
  await installStaffLevels(db, first.owner, `move-levels-${crypto.randomUUID()}`);
  currentPosition = `${first.company}-current-position`;
  targetPosition = `${first.company}-target-position`;
  otherBranch = `${first.company}-other-branch`;
  headquarters = `${first.company}-headquarters`;
  await db.batch([
    db.prepare(`INSERT INTO organization_positions
      (id,company_id,branch_id,department_id,title,level_id,created_by,created_at,updated_at)
      VALUES(?,?,?,?,?,?,?,1,1)`)
      .bind(currentPosition, first.company, first.branch, first.department,
        "Current role", `${first.company}-level-1`, "fixture"),
    db.prepare(`INSERT INTO organization_positions
      (id,company_id,branch_id,department_id,title,level_id,created_by,created_at,updated_at)
      VALUES(?,?,?,?,?,?,?,1,1)`)
      .bind(targetPosition, first.company, first.branch, first.accountsDepartment,
        "Destination role", `${first.company}-level-5`, "fixture"),
    db.prepare(`INSERT INTO organization_nodes
      (id,kind,slug,display_name,parent_id,company_id,branch_id)
      VALUES(?,'branch',?,'Other branch',?,?,?)`)
      .bind(otherBranch, otherBranch, first.company, first.company, otherBranch),
    db.prepare(`INSERT INTO organization_nodes
      (id,kind,slug,display_name,parent_id,company_id,branch_id)
      VALUES(?,'headquarters',?,'Headquarters',?,?,?)`)
      .bind(headquarters, headquarters, first.company, first.company, headquarters),
  ]);
  await db.prepare("UPDATE hr_engagements SET position_id=? WHERE id=?")
    .bind(currentPosition, `eng-${first.claimant.userId}`).run();
  await db.prepare("UPDATE organization_positions SET level_id=? WHERE id=?")
    .bind(`org-level-${first.company}-5`, targetPosition).run();
  await db.prepare("UPDATE hr_engagements SET level_id=? WHERE id=?")
    .bind(`org-level-${first.company}-1`, `eng-${first.head.userId}`).run();
});

it("allows Company HR to choose active same-company destinations without expanding a tree", async () => {
  const person = await loadPeopleMove(first.owner, "person", `eng-${first.claimant.userId}`);
  expect(person.source).toMatchObject({ kind: "person", label: "claimant", companyId: first.company });
  expect(validatePeopleMoveTarget(person, "position", targetPosition)).toMatchObject({ valid: true, targetName: "Destination role" });
  expect(validatePeopleMoveTarget(person, "position", currentPosition)).toMatchObject({ valid: false });

  const position = await loadPeopleMove(first.owner, "position", currentPosition);
  expect(validatePeopleMoveTarget(position, "department", first.accountsDepartment).valid).toBe(true);
  const targetLevel = position.directory.levels.find(item => item.companyId === first.company && item.number === 5);
  expect(targetLevel).toBeTruthy();
  expect(validatePeopleMoveTarget(position, "level", targetLevel!.id).valid).toBe(true);
  expect(validatePeopleMoveTarget(position, "level", `${first.company}-level-5`).valid).toBe(false);
  expect(validatePeopleMoveTarget(position, "department", first.department).valid).toBe(false);
  expect(validatePeopleMoveTarget(position, "branch", otherBranch).valid).toBe(false);

  const department = await loadPeopleMove(first.owner, "department", first.department);
  expect(validatePeopleMoveTarget(department, "branch", otherBranch).valid).toBe(true);
  expect(validatePeopleMoveTarget(department, "branch", first.branch).valid).toBe(false);
  expect(validatePeopleMoveTarget(department, "branch", headquarters).valid).toBe(false);
});

it("rejects an ordinary actor, another Company's source, and cross-Company targets", async () => {
  await expect(loadPeopleMove(first.claimant, "person", `eng-${first.claimant.userId}`))
    .rejects.toMatchObject({ status: 403, code: "PERMISSION_REQUIRED" });
  await expect(loadPeopleMove(first.owner, "person", `eng-${second.claimant.userId}`))
    .rejects.toMatchObject({ status: 404, code: "MOVE_SOURCE_NOT_FOUND" });
  const foreignPosition = `${second.company}-position`;
  await db.prepare(`INSERT INTO organization_positions
    (id,company_id,branch_id,department_id,title,level_id,created_by,created_at,updated_at)
    VALUES(?,?,?,?,?,?,?,1,1)`)
    .bind(foreignPosition, second.company, second.branch, second.department,
      "Other Company role", `${second.company}-level-1`, "fixture").run();
  const context = await loadPeopleMove(first.owner, "person", `eng-${first.claimant.userId}`);
  expect(context.directory.positions.some(item => item.id === foreignPosition)).toBe(true);
  expect(validatePeopleMoveTarget(context, "position", foreignPosition).valid).toBe(false);
  await expect(loadPeopleMove(first.owner, "person", "missing-engagement"))
    .rejects.toMatchObject({ status: 404, code: "MOVE_SOURCE_NOT_FOUND" });
});

it("schedules different destinations atomically and replays the batch without duplicates", async () => {
  const secondTarget = `${first.company}-second-target`;
  await db.prepare("INSERT INTO organization_positions(id,company_id,branch_id,department_id,title,level_id,created_by,created_at,updated_at) VALUES(?,?,?,?,?,?,?,1,1)")
    .bind(secondTarget, first.company, first.branch, first.accountsDepartment, "Second destination", `org-level-${first.company}-5`, "fixture").run();
  const input = placementBatchInput.parse({
    effectiveAt: Date.now() + 60000, timezone: "Asia/Bangkok", reason: "Reassign both employees",
    items: [
      { engagementId: `eng-${first.claimant.userId}`, positionId: targetPosition, managerPersonId: `person-${first.head.userId}`, acceptRecommendedLeave: false },
      { engagementId: `eng-${first.reviewer.userId}`, positionId: secondTarget, managerPersonId: `person-${first.head.userId}`, acceptRecommendedLeave: false },
    ],
  });
  const preview = await previewPlacementBatch(db, first.owner, input);
  expect(preview.ready, JSON.stringify(preview.items)).toBe(true);
  expect(preview.items.map(item => item.ready)).toEqual([true, true]);
  const command = `batch-${crypto.randomUUID()}`;
  const saved = await submitPlacementBatch(db, first.owner, input, preview.token!, command);
  expect(saved.count).toBe(2);
  expect(await submitPlacementBatch(db, first.owner, input, preview.token!, command)).toEqual(saved);
  const cases = (await db.prepare("SELECT id,status,definition FROM people_lifecycle_cases WHERE id IN (?,?) ORDER BY id").bind(...saved.ids).all<{id:string;status:string;definition:string}>()).results;
  expect(cases).toHaveLength(2);
  expect(cases.every(row => row.status === "scheduled")).toBe(true);
  expect(cases.map(row => (typeof row.definition === "string" ? JSON.parse(row.definition) : row.definition).acceptRecommendedPay)).toEqual([false, false]);
  await expect(submitPlacementBatch(db, first.owner, { ...input, reason: "Different request" }, preview.token!, command)).rejects.toMatchObject({ code: "IDEMPOTENCY_CONFLICT" });
});

it("rejects a changed batch before writing any placements", async () => {
  const input = placementBatchInput.parse({ effectiveAt: Date.now() + 60000, timezone: "Asia/Bangkok", reason: "Move employees together", items: [
    { engagementId: `eng-${first.claimant.userId}`, positionId: targetPosition, managerPersonId: `person-${first.head.userId}`, acceptRecommendedLeave: false },
    { engagementId: `eng-${first.reviewer.userId}`, positionId: targetPosition, managerPersonId: `person-${first.head.userId}`, acceptRecommendedLeave: false },
  ] });
  const preview = await previewPlacementBatch(db, first.owner, input);
  expect(preview.ready, JSON.stringify(preview.items)).toBe(true);
  await db.prepare("UPDATE hr_engagements SET position_id=?,version=version+1 WHERE id=?").bind(targetPosition, input.items[1].engagementId).run();
  const command = `batch-${crypto.randomUUID()}`;
  await expect(submitPlacementBatch(db, first.owner, input, preview.token!, command)).rejects.toMatchObject({ code: "BATCH_PREVIEW_CHANGED" });
  expect(await db.prepare("SELECT count(*) n FROM people_lifecycle_cases WHERE engagement_id IN (?,?)").bind(...input.items.map(item => item.engagementId)).first<number>("n")).toBe(0);
  expect(await db.prepare("SELECT count(*) n FROM command_receipts WHERE command_id=?").bind(command).first<number>("n")).toBe(0);
});

it("rolls back an earlier employee when a later command conflicts", async () => {
  const input = placementBatchInput.parse({ effectiveAt: Date.now() + 60000, timezone: "Asia/Bangkok", reason: "Atomic HR batch", items: [
    { engagementId: `eng-${first.claimant.userId}`, positionId: targetPosition, managerPersonId: `person-${first.head.userId}`, acceptRecommendedLeave: false },
    { engagementId: `eng-${first.reviewer.userId}`, positionId: targetPosition, managerPersonId: `person-${first.head.userId}`, acceptRecommendedLeave: false },
  ] });
  const preview = await previewPlacementBatch(db, first.owner, input);
  expect(preview.ready, JSON.stringify(preview.items)).toBe(true);
  const command = `batch-${crypto.randomUUID()}`;
  const childKey = `placement-batch:${createHash("sha256").update(command).digest("hex").slice(0, 32)}:1`;
  await db.prepare("INSERT INTO command_receipts(command_id,operation,actor_id,request_hash,record_id,result_json,created_at,authority_json) VALUES(?,'fixture.conflict',?,'different','fixture','{}',1,'{}')")
    .bind(childKey, first.owner.userId).run();
  await expect(submitPlacementBatch(db, first.owner, input, preview.token!, command)).rejects.toMatchObject({ code: "IDEMPOTENCY_CONFLICT" });
  expect(await db.prepare("SELECT count(*) n FROM people_lifecycle_cases WHERE engagement_id IN (?,?)").bind(...input.items.map(item => item.engagementId)).first<number>("n")).toBe(0);
  const firstChild = `placement-batch:${createHash("sha256").update(command).digest("hex").slice(0, 32)}:0`;
  expect(await db.prepare("SELECT count(*) n FROM command_receipts WHERE command_id IN (?,?)").bind(command, firstChild).first<number>("n")).toBe(0);
  expect(await db.prepare("SELECT count(*) n FROM audit_events WHERE command_id=?").bind(firstChild).first<number>("n")).toBe(0);
  expect(await db.prepare("SELECT count(*) n FROM outbox_events WHERE command_id=?").bind(firstChild).first<number>("n")).toBe(0);
});

it("keeps batch previews within one Company and rejects duplicate or oversized trays", async () => {
  const employee = { engagementId: `eng-${first.claimant.userId}`, positionId: targetPosition, managerPersonId: `person-${first.head.userId}`, acceptRecommendedLeave: false };
  const baseInput = { effectiveAt: Date.now() + 60000, timezone: "Asia/Bangkok", reason: "Review Company scope" };
  expect(placementBatchInput.safeParse({ ...baseInput, items: [employee, employee] }).success).toBe(false);
  expect(placementBatchInput.safeParse({ ...baseInput, items: Array.from({ length: 21 }, (_, index) => ({ ...employee, engagementId: `${employee.engagementId}-${index}` })) }).success).toBe(false);
  const crossCompany = placementBatchInput.parse({ ...baseInput, items: [{ ...employee, engagementId: `eng-${second.claimant.userId}` }] });
  const preview = await previewPlacementBatch(db, first.owner, crossCompany);
  expect(preview.ready).toBe(false);
  expect(preview.items[0].blockers).toContain("This employee is no longer current in this Company.");
  await expect(previewPlacementBatch(db, first.claimant, placementBatchInput.parse({ ...baseInput, items: [employee] }))).rejects.toMatchObject({ status: 403 });
});
