import { describe, expect, it, vi } from "vitest";
import type { WorkContext } from "@erpx/identity-access-server/public/authorization";
import type { SqlDatabase } from "@erpx/platform-runtime-server/public/runtime/types";

const calls: { kind: string; values: unknown[] }[] = [];
vi.mock("@erpx/finance-server/adapters/supplier-payment-purchases", () => ({
  supplierPaymentSourceOptions: vi.fn(async () => ({ sourceOnly: false })),
}));
vi.mock("../../../modules/finance/server/src/adapters/native-finance-read", () => {
  const record = (kind: string, values: readonly unknown[]) => {
    calls.push({ kind, values: [...values] });
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
    nativeFinanceReadAll: vi.fn(async (kind: string, values: readonly unknown[]) => {
      record(kind, values);
      return { results: [], success: true, meta: { changes: 0, duration: 0 } };
    }),
    nativeFinanceReadFirst: vi.fn(async (kind: string, values: readonly unknown[]) => {
      record(kind, values);
      return null;
    }),
  };
});

const { supplierPaymentRepository } = await import(
  "@erpx/finance-server/adapters/supplier-payment-repository"
);

const actor = { companyId: "c-1", userId: "u-1" } as WorkContext;
const db = {} as SqlDatabase;

describe("supplier payment native read port", () => {
  it("exposes only the bounded read surface consumed by procurement", () => {
    const port = supplierPaymentRepository(db, actor);
    expect(Object.keys(port).sort()).toEqual(["batch", "item", "list", "workspace"]);
  });

  it("routes the batch and item reads to the owner-planned row kinds", async () => {
    calls.length = 0;
    const port = supplierPaymentRepository(db, actor);
    await port.batch("b-1");
    await port.item("i-1");
    expect(calls).toEqual([
      { kind: "finance_rd_supplier_batch_row", values: ["b-1", "c-1"] },
      { kind: "finance_rd_supplier_instruction_row", values: ["i-1", "c-1"] },
    ]);
  });

  it("selects the capped console list and the uncapped work list kinds", async () => {
    calls.length = 0;
    const port = supplierPaymentRepository(db, actor);
    await port.list(false);
    await port.list(true);
    expect(calls).toEqual([
      { kind: "finance_rd_supplier_batch_list", values: ["c-1", false] },
      { kind: "finance_rd_supplier_batch_work_list", values: ["c-1", false] },
    ]);
  });
});
