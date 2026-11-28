import type{SqlDatabase}from '@erpx/platform-runtime-server/public/runtime/types';
import type{StaffActor}from '@erpx/operations-server/public/staff-policy';
import {withExpenseApprovalReroute}from '@erpx/people-payroll-server/public/expense-purchases';
import {replayCommand as nativeReplayCommand,executeCommand as nativeExecuteCommand}from '@erpx/workflow-server/public/commands';
import{nativeApprovalPlanningPorts}from '@erpx/workflow-server/adapters/approval-planning';
import{createApprovalsApplication}from '@erpx/workflow-server/application/approvals';
import{BusinessRuleError}from '@erpx/platform-shared-server/index';
import{ApiError as NativeApiError}from '@erpx/platform-runtime-server/public/http';
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
