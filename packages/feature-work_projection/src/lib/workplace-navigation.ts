import {destinationAvailable} from '@erpx/feature-work_projection/lib/destination-access';
import { productOfficeNavigation } from "@erpx/feature-work_projection/lib/product-office-navigation";
import type { ProductHostKey } from "@erpx/contracts/product-registry";
import {hasModuleAccess} from "@erpx/catalogue/module-access";
import {destinations, parentLabels, canonicalUiHref, destinationForPath, destinationTrail, selectionForPath, sourcePath} from '@erpx/feature-work_projection/lib/destination-registry';
import {sectionWorkspaceTopics,legacyWorkspaceDestination} from "@erpx/feature-work_projection/lib/section-workspaces";
import { hrSettingsTopics } from "@erpx/feature-people_payroll/lib/hr-settings-navigation";
import { technologySections } from "@erpx/feature-identity_access/lib/technology-navigation";
import { workViews, workViewAvailable, type WorkViewAvailability } from "@erpx/feature-work_projection/lib/work-item";
import { adminSections, type AdminSection } from "@erpx/feature-shared/lib/administration";
import { iamSections } from "@erpx/feature-identity_access/lib/iam";
import { hasPermission } from "@erpx/feature-identity_access/lib/permissions";
import { salesCustomerAccess, salesCustomerLegacyDestination } from "@erpx/feature-sales/lib/navigation";
import { peopleIamDestination } from "@erpx/feature-people_payroll/lib/people-iam-navigation";
import { financeDestinationForRoute, financeNavigation, financeRootItems, financeSidebarId } from "@erpx/feature-finance/lib/finance-navigation";
import { workplaceCategories, workplaceModuleDestinations, workplaceStartActions, type WorkplaceCategoryId, type AiSupport } from "@erpx/feature-work_projection/lib/workplace-catalogue";

export type NavigationScopeKind =
  | "platform"
  | "holding"
  | "branch"
  | "department";

export type NavigationContext = {
  workspaceBase: string;
  /** Exact server-selected product host; absent on the compatibility Console. */
  productId?: ProductHostKey;
  workViewAvailability?: WorkViewAvailability | null;
  handbookAccess?: {editor:boolean;owner:boolean};
  /** Daily POS/PMS links are rendered in the separate staff Front surface. */
  frontPosHref?: string;
  frontPmsHref?: string;
  /** Effective company rollout, supplied by the server for production navigation. */
  availableAppIds?: readonly string[];
  /** Module grants remain separate from display categories. */
  moduleIds?: readonly string[];
  financeWorkspaceV2?: boolean;
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
  openInNewTab?: boolean;
  keywords?: string[];
  categoryId?: WorkplaceCategoryId | "daily";
  ownerModuleId?: string;
  objective?: string;
  aiSupport?: AiSupport;
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
  id: WorkplaceCategoryId;
  label: string;
  icon: NavigationIconId;
  apps: NavigationItem[];
  secondaryItems?: NavigationItem[];
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
  /** Presentation-only parent selection; the destination still owns breadcrumbs. */
  sidebarId?: string;
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
  ["parties", "Directory"],
  ["warehouses", "Warehouses"],
] as const;

