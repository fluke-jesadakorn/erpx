import { expect, it } from "vitest";
import { demoStaffPlan, demoDepartments, demoInitialPassword } from "../../../fixtures/organization/builders/acme-organization";
import { isRoleValidAtScope } from "@erpx/server-identity_access/public/rules/workspace-access";

const ceo = { personId: "existing-ceo", positionId: "existing-ceo-position" };
const plan = demoStaffPlan(ceo);
it("adds only 92 unique Acme identities and covers every department in both branches", () => {
  expect(plan).toHaveLength(92);
  for (const field of ["userId", "personId", "engagementId", "email", "name"] as const) expect(new Set(plan.map(p => p[field])).size).toBe(92);
  expect(plan.every(p => ["bangkok", "new-york"].includes(p.branchId))).toBe(true);
  for (const branch of ["bangkok", "new-york"]) for (const department of demoDepartments) {
    const team = plan.filter(p => p.departmentId === `dept:${branch}:${department.id}`);
    expect(team.map(p => p.level)).toEqual(department.id === "operations" ? [2, 3, 4, 5, 6, 6] : [3, 4, 5, 6, 6]);
  }
});
it("connects every new position and staff member to a strictly senior manager and the existing CEO", () => {
  for (const person of plan) {
    let current = person;
    const seen = new Set<string>();
    while (current.managerPersonId !== ceo.personId) {
      expect(seen.has(current.personId)).toBe(false); seen.add(current.personId);
      const manager = plan.find(p => p.personId === current.managerPersonId)!;
      expect(manager).toBeDefined(); expect(manager.level).toBeLessThan(current.level);
      expect(current.reportsToPositionId).toBe(manager.positionId);
      current = manager;
    }
    expect(current.level).toBe(2); expect(current.reportsToPositionId).toBe(ceo.positionId);
  }
});
it("assigns valid business scopes and keeps Accounting preparation, approval and Treasury independent", () => {
  for (const person of plan) {
    expect(demoInitialPassword(person)).not.toContain("undefined");
    expect(person.grants.some(g => g.role === "employee")).toBe(true);
    for (const grant of person.grants) expect(isRoleValidAtScope(grant.role, grant.scope)).toBe(true);
    const financial = person.grants.filter(g => ["finance_preparer", "finance_approver", "treasury"].includes(g.role));
    expect(financial.length).toBeLessThanOrEqual(1);
    if (financial.some(g => g.role === "finance_approver")) expect(person.level).toBeLessThanOrEqual(4);
    expect(person.grants.some(g => String(g.role).includes("owner") || String(g.role).startsWith("platform"))).toBe(false);
  }
  expect(plan.filter(p => p.grants.some(g => g.role === "finance_approver"))).toHaveLength(4);
  expect(plan.filter(p => p.grants.some(g => g.role === "finance_preparer"))).toHaveLength(4);
  expect(plan.filter(p => p.grants.some(g => g.role === "treasury"))).toHaveLength(8);
});
