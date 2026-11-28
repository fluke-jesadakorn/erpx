import {ppRd37IdVersionInGuard} from './pp-read-tail-r37-sources';
import { nativePpStatement } from '@erpx/people-payroll-server/adapters/native-people-payroll-command';
import { PP_RD19_HR_ENGAGEMENT_VERSION_EXISTS_SQL } from './pp-read-tail-r19-sources';
import { ppRd25ByIdCompany } from './pp-read-tail-r25-sources';
import{nativeLeaveReadRepository,getLeave}from '@erpx/people-payroll-server/adapters/leave-read';
export{getLeave};
import { withWorkforcePurchasedCommand } from "@erpx/people-payroll-server/adapters/workforce-purchases";
import { leaveReviewEvidenceFacts } from '@erpx/files-server/public/leave-review-evidence-facts';
import type { SqlDatabase } from "@erpx/platform-runtime-server/public/runtime/types";
import "server-only";
import { z } from "zod";
import type { WorkContext } from "@erpx/identity-access-server/public/authorization";
import { holdingResources } from "../../../../../platform/runtime/server/src/public/runtime";
import { sha256 } from "@erpx/platform-runtime-server/public/crypto";
import { ApiError } from "@erpx/platform-runtime-server/public/http";
import {peopleRequestRoute} from '@erpx/organization-server/public/department-authority-reads';
import {assertion} from "@erpx/workflow-server/public/command-assertions";
import {executeCommand,replayCommand} from "@erpx/workflow-server/public/commands";
import { activeStaff, staffGuard } from "@erpx/operations-server/public/staff-policy";
import {approvalView,openApproval,prepareDecision,closeApprovals} from '@erpx/workflow-server/public/approval-planning';

