import type { WorkContext } from "@/server/modules/identity_access/public/authorization";
import type { SqlDatabase } from "@/server/modules/runtime/public/runtime/types";
import { prepareFlowCommand } from "@/server/modules/workflow/public/flow-routing";
type Actor = Pick<WorkContext, "userId" | "assignmentId" | "role" | "companyId" | "branchId" | "departmentId" | "authority" | "iam">;
/** Returns statements to join the caller's receipt/audit/outbox transaction. */
export async function invoiceCommandFlow(db: SqlDatabase, actor: Actor, command: string, deliveryId: string, input: unknown, version: number) {
    const delivery = await db.prepare("SELECT f.order_id FROM commerce_fulfillments f JOIN commerce_orders o ON o.id=f.order_id WHERE f.id=? AND o.company_id=?").bind(deliveryId, actor.companyId).first<{
        order_id: string;
    }>();
    return delivery ? prepareFlowCommand(db, actor, command, "order", delivery.order_id, "invoice", input, version) : null;
}
