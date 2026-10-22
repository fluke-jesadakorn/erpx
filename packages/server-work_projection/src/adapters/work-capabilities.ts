import {expenseWorkCreationAvailable} from '@erpx/server-people_payroll/public/expense-purchases';
import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
import "server-only";
import type { WorkContext } from "@erpx/server-identity_access/public/authorization";
import { activeStaff } from "@erpx/server-operations/public/staff-policy";
import { ApiError } from "@erpx/server-runtime/public/http";
import { canPrepareCounts } from "@erpx/server-inventory/public/inventory-counts";
import { canPrepareTransfers } from "@erpx/server-inventory/public/inventory-transfers";
import { publicScope } from "@erpx/server-identity_access/public/authorization";
import { createWorkCapabilities, type WorkCapabilitiesUnit } from "@erpx/server-work_projection/application/work-capabilities";
import { workTeamCoverageFacts } from "@erpx/server-support/public/work-team-coverage-facts";
const service = createWorkCapabilities({ errorStatus: error => error instanceof ApiError ? error.status : undefined, canPrepareCounts, canPrepareTransfers, publicScope });
export function workCapabilitiesUnit(db: SqlDatabase): WorkCapabilitiesUnit { return { coveredTeam: actor => workTeamCoverageFacts(db).coveredTeam(actor), activeStaff: actor => activeStaff(db, actor.companyId, actor.userId) }; }
export function hasCoveredTeam(db: SqlDatabase, actor: WorkContext) { return service.hasCoveredTeam(workCapabilitiesUnit(db), actor); }
export async function getWorkCapabilities(db: SqlDatabase, actor: WorkContext) {
 const result=await service.getWorkCapabilities(workCapabilitiesUnit(db),actor);
 return await expenseWorkCreationAvailable(db,actor)?result:{...result,creation:result.creation.filter(entry=>entry.kind!=='expense')};
}
