import {productionReadProjection as productionmanufacturingProjection} from '@erpx/manufacturing-server/public/production-read-projections';
import type { WorkContext } from "@erpx/identity-access-server/public/authorization";
import type { SqlDatabase } from "@erpx/platform-runtime-server/public/runtime/types";
import { ApiError } from "@erpx/platform-runtime-server/public/http";
import { createCapacityPlanner, type CapacityInput } from "@erpx/planning-server/application/capacity";
import type { CapacityOperation } from "@erpx/planning-server/domain/capacity";
import { resourceCalendarByKey } from "./planning-calendar-source-sql";
import { planningResourceAllocationRows } from "./planning-resource-allocation-sql";
import { planningPackageFactRows } from "./planning-package-fact-sql";

/** Bound to the caller's connection, including an existing native transaction. */
export async function calculateProductionCapacity(db: SqlDatabase, actor: WorkContext, input: CapacityInput) {
  return createCapacityPlanner({
    repository: {
      operations: async (specificationId, companyId) => (await db.prepare(planningPackageFactRows('o.*,r.version resource_version,r.status,r.daily_minutes,r.work_weekdays_json,r.branch_id',
        `${productionmanufacturingProjection.routingOperations.replace(/^FROM /, '')} JOIN production_resources r ON r.id=o.resource_id`,
        'o.specification_id=? AND r.company_id=? ORDER BY o.ordinal')).bind(specificationId, companyId).all<CapacityOperation>()).results,
      calendar: (resourceId, date) => db.prepare(resourceCalendarByKey('available_minutes,version')).bind(resourceId, date).first<{ available_minutes: number; version: number }>(),
      allocatedMinutes: async (resourceId, date) => Number((await db.prepare(planningResourceAllocationRows('COALESCE(sum(minutes),0) minutes', 'resource_id=? AND work_date=? AND released_at IS NULL')).bind(resourceId, date).first<{ minutes: number }>())?.minutes ?? 0),
    }, error: (status, code, message) => new ApiError(status, code, message),
  })(actor.companyId, input);
}
