import { adminSections, type AdminSection } from "./administration";
import { iamSections } from "./iam";
import { hasPermission } from "./permissions";
import { visiblePmsSections } from "./pms-model";
import { peopleIamDestination } from "./people-lifecycle";

export type NavigationScopeKind =
  | "platform"
  | "holding"
  | "branch"
  | "department";

export type NavigationContext = {
  workspaceBase: string;
  /** Effective company rollout, supplied by the server for production navigation. */
  availableAppIds?: readonly string[];
  scopeKind?: NavigationScopeKind;
  beta?: boolean;
  role?: string;
  permissions?: readonly string[];
  adminSections?: AdminSection[];
  /** From administrationAccess.manageOrganization, not readable section access. */
  canManageApprovals?: boolean;
  executiveVisible?: boolean;
  manageErpxVisible?: boolean;
};

export type NavigationIconId =
  | "home"
  | "work"
  | "inbox"
  | "calendar"
  | "people"
  | "commerce"
  | "finance"
  | "pms"
  | "learn"
  | "settings"
  | "feedback"
  | "technology"
  | "projects"
  | "feed"
  | "organization"
  | "payroll"
  | "access"
  | "quotation"
  | "purchasing"
  | "stock"
  | "stock-count"
  | "stock-transfer"
  | "product"
  | "warehouse"
  | "parties"
  | "ledger"
  | "invoice"
  | "bank"
  | "expense"
  | "staff-task"
  | "routine"
  | "handbook"
  | "education"
  | "admin-people"
  | "admin-organization"
  | "admin-approvals"
  | "admin-usage"
  | "admin-ai"
  | "admin-iam";

export type NavigationItem = {
  id: string;
  label: string;
  href: string;
  group: string;
  icon: NavigationIconId;
  keywords?: string[];
  /** Direct destinations nested under a sidebar module. */
  submodules?: NavigationItem[];
};

export type NavigationApp = {
  id: string;
  label: string;
  icon: NavigationIconId;
  href?: string;
  items: NavigationItem[];
  placement: "business" | "secondary";
  keywords?: string[];
};

/** Sidebar-only grouping. Categories never become route or breadcrumb parents. */
export type NavigationCategory = {
  id: "people-organization" | "operations" | "erpx-management" | "pos" | "pms" | "finance" | "management" | "system";
  label: string;
  icon: NavigationIconId;
  apps: NavigationItem[];
};

export type WorkplaceNavigationModel = {
  primary: NavigationItem[];
  apps: NavigationApp[];
  categories: NavigationCategory[];
  standalone: NavigationItem[];
  search: NavigationItem[];
  settingsDefault?: AdminSection;
};

type SearchParamsLike = Pick<URLSearchParams, "get">;

/** Keep only the selected hotel when moving between PMS sections. */
export function pmsSidebarHref(item: NavigationItem, pathname: string, searchParams: SearchParamsLike) {
  if (!item.id.startsWith("pms-") || !/(?:^|\/)pms(?:\/|$)/.test(pathname)) return item.href;
  const property = searchParams.get("property");
  if (!property) return item.href;
  const url = new URL(item.href, "https://navigation.invalid");
  url.searchParams.set("property", property);
  return `${url.pathname}${url.search}`;
}

export type NavigationResolution = {
  activeId: string;
  groupId?: string;
  detailLabel?: string;
};

export type NavigationTrailItem = {
  href?: string;
  label: string;
};

export const commerceNavigationViews = [
  ["sales", "Sales orders"],
  ["quote", "Quotations"],
  ["pricing", "Price lists"],
  ["purchase", "Purchase orders"],
  ["stock", "Stock & movements"],
  ["products", "Products"],
  ["parties", "Customers & suppliers"],
  ["warehouses", "Warehouses"],
] as const;

const adminSectionIcons: Record<AdminSection, NavigationIconId> = {
  people: "admin-people",
  organization: "admin-organization",
  approvals: "admin-approvals",
  usage: "admin-usage",
  ai: "admin-ai",
  feedback: "feedback",
};

function commerceViewLabel(view: (typeof commerceNavigationViews)[number][0]) {
  return commerceNavigationViews.find(([id]) => id === view)![1];
}

const holdingRolesWithPayroll = new Set([
  "holding_owner",
  "hr_admin",
  "people_manager",
  "payroll_preparer",
  "payroll_approver",
  "auditor",
]);

function uniqueDestinations(items: NavigationItem[]) {
  const seen = new Set<string>();
  return items.filter((item) => {
    if (seen.has(item.href)) return false;
    seen.add(item.href);
    return true;
  });
}

