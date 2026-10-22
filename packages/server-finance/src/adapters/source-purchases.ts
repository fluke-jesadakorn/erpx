import "server-only";
import type {SqlDatabase} from "@erpx/server-runtime/public/runtime/types";
import type {WorkContext} from "@erpx/server-identity_access/public/authorization";
import {assignmentReadSql} from "@erpx/server-identity_access/public/record-source-authority";
import {financePackageEnforcementMode,readPurchasedAccesses} from "@erpx/server-commercial/public/purchase-command-boundary";
import {ApiError} from "@erpx/server-runtime/public/http";

/** Source visibility intersects purchase history with the actual current assignment.
 * Do not use a caller-supplied source label to grant an included posting foundation. */
export async function financeSourceAccess(database:SqlDatabase,actor:WorkContext,capabilities:readonly string[]) {
  const mode=await financePackageEnforcementMode(),access=await readPurchasedAccesses(database,mode,actor.companyId,capabilities);
  const current=mode==="disabled"||!!await database.prepare(`SELECT ${assignmentReadSql("finance_scope","finance.read")} allowed FROM (SELECT ?::text company_id,?::text branch_id,?::text department_id) finance_scope`)
    .bind(actor.assignmentId,actor.userId,actor.role,actor.companyId,actor.branchId,actor.departmentId).first("allowed");
  return Object.fromEntries(capabilities.map(capability=>[capability,{...access[capability],history:access[capability].history&&current,settleExisting:access[capability].settleExisting&&current,newWork:access[capability].newWork&&current}]));
}
export async function financeSourceHistoryAccess(database:SqlDatabase,actor:WorkContext,capabilities:readonly string[]) {
  const access=await financeSourceAccess(database,actor,capabilities);
  return Object.fromEntries(capabilities.map(capability=>[capability,access[capability].history]));
}
const financialMetrics={receivables:"finance.receivables",payables:"finance.payables","invoiced-sales":"finance.accounting","cash-receipts":"finance.banking","close-blockers":"finance.close"} as const;
export async function financeMetricHistoryAccess(database:SqlDatabase,actor:WorkContext):Promise<Record<string,boolean>> {
  const access=await financeSourceHistoryAccess(database,actor,[...new Set(Object.values(financialMetrics))]);
  // Canonical Customer profiles belong to Customer 360 and its reference contract,
  // even though Finance retains the party table. They are not GL management.
  return {...Object.fromEntries(Object.entries(financialMetrics).map(([id,capability])=>[id,access[capability]])),"active-customers":true};
}
export async function requireFinanceMetricPurchase(database:SqlDatabase,actor:WorkContext,id:string) {
  if(!(id in financialMetrics))return;
  if(!(await financeMetricHistoryAccess(database,actor))[id])throw new ApiError(403,"METRIC_SOURCE_UNAVAILABLE","The Finance metric requires its source purchase and current source authority.");
}
