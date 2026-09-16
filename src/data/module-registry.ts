import registry from "./module-registry.json";

export type WorkspaceKind =
  | "workflow"
  | "board"
  | "ledger"
  | "analytics"
  | "timeline"
  | "library"
  | "directory"
  | "health"
  | "conversation";
export type CapabilityAvailability = "live" | "preview" | "partial";
export type ActionAvailability = {
  label: string;
  availability: "live" | "preview";
  destination?: string;
};
export type CapabilitySupport = {
  availability: CapabilityAvailability;
  actions: ActionAvailability[];
};
export type ModuleSection = {
  id: string;
  label: string;
  description: string;
  kind: WorkspaceKind;
  capability: boolean;
};
export type ModuleDefinition = {
  id: string;
  name: string;
  group: string;
  icon: string;
  description: string;
  recordType: string;
  action: string;
  primary: string[];
  sections: ModuleSection[];
};
export const moduleRegistry = registry as ModuleDefinition[];
export function findModule(id: string) {
  return moduleRegistry.find((module) => module.id === id);
}
export function liveDestination(
  moduleId: string,
  sectionId: string,
  workspaceBase?: string,
): string | undefined {
  if (workspaceBase === "/platform") return undefined;
  if (
    moduleId === "people_payroll" &&
    [
      "my-requests",
      "team-requests",
      "feature-leave-requests",
      "employees",
      "payroll-summaries",
      "feature-payroll-runs",
    ].includes(sectionId)
  )
    return sectionId === "my-requests" ||
      sectionId === "team-requests" ||
      sectionId === "feature-leave-requests"
      ? "work?type=leave"
      : ["payroll-summaries", "feature-payroll-runs"].includes(sectionId)
        ? "people?view=payroll"
        : "people";
  if (
    moduleId === "projects" &&
    [
      "work-breakdown",
      "board",
      "progress",
      "feature-projects-wbs",
      "feature-handoffs",
      "feature-attachments",
    ].includes(sectionId)
  )
    return "projects";
  if (
    moduleId === "operations" &&
    [
      "exceptions",
      "area-queues",
      "handovers",
      "feature-exception-queue",
      "feature-approval-decisions",
      "feature-resolution",
    ].includes(sectionId)
  )
    return "staff-tasks";
  const routes: Record<string, Record<string, string>> = {
    people_payroll: {
      expenses: "expenses",
      "feature-expenses": "expenses",
      "feature-expense-claims": "expenses",
    },
    identity_access: {
      people: "iam/users",
      assignments: "iam/users",
      "feature-people-principals": "iam/users",
      "feature-roles-permissions": "iam/roles",
    },
    sales: {
      quotes: "commerce?view=quote",
      orders: "commerce?view=sales",
      fulfillment: "commerce?view=sales",
      "feature-cancellations": "commerce?view=sales",
      "feature-revenue-handoff": "commerce?view=sales",
    },
    procurement: {
      "purchase-orders": "commerce?view=purchase",
      "goods-receipts": "commerce?view=purchase",
      "feature-invoice-match": "commerce?view=purchase",
      "feature-supplier-history": "commerce?view=parties",
    },
    inventory: {
      availability: "commerce?view=stock",
      movements: "commerce?view=stock",
      "feature-stock-register": "commerce?view=stock",
      "feature-receipts": "commerce?view=purchase",
      "feature-issues-movements": "commerce?view=stock",
    },
    customer_360: {
      profiles: "commerce?view=parties",
      "feature-customer-profile": "commerce?view=parties",
      "sales-summary": "commerce?view=sales",
    },
    crm: {
      accounts: "commerce?view=parties",
      "feature-sales-handoff": "commerce?view=quote",
    },
    finance: {
      journals: "finance/ledger",
      receivables: "finance/invoices?kind=receivable",
      payables: "finance/invoices?kind=payable",
      banking: "finance/bank",
      "close-checklist": "finance/ledger",
      "feature-chart-of-accounts": "finance/ledger",
      "feature-payments": "finance/invoices",
      "feature-receivables-aging": "finance/invoices?kind=receivable",
      "feature-bank-reconciliation": "finance/bank",
      "feature-reports": "finance/ledger",
      "feature-period-close": "finance/ledger",
    },
  };
  if (routes[moduleId]?.[sectionId]) return routes[moduleId][sectionId];
  if (
    moduleId === "collaboration" &&
    [
      "feed",
      "topics",
      "following",
      "feature-topics-feed",
      "feature-following-reactions",
      "feature-comments-moderation",
    ].includes(sectionId)
  )
    return "feed";
  return undefined;
}
export function moduleAvailability(
  module: ModuleDefinition,
  workspaceBase?: string,
): CapabilityAvailability {
  return module.sections.some(
    (section) =>
      capabilitySupport(module.id, section.id, workspaceBase).availability !== "preview",
  )
    ? "partial"
    : "preview";
}
export function capabilitySupport(
  moduleId: string,
  sectionId: string,
  workspaceBase?: string,
): CapabilitySupport {
  if (workspaceBase === "/platform") return { availability: "preview", actions: [{ label: "Platform actions", availability: "preview" }] };
  const destination = liveDestination(moduleId, sectionId);
  if (destination === "work" || destination === "work?type=leave")
    return {
      availability: "partial",
      actions: [
        {
          label: "Create and submit leave requests",
          availability: "live",
          destination,
        },
        {
          label: "Review eligible leave requests",
          availability: "live",
          destination,
        },
        {
          label: "Upload and inspect evidence",
          availability: "live",
          destination,
        },
        { label: "Create other requests", availability: "preview" },
      ],
    };
  if (destination === "feed") {
    const labels = /following/.test(sectionId)
      ? ["Follow updates", "React to updates"]
      : /comments/.test(sectionId)
        ? ["Add comments"]
        : ["Publish updates", "Read team updates", "Publish Stories"];
    return {
      availability: "partial",
      actions: [
        ...labels.map(
          (label): ActionAvailability => ({
            label,
            availability: "live",
            destination,
          }),
        ),
        {
          label: /moderation/.test(sectionId)
            ? "Moderate team content"
            : "Additional capability workflows",
          availability: "preview",
        },
      ],
    };
  }
  if (moduleId === "inventory" && destination === "commerce?view=stock")
    return {
      availability: "partial",
      actions: [
        {
          label: "Inspect stock quantities, valuation and movements",
          availability: "live",
          destination,
        },
        {
          label: "Prepare, review and post physical stock counts",
          availability: "live",
          destination: "stock-counts",
        },
        {
          label: "Prepare and confirm warehouse transfers",
          availability: "live",
          destination: "stock-transfers",
        },
        {
          label: "Reservations, goods in transit and lot tracking",
          availability: "preview",
        },
      ],
    };
  if (destination === "expenses")
    return {
      availability: "partial",
      actions: [
        {
          label: "Create, review and reimburse employee claims",
          availability: "live",
          destination,
        },
        {
          label: "Reverse an unpaid expense with accounting history",
          availability: "live",
          destination,
        },
        {
          label: "Bank execution and expense currency conversion",
          availability: "preview",
        },
      ],
    };
  if (
    destination === "staff-tasks" ||
    destination === "projects" ||
    destination === "routine-templates"
  )
    return {
      availability: "partial",
      actions: [
        {
          label: "Manage projects, assigned tasks and ordered approvals",
          availability: "live",
          destination,
        },
        {
          label: "Publish and schedule routine work",
          availability: "live",
          destination: "routine-templates",
        },
        {
          label: "Advanced project dependency planning",
          availability: "preview",
        },
      ],
    };
  if (destination)
    return {
      availability: "partial",
      actions: [
        {
          label:
            destination === "people"
              ? sectionId.includes("payroll")
                ? "Calculate and review gross-pay previews"
                : "Manage linked people and role assignments"
              : destination.startsWith("commerce")
                ? "Process connected orders, stock and invoices"
                : "Work with posted accounting records",
          availability: "live",
          destination,
        },
        { label: "Additional module workflows", availability: "preview" },
      ],
    };
  return {
    availability: "preview",
    actions: [{ label: "Business actions", availability: "preview" }],
  };
}
