import { expect, it } from "vitest";
import { createElement } from "react";
import { renderToStaticMarkup } from "react-dom/server";
import { buildOrganizationGraph, ancestorPath, teamIds, levelGroups, departmentHue, assignedLevel, searchOrganization } from "@erpx/feature-organization/lib/organization-chart";
import { OrganizationTeamView } from "@erpx/feature-organization/components/organization-team-view";
import type { OrganizationOverview, OrganizationPerson } from "@erpx/feature-organization/lib/organization";

function person(id: string, manager: string | null, extra: Partial<OrganizationPerson> = {}): OrganizationPerson {
  return { person_id: id, engagement_id: "e-" + id, user_id: null, email: null, legal_name: "Person " + id, engagement_version: 1, person_version: 1, company_id: "company", branch_id: "branch", department_id: "department", position_id: null, manager_person_id: manager, level_id: null, level_number: manager ? 6 : 1, level_name: manager ? "Staff" : "CEO", status: "active", person_status: "active", start_date: "2026-01-01", end_date: null, job_title: "Staff", issues: [], ...extra };
}
function data(people: OrganizationPerson[]): OrganizationOverview {
  return { actorId:"fixture",nodeChanges:[],changeQueue:{recordId:null,state:"",query:"",page:1,pageSize:20,total:0,pages:0},canManageUnits:false,people, positions: [], nodes: [{ status:"active",hierarchy_revision:1,id: "branch", display_name: "Bangkok", kind: "branch", slug: "bangkok", branch_id: null }, { status:"active",hierarchy_revision:1,id: "department", display_name: "Operations", kind: "department", slug: "operations", branch_id: "branch" }], levels: [], templates: [], canManage: false };
}
it("uses actual managers across levels and branches, with direct counts and full search", () => {
  const graph = buildOrganizationGraph(data([person("ceo", null), person("manager", "ceo", { level_number: 3 }), person("staff", "manager", { legal_name: "Sofia Example", branch_id: "elsewhere" }), person("direct", "ceo")]));
  expect(graph.parents.get("e-direct")).toBe("e-ceo");
  expect(graph.children.get("e-ceo")).toHaveLength(2);
  expect(ancestorPath(graph, "e-staff")).toEqual(["e-ceo", "e-manager"]);
  expect(teamIds(graph, "e-manager")).toEqual(new Set(["e-manager", "e-staff"]));
  expect(searchOrganization(graph, "sofia level 6")).toEqual(["e-staff"]);
  expect(searchOrganization(graph, "Bangkok Operations")).toHaveLength(3);
});
it("uses engagement identities and keeps ambiguous manager profiles reachable", () => {
  const graph = buildOrganizationGraph(data([person("manager", null), person("manager", null, { engagement_id: "second-engagement" }), person("staff", "manager")]));
  expect(graph.entries.size).toBe(3);
  expect(graph.parents.has("e-staff")).toBe(false);
  expect(graph.attention).toContain("e-staff");
  expect(teamIds(graph, null).size).toBe(3);
});
it("never leaks hidden managers or counts and does not call a scoped boundary a missing manager", () => {
  const graph = buildOrganizationGraph(data([person("staff", "private-manager")]));
  expect(graph.roots).toEqual(["e-staff"]);
  expect(graph.attention).toEqual([]);
  expect(graph.entries.get("e-staff")?.boundary).toBe("Manager is not available in this view.");
  expect(searchOrganization(graph, "private-manager")).toEqual([]);
  expect(graph.children.get("e-staff")).toEqual([]);
});
it("does not connect profiles across companies", () => {
  const graph = buildOrganizationGraph(data([person("manager", null, { company_id: "other" }), person("staff", "manager")]));
  expect(graph.parents.size).toBe(0);
});
it("breaks self and multi-person cycles while retaining all people and downstream reports", () => {
  const graph = buildOrganizationGraph(data([person("self", "self"), person("a", "b"), person("b", "c"), person("c", "a"), person("report", "b")]));
  expect(graph.attention).toEqual(expect.arrayContaining(["e-self", "e-a", "e-b", "e-c"]));
  expect(graph.parents.get("e-report")).toBe("e-b");
  expect(teamIds(graph, null).size).toBe(5);
  expect(ancestorPath(graph, "e-a")).toEqual([]);
});
it("groups assigned levels independently of reporting depth and retains unknown levels", () => {
  const overview = data([person("ceo", null), person("direct", "ceo"), person("unknown", null, { level_number: 9 })]);
  const graph = buildOrganizationGraph(overview);
  const levels = levelGroups(graph, overview, teamIds(graph, null));
  expect(levels[5].groups[0].ids).toEqual(["e-direct"]);
  expect(levels[6].groups[0].ids).toEqual(["e-unknown"]);
  expect([null, 0, -1, 1.5, 7].map(assignedLevel)).toEqual([7, 7, 7, 7, 7]);
});
it("keeps branches separate while sharing department colors", () => {
  const overview = data([person("a", null), person("b", null, { branch_id: "other", department_id: "other-ops" })]);
  overview.nodes.push({ status:"active",hierarchy_revision:1,id: "other-ops", kind: "department", slug: "operations", display_name: "Operations", branch_id: "other" });
  const graph = buildOrganizationGraph(overview);
  const groups = levelGroups(graph, overview, teamIds(graph, null))[0].groups;
  expect(groups).toHaveLength(2);
  expect(groups[0].hue).toBe(groups[1].hue);
  expect(departmentHue("unfamiliar")).toBe(departmentHue("unfamiliar"));
  expect(new Set(["finance", "accounting", "operations", "people", "sales", "purchasing", "warehouse", "it", "customer-service"].map(departmentHue)).size).toBe(9);
});
it("previews 500 profiles without changing the source and keeps all searchable", () => {
  const overview = data(Array.from({ length: 500 }, (_, i) => person(String(i), i ? "0" : null)));
  const before = JSON.stringify(overview);
  const graph = buildOrganizationGraph(overview);
  expect(teamIds(graph, "e-0").size).toBe(500);
  const html = renderToStaticMarkup(createElement(OrganizationTeamView, { graph, data: overview, focusId: null, branch: "", department: "", reveal: null, onFocus() {}, onDetails() {} }));
  expect(html.match(/data-person-id=/g)).toHaveLength(3);
  expect(html).toContain("497 remaining");
  expect(searchOrganization(graph, "Person 499")).toEqual(["e-499"]);
  expect(JSON.stringify(overview)).toBe(before);
});
