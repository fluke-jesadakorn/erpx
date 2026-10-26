import { afterEach, describe, expect, test, vi } from "vitest";
import { createElement } from "react";
import { renderToStaticMarkup } from "react-dom/server";
import { PeopleStructureWorkspace } from "@erpx/feature-people_payroll/components/people-structure-workspace";
import type { PeopleOrganizationProps } from "@erpx/feature-people_payroll/components/people-organization-contracts";
import { emptyDraft } from "@erpx/feature-organization/lib/organization-draft";
import { organizationMoveDenial } from "@erpx/feature-people_payroll/components/organization-explorer";
import type { HoldingPeopleDirectory } from "@erpx/server-organization/public/holding-people-directory";
import { organizationUnitOverview, organizationLocationPath } from "@erpx/feature-organization/lib/organization-presentation";

const navigation = vi.hoisted(() => ({query: ""}));
vi.mock("@erpx/feature-shared/lib/destination-search", () => ({useSearchParams: () => new URLSearchParams(navigation.query)}));
vi.mock("@erpx/feature-shared/lib/destination-router", () => ({useRouter: () => ({push: vi.fn(), replace: vi.fn(), refresh: vi.fn()})}));
afterEach(() => {navigation.query = "";});

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

describe("People unit summaries", () => {
  const data: HoldingPeopleDirectory = {
    ...directory,
    positions: [
      ...directory.positions,
      {...directory.positions[0], id: "company-direct", branchId: null, departmentId: null},
      {...directory.positions[0], id: "branch-direct", departmentId: null},
      {...directory.positions[0], id: "team-vacancy", departmentId: "a-team"},
    ],
    people: [
      ...directory.people,
      {...directory.people[0], id: "second-holder", engagementId: "second-holder", personId: "third", positionId: "a-position-1"},
      {...directory.people[0], id: "unassigned", engagementId: "unassigned", personId: "fourth", positionId: null},
    ],
  };

  test("company overview includes empty branches and branchless departments without exposing descendant position rows", () => {
    const summary = organizationUnitOverview(data, {kind: "company", id: "a"});
    expect(summary.children.map(node => node.id)).toEqual(expect.arrayContaining(["a-branch-1", "a-branch-2", "a-headquarters", "a-company-dept"]));
    expect(summary.children).toHaveLength(4);
    expect(summary.departments).toBe(3);
    expect(summary.positions).toHaveLength(5);
    expect(summary.people).toHaveLength(4);
    expect(summary.directPositions.map(position => position.id)).toEqual(["company-direct"]);
    expect(summary.loosePeople.map(person => person.id)).toEqual(["unassigned"]);
  });

  test("branch summary counts each position once, all holders and unassigned people, and includes empty departments", () => {
    const summary = organizationUnitOverview(data, {kind: "branch", id: "a-branch-1"});
    expect(summary.children.map(node => node.id)).toEqual(["a-dept", "a-dept-empty"]);
    expect(summary.positions).toHaveLength(4);
    expect(summary.people).toHaveLength(4);
    expect(summary.departments).toBe(2);
    expect(summary.directPositions.map(position => position.id)).toEqual(["branch-direct"]);
    expect(organizationUnitOverview(data, {kind: "branch", id: "a-branch-2"})).toMatchObject({children: [], positions: [], people: []});
  });

  test("department overview retains nested teams and breadcrumbs to their actual branch", () => {
    const summary = organizationUnitOverview(data, {kind: "department", id: "a-dept"});
    expect(summary.children.map(node => node.id)).toEqual(["a-team"]);
    expect(summary.positions).toHaveLength(3);
    expect(organizationLocationPath(data, {kind: "team", id: "a-team"})).toEqual([
      {kind: "company", id: "a"}, {kind: "branch", id: "a-branch-1"},
      {kind: "department", id: "a-dept"}, {kind: "team", id: "a-team"},
    ]);
  });

  test("archived summaries count historical holders and exclude empty current positions", () => {
    const summary = organizationUnitOverview(data, {kind: "company", id: "a"}, true);
    expect(summary.positions.map(position => position.id)).toEqual(["a-position-1", "a-position-2"]);
    expect(summary.people).toHaveLength(4);
    expect(summary.directPositions).toEqual([]);
    expect(summary.loosePeople).toHaveLength(1);
  });

  const props: PeopleOrganizationProps = {
    base: "/holdings/acme", holdingName: "Acme", currentCompanyId: "a", actorId: "reader",
    data, archivedPeople: data.people, accessOrganization: null, organization: null, privatePerson: null,
    groups: [], editorNodes: [], staffOptions: null, lifecycle: null, history: [], branding: null, authority: null,
    canManage: false, canManageStructure: false, canReadPay: false, canApprovePay: false,
  };
  const render = (people = props) => renderToStaticMarkup(createElement(PeopleStructureWorkspace, {people, definition: emptyDraft()}));

  test("initial company view renders compact summaries and collapsed exceptions for read-only users", () => {
    const html = render();
    expect(html).toContain('data-testid="people-unit-summary-a-branch-1"');
    expect(html).toContain('3 branches');
    expect(html).toContain('Company-wide');
    expect(html).toContain('Without a position');
    expect(html).not.toContain('data-testid="people-position-a-position-1"');
    expect(html).not.toContain('data-testid="people-person-unassigned"');
    expect(html).not.toContain('aria-label="Position occupancy"');
    expect(html).not.toContain('Collapse One');
    expect(html).not.toContain('+ Add');
  });

  test("branch links render department summaries, while department links show positions and all holders", () => {
    navigation.query = "unit=branch:a-branch-1";
    const branch = render();
    expect(branch).toContain('data-testid="people-unit-summary-a-dept"');
    expect(branch).toContain('data-testid="people-unit-summary-a-dept-empty"');
    expect(branch).not.toContain('data-testid="people-position-a-position-1"');
    navigation.query = "unit=department:a-dept";
    const department = render();
    for (const id of ["a-position-1", "a-position-2", "team-vacancy"]) expect(department).toContain(`data-testid="people-position-${id}"`);
    expect(department).toContain('data-testid="people-person-second-holder"');
    expect(department).toContain('Vacant position');
    expect(department).toContain('aria-label="Position occupancy"');
    expect(department).toContain('Open team schedule');
  });

  test("search reaches other permitted units and displays origins without changing the selected unit", () => {
    navigation.query = "unit=branch:a-branch-2&q=Analyst";
    const html = render();
    expect(html).toContain('data-testid="people-position-a-position-1"');
    expect(html).toContain('Company A · One');
    expect(html).toContain('Clear search');
    navigation.query = "unit=branch:a-branch-2";
    expect(render()).not.toContain('data-testid="people-position-a-position-1"');
  });

  test("archived department rows hide vacancy controls and preserve read permission restrictions", () => {
    navigation.query = "unit=department:a-dept&status=archived";
    const html = render({...props, canManage: true});
    expect(html).not.toContain('data-testid="people-position-team-vacancy"');
    expect(html).not.toContain('Vacant position');
    expect(html).not.toContain('aria-label="Position occupancy"');
    expect(html).toContain('archived people');
    // Read-only users cannot switch the data source through the archived URL.
    expect(render()).toContain('data-testid="people-position-team-vacancy"');
  });
});

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
