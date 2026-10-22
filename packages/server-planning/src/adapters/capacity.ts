import {productionReadProjection as productionmanufacturingProjection} from '@erpx/server-manufacturing/public/production-read-projections';
import type { WorkContext } from "@erpx/server-identity_access/public/authorization";
import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
import { ApiError } from "@erpx/server-runtime/public/http";
import { createCapacityPlanner, type CapacityInput } from "@erpx/server-planning/application/capacity";
import type { CapacityOperation } from "@erpx/server-planning/domain/capacity";

/** Bound to the caller's connection, including an existing native transaction. */
export async function calculateProductionCapacity(db: SqlDatabase, actor: WorkContext, input: CapacityInput) {
  return createCapacityPlanner({
    repository: {
      operations: async (specificationId, companyId) => (await db.prepare(`SELECT o.*,r.version resource_version,r.status,r.daily_minutes,r.work_weekdays_json,r.branch_id
        ${productionmanufacturingProjection.routingOperations} JOIN production_resources r ON r.id=o.resource_id
        WHERE o.specification_id=? AND r.company_id=? ORDER BY o.ordinal`).bind(specificationId, companyId).all<CapacityOperation>()).results,
      calendar: (resourceId, date) => db.prepare("SELECT available_minutes,version FROM production_resource_calendar WHERE resource_id=? AND work_date=?").bind(resourceId, date).first<{ available_minutes: number; version: number }>(),
      allocatedMinutes: async (resourceId, date) => Number((await db.prepare("SELECT COALESCE(sum(minutes),0) minutes FROM planning_resource_allocations WHERE resource_id=? AND work_date=? AND released_at IS NULL").bind(resourceId, date).first<{ minutes: number }>())?.minutes ?? 0),
    }, error: (status, code, message) => new ApiError(status, code, message),
  })(actor.companyId, input);
}
