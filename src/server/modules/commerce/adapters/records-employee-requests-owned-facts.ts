import { commerceFinalOwnerProjectionSources as procurementProjectionSources } from "../../procurement/public/commerce-final-owner-projection-sources";
import type { SqlDatabase } from "@/server/modules/runtime/public/runtime/types";
import { assertion } from "@/server/modules/workflow/public/commands";
export function EmployeeRequestsCommerceOwnedFacts(database: SqlDatabase) {
    return { readActOnEmployeeRequestOrders(company_id: string, branch_id: string | null, department_id: string | null, purchase_order_id: string, id: string) {
            return database.prepare(`SELECT o.id,o.state,o.version FROM commerce_orders o WHERE o.company_id=? AND o.kind='purchase' AND o.branch_id IS NOT DISTINCT FROM ? AND o.department_id IS NOT DISTINCT FROM ? AND (o.id=? OR EXISTS(SELECT 1 FROM (${procurementProjectionSources.procurement_request_orders}) l WHERE l.request_id=? AND l.order_id=o.id)) ORDER BY o.id`).bind(company_id, branch_id, department_id, purchase_order_id, id).all<{
                id: string;
                state: string;
                version: number;
            }>();
        },
        guard15(key: string, value: string, id: string, company_id: string, version: number, state: string, branch_id: string | null, department_id: string | null) {
            return assertion(database, key, value, "EXISTS(SELECT 1 FROM commerce_orders WHERE id=? AND company_id=? AND version=? AND state=? AND kind='purchase' AND branch_id IS NOT DISTINCT FROM ? AND department_id IS NOT DISTINCT FROM ?)", [
                id,
                company_id,
                version,
                state,
                branch_id,
                department_id,
            ]);
        },
        readActOnEmployeeRequestOrder(purchaseOrderId: string, company_id: string) {
            return database.prepare("SELECT kind,company_id,branch_id,department_id,state,version FROM commerce_orders WHERE id=? AND company_id=?").bind(purchaseOrderId, company_id).first<{
                kind: string;
                company_id: string;
                branch_id: string;
                department_id: string | null;
                state: string;
                version: number;
            }>();
        },
        guard20(key: string, value: "linked-order-version", purchaseOrderId: string, company_id: string, value2: number, value3: string, branch_id: string, department_id: string | null) {
            return assertion(database, key, value, "EXISTS(SELECT 1 FROM commerce_orders WHERE id=? AND company_id=? AND version=? AND state=? AND kind='purchase' AND branch_id IS NOT DISTINCT FROM ? AND department_id IS NOT DISTINCT FROM ?)", [
                purchaseOrderId,
                company_id,
                value2,
                value3,
                branch_id,
                department_id,
            ]);
        } };
}
