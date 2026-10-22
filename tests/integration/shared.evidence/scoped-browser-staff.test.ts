import { beforeAll, expect, it, vi } from "vitest";
import { applyMigrations, env } from "../../support/runtime-env";
import { seedExpenseCompany } from "../../../fixtures/expenses/company";
import { mapSpendingOrganization } from "../../support/helpers/spending-organization";
import { placeScopedBrowserEmployee } from "../../../fixtures/people_payroll/builders/scoped-browser-staff";
import { companyBusinessAcceptance } from "@erpx/server-organization/public/company-business-acceptance";
import { directoryPerson } from "@erpx/server-people_payroll/public/people-directory";

vi.mock("@erpx/server-runtime/adapters/runtime", () => ({ holdingResources: async () => ({ write: env.H001_DB, files: env.H001_FILES }), runtimeEnv: async () => env }));
const db = env.H001_DB;
beforeAll(async () => { await applyMigrations(db, env.STAFF_TEST_MIGRATIONS); });

it("places late-added scoped demo staff through native HR history before Company setup can be accepted", async () => {
  const f = await seedExpenseCompany(db, env.H001_FILES);
  await mapSpendingOrganization(f);
  const branch = await f.member("uat-branch"), department = await f.member("uat-department");
  for (const employee of [branch, department]) await db.prepare("UPDATE hr_people SET email=? WHERE company_id=? AND user_id=?").bind(employee.email, f.company, employee.userId).run();
  const branchAssignment = await f.grant(branch, "employee", "branch", f.branch), departmentAssignment = await f.grant(department, "employee", "department", f.department);
  const before = await companyBusinessAcceptance(db, f.owner);
  expect(before.canSubmit).toBe(false);
  expect(before.blockers).toEqual(["2 active staff record(s) need a level or position."]);
  const grants = (await db.prepare("SELECT id,role,scope_kind,scope_node_id FROM assignments WHERE company_id=? ORDER BY id").bind(f.company).all()).results;
  for (const input of [
    { userId: branch.userId, assignmentId: branchAssignment, scope: "branch" as const, nodeId: f.branch },
    { userId: department.userId, assignmentId: departmentAssignment, scope: "department" as const, nodeId: f.department, parentNodeId: f.branch },
  ]) {
    const placed = await placeScopedBrowserEmployee(f, input), current = await directoryPerson(f.owner, placed.personId, placed.engagementId);
    expect(current).toMatchObject({ position_id: placed.positionId, level_id: `org-level-${f.company}-6`, branch_id: f.branch, department_id: input.scope === "branch" ? null : f.department, manager_person_id: `person-${f.owner.userId}`, version: 2, engagement_version: 2, engagement_status: "active" });
    const history = (await db.prepare("SELECT before_json,after_json,command_id,actor_id FROM staff_organization_history WHERE engagement_id=?").bind(placed.engagementId).all<{ before_json: string; after_json: string; command_id: string; actor_id: string }>()).results;
    expect(history).toHaveLength(1);
    expect(JSON.parse(history[0].before_json).positionId).toBeNull();
    expect(JSON.parse(history[0].after_json)).toMatchObject({ positionId: placed.positionId, levelId: placed.levelId, branchId: f.branch, departmentId: input.scope === "branch" ? null : f.department });
    expect(history[0].actor_id).toBe(f.owner.userId);
    expect(await db.prepare("SELECT operation FROM command_receipts WHERE command_id=?").bind(history[0].command_id).first("operation")).toBe("people.directory.update");
  }
  const ready = await companyBusinessAcceptance(db, f.owner);
  expect(ready.canSubmit).toBe(true);
  expect(ready.checks.find(check => check.id === "staff-ready")).toMatchObject({ ready: true, detail: "All active staff have a level and position." });
  expect((await db.prepare("SELECT id,role,scope_kind,scope_node_id FROM assignments WHERE company_id=? ORDER BY id").bind(f.company).all()).results).toEqual(grants);
});

it("rejects mismatched Company scope and revoked demo assignments before any native placement is created", async () => {
  const f = await seedExpenseCompany(db, env.H001_FILES);
  await mapSpendingOrganization(f);
  const employee = await f.member("uat-department"), assignmentId = await f.grant(employee, "employee", "department", f.department);
  const input = { userId: employee.userId, assignmentId, scope: "department" as const, nodeId: f.department, parentNodeId: f.branch };
  const positions = await db.prepare("SELECT COUNT(*) n FROM organization_positions WHERE company_id=?").bind(f.company).first("n");
  await expect(placeScopedBrowserEmployee(f, { ...input, nodeId: f.accountsDepartment })).rejects.toThrow("EXACT_SCOPED_BROWSER_EMPLOYEE_REQUIRED");
  await expect(placeScopedBrowserEmployee(f, { ...input, parentNodeId: "foreign-branch" })).rejects.toThrow("EXACT_SCOPED_BROWSER_PLACEMENT_REQUIRED");
  await db.prepare("UPDATE assignments SET revoked_at=? WHERE id=?").bind(Date.now(), assignmentId).run();
  await expect(placeScopedBrowserEmployee(f, input)).rejects.toThrow("EXACT_SCOPED_BROWSER_EMPLOYEE_REQUIRED");
  expect(await db.prepare("SELECT COUNT(*) n FROM organization_positions WHERE company_id=?").bind(f.company).first("n")).toBe(positions);
  expect((await directoryPerson(f.owner, `person-${employee.userId}`, `eng-${employee.userId}`)).position_id).toBeNull();
  expect(await db.prepare("SELECT COUNT(*) n FROM staff_organization_history WHERE engagement_id=?").bind(`eng-${employee.userId}`).first("n")).toBe(0);
});
