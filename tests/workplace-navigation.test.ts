import { describe, it, expect } from "vitest";
import {
  workplaceNavigation,
  activeDestination,
} from "../src/lib/workplace-navigation";
import { filterWork, type WorkItem } from "../src/lib/work-item";
import { moduleRegistry } from "../src/data/module-registry";
describe("Workplace discovery", () => {
  it("keeps staff navigation compact and excludes inaccessible search results", () => {
    const nav = workplaceNavigation({
      workspaceBase: "/holdings/acme",
      role: "employee",
      allowedModules: ["people_payroll", "collaboration"],
      adminSections: ["feedback"],
    });
    expect(nav.primary.map((x) => x.id)).toEqual(["home", "work", "inbox"]);
    expect(nav.search.some((x) => x.href.includes("/finance"))).toBe(false);
    expect(nav.more.some((x) => x.label === "Administration")).toBe(false);
    expect(nav.search.some((x) => x.href.endsWith("/feed"))).toBe(false);
    expect(new Set(nav.search.map((x) => x.href)).size).toBe(nav.search.length);
  });
  it("retains all authorized module capabilities even when labelled preview", () => {
    const nav = workplaceNavigation({
      workspaceBase: "/holdings/acme",
      role: "holding_owner",
      allowedModules: moduleRegistry.map((m) => m.id),
      adminSections: [
        "people",
        "organization",
        "approvals",
        "usage",
        "feedback",
      ],
    });
    expect(
      nav.search.some((x) => x.href.includes("/modules/organization/")),
    ).toBe(true);
    expect(nav.more.find((x) => x.id === "settings")?.href).toContain(
      "section=people",
    );
  });
  it("keeps beta and platform discovery inside their supported surfaces", () => {
    const beta = workplaceNavigation({
      workspaceBase: "/holdings/beta",
      beta: true,
      allowedModules: [],
      adminSections: ["people", "organization", "usage", "feedback"],
    });
    expect(beta.more.map((x) => x.id)).toEqual([
      "people",
      "handbook",
      "settings",
      "feedback",
    ]);
    const platform = workplaceNavigation({
      workspaceBase: "/platform",
      allowedModules: ["finance", "identity_access"],
    });
    expect(
      platform.more.some((x) =>
        ["people", "settings", "commerce", "projects"].includes(x.id),
      ),
    ).toBe(false);
    expect(platform.search.every((x) => x.href.startsWith("/platform/"))).toBe(
      true,
    );
    expect(
      platform.search.some((x) => x.href === "/platform/finance/ledger"),
    ).toBe(false);
  });
  it("maps detail and legacy routes back to the correct primary destination", () => {
    expect(
      activeDestination("/holdings/acme", "/holdings/acme/staff-tasks/a"),
    ).toBe("work");
    expect(activeDestination("/holdings/acme", "/holdings/acme/posts/a")).toBe(
      "home",
    );
    expect(activeDestination("/holdings/acme", "/holdings/acme/settings")).toBe(
      "more",
    );
  });
});
it("filters mixed work without treating another queue as actionable", () => {
  const rows: WorkItem[] = [
    {
      id: "same",
      type: "leave",
      title: "Leave",
      detail: "Morning handover",
      href: "/leave",
      status: "pending_approval",
      updatedAt: 1,
      queue: "waiting",
    },
    {
      id: "same",
      type: "staff_task",
      title: "Handover",
      detail: "Review delivery",
      href: "/task",
      status: "completion_approval",
      updatedAt: 2,
      queue: "todo",
    },
  ];
  expect(
    filterWork(rows, "todo", "all", "handover").map((x) => x.type),
  ).toEqual(["staff_task"]);
  expect(filterWork(rows, "all", "leave", "")).toHaveLength(1);
  expect(filterWork(rows, "completed", "all", "")).toHaveLength(0);
});
