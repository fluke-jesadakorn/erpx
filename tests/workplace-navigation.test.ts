import { describe, it, expect } from "vitest";
import {
  workplaceNavigation,
  navigationCollapsedValue,
  activeDestination,
  navigationTrail,
  navigationCategory,
  pmsSidebarHref,
  resolveNavigation,
} from "../src/lib/workplace-navigation";
import { filterWork, workTypes, workViews, type WorkItem } from "../src/lib/work-item";
import { pmsCrossLinks } from "../src/lib/pms-cross-links";

describe("Side menu destination coverage", () => {
  const base = "/holdings/acme";
  const owner = workplaceNavigation({
    workspaceBase: base, role: "holding_owner", canManageApprovals: true,
    adminSections: ["people", "organization", "approvals", "usage", "ai", "feedback"],
  });

  it("keeps ERPX Management in its authorized holding placement", () => {
    const erpx = workplaceNavigation({ workspaceBase: "/holdings/erpx", scopeKind: "holding", role: "holding_owner", manageErpxVisible: true });
    expect(erpx.apps.find(app => app.id === "erpx-management")?.items.map(item => item.href)).toEqual([
      "/holdings/erpx/manage-erpx",
      "/holdings/erpx/manage-erpx/billing",
      "/holdings/erpx/manage-erpx/billing/usage",
      "/holdings/erpx/manage-erpx/iam",
      "/holdings/erpx/manage-erpx/executive",
    ]);
    expect(erpx.categories.map(category => category.id).slice(1, 5)).toEqual(["customers-sales", "purchasing", "inventory-quality", "projects-routines"]);
    expect(erpx.categories.find(category => category.id === "erpx-management")?.apps.some(item => item.id === "erpx-management")).toBe(true);
    expect(erpx.search.some(item => item.id === "manage-erpx-iam")).toBe(true);
    for (const workspaceBase of ["/holdings/acme", "/holdings/erpx/branches/main"]) {
      const other = workplaceNavigation({ workspaceBase, scopeKind: workspaceBase.includes("branches") ? "branch" : "holding", role: "holding_owner", manageErpxVisible: true });
      expect(other.apps.some(app => app.id === "erpx-management")).toBe(false);
    }
    expect(workplaceNavigation({ workspaceBase: "/holdings/erpx", manageErpxVisible: false }).apps.some(app => app.id === "erpx-management")).toBe(false);
  });

  it('shows only approved pilot apps in production navigation', () => {
    const pilot=workplaceNavigation({workspaceBase:base,role:'holding_owner',availableAppIds:['people','inventory','pos','pms','finance']});
    expect(pilot.apps.map((app)=>app.id)).toEqual(['people','inventory','pos','pms','finance','guides','services']);
    expect(pilot.search.some((item)=>item.id==='crm-pipeline')).toBe(false);
    expect(pilot.search.some((item)=>item.id==='finance-engines')).toBe(false);
    expect(pilot.search.some((item)=>item.id==='people-payroll'&&item.href===`${base}/people/payroll`)).toBe(true);
    expect(pilot.search.some((item)=>item.id==='future-features')).toBe(true);
    const purchasing=workplaceNavigation({workspaceBase:base,role:'holding_owner',availableAppIds:['procurement']});
    expect(purchasing.apps.find((app)=>app.id==='procurement')?.items.map((item)=>item.id)).toEqual(['operations-purchasing','procurement-rfqs','procurement-suppliers']);
  });

  it("makes each visible destination unique and highlights its own entry", () => {
    const contexts = [
      { workspaceBase: base, role: "employee" },
      { workspaceBase: base, role: "holding_owner", canManageApprovals: true, adminSections: ["approvals", "usage", "ai", "feedback"] as const },
      { workspaceBase: `${base}/branches/main`, scopeKind: "branch" as const, role: "holding_owner" },
      { workspaceBase: base, beta: true, adminSections: ["people", "organization", "usage", "feedback"] as const },
      { workspaceBase: "/platform", scopeKind: "platform" as const },
    ];
    for (const context of contexts) {
      const model = workplaceNavigation({ ...context, adminSections: context.adminSections ? [...context.adminSections] : [] });
      const destinations = [...model.primary, ...model.apps.flatMap(app => app.items), ...model.standalone];
      expect(new Set(destinations.map(item => item.href)).size).toBe(destinations.length);
      expect(model.search.map(item => item.href)).toEqual(expect.arrayContaining(destinations.map(item => item.href)));
      expect(new Set(model.search.map(item=>item.href)).size).toBe(model.search.length);
      for (const item of destinations) {
        const url = new URL(item.href, "https://navigation.invalid");
        expect(resolveNavigation(model, context.workspaceBase, url.pathname, url.searchParams).activeId, item.href).toBe(item.id);
      }
    }
  });

  it("exposes missing pages by their feature names in the appropriate group", () => {
    expect(owner.apps.find(app => app.id === "people")?.items.map(item => item.id)).toEqual(["people-directory", "people-evaluation", "people-roster", "people-payroll"]);
    expect(owner.apps.find(app => app.id === "sales")?.items.map(item => item.id)).toContain("operations-sales");
    expect(owner.apps.some(app => ["administration", "resources"].includes(app.id))).toBe(false);
    expect(owner.categories.find(category => category.id === "administration")?.apps.map(item => item.id)).toContain("administration-approvals");
    expect(owner.apps.find(app => app.id === "technology")?.items.map(item => item.id)).toContain("administration-usage");
    expect(owner.apps.find(app => app.id === "technology")?.items.map(item => item.id)).toContain("administration-ai");
    expect(owner.apps.find(app => app.id === "inventory")?.items.map(item => item.label)).toEqual(["Stock & movements", "Stock counts", "Warehouse transfers", "Products", "Warehouses", "Stock in transit"]);
  });

  it("keeps feature permissions and scope restrictions in the side menu", () => {
    const custom = workplaceNavigation({ workspaceBase: base, permissions: ["inventory.read"] });
    expect(custom.apps.find(app => app.id === "inventory")?.items.map(item => item.id)).toEqual(["operations-counts", "operations-transfers", "operations-transit"]);
    expect(custom.apps.some(app => ["sales", "procurement"].includes(app.id))).toBe(false);
    expect(custom.search.some(item => item.id === "people-settings")).toBe(false);
    const branch = workplaceNavigation({ workspaceBase: `${base}/branches/main`, scopeKind: "branch", role: "holding_owner" });
    expect(branch.search.some(item => item.id === "pos-settings")).toBe(false);
    expect(branch.search.some(item => item.id === "technology-business-readiness")).toBe(false);
    const housekeeper = workplaceNavigation({ workspaceBase: base, role: "pms_housekeeping" });
    expect(housekeeper.search.some(item => ["pms-folios", "pms-cashier", "pms-reports", "pms-night-audit"].includes(item.id))).toBe(false);
    expect(housekeeper.search.some(item => item.id === "pms-front")).toBe(true);
  });

  it("exposes Operations through its scoped sidebar and launcher using existing manager permissions", async () => {
    const { navigationLauncher } = await import("../src/lib/navigation-catalog");
    const { builtInPermissions } = await import("../src/lib/permissions");
    const permissions = builtInPermissions("operations_manager");
    expect(permissions).toContain("records.read");
    expect(permissions).toContain("module.operations.read");
    for (const [workspaceBase, scopeKind] of [[base, "holding"], ["/holdings/erpx", "holding"], [`${base}/branches/bangkok`, "branch"]] as const) {
      const model = workplaceNavigation({ workspaceBase, scopeKind, role: "operations_manager", permissions, moduleIds: ["operations"], availableAppIds: ["operations"] });
      const category = model.categories.find(entry => entry.id === "projects-routines");
      expect(category?.apps.map(entry => [entry.id, entry.label, entry.href, entry.ownerModuleId])).toEqual([["operations", "Operations", `${workspaceBase}/operations`, "operations"]]);
      expect(model.apps.find(entry => entry.id === "operations")?.items.map(entry => entry.href)).toEqual([`${workspaceBase}/operations`]);
      expect(navigationLauncher(model, "Operations").results.map(result => [result.item.label, result.item.href, result.categoryId])).toEqual([["Operations", `${workspaceBase}/operations`, "projects-routines"]]);
      expect(navigationLauncher(model, "").categories.find(entry => entry.id === "projects-routines")?.apps.map(entry => entry.label)).toEqual(["Operations"]);
      expect(model.search.some(entry => entry.href === `${workspaceBase}/operations`)).toBe(true);
      expect(model.apps.some(entry => entry.id === "finance")).toBe(false);
      expect(model.search.some(entry => entry.label === "Finance")).toBe(false);
    }
  });

  it("keeps Operations hidden without its module read permission or rollout and does not expose Projects", () => {
    const allowed = { workspaceBase: base, role: "operations_manager", permissions: ["records.read"], moduleIds: ["operations"], availableAppIds: ["operations"] };
    for (const context of [{ ...allowed, moduleIds: [] }, { ...allowed, moduleIds: ["inventory"] }, { ...allowed, permissions: [] }, { ...allowed, availableAppIds: [] }, { ...allowed, availableAppIds: ["projects"] }, { ...allowed, beta: true }]) {
      const model = workplaceNavigation(context);
      expect(model.apps.some(entry => entry.id === "operations")).toBe(false);
      expect(model.search.some(entry => entry.href === `${base}/operations`)).toBe(false);
    }
    const model = workplaceNavigation(allowed);
    expect(model.apps.some(entry => entry.id === "operations")).toBe(true);
    expect(model.apps.some(entry => entry.id === "projects")).toBe(false);
    expect(model.search.some(entry => ["projects-list", "projects-tasks", "projects-routines"].includes(entry.id))).toBe(false);
    const withoutRolloutList = workplaceNavigation({ ...allowed, availableAppIds: undefined });
    expect(withoutRolloutList.apps.some(entry => entry.id === "operations")).toBe(true);
    expect(withoutRolloutList.apps.find(entry => entry.id === "projects")?.items.map(entry => entry.id)).toEqual(["projects-list", "projects-tasks", "projects-routines"]);
  });

  it("keeps aliases, detail pages and query tabs under their current feature", () => {
    for (const [path, id, groupId] of [
      ["people/settings?section=connections", "technology-connections", "technology"],
      ["iam/connections", "technology-connections", "technology"],
      ["settings?section=people", "people-directory", "people"],
      ["commerce?view=sales", "operations-sales", "sales"],
      ["commerce?view=pricing", "operations-pricing", "sales"],
      ["commerce?view=purchase", "operations-purchasing", "procurement"],
      ["commerce?view=parties&kind=customer", "crm-customers", "crm"],
      ["commerce?view=parties&kind=supplier", "procurement-suppliers", "procurement"],
      ["procurement-rfqs/rfq-1", "procurement-rfqs", "procurement"],
      ["commerce?view=products", "operations-products", "inventory"],
      ["stock-counts/count-1", "operations-counts", "inventory"],
      ["stock-transfers/transfer-1", "operations-transfers", "inventory"],
      ["pos?tab=shifts", "pos-front", "pos"],
      ["pms", "pms-front", "pms"],
      ["pms/documents/doc-1", "pms-front", "pms"],
      ["people/person-1", "people-directory", "people"],
      ["people/organization", "people-directory", "people"],
      ["people?status=archived", "people-directory", "people"],
      ["people/payroll/runs/run-1", "people-payroll", "people"],
      ["people/payroll/settings", "people-payroll", "people"],
      ["people/payroll/history", "people-payroll", "people"],
      ["people/payroll/advances", "people-payroll", "people"],
      ["people/roster", "people-roster", "people"],
      ["people/settings?section=leave", "people-directory", "people"],
      ["people?panel=schedule", "people-directory", "people"],
      ["people?panel=settings&section=leave", "people-directory", "people"],
      ["projects/project-1", "projects-list", "projects"],
    ]) {
      const url = new URL(`${base}/${path}`, "https://navigation.invalid");
      expect(resolveNavigation(owner, base, url.pathname, url.searchParams)).toMatchObject({ activeId: id, groupId });
    }
  });
});

