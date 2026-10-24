import {workforcePurchaseControls,withWorkforcePurchasedRead,withWorkforcePurchasedCommand} from "@erpx/server-people_payroll/adapters/workforce-purchases";
import { leaveReviewEvidenceFacts } from '@erpx/server-files/public/leave-review-evidence-facts';
import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
import "server-only";
import { z } from "zod";
import type { WorkContext } from "@erpx/server-identity_access/public/authorization";
import { holdingResources } from "../../../../packages/server-runtime/src/public/runtime";
import { sha256 } from "@erpx/server-runtime/public/crypto";
import { ApiError } from "@erpx/server-runtime/public/http";
import {peopleRequestRoute} from '@erpx/server-organization/public/department-authority-reads';
import {assertion} from "@erpx/server-workflow/public/command-assertions";
import {executeCommand,replayCommand} from "@erpx/server-workflow/public/commands";
import { activeStaff, staffGuard } from "@erpx/server-operations/public/staff-policy";
import {approvalView,openApproval,prepareDecision,closeApprovals} from '@erpx/server-workflow/public/approval-planning';
import {participantSql} from '@erpx/server-workflow/public/approvals';
import { prepareAttachmentBinding } from "@erpx/server-files/public/attachments/service";
import { availableLeaveEvidence, leaveEvidenceManifest, pendingLeaveEvidenceCount, type LeaveEvidencePurpose } from "@erpx/server-people_payroll/public/leave-evidence";
import type { SqlStatement } from '@erpx/server-runtime/public/runtime/types';
import { BusinessRuleError } from '@erpx/server-shared/index';
import { leaveDraftSchema, leaveDecisionSchema, newLeaveSchema, leaveLifecycleSchema, type LeaveRequest } from '@erpx/server-people_payroll/domain/leave';
import { createLeaveApplication, type LeaveRepository } from '@erpx/server-people_payroll/application/leave';
import { leaveLegacyEmployeeFacts } from '@erpx/server-operations/public/leave-review-facts';
import { leaveReviewEvidence } from '@erpx/server-workflow/public/leave-review-facts';
export { leaveDraftSchema, leaveDecisionSchema, newLeaveSchema, leaveLifecycleSchema, type LeaveRequest } from '@erpx/server-people_payroll/domain/leave';
export { assertLeaveAttachmentAccess } from '@erpx/server-people_payroll/public/leave-evidence';
function application(provided?: SqlDatabase) {
    let database = provided!;
    const repository: LeaveRepository<SqlStatement> = { readGetLeaveLegacy(id: string, companyId: string) {
            return database.prepare("SELECT id FROM leave_requests WHERE id=? AND company_id=? AND state='pending_approval'")
                .bind(id, companyId)
                .first();
        },
        readGetLeaveRow(id: string, companyId: string, userId: string, userId2: string, userId3: string, assignmentId: string, userId4: string, assignmentId2: string, evidenceReader: boolean) {
            return database.prepare(`SELECT l.*,t.name leave_type_name FROM leave_requests l LEFT JOIN leave_types t ON t.id=l.leave_type_id AND t.company_id=l.company_id WHERE l.id=? AND l.company_id=? AND (l.created_by=? OR l.decided_by=? OR ${participantSql("leave", "l", "?", "?")} OR ${participantSql("leave_cancellation", "l", "?", "?")} OR ?::boolean)`)
                .bind(id, companyId, userId, userId2, userId3, assignmentId, userId4, assignmentId2, evidenceReader)
                .first<Omit<LeaveRequest, "approval" | "permittedActions">>();
        },
        readCreateLeaveEmployee(...args) { return leaveLegacyEmployeeFacts(database).readCreateLeaveEmployee(...args); },
        stageCreateLeaveEffect(...args) { return leaveLegacyEmployeeFacts(database).stageCreateLeaveEffect(...args); },
        stageCreateLeaveEffect2(...args) { return leaveLegacyEmployeeFacts(database).stageCreateLeaveEffect2(...args); },
        guard6(key: string, value: "leave-state", id: string, version: number, states: string[]) {
            return assertion(database, key, value, `EXISTS(SELECT 1 FROM leave_requests WHERE id=? AND version=? AND state IN (${states.map(() => "?").join(",")}))`, [id, version, ...states]);
        },
        guard7(key: string, value: "placement", engagement_id: string, engagement_version: number) {
            return assertion(database, key, value, "EXISTS(SELECT 1 FROM hr_engagements WHERE id=? AND version=?)", [engagement_id, engagement_version]);
        },
        stageSubmitLeaveEffect(startDate: string, days: number, handoverNote: string, state: "draft" | "pending_approval", person_id: string, engagement_id: string, value: string, department_id: string | null, userId: string, key: string, now: number, id: string) {
            return database.prepare("UPDATE leave_requests SET start_date=?,days=?,handover_note=?,state=?,person_id=?,engagement_id=?,branch_id=?,department_id=?,prepared_by=?,decided_by=NULL,decision_note=NULL,version=version+1,last_command_id=?,updated_at=? WHERE id=?")
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
        readDecideRow(id: string, companyId: string) { return database.prepare("SELECT * FROM leave_requests WHERE id=? AND company_id=?").bind(id, companyId).first<LeaveRequest>(); },
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
            return database.prepare("UPDATE leave_requests SET state=?,decision_note=?,decided_by=?,evidence_version=evidence_version+?,version=version+1,last_command_id=?,updated_at=? WHERE id=?")
                .bind(state, note, userId, value, key, now, id);
        },
        stageDecideEffect3(...args) { return leaveReviewEvidence(database).stageDecideEffect3(...args); },
        stageTransitionLeaveEffect(value: string, id: string, company_id: string, note: string, userId: string, now: number, now2: number) { return database.prepare("INSERT INTO leave_cancellation_reviews(id,request_id,company_id,reason,requested_by,created_at,updated_at) VALUES(?,?,?,?,?,?,?)").bind(value, id, company_id, note, userId, now, now2); },
        stageTransitionLeaveEffect2(key: string, now: number, id: string) { return database.prepare("UPDATE leave_requests SET cancellation_state='pending_review',version=version+1,last_command_id=?,updated_at=? WHERE id=?").bind(key, now, id); },
        stageTransitionLeaveEffect3(note: string, key: string, now: number, id: string) {
            return database.prepare("UPDATE leave_requests SET state='cancelled',decision_note=?,version=version+1,last_command_id=?,updated_at=? WHERE id=?")
                .bind(note, key, now, id);
        },
        readDecideLeaveCancellationRow(id: string, companyId: string) { return database.prepare("SELECT * FROM leave_requests WHERE id=? AND company_id=? AND state='approved' AND cancellation_state='pending_review'").bind(id, companyId).first<LeaveRequest>(); },
        stageDecideLeaveCancellationEffect(value: "approved" | "cancelled", value2: "approved" | "rejected" | "pending_review", key: string, now: number, id: string) { return database.prepare("UPDATE leave_requests SET state=?,cancellation_state=?,version=version+1,last_command_id=?,updated_at=? WHERE id=?").bind(value, value2, key, now, id); },
        stageDecideLeaveCancellationEffect2(value: "pending_approval" | "approved" | "rejected", userId: string, note: string, now: number, id: string) { return database.prepare("UPDATE leave_cancellation_reviews SET state=?,decided_by=?,decision_note=?,version=version+1,updated_at=? WHERE request_id=? AND state='pending_approval'").bind(value, userId, note, now, id); },
        stageBalanceTransitionInsert(value: string, kind: string, multiplier: number, key: string, value2: "Approved cancellation" | "Approved leave" | "Released reservation", actor: string, now: number, id: string, company_id: string, action: "use" | "release" | "reverse") {
            const source = action === 'reverse' ? "'usage','usage_reversal'" : "'reservation','reservation_release'";
            return database.prepare(`INSERT INTO leave_balance_entries(id,company_id,engagement_id,leave_type_id,request_id,kind,units,effective_date,entitlement_year,command_key,note,created_by,created_at)
 SELECT ?||':'||entitlement_year,company_id,engagement_id,leave_type_id,request_id,?,sum(units)*?,entitlement_year||'-01-01',entitlement_year,?||':'||entitlement_year,?,?,?
 FROM leave_balance_entries WHERE request_id=? AND company_id=? AND kind IN (${source}) GROUP BY company_id,engagement_id,leave_type_id,request_id,entitlement_year HAVING sum(units)<0`)
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
export async function getLeave(holdingId: string, id: string, context: WorkContext, provided?: SqlDatabase) { return withWorkforcePurchasedRead(context,async tx=>{const data=await invoke(() => application(tx).getLeave(holdingId,id,context)),purchase=await workforcePurchaseControls(context,tx);return {...data,...(purchase?{purchase}:{})};},provided); }
export async function createLeave(context: WorkContext, input: z.infer<typeof newLeaveSchema>, key: string) { return withWorkforcePurchasedCommand(context,{key,operation:"leave.create",purpose:"new-work",resultKind:"leave"},tx=>invoke(() => application(tx).createLeave(context,input,key))); }
export async function submitLeave(holdingId: string, id: string, input: z.infer<typeof leaveDraftSchema>, key: string, context: WorkContext) { return withWorkforcePurchasedCommand(context,{key,operation:input.saveOnly?"leave.save":"leave.submit",purpose:"new-work",target:{kind:"leave",id}},tx=>invoke(() => application(tx).submitLeave(holdingId,id,input,key,context))); }
export async function approveLeave(_holdingId: string, id: string, input: z.infer<typeof leaveDecisionSchema>, key: string, context: WorkContext) { return withWorkforcePurchasedCommand(context,{key,operation:"leave.approve",purpose:"settle-existing",target:{kind:"leave",id}},tx=>invoke(() => application(tx).approveLeave(_holdingId,id,input,key,context))); }
export async function transitionLeave(context: WorkContext, id: string, input: z.infer<typeof leaveLifecycleSchema>, key: string) { return withWorkforcePurchasedCommand(context,{key,operation:input.action==="reject"?"leave.reject":"leave.cancellation",purpose:"settle-existing",target:{kind:"leave",id}},tx=>invoke(() => application(tx).transitionLeave(context,id,input,key))); }
export async function decideLeaveCancellation(context: WorkContext, id: string, input: z.infer<typeof leaveDecisionSchema>, key: string, approve: boolean) { return withWorkforcePurchasedCommand(context,{key,operation:approve?"leave.cancellation.approve":"leave.cancellation.reject",purpose:"settle-existing",target:{kind:"leave",id}},tx=>invoke(() => application(tx).decideLeaveCancellation(context,id,input,key,approve))); }
