import 'server-only';
import { holdingResources } from '../../../../../platform/runtime/server/src/public/runtime';
import type { SqlDatabase } from '@erpx/platform-runtime-server/public/runtime/types';
import type { WorkContext } from '@erpx/identity-access-server/public/authorization';
import { requirePermission } from '@erpx/identity-access-server/public/iam-permissions';
import { activeStaff } from '@erpx/operations-server/public/staff-policy';
import { getProject } from '@erpx/projects-server/public/staff-projects';
import { nativeProjectMilestones } from '@erpx/projects-server/public/project-milestones';
import { nativeProjectGraph } from './native-project-graph';
import { ApiError } from '@erpx/platform-runtime-server/public/http';
import { BusinessRuleError } from '@erpx/platform-shared-server/index';
import { createProjectStructureApplication, type ProjectStructureRepository } from '@erpx/projects-server/application/structure';
import { projectStructureSources as owner0 } from '@erpx/identity-access-server/public/project-structure-sources';
import { projectStructureSources as owner1 } from '@erpx/operations-server/public/project-structure-sources';
import { projectStructureSources as owner2 } from '@erpx/projects-server/public/project-structure-sources';
import {projectPurchaseControls,requireProjectPurchasedRead} from '@erpx/projects-server/adapters/purchases';
import type { ProjectActualsActor } from './native-lane-27/client';
async function inward<T>(action: () => Promise<T>) { try {
    return await action();
}
catch (error) {
    if (error instanceof ApiError)
        throw new BusinessRuleError(error.status, error.code, error.message);
    throw error;
} }
async function exposed<T>(action: () => Promise<T>) { try {
    return await action();
}
catch (error) {
    if (error instanceof BusinessRuleError)
        throw new ApiError(error.status, error.code, error.message);
    throw error;
} }
const caller = (actor: unknown) => actor as ProjectActualsActor;
function application(db: SqlDatabase, owner: ProjectActualsActor, purchasedCostHistory = true) { const repository: ProjectStructureRepository = { ...owner0(db), ...owner1(db), ...owner2(db, owner) }; return createProjectStructureApplication({ purchasedCostHistory, repository, clock: { now: () => Date.now() }, requirePermission: (...args) => requirePermission(...args), activeStaff: (...args) => inward(() => activeStaff(db, ...args)), getProject: (...args) => inward(() => getProject(db, ...args)), listMilestones: (...args) => inward(() => nativeProjectMilestones(db, ...args)) }); }
/** Native Projects graph read. The mounted owner route performs the
 * Company/membership, purchase, context and serializable-boundary checks and
 * returns the complete graph; there is no TypeScript graph query or fallback.
 * The retained `providedWrite` parameter keeps the shared port signature. */
export async function projectGraph(a: WorkContext, projectId: string, providedWrite?: SqlDatabase) { const db = providedWrite ?? (await holdingResources(a.holdingId)).write;await requireProjectPurchasedRead(db,a,projectId);return exposed(() => nativeProjectGraph(a, projectId)); }
export async function projectHandoffTarget(db: SqlDatabase, a: WorkContext, id: string) { await requireProjectPurchasedRead(db,a,undefined,'projects.handover');return exposed(() => application(db,caller(a)).projectHandoffTarget(a, id)); }
export async function projectHandoffWork(a: WorkContext) {
 const {write:db}=await holdingResources(a.holdingId),access=await projectPurchaseControls(db,a);
 if(access&&!access['projects.handover'].history)return [];
 const rows=await exposed(()=>application(db,caller(a)).projectHandoffWork(a));
 return !access||access['projects.handover'].settleExisting?rows:rows.map(row=>({...row,canAct:false,queue:row.queue==='todo'?'waiting' as const:row.queue}));
}
