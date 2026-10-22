import "server-only";
import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
import type { WorkContext } from "@erpx/server-identity_access/public/authorization";
import { ApiError } from "@erpx/server-runtime/public/http";
import { readPurchasedAccesses, posPackageEnforcementMode } from "@erpx/server-commercial/public/purchase-command-boundary";
import { assignmentReadSql } from "@erpx/server-identity_access/public/record-source-authority";
const capabilities = { "pos-sales": "pos.sales", "net-pos-sales": "pos.sales", "pos-payment-mix": "pos.payments" } as const;
export async function posMetricHistoryAccess(database: SqlDatabase, actor: WorkContext) {
  const mode = await posPackageEnforcementMode(), access = await readPurchasedAccesses(database, mode, actor.companyId, [...new Set(Object.values(capabilities))]);
  const current = mode === "disabled" || !!await database.prepare(`SELECT ${assignmentReadSql("metric_scope", "pos.reports")} allowed
    FROM (SELECT ?::text company_id,?::text branch_id,?::text department_id) metric_scope`)
    .bind(actor.assignmentId, actor.userId, actor.role, actor.companyId, actor.branchId, actor.departmentId).first("allowed");
  return Object.fromEntries(Object.entries(capabilities).map(([id, capability]) => [id, access[capability].history && current])) as Record<keyof typeof capabilities, boolean>;
}
export async function requirePosMetricPurchase(database: SqlDatabase, actor: WorkContext, id: string) {
  if (!(await posMetricHistoryAccess(database, actor))[id as keyof typeof capabilities]) throw new ApiError(403, "METRIC_SOURCE_UNAVAILABLE", "The POS metric requires its source package and current source authority.");
}