const adminSectionIcons: Record<AdminSection, NavigationIconId> = {
  brand: "settings",
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
          ["people", "organization", "brand", "feedback"].includes(section),
        );
  const actor = { role, permissions: context.permissions, assignmentScope: scopeKind };
  const item = (
    id: string,
    label: string,
    route: string,
    group: string,
    icon: NavigationIconId,
    keywords?: string[],
    openInNewTab = false,
  ): NavigationItem => ({
    id,
    label,
    href: canonicalUiHref(route.startsWith("/") ? route : canonicalUiHref(`${base}/${route}`)),
    group,
    icon,
    ...(openInNewTab ? { openInNewTab: true } : {}),
    ...(keywords?.length ? { keywords } : {}),
  });

  const primary = platform
    ? [
        item("home", "Home", "home", "Workspace", "home"),
        item("work", "Work", "work", "Workspace", "work"),
        item("inbox", "Inbox", "inbox", "Workspace", "inbox"),
      ]
    : [
        item("home", "Today", "home", "Workspace", "home", ["work", "my day", "feed", "tasks"]),
        item("work", "My work", "work", "Workspace", "work", ["actions", "approvals", "tracking", "requests", "for me"]),
        item("calendar", "Calendar", "calendar", "Workspace", "calendar", ["shifts", "leave", "appointments", "onboarding"]),
        item("inbox", "Messages", "inbox", "Workspace", "inbox", ["chat", "notifications", "teams"]),
      ];
  if (!platform) {
    primary.find((entry) => entry.id === "work")!.submodules = workViews

      .filter(([id]) => context.workViewAvailability === undefined || workViewAvailable(id, context.workViewAvailability))
      .map(([id, label]) => item(`work-${id}`, label, `work?view=${id}`, "My work", "work"));
    primary.find((entry) => entry.id === "work")!.submodules!.push(item("work-reports", "Work reports", "work/reports", "My work", "work"));
    const inbox = primary.find((entry) => entry.id === "inbox")!;
    inbox.submodules = [
      item("inbox-chats", "Messages", "inbox?tab=chats", "Inbox", "inbox"),
      item("inbox-teams", "Teams", "inbox?tab=teams", "Inbox", "inbox"),
      item("inbox-updates", "Notifications", "inbox?tab=updates", "Inbox", "inbox"),
    ];
    const calendar = primary.find((entry) => entry.id === "calendar");
    if (calendar) calendar.submodules = [
      item("calendar-mine", "My calendar", "calendar?mode=mine", "Calendar", "calendar"),
      item("calendar-team", "Team calendar", "calendar?mode=team", "Calendar", "calendar"),
      item("calendar-coverage", "Coverage", "calendar?mode=team&preset=coverage", "Calendar", "calendar"),
    ];
  }

  if (platform) {
    const standalone = [
      ...(context.executiveVisible
        ? [item("platform-executive", "Executive", canonicalUiHref("/platform/executive"), "Platform", "finance", ["analytics", "provisioning", "health"])]
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
      search: uniqueDestinationIds([...primary, ...standalone]),
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
    group("executive", "Executive", "finance", "business", [item("executive-canvas", "AI Live Canvas", "executive", "Executive", "finance", ["reports", "dashboard", "analytics", "AI", "widgets"])], canonicalUiHref(`${base}/executive`));
  }

  const peopleItems = [
    item("people-directory", "People", "people?view=internal", "People", "admin-people", ["employees", "staff", "profiles", "organization", "positions", "archived profiles", "roster", "shifts", "schedule", "HR settings", "leave policy", "pay groups"]),
    ...(!beta && scopeKind === "holding" && (hasPermission(actor,"leave.policy.manage") || hasPermission(actor,"people.payroll.details") || (hasPermission(actor,"people.manage") && hasPermission(actor,"organization.manage"))) ? [item("people-settings", "HR settings", "people/settings", "People", "settings", ["staff levels", "leave policy", "holiday calendars", "pay groups", "templates", "lifecycle handoff"])] : []),
    ...(!beta && hasPermission(actor,"iam.read") && scopeKind === "holding" ? [item("people-users-access", "Users & Access", "people?view=users", "People", "access", ["users", "third party", "contractor", "external organizations", "external teams", "IAM", "permissions"])] : []),
    ...(!beta && hasPermission(actor,"people.read") ? [item("people-evaluation", "Skills & performance", "people/evaluation", "People", "admin-people", ["evaluation", "compensation", "skills", "pay bands", "annual review"])] : []),
    ...(!beta && hasPermission(actor,"roster.read") ? [item("people-roster","Roster","people/roster","People","calendar",["staffing","leave","overtime","coverage"])] : []),
    ...(scopeKind === "holding" && !beta &&
      (holdingRolesWithPayroll.has(role) || hasPermission(actor, "payroll.read") || hasPermission(actor, "payroll.payment.read"))
      ? [
          item("people-payroll", "Payroll", "people/payroll", "People", "payroll", ["salary", "wages", "social security", "pay runs", "current run"]),
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
  ];
  group(
    "people",
    "People",
    "people",
    "business",
    peopleItems,
    canonicalUiHref(`${base}/people`),
    ["directory", "employees"],
  );

  if (!beta && hasPermission(actor, "commerce.read")) {
    const inventoryReadable =
      scopeKind !== "department" && hasPermission(actor, "inventory.read");
    group(
      "sales",
      "Sales",
      "commerce",
      "business",
      [
        item(
          "operations-sales",
          commerceViewLabel("sales"),
          "commerce",
          "Sales",
          "invoice",
          ["orders", "sales"],
        ),
        item(
          "operations-quotations",
          commerceViewLabel("quote"),
          "commerce?view=quote",
          "Sales",
          "quotation",
          ["quotes", "sales"],
        ),
        ...(hasPermission(actor,"commerce.sales") ? [item(
          "operations-pricing",
          commerceViewLabel("pricing"),
          "commerce?view=pricing",
          "Sales",
          "invoice",
          ["price lists", "discount controls", "pricing rules"],
        )] : []),
      ],
      canonicalUiHref(`${base}/commerce`),
      ["sales orders", "quotations", "pricing"],
    );
    group(
      "procurement",
      "Purchasing",
      "purchasing",
      "business",
      [
        item(
          "operations-purchasing",
          commerceViewLabel("purchase"),
          "commerce?view=purchase",
          "Procurement",
          "purchasing",
          ["procurement", "purchasing"],
        ),
        item(
          "procurement-rfqs",
          "Supplier quotation requests",
          "procurement-rfqs",
          "Procurement",
          "quotation",
          ["RFQ", "supplier offers"],
        ),
        item(
          "procurement-suppliers",
          "Suppliers",
          "commerce?view=parties&kind=supplier",
          "Procurement",
          "parties",
          ["vendors", "supplier directory"],
        ),
      ],
      canonicalUiHref(`${base}/commerce?view=purchase`),
      ["purchasing", "suppliers", "RFQ"],
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
      canonicalUiHref(`${base}/commerce?view=stock`),
      ["stock", "products", "warehouses"],
    );
  }

  // Counts and transfers have their own read permission, independent of commerce.
  if (!beta && !hasPermission(actor, "commerce.read") && hasPermission(actor, "inventory.read")) {
    group("inventory", "Inventory", "warehouse", "business", [
      item("operations-counts", "Stock counts", "stock-counts", "Inventory", "stock-count"),
      item("operations-transfers", "Warehouse transfers", "stock-transfers", "Inventory", "stock-transfer"),
    ], canonicalUiHref(`${base}/stock-counts`));
  }

  if (!beta && hasPermission(actor, "inventory.read") && scopeKind !== "department") {
    const inventory=apps.find(app=>app.id==='inventory');
    if(inventory)inventory.items.push(item('operations-transit','Stock in transit','stock-transit','Inventory','stock-transfer',['dispatch','receiving','discrepancy']));
  }

  if (!beta && hasPermission(actor, "commerce.read")) {
    group("crm", "CRM", "parties", "business", [item("customer-timeline","Customers","customers","CRM","parties",["Customer 360","accounts","contacts","directory"]),item("crm-pipeline","Pipeline","crm","CRM","quotation",["leads","opportunities","activities"]),item("customer-followups","Follow-ups","customer-followups","CRM","calendar"),item("crm-customers","Customer directory","commerce?view=parties&kind=customer","CRM","parties",["customers","contacts"])], canonicalUiHref(`${base}/customers`));
  }
  if (!beta && hasPermission(actor, "support.read")) {
    group("support", "Customer support", "feedback", "business", [item("support-cases","Support cases","support","Customer support","feedback")], canonicalUiHref(`${base}/support`));
  }

  if (!beta && hasPermission(actor, "quality.read") && scopeKind !== "department") {
    group("quality", "Quality", "warehouse", "business", [item("quality-holds", "Quality holds", "quality", "Quality", "stock-count", ["inspection", "quarantine", "release"])], canonicalUiHref(`${base}/quality`));
  }

  if (!beta && hasPermission(actor, "pos.read")) {
    group("pos", "POS", "commerce", "business", [
      item("pos-front", "Open POS Front", context.frontPosHref ?? canonicalUiHref(`${base}/pos`), "POS", "product", ["cashier", "sales floor"], true),
      ...(hasPermission(actor, "pos.approve") ? [item("pos-approvals", "POS approvals", "pos?tab=history&queue=approval", "POS", "admin-approvals")] : []),
      ...(hasPermission(actor, "pos.reports") ? [item("pos-reports", "POS reports", "pos?tab=reports", "POS", "ledger")] : []),
      ...(hasPermission(actor, "pos.configure") || hasPermission(actor, "pos.payments.configure") ? [item("pos-settings", "POS settings", "pos?tab=settings", "POS", "settings")] : []),
    ], context.frontPosHref ?? canonicalUiHref(`${base}/pos`));
  }

  if (!beta && scopeKind !== "department" && hasPermission(actor,"pms.read")) {
    group("pms", "Hotel PMS", "pms", "business", [
      item("pms-front", "Open PMS Front", context.frontPmsHref ?? canonicalUiHref(`${base}/pms/dashboard`), "Hotel PMS", "pms", ["hotel", "reservation", "housekeeping", "rooms", "book room"], true),
      ...(hasPermission(actor, "pms.approve") ? [item("pms-approvals", "PMS approvals", "pms/guest-accounts?queue=approval", "Hotel PMS", "admin-approvals")] : []),
      ...(hasPermission(actor, "pms.reports") ? [item("pms-reports", "PMS reports", "pms/reports", "Hotel PMS", "ledger")] : []),
      ...(hasPermission(actor, "pms.configure") || hasPermission(actor, "pms.accounting.prepare") || hasPermission(actor, "pms.accounting.approve") ? [item("pms-settings", "PMS settings", "pms/settings", "Hotel PMS", "settings")] : []),
    ], context.frontPmsHref ?? canonicalUiHref(`${base}/pms/dashboard`), ["hotel", "reservation", "housekeeping", "rooms", "book room"]);
  }
  if (!beta) {
    const financeItems = hasPermission(actor, "finance.read") ? [
        item("finance-overview", "Finance work", "finance", "Finance", "home", ["finance work", "accounting"]),
        item("finance-sales", "Sales & collections", "finance/sales", "Finance", "invoice", ["quotations", "sales orders", "invoices", "receipts"]),
        item("finance-receivables", "Receivables", "finance/invoices?kind=receivable&queue=open", "Finance", "invoice", ["accounts receivable", "collections"]),
        item("finance-payables", "Payables", "finance/invoices?kind=payable&queue=open", "Finance", "invoice", ["accounts payable", "supplier invoices"]),
        item("finance-bank", "Cash & banking", "finance/bank", "Finance", "bank", ["bank statements", "cash", "reconciliation", "bank reconciliation"]),
        ...(scopeKind!=="department"&&hasPermission(actor,"pms.reports")?[item("finance-hotels","Hotel accounting","finance/hotels","Finance","pms",["hotel receivables","PMS GL","advances","deposits"])]:[]),
        item(
          "finance-ledger",
          "Accounting",
          "finance/accounting",
          "Finance",
          "ledger",
          ["journals", "general ledger"],
        ),
        item("finance-close", "Period close", "finance/close", "Finance", "routine", ["period close", "close checklist"]),
        item("finance-tax", "Tax", "finance/tax", "Finance", "ledger", ["VAT", "ภ.พ.30", "tax register"]),
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
    const financeEntries = context.financeWorkspaceV2 ? financeNavigation(context) : financeItems;
    group("finance", "Finance", "finance", "business", financeEntries, financeEntries[0]?.href, ["accounting"]);
  }

  if (!beta) {
    group(
      "projects",
      "Projects & Routines",
      "projects",
      "business",
      [
        item("projects-list", "All projects", "projects", "Projects & Routines", "projects", ["project list"]),
        item(
          "projects-tasks",
          "Staff tasks",
          "staff-tasks",
          "Projects & Routines",
          "staff-task",
          ["tasks", "handoffs"],
        ),
        item(
          "projects-routines",
          "Routine templates",
          "routine-templates",
          "Projects & Routines",
          "routine",
          ["routines", "schedules"],
        ),
      ],
      canonicalUiHref(`${base}/projects`),
      ["project list"],
    );

    if (context.moduleIds?.includes("operations") && hasPermission(actor, "records.read") &&
        (!context.availableAppIds || context.availableAppIds.includes("operations"))) {
      group("operations", "Operations", "projects", "business", [
        item("operations", "Operations", "operations", "Operations", "projects"),
      ], canonicalUiHref(`${base}/operations`));
    }

    const technologyItems = hasPermission(actor, "iam.read") ? [
      item("technology-organization", "Organization & Access", "technology?view=organization", "Technology", "access", ["positions", "employees", "levels", "departments", "Directory", "GPO", "provisioning", "Windows policies", "mappings"]),
      item("technology-devices", "Resources", "technology?view=resources", "Technology", "technology", ["equipment", "IT preparation"]),
      item("technology-directory", "Directory", "technology?view=directory", "Technology", "admin-iam", ["connections", "Active Directory", "Entra", "UCS", "OU", "field mappings", "verification"]),
      item("technology-settings", "Access settings", "technology?view=settings", "Technology", "access", ["custom attributes", "roles", "permissions", "access matrix", "group mappings", "reviews", "audit"]),
    ] : [];
    if (!hasPermission(actor, "iam.read") && ["people.read", "people.manage", "organization.manage"].some(permission => hasPermission(actor, permission))) technologyItems.unshift(
      item("technology-organization", "Organization & Access", "technology?view=organization", "Technology", "access", ["positions", "employees", "levels", "departments", "GPO"]),
    );
    if (visibleAdminSections.includes("ai")) technologyItems.push(
      item("administration-ai", "AI providers", "settings?section=ai", "Technology", "admin-ai"),
    );
    if (context.canManageApprovals && visibleAdminSections.includes("approvals")) technologyItems.push(
      item("administration-approvals", "Approval routing", "settings?section=approvals", "Administration", "admin-approvals", ["approval settings", "routes", "policies"]),
    );
    if (visibleAdminSections.includes("usage")) technologyItems.push(
      item("administration-usage", "Usage & billing", "settings?section=usage", "Technology", "admin-usage", ["subscription", "costs"]),
    );
    if (scopeKind === "branch" && role === "operations_manager") technologyItems.push(
      item("branch-usage", "Usage", "usage", "Technology", "admin-usage", ["AI", "files", "storage"]),
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

  if (!beta) {
    group("guides", "Knowledge", "learn", "secondary", [
      item("learn", "Guides", "learn", "Knowledge", "learn", ["help using ERPX", "SOP", "FAQs", "how to"]),
      item("handbook", "Company handbook", "handbook", "Knowledge", "handbook", ["policies", "knowledge"]),
    ]);
    // These owner pages require both a module grant and records.read.
    const modules = context.moduleIds ?? [];
    const extraModules = [
      ["talent", "Talent", "talent", "people"], ["benefits", "Benefits administration", "benefits", "people"],
      ["marketing", "Marketing", "marketing", "feed"], ["ecommerce", "E-commerce / Channels", "ecommerce", "commerce"],
      ["assets", "Assets", "assets", "technology"], ["maintenance", "Maintenance", "maintenance", "technology"],
      ["field_service", "Field service", "field-service", "technology"], ["planning", "Planning", "planning", "calendar"],
      ["manufacturing", "Manufacturing", "manufacturing", "product"], ["learning", "My learning & courses", "learning", "education"],
      ["agent", "Agent governance", "agents", "admin-ai"],
    ] as const;
    for (const [module, label, route, icon] of extraModules) {
      if (modules.includes(module) && hasPermission(actor, module === "ecommerce" ? "commerce.read" : "records.read"))
        group(module, label, icon, "business", [item(route, label, route, label, icon)], canonicalUiHref(`${base}/${route}`));
    }
    if (hasPermission(actor, "commerce.read") && hasPermission(actor, "commerce.sales") &&
        (!context.availableAppIds || context.availableAppIds.includes("customer_authority")))
      group("customer_authority", "Customer reviews", "access", "business", [
        item("customer-identity", "Identity reviews", "customer-identity", "Customer reviews", "parties"),
        item("customer-duplicates", "Duplicate reviews", "customer-duplicates", "Customer reviews", "parties"),
        item("customer-access", "Customer access reviews", "customer-access", "Customer reviews", "access"),
      ]);
    group("services", "Service desk", "feedback", "business", [
      item("service-desk", "Service desk", "requests", "Services", "feedback", ["IT requests", "equipment", "get help", "internal requests"]),
    ]);
  }

  if (context.availableAppIds) {
    const allowed = new Set(context.availableAppIds);
    for (let index = apps.length - 1; index >= 0; index--)
      if (!["guides", "services"].includes(apps[index].id) && !allowed.has(apps[index].id)) apps.splice(index, 1);
    const finance = apps.find((app) => app.id === 'finance');
    if (finance) finance.items = finance.items.filter((entry) => !['finance-engines','finance-group'].includes(entry.id));
  }

  // Branch usage remains available under Technology even when the optional
  // app rollout hides the rest of Technology for this branch.
  if (!beta && scopeKind === "branch" && role === "operations_manager" && !apps.some(app => app.id === "technology")) {
    group("technology", "Technology", "technology", "business", [
      item("branch-usage", "Usage", "usage", "Technology", "admin-usage", ["AI", "files", "storage"]),
    ]);
  }

  if (scopeKind === "holding" && base === canonicalUiHref("/holdings/erpx") && context.manageErpxVisible) {
    group("erpx-management", "ERPX Management", "technology", "business", [
      item("erpx-management", "Overview", "manage-erpx", "ERPX Management", "technology", ["customers", "provisioning", "recovery"]),
      item("manage-erpx-billing", "Customer billing", "manage-erpx/billing", "ERPX Management", "finance"),
      item("manage-erpx-usage", "Customer usage", "manage-erpx/billing/usage", "ERPX Management", "admin-usage"),
      item("manage-erpx-iam", "Platform IAM", "manage-erpx/iam", "ERPX Management", "access"),
      item("manage-erpx-executive", "Platform Executive", "manage-erpx/executive", "ERPX Management", "projects"),
    ], canonicalUiHref(`${base}/manage-erpx`), ["platform", "customers", "system"]);
  }

  const standalone = beta ? [] : [
    ...((!context.availableAppIds || context.availableAppIds.includes('legal')) && hasPermission(actor, "legal.read")
      ? [item("legal", "Legal", "legal", "Legal", "handbook", ["contracts", "cases", "compliance"])]
      : []),
    item("feedback", "Help using ERPX", "settings?section=feedback", "Help using ERPX", "feedback", ["product feedback", "reports"]),
    ...(scopeKind==='holding' && hasPermission(actor,'organization.manage')
      ? [item('pilot-features','Pilot feature rollout','settings/pilot-features','Pilot feature rollout','settings',['availability','coming soon','hotel pilot'])]
      : []),
    item("future-features", "Product roadmap", "future-features", "Product roadmap", "learn", ["future features", "coming soon", "prototype", "roadmap"]),
  ];

  const ownerForApp: Record<string, string> = {
    people: "organization", technology: "identity_access", procurement: "procurement",
    executive: "finance", resources: "collaboration", administration: "identity_access",
    "erpx-management": "manage_erpx", guides: "learning", services: "operations",
  };
  const metadata = (entry: NavigationItem, appId?: string): NavigationItem => {
    const owner = entry.id === "work" || entry.id.startsWith("work-") || entry.id.startsWith("calendar") ? "operations"
      : entry.id === "people-payroll" || entry.id === "people-roster" || entry.id === "people-evaluation" ? "people_payroll"
      : entry.id === "administration-usage" || entry.id === "branch-usage" ? "commercial"
      : entry.id === "administration-ai" ? "agent"
      : entry.id === "administration-approvals" ? "operations"
      : ownerForApp[appId ?? ""] ?? appId ?? (entry.id === "legal" ? "legal" : "collaboration");
    const destination = workplaceModuleDestinations[owner];
    const categoryId = ["home", "work", "calendar", "inbox"].includes(entry.id) ? "daily"
      : ["learn", "handbook", "feedback", "future-features"].includes(entry.id) ? "knowledge"
      : entry.id.startsWith("administration-") || ["pilot-features", "technology-business-readiness", "branch-usage"].includes(entry.id) ? "administration"
      : appId === "executive" ? "insights"
      : appId === "services" ? "services"
      : destination?.categoryId ?? "administration";
    return { ...entry, categoryId, ownerModuleId: owner, objective: destination?.objective,
      aiSupport: destination?.aiSupport ?? "guide" };
  };
  const hubAccess = salesCustomerAccess({ ...actor, branchId: scopeKind === "branch" ? "branch" : null, departmentId: scopeKind === "department" ? "department" : null }, context.availableAppIds);
  const movedDestination = (entry: NavigationItem) => {
    if (platform) return entry;
    const url = new URL(entry.href, "https://navigation.invalid");
    const route = url.pathname.slice(base.length + 1);
    const legacy=legacyWorkspaceDestination(base,route,Object.fromEntries(url.searchParams));
    if(legacy)return {...entry,href:legacy};
    if(beta)return entry;
    const href = salesCustomerLegacyDestination(base, route, Object.fromEntries(url.searchParams));
    return href ? { ...entry, href } : entry;
  };
  if (!platform) for (const app of apps) {
    app.items = app.items.filter(entry => entry.id !== "operations-pricing" || hubAccess.pricing).map(movedDestination);
    if (app.href) app.href = movedDestination({ id: app.id, label: app.label, href: app.href, group: app.label, icon: app.icon }).href;
  }
  const appDestinations = apps.flatMap((app) => app.items.map((child) => movedDestination(metadata(child, app.id))))
    .filter(entry => entry.id !== "operations-pricing" || hubAccess.pricing);
  const hubEntries: NavigationItem[] = beta ? [] : [
    ...(hubAccess.today ? [{ ...item("sales-hub", "Sales", "sales", "Sales", "commerce", ["today", "follow-ups", "pipeline", "quotations", "orders"]), categoryId: "customers-sales" as const, ownerModuleId: "sales" }] : []),
    ...(hubAccess.directory || hubAccess.support || hubAccess.contacts || hubAccess.reviews ? [{ ...item("customers-hub", "Customers", hubAccess.directory ? "customers" : hubAccess.contacts ? "customers?view=contacts" : hubAccess.support ? "customers?view=support" : "customers?view=reviews", "Customers", "parties", ["Customer 360", "prospects", "contacts", "service", "reviews"]), categoryId: "customers-sales" as const, ownerModuleId: "crm" }] : []),
  ];
  const hubTools = beta ? [] : [
    ...(hubAccess.contacts ? [{ ...item("crm-contacts", "Prospects & contacts", "customers?view=contacts", "Customers", "parties", ["accounts", "contacts"]), categoryId: "customers-sales" as const, ownerModuleId: "crm" }] : []),
    ...(hubAccess.reports ? [{ ...item("crm-reports", "CRM reports", "sales?view=reports", "Sales", "commerce", ["analytics", "forecast"]), categoryId: "customers-sales" as const, ownerModuleId: "crm" }] : []),
  ];
  // One level of work entries; detailed views stay with their owning page.
  const categoryEntries = [...appDestinations, ...standalone.map(entry => movedDestination(metadata(entry)))]
    .filter(entry => entry.id !== "crm-customers");
  const categories = workplaceCategories.flatMap(([id, label, icon]): NavigationCategory[] => {
    let entries = categoryEntries.filter(entry => entry.categoryId === id);
    for(const [family,ids,label] of [['assets',['assets','maintenance'],'Assets & maintenance']] as const){const matching=entries.filter(entry=>(ids as readonly string[]).includes(entry.id));if(matching.length){const preferred=matching.find(entry=>entry.id===ids[0])??matching[0];entries=entries.filter(entry=>!(ids as readonly string[]).includes(entry.id));entries.push({...preferred,label,href:family==='assets'?canonicalUiHref(`${base}/assets${preferred.id==='maintenance'?'?view=maintenance':''}`):canonicalUiHref(`${base}/knowledge${preferred.id==='learning'?'?view=learning&section=courses':''}`)});}}

    if (id === "customers-sales") {
      const secondaryItems = entries.filter(entry => ["marketing", "ecommerce"].includes(entry.id));
      return hubEntries.length || secondaryItems.length ? [{ id, label, icon, apps: hubEntries, secondaryItems }] : [];
    }
    if (id === "finance" && context.financeWorkspaceV2) entries = financeRootItems(entries);
    return entries.length ? [{ id, label, icon, apps: entries }] : [];
  });
  const legacyTechnologyIds: Record<string,string> = {"settings:attributes":"technology-attributes","directory:connections":"technology-connections","settings:roles":"technology-roles","settings:reviews":"technology-reviews","settings:matrix":"technology-matrix","settings:groups":"technology-mappings"};
  const technologyAliases = apps.some(app => app.items.some(entry => entry.id === "technology-settings"))
    ? Object.entries(technologySections).flatMap(([view, sections]) => sections.map(section => metadata(item(legacyTechnologyIds[`${view}:${section.id}`] ?? `technology-${view}-${section.id}`, section.id === "connections" ? "Directory connections" : section.title, `technology?view=${view}&section=${section.id}`, "Technology", "admin-iam"), "technology"))) : [];
  const hrAliases = appDestinations.some(entry => entry.id === "people-settings") ? hrSettingsTopics
    .filter(topic => topic.id === "pay-groups" ? hasPermission(actor,"people.payroll.details") : ["leave","calendars"].includes(topic.id) ? hasPermission(actor,"leave.policy.manage") : hasPermission(actor,"people.manage") && hasPermission(actor,"organization.manage"))
    .map(topic => metadata(item(`people-settings-${topic.id}`, topic.title, `people/settings?section=${topic.id}`, "People", "settings", [topic.description]), "people")) : [];

  const workspaceAliases=Object.entries(sectionWorkspaceTopics).filter(([family])=>family!=='projects').flatMap(([family,topics])=>topics.filter(topic=>{const moduleId=family==='administration'?null:family==='assets'?(topic.id==='register'?'assets':'maintenance'):family==='knowledge'?(topic.id.startsWith('learning:')?'learning':null):family==='catalog'?(topic.id==='pricing'?'sales':'inventory'):family;return !moduleId||context.availableAppIds===undefined||context.availableAppIds.includes(moduleId);}).filter(topic=>family==='administration'?visibleAdminSections.includes((topic.query as Record<string,string>).section as AdminSection)&&(topic.id!=='brand:media'||visibleAdminSections.includes('organization')):family==='assets'?hasPermission(actor,'records.read')&&hasModuleAccess(context.moduleIds??[],topic.id==='register'?'assets':'maintenance'):family==='knowledge'?(topic.id.startsWith('handbook:')?(topic.id==='handbook:published'||topic.id==='handbook:drafts'&&context.handbookAccess?.editor||topic.id==='handbook:editors'&&context.handbookAccess?.owner):hasPermission(actor,'records.read')&&hasModuleAccess(context.moduleIds??[],'learning')):family==='benefits'?hasPermission(actor,'records.read')&&hasModuleAccess(context.moduleIds??[],'benefits'):family==='finance'?hasPermission(actor,'finance.read'):hasPermission(actor,'commerce.read')&&(topic.id!=='pricing'||hubAccess.pricing)).map(topic=>({...item(family==='catalog'?`operations-${topic.id}`:`workspace-${family}-${topic.id}`,topic.title,`${topic.path}?${new URLSearchParams(topic.query)}`,family==='knowledge'?'Knowledge':family==='assets'?'Assets & maintenance':family,'settings'),categoryId:(family==='administration'?'administration':family==='knowledge'?'knowledge':family==='finance'?'finance':family==='catalog'?'inventory-quality':family==='benefits'?'people-organization':'services') as NavigationCategory["id"]})));
  const known=[...primary,...primary.flatMap(entry=>entry.submodules??[]),...appDestinations,...workspaceAliases,...technologyAliases,...hrAliases,...standalone.map(movedDestination)];
  const registered=destinations.filter(destination=>destinationAvailable(context,destination)).map(destination=>{
    const href=`${base}/${destination.path}`,existing=known.find(entry=>entry.href.split('?')[0]===href);
    const parent=destination.path.split('/').slice(0,-1).join('/');
    return {...(existing??item(`destination:${destination.path}`,destination.label,href,parentLabels[parent]??destination.label,'settings')),label:destination.label,href,categoryId:(['home','work','calendar','inbox'].includes(destination.path.split('/')[0])?'daily':destination.path.split('/')[0]) as NavigationItem['categoryId']};
  });
  function withSubmodules(entry:NavigationItem){const url=new URL(entry.href,'https://navigation.invalid');if(!url.pathname.startsWith(base+'/'))return entry;const path=url.pathname.slice(base.length+1),parent=Object.hasOwn(parentLabels,path)?path:path.split('/').slice(0,-1).join('/');if(parent.split('/').length<2||parent===entry.categoryId)return entry;const children=registered.filter(child=>{const childPath=child.href.slice(base.length+1);return child.id!==entry.id&&childPath!==path&&childPath.startsWith(parent+'/')&&childPath.slice(parent.length+1).split('/').length===1;});return children.length?{...entry,submodules:uniqueDestinations(children)}:entry;}

  for(const category of categories)category.apps=category.apps.map(withSubmodules);
  for(const app of apps)app.items=app.items.map(withSubmodules);
  const work=primary.find(entry=>entry.id==='work');if(work)work.submodules=registered.filter(entry=>entry.href.startsWith(base+'/work/'));
  const model: WorkplaceNavigationModel = {
    primary,
    apps,
    categories,
    standalone:standalone.map(movedDestination),
    search: uniqueDestinationIds([...primary.map(entry => metadata(entry)), ...primary.flatMap(entry => entry.submodules ?? []).map(entry => ({...metadata(entry), categoryId: "daily" as const})), ...hubEntries, ...hubTools, ...appDestinations, ...workspaceAliases, ...technologyAliases, ...hrAliases, ...registered, ...standalone.map(entry => movedDestination(metadata(entry))),
      ...workplaceStartActions.map(action=>({...item(`start-${action.id}`,action.label,`${base}${action.path}`,"Start","work"),categoryId:"daily" as const,ownerModuleId:action.owner,objective:action.objective,aiSupport:"prepare" as const}))]),
    settingsDefault: sectionWorkspaceTopics.administration.find(topic=>visibleAdminSections.includes(topic.query.section as AdminSection))?.query.section as AdminSection ?? "feedback",
  };
  return context.productId ? productOfficeNavigation(model, context.productId) : model;
}

function uniqueDestinationIds(items:NavigationItem[]){const seen=new Set<string>();return items.filter(item=>!seen.has(item.id)&&!!seen.add(item.id));}

function queryValue(searchParams: SearchParamsLike | undefined, key: string) {
  return searchParams?.get(key) ?? "";
}

function relativeSegments(base: string, pathname: string | null) {
  pathname ??= canonicalUiHref(`${base}/home`);
  const relative = pathname.startsWith(canonicalUiHref(`${base}/`))
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
  const publicPath = pathname.startsWith(base + '/') ? pathname.slice(base.length + 1) : '';
  const roots=[...model.primary,...model.categories.flatMap(category=>[...category.apps,...category.secondaryItems??[]]),...model.apps.flatMap(app=>app.items),...model.standalone];
  const direct=roots.find(item=>new URL(item.href,'https://navigation.invalid').pathname===pathname);
  if(direct){const app=model.apps.find(app=>app.items.some(item=>item.id===direct.id||item.submodules?.some(child=>child.id===direct.id)));return {activeId:direct.id,sidebarId:app?.id==='finance'?financeSidebarId(direct.id):direct.id,...(app?{groupId:app.id}:{})};}
  const exact=model.search.find(item=>new URL(item.href,'https://navigation.invalid').pathname===pathname);
  if(exact&&destinationForPath(publicPath)){const parent=roots.filter(item=>{const path=new URL(item.href,'https://navigation.invalid').pathname;return pathname.startsWith(path+'/');}).sort((a,b)=>b.href.length-a.href.length)[0];const app=model.apps.find(app=>app.items.some(item=>item.id===parent?.id||item.submodules?.some(child=>child.id===exact.id)))??model.apps.find(app=>app.items.some(item=>sourcePath(new URL(item.href,'https://navigation.invalid').pathname.slice(base.length+1))===destinationForPath(publicPath)?.source));const owner=parent??app?.items.find(item=>sourcePath(new URL(item.href,'https://navigation.invalid').pathname.slice(base.length+1))===destinationForPath(publicPath)?.source);return {activeId:exact.id,...(owner?{sidebarId:app?.id==='finance'?financeSidebarId(exact.id):owner.id}:{}),...(app?{groupId:app.id}:{})};}

  const destination = destinationForPath(publicPath);
  const areaParent=publicPath.match(/^finance\/(today|documents|cash|accounting|reports|setup)$/)?.[1];if(areaParent&&hasApp(model,'finance')){const id=({today:'finance-overview',documents:'finance-sales',cash:'finance-payments',accounting:'finance-ledger',reports:'finance-reports',setup:'finance-setup'} as Record<string,string>)[areaParent];return {activeId:id,sidebarId:id,groupId:'finance'};}
  if (destination || sourcePath(publicPath) !== publicPath) {
    const query = new URLSearchParams(Object.entries(selectionForPath(pathname)));
    for (const key of ['q','kind','view','type','section','tab','queue']) {
      const value=searchParams?.get(key); if(value!==null&&value!==undefined&&!query.has(key))query.set(key,value);
    }
    pathname=`${base}/${sourcePath(publicPath)}`;
    searchParams=query;
  }
  const [route, second, third] = relativeSegments(base, pathname);
  const standard = base !== "/platform" && !hasApp(model, "resources");
  if (base !== "/platform") {
    const salesViews: Record<string, string> = { today: "sales-today", pipeline: "crm-pipeline", documents: queryValue(searchParams, "type") === "sales" ? "operations-sales" : "operations-quotations", pricing: "operations-pricing", reports: "crm-reports" };
    const reviews: Record<string, string> = { identity: "customer-identity", duplicates: "customer-duplicates", access: "customer-access" };
    if (route === "sales") return { activeId: queryValue(searchParams, "kind") === "followups" ? "customer-followups" : salesViews[queryValue(searchParams, "view") || "today"] ?? "sales-today", sidebarId: "sales-hub", groupId: "sales" };
    if (route === "customers") {
      const view = queryValue(searchParams, "view"), root = model.search.find(item => item.id === "customers-hub");
      const defaultView = root?.href.includes("view=support") ? "support" : "directory";
      const activeId = second ? "customer-timeline" : ({ directory: view ? "crm-customers" : "customer-timeline", contacts: "crm-contacts", support: "support-cases", reviews: reviews[queryValue(searchParams, "review") || "identity"] } as Record<string, string>)[view || defaultView] ?? "customer-timeline";
      return { activeId, sidebarId: "customers-hub", groupId: "crm", detailLabel: second ? "Customer profile" : undefined };
    }
    if (route === "crm" || route === "customer-followups") return { activeId: route === "crm" ? "crm-pipeline" : "customer-followups", sidebarId: "sales-hub", groupId: "crm", detailLabel: second ? route === "crm" ? "CRM case" : "Customer follow-up" : undefined };
    if (route === "support" || ["customer-identity", "customer-duplicates", "customer-access"].includes(route)) return { activeId: route === "support" ? "support-cases" : route, sidebarId: "customers-hub", groupId: route === "support" ? "support" : "customer_authority", detailLabel: second ? route === "support" ? second === "sla" ? "Resolution-time watch" : second === "incidents" ? "Shared incidents" : "Support case" : "Review request" : undefined };
    if (route === "commerce" && second === "orders" && queryValue(searchParams, "view") !== "purchase") return { activeId: queryValue(searchParams, "view") === "quote" ? "operations-quotations" : "operations-sales", sidebarId: "sales-hub", groupId: "sales", detailLabel: "Sales document" };
    if (route === "commerce" && !second && ["", "sales", "quote", "pricing", "parties"].includes(queryValue(searchParams, "view")) && queryValue(searchParams, "kind") !== "supplier") {
      const view = queryValue(searchParams, "view");
      return { activeId: view === "parties" ? "crm-customers" : view === "pricing" ? "operations-pricing" : view === "quote" ? "operations-quotations" : "operations-sales", sidebarId: view === "parties" ? "customers-hub" : "sales-hub", groupId: view === "parties" ? "crm" : "sales" };
    }
  }
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
  if (route === "manage-erpx" && base === canonicalUiHref("/holdings/erpx")) {
    const activeId =
      second === "billing"
        ? third === "usage" ? "manage-erpx-usage" : "manage-erpx-billing"
        : second === "iam"
          ? "manage-erpx-iam"
          : second === "executive"
            ? "manage-erpx-executive"
            : "erpx-management";
    return { activeId, groupId: "erpx-management" };
  }
  if (route === "billing" && base === "/platform") return { activeId: "platform-billing" };
  if (route === "usage") return { activeId: model.search.some((item) => item.id === "branch-usage") ? "branch-usage" : "administration-usage", groupId: "technology" };
  if (route === "my-access") return { activeId: "my-access" };
  if (route === "work") return { activeId: "work" };
  if (route === "payroll") return {
    activeId: "people-payroll",
    groupId: "people",
    detailLabel: second === "runs" && third ? "Payroll run" : second === "history" ? "History" : second === "advances" ? "Advances" : second === "settings" ? "Settings" : undefined,
  };
  if (route === "future-features") return { activeId: "future-features" };
  if (route === "requests")
    return { activeId: second ? "work" : "service-desk", ...(second ? {} : { groupId: "services" }), detailLabel: second ? "Request" : undefined };
  if (route === "tasks")
    return {
      activeId: "work",
      detailLabel:
        second === "people_payroll" ? "Annual leave request" : "Task",
    };
  if (route === "technology") {
    const view = queryValue(searchParams,"view") || "organization";
    const normalized = view === "attributes" ? "settings" : view;
    const section = view === "attributes" ? "attributes" : queryValue(searchParams,"section");
    const detailLabel = normalized in technologySections ? technologySections[normalized as keyof typeof technologySections].find(item => item.id === section)?.title : undefined;
    return { activeId: ({resources:"technology-devices",directory:"technology-directory",settings:"technology-settings"} as Record<string,string>)[normalized] ?? "technology-organization", groupId:"technology", ...(detailLabel ? {detailLabel} : {}) };
  }

  if (route === "people") {
    if (!second && searchParams?.get("view") === "users") return { activeId: "people-users-access", groupId: "people" };
    if (second === "evaluation") return { activeId: "people-evaluation", groupId: "people" };
    if (second === "roster") return { activeId: model.search.some(item=>item.id==="people-roster") ? "people-roster" : "people-directory", groupId: "people" };
    if (second === "settings") {
      const section = queryValue(searchParams, "section");
      if (standard && ["connections", "roles", "matrix", "groups", "reviews", "attributes"].includes(section)) return { activeId: section === "connections" ? "technology-directory" : "technology-settings", groupId: "technology", detailLabel: section === "connections" ? "Connections" : technologySections.settings.find(item => item.id === section)?.title };
      return { activeId: "people-settings", groupId: "people", ...(hrSettingsTopics.find(topic => topic.id === section) ? {detailLabel:hrSettingsTopics.find(topic => topic.id === section)!.title} : {}) };

    }
    if (second === "organization")
      return { activeId: "people-directory", groupId: "people" };
    if (second === "access") {
      const view = queryValue(searchParams, "view");
      return standard
        ? { activeId: `technology-${view === "devices" ? "devices" : ["groups", "policies"].includes(view) ? "organization" : "devices"}`, groupId: "technology" }
        : { activeId: "people-access", groupId: "people" };
    }
    if (second === "payroll")
      return {
        activeId: "people-payroll",
        groupId: "people",
        detailLabel: third === "runs" ? "Payroll run" : third === "history" ? "History" : third === "advances" ? "Advances" : third === "settings" ? "Settings" : third ? "Payroll run" : undefined,
      };
    if (second === "move")
      return { activeId: "people-directory", groupId: "people", detailLabel: third === "review" ? "Review move" : "Move" };
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
        activeId: queryValue(searchParams, "view") === "purchase" ? "operations-purchasing" : queryValue(searchParams, "view") === "quote" ? "operations-quotations" : "operations-sales",
        groupId: queryValue(searchParams, "view") === "purchase" ? "procurement" : "sales",
        detailLabel: "Order",
      };
    const view = queryValue(searchParams, "view");
    if (view === "parties") return queryValue(searchParams, "kind") === "supplier"
      ? { activeId: "procurement-suppliers", groupId: "procurement" }
      : { activeId: "crm-customers", groupId: "crm" };
    const byView: Record<string, string> = {
      sales: "operations-sales",
      quote: "operations-quotations",
      pricing: "operations-pricing",
      purchase: "operations-purchasing",
      stock: "operations-stock",
      products: "operations-products",
      warehouses: "operations-warehouses",
    };
    return {
      activeId: byView[view] ?? "operations-sales",
      groupId: ["stock", "products", "warehouses"].includes(view) ? "inventory" : view === "purchase" ? "procurement" : "sales",
    };
  }
  if (route === "procurement-rfqs") return { activeId: "procurement-rfqs", groupId: "procurement", detailLabel: second ? "Supplier quotation request" : undefined };
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
  if (route === "pos") {
    const tab = queryValue(searchParams, "tab");
    if (tab === "reports") return { activeId: "pos-reports", groupId: "pos" };
    if (tab === "settings") return { activeId: "pos-settings", groupId: "pos" };
    if (tab === "history" && queryValue(searchParams, "queue") === "approval")
      return { activeId: "pos-approvals", groupId: "pos" };
    return { activeId: "pos-front", groupId: "pos" };
  }
  if (route === "stock-transfers")
    return {
      activeId: "operations-transfers",
      groupId: "inventory",
      detailLabel: second ? "Warehouse transfer" : undefined,
    };
  if (route === "pms") {
    if (second === "reports") return { activeId: "pms-reports", groupId: "pms" };
    if (second === "settings") return { activeId: "pms-settings", groupId: "pms" };
    if (second === "guest-accounts" && queryValue(searchParams, "queue") === "approval")
      return { activeId: "pms-approvals", groupId: "pms" };
    return { activeId: "pms-front", groupId: "pms" };
  }
  if(route==='assets'&&!second&&['maintenance','plans'].includes(queryValue(searchParams,'view')))return {activeId:model.search.some(entry=>entry.id==='assets')?'assets':'maintenance',groupId:'technology',detailLabel:queryValue(searchParams,'view')==='plans'?'Preventive plans':'Maintenance work'};
  const extraRouteApps: Record<string, string> = {
    assets: "assets", maintenance: "maintenance", "field-service": "field_service", planning: "planning",
    manufacturing: "manufacturing", talent: "talent", benefits: "benefits", learning: "learning",
    marketing: "marketing", ecommerce: "ecommerce", agents: "agent",
    "customer-identity": "customer_authority", "customer-duplicates": "customer_authority", "customer-access": "customer_authority",
  };
  if (extraRouteApps[route]) return { activeId: route, groupId: extraRouteApps[route], detailLabel: second ? "Record" : undefined };
  if (route === "finance") {
    if (model.search.some(item => item.id === "finance-payments")) {
      const activeId = financeDestinationForRoute(second, queryValue(searchParams, "kind"));
      const detailLabel = third ? ({ journals: "Journal", payments: "Payment", invoices: "Invoice", refunds: "Refund", "supplier-batches": "Supplier batch", tax: third === "entries" ? "Tax entry" : third === "corrections" ? "Tax correction" : "VAT workpaper" } as Record<string, string>)[second ?? ""] : undefined;
      return { activeId, sidebarId: financeSidebarId(activeId), groupId: "finance", detailLabel };
    }
    const bySection: Record<string, string> = {
      ledger: "finance-ledger",
      accounting: "finance-ledger",
      journals: "finance-ledger",
      sales: "finance-sales",
      invoices: queryValue(searchParams, "kind") === "payable" ? "finance-payables" : "finance-receivables",
      payments: "finance-payables",
      bank: "finance-bank",
      hotels:"finance-hotels",
      close: "finance-close",
      tax: "finance-tax",
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
        ...(model.search.some(item => item.id === "finance-payments") ? { sidebarId: financeSidebarId("finance-expenses") } : {}),
        groupId: "finance",
        detailLabel: second ? "Expense claim" : undefined,
      };
    return {
      activeId: "work",
      detailLabel: second ? "Expense claim" : undefined,
    };
  }
  if(route==='knowledge')return {activeId:queryValue(searchParams,'view')==='learning'?'learning':'handbook',groupId:standard?'guides':'resources',detailLabel:queryValue(searchParams,'section')||undefined};
  if(route==='settings'&&queryValue(searchParams,'section')==='brand')return {activeId:'administration-brand',groupId:'technology',detailLabel:'Brand & identity'};
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
      groupId: standard ? "guides" : "resources",
      detailLabel: second ? "Article" : undefined,
    };
  if (route === "learn")
    return {
      activeId: "learn",
      groupId: standard ? "guides" : hasApp(model, "resources") ? "resources" : undefined,
    };
  if (route === "settings") {
    if(second==='pilot-features')return {activeId:'pilot-features'};
    if(second==='business-readiness')return {activeId:'technology-business-readiness',groupId:'technology'};
    const section = queryValue(searchParams, "section") || (standard ? model.settingsDefault : "");
    if (standard) {
      if (section === "feedback") return { activeId: "feedback" };
      if (section === "usage") return { activeId: "administration-usage", groupId: "technology" };
      if (section === "ai") return { activeId: "administration-ai", groupId: "technology" };
      if (section === "approvals") return model.search.some((item) => item.id === "administration-approvals")
        ? { activeId: "administration-approvals", groupId: "technology" }
        : { activeId: "projects-routines", groupId: "projects" };
      return { activeId: "people-directory", groupId: "people" };
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
      return { activeId: section === "people" ? "people-settings" : "people-directory", groupId: "people" };
    return {
      activeId: `administration-${section}`,
      groupId: "administration",
    };
  }
  if (route === "iam") {
    if (base !== "/platform") {
      const destination = new URL(canonicalUiHref(`${base}/${peopleIamDestination(second ?? "overview")}`), "https://navigation.invalid");
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
  const relative=pathname.startsWith(base+'/')?pathname.slice(base.length+1):'';
  if(relative&&relative!=='home')return destinationTrail(base,relative);
  if (pathname.replace(/\/$/, "") === canonicalUiHref(`${base}/settings/stripe`))
    return [{ label: "Company Stripe" }];
  if (pathname.replace(/\/$/, "") === canonicalUiHref(`${base}/settings/notifications`))
    return [{ label: "Notification settings" }];
  if(pathname===canonicalUiHref(`${base}/assets`)){const view=queryValue(searchParams,'view')||'register',topic=sectionWorkspaceTopics.assets.find(item=>item.id===view);return [{label:'Assets & maintenance',href:canonicalUiHref(`${base}/assets`)},...(topic?[{label:topic.title}]:[])];}
  if(pathname===canonicalUiHref(`${base}/knowledge`)){const view=queryValue(searchParams,'view')||'handbook',section=queryValue(searchParams,'section')||(view==='learning'?'courses':'published'),topic=sectionWorkspaceTopics.knowledge.find(item=>item.id===`${view}:${section}`);return [{label:'Knowledge',href:canonicalUiHref(`${base}/knowledge`)},...(topic?[{label:topic.title}]:[])];}
  const resolved = resolveNavigation(model, base, pathname, searchParams);
  if (!resolved.activeId || resolved.activeId === "home") return [];
  if (["sales-hub", "customers-hub"].includes(resolved.sidebarId ?? "")) {
    const hub = resolved.sidebarId === "sales-hub" ? "sales" : "customers", labels: Record<string, string> = { "sales-today": "Today", "crm-pipeline": "Pipeline", "customer-followups": "Today", "operations-sales": "Sales documents", "operations-quotations": "Sales documents", "operations-pricing": "Price lists", "crm-reports": "Reports", "crm-contacts": "Prospects & contacts", "customer-timeline": "Customers", "crm-customers": "Customers", "support-cases": "Service", "customer-identity": "Review customer data", "customer-duplicates": "Review customer data", "customer-access": "Review customer data" };
    const label = labels[resolved.activeId], detail = resolved.detailLabel ?? (queryValue(searchParams, "detailId") ? ({ customer: "Customer profile", order: "Sales document", crm_lead: "CRM case", customer_followup: "Customer follow-up", support_ticket: "Support case" } as Record<string, string>)[queryValue(searchParams, "detailKind")] ?? "Record" : undefined);
    return [{ label: hub === "sales" ? "Sales" : "Customers", href: canonicalUiHref(`${base}/${hub}`) }, ...(label && label !== "Customers" ? [{ label }] : []), ...(detail ? [{ label: detail }] : [])];
  }
  if (resolved.activeId === "people-directory" && resolved.groupId === "people")
    return [
      { label: "People", ...(resolved.detailLabel ? { href: canonicalUiHref(`${base}/people`) } : {}) },
      ...(resolved.detailLabel ? [{ label: resolved.detailLabel }] : []),
    ];
  const payrollBase = canonicalUiHref(`${base}/people/payroll`);
  if (pathname === payrollBase || pathname.startsWith(`${payrollBase}/`)) {
    const section = pathname.slice(payrollBase.length).split("/")[1];
    return [
      { label: "People", href: canonicalUiHref(`${base}/people`) },
      { label: "Payroll", ...(section ? { href: payrollBase } : {}) },
      ...(section ? [{ label: section === "history" ? "History" : section === "advances" ? "Advances" : section === "settings" ? "Settings" : "Payroll run" }] : []),
    ];
  }
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
  const activeId = resolved.sidebarId ?? resolved.activeId;
  return model.categories.find(entry => [...entry.apps, ...(entry.secondaryItems ?? [])].some(item => item.id === activeId))
    ?? model.categories.find(entry => entry.apps.some(item =>
      model.apps.find(app => app.id === resolved.groupId)?.items.some(child => child.id === item.id)));

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
