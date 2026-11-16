import type{SqlDatabase}from '@erpx/server-runtime/public/runtime/types';
import type{WorkContext}from '@erpx/server-identity_access/public/authorization';
import type{EmployeeRequest}from '@erpx/server-records/domain/employee-requests';
import{createRequestOverrideRead}from '@erpx/server-records/application/employee-requests';
import{canOverrideStaffStep}from '@erpx/server-workflow/public/approval-planning';
import{BusinessRuleError}from '@erpx/server-shared/index';
import{ApiError as NativeApiError}from '@erpx/server-runtime/public/http';
function application(database:SqlDatabase){return createRequestOverrideRead({repository:{readReadRequestForOverrideRow(id:string,companyId:string){return database.prepare("SELECT * FROM employee_requests WHERE id=? AND company_id=?").bind(id,companyId).first<EmployeeRequest>();}},canOverrideStaffStep:(...args)=>fromNative(()=>canOverrideStaffStep(database,...args))});}
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
