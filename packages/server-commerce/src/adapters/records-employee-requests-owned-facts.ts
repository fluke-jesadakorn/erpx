import { commerceFinalOwnerProjectionSources as procurementProjectionSources } from "@erpx/server-procurement/public/commerce-final-owner-projection-sources";
import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
import { ApiError } from "@erpx/server-runtime/public/http";
import {assertion} from "@erpx/server-workflow/public/command-assertions";
import {COMMERCE_PURCHASE_ORDER_VERSION_GUARD_SQL} from "./guarded-source-dedupe-sql";
import {
  NativeCommerceEmployeeOrderError,
  nativeCommerceEmployeeOrderReader,
} from '@erpx/feature-commerce/public/native-commerce-employee-request-order';

type EmployeeOrder = {
  kind: string;
  company_id: string;
  branch_id: string;
  department_id: string | null;
  state: string;
  version: number;
};
const orderUnavailable = () => new ApiError(
  503,
  'NATIVE_COMMERCE_EMPLOYEE_ORDER_UNAVAILABLE',
  'The native Commerce employee-request order reader is unavailable.',
);
const orderFailure = (error: unknown): never => {
  if (error instanceof NativeCommerceEmployeeOrderError)
    throw new ApiError(error.status, error.code, error.message);
  throw error instanceof Error ? error : orderUnavailable();
};
/** Fail-closed bridge to the mounted owner read. No SQL is prepared on the
 * caller's unit; a missing, malformed or failing native read never falls back. */
async function nativeOrder(purchaseOrderId: string, companyId: string): Promise<EmployeeOrder | null> {
  let reader: ReturnType<typeof nativeCommerceEmployeeOrderReader>;
  try {
    reader = nativeCommerceEmployeeOrderReader();
  } catch (error) {
    throw orderFailure(error);
  }
  // The retired reader typed `branch_id` as string while permitting SQL NULL;
  // the cast preserves that exact runtime shape and its scope comparison.
  return reader
    .read(companyId, purchaseOrderId)
    .then((order) => order as EmployeeOrder | null)
    .catch(orderFailure);
}
export function EmployeeRequestsCommerceOwnedFacts(database: SqlDatabase) {
    return { readActOnEmployeeRequestOrders(company_id: string, branch_id: string | null, department_id: string | null, purchase_order_id: string, id: string) {
            return database.prepare(`SELECT o.id,o.state,o.version FROM commerce_orders o WHERE o.company_id=? AND o.kind='purchase' AND o.branch_id IS NOT DISTINCT FROM ? AND o.department_id IS NOT DISTINCT FROM ? AND (o.id=? OR EXISTS(SELECT 1 FROM (${procurementProjectionSources.procurement_request_orders}) l WHERE l.request_id=? AND l.order_id=o.id)) ORDER BY o.id`).bind(company_id, branch_id, department_id, purchase_order_id, id).all<{
                id: string;
                state: string;
                version: number;
            }>();
        },
        guard15(key: string, value: string, id: string, company_id: string, version: number, state: string, branch_id: string | null, department_id: string | null) {
            return assertion(database, key, value, COMMERCE_PURCHASE_ORDER_VERSION_GUARD_SQL, [
                id,
                company_id,
                version,
                state,
                branch_id,
                department_id,
            ]);
        },
        readActOnEmployeeRequestOrder(purchaseOrderId: string, company_id: string) {
            return nativeOrder(purchaseOrderId, company_id);
        },
        guard20(key: string, value: "linked-order-version", purchaseOrderId: string, company_id: string, value2: number, value3: string, branch_id: string, department_id: string | null) {
            return assertion(database, key, value, COMMERCE_PURCHASE_ORDER_VERSION_GUARD_SQL, [
                purchaseOrderId,
                company_id,
                value2,
                value3,
                branch_id,
                department_id,
            ]);
        } };
}
