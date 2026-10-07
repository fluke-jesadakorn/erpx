import type { WorkContext } from "@/server/modules/identity_access/public/authorization";
import type { SqlDatabase } from "@/server/modules/runtime/public/runtime/types";
import { prepareFlowCommand } from "@/server/modules/workflow/public/flow-routing";

type Actor = Pick<WorkContext, "userId" | "assignmentId" | "role" | "companyId" | "branchId" | "departmentId" | "authority" | "iam">;

/** The Expense owner resolves its payment identity; the caller owns the commit. */
export async function expensePaymentCommandFlow(db: SqlDatabase, actor: Actor, command: string, operation: string, recordId: string, input: unknown, version: number) {
  const expenseId = operation === "expense.payment.prepare" ? recordId : (await db.prepare("SELECT expense_id FROM expense_payment_drafts WHERE id=? AND company_id=?").bind(recordId, actor.companyId).first<{ expense_id: string }>())?.expense_id;
  return expenseId ? prepareFlowCommand(db, actor, command, "expense", expenseId, "payment", input, version) : null;
}