export function workplaceNavigation(
  context: NavigationContext,
): WorkplaceNavigationModel {
  const {
    workspaceBase: base,
    beta = false,
    role = "employee",
    adminSections: allowedAdminSections = [],
  } = context;
  const scopeKind =
    context.scopeKind ?? (base === "/platform" ? "platform" : "holding");
  const platform = scopeKind === "platform";
  const visibleAdminSections =
    scopeKind === "holding" || platform
      ? allowedAdminSections
      : allowedAdminSections.filter((section) =>
          ["people", "organization", "feedback"].includes(section),
        );
  const actor = { role, permissions: context.permissions, assignmentScope: scopeKind };
  const item = (
    id: string,
    label: string,
    route: string,
    group: string,
    icon: NavigationIconId,
    keywords?: string[],
  ): NavigationItem => ({
    id,
    label,
    href: route.startsWith("/") ? route : `${base}/${route}`,
    group,
    icon,
    ...(keywords?.length ? { keywords } : {}),
  });

  const primary = [
    item("home", "Home", "home", "Workspace", "home"),
    item("work", "Work", "work", "Workspace", "work"),
    item("inbox", "Inbox", "inbox", "Workspace", "inbox"),
    ...(!platform && !beta ? [item("calendar", "Calendar", "calendar", "Workspace", "calendar", ["shifts", "leave", "appointments", "onboarding"])] : []),
  ];

  if (platform) {
    const standalone = [
      ...(context.executiveVisible
        ? [item("platform-executive", "Executive", "/platform/executive", "Platform", "finance", ["analytics", "provisioning", "health"])]
        : []),
      item("platform-billing", "Billing", "billing", "Platform", "settings", ["invoices", "support", "costs"]),
      item("learn", "Learn", "learn", "Platform", "learn"),
      item("feed", "Platform updates", "feed", "Platform", "feed", [
        "updates",
      ]),
      item(
        "platform-iam",
        "Identity & access",
        "iam",
        "Platform",
        "settings",
        ["users", "roles", "permissions"],
      ),
      item(
        "my-access",
        "My access",
        "/my-access",
        "Platform",
        "settings",
        ["security", "sessions"],
      ),
    ];
    return {
      primary,
      apps: [],
      categories: [],
      standalone,
      search: uniqueDestinations([...primary, ...standalone]),
    };
  }

  const apps: NavigationApp[] = [];
  const group = (
    id: string,
    label: string,
    icon: NavigationIconId,
    placement: NavigationApp["placement"],
    items: NavigationItem[],
    href?: string,
    keywords?: string[],
  ) =>
    items.length && apps.push({
      id,
      label,
      icon,
      placement,
      items,
      ...(href ? { href } : {}),
      ...(keywords?.length ? { keywords } : {}),
    });

  if ((context.executiveVisible ?? hasPermission(actor, "executive.access"))) {
    group("executive", "Executive", "finance", "business", [item("executive-canvas", "AI Live Canvas", "executive", "Executive", "finance", ["reports", "dashboard", "analytics", "AI", "widgets"])], `${base}/executive`);
  }

  const peopleItems = [
    item("people-directory", "People directory", "people", "People", "admin-people", ["employees", "staff", "profiles"]),
    ...(!beta ? [item("people-roster", "Roster", "people/roster", "People", "routine", ["shifts", "schedule", "swap"])] : []),
    item(
      "people-organization",
      "Organization & positions",
      "people/organization",
      "People",
      "organization",
      ["organization chart", "positions"],
    ),
    ...(scopeKind === "holding" &&
    !beta &&
    (holdingRolesWithPayroll.has(role) || hasPermission(actor, "payroll.read"))
      ? [
          item(
            "people-payroll",
            "Payroll",
            "people?view=payroll",
            "People",
            "payroll",
          ),
        ]
      : []),
    ...(beta && hasPermission(actor, "iam.read")
      ? [
          item(
            "people-access",
            "Access & devices",
            "people/access",
            "People",
            "access",
            ["employee access"],
          ),
        ]
      : []),
    ...(!beta ? [
      item("handbook", "Company handbook", "handbook", "People", "handbook", ["policies", "knowledge"]),
      item("learn", "Learn", "learn", "People", "education"),
    ] : []),
    ...(!beta && (hasPermission(actor, "iam.read") || hasPermission(actor, "organization.manage"))
      ? [item("people-settings", "People settings", "people/settings", "People", "settings", ["HR templates", "lifecycle handoff", "custom attributes"])]
      : []),
  ];
  group(
    "people",
    "People",
    "people",
    "business",
    peopleItems,
    `${base}/people`,
    ["directory", "employees"],
  );

  if (!beta && hasPermission(actor, "commerce.read")) {
    const inventoryReadable =
      scopeKind !== "department" && hasPermission(actor, "inventory.read");
    group(
      "operations",
      "Sales & purchasing",
      "commerce",
      "business",
      [
        item(
          "operations-sales",
          commerceViewLabel("sales"),
          "commerce",
          "Sales & purchasing",
          "invoice",
          ["orders", "sales"],
        ),
        item(
          "operations-quotations",
          commerceViewLabel("quote"),
          "commerce?view=quote",
          "Sales & purchasing",
          "quotation",
          ["quotes", "sales"],
        ),
        ...(hasPermission(actor,"commerce.sales") ? [item(
          "operations-pricing",
          commerceViewLabel("pricing"),
          "commerce?view=pricing",
          "Sales & purchasing",
          "invoice",
          ["price lists", "discount approvals"],
        )] : []),
        item(
          "operations-purchasing",
          commerceViewLabel("purchase"),
          "commerce?view=purchase",
          "Sales & purchasing",
          "purchasing",
          ["procurement", "purchasing"],
        ),
        item(
          "operations-parties",
          commerceViewLabel("parties"),
          "commerce?view=parties",
          "Sales & purchasing",
          "parties",
          ["partners", "customers", "suppliers"],
        ),
      ],
      `${base}/commerce`,
      ["operations", "sales orders", "procurement"],
    );
    group(
      "inventory",
      "Inventory",
      "warehouse",
      "business",
      [
        item(
          "operations-stock",
          commerceViewLabel("stock"),
          "commerce?view=stock",
          "Inventory",
          "stock",
          ["inventory"],
        ),
        ...(inventoryReadable
          ? [
              item(
                "operations-counts",
                "Stock counts",
                "stock-counts",
                "Inventory",
                "stock-count",
                ["inventory adjustments"],
              ),
              item(
                "operations-transfers",
                "Warehouse transfers",
                "stock-transfers",
                "Inventory",
                "stock-transfer",
                ["inventory transfers"],
              ),
            ]
          : []),
        item(
          "operations-products",
          commerceViewLabel("products"),
          "commerce?view=products",
          "Inventory",
          "product",
          ["catalog"],
        ),
        item(
          "operations-warehouses",
          commerceViewLabel("warehouses"),
          "commerce?view=warehouses",
          "Inventory",
          "warehouse",
          ["locations"],
        ),
      ],
      `${base}/commerce?view=stock`,
      ["stock", "products", "warehouses"],
    );
  }

  // Counts and transfers have their own read permission, independent of commerce.
  if (!beta && !hasPermission(actor, "commerce.read") && hasPermission(actor, "inventory.read")) {
    group("inventory", "Inventory", "warehouse", "business", [
      item("operations-counts", "Stock counts", "stock-counts", "Inventory", "stock-count"),
      item("operations-transfers", "Warehouse transfers", "stock-transfers", "Inventory", "stock-transfer"),
    ], `${base}/stock-counts`);
  }

  if (!beta && hasPermission(actor, "inventory.read") && scopeKind !== "department") {
    const inventory=apps.find(app=>app.id==='inventory');
    if(inventory)inventory.items.push(item('operations-transit','Stock in transit','stock-transit','Inventory','stock-transfer',['dispatch','receiving','discrepancy']));
  }

  if (!beta && hasPermission(actor, "commerce.read")) {
    group("crm", "Customers & CRM", "parties", "business", [item("crm-pipeline","Leads & opportunities","crm","Customers & CRM","quotation"),item("customer-timeline","Customer 360","customers","Customers & CRM","parties"),item("customer-followups","Customer follow-ups","customer-followups","Customers & CRM","calendar")], `${base}/crm`);
  }
  if (!beta && hasPermission(actor, "support.read")) {
    group("support", "Customer support", "feedback", "business", [item("support-cases","Support cases","support","Customer support","feedback")], `${base}/support`);
  }

  if (!beta && hasPermission(actor, "quality.read") && scopeKind !== "department") {
    group("quality", "Quality", "warehouse", "business", [item("quality-holds", "Quality holds", "quality", "Quality", "stock-count", ["inspection", "quarantine", "release"])], `${base}/quality`);
  }

  if (!beta && hasPermission(actor, "pos.read")) {
    group("pos", "POS", "commerce", "business", [
      item("pos-sale", "Point of sale", "pos", "POS", "product", ["cashier", "sales floor"]),
      item("pos-kitchen", "Kitchen", "pos?tab=kitchen", "POS", "routine"),
      item("pos-shifts", "Cash sessions", "pos?tab=shifts", "POS", "bank"),
      item("pos-history", "Receipts & refunds", "pos?tab=history", "POS", "invoice"),
      ...(hasPermission(actor, "pos.reports") ? [item("pos-reports", "POS reports", "pos?tab=reports", "POS", "ledger")] : []),
      ...(hasPermission(actor, "pos.configure") || hasPermission(actor, "pos.payments.configure") ? [item("pos-settings", "POS settings", "pos?tab=settings", "POS", "settings")] : []),
    ], `${base}/pos`);
  }

  if (!beta && scopeKind !== "department" && hasPermission(actor,"pms.read")) {
    const pmsIcons: Record<string, NavigationIconId> = {
      dashboard: "home", reservations: "quotation", "front-desk": "people",
      rooms: "warehouse", housekeeping: "staff-task", folios: "invoice",
      cashier: "bank", inventory: "stock", guests: "parties", reports: "ledger",
      "night-audit": "routine", settings: "settings",
    };
    group("pms", "Hotel PMS", "pms", "business", visiblePmsSections((permission) => hasPermission(actor, permission)).map(([key, label]) => item(`pms-${key}`, label, `pms/${key}`, "Hotel PMS", pmsIcons[key])), `${base}/pms/dashboard`, ["hotel", "reservation", "housekeeping", "rooms", "book room"]);
  }
  if (!beta) {
    const financeItems = hasPermission(actor, "finance.read") ? [
        item("finance-overview", "My work", "finance", "Finance", "home", ["finance work", "accounting"]),
        item("finance-receivables", "Receivables", "finance/invoices?kind=receivable&queue=open", "Finance", "invoice", ["accounts receivable", "collections"]),
        item("finance-payables", "Payables", "finance/invoices?kind=payable&queue=open", "Finance", "invoice", ["accounts payable", "supplier invoices"]),
        item("finance-bank", "Cash & banking", "finance/bank", "Finance", "bank", ["bank statements", "cash", "reconciliation", "bank reconciliation"]),
        item(
          "finance-ledger",
          "Accounting",
          "finance/accounting",
          "Finance",
          "ledger",
          ["journals", "general ledger"],
        ),
        item("finance-close", "Period close", "finance/close", "Finance", "routine", ["period close", "close checklist"]),
        item("finance-reports", "Reports", "finance/reports", "Finance", "ledger", ["trial balance", "financial statements", "aging"]),
        item("finance-engines", "Calculation engines", "finance/engines", "Finance", "routine", ["cash flow", "statutory", "goodwill", "NCI", "equity method", "manufacturing costing"]),
        ...(scopeKind === "holding" ? [item("finance-group", "Group reporting", "finance/group", "Finance", "organization", ["consolidation", "intercompany", "NCI"])] : []),
        item("finance-setup", "Setup", "finance/setup", "Finance", "settings", ["chart of accounts", "opening balances", "tax"]),
        item(
          "finance-expenses",
          "Employee expenses",
          "expenses",
          "Finance",
          "expense",
          ["expense claims", "reimbursements"],
        ),
    ] : [];
    if (visibleAdminSections.includes("usage")) financeItems.push(
      item("administration-usage", "Usage & billing", "settings?section=usage", "Finance", "admin-usage", ["subscription", "costs"]),
    );
    group("finance", "Finance", "finance", "business", financeItems, financeItems[0]?.href, ["accounting"]);
  }

  if (!beta) {
    group(
      "projects",
      "Projects & workflows",
      "projects",
      "business",
      [
        item("projects-list", "All projects", "projects", "Projects & workflows", "projects", ["project list"]),
        item(
          "projects-tasks",
          "Staff tasks",
          "staff-tasks",
          "Projects & workflows",
          "staff-task",
          ["tasks", "handoffs"],
        ),
        item(
          "projects-routines",
          "Routine templates",
          "routine-templates",
          "Projects & workflows",
          "routine",
          ["routines", "schedules"],
        ),
        ...(context.canManageApprovals && visibleAdminSections.includes("approvals") ? [
          item("administration-approvals", "Approvals", "settings?section=approvals", "Projects & workflows", "admin-approvals", ["approval queue", "requests"]),
        ] : []),
      ],
      `${base}/projects`,
      ["project list"],
    );

    const technologyItems = hasPermission(actor, "iam.read") ? [
      item("technology-positions", "Positions", "technology?view=positions", "Technology", "access", ["employee access", "position access"]),
      item("technology-employees", "Employees", "technology?view=employees", "Technology", "people", ["provisioning"]),
      item("technology-groups", "Directory & GPO", "technology?view=directory", "Technology", "people", ["Windows policies", "directory groups", "mappings"]),
      item("technology-devices", "Resources", "technology?view=resources", "Technology", "technology", ["equipment", "IT preparation"]),
      item("technology-connections", "Directory connections", "people/settings?section=connections", "Technology", "admin-iam", ["Active Directory", "Entra", "UCS"]),
      item("technology-roles", "Roles & permissions", "people/settings?section=roles", "Technology", "access", ["access matrix", "security"]),
      item("technology-reviews", "Reviews & audit", "people/settings?section=reviews", "Technology", "stock-count", ["access reviews", "history"]),
    ] : [];
    if (visibleAdminSections.includes("ai")) technologyItems.push(
      item("administration-ai", "AI providers", "settings?section=ai", "Technology", "admin-ai"),
    );
    if(scopeKind==='holding'&&hasPermission(actor,'organization.manage'))technologyItems.push(
      item('technology-business-readiness','Business readiness','settings/business-readiness','Technology','stock-count',['acceptance','A–J','release gates']),
    );
    group("technology", "Technology", "technology", "business", technologyItems, technologyItems[0]?.href);
  }

  if (beta) {
    group(
      "resources",
      "Resources",
      "learn",
      "secondary",
      [
        item(
          "handbook",
          "Company handbook",
          "handbook",
          "Resources",
          "handbook",
          ["policies", "knowledge"],
        ),
        item(
          "feedback",
          "Help & feedback",
          "settings?section=feedback",
          "Resources",
          "feedback",
          ["support", "reports"],
        ),
      ],
      undefined,
      ["knowledge", "support"],
    );

    const settingsItems = adminSections
      .filter(
        ([section]) =>
          section !== "feedback" && visibleAdminSections.includes(section),
      )
      .map(([section, label]) =>
        item(
          `administration-${section}`,
          label,
          `settings?section=${section}`,
          "Administration",
          adminSectionIcons[section],
        ),
      );
    const administrationItems = settingsItems;
    if (administrationItems.length) {
      group(
        "administration",
        "Administration",
        "settings",
        "secondary",
        administrationItems,
        administrationItems[0].href,
      );
    }
  }

  if (context.availableAppIds) {
    const allowed = new Set(context.availableAppIds);
    for (let index = apps.length - 1; index >= 0; index--)
      if (!allowed.has(apps[index].id)) apps.splice(index, 1);
    const finance = apps.find((app) => app.id === 'finance');
    if (finance) finance.items = finance.items.filter((entry) => !['finance-engines','finance-group'].includes(entry.id));
    const operations = apps.find((app) => app.id === 'operations');
    if (operations) operations.items = operations.items.filter((entry) => ['operations-purchasing','operations-parties'].includes(entry.id));
    const people = apps.find((app) => app.id === 'people');
    if (people) people.items = people.items.filter((entry) => entry.id !== 'people-payroll');
  }

  if (scopeKind === "holding" && base === "/holdings/erpx" && context.manageErpxVisible) {
    group("erpx-management", "ERPX Management", "technology", "business", [
      item("erpx-management", "Overview", "manage-erpx", "ERPX Management", "technology", ["customers", "provisioning", "recovery"]),
      item("manage-erpx-billing", "Customer billing", "manage-erpx/billing", "ERPX Management", "finance"),
      item("manage-erpx-iam", "Platform IAM", "manage-erpx/iam", "ERPX Management", "access"),
      item("manage-erpx-executive", "Platform Executive", "manage-erpx/executive", "ERPX Management", "projects"),
    ], `${base}/manage-erpx`, ["platform", "customers", "system"]);
  }

  const standalone = beta ? [] : [
    ...((!context.availableAppIds || context.availableAppIds.includes('legal')) && hasPermission(actor, "legal.read")
      ? [item("legal", "Legal", "legal", "Legal", "handbook", ["contracts", "cases", "compliance"])]
      : []),
    item("feedback", "Help & feedback", "settings?section=feedback", "Help & feedback", "feedback", ["support", "reports"]),
    ...(scopeKind==='holding' && hasPermission(actor,'organization.manage')
      ? [item('pilot-features','Pilot feature rollout','settings/pilot-features','Pilot feature rollout','settings',['availability','coming soon','hotel pilot'])]
      : []),
    item("future-features", "Future features", "future-features", "Future features", "learn", ["coming soon", "prototype", "roadmap"]),
  ];

  const appDestinations = apps.flatMap((entry) => entry.items.map((child) => ({
    ...child,
    keywords: [...(child.keywords ?? []), ...(entry.keywords ?? [])],
  })));
  const appItems = new Map(apps.map((app) => [app.id, {
    id: app.id,
    label: app.label,
    icon: app.icon,
    href: app.href ?? app.items[0].href,
    group: "Apps",
    keywords: app.keywords,
    ...(app.id === "people"
      ? { submodules: app.items.filter((entry) => entry.id !== "people-directory") }
      : {}),
  } satisfies NavigationItem]));
  const destinationItems = new Map(
    [...primary, ...standalone].map((entry) => [entry.id, entry]),
  );
  const category = (
    id: NavigationCategory["id"],
    label: string,
    icon: NavigationIconId,
    ids: string[],
  ): NavigationCategory | null => {
    const entries = ids.flatMap((entryId) => {
      const entry = appItems.get(entryId) ?? destinationItems.get(entryId);
      return entry ? [{ ...entry, group: label }] : [];
    });
    return entries.length ? { id, label, icon, apps: entries } : null;
  };
  const pos = apps.find((app) => app.id === "pos");
  const pms = apps.find((app) => app.id === "pms");
  const erpxManagementApp = apps.find((app) => app.id === "erpx-management");
  const erpxManagementCategory: NavigationCategory | null = erpxManagementApp
    ? {
        id: "erpx-management",
        label: "ERPX Management",
        icon: "technology",
        apps: erpxManagementApp.items,
      }
    : null;
  const categories = (beta
    ? [
        category("people-organization", "People & organization", "people", ["people", "resources"]),
        category("management", "Management", "projects", ["executive"]),
        category("system", "System", "technology", ["administration"]),
      ]
    : [
        category("people-organization", "People & organization", "people", ["people"]),
        category("operations", "Operations", "commerce", ["operations", "crm", "support", "inventory", "quality"]),
        erpxManagementCategory,
        ...(pos ? [{ id: "pos", label: "POS", icon: "commerce", apps: pos.items } satisfies NavigationCategory] : []),
        ...(pms ? [{ id: "pms", label: "Hotel PMS", icon: "pms", apps: pms.items } satisfies NavigationCategory] : []),
        category("finance", "Finance", "finance", ["finance"]),
        category("management", "Management", "projects", ["executive", "projects", "legal"]),
        category("system", "System", "technology", ["technology", "feedback", "pilot-features", "future-features"]),
      ]).filter((entry): entry is NavigationCategory => entry !== null);
  return {
    primary,
    apps,
    categories,
    standalone,
    search: uniqueDestinations([...primary, ...appDestinations, ...standalone]),
    settingsDefault: visibleAdminSections.some((section) => ["people", "usage", "ai"].includes(section)) ? visibleAdminSections[0] : "feedback",
  };
}

