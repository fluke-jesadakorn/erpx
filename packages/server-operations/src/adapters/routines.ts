import {workRoutineReadScope} from '@erpx/server-operations/adapters/work-routine-purchases';
import type { SqlDatabase, SqlStatement, SqlValue } from "@erpx/server-runtime/public/runtime/types";
import { companyAuthority, type StaffActor } from "@erpx/server-operations/public/staff-policy";
import { ApiError as NativeApiError } from '@erpx/server-runtime/public/http';
import { type TemplateRow } from '@erpx/server-operations/domain/routines';
import { createRoutinesApplication, type RoutinesRepository } from '@erpx/server-operations/application/routines';
import { BusinessRuleError } from '@erpx/server-shared/index';
import { operationsAliasSelectAll } from './operations-alias-select-all';
export { templateSchema } from '@erpx/server-operations/domain/routines';
export type { TemplateRow } from '@erpx/server-operations/domain/routines';
export type { ScheduleRow } from '@erpx/server-operations/domain/routines';
/** The routine-template reads use the canonical alias-qualified SELECT-all head
 *  from the read tail. Each caller supplies its own unchanged WHERE/ORDER tail,
 *  so the rendered SQL and binds stay byte-identical to the retired literals. */
function application(provided?: SqlDatabase,readScope={where:'TRUE',args:[] as SqlValue[]}) {
    let database = provided!;
    const repository: RoutinesRepository<SqlStatement> = { readGetTemplateRow(id: string, companyId: string) {
            return database.prepare(operationsAliasSelectAll('t','routine_templates', `t.id=? AND t.company_id=? AND (${readScope.where})`))
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
export async function getTemplate(db: SqlDatabase, actor: StaffActor, id: string) { const scope=await workRoutineReadScope(db,actor);return invoke(() => application(db,scope).getTemplate(actor, id)); }
export async function canEditTemplate(db: SqlDatabase, actor: StaffActor, row: TemplateRow) { return invoke(() => application(db).canEditTemplate(actor, row)); }
