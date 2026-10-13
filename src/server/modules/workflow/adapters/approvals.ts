import {workflowFinalOwnerProjections as peoplepayrollFinalOwnerProjections} from "../../people_payroll/public/workflow-final-owner-projections";
const peoplepayrollFinalSources=peoplepayrollFinalOwnerProjections();
import { createApprovalsSourceRepository as organization_approvals } from '../../organization/public/workflow-approvals-facts';
import { createApprovalsSourceRepository as identity_access_approvals } from '../../identity_access/public/workflow-approvals-facts';
import { createApprovalsSourceRepository as people_payroll_approvals } from '../../people_payroll/public/workflow-approvals-facts';
import { createApprovalsSourceRepository as files_approvals } from '../../files/public/workflow-approvals-facts';
import { legacyApprovalRecord, stageLegacyApprovalScope, stagePeopleApprovalRevision } from '../../people_payroll/public/workflow-approval-effects';
import { stageTaskApprovalRevision } from '../../operations/public/workflow-approval-effects';
import { departmentRoute } from "@/server/modules/organization/public/department-authority";
import { personIdentityIds, currentPersonPrincipals } from "@/server/modules/identity_access/public/iam-person";
import type { SqlDatabase, SqlStatement } from "@/server/modules/runtime/public/runtime/types";
import { assertion } from "@/server/modules/workflow/adapters/commands";
import { appointedHrEligible, departmentStepEligible } from "@/server/modules/organization/public/department-authority";
import { organizationGuard } from "@/server/modules/organization/public/organization";
import { expensePoolCandidates, expenseAuthoritySnapshot, legacyExpenseRoute } from "@/server/modules/people_payroll/public/expense-routing";
import { activeStaff, staffPeople, reportingChain, authorityGuard, requireCompanyAuthority, eligibleRoleUsers, notification, requestRoute, staffGuard, type ApprovalRoute, type RecordScope, type StaffActor } from "@/server/modules/operations/public/staff-policy";
import { holdingResources as nativeHoldingResources } from '../../runtime/public/runtime';
import { policyRead as nativePolicyRead, policyNow as nativePolicyNow } from '../../identity_access/public/iam-read-snapshot';
import { replayCommand as nativeReplayCommand, executeCommand as nativeExecuteCommand } from '../../workflow/public/commands';
import { ApiError as NativeApiError } from '../../runtime/public/http';
import { type ApprovalKind, type ApprovalPhase, type ApprovalInstance, type ApprovalStep, type ApprovalDecision, type ApprovalView } from '../domain/approvals';
import { createApprovalsApplication, type ApprovalsRepository } from '../application/approvals';
import { BusinessRuleError } from '../../shared';
export { participantSql } from './approval-participant-projection';
export type { ApprovalKind } from '../domain/approvals';
export type { ApprovalPhase } from '../domain/approvals';
export type { ApprovalInstance } from '../domain/approvals';
export type { ApprovalStep } from '../domain/approvals';
export type { ApprovalDecision } from '../domain/approvals';
export type { ApprovalView } from '../domain/approvals';
type ApprovalReadOverrides = Partial<Pick<ApprovalsRepository<SqlStatement>, "readCurrentApprovalEffect" | "readStepsForEffect" | "readApprovalViewHistory">>;
function application(provided?: SqlDatabase, readOverrides?: ApprovalReadOverrides) {
    let database = provided!;
    const repository: ApprovalsRepository<SqlStatement> = { readCanOverrideStaffStepAppointed(companyId: string, person_id: string, now: number) { return organization_approvals(database).readCanOverrideStaffStepAppointed(companyId, person_id, now); },
        readCanOverrideStaffStepRole(assignmentId: string, companyId: string, userId: string, branchId: string | null, departmentId: string | null) { return identity_access_approvals(database).readCanOverrideStaffStepRole(assignmentId, companyId, userId, branchId, departmentId); },
        guard3(key: string, value: "override-current-right", assignmentId: string, companyId: string, userId: string, branchId: string | null, departmentId: string | null) { return identity_access_approvals(database).guard3(key, value, assignmentId, companyId, userId, branchId, departmentId); },
        guard4(key: string, value: "override-current-chain", companyId: string, assignedUserId: string, companyId2: string, companyId3: string, userId: string) { return people_payroll_approvals(database).guard4(key, value, companyId, assignedUserId, companyId2, companyId3, userId); },
        guard5(key: string, value: "override-independent", userId: string, requesterUserId: string, companyId: string, preparerUserId: string | null, userId2: string, preparerUserId2: string | null, companyId2: string) { return identity_access_approvals(database).guard5(key, value, userId, requesterUserId, companyId, preparerUserId, userId2, preparerUserId2, companyId2); },
        readCurrentApprovalEffect(kind: ApprovalKind, id: string, companyId: string) {
            return database.prepare("SELECT * FROM approval_instances WHERE record_type=? AND record_id=? AND company_id=? ORDER BY insertion_order DESC LIMIT 1")
                .bind(kind, id, companyId)
                .first<ApprovalInstance>();
        },
        readStepsForEffect(company_id: string, id: string) {
            return database.prepare(`SELECT s.*,COALESCE(p.legal_name,s.approver_user_id) approver_name FROM approval_steps s LEFT ${peoplepayrollFinalSources.approvals_hr_people_p_join} ON p.user_id=s.approver_user_id AND p.company_id=? AND p.merged_into_id IS NULL WHERE s.instance_id=? ORDER BY s.ordinal`)
                .bind(company_id, id)
                .all<ApprovalStep>();
        },
        readOpenApprovalPrevious(kind: ApprovalKind, id: string, phase: ApprovalPhase) {
            return database.prepare("SELECT COALESCE(MAX(cycle),0) cycle FROM approval_instances WHERE record_type=? AND record_id=? AND phase=?")
                .bind(kind, id, phase)
                .first<{
                cycle: number;
            }>();
        },
        guard9(key: string, value: string, check: NonNullable<ApprovalRoute["checks"]>[number]) { return assertion(database, key, value, check.query, check.args); },
        stageOpenApprovalStatements(now: number, kind: ApprovalKind, id: string) {
            return database.prepare("UPDATE approval_instances SET state='superseded',version=version+1,updated_at=? WHERE record_type=? AND record_id=? AND state IN ('pending','blocked')")
                .bind(now, kind, id);
        },
        stageOpenApprovalStatements2(instanceId: string, company_id: string, branch_id: string | null, department_id: string | null, kind: ApprovalKind, id: string, phase: ApprovalPhase, cycle: number, requester: string, assignee: string | null, value: "pending" | "approved", now: number, now2: number) {
            return database.prepare("INSERT INTO approval_instances(id,company_id,branch_id,department_id,record_type,record_id,phase,cycle,requester_user_id,assignee_user_id,state,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?,?,?, ?,?)")
                .bind(instanceId, company_id, branch_id, department_id, kind, id, phase, cycle, requester, assignee, value, now, now2);
        },
        guard12(key: string, value: string, company_id: string, appointedPersonId: string, userId: string) { return organization_approvals(database).guard12(key, value, company_id, appointedPersonId, userId); },
        stageOpenApprovalStatements3(value: string, instanceId: string, ordinal: number, userId: string, authority: "named" | "department" | "hr_admin" | "holding_owner", value2: string | null, value3: string | null, value4: string | null, value5: string | null) {
            return database.prepare("INSERT INTO approval_steps(id,instance_id,ordinal,approver_user_id,authority,approver_pool_json,required_role,stage,route_snapshot_json) VALUES(?,?,?,?,?,?,?,?,?)")
                .bind(value, instanceId, ordinal, userId, authority, value2, value3, value4, value5);
        },
        readInitializeLegacyApprovalExpense(record_id: string, company_id: string) { return people_payroll_approvals(database).readInitializeLegacyApprovalExpense(record_id, company_id); },
        readInitializeLegacyApprovalRecord(recordId: string, companyId: string, pendingState: 'pending_approval' | 'in_review', kind: ApprovalKind) { return legacyApprovalRecord(database, kind, recordId, companyId, pendingState); },
        guard16(key: string, value: "legacy-route", id: string, version: number, id2: string) {
            return assertion(database, key, value, "EXISTS(SELECT 1 FROM approval_instances WHERE id=? AND version=? AND state='blocked') AND NOT EXISTS(SELECT 1 FROM approval_steps WHERE instance_id=?)", [id, version, id2]);
        },
        guard17(key: string, value: "legacy-record", record_id: string, company_id: string, pendingState: "pending_approval" | "in_review", version: number, kind: ApprovalKind) {
            const { table, column } = nativeApprovalRecord(kind);
            return assertion(database, key, value, `EXISTS(SELECT 1 FROM ${table} WHERE id=? AND company_id=? AND ${column}=? AND version=?)`, [record_id, company_id, pendingState, version]);
        },
        guard18(key: string, value: string, check: NonNullable<ApprovalRoute["checks"]>[number]) { return assertion(database, key, value, check.query, check.args); },
        stageInitializeLegacyApprovalEffect(value: string, id: string, ordinal: number, userId: string, authority: "named" | "department" | "hr_admin" | "holding_owner") {
            return database.prepare("INSERT INTO approval_steps(id,instance_id,ordinal,approver_user_id,authority) VALUES(?,?,?,?,?)")
                .bind(value, id, ordinal, userId, authority);
        },
        stageInitializeLegacyApprovalEffect2(branch_id: string | null, department_id: string | null, now: number, id: string) {
            return database.prepare("UPDATE approval_instances SET state='pending',branch_id=?,department_id=?,blocked_reason=NULL,version=version+1,updated_at=? WHERE id=?")
                .bind(branch_id, department_id, now, id);
        },
        stageInitializeLegacyApprovalEffect3(branchId: string | null, departmentId: string | null, now: number, recordId: string, kind: ApprovalKind) { return stageLegacyApprovalScope(database, kind, branchId, departmentId, now, recordId); },
        stageInitializeLegacyApprovalEffect4(key: string, id: string, version: number, key2: string, now: number) {
            return database.prepare("INSERT INTO approval_decisions(id,instance_id,action,actor_user_id,reason,reviewed_version,command_id,created_at) VALUES(?,?,'initialize','system','Initialize pending request under department routing',?,?,?)")
                .bind(key, id, version, key2, now);
        },
        stageInitializeLegacyApprovalEffect5(key: string, key2: string, value: "system", value2: "system", value3: "approval.initialize", record_type: ApprovalKind, record_id: string, value4: number, value5: "Initialize route; no historical approval inferred", now: number) {
            return database.prepare("INSERT INTO audit_events VALUES(?,?,?,?,?,?,?,?,?,?)")
                .bind(key, key2, value, value2, value3, record_type, record_id, value4, value5, now);
        },
        stageInitializeLegacyApprovalEffect6(key: string, key2: string, value: string, record_type: ApprovalKind, record_id: string, value2: number, value3: string, now: number) {
            return database.prepare("INSERT INTO outbox_events(id,command_id,event_type,aggregate_type,aggregate_id,aggregate_version,payload_json,created_at) VALUES(?,?,?,?,?,?,?,?)")
                .bind(key, key2, value, record_type, record_id, value2, value3, now);
        },
        stageInitializeLegacyApprovalEffect7(key: string) { return database.prepare("DELETE FROM command_assertions WHERE command_id=?").bind(key); },
        guard26(key: string, label: string, approver_user_id: string, company_id: string, authority: "named" | "department" | "hr_admin" | "holding_owner", now: number, now2: number, authority2: "named" | "department" | "hr_admin" | "holding_owner", branch_id: string | null, department_id: string | null) { return identity_access_approvals(database).guard26(key, label, approver_user_id, company_id, authority, now, now2, authority2, branch_id, department_id); },
        readApprovalViewActiveScope(assignmentId: string, userId: string, companyId: string, role: string, now: number, now2: number, policyStep: boolean, branch_id: string | null, department_id: string | null) { return identity_access_approvals(database).readApprovalViewActiveScope(assignmentId, userId, companyId, role, now, now2, policyStep, branch_id, department_id); },
        readApprovalViewHistory(kind: "expense" | "leave" | "leave_cancellation" | "overtime" | "task", id: string, companyId: string) {
            return database.prepare(`SELECT d.*,COALESCE(p.legal_name,d.actor_user_id) actor_name,(SELECT legal_name ${peoplepayrollFinalSources.approvals_hr_people_from} WHERE company_id=i.company_id AND iam_same_person(user_id,d.previous_approver_user_id,company_id) AND merged_into_id IS NULL LIMIT 1) previous_approver_name,(SELECT legal_name ${peoplepayrollFinalSources.approvals_hr_people_from} WHERE company_id=i.company_id AND iam_same_person(user_id,d.approver_user_id,company_id) AND merged_into_id IS NULL LIMIT 1) approver_name FROM approval_decisions d JOIN approval_instances i ON i.id=d.instance_id LEFT ${peoplepayrollFinalSources.approvals_hr_people_p_join} ON p.user_id=d.actor_user_id AND p.company_id=i.company_id AND p.merged_into_id IS NULL WHERE i.record_type=? AND i.record_id=? AND i.company_id=? ORDER BY d.created_at,d.insertion_order`)
                .bind(kind, id, companyId)
                .all<ApprovalDecision>();
        },
        readExpenseDecisionEvidenceAssignmentsreceiptsperson(company_id: string, assignmentId: string, now: number, now2: number) { return identity_access_approvals(database).readExpenseDecisionEvidenceAssignmentsreceiptsperson(company_id, assignmentId, now, now2); },
        readExpenseDecisionEvidenceAssignmentsreceiptsperson2(record_id: string) { return files_approvals(database).readExpenseDecisionEvidenceAssignmentsreceiptsperson2(record_id); },
        guard31(key: string, value: "approval-version", id: string, version: number, ordinal: number, id2: string, userId: string, override: boolean) {
            return assertion(database, key, value, override
                ? "EXISTS(SELECT 1 FROM approval_instances WHERE id=? AND version=? AND state='pending' AND current_step=?) AND EXISTS(SELECT 1 FROM approval_steps WHERE id=? AND state='pending' AND approver_user_id<>?)"
                : "EXISTS(SELECT 1 FROM approval_instances WHERE id=? AND version=? AND state='pending' AND current_step=?) AND EXISTS(SELECT 1 FROM approval_steps WHERE id=? AND ? IN (SELECT approver_user_id UNION SELECT value FROM erpx_json_each(COALESCE(approver_pool_json,'[]'))) AND state='pending')", [id, version, ordinal, id2, userId]);
        },
        guard32(key: string, value: "override-authority", assignmentId: string, companyId: string, userId: string, branch_id: string | null, department_id: string | null) { return identity_access_approvals(database).guard32(key, value, assignmentId, companyId, userId, branch_id, department_id); },
        guard33(key: string, value: "appointed-approval-current", value2: (string | null)[], policyStep: boolean) { return organization_approvals(database).guard33(key, value, value2, policyStep); },
        guard34(key: string, value: "active-decision-role", assignmentId: string, value2: string) { return identity_access_approvals(database).guard34(key, value, assignmentId, value2); },
        guard35(key: string, value: "override-reporting-chain", companyId: string, approver_user_id: string, companyId2: string, companyId3: string, userId: string) { return people_payroll_approvals(database).guard35(key, value, companyId, approver_user_id, companyId2, companyId3, userId); },
        guard36(key: string, value: "override-independent", userId: string, requester_user_id: string, company_id: string, assignee_user_id: string | null, userId2: string, assignee_user_id2: string | null, company_id2: string, id: string, ordinal: number, userId3: string, company_id3: string) { return assertion(database, key, value, `NOT iam_same_person(?, ?, ?) AND (?::text IS NULL OR NOT iam_same_person(?, ?, ?)) AND NOT EXISTS(SELECT 1 FROM approval_steps s WHERE s.instance_id=? AND s.ordinal<? AND s.state='approved' AND iam_same_person(s.approver_user_id,?,?))`, [userId, requester_user_id, company_id, assignee_user_id, userId2, assignee_user_id2, company_id2, id, ordinal, userId3, company_id3]); },
        guard37(key: string, value: "override-expense-preparer", id: string, company_id: string, userId: string, userId2: string) { return people_payroll_approvals(database).guard37(key, value, id, company_id, userId, userId2); },
        guard38(key: string, value: "expense-separation", id: string, userId: string, userId2: string, id2: string, ordinal: number, userId3: string) { return people_payroll_approvals(database).guard38(key, value, id, userId, userId2, id2, ordinal, userId3); },
        guard39(key: string, value: "expense-role", company_id: string, assignmentId: string, required_role: string, now: number, now2: number, branch_id: string | null, department_id: string | null) { return identity_access_approvals(database).guard39(key, value, company_id, assignmentId, required_role, now, now2, branch_id, department_id); },
        stagePrepareDecisionStatements(value: "approved" | "rejected", userId: string, id: string) {
            return database.prepare("UPDATE approval_steps SET state=?,approver_user_id=? WHERE id=?")
                .bind(value, userId, id);
        },
        stagePrepareDecisionStatements2(value: "pending" | "approved" | "rejected", value2: number, now: number, id: string) {
            return database.prepare("UPDATE approval_instances SET state=?,current_step=?,version=version+1,updated_at=? WHERE id=?")
                .bind(value, value2, now, id);
        },
        stagePrepareDecisionStatements3(value: string, id: string, id2: string, value2: "override" | "approve" | "reject", userId: string, value3: string | null, userId2: string, reason: string, version: number, key: string, now: number, decisionEvidence: string) {
            return database.prepare("INSERT INTO approval_decisions(id,instance_id,step_id,action,actor_user_id,previous_approver_user_id,approver_user_id,reason,reviewed_version,command_id,created_at,authority_json) VALUES(?,?,?,?,?,?,?,?,?,?,?,?)")
                .bind(value, id, id2, value2, userId, value3, userId2, reason, version, key, now, decisionEvidence);
        },
        stageCloseApprovalsEffect(key: string, action: "cancel" | "revise", actorId: string, reason: string, version: number, key2: string, now: number, kind: ApprovalKind, id: string) {
            return database.prepare("INSERT INTO approval_decisions(id,instance_id,action,actor_user_id,reason,reviewed_version,command_id,created_at) SELECT ?||'-'||id,id,?,?,?,?,?,? FROM approval_instances WHERE record_type=? AND record_id=? AND state IN ('pending','blocked','approved')")
                .bind(key, action, actorId, reason, version, key2, now, kind, id);
        },
        stageCloseApprovalsEffect2(value: "cancelled" | "superseded", now: number, kind: ApprovalKind, id: string) {
            return database.prepare("UPDATE approval_instances SET state=?,version=version+1,updated_at=? WHERE record_type=? AND record_id=? AND state IN ('pending','blocked','approved')")
                .bind(value, now, kind, id);
        },
        readRerouteApprovalInstance(id: string, companyId: string) {
            return database.prepare("SELECT * FROM approval_instances WHERE id=? AND company_id=?")
                .bind(id, companyId)
                .first<ApprovalInstance>();
        },
        readRerouteApprovalEffect(record_id: string, approverUserId: string, approverUserId2: string) { return people_payroll_approvals(database).readRerouteApprovalEffect(record_id, approverUserId, approverUserId2); },
        readRerouteApprovalClaim(record_id: string, company_id: string) { return people_payroll_approvals(database).readRerouteApprovalClaim(record_id, company_id); },
        stageRerouteApprovalEffect(value: string, approverUserId: string, companyId: string, value2: "expense" | "leave" | "staff_task" | "leave_cancellation" | "overtime", record_id: string) {
            return database.prepare("UPDATE flow_routes SET steps_json=jsonb_set(steps_json::jsonb,ARRAY[?::text,'primaryUserId'],to_jsonb(?::text),true)::text,version=version+1 WHERE company_id=? AND kind=? AND record_id=?")
                .bind(value, approverUserId, companyId, value2, record_id);
        },
        guard49(key: string, value: "route-version", id: string, instanceVersion: number, current_step: number, record_id: string, companyId: string, expectedVersion: number, state: "approved" | "pending_approval" | "in_review" | "pending_authorization" | "pending_hr" | "pending_verification" | "start_approval" | "completion_approval", kind: ApprovalKind) {
            const { table, column } = nativeApprovalRecord(kind);
            return assertion(database, key, value, `EXISTS(SELECT 1 FROM approval_instances WHERE id=? AND version=? AND state='pending' AND current_step=?) AND EXISTS(SELECT 1 FROM ${table} WHERE id=? AND company_id=? AND version=? AND ${column}=?)`, [
                id,
                instanceVersion,
                current_step,
                record_id,
                companyId,
                expectedVersion,
                state,
            ]);
        },
        stageRerouteApprovalEffect2(approverUserId: string, value: string | null, authority: "named" | "department" | "hr_admin" | "holding_owner", value2: string | null, id: string) {
            return database.prepare("UPDATE approval_steps SET approver_user_id=?,approver_pool_json=?,authority=?,route_snapshot_json=? WHERE id=? AND state='pending'")
                .bind(approverUserId, value, authority, value2, id);
        },
        stageRerouteApprovalEffect3(now: number, id: string) {
            return database.prepare("UPDATE approval_instances SET blocked_reason=NULL,version=version+1,updated_at=? WHERE id=?")
                .bind(now, id);
        },
        stageRerouteApprovalEffect4(value: string, id: string, id2: string, userId: string, approver_user_id: string, approverUserId: string, reason: string, expectedVersion: number, key: string, now: number) {
            return database.prepare("INSERT INTO approval_decisions(id,instance_id,step_id,action,actor_user_id,previous_approver_user_id,approver_user_id,reason,reviewed_version,command_id,created_at) VALUES(?,?,?,'reroute',?,?,?,?,?,?,?)")
                .bind(value, id, id2, userId, approver_user_id, approverUserId, reason, expectedVersion, key, now);
        },
        stageRerouteApprovalEffect5(now: number, recordId: string, kind: ApprovalKind) { return kind === 'expense' || kind === 'leave' || kind === 'leave_cancellation' || kind === 'overtime' ? stagePeopleApprovalRevision(database, kind, now, recordId) : stageTaskApprovalRevision(database, now, recordId); } };
    Object.assign(repository, readOverrides);
    return createApprovalsApplication({ repository, clock: { now: Date.now }, identifiers: { next: () => crypto.randomUUID() }, commands: { replay: (...args) => nativeReplayCommand(database, ...args), commit: (...args) => nativeExecuteCommand(database, ...args) }, prepare: async (holding) => { if (!provided)
            database = (await nativeHoldingResources(holding)).write; }, production: process.env.NODE_ENV === 'production', reportRouteBlocked: event => console.warn(JSON.stringify(event)), batch: effects => database.batch(effects), policyRead: (key, read) => nativePolicyRead(database, key, read), policyNow: nativePolicyNow, departmentRoute: (...args) => fromNative(() => departmentRoute(database, ...args)), personIdentityIds: (...args) => fromNative(() => personIdentityIds(database, ...args)), currentPersonPrincipals: (...args) => fromNative(() => currentPersonPrincipals(database, ...args)), appointedHrEligible: (...args) => fromNative(() => appointedHrEligible(database, ...args)), departmentStepEligible: (...args) => fromNative(() => departmentStepEligible(database, ...args)), organizationGuard: (...args) => fromNative(() => organizationGuard(database, ...args)), expensePoolCandidates: (...args) => fromNative(() => expensePoolCandidates(database, ...args)), expenseAuthoritySnapshot: (...args) => fromNative(() => expenseAuthoritySnapshot(database, ...args)), legacyExpenseRoute: (...args) => fromNative(() => legacyExpenseRoute(database, ...args)), activeStaff: (...args) => fromNative(() => activeStaff(database, ...args)), staffPeople: (...args) => fromNative(() => staffPeople(database, ...args)), reportingChain: (...args) => reportingChain(...args), authorityGuard: (...args) => authorityGuard(database, ...args), requireCompanyAuthority: (...args) => fromNative(() => requireCompanyAuthority(database, ...args)), eligibleRoleUsers: (...args) => fromNative(() => eligibleRoleUsers(database, ...args)), notification: (...args) => notification(database, ...args), requestRoute: (...args) => fromNative(() => requestRoute(database, ...args)), staffGuard: (...args) => staffGuard(database, ...args) });
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
function invokeSync<Result>(run: () => Result) { try {
    return run();
}
catch (error) {
    if (error instanceof BusinessRuleError)
        throw new NativeApiError(error.status, error.code, error.message);
    throw error;
} }
export async function canOverrideStaffStep(db: SqlDatabase, context: {
    companyId: string;
    branchId: string | null;
    departmentId: string | null;
    requesterUserId: string;
    preparerUserId: string | null;
    assignedUserId: string;
}, actor: Pick<StaffActor, "companyId" | "userId" | "role" | "assignmentId">): Promise<boolean> { return invoke(() => application(db).canOverrideStaffStep(context, actor)); }
export async function staffOverrideGuards(db: SqlDatabase, key: string, context: {
    companyId: string;
    branchId: string | null;
    departmentId: string | null;
    requesterUserId: string;
    preparerUserId: string | null;
    assignedUserId: string;
}, actor: Pick<StaffActor, "companyId" | "userId" | "role" | "assignmentId">): Promise<SqlStatement[]> { return invoke(() => application(db).staffOverrideGuards(key, context, actor)); }
export async function currentApproval(db: SqlDatabase, kind: ApprovalKind, id: string, companyId: string): Promise<ApprovalInstance | null> { return invoke(() => application(db).currentApproval(kind, id, companyId)); }
export async function taskRoute(db: SqlDatabase, scope: RecordScope, creator: string, assignee: string, phase: ApprovalPhase, users: string[], serviceTask = false): Promise<ApprovalRoute> { return invoke(() => application(db).taskRoute(scope, creator, assignee, phase, users, serviceTask)); }
export async function openApproval(db: SqlDatabase, key: string, kind: ApprovalKind, id: string, phase: ApprovalPhase, requester: string, assignee: string | null, route: ApprovalRoute, recordVersion: number): Promise<{
    statements: SqlStatement[];
    instanceId: string;
}> { return invoke(() => application(db).openApproval(key, kind, id, phase, requester, assignee, route, recordVersion)); }
export async function initializeLegacyApproval(db: SqlDatabase, instance: ApprovalInstance): Promise<ApprovalInstance> { return invoke(() => application(db).initializeLegacyApproval(instance)); }
export async function stepBlock(db: SqlDatabase, instance: ApprovalInstance, step: ApprovalStep, steps: ApprovalStep[]): Promise<string | null> { return invoke(() => application(db).stepBlock(instance, step, steps)); }
export async function approvalView(db: SqlDatabase, kind: ApprovalKind, id: string, actor: Pick<StaffActor, "companyId" | "userId" | "role" | "assignmentId"> & {
    inspection?: true;
}): Promise<ApprovalView> { return invoke(() => application(db).approvalView(kind, id, actor)); }
/** Request-owned read facts. Blocked legacy routes retain the mutating reader. */
export async function expenseApprovalReadBatch(db: SqlDatabase, actor: Pick<StaffActor,"companyId"|"userId"|"role"|"assignmentId">, ids: readonly string[]) {
    const requested = new Set(ids);
    if (requested.size > 250) throw new Error("EXPENSE_BATCH_TOO_LARGE");
    const encoded = JSON.stringify([...requested]);
    const instances = requested.size ? (await db.prepare("SELECT DISTINCT ON(record_id) * FROM approval_instances WHERE record_type='expense' AND company_id=? AND record_id IN(SELECT value FROM erpx_json_each(?)) ORDER BY record_id,insertion_order DESC").bind(actor.companyId,encoded).all<ApprovalInstance>()).results : [];
    const current = new Map(instances.map(row => [row.record_id,row]));
    const stable = instances.filter(row => row.state !== "blocked");
    const [steps, history] = requested.size ? await Promise.all([
        stable.length ? db.prepare(`SELECT s.*,COALESCE(p.legal_name,s.approver_user_id) approver_name FROM approval_steps s LEFT ${peoplepayrollFinalSources.approvals_hr_people_p_join} ON p.user_id=s.approver_user_id AND p.company_id=? AND p.merged_into_id IS NULL WHERE s.instance_id IN(SELECT value FROM erpx_json_each(?)) ORDER BY s.ordinal`).bind(actor.companyId,JSON.stringify(stable.map(row => row.id))).all<ApprovalStep>() : Promise.resolve({results:[] as ApprovalStep[]}),
        db.prepare(`SELECT i.record_id batch_record_id,d.*,COALESCE(p.legal_name,d.actor_user_id) actor_name,(SELECT legal_name ${peoplepayrollFinalSources.approvals_hr_people_from} WHERE company_id=i.company_id AND iam_same_person(user_id,d.previous_approver_user_id,company_id) AND merged_into_id IS NULL LIMIT 1) previous_approver_name,(SELECT legal_name ${peoplepayrollFinalSources.approvals_hr_people_from} WHERE company_id=i.company_id AND iam_same_person(user_id,d.approver_user_id,company_id) AND merged_into_id IS NULL LIMIT 1) approver_name FROM approval_decisions d JOIN approval_instances i ON i.id=d.instance_id LEFT ${peoplepayrollFinalSources.approvals_hr_people_p_join} ON p.user_id=d.actor_user_id AND p.company_id=i.company_id AND p.merged_into_id IS NULL WHERE i.record_type='expense' AND i.record_id IN(SELECT value FROM erpx_json_each(?)) AND i.company_id=? ORDER BY d.created_at,d.insertion_order`).bind(encoded,actor.companyId).all<ApprovalDecision & {batch_record_id:string}>(),
    ]) : [{results:[] as ApprovalStep[]},{results:[] as (ApprovalDecision & {batch_record_id:string})[]}];
    const stepsByInstance = new Map<string,ApprovalStep[]>(), historyByRecord = new Map<string,ApprovalDecision[]>();
    for (const step of steps.results) { const group=stepsByInstance.get(step.instance_id)??[]; group.push(step); stepsByInstance.set(step.instance_id,group); }
    for (const {batch_record_id,...decision} of history.results) { const group=historyByRecord.get(batch_record_id)??[]; group.push(decision); historyByRecord.set(batch_record_id,group); }
    const views = new Map<string,Promise<ApprovalView>>();
    return (id:string):Promise<ApprovalView> => {
        if (!requested.has(id)) return approvalView(db,"expense",id,actor);
        let view=views.get(id);
        if (!view) {
            const instance=current.get(id)??null;
            if (instance?.state === "blocked") view=approvalView(db,"expense",id,actor);
            else view=invoke(() => application(db, {
                readCurrentApprovalEffect:async(kind,recordId,company) => {
                    if (kind!=="expense" || recordId!==id || company!==actor.companyId) throw new Error("EXPENSE_BATCH_SCOPE_MISMATCH");
                    return instance;
                },
                readStepsForEffect:async(company,instanceId) => {
                    if (company!==actor.companyId || instanceId!==instance?.id) throw new Error("EXPENSE_BATCH_SCOPE_MISMATCH");
                    return {results:stepsByInstance.get(instanceId)??[]};
                },
                readApprovalViewHistory:async(kind,recordId,company) => {
                    if (kind!=="expense" || recordId!==id || company!==actor.companyId) throw new Error("EXPENSE_BATCH_SCOPE_MISMATCH");
                    return {results:historyByRecord.get(id)??[]};
                },
            }).approvalView("expense",id,actor));
            views.set(id,view);
        }
        return view;
    };
}
export async function prepareDecision(db: SqlDatabase, actor: StaffActor, key: string, kind: ApprovalKind, id: string, approve: boolean, reason: string, version: number, override = false): Promise<{
    statements: SqlStatement[];
    final: boolean;
    instance: ApprovalInstance;
    next: ApprovalStep | undefined;
}> { return invoke(() => application(db).prepareDecision(actor, key, kind, id, approve, reason, version, override)); }
export function closeApprovals(db: SqlDatabase, key: string, kind: ApprovalKind, id: string, actorId: string, action: "cancel" | "revise", reason: string, version: number) { return invokeSync(() => application(db).closeApprovals(key, kind, id, actorId, action, reason, version)); }
export async function rerouteApproval(db: SqlDatabase, actor: StaffActor, id: string, input: {
    expectedVersion: number;
    instanceVersion: number;
    approverUserId: string;
    reason: string;
}, key: string): Promise<Record<string, unknown>> { return invoke(() => application(db).rerouteApproval(actor, id, input, key)); }
function nativeApprovalRecord(kind: ApprovalKind) { return { table: kind === "leave" || kind === "leave_cancellation" ? "leave_requests" : kind === "overtime" ? "overtime_requests" : kind === "task" ? "staff_tasks" : "finance_expenses", column: kind === "expense" ? "status" : "state" }; }
