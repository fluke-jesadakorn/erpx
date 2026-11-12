import 'server-only';
import { holdingResources } from '../../../../packages/server-runtime/src/public/runtime';
import type { SqlDatabase } from '@erpx/server-runtime/public/runtime/types';
import type { WorkContext } from '@erpx/server-identity_access/public/authorization';
import { requirePermission } from '@erpx/server-identity_access/public/iam-permissions';
import { activeStaff } from '@erpx/server-operations/public/staff-policy';
import { getProject } from '@erpx/server-projects/public/staff-projects';
import { listProjectMilestones } from '@erpx/server-projects/public/project-milestones';
import { ApiError } from '@erpx/server-runtime/public/http';
import { BusinessRuleError } from '@erpx/server-shared/index';
import { createProjectStructureApplication, type ProjectStructureRepository } from '@erpx/server-projects/application/structure';
import { projectStructureSources as owner0 } from '@erpx/server-identity_access/public/project-structure-sources';
import { projectStructureSources as owner1 } from '@erpx/server-operations/public/project-structure-sources';
import { projectStructureSources as owner2 } from '@erpx/server-projects/public/project-structure-sources';
import {projectPurchaseControls,requireProjectPurchasedRead} from '@erpx/server-projects/adapters/purchases';
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
function application(db: SqlDatabase, purchasedCostHistory = true) { const repository: ProjectStructureRepository = { ...owner0(db), ...owner1(db), ...owner2(db) }; return createProjectStructureApplication({ purchasedCostHistory, repository, clock: { now: () => Date.now() }, requirePermission: (...args) => requirePermission(...args), activeStaff: (...args) => inward(() => activeStaff(db, ...args)), getProject: (...args) => inward(() => getProject(db, ...args)), listMilestones: (...args) => inward(() => listProjectMilestones(db, ...args)) }); }
async function purchasedApplication(db:SqlDatabase,a:WorkContext){
 const access=await projectPurchaseControls(db,a);return application(db,!access||access['projects.costs'].history);
}
export async function projectGraph(a: WorkContext, projectId: string, providedWrite?: SqlDatabase) { const db = providedWrite ?? (await holdingResources(a.holdingId)).write;await requireProjectPurchasedRead(db,a,projectId);const owner=await purchasedApplication(db,a);return exposed(() => owner.projectGraph(a, projectId)); }
export async function projectHandoffs(a: WorkContext, projectId: string) { const { write: db } = await holdingResources(a.holdingId);await requireProjectPurchasedRead(db,a,projectId,'projects.handover');const owner=await purchasedApplication(db,a);return exposed(() => owner.projectHandoffs(a, projectId)); }
export async function projectHandoffTarget(db: SqlDatabase, a: WorkContext, id: string) { await requireProjectPurchasedRead(db,a,undefined,'projects.handover');return exposed(() => application(db).projectHandoffTarget(a, id)); }
export async function projectHandoffWork(a: WorkContext) {
 const {write:db}=await holdingResources(a.holdingId),access=await projectPurchaseControls(db,a);
 if(access&&!access['projects.handover'].history)return [];
 const rows=await exposed(()=>application(db).projectHandoffWork(a));
 return !access||access['projects.handover'].settleExisting?rows:rows.map(row=>({...row,canAct:false,queue:row.queue==='todo'?'waiting' as const:row.queue}));
}
