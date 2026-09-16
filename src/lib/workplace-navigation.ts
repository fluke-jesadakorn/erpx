import { adminSections, type AdminSection } from "./administration";
import { iamSections } from "./iam";
import { hasPermission } from "./permissions";

export type NavigationScopeKind =
  | "platform"
  | "holding"
  | "branch"
  | "department";

export type NavigationContext = {
  workspaceBase: string;
  scopeKind?: NavigationScopeKind;
  beta?: boolean;
  role?: string;
  permissions?: readonly string[];
  adminSections?: AdminSection[];
};

export type NavigationIconId =
  | "home"
  | "work"
  | "inbox"
  | "people"
  | "commerce"
  | "finance"
  | "learn"
  | "settings"
  | "feedback"
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
};

export type NavigationGroup = {
  id: string;
  label: string;
  icon: NavigationIconId;
  href?: string;
  items: NavigationItem[];
  placement: "business" | "secondary";
  keywords?: string[];
};

export type WorkplaceNavigationModel = {
  primary: NavigationItem[];
  groups: NavigationGroup[];
  standalone: NavigationItem[];
  search: NavigationItem[];
};

type SearchParamsLike = Pick<URLSearchParams, "get">;

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
  const actor = { role, permissions: context.permissions };
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
  ];

  if (platform) {
    const standalone = [
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
      groups: [],
      standalone,
      search: uniqueDestinations([...primary, ...standalone]),
    };
  }

  const groups: NavigationGroup[] = [];
  const group = (
    id: string,
    label: string,
    icon: NavigationIconId,
    placement: NavigationGroup["placement"],
    items: NavigationItem[],
    href?: string,
    keywords?: string[],
  ) =>
    groups.push({
      id,
      label,
      icon,
      placement,
      items,
      ...(href ? { href } : {}),
      ...(keywords?.length ? { keywords } : {}),
    });

  const peopleItems = [
    item(
      "people-organization",
      "Organization",
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
    ...(hasPermission(actor, "iam.read")
      ? [
          item(
            "people-access",
            "Directory & access",
            "people/access",
            "People",
            "access",
            ["employee access"],
          ),
        ]
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
      "Operations",
      "commerce",
      "business",
      [
        item(
          "operations-quotations",
          commerceViewLabel("quote"),
          "commerce?view=quote",
          "Operations",
          "quotation",
          ["quotes", "sales"],
        ),
        item(
          "operations-purchasing",
          commerceViewLabel("purchase"),
          "commerce?view=purchase",
          "Operations",
          "purchasing",
          ["procurement", "purchasing"],
        ),
        item(
          "operations-stock",
          commerceViewLabel("stock"),
          "commerce?view=stock",
          "Operations",
          "stock",
          ["inventory"],
        ),
        ...(inventoryReadable
          ? [
              item(
                "operations-counts",
                "Stock counts",
                "stock-counts",
                "Operations",
                "stock-count",
                ["inventory adjustments"],
              ),
              item(
                "operations-transfers",
                "Warehouse transfers",
                "stock-transfers",
                "Operations",
                "stock-transfer",
                ["inventory transfers"],
              ),
            ]
          : []),
        item(
          "operations-products",
          commerceViewLabel("products"),
          "commerce?view=products",
          "Operations",
          "product",
          ["catalog"],
        ),
        item(
          "operations-warehouses",
          commerceViewLabel("warehouses"),
          "commerce?view=warehouses",
          "Operations",
          "warehouse",
          ["locations"],
        ),
        item(
          "operations-parties",
          commerceViewLabel("parties"),
          "commerce?view=parties",
          "Operations",
          "parties",
          ["partners", "customers", "suppliers"],
        ),
      ],
      `${base}/commerce`,
      ["sales orders", "sales", "purchasing", "inventory"],
    );
  }

  if (!beta && hasPermission(actor, "finance.read")) {
    group(
      "finance",
      "Finance",
      "finance",
      "business",
      [
        item(
          "finance-ledger",
          "Ledger & reports",
          "finance/ledger",
          "Finance",
          "ledger",
          ["journals", "general ledger", "period close"],
        ),
        item(
          "finance-invoices",
          "Invoices",
          "finance/invoices",
          "Finance",
          "invoice",
          ["receivables", "payables", "payments"],
        ),
        item(
          "finance-bank",
          "Bank reconciliation",
          "finance/bank",
          "Finance",
          "bank",
          ["bank statements"],
        ),
        item(
          "finance-expenses",
          "Employee expenses",
          "expenses",
          "Finance",
          "expense",
          ["expense claims", "reimbursements"],
        ),
      ],
      `${base}/finance`,
      ["finance overview", "accounting"],
    );
  }

  if (!beta) {
    group(
      "projects",
      "Projects",
      "projects",
      "business",
      [
        item(
          "projects-tasks",
          "Staff tasks",
          "staff-tasks",
          "Projects",
          "staff-task",
          ["tasks", "handoffs"],
        ),
        item(
          "projects-routines",
          "Routine templates",
          "routine-templates",
          "Projects",
          "routine",
          ["routines", "schedules"],
        ),
      ],
      `${base}/projects`,
      ["project list"],
    );
  }

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
      ...(!beta
        ? [item("learn", "Learn", "learn", "Resources", "education")]
        : []),
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
  const iamItem = hasPermission(actor, "iam.read")
    ? item(
        "administration-iam",
        "Identity & access",
        "iam",
        "Administration",
        "admin-iam",
        ["users", "roles", "permissions", "access matrix"],
      )
    : null;
  const administrationItems = [
    ...settingsItems,
    ...(iamItem ? [iamItem] : []),
  ];
  if (administrationItems.length) {
    const [landing, ...children] = administrationItems;
    group(
      "administration",
      "Administration",
      "settings",
      "secondary",
      children,
      landing.href,
      [landing.label, ...(landing.keywords ?? [])],
    );
  }

  const groupDestinations = groups.flatMap((entry) => [
    ...(entry.href
      ? [
          item(
            entry.id,
            entry.label,
            entry.href,
            entry.label,
            entry.icon,
            entry.keywords,
          ),
        ]
      : []),
    ...entry.items,
  ]);
  return {
    primary,
    groups,
    standalone: [],
    search: uniqueDestinations([...primary, ...groupDestinations]),
  };
}

