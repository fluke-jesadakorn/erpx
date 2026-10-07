import type { WorkContext } from "@/server/modules/identity_access/public/authorization";
import type { SqlDatabase, SqlStatement } from "@/server/modules/runtime/public/runtime/types";
import { executeCommand, replayCommand } from "@/server/modules/workflow/public/commands";
import { ApiError } from "@/server/modules/runtime/public/http";
import type { ExpenseCommandActor, ExpenseCommandPort, ExpenseMutation } from "../application/expense-commands";

const nativeMutation = Symbol("native-expense-mutation");
type NativeMutation = { [nativeMutation]: true; statements: SqlStatement[] };

export function expenseMutation<Result extends Record<string, unknown>>(result: Result, statements: SqlStatement[]): ExpenseMutation<Result> {
  return { result, native: { [nativeMutation]: true, statements } satisfies NativeMutation };
}
export function expenseStatements(mutation: ExpenseMutation): SqlStatement[] {
  const native = mutation.native as NativeMutation;
  if (!native?.[nativeMutation]) throw new Error("NATIVE_EXPENSE_MUTATION_REQUIRED");
  return native.statements;
}

/** Each port is bound to one database/owned transaction; it never resolves runtime. */
export function expenseCommandPort<Actor extends ExpenseCommandActor = WorkContext>(database: SqlDatabase): ExpenseCommandPort<Actor> {
  return {
    replay: (actor, key, operation, id, input) => replayCommand(database, actor, key, operation, id, input),
    error: violation => new ApiError(violation.status, violation.code, violation.message),
    async commit(actor, key, operation, input, id, version, mutation, scope) {
      // executeCommand preserves guards, receipt, audit and outbox in the same
      // committing batch. PostgreSQL checks that every statement owns this DB.
      return executeCommand(database, actor, key, operation, input, id, version, mutation.result, expenseStatements(mutation), scope ?? actor);
    },
  };
}
