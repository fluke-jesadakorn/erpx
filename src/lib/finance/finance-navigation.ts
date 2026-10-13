import { hasPermission } from "@/lib/identity_access/permissions";
import type { NavigationContext, NavigationIconId, NavigationItem } from "@/lib/work_projection/workplace-navigation";

export type FinanceArea = "today" | "documents" | "cash" | "accounting" | "reports" | "setup";
type Destination = {
  id: string;
  label: string;
  route: string;
  area: FinanceArea;
  icon: NavigationIconId;
  keywords: string[];
  tool?: boolean;
  advanced?: boolean;
  hotel?: boolean;
  holdingOnly?: boolean;
  expense?: boolean;
};

/** One destination catalog owns discovery, area navigation and retained URLs. */
export const financeDestinations: readonly Destination[] = [
  { id: "finance-overview", label: "Today", route: "finance", area: "today", icon: "home", keywords: ["my work", "finance work", "priorities"] },
  { id: "finance-sales", label: "Sales & collections", route: "finance/sales", area: "documents", icon: "invoice", keywords: ["quotations", "sales orders", "invoices", "receipts", "collections"] },
  { id: "finance-receivables", label: "Receivables", route: "finance/invoices?kind=receivable&queue=open", area: "documents", icon: "invoice", keywords: ["accounts receivable", "collections", "customer invoices"] },
  { id: "finance-payables", label: "Payables", route: "finance/invoices?kind=payable&queue=open", area: "documents", icon: "invoice", keywords: ["accounts payable", "supplier invoices"] },
  { id: "finance-expenses", label: "Employee expenses", route: "expenses", area: "documents", icon: "expense", keywords: ["expense claims", "reimbursements"], expense: true },
  { id: "finance-payments", label: "Payments", route: "finance/payments", area: "cash", icon: "bank", keywords: ["payment obligations", "receipts", "treasury", "payroll payments"] },
  { id: "finance-bank", label: "Bank reconciliation", route: "finance/bank", area: "cash", icon: "bank", keywords: ["bank statements", "cash", "reconciliation"] },
  { id: "finance-supplier-batches", label: "Supplier batches", route: "finance/supplier-batches", area: "cash", icon: "purchasing", keywords: ["payment file", "supplier instructions"], tool: true },
  { id: "finance-refunds", label: "Refunds", route: "finance/refunds", area: "cash", icon: "bank", keywords: ["returns", "refund obligations"], tool: true },
  { id: "finance-ledger", label: "Journals", route: "finance/accounting", area: "accounting", icon: "ledger", keywords: ["accounting", "general ledger", "posting"] },
  { id: "finance-tax", label: "Tax", route: "finance/tax", area: "accounting", icon: "ledger", keywords: ["VAT", "ภ.พ.30", "tax register", "filing workpapers"] },
  { id: "finance-close", label: "Period close", route: "finance/close", area: "accounting", icon: "routine", keywords: ["close checklist", "revaluation", "reconciliation"] },
  { id: "finance-hotels", label: "Hotel source reconciliation", route: "finance/hotels", area: "accounting", icon: "pms", keywords: ["hotel accounting", "PMS GL", "advances", "deposits"], tool: true, hotel: true },
  { id: "finance-restaurants", label: "Restaurant accounting", route: "finance/restaurants", area: "accounting", icon: "commerce", keywords: ["POS accounting", "settlement", "daily confirmation"], tool: true },
  { id: "finance-hospitality-settings", label: "Hotel accounting settings", route: "finance/hospitality-settings", area: "setup", icon: "pms", keywords: ["PMS accounting mapping"], tool: true, hotel: true },
  { id: "finance-engines", label: "Calculations & schedules", route: "finance/engines", area: "accounting", icon: "routine", keywords: ["calculation engines", "cash flow", "accruals", "goodwill", "NCI", "equity method", "manufacturing costing"], tool: true, advanced: true },
  { id: "finance-reports", label: "Company reports", route: "finance/reports", area: "reports", icon: "ledger", keywords: ["reports", "trial balance", "financial statements", "aging"] },
  { id: "finance-group", label: "Group reports", route: "finance/group", area: "reports", icon: "organization", keywords: ["group reporting", "consolidation", "intercompany", "NCI"], holdingOnly: true, advanced: true },
  { id: "finance-setup", label: "Finance setup", route: "finance/setup", area: "setup", icon: "settings", keywords: ["chart of accounts", "opening balances", "policies", "bank accounts"] },
];

