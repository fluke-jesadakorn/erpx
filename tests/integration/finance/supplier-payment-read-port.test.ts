import { describe, expect, it, vi } from "vitest";
import type { WorkContext } from "@erpx/server-identity_access/public/authorization";
import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";

vi.mock("@erpx/server-finance/adapters/supplier-payment-purchases", () => ({
  supplierPaymentSourceOptions: vi.fn(async () => ({ sourceOnly: false })),
  supplierBatchOwnedSql: vi.fn(() => "TRUE"),
}));

const { supplierPaymentRepository } = await import(
  "@erpx/server-finance/adapters/supplier-payment-repository"
);

type Call = { sql: string; args: unknown[] };
function recordingDb(): { calls: Call[]; database: SqlDatabase } {
  const calls: Call[] = [];
  const statement = (sql: string) => {
    let args: unknown[] = [];
    return {
      bind(...values: unknown[]) {
        args = values;
        return this;
      },
      async first() {
        calls.push({ sql, args });
        return null;
      },
      async all() {
        calls.push({ sql, args });
        const marker = /available_minor/.test(sql)
          ? "invoices"
          : /invoice_number,i\.currency/.test(sql)
            ? "items"
            : /finance_bank_transactions/.test(sql)
              ? "bank"
              : /finance_periods/.test(sql)
                ? "periods"
                : "accounts";
        return { results: [{ marker }] };
      },
    };
  };
  return { calls, database: { prepare: statement } as unknown as SqlDatabase };
}

const actor = { companyId: "c-1", userId: "u-1" } as WorkContext;

describe("supplier payment read-port workspace composition", () => {
  it("returns the complete native workspace shape from the bounded port", async () => {
    const { calls, database } = recordingDb();
    const port = supplierPaymentRepository(database, actor, { invoiceId: "inv-1" });
    const workspace = await port.workspace("b-1", { bank_account_id: "cash-1" } as never);

    expect(Object.keys(workspace).sort()).toEqual(["accounts", "bank", "invoices", "items", "periods"]);
    // With a selected batch the instruction list loads; the invoice picker is
    // only composed for a new batch, so it is empty here.
    expect(workspace.items).toEqual([{ marker: "items" }]);
    expect(workspace.invoices).toEqual([]);
    expect(workspace.accounts).toEqual([{ marker: "accounts" }]);
    expect(workspace.periods).toEqual([{ marker: "periods" }]);

    // Every workspace statement is issued through the port and stays in the actor's Company.
    // Instruction list + account/period setup lists are issued; the invoice
    // picker needs a new batch and the bank list needs finance.pay, so both skip.
    expect(calls).toHaveLength(3);
    expect(
      calls.filter((call) => /finance_accounts|finance_periods/.test(call.sql))
        .every((call) => call.args.includes("c-1")),
    ).toBe(true);
    expect(calls.find((call) => /invoice_number,i\.currency/.test(call.sql))?.args).toEqual(["b-1"]);
  });

  it("scopes the new-batch invoice picker by company and optional source invoice", async () => {
    const { calls, database } = recordingDb();
    const port = supplierPaymentRepository(database, actor, { invoiceId: "inv-1" });
    await port.workspace(undefined, null);
    const invoiceQuery = calls.find((call) => /available_minor/.test(call.sql));
    expect(invoiceQuery?.args).toEqual(["c-1", "inv-1"]);
  });

  it("returns empty collections for a new batch with no id and no selected batch", async () => {
    const { database } = recordingDb();
    const port = supplierPaymentRepository(database, actor);
    const workspace = await port.workspace(undefined, null);
    // A new batch has no instructions/invoices/bank match, but the account and
    // period setup lists are always part of the native workspace read.
    expect(workspace).toEqual({
      items: [],
      invoices: [{ marker: "invoices" }],
      accounts: [{ marker: "accounts" }],
      periods: [{ marker: "periods" }],
      bank: [],
    });
  });
});
