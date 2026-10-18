import {expenseWorkCreationAvailable} from '../../people_payroll/public/expense-purchases';
import type { SqlDatabase } from "@/server/modules/runtime/public/runtime/types";
import "server-only";
import type { WorkContext } from "@/server/modules/identity_access/public/authorization";
import { activeStaff } from "@/server/modules/operations/public/staff-policy";
import { ApiError } from "@/server/modules/runtime/public/http";
import { canPrepareCounts } from "@/server/modules/inventory/public/inventory-counts";
import { canPrepareTransfers } from "@/server/modules/inventory/public/inventory-transfers";
import { publicScope } from "@/server/modules/identity_access/public/authorization";
import { createWorkCapabilities, type WorkCapabilitiesUnit } from "../application/work-capabilities";
import { workTeamCoverageFacts } from "../../support/public/work-team-coverage-facts";
const service = createWorkCapabilities({ errorStatus: error => error instanceof ApiError ? error.status : undefined, canPrepareCounts, canPrepareTransfers, publicScope });
export function workCapabilitiesUnit(db: SqlDatabase): WorkCapabilitiesUnit { return { coveredTeam: actor => workTeamCoverageFacts(db).coveredTeam(actor), activeStaff: actor => activeStaff(db, actor.companyId, actor.userId) }; }
export function hasCoveredTeam(db: SqlDatabase, actor: WorkContext) { return service.hasCoveredTeam(workCapabilitiesUnit(db), actor); }
export async function getWorkCapabilities(db: SqlDatabase, actor: WorkContext) {
 const result=await service.getWorkCapabilities(workCapabilitiesUnit(db),actor);
 return await expenseWorkCreationAvailable(db,actor)?result:{...result,creation:result.creation.filter(entry=>entry.kind!=='expense')};
}