function queryValue(searchParams: SearchParamsLike | undefined, key: string) {
  return searchParams?.get(key) ?? "";
}

function relativeSegments(base: string, pathname: string) {
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

function hasGroup(model: WorkplaceNavigationModel, id: string) {
  return model.groups.some((entry) => entry.id === id);
}

export function resolveNavigation(
  model: WorkplaceNavigationModel,
  base: string,
  pathname: string,
  searchParams?: SearchParamsLike,
): NavigationResolution {
  const [route, second, third] = relativeSegments(base, pathname);
  if (!route || route === "home")
    return { activeId: "home" };
  if (route === "feed")
    return { activeId: base === "/platform" ? "feed" : "home" };
  if (route === "posts")
    return { activeId: "home", detailLabel: "Update" };
  if (route === "inbox") return { activeId: "inbox" };
  if (route === "work") return { activeId: "work" };
  if (route === "requests")
    return { activeId: "work", detailLabel: second ? "Request" : undefined };
  if (route === "tasks")
    return {
      activeId: "work",
      detailLabel:
        second === "people_payroll" ? "Annual leave request" : "Task",
    };
  if (route === "people") {
    if (second === "organization")
      return { activeId: "people-organization", groupId: "people" };
    if (second === "access")
      return { activeId: "people-access", groupId: "people" };
    if (second === "payroll")
      return {
        activeId: "people-payroll",
        groupId: "people",
        detailLabel: third ? "Payroll run" : undefined,
      };
    if (queryValue(searchParams, "view") === "payroll")
      return { activeId: "people-payroll", groupId: "people" };
    return {
      activeId: "people",
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
      quote: "operations-quotations",
      purchase: "operations-purchasing",
      stock: "operations-stock",
      products: "operations-products",
      warehouses: "operations-warehouses",
      parties: "operations-parties",
    };
    return {
      activeId: byView[view] ?? "operations",
      groupId: "operations",
    };
  }
  if (route === "stock-counts")
    return {
      activeId: "operations-counts",
      groupId: "operations",
      detailLabel: second ? "Stock count" : undefined,
    };
  if (route === "stock-transfers")
    return {
      activeId: "operations-transfers",
      groupId: "operations",
      detailLabel: second ? "Warehouse transfer" : undefined,
    };
  if (route === "finance") {
    const bySection: Record<string, string> = {
      ledger: "finance-ledger",
      journals: "finance-ledger",
      invoices: "finance-invoices",
      payments: "finance-invoices",
      bank: "finance-bank",
    };
    const activeId = bySection[second ?? ""] ?? "finance";
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
    if (hasGroup(model, "finance"))
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
      activeId: "projects",
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
      groupId: "resources",
      detailLabel: second ? "Article" : undefined,
    };
  if (route === "learn")
    return {
      activeId: "learn",
      groupId: hasGroup(model, "resources") ? "resources" : undefined,
    };
  if (route === "settings") {
    const section = queryValue(searchParams, "section");
    const administration = model.groups.find(
      (entry) => entry.id === "administration",
    );
    if (!section)
      return administration
        ? { activeId: "administration", groupId: "administration" }
        : { activeId: "feedback", groupId: "resources" };
    if (section === "feedback")
      return { activeId: "feedback", groupId: "resources" };
    const landingSection = administration?.href
      ? new URL(administration.href, "https://navigation.invalid").searchParams.get(
          "section",
        )
      : null;
    if (section === landingSection)
      return {
        activeId: "administration",
        groupId: "administration",
        detailLabel: adminSections.find(([id]) => id === section)?.[1],
      };
    return {
      activeId: `administration-${section}`,
      groupId: "administration",
    };
  }
  if (route === "iam") {
    const administration = model.groups.find(
      (entry) => entry.id === "administration",
    );
    const iamIsLanding = administration?.href?.split("?")[0] === `${base}/iam`;
    return {
      activeId:
        base === "/platform"
          ? "platform-iam"
          : iamIsLanding
            ? "administration"
            : "administration-iam",
      groupId: base === "/platform" ? undefined : "administration",
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
  const group = model.groups.find((entry) => entry.id === resolved.groupId);
  if (!group)
    return resolved.detailLabel ? [{ label: resolved.detailLabel }] : [];
  const child = group.items.find((entry) => entry.id === resolved.activeId);
  const trail: NavigationTrailItem[] = [
    {
      label: group.label,
      ...(child && group.href ? { href: group.href } : {}),
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
