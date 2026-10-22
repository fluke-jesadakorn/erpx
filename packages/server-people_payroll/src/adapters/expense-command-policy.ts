import type { WorkContext } from "@erpx/server-identity_access/public/authorization";
import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
import type { OwnerCommandFlowTarget } from "@erpx/contracts/command-flow";

type Actor = Pick<WorkContext, "userId" | "assignmentId" | "role" | "companyId" | "branchId" | "departmentId" | "authority" | "iam">;

/** The Expense owner resolves its payment identity; the caller owns the commit. */
export async function expensePaymentCommandFlowTarget(db: SqlDatabase, actor: Actor, operation: string, recordId: string): Promise<OwnerCommandFlowTarget | null> {
  if(operation === "expense.payment.cancel")return null;
  const expenseId = operation === "expense.payment.prepare" ? recordId : (await db.prepare("SELECT expense_id FROM expense_payment_drafts WHERE id=? AND company_id=?").bind(recordId, actor.companyId).first<{ expense_id: string }>())?.expense_id;
  return expenseId ? { kind: "expense", id: expenseId, action: "payment" } : null;
}
