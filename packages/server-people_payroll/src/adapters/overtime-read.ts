import 'server-only';
import type{SqlDatabase,SqlStatement}from '@erpx/server-runtime/public/runtime/types';
import type{WorkContext}from '@erpx/server-identity_access/public/authorization';
import{holdingResources}from '@erpx/server-runtime/public/runtime';
import{workforcePurchaseControls,withWorkforcePurchasedRead}from '@erpx/server-people_payroll/adapters/workforce-purchases';
import{createOvertimeRead,type OvertimeReadPorts}from '@erpx/server-people_payroll/application/overtime';
import type{OvertimeRow}from '@erpx/server-people_payroll/domain/overtime';
import{participantSql}from '@erpx/server-workflow/public/approval-participant-projection';
import{approvalView}from '@erpx/server-workflow/public/approval-planning';
import{BusinessRuleError}from '@erpx/server-shared/index';
import{ApiError}from '@erpx/server-runtime/public/http';
export function nativeOvertimeReadRepository(database:SqlDatabase):OvertimeReadPorts<SqlStatement>['repository']{return{readGetOvertimeRow(id: string, companyId: string, userId: string, userId2: string, assignmentId: string) { return database.prepare(`SELECT o.* FROM overtime_requests o WHERE o.id=? AND o.company_id=? AND (o.created_by=? OR ${participantSql('overtime', 'o', '?', '?')})`).bind(id, companyId, userId, userId2, assignmentId).first<OvertimeRow>(); },readGetOvertimeHandoff(id: string, companyId: string) { return database.prepare('SELECT * FROM overtime_payroll_handoffs WHERE request_id=? AND company_id=?').bind(id, companyId).first(); }};}
function overtimeReadContext(provided?:SqlDatabase):OvertimeReadPorts<SqlStatement>{let database=provided!;return{prepare:async holding=>{if(!provided)database=(await holdingResources(holding)).write;},repository:{readGetOvertimeRow:(...args)=>nativeOvertimeReadRepository(database).readGetOvertimeRow(...args),readGetOvertimeHandoff:(...args)=>nativeOvertimeReadRepository(database).readGetOvertimeHandoff(...args)},approvalView:(...args)=>fromNative(()=>approvalView(database,...args))};}
async function fromNative<Result>(run: () => Promise<Result>) {
    try {
        return await run();
    }
    catch (error) {
        if (error instanceof ApiError)
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
            throw new ApiError(error.status, error.code, error.message);
        throw error;
    }
}
export async function getOvertime(actor: WorkContext, id: string, database?: SqlDatabase) { return withWorkforcePurchasedRead(actor,async tx=>{const data=await invoke(() => createOvertimeRead(overtimeReadContext(tx)).getOvertime(actor,id)),purchase=await workforcePurchaseControls(actor,tx);return {...data,...(purchase?{purchase}:{})};},database); }