export const financeAreas = [
  { id: "today", label: "Today", destination: "finance-overview" },
  { id: "documents", label: "Documents", destination: "finance-sales" },
  { id: "cash", label: "Cash", destination: "finance-payments" },
  { id: "accounting", label: "Accounting", destination: "finance-ledger" },
  { id: "reports", label: "Reports", destination: "finance-reports" },
] as const;

export function financeNavigation(context: NavigationContext): NavigationItem[] {
  const actor = { role: context.role ?? "employee", permissions: context.permissions, assignmentScope: context.scopeKind };
  if (context.beta || !hasPermission(actor, "finance.read") || (context.availableAppIds && !context.availableAppIds.includes("finance"))) return [];
  return financeDestinations.filter(destination =>
    (!destination.advanced || !context.availableAppIds) &&
    (!destination.holdingOnly || (context.scopeKind ?? "holding") === "holding") &&
    (!destination.hotel || (context.scopeKind !== "department" && hasPermission(actor, "pms.reports"))) &&
    (!destination.expense || hasPermission(actor, "expense.read") || hasPermission(actor, "expense.review.read")),
  ).map(destination => ({
    id: destination.id, label: destination.label, href: `${context.workspaceBase}/${destination.route}`,
    group: "Finance", icon: destination.icon, keywords: destination.keywords,
  }));
}

export function financeAreaForDestination(id: string): FinanceArea {
  return financeDestinations.find(destination => destination.id === id)?.area ?? "today";
}

export function financeAreaItems(items: readonly NavigationItem[], area: FinanceArea) {
  return items.filter(item => {
    const destination = financeDestinations.find(entry => entry.id === item.id);
    return destination?.area === area && !destination.tool;
  });
}

export function financeRootItems(items: readonly NavigationItem[]): NavigationItem[] {
  return financeAreas.flatMap(area => {
    const item = items.find(entry => entry.id === area.destination);
    return item ? [{ ...item, label: area.label }] : [];
  });
}

export function financeSidebarId(destinationId: string) {
  const area = financeAreaForDestination(destinationId);
  if (area === "setup") return "finance-setup";
  return financeAreas.find(entry => entry.id === area)?.destination ?? "finance-overview";
}

/** Preserve finance context without carrying record-specific filters or cursors. */
export function financeContextHref(href: string, context: Pick<URLSearchParams, "get">) {
  const target = new URL(href, "https://navigation.invalid");
  const chart = context.get("chart") ?? context.get("chartId");
  if (chart && !target.pathname.endsWith("/expenses")) {
    const chartKey = /\/finance\/invoices(?:\/|$)/.test(target.pathname) ? "chartId" : "chart";
    target.searchParams.set(chartKey, chart);
  }
  if (target.pathname.endsWith("/finance/reports")) {
    for (const key of ["periodId", "startDate", "endDate"]) {
      const value = context.get(key);
      if (value) target.searchParams.set(key, value);
    }
  }
  if (target.pathname.endsWith("/finance/invoices") && context.get("asOf")) target.searchParams.set("asOf", context.get("asOf")!);
  return `${target.pathname}${target.search}${target.hash}`;
}

export function financeDestinationForRoute(section: string | undefined, kind?: string | null) {
  return ({
    "due-drafts":"finance-overview",
    ledger: "finance-ledger", accounting: "finance-ledger", journals: "finance-ledger", assets: "finance-ledger", "book-adjustments": "finance-ledger", instruments: "finance-ledger", "asset-policies": "finance-ledger",
    sales: "finance-sales",
    invoices: kind === "payable" ? "finance-payables" : "finance-receivables",
    contracts: "finance-receivables", collections: "finance-payments",
    restaurants:"finance-restaurants", "hospitality-settings":"finance-hospitality-settings", "hotel-accounts":"finance-hotels", "hotel-documents":"finance-hotels", "hotel-import":"finance-hotels", payments: "finance-payments", bank: "finance-bank", hotels: "finance-hotels", close: "finance-close",
    tax: "finance-tax", reports: "finance-reports", engines: "finance-engines", group: "finance-group", "group-editions":"finance-group",
    setup: "finance-setup", refunds: "finance-refunds", "supplier-batches": "finance-supplier-batches",
  } as Record<string, string>)[section ?? ""] ?? "finance-overview";
}
