import { describe, expect, test } from "vitest";
import { organizationMoveDenial } from "../src/components/people/organization-explorer";
import type { HoldingPeopleDirectory } from "../src/server/holding-people-directory";

const directory: HoldingPeopleDirectory = {
  companies: [
    { id: "a", slug: "a", name: "Company A" },
    { id: "b", slug: "b", name: "Company B" },
  ],
  nodes: [
    { id: "a-branch-1", companyId: "a", kind: "branch", slug: "one", name: "One", parentId: null, branchId: null },
    { id: "a-branch-2", companyId: "a", kind: "branch", slug: "two", name: "Two", parentId: null, branchId: null },
    { id: "a-headquarters", companyId: "a", kind: "headquarters", slug: "hq", name: "Headquarters", parentId: null, branchId: null },
    { id: "a-dept", companyId: "a", kind: "department", slug: "dept", name: "Department", parentId: "a-branch-1", branchId: "a-branch-1" },
    { id: "a-dept-empty", companyId: "a", kind: "department", slug: "empty", name: "Empty department", parentId: "a-branch-1", branchId: "a-branch-1" },
    { id: "a-company-dept", companyId: "a", kind: "department", slug: "company", name: "Company department", parentId: "a", branchId: null },
    { id: "a-team", companyId: "a", kind: "team", slug: "team", name: "Team", parentId: "a-dept", branchId: "a-branch-1" },
    { id: "b-dept", companyId: "b", kind: "department", slug: "dept", name: "Department", parentId: null, branchId: null },
  ],
  levels: [
    { id: "a-level-1", companyId: "a", code: "L1", name: "Senior", number: 1 },
    { id: "a-level-2", companyId: "a", code: "L2", name: "Staff", number: 2 },
    { id: "a-level-7", companyId: "a", code: "L7", name: "Unsupported", number: 7 },
  ],
  positions: [
    { id: "a-position-1", companyId: "a", branchId: "a-branch-1", departmentId: "a-dept", title: "Analyst", levelId: "a-level-2", reportsToPositionId: null, isCeo: false },
    { id: "a-position-2", companyId: "a", branchId: "a-branch-1", departmentId: "a-dept", title: "Manager", levelId: "a-level-1", reportsToPositionId: null, isCeo: false },
    { id: "b-position", companyId: "b", branchId: null, departmentId: "b-dept", title: "Manager", levelId: "b-level", reportsToPositionId: null, isCeo: false },
  ],
  people: [
    { id: "engagement-1", personId: "person-1", engagementId: "engagement-1", companyId: "a", branchId: "a-branch-1", departmentId: "a-dept", positionId: "a-position-1", legalName: "One", jobTitle: "Analyst", workEmail: null, workPhone: null },
    { id: "engagement-2", personId: "person-2", engagementId: "engagement-2", companyId: "a", branchId: "a-branch-1", departmentId: "a-dept", positionId: "a-position-2", legalName: "Two", jobTitle: "Manager", workEmail: null, workPhone: null },
  ],
};

describe("organization move review targets", () => {
  const mayManageA = (id: string) => id === "a";

  test("allows another person to join an occupied position", () => {
    expect(organizationMoveDenial(directory, { kind: "person", id: "engagement-1" }, { kind: "position", id: "a-position-2" }, mayManageA)).toBeNull();
  });

  test("rejects same position, cross-company moves and missing scoped permission", () => {
    expect(organizationMoveDenial(directory, { kind: "person", id: "engagement-1" }, { kind: "position", id: "a-position-1" }, mayManageA)).toContain("already holds");
    expect(organizationMoveDenial(directory, { kind: "person", id: "engagement-1" }, { kind: "position", id: "b-position" }, mayManageA)).toContain("between companies");
    expect(organizationMoveDenial(directory, { kind: "person", id: "engagement-1" }, { kind: "position", id: "a-position-2" }, () => false)).toContain("cannot change");
  });

  test("supports position department/level and department branch moves only", () => {
    expect(organizationMoveDenial(directory, { kind: "position", id: "a-position-1" }, { kind: "level", id: "a-level-1" }, mayManageA)).toBeNull();
    expect(organizationMoveDenial(directory, { kind: "position", id: "a-position-1" }, { kind: "level", id: "a-level-7" }, mayManageA)).toContain("six active");
    expect(organizationMoveDenial(directory, { kind: "position", id: "a-position-1" }, { kind: "department", id: "a-dept" }, mayManageA)).toContain("already in");
    expect(organizationMoveDenial(directory, { kind: "position", id: "a-position-1" }, { kind: "department", id: "a-company-dept" }, mayManageA)).toContain("active branch");
    expect(organizationMoveDenial(directory, { kind: "department", id: "a-dept-empty" }, { kind: "branch", id: "a-branch-2" }, mayManageA)).toBeNull();
    expect(organizationMoveDenial(directory, { kind: "department", id: "a-dept" }, { kind: "branch", id: "a-branch-2" }, mayManageA)).toContain("child departments");
    expect(organizationMoveDenial(directory, { kind: "department", id: "a-dept" }, { kind: "branch", id: "a-headquarters" }, mayManageA)).toContain("active branch");
    expect(organizationMoveDenial(directory, { kind: "department", id: "a-team" }, { kind: "branch", id: "a-branch-2" }, mayManageA)).toContain("no longer available");
  });

  test("rejects a department URL collision at the destination branch", () => {
    const conflicting = {
      ...directory,
      nodes: [...directory.nodes, { id: "a-duplicate", companyId: "a", kind: "department" as const, slug: "empty", name: "Existing department", parentId: "a-branch-2", branchId: "a-branch-2" }],
    };
    expect(organizationMoveDenial(conflicting, { kind: "department", id: "a-dept-empty" }, { kind: "branch", id: "a-branch-2" }, mayManageA)).toContain("URL code");
  });
});
