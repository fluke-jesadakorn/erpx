import "server-only";
import type {SqlDatabase} from "../../runtime/public/runtime/types";
/** Exact quote/sale source identity; the CRM owner still checks the original
 * customer, scope, state and captured document versions. */
export const crmCommerceOrderSources="SELECT id,company_id,branch_id,department_id,kind,party_id,currency,source_id FROM commerce_orders";
export function crmCommerceSourceReadLocks(database:SqlDatabase){return [database.prepare("LOCK TABLE commerce_orders IN SHARE MODE")];}
