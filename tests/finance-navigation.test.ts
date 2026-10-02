import { describe, expect, it } from "vitest";
import { financeAreaItems, financeContextHref, financeNavigation } from "../src/lib/finance-navigation";
import { resolveNavigation, workplaceNavigation, type NavigationContext } from "../src/lib/workplace-navigation";

const base = "/holdings/acme";
const context: NavigationContext = { workspaceBase: base, role: "holding_owner", scopeKind: "holding", financeWorkspaceV2: true };

describe("Finance area navigation", () => {
  it("keeps discovery and existing detail URLs while highlighting one of five areas", () => {
    const model = workplaceNavigation(context);
    expect(model.categories.find(category => category.id === "finance")?.apps.map(item => item.label)).toEqual(["Today", "Documents", "Cash", "Accounting", "Reports"]);
    for (const [route, destination, sidebar] of [
      ["finance/invoices/invoice-1?kind=payable", "finance-payables", "finance-receivables"],
      ["finance/payments/payment-1", "finance-payments", "finance-payments"],
      ["finance/supplier-batches/batch-1", "finance-supplier-batches", "finance-payments"],
      ["finance/refunds/refund-1", "finance-refunds", "finance-payments"],
      ["finance/journals/journal-1", "finance-ledger", "finance-ledger"],
      ["finance/tax/filing-1", "finance-tax", "finance-ledger"],
      ["finance/engines", "finance-engines", "finance-ledger"],
      ["finance/group", "finance-group", "finance-reports"],
      ["expenses/expense-1", "finance-expenses", "finance-receivables"],
    ]) {
      const url = new URL(`${base}/${route}`, "https://test.invalid");
      expect(resolveNavigation(model, base, url.pathname, url.searchParams), route).toMatchObject({ activeId: destination, sidebarId: sidebar });
      expect(model.search.some(item => item.id === destination), destination).toBe(true);
    }
    const items = financeNavigation(context);
    for (const area of ["documents", "cash", "accounting", "reports"] as const) expect(financeAreaItems(items, area).length).toBeLessThanOrEqual(3);
  });

  it("retains role, scope and release gates without exposing hidden tools in search", () => {
    expect(financeNavigation({ ...context, role: "employee" })).toEqual([]);
    expect(financeNavigation({ ...context, beta: true })).toEqual([]);
    expect(financeNavigation({ ...context, availableAppIds: ["people"] })).toEqual([]);
    const production = financeNavigation({ ...context, availableAppIds: ["finance"] });
    expect(production.some(item => ["finance-engines", "finance-group"].includes(item.id))).toBe(false);
    const branch = financeNavigation({ ...context, scopeKind: "branch" });
    expect(branch.some(item => item.id === "finance-group")).toBe(false);
    const reader = financeNavigation({ ...context, role: "employee", permissions: ["finance.read"] });
    expect(reader.some(item => item.id === "finance-hotels")).toBe(false);
    expect(reader.some(item => item.id === "finance-expenses")).toBe(false);
  });

  it("carries financial context without moving list cursors or changing the destination kind", () => {
    const search = new URLSearchParams({ chart: "chart-2", periodId: "period-2", startDate: "2026-09-01", asOf: "2026-09-30", cursor: "other-record", status: "draft" });
    const payable = new URL(financeContextHref(`${base}/finance/invoices?kind=payable&queue=open`, search), "https://test.invalid");
    expect(Object.fromEntries(payable.searchParams)).toEqual({ kind: "payable", queue: "open", chartId: "chart-2", asOf: "2026-09-30" });
    const reports = new URL(financeContextHref(`${base}/finance/reports`, search), "https://test.invalid");
    expect(Object.fromEntries(reports.searchParams)).toEqual({ chart: "chart-2", periodId: "period-2", startDate: "2026-09-01" });
    expect(financeContextHref(`${base}/expenses`, search)).toBe(`${base}/expenses`);
  });
});
