import { describe, expect, it } from "vitest";
import {
  parseHoldingWorkspace,
  workspaceApiHref,
  workspaceBase,
  workspaceHref,
} from "../src/lib/workspace";
import {
  assignmentApplies,
  isRoleValidAtScope,
} from "../src/lib/workspace-access";
import type {
  ResolvedWorkspaceScope,
  WorkAssignment,
} from "../src/server/authorization";

const branch: Exclude<ResolvedWorkspaceScope, { kind: "platform" }> = {
  kind: "branch",
  holdingId: "h001",
  holdingSlug: "acme",
  holdingName: "Acme Group",
  companyId: "acme-co",
  branchId: "bangkok",
  branchSlug: "bangkok",
  branchName: "Bangkok Branch",
};

const department: Exclude<ResolvedWorkspaceScope, { kind: "platform" }> = {
  kind: "department",
  holdingId: "h001",
  holdingSlug: "acme",
  holdingName: "Acme Group",
  companyId: "acme-co",
  branchId: "bangkok",
  branchSlug: "bangkok",
  branchName: "Bangkok Branch",
  departmentId: "dept:bangkok:people",
  departmentSlug: "people",
  departmentName: "People",
};

function assignment(
  scopeKind: WorkAssignment["scopeKind"],
  scopeNodeId: string | null,
): WorkAssignment {
  return {
    id: `${scopeKind}:${scopeNodeId}`,
    role: scopeKind === "holding" ? "people_manager" : "employee",
    companyId: "acme-co",
    branchId: scopeKind === "branch" ? scopeNodeId : null,
    modules: ["people_payroll"],
    scopeKind,
    scopeNodeId,
  };
}

describe("canonical workspace routing", () => {
  it("builds all public and API workspace bases from immutable slugs", () => {
    expect(workspaceBase({ kind: "platform" })).toBe("/platform");
    expect(workspaceHref({ kind: "holding", holdingSlug: "acme" }, "work")).toBe("/holdings/acme/work");
    expect(workspaceBase({ kind: "branch", holdingSlug: "acme", branchSlug: "bangkok" })).toBe("/holdings/acme/branches/bangkok");
    expect(workspaceBase({ kind: "department", holdingSlug: "acme", departmentSlug: "finance" })).toBe("/holdings/acme/departments/finance");
    expect(workspaceApiHref({ kind: "department", holdingSlug: "acme", branchSlug: "bangkok", departmentSlug: "people" }, "feed")).toBe("/api/v1/holdings/acme/branches/bangkok/departments/people/feed");
  });

  it("parses Branch and Department placement without adding a role level", () => {
    expect(parseHoldingWorkspace("acme", ["branches", "bangkok", "departments", "people", "home"]))
      .toEqual({ scope: { kind: "department", holdingSlug: "acme", branchSlug: "bangkok", departmentSlug: "people" }, destination: ["home"] });
  });
});

describe("assignment containment", () => {
  it("inherits Holding and Branch assignments but denies Department siblings", () => {
    expect(assignmentApplies(assignment("holding", null), department)).toBe(true);
    expect(assignmentApplies(assignment("branch", "bangkok"), department)).toBe(true);
    expect(assignmentApplies(assignment("branch", "chiang-mai"), department)).toBe(false);
    expect(assignmentApplies(assignment("department", "dept:bangkok:people"), department)).toBe(true);
    expect(assignmentApplies(assignment("department", "dept:bangkok:finance"), department)).toBe(false);
    expect(assignmentApplies(assignment("department", "dept:bangkok:people"), branch)).toBe(false);
  });

  it("keeps Platform roles separate and holding_owner Holding-only", () => {
    expect(isRoleValidAtScope("platform_admin", "platform")).toBe(true);
    expect(isRoleValidAtScope("platform_admin", "holding")).toBe(false);
    expect(isRoleValidAtScope("holding_owner", "holding")).toBe(true);
    expect(isRoleValidAtScope("holding_owner", "branch")).toBe(false);
  });
  it('recognizes the existing POS staff roles at their published workspace scopes',()=>{
    for(const role of ['pos_cashier','pos_server','pos_kitchen','pos_manager']){
      expect(isRoleValidAtScope(role,'holding')).toBe(true);
      expect(isRoleValidAtScope(role,'branch')).toBe(true);
      expect(isRoleValidAtScope(role,'department')).toBe(false);
      expect(isRoleValidAtScope(role,'platform')).toBe(false);
    }
  });
});
