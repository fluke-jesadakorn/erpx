import 'server-only';
import type { SqlDatabase, SqlStatement } from '../../runtime/public/runtime/types';
import { ApiError } from '../../runtime/public/http';
import { assertion, replayCommand, executeCommand } from '../../workflow/public/commands';
import { activeStaff, staffGuard, type StaffActor } from '../../operations/public/staff-policy';
import { BusinessRuleError } from '../../shared';
import { createStaffProjectsApplication, type StaffProjectsRepository } from '../application/staff-projects';
import { projectSchema, type ProjectRow } from '../domain/staff-projects';
import { z } from 'zod';
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
        guardCurrentOwner(key: string, value: "project-version", recordId: string, expectedVersion: number, userId: string) {
            return assertion(db, key, value, "EXISTS(SELECT 1 FROM staff_projects WHERE id=? AND version=? AND owner_user_id=?)", [recordId, expectedVersion, userId]);
        },
        prepareProjectRevision(title: string, description: string, now: number, recordId: string) {
            return db
                .prepare("UPDATE staff_projects SET title=?,description=?,version=version+1,updated_at=? WHERE id=?")
                .bind(title, description, now, recordId);
        },
        prepareProject(recordId: string, companyId: string, branch_id: string | null, department_id: string | null, title: string, description: string, userId: string, now: number, now2: number) {
            return db
                .prepare("INSERT INTO staff_projects(id,company_id,branch_id,department_id,title,description,owner_user_id,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?)")
                .bind(recordId, companyId, branch_id, department_id, title, description, userId, now, now2);
        },
        prepareMemberReplacement(recordId: string) {
            return db
                .prepare("DELETE FROM staff_project_members WHERE project_id=?")
                .bind(recordId);
        },
        prepareMember(recordId: string, user: string, userId: string, now: number) {
            return db
                .prepare("INSERT INTO staff_project_members(project_id,user_id,added_by,created_at) VALUES(?,?,?,?)")
                .bind(recordId, user, userId, now);
        } };
    return createStaffProjectsApplication({ repository, clock: { now: () => Date.now() }, commands: { replay: (...args) => inward(() => replayCommand(db, ...args)), commit: (...args) => inward(() => executeCommand(db, ...args)) }, activeStaff: (...args) => inward(() => activeStaff(db, ...args)), staffGuard: (...args) => staffGuard(db, ...args) });
}
export function getProject(db: SqlDatabase, actor: StaffActor, id: string) { return exposed(() => application(db).getProject(actor, id)); }
export function listProjects(db: SqlDatabase, actor: StaffActor) { return exposed(() => application(db).listProjects(actor)); }
export function saveProject(db: SqlDatabase, actor: StaffActor, id: string | null, input: z.infer<typeof projectSchema>, key: string) { return exposed(() => application(db).saveProject(actor, id, input, key)); }
