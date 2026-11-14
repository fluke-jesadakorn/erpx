import { projectReadProjection as projectfinanceProjection } from '@erpx/server-finance/public/project-read-projections';
import 'server-only';
import type { SqlDatabase, SqlStatement } from '@erpx/server-runtime/public/runtime/types';
import type { WorkContext } from '@erpx/server-identity_access/public/authorization';
import { holdingResources } from '../../../../packages/server-runtime/src/public/runtime';
import { ApiError } from '@erpx/server-runtime/public/http';
import { requirePermission } from '@erpx/server-identity_access/public/iam-permissions';
import type { StaffActor } from '@erpx/server-operations/public/staff-policy';
import { getProject } from '@erpx/server-projects/public/staff-projects';
import { BusinessRuleError } from '@erpx/server-shared/index';
import type { ProjectWorkLogRow } from '@erpx/server-projects/domain/actuals';
import { requireProjectPurchasedRead } from '@erpx/server-projects/adapters/purchases';
import { createProjectActualsApplication, type ProjectActualsRepository } from '@erpx/server-projects/application/actuals';
import { projectactualsSources as owner0 } from '@erpx/server-finance/public/project-actuals-sources';
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
function application(db: SqlDatabase) {
    const repository: ProjectActualsRepository<SqlStatement> = {
        async workLogs(userId: string, userId2: string, userId3: string, companyId: string, id: string, userId4: string, userId5: string) {
            return db.prepare(`SELECT l.id,l.project_id,l.user_id,l.work_date,l.minutes,l.state,l.version,l.reviewed_by,
      l.rejection_reason,l.created_at,l.updated_at,
      iam_same_person(l.user_id,?,p.company_id) is_actor,
      (iam_same_person(p.owner_user_id,?,p.company_id) AND NOT iam_same_person(l.user_id,?,p.company_id)) can_review
    FROM staff_project_work_logs l JOIN staff_projects p ON p.id=l.project_id AND p.company_id=l.company_id
    WHERE l.company_id=? AND l.project_id=? AND (iam_same_person(p.owner_user_id,?,p.company_id) OR iam_same_person(l.user_id,?,p.company_id))
    ORDER BY l.work_date DESC,l.id`)
                .bind(userId, userId2, userId3, companyId, id, userId4, userId5).all<ProjectWorkLogRow>();
        },
        async costAllocations(userId: string, userId2: string, companyId: string, id: string) {
            return db.prepare(`SELECT a.id,a.project_id,a.journal_line_id,a.amount_minor::text amount_minor,a.state,a.version,
      a.created_by,a.approved_by,a.rejection_reason,a.created_at,a.updated_at,j.id journal_id,j.journal_number,
      NOT iam_same_person(a.created_by,?,a.company_id) can_review,
      iam_same_person(a.created_by,?,a.company_id) is_creator,
      j.entry_date,j.currency,j.status journal_status,ac.code account_code,ac.name account_name
    FROM staff_project_cost_allocations a ${projectfinanceProjection.allocationLine}
    ${projectfinanceProjection.expenseJournal} ${projectfinanceProjection.expenseAccount}
    WHERE a.company_id=? AND a.project_id=? ORDER BY j.entry_date DESC,j.journal_number DESC,a.id`)
                .bind(userId, userId2, companyId, id).all<Record<string, string | number | boolean | null>>();
        },
        ...owner0(db) };
    return createProjectActualsApplication({ repository, getProject: (...args) => inward(() => getProject(db, ...args)), requirePermission: (...args) => requirePermission(...args) });
}
export async function listProjectWorkLogs(db: SqlDatabase, actor: StaffActor, projectId: string) {await requireProjectPurchasedRead(db,actor,projectId,'projects.time');return exposed(() => application(db).listProjectWorkLogs(actor, projectId)); }
export async function listProjectCostAllocations(actor: WorkContext, projectId: string) { const { write: db } = await holdingResources(actor.holdingId);await requireProjectPurchasedRead(db,actor,projectId,'projects.costs');return exposed(() => application(db).listProjectCostAllocations(actor, projectId)); }
export async function listProjectCostSources(actor: WorkContext, projectId: string) { const { write: db } = await holdingResources(actor.holdingId);await requireProjectPurchasedRead(db,actor,projectId,'projects.costs');return exposed(() => application(db).listProjectCostSources(actor, projectId)); }
