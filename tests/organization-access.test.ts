import { beforeAll, beforeEach, expect, it, vi } from "vitest";
import { env, applyMigrations } from "./runtime-env";
import { expenseFixture, expenseDb as db, key } from "./helpers/expense-fixture";
import { organizationAccessData } from "../src/server/organization-access";
import { installStaffLevels, positionSchema, savePosition } from "../src/server/organization";
import type { WorkContext } from "../src/server/authorization";

vi.mock("../src/server/runtime", () => ({ holdingResources: async () => ({ write: db, files: env.H001_FILES }), runtimeEnv: async () => env }));
let fixture: Awaited<ReturnType<typeof expenseFixture>>;
let hr: WorkContext;
let positionId: string;
beforeAll(async () => { await applyMigrations(db, env.STAFF_TEST_MIGRATIONS); });
beforeEach(async () => {
  fixture = await expenseFixture(false);
  hr = await fixture.member("organization-access-hr", "hr_admin");
  await installStaffLevels(db, hr, key());
  positionId = String((await savePosition(db, hr, null, positionSchema.parse({
    title: "Company leadership", branchId: null, departmentId: null,
    levelId: `org-level-${fixture.company}-1`, reportsToPositionId: null,
    isCeo: true, expectedVersion: 0, reason: "Prepare organization explorer",
  }), key())).id);
});

it("lets IT inspect placement without exposing compensation or granting HR commands", async () => {
  const data = await organizationAccessData(db, fixture.it);
  expect(data.positions.some(position => position.id === positionId)).toBe(true);
  expect(data.people.length).toBeGreaterThan(0);
  expect(data.canManage).toBe(false);
  expect(data.canMovePeople).toBe(false);
  expect(data).not.toHaveProperty("defaults");
  expect(data).not.toHaveProperty("monthlyPayGroups");
  expect(data.people.every(person => !("currency" in person) && !("base_amount_minor" in person))).toBe(true);
});

it("exposes Company HR commands and the six ordered Level targets", async () => {
  const data = await organizationAccessData(db, hr);
  expect(data.canManage).toBe(true);
  expect(data.canMovePeople).toBe(true);
  expect(data.levels.map(level => level.level_number)).toEqual([1, 2, 3, 4, 5, 6]);
});

it("restricts a local explorer to its placement and offers no Company move command", async () => {
  const actor = { ...hr, branchId: fixture.branch, departmentId: fixture.department,
    assignmentScope: "department", scope: { ...hr.scope, kind: "department" } } as WorkContext;
  const data = await organizationAccessData(db, actor);
  expect(data.canManage).toBe(false);
  expect(data.canMovePeople).toBe(false);
  expect(data.people.every(person => person.department_id === fixture.department)).toBe(true);
  expect(data.positions.every(position => position.department_id === fixture.department)).toBe(true);
  expect(data.nodes.filter(node => node.kind === "department").map(node => node.id)).toEqual([fixture.department]);
  expect(data.positions.some(position => position.id === positionId)).toBe(false);
});

it("does not leak another Company's records", async () => {
  const data = await organizationAccessData(db, { ...hr, companyId: `missing-${key()}` });
  expect(data.people).toEqual([]);
  expect(data.positions).toEqual([]);
  expect(data.nodes).toEqual([]);
});
