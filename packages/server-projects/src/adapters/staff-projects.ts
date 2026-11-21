import 'server-only';
import type { SqlDatabase, SqlStatement } from '@erpx/server-runtime/public/runtime/types';
import { ApiError } from '@erpx/server-runtime/public/http';
import type { StaffActor } from '@erpx/server-operations/public/staff-policy';
import { BusinessRuleError } from '@erpx/server-shared/index';
import {requireProjectPurchasedRead} from '@erpx/server-projects/adapters/purchases';
import { createStaffProjectsApplication, type StaffProjectsRepository } from '@erpx/server-projects/application/staff-projects';
import type { ProjectRow } from '@erpx/server-projects/domain/staff-projects';
async function exposed<T>(action: () => Promise<T>) { try {
    return await action();
}
catch (error) {
    if (error instanceof BusinessRuleError)
        throw new ApiError(error.status, error.code, error.message);
    throw error;
} }
function application(db: SqlDatabase) {
    const repository: StaffProjectsRepository<SqlStatement> = { async visibleProject(id: string, companyId: string, userId: string, userId2: string) {
            return db
                .prepare("SELECT p.* FROM staff_projects p WHERE p.id=? AND p.company_id=? AND (p.owner_user_id=? OR EXISTS(SELECT 1 FROM staff_project_members m WHERE m.project_id=p.id AND m.user_id=?))")
                .bind(id, companyId, userId, userId2)
                .first<ProjectRow>();
        },
        async visibleProjects(companyId: string, userId: string, userId2: string) {
            return db
                .prepare("SELECT p.* FROM staff_projects p WHERE p.company_id=? AND (p.owner_user_id=? OR EXISTS(SELECT 1 FROM staff_project_members m WHERE m.project_id=p.id AND m.user_id=?)) ORDER BY p.updated_at DESC")
                .bind(companyId, userId, userId2)
                .all<ProjectRow>();
        },
        };
    return createStaffProjectsApplication({ repository });
}
export async function getProject(db: SqlDatabase, actor: StaffActor, id: string) { await requireProjectPurchasedRead(db,actor,id);return exposed(() => application(db).getProject(actor, id)); }
export async function listProjects(db: SqlDatabase, actor: StaffActor) { await requireProjectPurchasedRead(db,actor);return exposed(() => application(db).listProjects(actor)); }