import { prepareAttachmentBinding } from "@erpx/files-server/public/attachments/service";
import { availableLeaveEvidence, leaveEvidenceManifest, pendingLeaveEvidenceCount, type LeaveEvidencePurpose } from "@erpx/people-payroll-server/public/leave-evidence";
import type { SqlStatement } from '@erpx/platform-runtime-server/public/runtime/types';
import { BusinessRuleError } from '@erpx/platform-shared-server/index';
import { leaveDraftSchema, leaveDecisionSchema, newLeaveSchema, leaveLifecycleSchema, type LeaveRequest } from '@erpx/people-payroll-server/domain/leave';
import { createLeaveApplication, type LeaveRepository } from '@erpx/people-payroll-server/application/leave';
import { leaveLegacyEmployeeFacts } from '@erpx/operations-server/public/leave-review-facts';
import { leaveReviewEvidence } from '@erpx/workflow-server/public/leave-review-facts';
export { leaveDraftSchema, leaveDecisionSchema, newLeaveSchema, leaveLifecycleSchema, type LeaveRequest } from '@erpx/people-payroll-server/domain/leave';
export { assertLeaveAttachmentAccess } from '@erpx/people-payroll-server/public/leave-evidence';
function application(provided?: SqlDatabase) {
    let database = provided!;
    const repository: LeaveRepository<SqlStatement> = { readGetLeaveLegacy(...args){return nativeLeaveReadRepository(database).readGetLeaveLegacy(...args);},
        readGetLeaveRow(...args){return nativeLeaveReadRepository(database).readGetLeaveRow(...args);},
        readCreateLeaveEmployee(...args) { return leaveLegacyEmployeeFacts(database).readCreateLeaveEmployee(...args); },
        stageCreateLeaveEffect(...args) { return leaveLegacyEmployeeFacts(database).stageCreateLeaveEffect(...args); },
        stageCreateLeaveEffect2(...args) { return leaveLegacyEmployeeFacts(database).stageCreateLeaveEffect2(...args); },
        guard6(key: string, value: "leave-state", id: string, version: number, states: string[]) {
            return assertion(database, key, value, ppRd37IdVersionInGuard("leave_requests", "state", states.map(() => "?").join(",")), [id, version, ...states]);
        },
        guard7(key: string, value: "placement", engagement_id: string, engagement_version: number) {
            return assertion(database, key, value, PP_RD19_HR_ENGAGEMENT_VERSION_EXISTS_SQL, [engagement_id, engagement_version]);
        },
        stageSubmitLeaveEffect(startDate: string, days: number, handoverNote: string, state: "draft" | "pending_approval", person_id: string, engagement_id: string, value: string, department_id: string | null, userId: string, key: string, now: number, id: string) {
            return nativePpStatement('pp_leave_stmt_1')
                .bind(startDate, days, handoverNote, state, person_id, engagement_id, value, department_id, userId, key, now, id);
        },
        readDecisionEvidencePolicyRule(policy_version_id: string, leave_type_id: string) {
            return database.prepare(`SELECT evidence_min_units,evidence_purpose,evidence_min_count,
             evidence_due_stage,evidence_grace_days,evidence_note,evidence_privacy
           FROM leave_policy_rules
           WHERE policy_version_id=? AND leave_type_id=?`)
                .bind(policy_version_id, leave_type_id)
                .first<{
                evidence_min_units: number | null;
                evidence_purpose: LeaveEvidencePurpose | null;
                evidence_min_count: number;
                evidence_due_stage: "before_submit" | "before_final_approval";
                evidence_grace_days: number;
                evidence_note: string;
                evidence_privacy: "requester_approvers_hr";
            }>();
        },
        readDecideRow(id: string, companyId: string) { return database.prepare(ppRd25ByIdCompany('leave_requests')).bind(id, companyId).first<LeaveRequest>(); },
        guard11(...args) { return leaveReviewEvidenceFacts(database).guard11(...args); },
        guard12(...args) { return leaveReviewEvidenceFacts(database).guard12(...args); },
        guard13(...args) { return leaveReviewEvidenceFacts(database).guard13(...args); },
        guard14(...args) { return leaveReviewEvidenceFacts(database).guard14(...args); },
        guard15(...args) { return leaveReviewEvidenceFacts(database).guard15(...args); },
        guard16(...args) { return leaveReviewEvidenceFacts(database).guard16(...args); },
        stageDecideEvidenceManifestStep(...args) { return leaveReviewEvidence(database).stageDecideEvidenceManifestStep(...args); },
        stageDecideEffect(id: string, company_id: string) {
            return database.prepare("SELECT id FROM leave_requests WHERE id=? AND company_id=? FOR UPDATE")
                .bind(id, company_id);
        },
        stageDecideEffect2(state: "pending_approval" | "approved" | "rejected", note: string, userId: string, value: 0 | 1, key: string, now: number, id: string) {
            return nativePpStatement('pp_leave_stmt_2')
                .bind(state, note, userId, value, key, now, id);
        },
        stageDecideEffect3(...args) { return leaveReviewEvidence(database).stageDecideEffect3(...args); },
        stageTransitionLeaveEffect(value: string, id: string, company_id: string, note: string, userId: string, now: number, now2: number) { return nativePpStatement('pp_leave_stmt_3').bind(value, id, company_id, note, userId, now, now2); },
        stageTransitionLeaveEffect2(key: string, now: number, id: string) { return nativePpStatement('pp_leave_stmt_4').bind(key, now, id); },
        stageTransitionLeaveEffect3(note: string, key: string, now: number, id: string) {
            return nativePpStatement('pp_leave_stmt_5')
                .bind(note, key, now, id);
        },
        readDecideLeaveCancellationRow(id: string, companyId: string) { return database.prepare("SELECT * FROM leave_requests WHERE id=? AND company_id=? AND state='approved' AND cancellation_state='pending_review'").bind(id, companyId).first<LeaveRequest>(); },
        stageDecideLeaveCancellationEffect(value: "approved" | "cancelled", value2: "approved" | "rejected" | "pending_review", key: string, now: number, id: string) { return nativePpStatement('pp_leave_stmt_6').bind(value, value2, key, now, id); },
        stageDecideLeaveCancellationEffect2(value: "pending_approval" | "approved" | "rejected", userId: string, note: string, now: number, id: string) { return nativePpStatement('pp_leave_stmt_7').bind(value, userId, note, now, id); },
        stageBalanceTransitionInsert(value: string, kind: string, multiplier: number, key: string, value2: "Approved cancellation" | "Approved leave" | "Released reservation", actor: string, now: number, id: string, company_id: string, action: "use" | "release" | "reverse") {
            const source = action === 'reverse' ? "'usage','usage_reversal'" : "'reservation','reservation_release'";
            return nativePpStatement('pp_leave_stmt_8')
                .bind(value, kind, multiplier, key, value2, actor, now, id, company_id);
        } };
    return createLeaveApplication({ repository, clock: { now: Date.now }, identifiers: { next: () => crypto.randomUUID() }, hash: sha256, prepare: async (holding) => { if (!provided)
            database = (await holdingResources(holding)).write; }, commands: { replay: (...args) => replayCommand(database, ...args), commit: (...args) => executeCommand(database, ...args) }, peopleRequestRoute: (...args) => fromNative(() => peopleRequestRoute(database, ...args)), activeStaff: (...args) => fromNative(() => activeStaff(database, ...args)), staffGuard: (...args) => staffGuard(database, ...args), approvalView: (...args) => fromNative(() => approvalView(database, ...args)), openApproval: (...args) => fromNative(() => openApproval(database, ...args)), prepareDecision: (...args) => fromNative(() => prepareDecision(database, ...args)), closeApprovals: (...args) => closeApprovals(database, ...args), prepareAttachmentBinding: (...args) => fromNative(() => prepareAttachmentBinding(database, ...args)), availableLeaveEvidence: (...args) => fromNative(() => availableLeaveEvidence(database, ...args)), leaveEvidenceManifest: (...args) => fromNative(() => leaveEvidenceManifest(database, ...args)), pendingLeaveEvidenceCount: (...args) => fromNative(() => pendingLeaveEvidenceCount(database, ...args)) });
}
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