function queryValue(searchParams: SearchParamsLike | undefined, key: string) {
  return searchParams?.get(key) ?? "";
}

function relativeSegments(base: string, pathname: string | null) {
  pathname ??= `${base}/home`;
  const relative = pathname.startsWith(`${base}/`)
    ? pathname.slice(base.length + 1)
    : pathname.startsWith("/")
      ? pathname.slice(1)
      : pathname;
  return relative.split("/").filter(Boolean).map((segment) => {
    try {
      return decodeURIComponent(segment);
    } catch {
      return segment;
    }
  });
}

function hasApp(model: WorkplaceNavigationModel, id: string) {
  return model.apps.some((entry) => entry.id === id);
}

export function resolveNavigation(
  model: WorkplaceNavigationModel,
  base: string,
  pathname: string,
  searchParams?: SearchParamsLike,
): NavigationResolution {
  const [route, second, third] = relativeSegments(base, pathname);
  const standard = base !== "/platform" && !hasApp(model, "resources");
  if (!route || route === "home")
    return { activeId: "home" };
  if (route === "feed")
    return { activeId: base === "/platform" ? "feed" : "home" };
  if (route === "posts")
    return { activeId: "home", detailLabel: "Update" };
  if (route === "executive") return { activeId: "executive-canvas", groupId: "executive", detailLabel: second === "reports" ? "Report" : undefined };
  if (route === "legal") return { activeId: "legal" };
  if (route === "calendar") return { activeId: "calendar" };
  if (route === "inbox") return { activeId: "inbox" };
  if (route === "manage-erpx" && base === "/holdings/erpx") {
    const activeId =
      second === "billing"
        ? "manage-erpx-billing"
        : second === "iam"
          ? "manage-erpx-iam"
          : second === "executive"
            ? "manage-erpx-executive"
            : "erpx-management";
    return { activeId, groupId: "erpx-management" };
  }
  if (route === "billing" && base === "/platform") return { activeId: "platform-billing" };
  if (route === "my-access") return { activeId: "my-access" };
  if (route === "work") return { activeId: "work" };
  if (route === "future-features") return { activeId: "future-features" };
  if (route === "requests")
    return { activeId: "work", detailLabel: second ? "Request" : undefined };
  if (route === "tasks")
    return {
      activeId: "work",
      detailLabel:
        second === "people_payroll" ? "Annual leave request" : "Task",
    };
  if (route === "technology") return {activeId: ({positions:"technology-positions",employees:"technology-employees",directory:"technology-groups",resources:"technology-devices"} as Record<string,string>)[searchParams?.get("view")??"positions"]??"technology-positions",groupId:"technology"};
  if (route === "people") {
    if (second === "roster") return { activeId: "people-roster", groupId: "people" };
    if (second === "settings") {
      const section = queryValue(searchParams, "section");
      const technologyId: Record<string, string> = {
        connections: "technology-connections", roles: "technology-roles",
        matrix: "technology-roles", reviews: "technology-reviews",
        groups: "technology-groups",
      };
      if (standard && technologyId[section]) return { activeId: technologyId[section], groupId: "technology" };
      return { activeId: "people-settings", groupId: "people" };
    }
    if (second === "organization")
      return { activeId: "people-organization", groupId: "people" };
    if (second === "access") {
      const view = queryValue(searchParams, "view");
      return standard
        ? { activeId: `technology-${view === "devices" ? "devices" : ["groups", "policies"].includes(view) ? "groups" : "devices"}`, groupId: "technology" }
        : { activeId: "people-access", groupId: "people" };
    }
    if (second === "payroll")
      return {
        activeId: "people-payroll",
        groupId: "people",
        detailLabel: third ? "Payroll run" : undefined,
      };
    if (queryValue(searchParams, "view") === "payroll")
      return { activeId: "people-payroll", groupId: "people" };
    return {
      activeId: "people-directory",
      groupId: "people",
      detailLabel: second ? "Person" : undefined,
    };
  }
  if (route === "commerce") {
    if (second === "orders")
      return {
        activeId: "operations",
        groupId: "operations",
        detailLabel: "Order",
      };
    const view = queryValue(searchParams, "view");
    const byView: Record<string, string> = {
      sales: "operations-sales",
      quote: "operations-quotations",
      pricing: "operations-pricing",
      purchase: "operations-purchasing",
      stock: "operations-stock",
      products: "operations-products",
      warehouses: "operations-warehouses",
      parties: "operations-parties",
    };
    return {
      activeId: byView[view] ?? "operations-sales",
      groupId: ["stock", "products", "warehouses"].includes(view) ? "inventory" : "operations",
    };
  }
  if (route === "support") return {activeId:"support-cases",groupId:"support",detailLabel:second==="sla"?"Resolution-time watch":second==="incidents"?"Shared incidents":second?"Support case":undefined};
  if (route === "crm") return {activeId:"crm-pipeline",groupId:"crm",detailLabel:second?"CRM case":undefined};
  if (route === "customers") return {activeId:"customer-timeline",groupId:"crm",detailLabel:second?"Customer timeline":undefined};
  if (route === "customer-followups") return {activeId:"customer-followups",groupId:"crm",detailLabel:second?"Customer follow-up":undefined};
  if (route === "quality") return {activeId:"quality-holds",groupId:"quality",detailLabel:second?"Quality hold":undefined};
  if (route === "stock-transit") return {activeId:"operations-transit",groupId:"inventory",detailLabel:second?"Transport":undefined};
  if (route === "stock-counts")
    return {
      activeId: "operations-counts",
      groupId: "inventory",
      detailLabel: second ? "Stock count" : undefined,
    };
  if (route === "pos") return {activeId:`pos-${queryValue(searchParams, "tab") || "sale"}`,groupId:"pos"};
  if (route === "stock-transfers")
    return {
      activeId: "operations-transfers",
      groupId: "inventory",
      detailLabel: second ? "Warehouse transfer" : undefined,
    };
  if (route === "pms") return second === "documents"
    ? { activeId: "pms-folios", groupId: "pms", detailLabel: "Document" }
    : { activeId: `pms-${second ?? "dashboard"}`, groupId: "pms" };
  if (route === "finance") {
    const bySection: Record<string, string> = {
      ledger: "finance-ledger",
      accounting: "finance-ledger",
      journals: "finance-ledger",
      invoices: queryValue(searchParams, "kind") === "payable" ? "finance-payables" : "finance-receivables",
      payments: "finance-payables",
      bank: "finance-bank",
      close: "finance-close",
      reports: "finance-reports",
      engines: "finance-engines",
      group: "finance-group",
      setup: "finance-setup",
      refunds: "finance-payables",
      "supplier-batches": "finance-payables",
    };
    const activeId = bySection[second ?? ""] ?? "finance-overview";
    return {
      activeId,
      groupId: "finance",
      detailLabel:
        second === "journals"
          ? "Journal"
          : second === "payments"
            ? "Payment"
            : second === "invoices" && third
              ? "Invoice"
              : undefined,
    };
  }
  if (route === "expenses") {
    if (model.search.some((item) => item.id === "finance-expenses"))
      return {
        activeId: "finance-expenses",
        groupId: "finance",
        detailLabel: second ? "Expense claim" : undefined,
      };
    return {
      activeId: "work",
      detailLabel: second ? "Expense claim" : undefined,
    };
  }
  if (route === "projects")
    return {
      activeId: "projects-list",
      groupId: "projects",
      detailLabel: second ? "Project" : undefined,
    };
  if (route === "staff-tasks")
    return {
      activeId: "projects-tasks",
      groupId: "projects",
      detailLabel: second ? "Task" : undefined,
    };
  if (route === "routine-templates")
    return {
      activeId: "projects-routines",
      groupId: "projects",
      detailLabel: second ? "Routine template" : undefined,
    };
  if (route === "handbook")
    return {
      activeId: "handbook",
      groupId: standard ? "people" : "resources",
      detailLabel: second ? "Article" : undefined,
    };
  if (route === "learn")
    return {
      activeId: "learn",
      groupId: standard ? "people" : hasApp(model, "resources") ? "resources" : undefined,
    };
  if (route === "settings") {
    if(second==='pilot-features')return {activeId:'pilot-features'};
    if(second==='business-readiness')return {activeId:'technology-business-readiness',groupId:'technology'};
    const section = queryValue(searchParams, "section") || (standard ? model.settingsDefault : "");
    if (standard) {
      if (section === "feedback") return { activeId: "feedback" };
      if (section === "usage") return { activeId: "administration-usage", groupId: "finance" };
      if (section === "ai") return { activeId: "administration-ai", groupId: "technology" };
      if (section === "approvals") return model.search.some((item) => item.id === "administration-approvals")
        ? { activeId: "administration-approvals", groupId: "projects" }
        : { activeId: "projects-routines", groupId: "projects" };
      return { activeId: section === "organization" ? "people-organization" : "people-settings", groupId: "people" };
    }
    const administration = model.apps.find(
      (entry) => entry.id === "administration",
    );
    if (!section)
      return administration
        ? { activeId: administration.items[0].id, groupId: "administration" }
        : { activeId: "feedback", groupId: "resources" };
    if (section === "feedback")
      return { activeId: "feedback", groupId: "resources" };
    if (["people", "organization"].includes(section) && !administration?.items.some((entry) => entry.id === `administration-${section}`))
      return { activeId: section === "people" ? "people-settings" : "people-organization", groupId: "people" };
    return {
      activeId: `administration-${section}`,
      groupId: "administration",
    };
  }
  if (route === "iam") {
    if (base !== "/platform") {
      const destination = new URL(`${base}/${peopleIamDestination(second ?? "overview")}`, "https://navigation.invalid");
      return resolveNavigation(model, base, destination.pathname, destination.searchParams);
    }
    return {
      activeId: "platform-iam",
      detailLabel:
        second && second !== "overview"
          ? iamSections.find(([id]) => id === second)?.[1]
          : undefined,
    };
  }
  return { activeId: "" };
}

