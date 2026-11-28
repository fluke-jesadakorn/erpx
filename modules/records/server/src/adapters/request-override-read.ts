import type{SqlDatabase}from '@erpx/platform-runtime-server/public/runtime/types';
import type{WorkContext}from '@erpx/identity-access-server/public/authorization';
import type{EmployeeRequest}from '@erpx/records-server/domain/employee-requests';
import{createRequestOverrideRead}from '@erpx/records-server/application/employee-requests';
import{nativeEmployeeRequestReadRepository}from './employee-requests-read';
import{canOverrideStaffStep}from '@erpx/workflow-server/public/approval-planning';
import{BusinessRuleError}from '@erpx/platform-shared-server/index';
import{ApiError as NativeApiError}from '@erpx/platform-runtime-server/public/http';
/** Switch the override read to the owner's canonical native read repository; no local TS SQL. */
function application(database:SqlDatabase){const repository=nativeEmployeeRequestReadRepository(database);return createRequestOverrideRead({repository:{readReadRequestForOverrideRow:(...args)=>repository.readReadRequestForOverrideRow(...args)},canOverrideStaffStep:(...args)=>fromNative(()=>canOverrideStaffStep(database,...args))});}
async function fromNative<Result>(run: () => Promise<Result> | Result): Promise<Result> {
    try {
        return await run();
    }
    catch (error) {
        if (error instanceof NativeApiError)
            throw new BusinessRuleError(error.status, error.code, error.message);
        throw error;
    }
}
async function invoke<Result>(run: () => Promise<Result>) {
    try {
        return await run();
    }
    catch (error) {
        if (error instanceof BusinessRuleError)
            throw new NativeApiError(error.status, error.code, error.message);
        throw error;
    }
}
export async function canOverrideEmployeeRequest(db: SqlDatabase, actor: Pick<WorkContext, "companyId" | "userId" | "role" | "assignmentId">, row: EmployeeRequest) { return invoke(() => application(db).canOverrideEmployeeRequest(actor, row)); }
export async function readRequestForOverride(db: SqlDatabase, companyId: string, id: string) { return invoke(() => application(db).readRequestForOverride(companyId, id)); }
