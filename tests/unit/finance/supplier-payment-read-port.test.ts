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
function db(rows: Record<string, unknown>[] = []): { calls: Call[]; database: SqlDatabase } {
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
        return rows[0] ?? null;
      },
      async all() {
        calls.push({ sql, args });
        return { results: rows };
      },
    };
  };
  return { calls, database: { prepare: statement } as unknown as SqlDatabase };
}

const actor = { companyId: "c-1", userId: "u-1" } as WorkContext;

describe("supplier payment native read port", () => {
  it("exposes only the bounded read surface consumed by procurement", () => {
    const port = supplierPaymentRepository(db().database, actor);
    expect(Object.keys(port).sort()).toEqual(["batch", "item", "list", "workspace"]);
  });

  it("scopes the batch and item reads to the actor company", async () => {
    const { calls, database } = db([{ id: "b-1" }]);
    const port = supplierPaymentRepository(database, actor);
    await port.batch("b-1");
    await port.item("i-1");
    expect(calls).toHaveLength(2);
    expect(calls[0].args).toEqual(["b-1", "c-1"]);
    expect(calls[1].args).toEqual(["i-1", "c-1"]);
    expect(calls[0].sql).toContain("finance_supplier_batches");
    expect(calls[1].sql).toContain("finance_supplier_instructions");
  });

  it("scopes the list read and drops the work-only row cap for work lists", async () => {
    const consoleList = db([{ id: "b-1" }]);
    const portConsole = supplierPaymentRepository(consoleList.database, actor);
    await portConsole.list(false);
    expect(consoleList.calls[0].args).toEqual(["c-1"]);
    expect(consoleList.calls[0].sql).toContain("LIMIT 200");
    expect(consoleList.calls[0].sql).toContain("b.company_id=?");

    const workList = db([{ id: "b-1" }]);
    const portWork = supplierPaymentRepository(workList.database, actor);
    await portWork.list(true);
    expect(workList.calls[0].sql).not.toContain("LIMIT 200");
  });
});
