import 'server-only';
import { holdingResources } from '../../runtime/public/runtime';
import { isTransactionalSqlDatabase, type SqlDatabase, type SqlStatement } from '../../runtime/public/runtime/types';
import type { WorkContext } from '../../identity_access/public/authorization';
import { requirePermission } from '../../identity_access/public/iam-permissions';
import { replayCommand, executeCommand } from '../../workflow/public/commands';
import { activeStaff, staffGuard } from '../../operations/public/staff-policy';
import { getProject } from '../public/staff-projects';
import { listProjectMilestones, projectConstraint } from '../public/project-milestones';
import { ApiError } from '../../runtime/public/http';
import { BusinessRuleError } from '../../shared';
import { projectStructureCommandSchema } from '../domain/structure';
import { createProjectStructureApplication, type ProjectStructureRepository } from '../application/structure';
import { projectStructureSources as owner0 } from '../../identity_access/public/project-structure-sources';
import { projectStructureSources as owner1 } from '../../operations/public/project-structure-sources';
import { projectStructureSources as owner2 } from '../../projects/public/project-structure-sources';
import { projectStructureSources as owner3 } from '../../collaboration/public/project-structure-sources';
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
function application(db: SqlDatabase) { const repository: ProjectStructureRepository<SqlStatement> = { ...owner0(db), ...owner1(db), ...owner2(db), ...owner3(db) }; return createProjectStructureApplication({ repository, clock: { now: () => Date.now() }, commands: { replay: (...args) => inward(() => replayCommand(db, ...args)), commit: (...args) => inward(() => executeCommand(db, ...args)) }, requirePermission: (...args) => requirePermission(...args), activeStaff: (...args) => inward(() => activeStaff(db, ...args)), staffGuard: (...args) => staffGuard(db, ...args), getProject: (...args) => inward(() => getProject(db, ...args)), listMilestones: (...args) => inward(() => listProjectMilestones(db, ...args)), sha256: async (value) => Array.from(new Uint8Array(await crypto.subtle.digest('SHA-256', new TextEncoder().encode(value))), b => b.toString(16).padStart(2, '0')).join('') }); }
export async function projectGraph(a: WorkContext, projectId: string, providedWrite?: SqlDatabase) { const db = providedWrite ?? (await holdingResources(a.holdingId)).write; return exposed(() => application(db).projectGraph(a, projectId)); }
export async function projectStructureCommand(a: WorkContext, projectId: string, raw: unknown, key: string, providedWrite?: SqlDatabase): Promise<Record<string, unknown>> { if (providedWrite)
    return exposed(() => application(providedWrite).projectStructureCommand(a, projectId, raw, key)); application(null as unknown as SqlDatabase).access(a, true); const input = projectStructureCommandSchema.parse(raw), db = (await holdingResources(a.holdingId)).write; if (!isTransactionalSqlDatabase(db))
    throw new ApiError(503, 'PROJECT_TRANSACTION_REQUIRED', 'The transaction runtime is unavailable. Retry when it is ready.'); return db.transaction(tx => projectStructureCommand(a, projectId, input, key, tx)).catch(projectConstraint); }
export async function projectHandoffs(a: WorkContext, projectId: string) { const { write: db } = await holdingResources(a.holdingId); return exposed(() => application(db).projectHandoffs(a, projectId)); }
export async function projectHandoffTarget(db: SqlDatabase, a: WorkContext, id: string) { return exposed(() => application(db).projectHandoffTarget(a, id)); }
export async function projectHandoffWork(a: WorkContext) { const { write: db } = await holdingResources(a.holdingId); return exposed(() => application(db).projectHandoffWork(a)); }