describe("Workplace discovery", () => {
  it("keeps team schedules in the People workspace and cash sessions on compatible POS routes at every scope", () => {
    for (const [scopeKind, workspaceBase] of [
      ["holding", "/holdings/acme"],
      ["branch", "/holdings/acme/branches/bangkok"],
      ["department", "/holdings/acme/branches/bangkok/departments/operations"],
    ] as const) {
      const nav = workplaceNavigation({ workspaceBase, scopeKind, role: "holding_owner", adminSections: ["people", "organization"] });
      expect(nav.apps.find(app => app.id === "people")?.items.some(item => item.id === "people-roster")).toBe(true);
      expect(resolveNavigation(nav, workspaceBase, `${workspaceBase}/people/roster`)).toMatchObject({ activeId: "people-roster", groupId: "people" });
      if (scopeKind !== "department") expect(nav.apps.find(app => app.id === "pos")?.items).toContainEqual(expect.objectContaining({ label: "Open POS Front", href: `${workspaceBase}/pos` }));
    }
  });
  it("keeps staff navigation compact and excludes inaccessible search results", () => {
    const nav = workplaceNavigation({
      workspaceBase: "/holdings/acme",
      role: "employee",
      adminSections: ["feedback"],
    });
    expect(nav.primary.map((x) => [x.id, x.label])).toEqual([["home", "Today"], ["work", "My work"], ["calendar", "Calendar"], ["inbox", "Messages"]]);
    expect(nav.search.some((x) => x.href.includes("/finance"))).toBe(false);
    expect(nav.apps.some((x) => x.label === "Administration")).toBe(false);
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
      nav.apps.filter((x) => x.placement === "business").map((x) => x.id),
    ).toEqual(["executive", "people", "sales", "procurement", "inventory", "crm", "support", "quality", "pos", "pms", "finance", "projects", "technology", "customer_authority", "services"]);
    expect(nav.apps.find((x) => x.id === "projects")?.label).toBe("Projects & Routines");
    expect(
      nav.apps
        .find((x) => x.id === "sales")
        ?.items.map((x) => x.label),
    ).toEqual([
      "Sales orders",
      "Quotations",
      "Price lists",
    ]);
    expect(nav.apps.find((x) => x.id === "procurement")?.items.map((x) => x.label)).toEqual(["Purchase orders", "Supplier quotation requests", "Suppliers"]);
  });
  it("assigns semantic icons to feature destinations", () => {
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
      [...nav.apps.flatMap((app) => app.items), ...nav.standalone].map((child) => [child.label, child.icon]),
    );
    expect(iconsByLabel).toMatchObject({
      People: "admin-people",
      Payroll: "payroll",
      "Organization & Access": "access",
      Quotations: "quotation",
      "Purchase orders": "purchasing",
      "Stock & movements": "stock",
      "Stock counts": "stock-count",
      "Warehouse transfers": "stock-transfer",
      Products: "product",
      Warehouses: "warehouse",
      "Customer directory": "parties",
      "Suppliers": "parties",
      Accounting: "ledger",
      Receivables: "invoice",
      Payables: "invoice",
      "Cash & banking": "bank",
      "Employee expenses": "expense",
      "Staff tasks": "staff-task",
      "Routine templates": "routine",
      "Company handbook": "handbook",
      Guides: "learn",
      "Help using ERPX": "feedback",
      "Usage & billing": "admin-usage",
      "AI providers": "admin-ai",
    });
  });
  it("keeps Local workspaces operational and removes Company-only navigation", () => {
    const branch = workplaceNavigation({
      workspaceBase: "/holdings/acme/branches/bangkok",
      scopeKind: "branch",
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
    const people = branch.apps.find((app) => app.id === "people");
    expect(people?.items.some((item) => item.id === "people-payroll")).toBe(
      false,
    );
    expect(branch.search.some((item) => item.id === "technology-devices")).toBe(
      true,
    );
    expect(
      branch.apps
        .filter((app) => app.placement === "business")
        .map((app) => app.id),
    ).toEqual(["executive", "people", "sales", "procurement", "inventory", "crm", "support", "quality", "pos", "pms", "finance", "projects", "technology", "customer_authority", "services"]);
    expect(
      branch.search.some((item) =>
        ["approvals", "usage", "ai"].some((section) =>
          item.href.includes(`section=${section}`),
        ),
      ),
    ).toBe(false);
    expect(branch.search.some((item) => item.href.endsWith("/technology?view=resources"))).toBe(true);
    expect(branch.search.some((item) => item.href.endsWith("/iam"))).toBe(false);

    const department = workplaceNavigation({
      workspaceBase:
        "/holdings/acme/branches/bangkok/departments/operations",
      scopeKind: "department",
      role: "holding_owner",
      adminSections: ["people", "organization", "usage", "feedback"],
    });
    const operations = department.apps.find(
      (app) => app.id === "inventory",
    );
    expect(
      operations?.items.some((item) =>
        ["operations-counts", "operations-transfers", "operations-transit"].includes(item.id),
      ),
    ).toBe(false);
  });
  it("keeps beta and platform discovery inside their supported surfaces", () => {
    const beta = workplaceNavigation({
      workspaceBase: "/holdings/beta",
      beta: true,
      adminSections: ["people", "organization", "usage", "feedback"],
    });
    expect(
      beta.apps.filter((x) => x.placement === "business").map((x) => x.id),
    ).toEqual(["people"]);
    expect(beta.apps.map((x) => x.id)).toEqual([
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
    expect(platform.apps).toEqual([]);
    expect(platform.standalone.map((x) => x.id)).toEqual([
      "platform-billing",
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
    ).toMatchObject({ activeId: "operations-quotations", groupId: "sales" });
    expect(
      resolveNavigation(
        owner,
        "/holdings/acme",
        "/holdings/acme/settings",
        new URLSearchParams("section=organization"),
      ),
    ).toMatchObject({
      activeId: "people-directory",
      groupId: "people",
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
    ).toEqual(["Finance", "Receivables", "Invoice"]);
  });
  it("does not expose generic workspace records as a Work type", () => {
    expect(workTypes.map(([id])=>id)).not.toContain("record");
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

describe("Services grouped by purpose", () => {
  const base = "/holdings/acme";
  const sections = ["people", "organization", "approvals", "usage", "ai", "feedback"] as const;
  const owner = workplaceNavigation({ workspaceBase: base, role: "holding_owner", adminSections: [...sections], canManageApprovals: true });
  const cases = [
    ["handbook", "handbook", "Knowledge", "Company handbook"],
    ["learn", "learn", "Knowledge", "Guides"],
    ["people/settings", "people-directory", "People", "People"],
    ["people/settings?section=templates", "people-directory", "People", "People"],
    ["people/settings?section=attributes", "people-directory", "People", "People"],
    ["settings?section=usage", "administration-usage", "Technology", "Usage & billing"],
    ["settings?section=approvals", "administration-approvals", "Technology", "Approval routing"],
    ["people/access", "technology-devices", "Technology", "Resources"],
    ["people/access?view=packages", "technology-devices", "Technology", "Resources"],
    ["people/access?view=groups", "technology-organization", "Technology", "Organization & Access"],
    ["people/access?view=policies", "technology-organization", "Technology", "Organization & Access"],
    ["people/access?view=devices", "technology-devices", "Technology", "Resources"],
    ["people/settings?section=connections", "technology-connections", "Technology", "Directory connections"],
    ["people/settings?section=roles", "technology-roles", "Technology", "Roles & permissions"],
    ["people/settings?section=matrix", "technology-roles", "Technology", "Roles & permissions"],
    ["people/settings?section=reviews", "technology-reviews", "Technology", "Reviews & audit"],
    ["settings?section=ai", "administration-ai", "Technology", "AI providers"],
    ["iam/groups", "technology-organization", "Technology", "Organization & Access"],
    ["iam/mappings", "technology-organization", "Technology", "Organization & Access"],
    ["iam/connections", "technology-connections", "Technology", "Directory connections"],
    ["iam/roles", "technology-roles", "Technology", "Roles & permissions"],
    ["iam/matrix", "technology-roles", "Technology", "Roles & permissions"],
    ["settings?section=people", "people-directory", "People", "People"],
    ["settings?section=organization", "people-directory", "People", "People"],
    ["iam/overview", "people-directory", "People", "People"],
    ["iam/users", "people-directory", "People", "People"],
  ];

  it.each(cases)("resolves %s and discovers its feature once", (path, id, group, label) => {
    const url = new URL(`${base}/${path}`, "https://navigation.invalid");
    expect(resolveNavigation(owner, base, url.pathname, url.searchParams).activeId).toBe(id);
    expect(navigationTrail(owner, base, url.pathname, url.searchParams).map(item => item.label)).toEqual(id === "people-directory" ? ["People"] : [group, label]);
    expect(owner.search.filter(item => item.id === id)).toEqual([expect.objectContaining({ label, group: id === "administration-approvals" ? "Administration" : group })]);
  });

  it("keeps the exact group order, People contents, and a direct help link", () => {
    expect(owner.apps.map(app => app.label)).toEqual(["Executive", "People", "Sales", "Purchasing", "Inventory", "CRM", "Customer support", "Quality", "POS", "Hotel PMS", "Finance", "Projects & Routines", "Technology", "Knowledge", "Customer reviews", "Service desk"]);
    expect(owner.apps.find(app => app.id === "people")!.items.map(item => item.label)).toEqual(["People", "Skills & performance", "Roster", "Payroll"]);
    expect(owner.categories.find(category => category.id === "people-organization")?.apps.every(item => !item.submodules?.length)).toBe(true);
    expect(owner.search.some(item => item.href === `${base}/people/organization` || item.href === `${base}/people?status=archived`)).toBe(false);
    expect(navigationTrail(owner, base, `${base}/people/organization`).map(item => item.label)).toEqual(["People"]);
    expect(navigationTrail(owner, base, `${base}/people/person-1`).map(item => item.label)).toEqual(["People", "Person"]);
    expect(navigationTrail(owner, base, `${base}/people/move`).map(item => item.label)).toEqual(["People", "Move"]);
    expect(navigationTrail(owner, base, `${base}/people/move/review`).map(item => item.label)).toEqual(["People", "Review move"]);
    expect(owner.apps.some(app => app.id === "payroll")).toBe(false);
    expect(owner.apps.find(app => app.id === "people")?.items.filter(item => item.id.startsWith("people-payroll")).map(item => item.href)).toEqual([
      `${base}/people/payroll`,
    ]);
    expect(owner.categories.find(category => category.id === "people-organization")?.apps[0].submodules?.find(item => item.id === "people-payroll")?.submodules).toBeUndefined();
    const treasury = workplaceNavigation({ workspaceBase: base, role: "treasury" });
    expect(treasury.apps.find(app => app.id === "people")?.items.filter(item => item.id.startsWith("people-payroll")).map(item => item.id)).toEqual(["people-payroll"]);
    expect(owner.apps.find(app=>app.id==="technology")?.icon).toBe("technology");
    expect(owner.apps.find(app=>app.id==="technology")?.items.map(item => item.label)).toEqual(["Organization & Access", "Resources", "Custom attributes", "Directory connections", "Roles & permissions", "Reviews & audit", "AI providers", "Approval routing", "Usage & billing", "Business readiness"]);
    expect(owner.standalone.map(item => item.label)).toEqual(["Legal", "Help using ERPX", "Pilot feature rollout", "Product roadmap"]);
    expect(navigationTrail(owner,base,`${base}/settings/pilot-features`).map(item=>item.label)).toEqual(['Pilot feature rollout']);
    expect(navigationTrail(owner,base,`${base}/settings/stripe`).map(item=>item.label)).toEqual(['Company Stripe']);
    expect(navigationTrail(owner,base,`${base}/settings/notifications`).map(item=>item.label)).toEqual(['Notification settings']);
    expect(navigationTrail(owner, base, `${base}/settings`, new URLSearchParams("section=feedback"))).toEqual([{ label: "Help using ERPX" }]);
    expect(resolveNavigation(owner, base, `${base}/iam/reviews`)).toEqual({ activeId: "work" });
    expect(navigationTrail(owner, base, `${base}/handbook/article-1`).map(item => item.label)).toEqual(["Knowledge", "Company handbook", "Article"]);
  });

  it("maps app-level navigation into presentation categories without changing breadcrumbs", () => {
    expect(owner.categories.map(category => category.id)).toEqual(["people-organization","customers-sales","purchasing","inventory-quality","projects-routines","services","finance","pos","pms","knowledge","insights","administration"]);
    expect(owner.categories.find(category=>category.id==="customers-sales")?.apps.slice(0,3).map(item=>item.label)).toEqual(["Sales","Customers"]);
    expect(owner.categories.flatMap(category=>category.apps).every(item=>!item.submodules?.length)).toBe(true);
    const bank = resolveNavigation(owner, base, `${base}/finance/bank`);
    expect(navigationCategory(owner, bank)?.id).toBe("finance");
    expect(navigationTrail(owner, base, `${base}/finance/bank`).map(item => item.label)).toEqual(["Finance", "Cash & banking"]);
    const pms = resolveNavigation(owner, base, `${base}/pms/reports`);
    expect(navigationCategory(owner, pms)?.id).toBe("pms");
    expect(navigationTrail(owner, base, `${base}/pms/reports`).map(item => item.label)).toEqual(["Hotel PMS", "PMS reports"]);
    const pos = resolveNavigation(owner, base, `${base}/pos`, new URLSearchParams("tab=shifts"));
    expect(navigationCategory(owner, pos)?.id).toBe("pos");
    expect(pos.activeId).toBe("pos-front");
    expect(navigationTrail(owner, base, `${base}/pos`, new URLSearchParams("tab=shifts")).map(item => item.label)).toEqual(["POS", "Open POS Front"]);
  });

  it("searches actual work destinations and labels their discovery categories", async () => {
    const { navigationLauncher, navigationSearch } = await import("../src/lib/navigation-catalog");
    const results = navigationLauncher(owner, "approval").results;
    expect(results.find(result => result.item.id === "administration-approvals")).toMatchObject({ context: "Administration", categoryId: "administration" });
    expect(results.some(result => result.item.label === "Price lists")).toBe(false);
    expect(navigationLauncher(owner, "follow-ups").results.find(result => result.item.id === "customer-followups")).toMatchObject({ context: "Customers & sales" });
    expect(navigationSearch(owner, "Today").some(result => result.item.id === "home")).toBe(true);
    expect(navigationSearch(owner, "Purchase request").some(result => result.item.id === "start-buy")).toBe(true);
    expect(navigationSearch(owner, "Administration").some(result => result.item.id === "administration-approvals")).toBe(true);
  });

  it("keeps all CRM destinations active under their own section and breadcrumb", async () => {
    const { navigationLauncher } = await import("../src/lib/navigation-catalog");
    const crm = owner.categories.find(category=>category.id==="customers-sales")!;
    expect(crm.apps.map(item=>item.id)).toEqual(["sales-hub", "customers-hub"]);
    expect(crm.apps.some(item=>item.id==="crm-customers")).toBe(false);
    for (const [path, activeId, label] of [
      ["crm", "crm-pipeline", "Pipeline"],
      ["customers", "customer-timeline", "Customers"],
      ["customer-followups", "customer-followups", "Follow-ups"],
      ["commerce?view=parties&kind=customer", "crm-customers", "Customer directory"],
    ]) {
      const url = new URL(`${base}/${path}`, "https://navigation.invalid");
      const resolved = resolveNavigation(owner, base, url.pathname, url.searchParams);
      expect(resolved).toMatchObject({ activeId, groupId: "crm" });
      expect(navigationCategory(owner, resolved)?.id).toBe("customers-sales");
      expect(navigationTrail(owner, base, url.pathname, url.searchParams).map((item) => item.label)).toEqual(path === "crm" ? ["Sales", "Pipeline"] : path === "customer-followups" ? ["Sales", "Today"] : ["Customers"]);
      expect(navigationLauncher(owner, label).results.some((result) => (result.item.id === activeId || activeId === "customer-timeline" && result.item.id === "customers-hub") && result.categoryId === "customers-sales")).toBe(true);
    }
    expect(navigationTrail(owner, base, `${base}/customers/customer-1`).map((item) => item.label)).toEqual(["Customers", "Customer profile"]);
    expect(navigationTrail(owner, base, `${base}/customer-followups/followup-1`).map((item) => item.label)).toEqual(["Sales", "Today", "Customer follow-up"]);
    expect(navigationLauncher(owner, "").categories.find((category) => category.id === "customers-sales")?.apps[0].label).toBe("Sales");
  });

  it("groups support with CRM without granting CRM access", async () => {
    const { navigationLauncher } = await import("../src/lib/navigation-catalog");
    const crmOnly = workplaceNavigation({ workspaceBase: base, permissions: ["commerce.read"], availableAppIds: ["crm"] });
    expect(crmOnly.categories.some((category) => category.id === "customers-sales")).toBe(true);
    expect(crmOnly.categories.some((category) => category.id === "projects-routines")).toBe(false);
    expect(crmOnly.categories.find((category) => category.id === "customers-sales")?.apps.map((app) => app.id)).toEqual(["sales-hub","customers-hub"]);
    const hidden = workplaceNavigation({ workspaceBase: base, permissions: ["commerce.read", "support.read"], availableAppIds: ["procurement", "support"] });
    expect(hidden.categories.find((category) => category.id === "purchasing")?.apps.map((app) => app.id)).toEqual(["operations-purchasing","procurement-rfqs","procurement-suppliers"]);
    expect(hidden.categories.find((category) => category.id === "customers-sales")?.apps.map((app) => app.id)).toEqual(["customers-hub"]);
    expect(navigationLauncher(hidden, "follow-ups").results.some((result) => result.categoryId === "customers-sales")).toBe(false);
    const denied = workplaceNavigation({ workspaceBase: base, permissions: [], availableAppIds: ["crm"] });
    expect(denied.apps.some((app) => app.id === "crm")).toBe(false);
    expect(denied.search.some((item) => item.id === "customer-followups")).toBe(false);
  });

  it("shows POS pages directly in the sidebar and launcher within the existing rollout and permissions", async () => {
    const { navigationLauncher } = await import("../src/lib/navigation-catalog");
    const pilot = workplaceNavigation({ workspaceBase: base, role: "holding_owner", availableAppIds: ["procurement", "pos", "finance"] });
    expect(pilot.categories.map((category) => category.id)).toEqual(["purchasing", "services", "finance", "pos", "knowledge", "administration"]);
    expect(pilot.categories.find((category) => category.id === "purchasing")?.apps.some((item) => item.id === "pos")).toBe(false);
    const posItems = pilot.categories.find((category) => category.id === "pos")!.apps;
    expect(posItems.map((item) => [item.id, item.href])).toEqual([
      ["pos-front", `${base}/pos`],
      ["pos-approvals", `${base}/pos?tab=history&queue=approval`],
      ["pos-reports", `${base}/pos?tab=reports`],
      ["pos-settings", `${base}/pos?tab=settings`],
    ]);
    expect(navigationLauncher(pilot, "").categories.find((category) => category.id === "pos")?.apps).toEqual(posItems);
    expect(navigationLauncher(pilot, "cashier").results.some((result) => result.item.id === "pos-front" && result.categoryId === "pos")).toBe(true);

    const hidden = workplaceNavigation({ workspaceBase: base, role: "holding_owner", availableAppIds: ["finance"] });
    expect(hidden.categories.some((category) => category.id === "pos")).toBe(false);
    for (const [scopeKind, workspaceBase] of [
      ["holding", base], ["branch", `${base}/branches/main`],
    ] as const) {
      const scoped = workplaceNavigation({ workspaceBase, scopeKind, permissions: ["pos.read"] });
      expect(scoped.categories.find((category) => category.id === "pos")?.apps.map((item) => item.id)).toEqual(["pos-front"]);
      expect(navigationLauncher(scoped, "").categories.find((category) => category.id === "pos")?.apps).toHaveLength(1);
    }
    const department = workplaceNavigation({ workspaceBase: `${base}/branches/main/departments/front`, scopeKind: "department", permissions: ["pos.read"] });
    expect(department.categories.some((category) => category.id === "pos")).toBe(false);
    const reports = workplaceNavigation({ workspaceBase: base, permissions: ["pos.read", "pos.reports"] });
    expect(reports.categories.find((category) => category.id === "pos")?.apps.map((item) => item.id)).toContain("pos-reports");
    const settings = workplaceNavigation({ workspaceBase: base, permissions: ["pos.read", "pos.payments.configure"] });
    expect(settings.categories.find((category) => category.id === "pos")?.apps.map((item) => item.id)).toContain("pos-settings");
  });

  it("shows PMS pages only when authorized and enabled, while the launcher keeps one app", async () => {
    const { navigationLauncher } = await import("../src/lib/navigation-catalog");
    const pilot = workplaceNavigation({ workspaceBase: base, role: "holding_owner", availableAppIds: ["pms"] });
    expect(pilot.categories.map((category) => category.id)).toEqual(["services", "pms", "knowledge", "administration"]);
    expect(pilot.categories.find(category=>category.id==="pms")?.apps).toHaveLength(4);
    expect(navigationLauncher(pilot, "").categories.find(category=>category.id==="pms")?.apps.map(item=>item.label)).toEqual(["Open PMS Front","PMS approvals","PMS reports","PMS settings"]);
    expect(navigationLauncher(pilot, "hotel").results.map((result) => result.item.id)).toContain("pms-front");
    const hidden = workplaceNavigation({ workspaceBase: base, role: "holding_owner", availableAppIds: ["finance"] });
    expect(hidden.categories.some((category) => category.id === "pms")).toBe(false);
    const housekeeper = workplaceNavigation({ workspaceBase: base, role: "pms_housekeeping", availableAppIds: ["pms"] });
    expect(housekeeper.categories.find((category) => category.id === "pms")?.apps.some((item) => item.id === "pms-reports")).toBe(false);
  });

  it("carries only the selected property between PMS sidebar pages", () => {
    const reports = owner.apps.find((app) => app.id === "pms")!.items.find((item) => item.id === "pms-reports")!;
    expect(pmsSidebarHref(reports, `${base}/pms/folios`, new URLSearchParams("property=hotel-1&folio=folio-2"))).toBe(`${base}/pms/reports?property=hotel-1`);
    expect(pmsSidebarHref(reports, `${base}/pms`, new URLSearchParams("property=hotel-1"))).toBe(`${base}/pms/reports?property=hotel-1`);
    expect(pmsSidebarHref(reports, `${base}/finance`, new URLSearchParams("property=hotel-1"))).toBe(reports.href);
  });

  it("shows shared system links only with destination access and rollout", () => {
    const actor = { role: "holding_owner", assignmentScope: "holding", permissions: ["pms.read", "commerce.read", "finance.read", "pos.read"] };
    expect(pmsCrossLinks(actor)).toEqual({ inventory: true, finance: true, pos: true });
    expect(pmsCrossLinks(actor, new Set(["pms", "finance"]))).toEqual({ inventory: false, finance: true, pos: false });
    expect(pmsCrossLinks({ ...actor, permissions: ["pms.read"] }, new Set(["pms", "inventory", "finance", "pos"]))).toEqual({ inventory: false, finance: false, pos: false });
  });

  it.each([
    ["holding_owner", true, true, true],
    ["hr_admin", true, true, false],
    ["it_admin", true, false, true],
    ["employee", false, false, false],
  ] as const)("retains %s authorization in Company, Branch and Department navigation", (role, iam, approvals, ai) => {
    for (const [scopeKind, workspaceBase] of [
      ["holding", base], ["branch", `${base}/branches/main`], ["department", `${base}/branches/main/departments/front`],
    ] as const) {
      const nav = workplaceNavigation({ workspaceBase, scopeKind, role, canManageApprovals: approvals, adminSections: ["approvals", "feedback", ...(role === "holding_owner" ? ["usage" as const] : []), ...(ai ? ["ai" as const] : [])] });
      const ids = nav.search.map(item => item.id);
      expect(ids.includes("technology-devices")).toBe(iam);
      expect(ids.includes("technology-roles")).toBe(iam);
      expect(ids.includes("administration-approvals")).toBe(approvals && scopeKind === "holding");
      expect(ids.includes("administration-ai")).toBe(ai && scopeKind === "holding");
      expect(ids.includes("administration-usage")).toBe(role === "holding_owner" && scopeKind === "holding");
      expect(ids.includes("projects-routines")).toBe(true);
      expect(ids.includes("handbook")).toBe(true);
      expect(ids.includes("learn")).toBe(true);
      expect(nav.apps.every(app => app.items.length > 0)).toBe(true);
      expect(new Set(nav.search.map(item => item.href)).size).toBe(nav.search.length);
      for (const item of nav.search) {
        expect(item.href.startsWith(`${workspaceBase}/`)).toBe(true);
        const url = new URL(item.href, "https://navigation.invalid");
        const parent=nav.primary.find(entry=>entry.submodules?.some(child=>child.id===item.id));
        if(!item.id.startsWith("start-"))expect([resolveNavigation(nav, workspaceBase, url.pathname, url.searchParams).activeId, resolveNavigation(nav, workspaceBase, url.pathname, url.searchParams).sidebarId]).toContain(parent?.id??item.id);
      }
    }
  });

  it("uses the server approval flag independently from the readable legacy section", () => {
    for (const canManageApprovals of [undefined, false, true]) {
      const nav = workplaceNavigation({ workspaceBase: base, role: "holding_owner", adminSections: ["approvals"], canManageApprovals });
      expect(nav.search.some(item => item.id === "administration-approvals")).toBe(canManageApprovals === true);
    }
    const staff = workplaceNavigation({ workspaceBase: base, adminSections: ["approvals", "feedback"], canManageApprovals: false });
    expect(resolveNavigation(staff, base, `${base}/settings`, new URLSearchParams("section=approvals"))).toMatchObject({ activeId: "projects-routines", groupId: "projects" });
    expect(staff.apps.some(app => app.id === "technology")).toBe(false);
  });

  it("places usage in Technology even without Finance access", () => {
    const billing = workplaceNavigation({ workspaceBase: base, permissions: [], adminSections: ["usage", "feedback"] });
    expect(billing.apps.find(app => app.id === "finance")).toBeUndefined();
    expect(billing.apps.find(app => app.id === "technology")).toMatchObject({ href: `${base}/settings?section=usage`, items: [expect.objectContaining({ id: "administration-usage" })] });
    expect(resolveNavigation(billing, base, `${base}/expenses/expense-1`).activeId).toBe("work");
    expect(resolveNavigation(billing, base, `${base}/settings`)).toMatchObject({ activeId: "administration-usage", groupId: "technology" });
    const branchBase = `${base}/branches/main`;
    const branch = workplaceNavigation({ workspaceBase: branchBase, scopeKind: "branch", role: "operations_manager", permissions: [], availableAppIds: ["people"] });
    expect(branch.apps.find(app => app.id === "technology")?.items.map(item => item.id)).toEqual(["branch-usage"]);
    expect(resolveNavigation(branch, branchBase, `${branchBase}/usage`)).toMatchObject({ activeId: "branch-usage", groupId: "technology" });
    const ai = workplaceNavigation({ workspaceBase: base, permissions: [], adminSections: ["ai", "feedback"] });
    expect(ai.apps.find(app => app.id === "technology")?.items.map(item => item.id)).toEqual(["administration-ai"]);
    const iam = workplaceNavigation({ workspaceBase: base, role: "custom", permissions: ["iam.read"] });
    expect(iam.apps.find(app => app.id === "technology")?.items).toHaveLength(6);
    const hr = workplaceNavigation({ workspaceBase: base, role: "custom", permissions: ["organization.manage"] });
    expect(hr.search.some(item => item.id === "people-directory")).toBe(true);
    expect(hr.search.some(item => item.id === "people-settings")).toBe(false);
    expect(hr.apps.find(app => app.id === "technology")?.items.map(item=>item.id)).toEqual(["technology-organization", "technology-business-readiness"]);
    const denied = workplaceNavigation({ workspaceBase: base, role: "holding_owner", permissions: [] });
    expect(denied.apps.some(app => ["finance", "technology"].includes(app.id))).toBe(false);
  });

  it("retains Beta and Platform feature organization", () => {
    const beta = workplaceNavigation({ workspaceBase: base, beta: true, role: "holding_owner", adminSections: [...sections] });
    expect(beta.apps.map(app => app.id)).toEqual(["executive", "people", "resources", "administration"]);
    expect(beta.apps.find(app => app.id === "people")!.items.map(item => item.id)).toEqual(["people-directory", "people-access"]);
    expect(beta.apps.find(app => app.id === "administration")!.items.map(item => item.id)).toEqual(["administration-people", "administration-organization", "administration-approvals", "administration-usage", "administration-ai"]);
    expect(beta.standalone).toEqual([]);
    const platform = workplaceNavigation({ workspaceBase: "/platform", scopeKind: "platform" });
    expect(platform.apps).toEqual([]);
    expect(platform.standalone.map(item => item.id)).toEqual(["platform-billing", "learn", "feed", "platform-iam", "my-access"]);
  });

  it("migrates retained expansion choices and defaults Technology to collapsed", () => {
    const groups = ["people", "finance", "projects", "technology"];
    expect(JSON.parse(navigationCollapsedValue(null, '["finance","resources","administration"]', groups))).toEqual(["finance", "technology"]);
    expect(JSON.parse(navigationCollapsedValue(null, "[]", groups))).toEqual(["technology"]);
    expect(JSON.parse(navigationCollapsedValue("[]", '["finance"]', groups))).toEqual([]);
    expect(JSON.parse(navigationCollapsedValue(null, null, groups))).toEqual(groups);
    expect(JSON.parse(navigationCollapsedValue(null, "invalid", groups))).toEqual(groups);
    // A role switch may temporarily hide Finance or Technology. Keep their choices.
    expect(JSON.parse(navigationCollapsedValue(null, '["finance","resources"]', ["people"]))).toEqual(["finance", "technology"]);
    expect(JSON.parse(navigationCollapsedValue(null, null, ["people"]))).toEqual(["people", "technology"]);
  });
});

 it("keeps Executive hidden without eligibility or a named share, and resolves report breadcrumbs",()=>{
   const base="/holdings/acme";
   const hidden=workplaceNavigation({workspaceBase:base,role:"holding_owner",executiveVisible:false});
   expect(hidden.apps.some(app=>app.id==="executive")).toBe(false);
   const shared=workplaceNavigation({workspaceBase:base,role:"employee",executiveVisible:true});
   expect(shared.apps.find(app=>app.id==="executive")?.items).toHaveLength(1);
   expect(resolveNavigation(shared,base,`${base}/executive/reports/private-id`).groupId).toBe("executive");
 });

it("keeps the concise My work views, reports and Team visibility", () => {
  const context = { workspaceBase: "/holdings/acme", scopeKind: "holding" as const, role: "holding_owner" };
  const views = (team: boolean, routing: boolean) => workplaceNavigation({ ...context, workViewAvailability: { team, routing } }).primary.find(item => item.id === "work")!.submodules!;
  expect(views(true, true).map(item => item.id)).toEqual([...workViews.filter(([id]) => ["actions", "approvals", "tracking", "team"].includes(id)).map(([id]) => `work-${id}`), "work-reports"]);
  expect(views(false, false).map(item => item.id)).toEqual(["work-actions", "work-approvals", "work-tracking", "work-reports"]);
  expect(views(true, false).some(item => item.id === "work-team")).toBe(true);
  expect(views(false, true).some(item => item.id === "work-routing")).toBe(false);
  for (const item of views(true, true)) expect(item.href).toBe(item.id === "work-reports" ? "/holdings/acme/work/reports" : `/holdings/acme/work?view=${item.id.slice(5)}`);
});
