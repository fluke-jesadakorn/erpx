import { hasWorkflowPermission } from "@erpx/feature-identity_access/lib/permissions";
import type { ApprovalRoute, RecordScope, StaffActor } from "@erpx/server-operations/public/staff-policy";
import type { departmentRoute } from "@erpx/server-organization/public/department-authority";
import type { personIdentityIds } from "@erpx/server-identity_access/public/iam-person";
import type { currentPersonPrincipals } from "@erpx/server-identity_access/public/iam-person";
import type { appointedHrEligible } from "@erpx/server-organization/public/department-authority";
import type { departmentStepEligible } from "@erpx/server-organization/public/department-authority";
import type { organizationGuard } from "@erpx/server-organization/public/organization";
import type { expensePoolCandidates } from "@erpx/server-people_payroll/public/expense-routing";
import type { expenseAuthoritySnapshot } from "@erpx/server-people_payroll/public/expense-routing";
import type { legacyExpenseRoute } from "@erpx/server-people_payroll/public/expense-routing";
import type { activeStaff } from "@erpx/server-operations/public/staff-policy";
import type { staffPeople } from "@erpx/server-operations/public/staff-policy";
import type { reportingChain } from "@erpx/server-operations/public/staff-policy";
import type { authorityGuard } from "@erpx/server-operations/public/staff-policy";
import type { requireCompanyAuthority } from "@erpx/server-operations/public/staff-policy";
import type { eligibleRoleUsers } from "@erpx/server-operations/public/staff-policy";
import type { notification } from "@erpx/server-operations/public/staff-policy";
import type { requestRoute } from "@erpx/server-operations/public/staff-policy";
import type { staffGuard } from "@erpx/server-operations/public/staff-policy";
import { type ApprovalKind, type ApprovalPhase, type ApprovalInstance, type ApprovalStep, type ApprovalDecision, type ApprovalView } from '@erpx/server-workflow/domain/approvals';
import { BusinessRuleError, type Clock, type IdentifierSource } from '@erpx/server-shared/index';
import type { OwnerCommands } from '@erpx/server-workflow/domain/owner-command-ports';
type Tail<Value extends unknown[]> = Value extends [
    unknown,
    ...infer Rest
] ? Rest : never;
type PortValue<Value, Effect> = Value extends {
    bind(...args: never[]): unknown;
    run(): unknown;
} ? Effect : Value extends {
    guard: (...args: infer Args) => unknown;
} ? Omit<Value, 'guard'> & {
    guard(...args: Args): Effect;
} : Value extends readonly {
    bind(...args: never[]): unknown;
    run(): unknown;
}[] ? Effect[] : Value;
type PortResult<Value, Effect> = Value extends Promise<infer Result> ? Promise<PortValue<Result, Effect>> : PortValue<Value, Effect>;
export interface ApprovalsRepository<Effect> {
    readCanOverrideStaffStepAppointed(companyId: string, person_id: string, now: number): Promise<Record<string, unknown> | null>;
    readCanOverrideStaffStepRole(assignmentId: string, companyId: string, userId: string, branchId: string | null, departmentId: string | null): Promise<Record<string, unknown> | null>;
    guard3(key: string, value: "override-current-right", assignmentId: string, companyId: string, userId: string, branchId: string | null, departmentId: string | null): Effect;
    guard4(key: string, value: "override-current-chain", companyId: string, assignedUserId: string, companyId2: string, companyId3: string, userId: string): Effect;
    guard5(key: string, value: "override-independent", userId: string, requesterUserId: string, companyId: string, preparerUserId: string | null, userId2: string, preparerUserId2: string | null, companyId2: string): Effect;
    readCurrentApprovalEffect(kind: ApprovalKind, id: string, companyId: string): Promise<ApprovalInstance | null>;
    readStepsForEffect(company_id: string, id: string): Promise<{
        results: (ApprovalStep)[];
    }>;
    readOpenApprovalPrevious(kind: ApprovalKind, id: string, phase: ApprovalPhase): Promise<{
        cycle: number;
    } | null>;
    guard9(key: string, value: string, check: NonNullable<ApprovalRoute["checks"]>[number]): Effect;
    stageOpenApprovalStatements(now: number, kind: ApprovalKind, id: string): Effect;
    stageOpenApprovalStatements2(instanceId: string, company_id: string, branch_id: string | null, department_id: string | null, kind: ApprovalKind, id: string, phase: ApprovalPhase, cycle: number, requester: string, assignee: string | null, value: "pending" | "approved", now: number, now2: number): Effect;
    guard12(key: string, value: string, company_id: string, appointedPersonId: string, userId: string): Effect;
    stageOpenApprovalStatements3(value: string, instanceId: string, ordinal: number, userId: string, authority: "named" | "department" | "hr_admin" | "holding_owner", value2: string | null, value3: string | null, value4: string | null, value5: string | null): Effect;
    readInitializeLegacyApprovalExpense(record_id: string, company_id: string): Promise<{
        created_by: string;
        prepared_by: string | null;
    } | null>;
    readInitializeLegacyApprovalRecord(record_id: string, company_id: string, pendingState: "pending_approval" | "in_review", kind: ApprovalKind): Promise<{
        version: number;
    } | null>;
    guard16(key: string, value: "legacy-route", id: string, version: number, id2: string): Effect;
    guard17(key: string, value: "legacy-record", record_id: string, company_id: string, pendingState: "pending_approval" | "in_review", version: number, kind: ApprovalKind): Effect;
    guard18(key: string, value: string, check: NonNullable<ApprovalRoute["checks"]>[number]): Effect;
    stageInitializeLegacyApprovalEffect(value: string, id: string, ordinal: number, userId: string, authority: "named" | "department" | "hr_admin" | "holding_owner"): Effect;
    stageInitializeLegacyApprovalEffect2(branch_id: string | null, department_id: string | null, now: number, id: string): Effect;
    stageInitializeLegacyApprovalEffect3(value: string | null, department_id: string | null, now: number, record_id: string, kind: ApprovalKind): Effect;
    stageInitializeLegacyApprovalEffect4(key: string, id: string, version: number, key2: string, now: number): Effect;
    stageInitializeLegacyApprovalEffect5(key: string, key2: string, value: "system", value2: "system", value3: "approval.initialize", record_type: ApprovalKind, record_id: string, value4: number, value5: "Initialize route; no historical approval inferred", now: number): Effect;
    stageInitializeLegacyApprovalEffect6(key: string, key2: string, value: string, record_type: ApprovalKind, record_id: string, value2: number, value3: string, now: number): Effect;
    stageInitializeLegacyApprovalEffect7(key: string): Effect;
    guard26(key: string, label: string, approver_user_id: string, company_id: string, authority: "named" | "department" | "hr_admin" | "holding_owner", now: number, now2: number, authority2: "named" | "department" | "hr_admin" | "holding_owner", branch_id: string | null, department_id: string | null): Effect;
    readApprovalViewActiveScope(assignmentId: string, userId: string, companyId: string, role: string, now: number, now2: number, policyStep: boolean, branch_id: string | null, department_id: string | null): Promise<Record<string, unknown> | null>;
    readApprovalViewHistory(kind: "expense" | "leave" | "leave_cancellation" | "overtime" | "task", id: string, companyId: string): Promise<{
        results: (ApprovalDecision)[];
    }>;
    readExpenseDecisionEvidenceAssignmentsreceiptsperson(company_id: string, assignmentId: string, now: number, now2: number): Promise<{
        results: (Record<string, unknown>)[];
    }>;
    readExpenseDecisionEvidenceAssignmentsreceiptsperson2(record_id: string): Promise<{
        results: (Record<string, unknown>)[];
    }>;
    guard31(key: string, value: "approval-version", id: string, version: number, ordinal: number, id2: string, userId: string, override: boolean): Effect;
    guard32(key: string, value: "override-authority", assignmentId: string, companyId: string, userId: string, branch_id: string | null, department_id: string | null): Effect;
    guard33(key: string, value: "appointed-approval-current", value2: (string | null)[], policyStep: boolean): Effect;
    guard34(key: string, value: "active-decision-role", assignmentId: string, value2: string): Effect;
    guard35(key: string, value: "override-reporting-chain", companyId: string, approver_user_id: string, companyId2: string, companyId3: string, userId: string): Effect;
    guard36(key: string, value: "override-independent", userId: string, requester_user_id: string, company_id: string, assignee_user_id: string | null, userId2: string, assignee_user_id2: string | null, company_id2: string, id: string, ordinal: number, userId3: string, company_id3: string): Effect;
    guard37(key: string, value: "override-expense-preparer", id: string, company_id: string, userId: string, userId2: string): Effect;
    guard38(key: string, value: "expense-separation", id: string, userId: string, userId2: string, id2: string, ordinal: number, userId3: string): Effect;
    guard39(key: string, value: "expense-role", company_id: string, assignmentId: string, required_role: string, now: number, now2: number, branch_id: string | null, department_id: string | null): Effect;
    stagePrepareDecisionStatements(value: "approved" | "rejected", userId: string, id: string): Effect;
    stagePrepareDecisionStatements2(value: "pending" | "approved" | "rejected", value2: number, now: number, id: string): Effect;
    stagePrepareDecisionStatements3(value: string, id: string, id2: string, value2: "override" | "approve" | "reject", userId: string, value3: string | null, userId2: string, reason: string, version: number, key: string, now: number, decisionEvidence: string): Effect;
    stageCloseApprovalsEffect(key: string, action: "cancel" | "revise", actorId: string, reason: string, version: number, key2: string, now: number, kind: ApprovalKind, id: string): Effect;
    stageCloseApprovalsEffect2(value: "cancelled" | "superseded", now: number, kind: ApprovalKind, id: string): Effect;
    readRerouteApprovalInstance(id: string, companyId: string): Promise<ApprovalInstance | null>;
    readRerouteApprovalEffect(record_id: string, approverUserId: string, approverUserId2: string): Promise<Record<string, unknown> | null>;
    readRerouteApprovalClaim(record_id: string, company_id: string): Promise<{
        claimant_user_id: string;
        prepared_by: string;
    } | null>;
    stageRerouteApprovalEffect(value: string, approverUserId: string, companyId: string, value2: "expense" | "leave" | "staff_task" | "leave_cancellation" | "overtime", record_id: string): Effect;
    guard49(key: string, value: "route-version", id: string, instanceVersion: number, current_step: number, record_id: string, companyId: string, expectedVersion: number, state: "approved" | "pending_approval" | "in_review" | "pending_authorization" | "pending_hr" | "pending_verification" | "start_approval" | "completion_approval", kind: ApprovalKind): Effect;
    stageRerouteApprovalEffect2(approverUserId: string, value: string | null, authority: "named" | "department" | "hr_admin" | "holding_owner", value2: string | null, id: string): Effect;
    stageRerouteApprovalEffect3(now: number, id: string): Effect;
    stageRerouteApprovalEffect4(value: string, id: string, id2: string, userId: string, approver_user_id: string, approverUserId: string, reason: string, expectedVersion: number, key: string, now: number): Effect;
    stageRerouteApprovalEffect5(now: number, record_id: string, kind: ApprovalKind): Effect;
}
export interface ApprovalsPorts<Effect> {
    repository: ApprovalsRepository<Effect>;
    clock: Clock;
    identifiers: IdentifierSource;
    commands: OwnerCommands<Effect>;
    prepare(holdingId: string): Promise<void>;
    production: boolean;
    reportRouteBlocked(event: {
        event: string;
        recordType: ApprovalKind;
        recordId: string;
        companyId: string;
        reason: string;
    }): void;
    batch(effects: Effect[]): Promise<unknown>;
    policyRead<Result>(key: string, read: () => Promise<Result>): Promise<Result>;
    policyNow(): number;
    departmentRoute(...args: Tail<Parameters<typeof departmentRoute>>): PortResult<ReturnType<typeof departmentRoute>, Effect>;
    personIdentityIds(...args: Tail<Parameters<typeof personIdentityIds>>): PortResult<ReturnType<typeof personIdentityIds>, Effect>;
    currentPersonPrincipals(...args: Tail<Parameters<typeof currentPersonPrincipals>>): PortResult<ReturnType<typeof currentPersonPrincipals>, Effect>;
    appointedHrEligible(...args: Tail<Parameters<typeof appointedHrEligible>>): PortResult<ReturnType<typeof appointedHrEligible>, Effect>;
    departmentStepEligible(...args: Tail<Parameters<typeof departmentStepEligible>>): PortResult<ReturnType<typeof departmentStepEligible>, Effect>;
    organizationGuard(...args: Tail<Parameters<typeof organizationGuard>>): PortResult<ReturnType<typeof organizationGuard>, Effect>;
    expensePoolCandidates(...args: Tail<Parameters<typeof expensePoolCandidates>>): PortResult<ReturnType<typeof expensePoolCandidates>, Effect>;
    expenseAuthoritySnapshot(...args: Tail<Parameters<typeof expenseAuthoritySnapshot>>): PortResult<ReturnType<typeof expenseAuthoritySnapshot>, Effect>;
    legacyExpenseRoute(...args: Tail<Parameters<typeof legacyExpenseRoute>>): PortResult<ReturnType<typeof legacyExpenseRoute>, Effect>;
    activeStaff(...args: Tail<Parameters<typeof activeStaff>>): PortResult<ReturnType<typeof activeStaff>, Effect>;
    staffPeople(...args: Tail<Parameters<typeof staffPeople>>): PortResult<ReturnType<typeof staffPeople>, Effect>;
    reportingChain(...args: Parameters<typeof reportingChain>): PortResult<ReturnType<typeof reportingChain>, Effect>;
    authorityGuard(...args: Tail<Parameters<typeof authorityGuard>>): PortResult<ReturnType<typeof authorityGuard>, Effect>;
    requireCompanyAuthority(...args: Tail<Parameters<typeof requireCompanyAuthority>>): PortResult<ReturnType<typeof requireCompanyAuthority>, Effect>;
    eligibleRoleUsers(...args: Tail<Parameters<typeof eligibleRoleUsers>>): PortResult<ReturnType<typeof eligibleRoleUsers>, Effect>;
    notification(...args: Tail<Parameters<typeof notification>>): PortResult<ReturnType<typeof notification>, Effect>;
    requestRoute(...args: Tail<Parameters<typeof requestRoute>>): PortResult<ReturnType<typeof requestRoute>, Effect>;
    staffGuard(...args: Tail<Parameters<typeof staffGuard>>): PortResult<ReturnType<typeof staffGuard>, Effect>;
}
export function createApprovalsApplication<Effect>(ports: ApprovalsPorts<Effect>) {
    const r = ports.repository;
    async function canOverrideStaffStep(context: {
        companyId: string;
        branchId: string | null;
        departmentId: string | null;
        requesterUserId: string;
        preparerUserId: string | null;
        assignedUserId: string;
    }, actor: Pick<StaffActor, "companyId" | "userId" | "role" | "assignmentId">): Promise<boolean> {
        const [people, assigned, reviewer] = await Promise.all([ports.staffPeople(context.companyId), ports.activeStaff(context.companyId, context.assignedUserId).catch(() => null), ports.activeStaff(context.companyId, actor.userId).catch(() => null)]);
        if (!assigned || !reviewer || !assigned.level_number || !reviewer.level_number || reviewer.level_number >= assigned.level_number)
            return false;
        if ([context.requesterUserId, context.preparerUserId].some(id => id && people.some(p => p.user_id === id && p.person_id === reviewer.person_id)))
            return false;
        if (!ports.reportingChain(people, assigned.person_id).some(p => p.person_id === reviewer.person_id))
            return false;
        const appointed = await r.readCanOverrideStaffStepAppointed(context.companyId, reviewer.person_id, ports.clock.now());
        const role = await r.readCanOverrideStaffStepRole(actor.assignmentId, context.companyId, actor.userId, context.branchId, context.departmentId);
        return !!(appointed || role);
    }
    async function canOverrideDepartment(instance: ApprovalInstance, step: ApprovalStep, actor: Pick<StaffActor, "companyId" | "userId" | "role" | "assignmentId">) {
        if (instance.state !== "pending" || !(step.stage === "department_approval" || step.authority === "department"))
            return false;
        return canOverrideStaffStep({ companyId: instance.company_id, branchId: instance.branch_id, departmentId: instance.department_id, requesterUserId: instance.requester_user_id, preparerUserId: instance.assignee_user_id, assignedUserId: step.approver_user_id }, actor);
    }
    async function staffOverrideGuards(key: string, context: {
        companyId: string;
        branchId: string | null;
        departmentId: string | null;
        requesterUserId: string;
        preparerUserId: string | null;
        assignedUserId: string;
    }, actor: Pick<StaffActor, "companyId" | "userId" | "role" | "assignmentId">): Promise<Effect[]> {
        return [await ports.organizationGuard(context.companyId, key),
            r.guard3(key, "override-current-right", actor.assignmentId, context.companyId, actor.userId, context.branchId, context.departmentId),
            r.guard4(key, "override-current-chain", context.companyId, context.assignedUserId, context.companyId, context.companyId, actor.userId),
            r.guard5(key, "override-independent", actor.userId, context.requesterUserId, context.companyId, context.preparerUserId, actor.userId, context.preparerUserId, context.companyId),
            ports.staffGuard(key, context.companyId, actor.userId, "override-staff")];
    }
    async function currentApproval(kind: ApprovalKind, id: string, companyId: string): Promise<ApprovalInstance | null> {
        return r.readCurrentApprovalEffect(kind, id, companyId);
    }
    async function stepsFor(instance: ApprovalInstance) {
        return (await r.readStepsForEffect(instance.company_id, instance.id)).results;
    }
    async function taskRoute(scope: RecordScope, creator: string, assignee: string, phase: ApprovalPhase, users: string[], serviceTask = false): Promise<ApprovalRoute> {
        try {
            await ports.activeStaff(scope.company_id, assignee);
            if (users.length > 20 || new Set(users).size !== users.length)
                throw new BusinessRuleError(422, "APPROVAL_STEPS_INVALID", "Choose up to twenty different ordered approvers per phase.");
            for (const user of users) {
                if (user === assignee || (phase === "start" && user === creator))
                    throw new BusinessRuleError(422, "SELF_APPROVAL", "Assignees cannot approve their work; creators cannot approve their own start request.");
                await ports.activeStaff(scope.company_id, user);
            }
            // A task assigned by someone else is work to perform, not the assignee's
            // department request. A self-submitted task follows the department flow.
            const department = users.length && creator === assignee && scope.department_id && !serviceTask ? await ports.departmentRoute(scope.company_id, assignee) : null;
            if (department?.blockedReason)
                throw new BusinessRuleError(409, "DEPARTMENT_FLOW_REQUIRED", department.blockedReason);
            return {
                scope,
                steps: [...(department?.steps ?? []), ...users.map((userId) => ({ userId, authority: "named" as const }))],
                checks: department?.checks,
                blockedReason: null,
            };
        }
        catch (error) {
            if (!(error instanceof BusinessRuleError))
                throw error;
            return { scope, steps: [], blockedReason: error.message };
        }
    }
    async function openApproval(key: string, kind: ApprovalKind, id: string, phase: ApprovalPhase, requester: string, assignee: string | null, route: ApprovalRoute, recordVersion: number): Promise<{
        statements: Effect[];
        instanceId: string;
    }> {
        if (route.blockedReason)
            throw new BusinessRuleError(409, "APPROVAL_ROUTE_BLOCKED", route.blockedReason);
        const previous = await r.readOpenApprovalPrevious(kind, id, phase);
        const instanceId = `approval-${key}`, cycle = (previous?.cycle ?? 0) + 1, now = ports.clock.now();
        const statements = [
            ...(route.checks ?? []).map((check, i) => r.guard9(key, `route-snapshot-${i}`, check)),
            r.stageOpenApprovalStatements(now, kind, id),
            r.stageOpenApprovalStatements2(instanceId, route.scope.company_id, route.scope.branch_id, route.scope.department_id, kind, id, phase, cycle, requester, assignee, route.steps.length ? "pending" : "approved", now, now),
            ...route.steps.flatMap((s, ordinal) => [
                ports.staffGuard(key, route.scope.company_id, s.userId, `route-staff-${ordinal}`),
                ...(s.authority === "hr_admin" || s.authority === "holding_owner"
                    ? [
                        s.authority === "hr_admin" && s.snapshot?.appointmentKind === "hr" && typeof s.snapshot.appointedPersonId === "string"
                            ? r.guard12(key, `route-role-${ordinal}`, route.scope.company_id, s.snapshot.appointedPersonId, s.userId)
                            : roleGuard(key, route.scope, { approver_user_id: s.userId, authority: s.authority }, `route-role-${ordinal}`),
                    ]
                    : []),
                r.stageOpenApprovalStatements3(`${instanceId}-${ordinal}`, instanceId, ordinal, s.userId, s.authority, s.candidateUserIds && !(s.snapshot && "departmentPolicyVersion" in s.snapshot)
                    ? JSON.stringify(s.candidateUserIds)
                    : null, s.requiredRole ?? null, s.stage ?? null, s.snapshot ? JSON.stringify(s.snapshot) : null),
            ]),
        ];
        if (route.steps[0])
            for (const userId of route.steps[0].candidateUserIds ?? [
                route.steps[0].userId,
            ])
                statements.push(ports.notification(route.scope, userId, kind, id, recordVersion, `${phase === "start" ? "Start" : phase === "completion" ? "Completion" : "Request"} approval needs your decision`));
        return { statements, instanceId };
    }
    async function initializeLegacyApproval(instance: ApprovalInstance): Promise<ApprovalInstance> {
        if (["task", "overtime", "leave_cancellation", "legal"].includes(instance.record_type) || instance.state !== "blocked")
            return instance;
        if ((await stepsFor(instance)).length)
            return instance;
        const expense = instance.record_type === "expense"
            ? await r.readInitializeLegacyApprovalExpense(instance.record_id, instance.company_id)
            : null;
        const route = await ports.requestRoute(instance.company_id, instance.requester_user_id, instance.record_type === "expense" ? "expense" : "leave", expense
            ? [
                expense.created_by,
                ...(expense.prepared_by ? [expense.prepared_by] : []),
            ]
            : []);
        if (route.blockedReason) {
            ports.reportRouteBlocked({
                event: "approval_route_blocked",
                recordType: instance.record_type,
                recordId: instance.record_id,
                companyId: instance.company_id,
                reason: route.blockedReason,
            });
            return { ...instance, blocked_reason: route.blockedReason };
        }
        const pendingState = instance.record_type === 'leave' ? 'pending_approval' : 'in_review';
        const record = await r.readInitializeLegacyApprovalRecord(instance.record_id, instance.company_id, pendingState, instance.record_type);
        if (!record)
            return instance;
        const key = `initialize-${instance.id}`, now = ports.clock.now();
        try {
            await ports.batch([
                r.guard16(key, "legacy-route", instance.id, instance.version, instance.id),
                r.guard17(key, "legacy-record", instance.record_id, instance.company_id, pendingState, record.version, instance.record_type),
                ports.staffGuard(key, instance.company_id, instance.requester_user_id, "legacy-requester"),
                ...(route.checks ?? []).map((check, i) => r.guard18(key, `legacy-snapshot-${i}`, check)),
                ...route.steps.flatMap((s, ordinal) => [
                    ports.staffGuard(key, instance.company_id, s.userId, `eligible-${ordinal}`),
                    ...(s.authority === "hr_admin" || s.authority === "holding_owner"
                        ? [
                            roleGuard(key, route.scope, { approver_user_id: s.userId, authority: s.authority }, `legacy-role-${ordinal}`),
                        ]
                        : []),
                    r.stageInitializeLegacyApprovalEffect(`${instance.id}-${ordinal}`, instance.id, ordinal, s.userId, s.authority),
                ]),
                r.stageInitializeLegacyApprovalEffect2(route.scope.branch_id, route.scope.department_id, now, instance.id),
                r.stageInitializeLegacyApprovalEffect3(instance.record_type === "leave"
                    ? (route.scope.branch_id ?? "")
                    : route.scope.branch_id, route.scope.department_id, now, instance.record_id, instance.record_type),
                r.stageInitializeLegacyApprovalEffect4(key, instance.id, record.version, key, now),
                r.stageInitializeLegacyApprovalEffect5(key, key, "system", "system", "approval.initialize", instance.record_type, instance.record_id, record.version + 1, "Initialize route; no historical approval inferred", now),
                r.stageInitializeLegacyApprovalEffect6(key, key, `${instance.record_type}.approval.initialize`, instance.record_type, instance.record_id, record.version + 1, JSON.stringify({
                    id: instance.record_id,
                    state: pendingState,
                    companyId: instance.company_id,
                    branchId: route.scope.branch_id,
                    departmentId: route.scope.department_id,
                    version: record.version + 1,
                }), now),
                ports.notification(route.scope, route.steps[0].userId, instance.record_type, instance.record_id, instance.version, "Request approval needs your decision"),
                r.stageInitializeLegacyApprovalEffect7(key),
            ]);
        }
        catch (error) {
            if (!/constraint|UNIQUE/i.test(String(error)))
                throw error;
        }
        return ((await currentApproval(instance.record_type, instance.record_id, instance.company_id)) ?? instance);
    }
    async function stepBlock(instance: ApprovalInstance, step: ApprovalStep, steps: ApprovalStep[]): Promise<string | null> {
        const snapshot = JSON.parse(step.route_snapshot_json ?? "{}") as Record<string, unknown>;
        if (step.stage === "department_approval" && "departmentPolicyVersion" in snapshot && !await ports.departmentStepEligible(instance.company_id, step.approver_user_id, snapshot))
            return "The selected department approver no longer occupies the saved position. Reroute with a reason.";
        if (step.stage === "hr_approval" && snapshot.appointedPersonId && !await ports.appointedHrEligible(instance.company_id, step.approver_user_id, snapshot.appointedPersonId))
            return "The appointed HR reviewer is unavailable. Reroute with a reason.";
        if (instance.record_type === "expense" && step.approver_pool_json)
            return (await ports.expensePoolCandidates(instance, step, steps)).length
                ? null
                : "No eligible independent reviewer remains. Resolve this approval in Administration.";
        try {
            const principalMap = await ports.currentPersonPrincipals(instance.company_id, [step.approver_user_id, ...steps.map(s => s.approver_user_id), instance.requester_user_id, ...(instance.assignee_user_id ? [instance.assignee_user_id] : [])]);
            const principal = (id: string) => principalMap.get(id) ?? id;
            await ports.activeStaff(instance.company_id, step.approver_user_id);
            if (instance.assignee_user_id)
                await ports.activeStaff(instance.company_id, instance.assignee_user_id);
            if (instance.assignee_user_id && principal(instance.assignee_user_id) === principal(step.approver_user_id) ||
                (instance.phase !== "completion" &&
                    principal(instance.requester_user_id) === principal(step.approver_user_id)))
                return "This approval would be a self-approval.";
            if ((step.authority === "hr_admin" && !snapshot.appointedPersonId) || step.authority === "holding_owner") {
                if (!(await ports.eligibleRoleUsers(instance, step.authority)).includes(principal(step.approver_user_id)))
                    return "The assigned approver no longer has the required company or HR authority.";
                if (step.authority === "hr_admin" &&
                    steps.some((s) => s.ordinal < step.ordinal &&
                        principal(s.approver_user_id) === principal(step.approver_user_id)))
                    return "The HR decision must be independent of the department decision.";
            }
            return null;
        }
        catch (error) {
            if (!(error instanceof BusinessRuleError))
                throw error;
            return "The assigned approver is unavailable. An administrator must reroute with a reason.";
        }
    }
    function roleGuard(key: string, instance: RecordScope, step: Pick<ApprovalStep, "approver_user_id" | "authority">, label = "approval-authority") {
        const now = ports.clock.now();
        return r.guard26(key, label, step.approver_user_id, instance.company_id, step.authority, now, now, step.authority, instance.branch_id, instance.department_id);
    }
    async function approvalView(kind: ApprovalKind, id: string, actor: Pick<StaffActor, "companyId" | "userId" | "role" | "assignmentId"> & {
        inspection?: true;
    }): Promise<ApprovalView> {
        if (kind === "legal")
            throw new BusinessRuleError(403, "LEGAL_OWNER_REQUIRED", "Use the Legal approval endpoint with record access checks.");
        let instance = await currentApproval(kind, id, actor.companyId);
        if (instance && !actor.inspection)
            instance = await initializeLegacyApproval(instance);
        const steps = instance ? await stepsFor(instance) : [];
        const currentApprover = instance && instance.state === "pending"
            ? (steps.find((s) => s.ordinal === instance.current_step) ?? null)
            : null;
        const poolCandidates = currentApprover?.approver_pool_json && instance?.record_type === "expense"
            ? await ports.expensePoolCandidates(instance, currentApprover, steps)
            : null;
        const blockedReason = instance?.blocked_reason ??
            (currentApprover
                ? poolCandidates
                    ? poolCandidates.length
                        ? null
                        : "No eligible independent reviewer remains. Resolve this approval in Administration."
                    : await stepBlock(instance!, currentApprover, steps)
                : null);
        const policyStep = !!currentApprover && (currentApprover.stage === "department_approval" && "departmentPolicyVersion" in JSON.parse(currentApprover.route_snapshot_json ?? "{}") || currentApprover.stage === "hr_approval" && !!JSON.parse(currentApprover.route_snapshot_json ?? "{}").appointedPersonId);
        const activeScope = instance ? !!await r.readApprovalViewActiveScope(actor.assignmentId, actor.userId, actor.companyId, actor.role, ports.clock.now(), ports.clock.now(), policyStep, instance.branch_id, instance.department_id) : false;
        const history = (await r.readApprovalViewHistory(kind, id, actor.companyId)).results;
        const eligibleApproverIds = blockedReason ? [] : await ports.personIdentityIds(actor.companyId, poolCandidates ?? (currentApprover ? [currentApprover.approver_user_id] : []));
        const canOverride = !!instance && !!currentApprover && await canOverrideDepartment(instance, currentApprover, actor);
        return {
            instance,
            steps,
            history,
            currentApprover,
            blockedReason,
            eligibleApproverIds,
            canOverride,
            canDecide: !!currentApprover && activeScope &&
                !blockedReason &&
                eligibleApproverIds.includes(actor.userId) && (policyStep || kind === "task" && currentApprover.authority === "named" && !currentApprover.required_role || hasWorkflowPermission(actor, currentApprover.required_role ?? (["named", "department"].includes(currentApprover.authority) ? "people_manager" : currentApprover.authority))),
        };
    }
    async function expenseDecisionEvidence(actor: StaffActor, instance: ApprovalInstance, step: ApprovalStep, eligible: string[]) {
        const now = ports.clock.now();
        const [assignments, receipts, person] = await Promise.all([
            r.readExpenseDecisionEvidenceAssignmentsreceiptsperson(instance.company_id, actor.assignmentId, now, now),
            r.readExpenseDecisionEvidenceAssignmentsreceiptsperson2(instance.record_id),
            ports.activeStaff(instance.company_id, actor.userId).catch((error) => {
                if (error instanceof BusinessRuleError)
                    return null;
                throw error;
            }),
        ]);
        return JSON.stringify({
            actorId: actor.userId,
            companyId: instance.company_id,
            branchId: instance.branch_id,
            departmentId: instance.department_id,
            stage: step.stage,
            authority: step.authority,
            requiredRole: step.required_role,
            route: JSON.parse(step.route_snapshot_json ?? "{}"),
            eligiblePersonIds: eligible,
            activeAuthority: { role: actor.role, assignmentId: actor.assignmentId, ...actor.authority },
            assignments: assignments.results,
            person,
            receipts: receipts.results,
        });
    }
    async function prepareDecision(actor: StaffActor, key: string, kind: ApprovalKind, id: string, approve: boolean, reason: string, version: number, override = false): Promise<{
        statements: Effect[];
        final: boolean;
        instance: ApprovalInstance;
        next: ApprovalStep | undefined;
    }> {
        if (kind === 'legal')
            throw new BusinessRuleError(409, 'LEGAL_OWNER_REQUIRED', 'Use the Legal decision adapter with document and signer checks.');
        if ((override && reason.trim().length < 3) || (!approve && !reason.trim()))
            throw new BusinessRuleError(422, "REASON_REQUIRED", "Explain why this work is being rejected.");
        const authoritySnapshot = kind === "expense"
            ? await ports.expenseAuthoritySnapshot(actor.companyId)
            : null;
        const view = await approvalView(kind, id, actor), instance = view.instance, step = view.currentApprover;
        if (!instance || !step || !(override ? view.canOverride : view.canDecide))
            throw new BusinessRuleError(403, "CURRENT_APPROVER_REQUIRED", view.blockedReason ??
                "Only the named approver of the current step can decide.");
        const next = view.steps.find((s) => s.ordinal === step.ordinal + 1);
        const stepSnapshot = JSON.parse(step.route_snapshot_json ?? "{}") as Record<string, unknown>;
        const policyStep = step.stage === "department_approval" && "departmentPolicyVersion" in stepSnapshot;
        const appointedHrStep = step.stage === "hr_approval" && typeof stepSnapshot.appointedPersonId === "string";
        const decisionEvidence = override
            ? JSON.stringify({ override: true, outcome: approve ? "approve" : "reject", originalApproverUserId: step.approver_user_id, actualApproverUserId: actor.userId, activeAuthority: { role: actor.role, assignmentId: actor.assignmentId, ...actor.authority } })
            : kind === "expense" && step.approver_pool_json
                ? await expenseDecisionEvidence(actor, instance, step, view.eligibleApproverIds ?? [])
                : JSON.stringify({ activeAuthority: { role: actor.role, assignmentId: actor.assignmentId, ...actor.authority, companyId: actor.companyId, branchId: actor.branchId, departmentId: actor.departmentId } });
        const final = !approve || !next, now = ports.clock.now();
        const statements = [
            ...(override ? [await ports.organizationGuard(actor.companyId, key)] : []),
            ...(authoritySnapshot
                ? [authoritySnapshot.guard(key)]
                : []),
            r.guard31(key, "approval-version", instance.id, instance.version, step.ordinal, step.id, actor.userId, override),
            ...(override ? [r.guard32(key, "override-authority", actor.assignmentId, actor.companyId, actor.userId, instance.branch_id, instance.department_id)] : policyStep || appointedHrStep ? [r.guard33(key, "appointed-approval-current", policyStep ? [stepSnapshot.selectedPersonId as string, actor.companyId, actor.userId, (stepSnapshot.selector as {
                        positionId?: string;
                    })?.positionId ?? null, (stepSnapshot.selector as {
                        positionId?: string;
                    })?.positionId ?? null] : [actor.companyId, stepSnapshot.appointedPersonId as string, actor.userId], policyStep)] : [
                r.guard34(key, "active-decision-role", actor.assignmentId, step.required_role ?? (kind === "task" && step.authority === "named" ? actor.role : ["named", "department"].includes(step.authority) ? "people_manager" : step.authority)),
            ]),
            ...(override ? [r.guard35(key, "override-reporting-chain", actor.companyId, step.approver_user_id, actor.companyId, actor.companyId, actor.userId)] : []),
            ports.staffGuard(key, actor.companyId, actor.userId, "decision-staff"),
            ...(override ? [
                r.guard36(key, "override-independent", actor.userId, instance.requester_user_id, instance.company_id, instance.assignee_user_id, actor.userId, instance.assignee_user_id, instance.company_id, instance.id, step.ordinal, actor.userId, instance.company_id),
                ...(kind === "expense" ? [r.guard37(key, "override-expense-preparer", id, instance.company_id, actor.userId, actor.userId)] : []),
            ] : []),
            ...(instance.assignee_user_id
                ? [
                    ports.staffGuard(key, actor.companyId, instance.assignee_user_id, "decision-assignee"),
                ]
                : []),
            ...(!step.approver_pool_json &&
                ((step.authority === "hr_admin" && !appointedHrStep) || step.authority === "holding_owner")
                ? [
                    roleGuard(key, instance, {
                        ...step,
                        approver_user_id: actor.userId,
                    }),
                ]
                : []),
            ...(kind === "expense" && step.approver_pool_json && !override
                ? [
                    r.guard38(key, "expense-separation", id, actor.userId, actor.userId, instance.id, step.ordinal, actor.userId),
                    ...(step.required_role
                        ? [
                            r.guard39(key, "expense-role", instance.company_id, actor.assignmentId, step.required_role, now, now, instance.branch_id, instance.department_id),
                        ]
                        : []),
                ]
                : []),
            r.stagePrepareDecisionStatements(approve ? "approved" : "rejected", actor.userId, step.id),
            r.stagePrepareDecisionStatements2(!approve ? "rejected" : final ? "approved" : "pending", next && approve ? next.ordinal : step.ordinal, now, instance.id),
            r.stagePrepareDecisionStatements3(ports.identifiers.next(), instance.id, step.id, override ? "override" : approve ? "approve" : "reject", actor.userId, override ? step.approver_user_id : null, actor.userId, reason, version, key, now, decisionEvidence),
            ports.notification(instance, instance.requester_user_id, kind, id, version + 1, approve
                ? final
                    ? "Approval complete"
                    : "Step approved; awaiting the next decision"
                : "Rejected: correction required"),
        ];
        if (approve && next)
            for (const userId of kind === "expense" && next.approver_pool_json
                ? await ports.expensePoolCandidates(instance, next, view.steps.map((s) => s.id === step.id
                    ? { ...s, state: "approved", approver_user_id: actor.userId }
                    : s))
                : [next.approver_user_id])
                statements.push(ports.notification(instance, userId, kind, id, version + 1, "Approval needs your decision"));
        return { statements, final, instance, next };
    }
    function closeApprovals(key: string, kind: ApprovalKind, id: string, actorId: string, action: "cancel" | "revise", reason: string, version: number) {
        const now = ports.clock.now();
        return [
            r.stageCloseApprovalsEffect(key, action, actorId, reason, version, key, now, kind, id),
            r.stageCloseApprovalsEffect2(action === "cancel" ? "cancelled" : "superseded", now, kind, id),
        ];
    }
    async function rerouteApproval(actor: StaffActor, id: string, input: {
        expectedVersion: number;
        instanceVersion: number;
        approverUserId: string;
        reason: string;
    }, key: string): Promise<Record<string, unknown>> {
        await ports.requireCompanyAuthority(actor);
        const authoritySnapshot = await ports.expenseAuthoritySnapshot(actor.companyId);
        const instance = await r.readRerouteApprovalInstance(id, actor.companyId);
        if (!instance)
            throw new BusinessRuleError(409, "ROUTE_CONFIGURATION_REQUIRED", "Configure the request route before rerouting an active step.");
        if (instance.record_type === 'legal')
            throw new BusinessRuleError(409, 'LEGAL_OWNER_REQUIRED', 'Use Legal to replace approvers and review document scope.');
        const operation = `${instance.record_type}.approval.reroute`, prior = await ports.commands.replay(actor, key, operation, instance.record_id, input);
        if (prior)
            return prior;
        if (instance.state !== "pending")
            throw new BusinessRuleError(409, "APPROVAL_NOT_PENDING", "Only a pending approval can be rerouted.");
        const steps = await stepsFor(instance), current = steps.find((s) => s.ordinal === instance.current_step);
        if (!current)
            throw new BusinessRuleError(409, "APPROVAL_STEP_MISSING", "The current approval step is missing.");
        if (["leave", "leave_cancellation", "overtime"].includes(instance.record_type) &&
            steps.some((s) => s.id !== current.id && s.approver_user_id === input.approverUserId))
            throw new BusinessRuleError(422, "INDEPENDENT_HR_REQUIRED", "Department and HR decisions must remain independent.");
        if (instance.record_type === "expense" &&
            (await r.readRerouteApprovalEffect(instance.record_id, input.approverUserId, input.approverUserId)))
            throw new BusinessRuleError(422, "SELF_APPROVAL", "An expense preparer cannot review their own preparation.");
        const replacement = {
            ...current,
            approver_user_id: input.approverUserId,
            ...(current.approver_pool_json
                ? { approver_pool_json: JSON.stringify([input.approverUserId]) }
                : {}),
        };
        if (current.stage === "department_approval") {
            const snapshot = JSON.parse(current.route_snapshot_json ?? "{}") as Record<string, unknown>;
            if ("departmentPolicyVersion" in snapshot) {
                const selected = await ports.activeStaff(instance.company_id, input.approverUserId);
                replacement.route_snapshot_json = JSON.stringify({
                    ...snapshot,
                    selector: { kind: "person", personId: selected.person_id },
                    selectedPersonId: selected.person_id,
                    selectedPositionId: selected.position_id,
                    reroutedFromPersonId: snapshot.selectedPersonId,
                    rerouted: true,
                });
            }
        }
        if (current.approver_pool_json && current.stage === "department_approval" && !JSON.parse(current.route_snapshot_json ?? "{}").spendingPolicy) {
            const claim = await r.readRerouteApprovalClaim(instance.record_id, instance.company_id);
            if (claim) {
                const route = await ports.legacyExpenseRoute(instance.company_id, claim.claimant_user_id, [claim.prepared_by]);
                if (route.steps[0]) {
                    replacement.authority = route.steps[0].authority;
                    replacement.route_snapshot_json = JSON.stringify(route.steps[0].snapshot ?? {});
                }
            }
        }
        const blocked = await stepBlock(instance, replacement, steps);
        if (blocked)
            throw new BusinessRuleError(422, "APPROVER_INELIGIBLE", blocked);
        const state = instance.record_type === "leave"
            ? "pending_approval"
            : instance.record_type === "leave_cancellation" ? "approved"
                : instance.record_type === "overtime" ? instance.phase === "start" ? "pending_authorization" : current.authority === "hr_admin" ? "pending_hr" : "pending_verification"
                    : instance.record_type === "expense"
                        ? "in_review"
                        : instance.phase === "start"
                            ? "start_approval"
                            : "completion_approval", now = ports.clock.now();
        return ports.commands.commit(actor, key, operation, input, instance.record_id, input.expectedVersion + 1, {
            id: instance.record_id,
            instanceId: id,
            version: input.expectedVersion + 1,
        }, [
            ports.authorityGuard(key, actor),
            r.stageRerouteApprovalEffect(current.stage ?? (instance.record_type === "leave" ? `approval_${current.ordinal}` : `${instance.phase}_${current.ordinal}`), input.approverUserId, actor.companyId, instance.record_type === "task" ? "staff_task" : instance.record_type, instance.record_id),
            ...(current.approver_pool_json ? [authoritySnapshot.guard(key)] : []),
            ports.staffGuard(key, actor.companyId, input.approverUserId),
            ...(!replacement.approver_pool_json &&
                (replacement.authority === "hr_admin" ||
                    replacement.authority === "holding_owner")
                ? [roleGuard(key, instance, replacement)]
                : []),
            r.guard49(key, "route-version", id, input.instanceVersion, instance.current_step, instance.record_id, actor.companyId, input.expectedVersion, state, instance.record_type),
            r.stageRerouteApprovalEffect2(input.approverUserId, replacement.approver_pool_json ?? null, replacement.authority, replacement.route_snapshot_json ?? null, current.id),
            r.stageRerouteApprovalEffect3(now, id),
            r.stageRerouteApprovalEffect4(ports.identifiers.next(), id, current.id, actor.userId, current.approver_user_id, input.approverUserId, input.reason, input.expectedVersion, key, now),
            r.stageRerouteApprovalEffect5(now, instance.record_id, instance.record_type),
            ports.notification(instance, input.approverUserId, instance.record_type, instance.record_id, input.expectedVersion + 1, "Approval rerouted to you"),
        ], {
            companyId: instance.company_id,
            branchId: instance.branch_id,
            departmentId: instance.department_id,
        });
    }
    return { canOverrideStaffStep, staffOverrideGuards, currentApproval, taskRoute, openApproval, initializeLegacyApproval, stepBlock, approvalView, prepareDecision, closeApprovals, rerouteApproval };
}
