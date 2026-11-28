import { describe, expect, it, vi } from "vitest";
import type { WorkContext } from "@erpx/identity-access-server/public/authorization";
import type { SqlDatabase } from "@erpx/platform-runtime-server/public/runtime/types";

type Call = { kind: string; values: unknown[] };
const calls: Call[] = [];
vi.mock("@erpx/finance-server/adapters/supplier-payment-purchases", () => ({
  supplierPaymentSourceOptions: vi.fn(async () => ({ sourceOnly: false })),
}));
vi.mock("../../../modules/finance/server/src/adapters/native-finance-read", () => {
  const marker = (kind: string): string =>
    kind === "finance_rd_supplier_batch_items"
      ? "items"
      : kind === "finance_rd_supplier_batch_invoice" || kind === "finance_rd_supplier_batch_invoices"
        ? "invoices"
        : kind === "finance_rd_supplier_batch_bank"
          ? "bank"
          : kind === "finance_rd_supplier_batch_periods"
            ? "periods"
            : "accounts";
  const all = (kind: string, values: readonly unknown[]) => {
    calls.push({ kind, values: [...values] });
    return { results: [{ marker: marker(kind) }], success: true, meta: { changes: 0, duration: 0 } };
  };
  return {
    FINANCE_RD_SUPPLIER_BATCH_ROW: "finance_rd_supplier_batch_row",
    FINANCE_RD_SUPPLIER_INSTRUCTION_ROW: "finance_rd_supplier_instruction_row",
    FINANCE_RD_SUPPLIER_BATCH_LIST: "finance_rd_supplier_batch_list",
    FINANCE_RD_SUPPLIER_BATCH_WORK_LIST: "finance_rd_supplier_batch_work_list",
    FINANCE_RD_SUPPLIER_BATCH_ITEMS: "finance_rd_supplier_batch_items",
    FINANCE_RD_SUPPLIER_BATCH_INVOICES: "finance_rd_supplier_batch_invoices",
    FINANCE_RD_SUPPLIER_BATCH_INVOICE: "finance_rd_supplier_batch_invoice",
    FINANCE_RD_SUPPLIER_BATCH_ACCOUNTS: "finance_rd_supplier_batch_accounts",
    FINANCE_RD_SUPPLIER_BATCH_PERIODS: "finance_rd_supplier_batch_periods",
    FINANCE_RD_SUPPLIER_BATCH_BANK: "finance_rd_supplier_batch_bank",
    nativeFinanceReadAll: vi.fn(all),
    nativeFinanceReadFirst: vi.fn(async (kind: string, values: readonly unknown[]) => {
      calls.push({ kind, values: [...values] });
      return { id: values[0] };
    }),
  };
});

const { supplierPaymentRepository } = await import(
  "@erpx/finance-server/adapters/supplier-payment-repository"
);

const actor = { companyId: "c-1", userId: "u-1" } as WorkContext;
const db = {} as SqlDatabase;

describe("supplier payment native read-port workspace composition", () => {
  it("returns the complete native workspace shape from the bounded port", async () => {
    calls.length = 0;
    const port = supplierPaymentRepository(db, actor, { invoiceId: "inv-1" });
    const workspace = await port.workspace("b-1", { bank_account_id: "cash-1" } as never);

    expect(Object.keys(workspace).sort()).toEqual(["accounts", "bank", "invoices", "items", "periods"]);
    // With a selected batch the instruction list loads; the invoice picker is
    // only composed for a new batch, so it is empty here.
    expect(workspace.items).toEqual([{ marker: "items" }]);
    expect(workspace.invoices).toEqual([]);
    expect(workspace.accounts).toEqual([{ marker: "accounts" }]);
    expect(workspace.periods).toEqual([{ marker: "periods" }]);
    expect(workspace.bank).toEqual([]);

    // The Company scope reaches every setup read; finance.pay gates the bank list.
    expect(calls).toHaveLength(3);
    expect(calls.map((call) => call.kind).sort()).toEqual([
      "finance_rd_supplier_batch_accounts",
      "finance_rd_supplier_batch_items",
      "finance_rd_supplier_batch_periods",
    ]);
    expect(
      calls
        .filter((call) => call.kind !== "finance_rd_supplier_batch_items")
        .every((call) => call.values.includes("c-1")),
    ).toBe(true);
    expect(calls.find((call) => call.kind === "finance_rd_supplier_batch_items")?.values).toEqual([
      "b-1",
    ]);
  });

  it("narrows the new-batch invoice picker by the exact source invoice", async () => {
    calls.length = 0;
    const port = supplierPaymentRepository(db, actor, { invoiceId: "inv-1" });
    await port.workspace(undefined, null);
    const picker = calls.find((call) => call.kind === "finance_rd_supplier_batch_invoice");
    expect(picker?.values).toEqual(["c-1", false, "inv-1"]);
  });

  it("uses the unbounded picker when no invoiceId was supplied", async () => {
    calls.length = 0;
    const port = supplierPaymentRepository(db, actor);
    await port.workspace(undefined, null);
    expect(calls.map((call) => call.kind)).toContain("finance_rd_supplier_batch_invoices");
    expect(calls.every((call) => call.values.length < 3)).toBe(true);
  });
});
