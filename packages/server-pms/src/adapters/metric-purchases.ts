import "server-only";
import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
import type { WorkContext } from "@erpx/server-identity_access/public/authorization";
import { ApiError } from "@erpx/server-runtime/public/http";
import { readPurchasedAccesses, pmsPackageEnforcementMode } from "@erpx/server-commercial/public/purchase-command-boundary";
import { assignmentReadSql } from "@erpx/server-identity_access/public/record-source-authority";
const capabilities = { "hotel-rooms": "pms.rooms", "hotel-occupancy": "pms.rooms", "hotel-adr": "pms.rooms", "hotel-revpar": "pms.rooms", "hotel-guest-account-balance": "pms.guest-accounts", "hotel-night-audit-blockers": "pms.night-audit" } as const;
export async function pmsMetricHistoryAccess(database: SqlDatabase, actor: WorkContext) {
  const mode = await pmsPackageEnforcementMode(), access = await readPurchasedAccesses(database, mode, actor.companyId, [...new Set(Object.values(capabilities))]);
  const current = mode === "disabled" ? { readable: true, reports: true } : await database.prepare(`SELECT
    ${assignmentReadSql("metric_scope", "pms.read")} readable,${assignmentReadSql("metric_scope", "pms.reports")} reports
    FROM (SELECT ?::text company_id,?::text branch_id,?::text department_id) metric_scope`)
    .bind(actor.assignmentId, actor.userId, actor.role, actor.assignmentId, actor.userId, actor.role, actor.companyId, actor.branchId, actor.departmentId).first<{ readable: boolean; reports: boolean }>();
  return Object.fromEntries(Object.entries(capabilities).map(([id, capability]) => [id, access[capability].history && !!current?.readable && (id === "hotel-rooms" || !!current?.reports)])) as Record<keyof typeof capabilities, boolean>;
}
export async function requirePmsMetricPurchase(database: SqlDatabase, actor: WorkContext, id: string) {
  if (!(await pmsMetricHistoryAccess(database, actor))[id as keyof typeof capabilities]) throw new ApiError(403, "METRIC_SOURCE_UNAVAILABLE", "The Hotel metric requires its source package and current source authority.");
}
