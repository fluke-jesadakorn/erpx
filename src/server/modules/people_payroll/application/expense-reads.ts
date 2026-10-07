import type { WorkContext } from "@/server/modules/identity_access/public/authorization";
import type { ExpenseRecord, ExpenseCommandPort } from "./expense-commands";

export interface ExpenseReadPorts<Row extends ExpenseRecord> {
  legacyIds(actor: WorkContext): Promise<string[]>;
  isLegacy(actor: WorkContext, id: string): Promise<boolean>;
  initializeApproval(actor: WorkContext, id: string): Promise<void>;
  visibleRows(actor: WorkContext): Promise<Row[]>;
  visibleRow(actor: WorkContext, id: string): Promise<Row | null>;
  canRead(actor: WorkContext, row: Row): Promise<boolean>;
  decorate(actor: WorkContext, row: Row): Promise<Row>;
  error: ExpenseCommandPort["error"];
}

/** Native visibility queries and current IAM checks remain mandatory owner ports. */
export function createExpenseReads<Row extends ExpenseRecord>(ports: ExpenseReadPorts<Row>) {
  return {
    async list(actor: WorkContext): Promise<Row[]> {
      for (const id of await ports.legacyIds(actor)) await ports.initializeApproval(actor, id);
      const visible = await Promise.all((await ports.visibleRows(actor)).map(async (row): Promise<{ row: Row | null }> => ({ row: await ports.canRead(actor, row) ? await ports.decorate(actor, row) : null })));
      return visible.flatMap(value => value.row === null ? [] : [value.row]);
    },
    async get(actor: WorkContext, id: string): Promise<Row> {
      if (await ports.isLegacy(actor, id)) await ports.initializeApproval(actor, id);
      const row = await ports.visibleRow(actor, id);
      if (!row || !await ports.canRead(actor, row)) throw ports.error({ status: 404, code: "EXPENSE_NOT_FOUND", message: "Expense claim not found in your workspace." });
      return ports.decorate(actor, row);
    },
  };
}