export function navigationTrail(
  model: WorkplaceNavigationModel,
  base: string,
  pathname: string,
  searchParams?: SearchParamsLike,
): NavigationTrailItem[] {
  if (pathname.replace(/\/$/, "") === `${base}/settings/stripe`)
    return [{ label: "Company Stripe" }];
  const resolved = resolveNavigation(model, base, pathname, searchParams);
  if (!resolved.activeId || resolved.activeId === "home") return [];
  const primary = model.primary.find((entry) => entry.id === resolved.activeId);
  if (primary)
    return [
      {
        label: primary.label,
        href: resolved.detailLabel ? primary.href : undefined,
      },
      ...(resolved.detailLabel ? [{ label: resolved.detailLabel }] : []),
    ];
  const standalone = model.standalone.find(
    (entry) => entry.id === resolved.activeId,
  );
  if (standalone) return [{ label: standalone.label }];
  const app = model.apps.find((entry) => entry.id === resolved.groupId);
  if (!app)
    return resolved.detailLabel ? [{ label: resolved.detailLabel }] : [];
  const child = app.items.find((entry) => entry.id === resolved.activeId);
  const trail: NavigationTrailItem[] = [
    {
      label: app.label,
      ...(child && app.href ? { href: app.href } : {}),
    },
  ];
  if (child)
    trail.push({
      label: child.label,
      ...(resolved.detailLabel ? { href: child.href } : {}),
    });
  if (resolved.detailLabel) trail.push({ label: resolved.detailLabel });
  return trail;
}

