import 'server-only';
import type { SqlDatabase, SqlStatement } from '@erpx/server-runtime/public/runtime/types';
import type { WorkContext } from '@erpx/server-identity_access/public/authorization';
import { holdingResources } from '../../../../packages/server-runtime/src/public/runtime';
import { ApiError } from '@erpx/server-runtime/public/http';
import { requirePermission } from '@erpx/server-identity_access/public/iam-permissions';
import type { StaffActor } from '@erpx/server-operations/public/staff-policy';
import { getProject } from '@erpx/server-projects/public/staff-projects';
import { BusinessRuleError } from '@erpx/server-shared/index';
import { requireProjectPurchasedRead } from '@erpx/server-projects/adapters/purchases';
import { createProjectActualsApplication, type ProjectActualsRepository } from '@erpx/server-projects/application/actuals';
import { projectactualsSources as owner0 } from '@erpx/server-finance/public/project-actuals-sources';
import { nativeProjectCostAllocations, nativeProjectWorkLogs, type ProjectActualsActor } from './native-lane-27/client';
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
    if (error instanceof BusinessRuleError) {
        const out = new ApiError(error.status, error.code, error.message);
        out.cause = error.cause;
        throw out;
    }
    throw error;
} }
function application(db: SqlDatabase, caller: ProjectActualsActor) {
    const repository: ProjectActualsRepository<SqlStatement> = {
        async workLogs(userId: string, _userId2: string, _userId3: string, companyId: string, id: string, _userId4: string, _userId5: string) {
            return { results: await nativeProjectWorkLogs({ ...caller, companyId, userId }, id) };
        },
        async costAllocations(userId: string, _userId2: string, companyId: string, id: string) {
            return { results: await nativeProjectCostAllocations({ ...caller, companyId, userId }, id) };
        },
        ...owner0(db) };
    return createProjectActualsApplication({ repository, getProject: (...args) => inward(() => getProject(db, ...args)), requirePermission: (...args) => requirePermission(...args) });
}
const caller = (actor: unknown) => actor as ProjectActualsActor;
export async function listProjectWorkLogs(db: SqlDatabase, actor: StaffActor, projectId: string) {await requireProjectPurchasedRead(db,actor,projectId,'projects.time');return exposed(() => application(db, caller(actor)).listProjectWorkLogs(actor, projectId)); }
export async function listProjectCostAllocations(actor: WorkContext, projectId: string) { const { write: db } = await holdingResources(actor.holdingId);await requireProjectPurchasedRead(db,actor,projectId,'projects.costs');return exposed(() => application(db, caller(actor)).listProjectCostAllocations(actor, projectId)); }
export async function listProjectCostSources(actor: WorkContext, projectId: string) { const { write: db } = await holdingResources(actor.holdingId);await requireProjectPurchasedRead(db,actor,projectId,'projects.costs');return exposed(() => application(db, caller(actor)).listProjectCostSources(actor, projectId)); }
