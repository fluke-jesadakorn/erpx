import { workforcePurchaseControls, withWorkforcePurchasedRead } from "@erpx/server-people_payroll/adapters/workforce-purchases";

import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
import "server-only";

import type { WorkContext } from "@erpx/server-identity_access/public/authorization";
import { holdingResources } from "../../../../packages/server-runtime/src/public/runtime";

import { ApiError } from "@erpx/server-runtime/public/http";
import {peopleRequestRoute} from '@erpx/server-organization/public/department-authority-reads';

import { approvalView } from '@erpx/server-workflow/public/approval-planning';
import {participantSql} from '@erpx/server-workflow/public/approval-participant-projection';

import type { SqlStatement } from '@erpx/server-runtime/public/runtime/types';
import { BusinessRuleError } from '@erpx/server-shared/index';
import { type LeaveRequest } from '@erpx/server-people_payroll/domain/leave';

import{createLeaveRead,type LeaveReadPorts}from '@erpx/server-people_payroll/application/leave';
export function nativeLeaveReadRepository(database:SqlDatabase):LeaveReadPorts<SqlStatement>['repository']{return{readGetLeaveLegacy(id: string, companyId: string) {
            return database.prepare("SELECT id FROM leave_requests WHERE id=? AND company_id=? AND state='pending_approval'")
                .bind(id, companyId)
                .first();
        },readGetLeaveRow(id: string, companyId: string, userId: string, userId2: string, userId3: string, assignmentId: string, userId4: string, assignmentId2: string, evidenceReader: boolean) {
            return database.prepare(`SELECT l.*,t.name leave_type_name FROM leave_requests l LEFT JOIN leave_types t ON t.id=l.leave_type_id AND t.company_id=l.company_id WHERE l.id=? AND l.company_id=? AND (l.created_by=? OR l.decided_by=? OR ${participantSql("leave", "l", "?", "?")} OR ${participantSql("leave_cancellation", "l", "?", "?")} OR ?::boolean)`)
                .bind(id, companyId, userId, userId2, userId3, assignmentId, userId4, assignmentId2, evidenceReader)
                .first<Omit<LeaveRequest, "approval" | "permittedActions">>();
        }};}
function application(provided?:SqlDatabase){let database=provided!;return createLeaveRead({prepare:async holding=>{if(!provided)database=(await holdingResources(holding)).write;},repository:{readGetLeaveLegacy:(...args)=>nativeLeaveReadRepository(database).readGetLeaveLegacy(...args),readGetLeaveRow:(...args)=>nativeLeaveReadRepository(database).readGetLeaveRow(...args)},approvalView:(...args)=>fromNative(()=>approvalView(database,...args)),peopleRequestRoute:(...args)=>fromNative(()=>peopleRequestRoute(database,...args))});}
async function fromNative<Result>(run: () => Promise<Result>) { try {
    return await run();
}
catch (error) {
    if (error instanceof ApiError)
        throw new BusinessRuleError(error.status, error.code, error.message);
    throw error;
} }
async function invoke<Result>(run: () => Promise<Result>) { try {
    return await run();
}
catch (error) {
    if (error instanceof BusinessRuleError)
        throw new ApiError(error.status, error.code, error.message);
    throw error;
} }
export async function getLeave(holdingId: string, id: string, context: WorkContext, provided?: SqlDatabase) { return withWorkforcePurchasedRead(context,async tx=>{const data=await invoke(() => application(tx).getLeave(holdingId,id,context)),purchase=await workforcePurchaseControls(context,tx);return {...data,...(purchase?{purchase}:{})};},provided); }
