import { describe, it, expect } from "vitest";
import {
  workplaceNavigation,
  activeDestination,
  navigationTrail,
  resolveNavigation,
} from "../src/lib/workplace-navigation";
import { filterWork, workTypes, type WorkItem } from "../src/lib/work-item";
describe("Workplace discovery", () => {
  it("keeps staff navigation compact and excludes inaccessible search results", () => {
    const nav = workplaceNavigation({
      workspaceBase: "/holdings/acme",
      role: "employee",
      adminSections: ["feedback"],
    });
    expect(nav.primary.map((x) => x.id)).toEqual(["home", "work", "inbox"]);
    expect(nav.search.some((x) => x.href.includes("/finance"))).toBe(false);
    expect(nav.groups.some((x) => x.label === "Administration")).toBe(false);
    expect(nav.search.some((x) => x.href.endsWith("/feed"))).toBe(false);
    expect(new Set(nav.search.map((x) => x.href)).size).toBe(nav.search.length);
  });
  it("shows authorized live areas without module capability results", () => {
    const nav = workplaceNavigation({
      workspaceBase: "/holdings/acme",
      role: "holding_owner",
      adminSections: [
        "people",
        "organization",
        "approvals",
        "usage",
        "feedback",
      ],
    });
    expect(
      nav.search.some((x) => x.href.includes("/modules/")),
    ).toBe(false);
    expect(
      nav.groups.filter((x) => x.placement === "business").map((x) => x.id),
    ).toEqual([
      "people",
      "operations",
      "finance",
      "projects",
    ]);
    expect(nav.groups.find((x) => x.id === "administration")?.href).toContain(
      "section=people",
    );
    expect(
      nav.groups
        .find((x) => x.id === "operations")
        ?.items.map((x) => x.label),
    ).toEqual([
      "Quotations",
      "Purchase orders",
      "Stock & movements",
      "Stock counts",
      "Warehouse transfers",
      "Products",
      "Warehouses",
      "Customers & suppliers",
    ]);
  });
  it("assigns a semantic non-parent icon to every authorized child", () => {
    const nav = workplaceNavigation({
      workspaceBase: "/holdings/acme",
      scopeKind: "holding",
      role: "holding_owner",
      adminSections: [
        "people",
        "organization",
        "approvals",
        "usage",
        "ai",
        "feedback",
      ],
    });
    const iconsByLabel = Object.fromEntries(
      nav.groups.flatMap((group) =>
        group.items.map((child) => [child.label, child.icon]),
      ),
    );
    expect(iconsByLabel).toMatchObject({
      Organization: "admin-organization",
      Payroll: "payroll",
      "Directory & access": "access",
      Quotations: "quotation",
      "Purchase orders": "purchasing",
      "Stock & movements": "stock",
      "Stock counts": "stock-count",
      "Warehouse transfers": "stock-transfer",
      Products: "product",
      Warehouses: "warehouse",
      "Customers & suppliers": "parties",
      "Ledger & reports": "ledger",
      Invoices: "invoice",
      "Bank reconciliation": "bank",
      "Employee expenses": "expense",
      "Staff tasks": "staff-task",
      "Routine templates": "routine",
      "Company handbook": "handbook",
      Learn: "education",
      "Help & feedback": "feedback",
      "Approvals & routines": "admin-approvals",
      "Usage & billing": "admin-usage",
      "AI providers": "admin-ai",
      "Identity & access": "admin-iam",
    });
    for (const group of nav.groups) {
      expect(group.items.every((child) => child.icon !== group.icon)).toBe(true);
    }
  });
  it("keeps beta and platform discovery inside their supported surfaces", () => {
    const beta = workplaceNavigation({
      workspaceBase: "/holdings/beta",
      beta: true,
      adminSections: ["people", "organization", "usage", "feedback"],
    });
    expect(
      beta.groups.filter((x) => x.placement === "business").map((x) => x.id),
    ).toEqual(["people"]);
    expect(beta.groups.map((x) => x.id)).toEqual([
      "people",
      "resources",
      "administration",
    ]);
    const platform = workplaceNavigation({
      workspaceBase: "/platform",
      scopeKind: "platform",
    });
    expect(
      platform.standalone.some((x) =>
        ["people", "settings", "commerce", "projects"].includes(x.id),
      ),
    ).toBe(false);
    expect(platform.groups).toEqual([]);
    expect(platform.standalone.map((x) => x.id)).toEqual([
      "learn",
      "feed",
      "platform-iam",
      "my-access",
    ]);
    expect(platform.search.every((x) => x.href.startsWith("/platform/"))).toBe(
      false,
    );
    expect(
      platform.search.some((x) => x.href === "/platform/finance/ledger"),
    ).toBe(false);
  });
  it("maps collections, details, and query views to their canonical parents", () => {
    expect(
      activeDestination("/holdings/acme", "/holdings/acme/staff-tasks/a"),
    ).toBe("projects");
    expect(activeDestination("/holdings/acme", "/holdings/acme/posts/a")).toBe(
      "home",
    );
    const owner = workplaceNavigation({
      workspaceBase: "/holdings/acme",
      scopeKind: "holding",
      role: "holding_owner",
      adminSections: ["people", "organization", "approvals", "usage", "feedback"],
    });
    expect(
      resolveNavigation(
        owner,
        "/holdings/acme",
        "/holdings/acme/commerce",
        new URLSearchParams("view=quote"),
      ),
    ).toMatchObject({ activeId: "operations-quotations", groupId: "operations" });
    expect(
      resolveNavigation(
        owner,
        "/holdings/acme",
        "/holdings/acme/settings",
        new URLSearchParams("section=organization"),
      ),
    ).toMatchObject({
      activeId: "administration-organization",
      groupId: "administration",
    });
    expect(activeDestination("/holdings/acme", "/holdings/acme/finance")).toBe(
      "finance",
    );
    expect(
      navigationTrail(
        owner,
        "/holdings/acme",
        "/holdings/acme/finance/invoices/inv-1",
      ).map((crumb) => crumb.label),
    ).toEqual(["Finance", "Invoices", "Invoice"]);
  });
  it("does not expose generic workspace records as a Work type", () => {
    expect(workTypes.some(([id]) => id === "record")).toBe(false);
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
