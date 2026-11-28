import {expenseWorkCreationAvailable} from '@erpx/people-payroll-server/public/expense-purchases';
import type { SqlDatabase } from "@erpx/platform-runtime-server/public/runtime/types";
import "server-only";
import type { WorkContext } from "@erpx/identity-access-server/public/authorization";
import { activeStaff } from "@erpx/operations-server/public/staff-policy";
import { ApiError } from "@erpx/platform-runtime-server/public/http";
import { canPrepareCounts } from "@erpx/inventory-server/public/inventory-counts";
import { canPrepareTransfers } from "@erpx/inventory-server/public/inventory-transfer-read";
import { publicScope } from "@erpx/identity-access-server/public/authorization";
import { createWorkCapabilities, type WorkCapabilitiesUnit } from "@erpx/work-projection-server/application/work-capabilities";
import { workTeamCoverageFacts } from "@erpx/support-server/public/work-team-coverage-facts";
const service = createWorkCapabilities({ errorStatus: error => error instanceof ApiError ? error.status : undefined, canPrepareCounts, canPrepareTransfers, publicScope });
export function workCapabilitiesUnit(db: SqlDatabase): WorkCapabilitiesUnit { return { coveredTeam: actor => workTeamCoverageFacts(db).coveredTeam(actor), activeStaff: actor => activeStaff(db, actor.companyId, actor.userId) }; }
export function hasCoveredTeam(db: SqlDatabase, actor: WorkContext) { return service.hasCoveredTeam(workCapabilitiesUnit(db), actor); }
export async function getWorkCapabilities(db: SqlDatabase, actor: WorkContext) {
 const result=await service.getWorkCapabilities(workCapabilitiesUnit(db),actor);
 return await expenseWorkCreationAvailable(db,actor)?result:{...result,creation:result.creation.filter(entry=>entry.kind!=='expense')};
}