/** Backwards-compatible root resolver for callers that only need highlighting. */
export function activeDestination(
  base: string,
  pathname: string,
  searchParams?: SearchParamsLike,
) {
  const model = workplaceNavigation({ workspaceBase: base });
  const resolved = resolveNavigation(model, base, pathname, searchParams);
  return (resolved.groupId ?? resolved.activeId) || "more";
}

export function navigationCategory(
  model: WorkplaceNavigationModel,
  resolved: NavigationResolution,
) {
  const activeAppId = resolved.groupId ?? resolved.activeId;
  return model.categories.find((entry) =>
    entry.id === activeAppId || entry.apps.some((app) => app.id === activeAppId),
  );
}

/** Carry forward retained groups' v2 preferences; new Technology starts closed. */
export function navigationCollapsedValue(
  current: string | null,
  legacy: string | null,
  groupIds: string[],
) {
  if (current !== null) return current;
  if (legacy !== null) {
    try {
      const previous: unknown = JSON.parse(legacy);
      if (Array.isArray(previous)) return JSON.stringify([...new Set([
        ...previous.filter((id): id is string => typeof id === "string" && !["resources", "administration"].includes(id)),
        "technology",
      ])]);
    } catch { /* Fall back to the default collapsed groups. */ }
  }
  return JSON.stringify([...new Set([...groupIds, "technology"])]);
}
