import { expect, it } from "vitest";
import { createElement } from "react";
import { renderToStaticMarkup } from "react-dom/server";
import { buildOrganizationGraph, ancestorPath, visibleChartIds, searchOrganization } from "../src/lib/organization-chart";
import { layoutOrganizationChart, chartCardWidth, chartCardHeight } from "../src/lib/organization-chart-layout";
import { OrganizationTeamView } from "../src/components/modules/organization-team-view";
import type { OrganizationOverview, OrganizationPerson } from "../src/lib/organization";

function person(id: string, manager: string | null, extra: Partial<OrganizationPerson> = {}): OrganizationPerson {
  return { person_id: id, engagement_id: "e-" + id, user_id: null, email: null, legal_name: "Person " + id, engagement_version: 1, person_version: 1, company_id: "company", branch_id: "branch", department_id: "department", position_id: null, manager_person_id: manager, level_id: null, level_number: manager ? 6 : 1, level_name: manager ? "Staff" : "CEO", status: "active", person_status: "active", start_date: "2026-01-01", end_date: null, job_title: "Staff", issues: [], ...extra };
}
function data(people: OrganizationPerson[]): OrganizationOverview {
  return { people, positions: [], nodes: [{ id: "branch", display_name: "Bangkok", kind: "branch", branch_id: null }, { id: "department", display_name: "Operations", kind: "department", branch_id: "branch" }], levels: [], templates: [], canManage: false };
}
it("uses actual managers across levels and branches, with direct counts and full search", () => {
  const graph = buildOrganizationGraph(data([person("ceo", null), person("manager", "ceo", { level_number: 3 }), person("staff", "manager", { legal_name: "Sofia Example", branch_id: "elsewhere" }), person("direct", "ceo")]));
  expect(graph.parents.get("e-direct")).toBe("e-ceo");
  expect(graph.children.get("e-ceo")).toHaveLength(2);
  expect(ancestorPath(graph, "e-staff")).toEqual(["e-ceo", "e-manager"]);
  expect(visibleChartIds(graph, new Set(graph.roots))).not.toContain("e-staff");
  expect(searchOrganization(graph, "sofia level 6")).toEqual(["e-staff"]);
  expect(searchOrganization(graph, "Bangkok Operations")).toHaveLength(3);
});
it("uses engagement identities and keeps ambiguous manager profiles reachable", () => {
  const graph = buildOrganizationGraph(data([person("manager", null), person("manager", null, { engagement_id: "second-engagement" }), person("staff", "manager")]));
  expect(graph.entries.size).toBe(3);
  expect(graph.parents.has("e-staff")).toBe(false);
  expect(graph.attention).toContain("e-staff");
  expect(visibleChartIds(graph, new Set())).toHaveLength(3);
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
  expect(new Set(visibleChartIds(graph, new Set(graph.entries.keys()))).size).toBe(5);
  expect(ancestorPath(graph, "e-a")).toEqual([]);
});
it("keeps incomplete profiles and multiple starting teams visible", () => {
  const graph = buildOrganizationGraph(data([person("a", null), person("b", null, { level_number: null, issues: ["Assign a manager."] })]));
  const html = renderToStaticMarkup(createElement(OrganizationTeamView, { graph, focusId: null, onFocus() {}, onDetails() {} }));
  expect(html).toContain("Choose a starting team");
  expect(html).toContain("Level not mapped");
  expect(html.match(/data-person-id=/g)).toHaveLength(2);
});
it("renders the selected person's actual manager, their team and a leaf empty state", () => {
  const graph = buildOrganizationGraph(data([person("ceo", null), person("manager", "ceo"), person("leaf", "manager")]));
  const html = renderToStaticMarkup(createElement(OrganizationTeamView, { graph, focusId: "e-leaf", onFocus() {}, onDetails() {} }));
  expect(html).toContain("Person manager");
  expect(html).toContain("No direct reports in this view.");
  expect(html.match(/data-person-id=/g)).toHaveLength(2);
});
it("handles 500 profiles through layout and actual team rendering without changing the source", () => {
  const overview = data(Array.from({ length: 500 }, (_, i) => person(String(i), i ? "0" : null)));
  const before = JSON.stringify(overview);
  const graph = buildOrganizationGraph(overview);
  const ids = visibleChartIds(graph, new Set(graph.entries.keys()));
  const layout = layoutOrganizationChart(graph, ids);
  expect(layout.positions.size).toBe(500);
  expect(layout.edges).toHaveLength(499);
  const points = [...layout.positions.values()].sort((a, b) => a.x - b.x);
  expect(points.every(p => Number.isFinite(p.x) && Number.isFinite(p.y))).toBe(true);
  const childPoints = ids.filter(id => id !== "e-0").map(id => layout.positions.get(id)!).sort((a, b) => a.x - b.x);
  for (let i = 1; i < childPoints.length; i++) expect(childPoints[i].x - childPoints[i - 1].x).toBeGreaterThanOrEqual(chartCardWidth);
  expect(childPoints[0].y - layout.positions.get("e-0")!.y).toBeGreaterThan(chartCardHeight);
  const html = renderToStaticMarkup(createElement(OrganizationTeamView, { graph, focusId: "e-0", onFocus() {}, onDetails() {} }));
  expect(html.match(/data-person-id=/g)).toHaveLength(500);
  expect(html).toContain("499");
  expect(JSON.stringify(overview)).toBe(before);
});
