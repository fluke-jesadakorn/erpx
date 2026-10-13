import { z } from 'zod';
import { overtimeDetailsSchema, overtimeCommandSchema, overtimeMinutes, overtimeLabels, type OvertimeDetails } from "@/lib/people_payroll/overtime";
import type { WorkItem } from "@/lib/work_projection/work-item";
import type { activeStaff, notification, requireCompanyAuthority } from "@/server/modules/operations/public/staff-policy";
import type { peopleRequestRoute } from "@/server/modules/organization/public/department-authority";
import type { approvalView, openApproval, prepareDecision, closeApprovals } from "@/server/modules/workflow/public/approvals";
import type { prepareAttachmentBinding } from "@/server/modules/files/public/attachments/service";
import type { WorkContext } from '../../identity_access/public/authorization';
import { BusinessRuleError, type Clock } from '../../shared';
import type { RosterCommands } from './people-roster';
import { fail, type OvertimeRow, detailsOf } from '../domain/overtime';
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
export interface OvertimeRepository<Effect> {
    readOvertimeSetupEligibility(companyId: string, engagement_id: string): Promise<{
        eligible: boolean;
    } | null>;
    readPreviewOvertimeTime(startLocal: string, timezone: string, endLocal: string, timezone2: string, startLocal2: string, timezone3: string, timezone4: string, endLocal2: string, timezone5: string, timezone6: string): Promise<{
        starts_at: number;
        ends_at: number;
        local_start: string;
        local_end: string;
    } | null>;
    readPreviewOvertimeOverlap(companyId: string, engagement_id: string, excludeId: string | null, ends_at: number, starts_at: number): Promise<Record<string, unknown> | null>;
    readPreviewOvertimeEffect(rosterShiftId: string, companyId: string, engagement_id: string): Promise<Record<string, unknown> | null>;
    readPreviewOvertimeReviewers(companyId: string, userId: string): Promise<{
        legal_name: string;
    } | null>;
    readGetOvertimeRow(id: string, companyId: string, userId: string, userId2: string, assignmentId: string): Promise<OvertimeRow | null>;
    readGetOvertimeHandoff(id: string, companyId: string): Promise<Record<string, unknown> | null>;
    readListOvertimeRows(companyId: string, userId: string, userId2: string, assignmentId: string): Promise<{
        results: ({
            id: string;
        })[];
    }>;
    stageCreateOvertimeEffect(id: string, companyId: string, engagement_id: string, person_id: string, branch_id: string | null, department_id: string | null, userId: string, mode: "advance" | "retrospective", timezone: string, startLocal: string, endLocal: string, startsAt: number, endsAt: number, breakMinutes: number, minutes: number, reason: string, lateExplanation: string, rosterShiftId: string | null, now: number, now2: number): Effect;
    readCommandOvertimeEffect(id: string, companyId: string): Promise<Record<string, unknown> | null>;
    readCommandOvertimeRecord(id: string, companyId: string): Promise<OvertimeRow | null>;
    readCommandOvertimeSets(companyId: string, attachmentSetIds: string[], placeholders: string): Promise<{
        results: ({
            id: string;
            purpose_key: string;
            component_kind: string;
        })[];
    }>;
    stageCommandOvertimeEffect(startLocal: string, endLocal: string, timezone: string, startsAt: number, endsAt: number, breakMinutes: number, minutes: number, reason: string, lateExplanation: string, rosterShiftId: string | null, id: string): Effect;
    stageCommandOvertimeEffect2(value: string, id: string, authorization: boolean): Effect;
    readCommandOvertimeConflict(companyId: string, engagement_id: string, value: string, value2: string): Promise<Record<string, unknown> | null>;
    stageCommandOvertimeEffect3(value: string, id: string, companyId: string, engagement_id: string, version: number, value2: string, minutes: number, timezone: string, userId: string, now: number): Effect;
    stageCommandOvertimeEffect4(note: string, id: string): Effect;
    stageCommandOvertimeEffect5(state: string, version: number, now: number, id: string): Effect;
    readSetOvertimeEligibilityEffect(engagementId: string, companyId: string): Promise<Record<string, unknown> | null>;
    readSetOvertimeEligibilityCurrent(companyId: string, engagementId: string): Promise<{
        version: number;
    } | null>;
    stageSetOvertimeEligibilityEffect(topLevel: boolean, engagementId: string, companyId: string): Effect;
    stageSetOvertimeEligibilityEffect2(companyId: string, engagementId: string, eligible: boolean, reason: string, userId: string, now: number, version: number): Effect;
}
export interface OvertimePorts<Effect> {
    repository: OvertimeRepository<Effect>;
    clock: Clock;
    prepare(holdingId: string): Promise<void>;
    commands: RosterCommands<Effect>;
    activeStaff(...args: Tail<Parameters<typeof activeStaff>>): PortResult<ReturnType<typeof activeStaff>, Effect>;
    notification(...args: Tail<Parameters<typeof notification>>): PortResult<ReturnType<typeof notification>, Effect>;
    requireCompanyAuthority(...args: Tail<Parameters<typeof requireCompanyAuthority>>): PortResult<ReturnType<typeof requireCompanyAuthority>, Effect>;
    peopleRequestRoute(...args: Tail<Parameters<typeof peopleRequestRoute>>): PortResult<ReturnType<typeof peopleRequestRoute>, Effect>;
    approvalView(...args: Tail<Parameters<typeof approvalView>>): PortResult<ReturnType<typeof approvalView>, Effect>;
    openApproval(...args: Tail<Parameters<typeof openApproval>>): PortResult<ReturnType<typeof openApproval>, Effect>;
    prepareDecision(...args: Tail<Parameters<typeof prepareDecision>>): PortResult<ReturnType<typeof prepareDecision>, Effect>;
    closeApprovals(...args: Tail<Parameters<typeof closeApprovals>>): PortResult<ReturnType<typeof closeApprovals>, Effect>;
    prepareAttachmentBinding(...args: Tail<Parameters<typeof prepareAttachmentBinding>>): PortResult<ReturnType<typeof prepareAttachmentBinding>, Effect>;
    transaction<Result>(run: (ports: OvertimePorts<Effect>) => Promise<Result>): Promise<Result>;
}
export function createOvertimeApplication<Effect>(ports: OvertimePorts<Effect>) {
    const repository = ports.repository;
    async function overtimeSetup(actor: WorkContext) {
        const staff = await ports.activeStaff(actor.companyId, actor.userId);
        const eligibility = await repository.readOvertimeSetupEligibility(actor.companyId, staff.engagement_id);
        const route = await ports.peopleRequestRoute(actor.companyId, actor.userId);
        const blockers = [...(eligibility?.eligible === false || staff.worker_type === 'contractor' && !eligibility?.eligible ? ["Ask HR to configure explicit overtime eligibility for your engagement in People settings."] : []), ...(route.blockedReason ? [route.blockedReason] : [])];
        return { staff, route, blockers };
    }
    async function previewOvertime(actor: WorkContext, raw: OvertimeDetails, actual = false, excludeId: string | null = null) {
        const input = overtimeDetailsSchema.parse(raw);
        await ports.prepare(actor.holdingId);
        const { staff, route, blockers } = await overtimeSetup(actor);
        const time = await repository.readPreviewOvertimeTime(input.startLocal, input.timezone, input.endLocal, input.timezone, input.startLocal, input.timezone, input.timezone, input.endLocal, input.timezone, input.timezone);
        if (!time || time.local_start !== input.startLocal || time.local_end !== input.endLocal)
            fail("OVERTIME_TIME", "This local time does not exist in the selected timezone.", 422);
        let minutes: number;
        try {
            minutes = overtimeMinutes(time!.starts_at, time!.ends_at, input.breakMinutes);
        }
        catch (e) {
            return fail("OVERTIME_TIME", (e as Error).message, 422);
        }
        if (input.startLocal.slice(0, 10) < staff.start_date || staff.end_date && input.endLocal.slice(0, 10) > staff.end_date)
            blockers.push("The interval must fall within your engagement dates.");
        if (actual || input.mode === 'retrospective') {
            if (time!.ends_at > ports.clock.now())
                blockers.push("Actual hours cannot be in the future.");
            if (input.mode === 'retrospective' && !input.lateExplanation.trim())
                blockers.push("Explain why these hours are being submitted retrospectively.");
        }
        else if (time!.starts_at < ports.clock.now())
            blockers.push("Use a retrospective claim for work that has already started.");
        const overlap = await repository.readPreviewOvertimeOverlap(actor.companyId, staff.engagement_id, excludeId, time!.ends_at, time!.starts_at);
        if (overlap)
            blockers.push("Another overtime request overlaps this interval.");
        if (input.rosterShiftId && !await repository.readPreviewOvertimeEffect(input.rosterShiftId, actor.companyId, staff.engagement_id))
            blockers.push("The roster reference must be one of your published assigned shifts.");
        const reviewers = await Promise.all(route.steps.map(async (step) => ({ stage: step.authority === 'hr_admin' ? 'Independent HR approval' : 'Reporting manager', userId: step.userId, name: (await repository.readPreviewOvertimeReviewers(actor.companyId, step.userId))?.legal_name ?? 'Assigned reviewer' })));
        return { ...input, startsAt: time!.starts_at, endsAt: time!.ends_at, minutes, hours: minutes / 60, blockers, canSubmit: !blockers.length, reviewers };
    }
    async function getOvertime(actor: WorkContext, id: string) {
        await ports.prepare(actor.holdingId);
        const row = await repository.readGetOvertimeRow(id, actor.companyId, actor.userId, actor.userId, actor.assignmentId);
        if (!row)
            return fail('NOT_FOUND', 'Overtime request not found.', 404);
        const approval = await ports.approvalView('overtime', id, actor), own = row.created_by === actor.userId;
        const actions = [...(own && ['draft', 'rejected'].includes(row.state) ? ['save', 'submit'] : []), ...(own && ['authorized', 'returned'].includes(row.state) ? ['save', 'actuals'] : []), ...(own && !['approved', 'withdrawn'].includes(row.state) ? ['withdraw'] : []), ...(approval.canDecide ? row.state === 'pending_authorization' ? ['authorize', 'reject'] : row.state === 'pending_verification' ? ['verify', 'return'] : row.state === 'pending_hr' ? ['approve', 'return'] : [] : [])];
        const handoff = await repository.readGetOvertimeHandoff(id, actor.companyId);
        return { ...row, approval, permittedActions: actions, handoff, details: detailsOf(row) };
    }
    async function listOvertime(actor: WorkContext) {
        await ports.prepare(actor.holdingId);
        const rows = (await repository.readListOvertimeRows(actor.companyId, actor.userId, actor.userId, actor.assignmentId)).results;
        return Promise.all(rows.map(r => getOvertime(actor, r.id)));
    }
    async function createOvertime(actor: WorkContext, raw: OvertimeDetails, key: string) {
        const input = overtimeDetailsSchema.parse(raw);
        await ports.prepare(actor.holdingId);
        const id = `ot-${key}`;
        return ports.transaction(context => createOvertimeApplication(context).applyCreate(actor, input, key, id));
    }
    async function commandOvertime(actor: WorkContext, id: string, raw: z.infer<typeof overtimeCommandSchema>, key: string) {
        const input = overtimeCommandSchema.parse(raw);
        await ports.prepare(actor.holdingId);
        const operation = `overtime.${input.action}`;
        return ports.transaction(context => createOvertimeApplication(context).applyCommand(actor, id, input, key, operation));
    }
    async function overtimeWork(actor: WorkContext): Promise<WorkItem[]> {
        return (await listOvertime(actor)).map(row => ({ id: row.id, type: 'overtime', title: 'Overtime request', detail: `${row.start_local.slice(0, 10)} · ${(row.minutes / 60).toFixed(2)} hours`, href: `/overtime/${row.id}`, status: row.state, updatedAt: row.updated_at, queue: row.permittedActions.some(a => a !== 'withdraw') ? 'todo' : ['approved', 'withdrawn', 'rejected'].includes(row.state) ? 'completed' : 'waiting', canAct: row.permittedActions.some(a => a !== 'withdraw'), nextAction: overtimeLabels[row.state], responsibleQueue: row.approval.currentApprover?.approver_name ?? 'Requester', blocker: row.approval.blockedReason, relationships: [...(row.created_by === actor.userId ? ['requested' as const] : []), ...(row.approval.canDecide ? ['approval' as const] : [])] }));
    }
    async function setOvertimeEligibility(actor: WorkContext, raw: unknown, key: string) {
        const input = z.object({ engagementId: z.string().min(1), expectedVersion: z.number().int().nonnegative().default(0), eligible: z.boolean(), topLevel: z.boolean().default(false), reason: z.string().trim().min(3).max(500) }).parse(raw);
        await ports.prepare(actor.holdingId);
        return ports.transaction(context => createOvertimeApplication(context).applyEligibility(actor, input, key));
    }
    async function applyCreate(actor: WorkContext, input: OvertimeDetails, key: string, id: string) {
        const prior = await ports.commands.replay(actor, key, 'overtime.create', id, input);
        if (prior)
            return prior;
        const p = await previewOvertime(actor, input, false, null), { staff } = await overtimeSetup(actor), now = ports.clock.now();
        return ports.commands.commit(actor, key, 'overtime.create', input, id, 1, { id, version: 1, state: 'draft' }, [
            repository.stageCreateOvertimeEffect(id, actor.companyId, staff.engagement_id, staff.person_id, staff.branch_id, staff.department_id, actor.userId, input.mode, input.timezone, input.startLocal, input.endLocal, p.startsAt, p.endsAt, input.breakMinutes, p.minutes, input.reason, input.lateExplanation, input.rosterShiftId, now, now)
        ], { companyId: actor.companyId, branchId: staff.branch_id, departmentId: staff.department_id });
    }
    async function applyCommand(actor: WorkContext, id: string, input: z.infer<typeof overtimeCommandSchema>, key: string, operation: string) {
        const prior = await ports.commands.replay(actor, key, operation, id, input);
        if (prior)
            return prior;
        await repository.readCommandOvertimeEffect(id, actor.companyId);
        const row = input.override ? await (async () => {
            const record = await repository.readCommandOvertimeRecord(id, actor.companyId);
            if (!record)
                throw new BusinessRuleError(404, "NOT_FOUND", "Overtime request not found.");
            const approval = await ports.approvalView("overtime", id, actor);
            return { ...record, approval, permittedActions: approval.canOverride ? (record.state === "pending_authorization" ? ["authorize", "reject"] : record.state === "pending_verification" ? ["verify", "return"] : record.state === "pending_hr" ? ["approve", "return"] : []) : [], handoff: null, details: detailsOf(record) };
        })() : await getOvertime(actor, id);
        if (row.version !== input.expectedVersion)
            fail('STALE_VERSION', 'This request changed. Reload to review the current version; your entered values are retained.');
        if (!row.permittedActions.includes(input.action))
            fail('OVERTIME_ACTION', 'This action is not available to you at the current stage.', 403);
        const allowedAttachmentPurposes = input.action === "submit"
            ? (row.mode === "advance" ? ["planned_evidence"] : ["actual_evidence", "exception_evidence"])
            : input.action === "actuals" ? ["actual_evidence", "exception_evidence"] : [];
        if (input.attachmentSetIds.length && !allowedAttachmentPurposes.length)
            fail("OVERTIME_ATTACHMENT_ACTION", "Evidence can be bound only when planned or actual hours are submitted.", 422);
        if (input.attachmentSetIds.length) {
            const placeholders = input.attachmentSetIds.map(() => "?").join(","), sets = (await repository.readCommandOvertimeSets(actor.companyId, input.attachmentSetIds, placeholders)).results;
            if (sets.length !== new Set(input.attachmentSetIds).size || sets.some(set => set.component_kind !== "action" || !allowedAttachmentPurposes.includes(set.purpose_key)))
                fail("OVERTIME_ATTACHMENT_PURPOSE", "Choose only the evidence purpose allowed for this overtime submission.", 422);
        }
        const attachmentBinding = input.attachmentSetIds.length ? await ports.prepareAttachmentBinding(actor, { attachmentSetIds: input.attachmentSetIds, bindingContract: { targetType: "overtime_request", bindingType: input.action === "submit" ? "overtime_submission" : "overtime_actuals", contexts: allowedAttachmentPurposes.map(purposeKey => ({ purposeKey, componentKind: "action" as const, componentRef: input.action === "actuals" ? "actual-hours" : "request-submission" })) }, targetId: id, expectedTargetVersion: input.expectedVersion, commandId: key, bindingId: `${id}:${input.action}:${input.expectedVersion + 1}` }) : null;
        const version = row.version + 1, now = ports.clock.now(), statements = [...(attachmentBinding?.guards ?? [])];
        let state = row.state;
        if (['save', 'submit', 'actuals'].includes(input.action)) {
            const details = input.details ?? row.details;
            if (details.mode !== row.mode)
                fail('OVERTIME_MODE', 'Start a new request to change between advance and retrospective.');
            const actual = input.action === 'actuals' || ['authorized', 'returned'].includes(row.state);
            const p = await previewOvertime(actor, details, actual, id), setup = await overtimeSetup(actor);
            if (setup.staff.engagement_id !== row.engagement_id)
                fail('ENGAGEMENT_CHANGED', 'This request belongs to a previous engagement. Start a new request.');
            if (input.action !== 'save' && p.blockers.length)
                fail('OVERTIME_BLOCKED', p.blockers.join(' '));
            statements.push(repository.stageCommandOvertimeEffect(details.startLocal, details.endLocal, details.timezone, p.startsAt, p.endsAt, details.breakMinutes, p.minutes, details.reason, details.lateExplanation, details.rosterShiftId, id));
            if (input.action !== 'save') {
                const authorization = row.mode === 'advance' && !actual;
                state = authorization ? 'pending_authorization' : 'pending_verification';
                const route = { ...setup.route, steps: authorization ? setup.route.steps.filter(s => s.stage === "department_approval") : setup.route.steps };
                statements.push(...(await ports.openApproval(key, 'overtime', id, authorization ? 'start' : 'request', row.created_by, null, route, version)).statements);
                statements.push(repository.stageCommandOvertimeEffect2(JSON.stringify(p), id, authorization));
            }
        }
        else if (input.action === 'withdraw') {
            state = 'withdrawn';
            statements.push(...ports.closeApprovals(key, 'overtime', id, actor.userId, 'cancel', input.note, row.version));
        }
        else {
            const approve = !['return', 'reject'].includes(input.action), decision = await ports.prepareDecision(actor, key, 'overtime', id, approve, input.note, row.version, input.override);
            statements.push(...decision.statements);
            state = !approve ? (row.state === 'pending_authorization' ? 'rejected' : 'returned')
                : row.state === 'pending_authorization' ? (decision.final ? 'authorized' : 'pending_authorization')
                    : decision.final ? 'approved' : decision.next?.stage === 'hr_approval' ? 'pending_hr' : 'pending_verification';
            if (state === 'approved') {
                if (!decision.final || !row.actuals_snapshot || row.ends_at > now)
                    fail('OVERTIME_REVIEW', 'Complete independent actual-hours reviews before handoff.');
                const conflict = await repository.readCommandOvertimeConflict(actor.companyId, row.engagement_id, row.start_local.slice(0, 10), row.end_local.slice(0, 10));
                if (conflict)
                    fail('OVERTIME_RECONCILIATION', 'Manual overtime exists on these dates. Ask Payroll to reconcile that entry before final approval; an approved time entry cannot be overwritten.');
                statements.push(repository.stageCommandOvertimeEffect3(`handoff-${id}`, id, actor.companyId, row.engagement_id, version, row.start_local.slice(0, 10), row.minutes, row.timezone, actor.userId, now));
            }
            statements.push(repository.stageCommandOvertimeEffect4(input.note, id));
        }
        statements.push(repository.stageCommandOvertimeEffect5(state, version, now, id), ...(attachmentBinding?.steps ?? []), ports.notification(row, row.created_by, 'overtime', id, version, overtimeLabels[state]));
        return ports.commands.commit(actor, key, operation, input, id, version, { id, state, version }, statements, { companyId: row.company_id, branchId: row.branch_id, departmentId: row.department_id });
    }
    async function applyEligibility(actor: WorkContext, input: {
        engagementId: string;
        expectedVersion: number;
        eligible: boolean;
        topLevel: boolean;
        reason: string;
    }, key: string) {
        const prior = await ports.commands.replay(actor, key, 'overtime.eligibility', input.engagementId, input);
        if (prior)
            return prior;
        await ports.requireCompanyAuthority(actor, 'leave.policy.manage');
        if (!await repository.readSetOvertimeEligibilityEffect(input.engagementId, actor.companyId))
            fail('NOT_FOUND', 'Engagement not found.', 404);
        const current = await repository.readSetOvertimeEligibilityCurrent(actor.companyId, input.engagementId);
        if ((current?.version ?? 0) !== input.expectedVersion)
            fail('STALE_VERSION', 'Eligibility changed. Reload settings and review before saving.');
        const version = input.expectedVersion + 1;
        return ports.commands.commit(actor, key, 'overtime.eligibility', input, input.engagementId, version, { id: input.engagementId, version }, [
            repository.stageSetOvertimeEligibilityEffect(input.topLevel, input.engagementId, actor.companyId),
            repository.stageSetOvertimeEligibilityEffect2(actor.companyId, input.engagementId, input.eligible, input.reason, actor.userId, ports.clock.now(), version)
        ]);
    }
    return { overtimeSetup, previewOvertime, getOvertime, listOvertime, createOvertime, commandOvertime, overtimeWork, setOvertimeEligibility, applyCreate, applyCommand, applyEligibility };
}
export type OvertimeDetail = Awaited<ReturnType<ReturnType<typeof createOvertimeApplication>['getOvertime']>>;
