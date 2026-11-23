import type { WorkContext } from "@erpx/server-identity_access/public/authorization";
import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
import type { OwnerCommandFlowTarget } from "@erpx/contracts/command-flow";
import { expensePaymentDraftExpenseId } from "@erpx/server-people_payroll/adapters/document-page-payment-source";

type Actor = Pick<WorkContext, "userId" | "assignmentId" | "role" | "companyId" | "branchId" | "departmentId" | "authority" | "iam">;

/** The Expense owner resolves its payment identity; the caller owns the commit. */
export async function expensePaymentCommandFlowTarget(db: SqlDatabase, actor: Actor, operation: string, recordId: string): Promise<OwnerCommandFlowTarget | null> {
  if(operation === "expense.payment.cancel")return null;
  const expenseId = operation === "expense.payment.prepare" ? recordId : (await expensePaymentDraftExpenseId(db, recordId, actor.companyId))?.expense_id;
  return expenseId ? { kind: "expense", id: expenseId, action: "payment" } : null;
}
