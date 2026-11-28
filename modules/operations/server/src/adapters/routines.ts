import type { SqlDatabase } from "@erpx/platform-runtime-server/public/runtime/types";
import { companyAuthority, type StaffActor } from "@erpx/operations-server/public/staff-policy";
import { ApiError as NativeApiError } from '@erpx/platform-runtime-server/public/http';
import { type TemplateRow } from '@erpx/operations-server/domain/routines';
import { createRoutinesApplication } from '@erpx/operations-server/application/routines';
import { BusinessRuleError } from '@erpx/platform-shared-server/index';
export { templateSchema } from '@erpx/operations-server/domain/routines';
export type { TemplateRow } from '@erpx/operations-server/domain/routines';
export type { ScheduleRow } from '@erpx/operations-server/domain/routines';
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
