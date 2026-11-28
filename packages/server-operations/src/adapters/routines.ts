import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
import { companyAuthority, type StaffActor } from "@erpx/server-operations/public/staff-policy";
import { ApiError as NativeApiError } from '@erpx/server-runtime/public/http';
import { type TemplateRow } from '@erpx/server-operations/domain/routines';
import { createRoutinesApplication } from '@erpx/server-operations/application/routines';
import { BusinessRuleError } from '@erpx/server-shared/index';
export { templateSchema } from '@erpx/server-operations/domain/routines';
export type { TemplateRow } from '@erpx/server-operations/domain/routines';
export type { ScheduleRow } from '@erpx/server-operations/domain/routines';
function application(provided?: SqlDatabase) {
    let database = provided!;
    return createRoutinesApplication({ companyAuthority: (...args) => fromNative(() => companyAuthority(database, ...args)) });
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
export async function canEditTemplate(db: SqlDatabase, actor: StaffActor, row: TemplateRow) { return invoke(() => application(db).canEditTemplate(actor, row)); }
