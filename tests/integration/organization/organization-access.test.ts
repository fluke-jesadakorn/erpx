import { beforeAll, beforeEach, expect, it, vi } from "vitest";
import { env, applyMigrations } from "../../support/runtime-env";
import { expenseFixture, expenseDb as db, key } from "../../support/helpers/expense-fixture";
import { organizationAccessData } from "@erpx/server-organization/public/organization-access";
import { getPerson, listPeople } from "@erpx/server-people_payroll/public/people-payroll";
import { installStaffLevels, positionSchema, savePosition } from "@erpx/server-organization/public/organization";
import type { WorkContext } from "@erpx/server-identity_access/public/authorization";

vi.mock("@erpx/server-runtime/adapters/runtime", () => ({ holdingResources: async () => ({ write: db, files: env.H001_FILES }), runtimeEnv: async () => env }));
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
  const privateEmail = `private-${key()}@example.test`;
  await db.prepare("UPDATE hr_people SET email=? WHERE id=(SELECT id FROM hr_people WHERE company_id=? LIMIT 1)")
    .bind(privateEmail, fixture.company).run();
  const data = await organizationAccessData(db, fixture.it);
  expect(data.positions.some(position => position.id === positionId)).toBe(true);
  expect(data.people.length).toBeGreaterThan(0);
  expect(data.canManage).toBe(false);
  expect(data.canMovePeople).toBe(false);
  expect(data).not.toHaveProperty("defaults");
  expect(data).not.toHaveProperty("monthlyPayGroups");
  expect(data.people.every(person => !("currency" in person) && !("base_amount_minor" in person))).toBe(true);
  expect(data.people.every(person => person.email === null && person.start_date === "" && person.end_date === null)).toBe(true);
  expect(JSON.stringify(data)).not.toContain(privateEmail);
  expect((await organizationAccessData(db, hr)).people.some(person => person.email === privateEmail)).toBe(true);
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

it("keeps the richer IT organization projection away from ordinary directory readers", async () => {
  await expect(organizationAccessData(db, fixture.claimant)).rejects.toMatchObject({ status: 403 });
});

it("keeps legacy HR list and private person APIs behind HR or self access", async () => {
  await expect(listPeople(fixture.claimant)).rejects.toMatchObject({ status: 403 });
  const other = await db.prepare("SELECT id FROM hr_people WHERE company_id=? AND user_id IS DISTINCT FROM ? LIMIT 1")
    .bind(fixture.company, fixture.claimant.userId).first<{id:string}>();
  expect(other).toBeTruthy();
  await expect(getPerson(fixture.claimant, other!.id)).rejects.toMatchObject({ status: 403 });
});
