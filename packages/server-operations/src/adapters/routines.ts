import {workRoutineReadScope} from '@erpx/server-operations/adapters/work-routine-purchases';
import type { SqlDatabase, SqlStatement, SqlValue } from "@erpx/server-runtime/public/runtime/types";
import { companyAuthority, type StaffActor } from "@erpx/server-operations/public/staff-policy";
import { ApiError as NativeApiError } from '@erpx/server-runtime/public/http';
import { type TemplateRow } from '@erpx/server-operations/domain/routines';
import { createRoutinesApplication, type RoutinesRepository } from '@erpx/server-operations/application/routines';
import { BusinessRuleError } from '@erpx/server-shared/index';
export { templateSchema } from '@erpx/server-operations/domain/routines';
export type { TemplateRow } from '@erpx/server-operations/domain/routines';
export type { ScheduleRow } from '@erpx/server-operations/domain/routines';
function application(provided?: SqlDatabase,readScope={where:'TRUE',args:[] as SqlValue[]}) {
    let database = provided!;
    const repository: RoutinesRepository<SqlStatement> = { readListTemplatesEffect(companyId: string, userId: string, value: 0 | 1) {
            return database.prepare(`SELECT t.* FROM routine_templates t WHERE t.company_id=? AND (t.published=1 OR t.creator_user_id=? OR ?=1) AND (${readScope.where}) ORDER BY t.updated_at DESC,t.id`)
                .bind(companyId, userId, value,...readScope.args)
                .all<TemplateRow>();
        },
        readGetTemplateRow(id: string, companyId: string) {
            return database.prepare(`SELECT t.* FROM routine_templates t WHERE t.id=? AND t.company_id=? AND (${readScope.where})`)
                .bind(id, companyId,...readScope.args)
                .first<TemplateRow>();
        } };
    return createRoutinesApplication({ repository, companyAuthority: (...args) => fromNative(() => companyAuthority(database, ...args)) });
}
async function fromNative<Result>(run: () => Promise<Result> | Result): Promise<Result> { try {
    return await run();
}
catch (error) {
    if (error instanceof NativeApiError)
        throw new BusinessRuleError(error.status, error.code, error.message);
    throw error;
} }
async function invoke<Result>(run: () => Promise<Result>) { try {
    return await run();
}
catch (error) {
    if (error instanceof BusinessRuleError)
        throw new NativeApiError(error.status, error.code, error.message);
    throw error;
} }
export async function listTemplates(db: SqlDatabase, actor: StaffActor) { return invoke(async() => application(db,await workRoutineReadScope(db,actor)).listTemplates(actor)); }
export async function getTemplate(db: SqlDatabase, actor: StaffActor, id: string) { const scope=await workRoutineReadScope(db,actor);return invoke(() => application(db,scope).getTemplate(actor, id)); }
export async function canEditTemplate(db: SqlDatabase, actor: StaffActor, row: TemplateRow) { return invoke(() => application(db).canEditTemplate(actor, row)); }
