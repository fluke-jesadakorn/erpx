import 'server-only';
import { holdingResources } from '../../../../packages/server-runtime/src/public/runtime';
import { isTransactionalSqlDatabase, type SqlDatabase, type SqlStatement } from '@erpx/server-runtime/public/runtime/types';
import type { WorkContext } from '@erpx/server-identity_access/public/authorization';
import { requirePermission } from '@erpx/server-identity_access/public/iam-permissions';
import { replayCommand, executeCommand } from '@erpx/server-workflow/public/commands';
import { activeStaff, staffGuard } from '@erpx/server-operations/public/staff-policy';
import { getProject } from '@erpx/server-projects/public/staff-projects';
import { listProjectMilestones, projectConstraint } from '@erpx/server-projects/public/project-milestones';
import { ApiError } from '@erpx/server-runtime/public/http';
import { BusinessRuleError } from '@erpx/server-shared/index';
import { projectStructureCommandSchema } from '@erpx/server-projects/domain/structure';
import { createProjectStructureApplication, type ProjectStructureRepository } from '@erpx/server-projects/application/structure';
import { projectStructureSources as owner0 } from '@erpx/server-identity_access/public/project-structure-sources';
import { projectStructureSources as owner1 } from '@erpx/server-operations/public/project-structure-sources';
import { projectStructureSources as owner2 } from '@erpx/server-projects/public/project-structure-sources';
import {type ProjectPurchaseTarget,projectPurchaseControls,requireProjectPurchasedRead,withProjectPurchasedCommand} from '@erpx/server-projects/adapters/purchases';
import {projectsPackageEnforcementMode} from '@erpx/server-commercial/public/purchase-command-boundary';
import { projectStructureSources as owner3 } from '@erpx/server-collaboration/public/project-structure-sources';
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
function application(db: SqlDatabase, purchasedCostHistory = true) { const repository: ProjectStructureRepository<SqlStatement> = { ...owner0(db), ...owner1(db), ...owner2(db), ...owner3(db) }; return createProjectStructureApplication({ purchasedCostHistory, repository, clock: { now: () => Date.now() }, commands: { replay: (...args) => inward(() => replayCommand(db, ...args)), commit: (...args) => inward(() => executeCommand(db, ...args)) }, requirePermission: (...args) => requirePermission(...args), activeStaff: (...args) => inward(() => activeStaff(db, ...args)), staffGuard: (...args) => staffGuard(db, ...args), getProject: (...args) => inward(() => getProject(db, ...args)), listMilestones: (...args) => inward(() => listProjectMilestones(db, ...args)), sha256: async (value) => Array.from(new Uint8Array(await crypto.subtle.digest('SHA-256', new TextEncoder().encode(value))), b => b.toString(16).padStart(2, '0')).join('') }); }
async function purchasedApplication(db:SqlDatabase,a:WorkContext){
 const access=await projectPurchaseControls(db,a);return application(db,!access||access['projects.costs'].history);
}
export async function projectGraph(a: WorkContext, projectId: string, providedWrite?: SqlDatabase) { const db = providedWrite ?? (await holdingResources(a.holdingId)).write;await requireProjectPurchasedRead(db,a,projectId);const owner=await purchasedApplication(db,a);return exposed(() => owner.projectGraph(a, projectId)); }
export async function projectStructureCommand(a: WorkContext, projectId: string, raw: unknown, key: string, providedWrite?: SqlDatabase): Promise<Record<string, unknown>> {
 application(null as unknown as SqlDatabase).access(a,true);const input=projectStructureCommandSchema.parse(raw),db=providedWrite??(await holdingResources(a.holdingId)).write;
 if(await projectsPackageEnforcementMode()==='disabled'){
  if(providedWrite)return exposed(()=>application(db).projectStructureCommand(a,projectId,input,key));
  if(!isTransactionalSqlDatabase(db))throw new ApiError(503,'PROJECT_TRANSACTION_REQUIRED','The transaction runtime is unavailable. Retry when it is ready.');
  return db.transaction(tx=>exposed(()=>application(tx).projectStructureCommand(a,projectId,input,key))).catch(projectConstraint);
 }
 const capability=input.action==='link-actual'?(input.sourceKind==='cost_allocation'?'projects.costs':'projects.time'):input.action.includes('handoff')||input.action==='reopen'?'projects.handover':'projects.wbs';
 const target:ProjectPurchaseTarget|undefined=input.action==='link-actual'?{kind:input.sourceKind==='cost_allocation'?'allocation':'work_log',id:input.sourceId}:input.action==='prepare-handoff'?{kind:'handoff',id:`project-handoff-${key}`}:'handoffId' in input?{kind:'handoff',id:input.handoffId}:undefined;
 return withProjectPurchasedCommand(db,a,{target,key,operations:[`project.structure.${input.action}`],projectId,capability,purpose:['assign-task','reopen'].includes(input.action)?'new-work':'settle-existing',existing:true},tx=>exposed(()=>application(tx).projectStructureCommand(a,projectId,input,key)));
}
export async function projectHandoffs(a: WorkContext, projectId: string) { const { write: db } = await holdingResources(a.holdingId);await requireProjectPurchasedRead(db,a,projectId,'projects.handover');const owner=await purchasedApplication(db,a);return exposed(() => owner.projectHandoffs(a, projectId)); }
export async function projectHandoffTarget(db: SqlDatabase, a: WorkContext, id: string) { await requireProjectPurchasedRead(db,a,undefined,'projects.handover');return exposed(() => application(db).projectHandoffTarget(a, id)); }
export async function projectHandoffWork(a: WorkContext) {
 const {write:db}=await holdingResources(a.holdingId),access=await projectPurchaseControls(db,a);
 if(access&&!access['projects.handover'].history)return [];
 const rows=await exposed(()=>application(db).projectHandoffWork(a));
 return !access||access['projects.handover'].settleExisting?rows:rows.map(row=>({...row,canAct:false,queue:row.queue==='todo'?'waiting' as const:row.queue}));
}