export async function createLeave(context: WorkContext, input: z.infer<typeof newLeaveSchema>, key: string) { return withWorkforcePurchasedCommand(context,{key,operation:"leave.create",purpose:"new-work",resultKind:"leave"},tx=>invoke(() => application(tx).createLeave(context,input,key))); }
export async function submitLeave(holdingId: string, id: string, input: z.infer<typeof leaveDraftSchema>, key: string, context: WorkContext) { return withWorkforcePurchasedCommand(context,{key,operation:input.saveOnly?"leave.save":"leave.submit",purpose:"new-work",target:{kind:"leave",id}},tx=>invoke(() => application(tx).submitLeave(holdingId,id,input,key,context))); }
export async function approveLeave(_holdingId: string, id: string, input: z.infer<typeof leaveDecisionSchema>, key: string, context: WorkContext) { return withWorkforcePurchasedCommand(context,{key,operation:"leave.approve",purpose:"settle-existing",target:{kind:"leave",id}},tx=>invoke(() => application(tx).approveLeave(_holdingId,id,input,key,context))); }
export async function transitionLeave(context: WorkContext, id: string, input: z.infer<typeof leaveLifecycleSchema>, key: string) { return withWorkforcePurchasedCommand(context,{key,operation:input.action==="reject"?"leave.reject":"leave.cancellation",purpose:"settle-existing",target:{kind:"leave",id}},tx=>invoke(() => application(tx).transitionLeave(context,id,input,key))); }
export async function decideLeaveCancellation(context: WorkContext, id: string, input: z.infer<typeof leaveDecisionSchema>, key: string, approve: boolean) { return withWorkforcePurchasedCommand(context,{key,operation:approve?"leave.cancellation.approve":"leave.cancellation.reject",purpose:"settle-existing",target:{kind:"leave",id}},tx=>invoke(() => application(tx).decideLeaveCancellation(context,id,input,key,approve))); }
