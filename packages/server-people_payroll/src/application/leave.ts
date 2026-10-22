import { z } from 'zod';
import { hasPermission } from "@erpx/feature-identity_access/lib/permissions";
import type { peopleRequestRoute } from "@erpx/server-organization/public/department-authority";
import type { activeStaff, staffGuard } from "@erpx/server-operations/public/staff-policy";
import type { approvalView, openApproval, prepareDecision, closeApprovals } from "@erpx/server-workflow/public/approvals";
import type { prepareAttachmentBinding } from "@erpx/server-files/public/attachments/service";
import type { availableLeaveEvidence, leaveEvidenceManifest, pendingLeaveEvidenceCount } from "@erpx/server-people_payroll/public/leave-evidence";
import type { WorkContext } from '@erpx/server-identity_access/public/authorization';
import { parseLeaveEvidencePolicy, type LeaveEvidencePolicy, type LeaveEvidencePurpose } from '@erpx/server-people_payroll/domain/leave-evidence';
import { BusinessRuleError, type Clock, type IdentifierSource } from '@erpx/server-shared/index';
import type { RosterCommands } from '@erpx/server-people_payroll/application/people-roster';
import { leaveDraftSchema, leaveDecisionSchema, newLeaveSchema, leaveLifecycleSchema, type LeaveRequest, eventScope } from '@erpx/server-people_payroll/domain/leave';
type Tail<Value extends unknown[]> = Value extends [
    unknown,
    ...infer Rest
] ? Rest : never;
type PortResult<Value, Effect> = Value extends Promise<infer Result> ? Promise<PortResult<Result, Effect>> : Value extends {
    bind(...args: never[]): unknown;
    run(): unknown;
} ? Effect : Value extends (...args: never[]) => unknown ? Value : Value extends readonly unknown[] ? {
    [Index in keyof Value]: PortResult<Value[Index], Effect>;
} : Value extends object ? {
    [Key in keyof Value]: PortResult<Value[Key], Effect>;
} : Value;
export interface LeaveRepository<Effect> {
    readGetLeaveLegacy(id: string, companyId: string): Promise<Record<string, unknown> | null>;
    readGetLeaveRow(id: string, companyId: string, userId: string, userId2: string, userId3: string, assignmentId: string, userId4: string, assignmentId2: string, evidenceReader: boolean): Promise<Omit<LeaveRequest, "approval" | "permittedActions"> | null>;
    readCreateLeaveEmployee(person_id: string, companyId: string): Promise<{
        id: string;
    } | null>;
    stageCreateLeaveEffect(employeeId: string, person_id: string, companyId: string, value: string, department_id: string | null, legal_name: string, now: number): Effect;
    stageCreateLeaveEffect2(id: string, value: string, person_id: string, companyId: string, person_id2: string, engagement_id: string, companyId2: string, value2: string, department_id: string | null, startDate: string, days: number, handoverNote: string, userId: string, now: number, now2: number): Effect;
    guard6(key: string, value: "leave-state", id: string, version: number, states: string[]): Effect;
    guard7(key: string, value: "placement", engagement_id: string, engagement_version: number): Effect;
    stageSubmitLeaveEffect(startDate: string, days: number, handoverNote: string, state: "draft" | "pending_approval", person_id: string, engagement_id: string, value: string, department_id: string | null, userId: string, key: string, now: number, id: string): Effect;
    readDecisionEvidencePolicyRule(policy_version_id: string, leave_type_id: string): Promise<{
        evidence_min_units: number | null;
        evidence_purpose: LeaveEvidencePurpose | null;
        evidence_min_count: number;
        evidence_due_stage: "before_submit" | "before_final_approval";
        evidence_grace_days: number;
        evidence_note: string;
        evidence_privacy: "requester_approvers_hr";
    } | null>;
    readDecideRow(id: string, companyId: string): Promise<LeaveRequest | null>;
    guard11(key: string, value: "leave-evidence-canonical-count", id: string, company_id: string, length: number): Effect;
    guard12(key: string, value: string, id: string, company_id: string, linkId: string, sourceId: string, purposeKey: string, checksumSha256: string, mediaType: string, bytes: number): Effect;
    guard13(key: string, value: "leave-evidence-legacy-count", id: string, company_id: string, length: number): Effect;
    guard14(key: string, value: string, sourceId: string, id: string, company_id: string, purposeKey: string, checksumSha256: string, mediaType: string, bytes: number): Effect;
    guard15(key: string, value: "leave-evidence-no-pending", id: string, company_id: string): Effect;
    guard16(key: string, value: "leave-evidence-required-count", value2: (string | number)[], legacyIncluded: boolean): Effect;
    stageDecideEvidenceManifestStep(value: string, id: string, company_id: string, id2: string, key: string, expectedVersion: number, value2: string | null, userId: string, value3: string, now: number): Effect;
    stageDecideEffect(id: string, company_id: string): Effect;
    stageDecideEffect2(state: "pending_approval" | "approved" | "rejected", note: string, userId: string, value: 0 | 1, key: string, now: number, id: string): Effect;
    stageDecideEffect3(value: string, key: string, id: string, operation: "leave.approve" | "leave.reject", userId: string, assignmentId: string, expectedVersion: number, value2: string, now: number): Effect;
    stageTransitionLeaveEffect(value: string, id: string, company_id: string, note: string, userId: string, now: number, now2: number): Effect;
    stageTransitionLeaveEffect2(key: string, now: number, id: string): Effect;
    stageTransitionLeaveEffect3(note: string, key: string, now: number, id: string): Effect;
    readDecideLeaveCancellationRow(id: string, companyId: string): Promise<LeaveRequest | null>;
    stageDecideLeaveCancellationEffect(value: "approved" | "cancelled", value2: "approved" | "rejected" | "pending_review", key: string, now: number, id: string): Effect;
    stageDecideLeaveCancellationEffect2(value: "pending_approval" | "approved" | "rejected", userId: string, note: string, now: number, id: string): Effect;
    stageBalanceTransitionInsert(value: string, kind: string, multiplier: number, key: string, value2: "Approved cancellation" | "Approved leave" | "Released reservation", actor: string, now: number, id: string, company_id: string, action: "use" | "release" | "reverse"): Effect;
}
export interface LeavePorts<Effect> {
    repository: LeaveRepository<Effect>;
    clock: Clock;
    identifiers: IdentifierSource;
    hash(value: string): Promise<string>;
    prepare(holdingId: string): Promise<void>;
    commands: RosterCommands<Effect>;
    peopleRequestRoute(...args: Tail<Parameters<typeof peopleRequestRoute>>): PortResult<ReturnType<typeof peopleRequestRoute>, Effect>;
    activeStaff(...args: Tail<Parameters<typeof activeStaff>>): PortResult<ReturnType<typeof activeStaff>, Effect>;
    staffGuard(...args: Tail<Parameters<typeof staffGuard>>): PortResult<ReturnType<typeof staffGuard>, Effect>;
    approvalView(...args: Tail<Parameters<typeof approvalView>>): PortResult<ReturnType<typeof approvalView>, Effect>;
    openApproval(...args: Tail<Parameters<typeof openApproval>>): PortResult<ReturnType<typeof openApproval>, Effect>;
    prepareDecision(...args: Tail<Parameters<typeof prepareDecision>>): PortResult<ReturnType<typeof prepareDecision>, Effect>;
    closeApprovals(...args: Tail<Parameters<typeof closeApprovals>>): PortResult<ReturnType<typeof closeApprovals>, Effect>;
    prepareAttachmentBinding(...args: Tail<Parameters<typeof prepareAttachmentBinding>>): PortResult<ReturnType<typeof prepareAttachmentBinding>, Effect>;
    availableLeaveEvidence(...args: Tail<Parameters<typeof availableLeaveEvidence>>): PortResult<ReturnType<typeof availableLeaveEvidence>, Effect>;
    leaveEvidenceManifest(...args: Tail<Parameters<typeof leaveEvidenceManifest>>): PortResult<ReturnType<typeof leaveEvidenceManifest>, Effect>;
    pendingLeaveEvidenceCount(...args: Tail<Parameters<typeof pendingLeaveEvidenceCount>>): PortResult<ReturnType<typeof pendingLeaveEvidenceCount>, Effect>;
}
export function createLeaveApplication<Effect>(ports: LeavePorts<Effect>) {
    const repository = ports.repository;
    async function getLeave(holdingId: string, id: string, context: WorkContext): Promise<LeaveRequest> {
        await ports.prepare(holdingId);
        // Initialize legacy routing before checking named participant access.
        const legacy = await repository.readGetLeaveLegacy(id, context.companyId);
        if (legacy)
            await ports.approvalView("leave", id, context);
        const evidenceReader = hasPermission(context, "leave.evidence.read");
        const row = await repository.readGetLeaveRow(id, context.companyId, context.userId, context.userId, context.userId, context.assignmentId, context.userId, context.assignmentId, evidenceReader);
        if (!row)
            throw new BusinessRuleError(404, "NOT_FOUND", "Leave request not found.");
        const approval = await ports.approvalView("leave", id, context);
        if (["draft", "rejected"].includes(row.state))
            approval.blockedReason = (await ports.peopleRequestRoute(context.companyId, row.created_by)).blockedReason;
        const cancellationApproval = row.cancellation_state === "pending_review" ? await ports.approvalView("leave_cancellation", id, context) : undefined;
        const own = row.created_by === context.userId;
        return {
            ...row,
            branch_id: row.branch_id || null,
            approval,
            cancellationApproval,
            permittedActions: [
                ...(cancellationApproval?.canDecide ? ["approve_cancellation", "reject_cancellation"] : []),
                ...(own && ["draft", "rejected"].includes(row.state)
                    ? ["save", ...(!approval.blockedReason ? ["submit"] : [])]
                    : []),
                ...(own && ["draft", "rejected", "pending_approval", "approved"].includes(row.state) && row.cancellation_state !== "pending_review"
                    ? ["cancel"]
                    : []),
                ...(approval.canDecide && row.state === "pending_approval"
                    ? ["approve", "reject"]
                    : []),
                ...(approval.canOverride && row.state === "pending_approval" ? ["override_approve", "override_reject"] : []),
            ],
        };
    }
    async function createLeave(context: WorkContext, input: z.infer<typeof newLeaveSchema>, key: string) {
        await ports.prepare(context.holdingId);
        const id = `leave-${key}`;
        const previous = await ports.commands.replay(context, key, "leave.create", id, input);
        if (previous)
            return previous;
        const staff = await ports.activeStaff(context.companyId, context.userId), now = ports.clock.now();
        const employee = await repository.readCreateLeaveEmployee(staff.person_id, context.companyId);
        const employeeId = employee?.id ?? `employee-${staff.person_id}`;
        return ports.commands.commit(context, key, "leave.create", input, id, 1, { id, state: "draft", version: 1 }, [
            ports.staffGuard(key, context.companyId, context.userId),
            ...(!employee
                ? [
                    repository.stageCreateLeaveEffect(employeeId, staff.person_id, context.companyId, staff.branch_id ?? "", staff.department_id, staff.legal_name, now),
                ]
                : []),
            repository.stageCreateLeaveEffect2(id, `LR-${key}`, staff.person_id, context.companyId, staff.person_id, staff.engagement_id, context.companyId, staff.branch_id ?? "", staff.department_id, input.startDate, input.days, input.handoverNote, context.userId, now, now),
        ], eventScope(staff));
    }
    function guard(key: string, id: string, version: number, states: string[]) {
        return repository.guard6(key, "leave-state", id, version, states);
    }
    async function submitLeave(holdingId: string, id: string, input: z.infer<typeof leaveDraftSchema>, key: string, context: WorkContext) {
        await ports.prepare(holdingId);
        const operation = input.saveOnly ? "leave.save" : "leave.submit";
        const prior = await ports.commands.replay(context, key, operation, id, input);
        if (prior)
            return prior;
        const row = await getLeave(holdingId, id, context);
        if (row.created_by !== context.userId)
            throw new BusinessRuleError(403, "REQUESTER_REQUIRED", "Only the requester can submit or correct this request.");
        const staff = await ports.activeStaff(context.companyId, context.userId), route = await ports.peopleRequestRoute(context.companyId, context.userId);
        const approval = input.saveOnly
            ? null
            : await ports.openApproval(key, "leave", id, "request", context.userId, null, route, input.expectedVersion + 1);
        const state = input.saveOnly ? "draft" : "pending_approval", now = ports.clock.now();
        return ports.commands.commit(context, key, operation, input, id, input.expectedVersion + 1, { id, state, version: input.expectedVersion + 1 }, [
            guard(key, id, input.expectedVersion, ["draft", "rejected"]),
            ports.staffGuard(key, context.companyId, context.userId),
            repository.guard7(key, "placement", staff.engagement_id, staff.engagement_version),
            ...(approval?.statements ?? []),
            repository.stageSubmitLeaveEffect(input.startDate, input.days, input.handoverNote, state, staff.person_id, staff.engagement_id, staff.branch_id ?? "", staff.department_id, context.userId, key, now, id),
        ], eventScope(staff));
    }
    async function decisionEvidencePolicy(row: LeaveRequest): Promise<LeaveEvidencePolicy> {
        const snapshot = parseLeaveEvidencePolicy(row.evidence_policy_snapshot);
        if (snapshot)
            return snapshot;
        const rule = row.policy_version_id && row.leave_type_id
            ? await repository.readDecisionEvidencePolicyRule(row.policy_version_id, row.leave_type_id)
            : null;
        const minimumUnits = rule?.evidence_min_units === null || rule?.evidence_min_units === undefined
            ? null
            : Number(rule.evidence_min_units), units = Number(row.calculated_units ?? row.days), minimumCount = Number(rule?.evidence_min_count ?? 0);
        return {
            required: Boolean(rule?.evidence_purpose && minimumUnits !== null && units >= minimumUnits && minimumCount > 0),
            minimumUnits,
            purposeKey: rule?.evidence_purpose ?? null,
            minimumCount,
            dueStage: rule?.evidence_due_stage ?? "before_final_approval",
            graceDays: Number(rule?.evidence_grace_days ?? 0),
            privacy: rule?.evidence_privacy ?? "requester_approvers_hr",
            note: rule?.evidence_note ?? "",
        };
    }
    async function decide(context: WorkContext, id: string, input: z.infer<typeof leaveDecisionSchema>, key: string, approve: boolean) {
        await ports.prepare(context.holdingId);
        const operation = approve ? "leave.approve" : "leave.reject";
        const prior = await ports.commands.replay(context, key, operation, id, input);
        if (prior)
            return prior;
        const row = input.override ? await repository.readDecideRow(id, context.companyId) : await getLeave(context.holdingId, id, context), checked = row ?? (() => { throw new BusinessRuleError(404, "NOT_FOUND", "Leave request not found."); })(), decision = await ports.prepareDecision(context, key, "leave", id, approve, input.note, input.expectedVersion, input.override);
        const state = !approve
            ? "rejected"
            : decision.final
                ? "approved"
                : "pending_approval", now = ports.clock.now();
        const evidencePolicy = await decisionEvidencePolicy(checked);
        const observedEvidence = await ports.leaveEvidenceManifest(id);
        if (approve && decision.final) {
            const pendingEvidence = await ports.pendingLeaveEvidenceCount(id);
            if (pendingEvidence)
                throw new BusinessRuleError(409, "LEAVE_EVIDENCE_SCANNING", "Wait for every leave evidence file to finish security scanning before final approval.");
            if (evidencePolicy.required && evidencePolicy.purposeKey) {
                const available = await ports.availableLeaveEvidence(id, evidencePolicy);
                if (available.count < evidencePolicy.minimumCount)
                    throw new BusinessRuleError(409, "LEAVE_EVIDENCE_REQUIRED", `Final approval requires ${evidencePolicy.minimumCount} clean ${evidencePolicy.purposeKey.replaceAll("_", " ")} file${evidencePolicy.minimumCount === 1 ? "" : "s"}.`);
            }
        }
        const binding = observedEvidence.draftSetIds.length
            ? await ports.prepareAttachmentBinding(context, {
                attachmentSetIds: observedEvidence.draftSetIds,
                bindingContract: {
                    targetType: "leave_request",
                    bindingType: "leave_approval",
                    contexts: ["medical_certificate", "leave_supporting_photo"].flatMap(purposeKey => (["record", "action"] as const).map(componentKind => ({ purposeKey, componentKind, componentRef: `leave-evidence:${purposeKey}` }))),
                },
                targetId: id,
                expectedTargetVersion: input.expectedVersion,
                commandId: key,
                bindingId: decision.instance.id,
            })
            : null;
        // The query captures the complete manifest visible to this reviewer. Binding
        // freezes only newly staged sets; already-bound files stay in every later
        // decision manifest without being rebound.
        const evidenceManifest = observedEvidence.manifest;
        const evidenceSnapshotGuards = [
            repository.guard11(key, "leave-evidence-canonical-count", id, checked.company_id, observedEvidence.canonicalManifest.length),
            ...observedEvidence.canonicalManifest.map((file, index) => repository.guard12(key, `leave-evidence-manifest-${index}`, id, checked.company_id, file.linkId, file.sourceId, file.purposeKey, file.checksumSha256, file.mediaType, file.bytes)),
            ...(observedEvidence.legacyIncluded
                ? [
                    repository.guard13(key, "leave-evidence-legacy-count", id, checked.company_id, observedEvidence.legacyManifest.length),
                    ...observedEvidence.legacyManifest.map((file, index) => repository.guard14(key, `leave-evidence-legacy-${index}`, file.sourceId, id, checked.company_id, file.purposeKey, file.checksumSha256, file.mediaType, file.bytes)),
                ]
                : []),
            ...(approve && decision.final
                ? [
                    repository.guard15(key, "leave-evidence-no-pending", id, checked.company_id),
                    ...(evidencePolicy.required && evidencePolicy.purposeKey
                        ? [
                            repository.guard16(key, "leave-evidence-required-count", observedEvidence.legacyIncluded
                                ? [
                                    id,
                                    checked.company_id,
                                    evidencePolicy.purposeKey,
                                    id,
                                    checked.company_id,
                                    evidencePolicy.purposeKey,
                                    evidencePolicy.minimumCount,
                                ]
                                : [
                                    id,
                                    checked.company_id,
                                    evidencePolicy.purposeKey,
                                    evidencePolicy.minimumCount,
                                ], observedEvidence.legacyIncluded),
                        ]
                        : []),
                ]
                : []),
        ];
        const evidenceManifestStep = evidenceManifest.length
            ? [
                repository.stageDecideEvidenceManifestStep(`leave-evidence-${key}`, id, checked.company_id, decision.instance.id, key, input.expectedVersion, checked.policy_version_id ?? null, context.userId, JSON.stringify({
                    policy: evidencePolicy,
                    files: evidenceManifest,
                }), now),
            ]
            : [];
        return ports.commands.commit(context, key, operation, input, id, input.expectedVersion + 1, { id, state, version: input.expectedVersion + 1 }, [
            // File mutations for Leave take this same row lock and reauthorize after
            // acquiring it. That makes the reviewed manifest and the decision one
            // serializable boundary without expanding the lock to unrelated records.
            repository.stageDecideEffect(id, checked.company_id),
            guard(key, id, input.expectedVersion, ["pending_approval"]),
            ...evidenceSnapshotGuards,
            ...(binding?.guards ?? []),
            ...decision.statements,
            ...(binding?.steps ?? []),
            repository.stageDecideEffect2(state, input.note, context.userId, binding?.manifest.length ? 1 : 0, key, now, id),
            ...((decision.final || !approve) ? balanceTransition(checked, key, context.userId, approve && decision.final ? "use" : "release") : []),
            ...evidenceManifestStep,
            repository.stageDecideEffect3(ports.identifiers.next(), key, id, operation, context.userId, context.assignmentId, input.expectedVersion, await ports.hash(JSON.stringify(input)), now),
        ], eventScope(checked));
    }
    async function approveLeave(_holdingId: string, id: string, input: z.infer<typeof leaveDecisionSchema>, key: string, context: WorkContext) {
        return decide(context, id, input, key, true);
    }
    async function transitionLeave(context: WorkContext, id: string, input: z.infer<typeof leaveLifecycleSchema>, key: string) {
        if (input.action === "reject")
            return decide(context, id, input, key, false);
        await ports.prepare(context.holdingId);
        const prior = await ports.commands.replay(context, key, "leave.cancellation", id, input);
        if (prior)
            return prior;
        const row = await getLeave(context.holdingId, id, context);
        if (row.created_by !== context.userId)
            throw new BusinessRuleError(403, "REQUESTER_REQUIRED", "Only the requester can cancel this request.");
        if (row.state === "approved") {
            const route = await ports.peopleRequestRoute(context.companyId, context.userId), now = ports.clock.now();
            if (row.cancellation_state === "pending_review")
                throw new BusinessRuleError(409, "CANCELLATION_PENDING", "Cancellation is already in review.");
            if (!input.note.trim())
                throw new BusinessRuleError(422, "REASON_REQUIRED", "Explain why the approved leave should be cancelled.");
            if (route.blockedReason)
                throw new BusinessRuleError(409, "CANCELLATION_ROUTE_BLOCKED", route.blockedReason);
            const approval = await ports.openApproval(key, "leave_cancellation", id, "request", context.userId, null, route, input.expectedVersion + 1);
            return ports.commands.commit(context, key, "leave.cancellation", input, id, input.expectedVersion + 1, { id, state: "approved", cancellationState: "pending_review", version: input.expectedVersion + 1 }, [
                guard(key, id, input.expectedVersion, ["approved"]),
                ...approval.statements,
                repository.stageTransitionLeaveEffect(`leave-cancel-${key}`, id, row.company_id, input.note, context.userId, now, now),
                repository.stageTransitionLeaveEffect2(key, now, id),
            ], eventScope(row));
        }
        return ports.commands.commit(context, key, "leave.cancellation", input, id, input.expectedVersion + 1, { id, state: "cancelled", version: input.expectedVersion + 1 }, [
            guard(key, id, input.expectedVersion, [
                "draft",
                "rejected",
                "pending_approval",
            ]),
            ...ports.closeApprovals(key, "leave", id, context.userId, "cancel", input.note, input.expectedVersion),
            repository.stageTransitionLeaveEffect3(input.note, key, ports.clock.now(), id),
            ...(row.state === "pending_approval" ? balanceTransition(row, key, context.userId, "release") : []),
        ], eventScope(row));
    }
    async function decideLeaveCancellation(context: WorkContext, id: string, input: z.infer<typeof leaveDecisionSchema>, key: string, approve: boolean) {
        await ports.prepare(context.holdingId);
        const operation = approve ? "leave.cancellation.approve" : "leave.cancellation.reject";
        const prior = await ports.commands.replay(context, key, operation, id, input);
        if (prior)
            return prior;
        const row = await repository.readDecideLeaveCancellationRow(id, context.companyId);
        if (!row)
            throw new BusinessRuleError(404, "NOT_FOUND", "Pending leave cancellation review not found.");
        const decision = await ports.prepareDecision(context, key, "leave_cancellation", id, approve, input.note, input.expectedVersion, input.override);
        const final = approve && decision.final, now = ports.clock.now();
        return ports.commands.commit(context, key, approve ? "leave.cancellation.approve" : "leave.cancellation.reject", input, id, input.expectedVersion + 1, { id, state: final ? "cancelled" : "approved", cancellationState: final ? "approved" : approve ? "pending_review" : "rejected", version: input.expectedVersion + 1 }, [
            guard(key, id, input.expectedVersion, ["approved"]), ...decision.statements,
            repository.stageDecideLeaveCancellationEffect(final ? "cancelled" : "approved", final ? "approved" : approve ? "pending_review" : "rejected", key, now, id),
            repository.stageDecideLeaveCancellationEffect2(final ? "approved" : approve ? "pending_approval" : "rejected", context.userId, input.note, now, id),
            ...(final ? balanceTransition(row, key, context.userId, "reverse") : []),
        ], eventScope(row));
    }
    function balanceTransition(row: LeaveRequest, key: string, actor: string, action: "use" | "release" | "reverse") {
        const insert = (kind: string, multiplier: number) => repository.stageBalanceTransitionInsert(`${key}:${kind}`, kind, multiplier, key, action === 'reverse' ? 'Approved cancellation' : action === 'use' ? 'Approved leave' : 'Released reservation', actor, ports.clock.now(), row.id, row.company_id, action);
        // Usage must read the reservation before its release is inserted.
        return action === 'use' ? [insert('usage', 1), insert('reservation_release', -1)] : [insert(action === 'reverse' ? 'usage_reversal' : 'reservation_release', -1)];
    }
    return { getLeave, createLeave, submitLeave, approveLeave, transitionLeave, decideLeaveCancellation };
}
