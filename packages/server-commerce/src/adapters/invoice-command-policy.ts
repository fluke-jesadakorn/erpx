import type { WorkContext } from "@erpx/server-identity_access/public/authorization";
import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
import type { OwnerCommandFlowTarget } from "@erpx/contracts/command-flow";
import { nativeCommerceInvoiceTarget } from './native-lane-13/client';
type Actor = Pick<WorkContext, "userId" | "assignmentId" | "role" | "companyId" | "branchId" | "departmentId" | "authority" | "iam">;
/**
 * Native owner delivery -> order identity read. The `database` argument is
 * retained only for call-site compatibility; the mounted axum owner executes
 * the Company-scoped statement and projects the command target. No TypeScript
 * SQL or fallback remains; an unavailable native runtime fails closed 503.
 */
export async function invoiceCommandFlowTarget(_db: SqlDatabase, actor: Actor, deliveryId: string): Promise<OwnerCommandFlowTarget | null> {
    return nativeCommerceInvoiceTarget(deliveryId, { companyId: actor.companyId });
}
