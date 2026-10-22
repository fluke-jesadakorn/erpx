import type { WorkContext } from "@erpx/server-identity_access/public/authorization";
import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
import type { OwnerCommandFlowTarget } from "@erpx/contracts/command-flow";
type Actor = Pick<WorkContext, "userId" | "assignmentId" | "role" | "companyId" | "branchId" | "departmentId" | "authority" | "iam">;
/** Resolve the owner identity without importing or executing Workflow. */
export async function invoiceCommandFlowTarget(db: SqlDatabase, actor: Actor, deliveryId: string): Promise<OwnerCommandFlowTarget | null> {
    const delivery = await db.prepare("SELECT f.order_id FROM commerce_fulfillments f JOIN commerce_orders o ON o.id=f.order_id WHERE f.id=? AND o.company_id=?").bind(deliveryId, actor.companyId).first<{
        order_id: string;
    }>();
    return delivery ? { kind: "order", id: delivery.order_id, action: "invoice" } : null;
}
