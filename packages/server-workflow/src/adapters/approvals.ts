import type{SqlDatabase}from '@erpx/server-runtime/public/runtime/types';
import type{StaffActor}from '@erpx/server-operations/public/staff-policy';
import {withExpenseApprovalReroute}from '@erpx/server-people_payroll/public/expense-purchases';
import {replayCommand as nativeReplayCommand,executeCommand as nativeExecuteCommand}from '@erpx/server-workflow/public/commands';
import{nativeApprovalPlanningPorts}from '@erpx/server-workflow/adapters/approval-planning';
import{createApprovalsApplication}from '@erpx/server-workflow/application/approvals';
import{BusinessRuleError}from '@erpx/server-shared/index';
import{ApiError as NativeApiError}from '@erpx/server-runtime/public/http';
function application(database:SqlDatabase){return createApprovalsApplication({...nativeApprovalPlanningPorts(database),commands:{replay:(...args)=>nativeReplayCommand(database,...args),commit:(...args)=>nativeExecuteCommand(database,...args)}});}
async function invoke<Result>(run: () => Promise<Result>) { try {
    return await run();
}
catch (error) {
    if (error instanceof BusinessRuleError)
        throw new NativeApiError(error.status, error.code, error.message);
    throw error;
} }
export async function rerouteApproval(db: SqlDatabase, actor: StaffActor, id: string, input: {
    expectedVersion: number;
    instanceVersion: number;
    approverUserId: string;
    reason: string;
}, key: string): Promise<Record<string, unknown>> { return withExpenseApprovalReroute(db,actor,id,key,transaction=>invoke(() => application(transaction).rerouteApproval(actor,id,input,key))); }
