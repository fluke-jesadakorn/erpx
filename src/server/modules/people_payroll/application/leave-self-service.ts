import { z } from 'zod';
import { calculateLeaveDays, dateRange, addLeaveDays, leaveCreateSchema, leavePreviewSchema, type LeaveCalculationDay } from "@/lib/people_payroll/leave-self-service";
import type { activeStaff, requireCompanyAuthority } from "@/server/modules/operations/public/staff-policy";
import type { openApproval } from "@/server/modules/workflow/public/approvals";
import type { peopleRequestRoute } from "@/server/modules/organization/public/department-authority";
import type { availableLeaveEvidence } from "../public/leave-evidence";
import type { WorkContext } from '../../identity_access/public/authorization';
import { BusinessRuleError, type Clock, type IdentifierSource } from '../../shared';
import type { RosterCommands } from './people-roster';
import { type Policy, type LeaveType, evidencePolicy, workweek, leaveListPageSchema, leaveCursorSchema, type LeaveListRow, leaveMonthSchema, type LeaveCalendarMonth, leaveEditSchema, leaveAdminSchema } from '../domain/leave-self-service';
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
export interface LeaveSelfServiceRepository<Effect> {
    readPolicyForMatches(companyId: string, engagement_id: string, date: string, date2: string, date3: string, date4: string): Promise<(Policy)[]>;
    readBalanceEffect(companyId: string, engagementId: string, leaveTypeId: string, year: number): Promise<{
        available: number;
        reserved: number;
        used: number;
        accrued: number;
    } | null>;
    readPreviewLeaveType(policy_version_id: string, leaveTypeId: string, companyId: string, startDate: string, endDate: string): Promise<LeaveType | null>;
    readPreviewLeaveHolidays(holiday_calendar_id: string, startDate: string, endDate: string): Promise<({
        holiday_date: string;
    })[]>;
    readPreviewLeavePublished(companyId: string, department_id: string | null, endDate: string, startDate: string): Promise<({
        week_start: string;
    })[]>;
    readPreviewLeaveRosterDates(companyId: string, engagement_id: string, startDate: string, endDate: string): Promise<({
        local_date: string;
    })[]>;
    readPreviewLeaveCandidates(companyId: string, engagement_id: string, excludeId: string | null, startDate: string, endDate: string): Promise<({
        id: string;
        number: string;
        state: string;
        leave_date: string;
        segment: string;
    })[]>;
    readPreviewLeaveLegacy(companyId: string, engagement_id: string, excludeId: string | null, endDate: string, startDate: string): Promise<({
        id: string;
        number: string;
        state: string;
    })[]>;
    readPreviewLeaveApprovers(companyId: string, value: string): Promise<({
        user_id: string;
        legal_name: string;
    })[]>;
    readLeaveSummaryTypes(policy_version_id: string): Promise<(LeaveType)[]>;
    readLeaveSummaryRequests(companyId: string, engagement_id: string): Promise<(Record<string, unknown>)[]>;
    readLeaveSummaryHolidays(holiday_calendar_id: string): Promise<({
        holiday_date: string;
        name: string;
    })[]>;
    readListOwnLeavePagePage(args: unknown[], value: number, clauses: string[]): Promise<(LeaveListRow)[]>;
    readLeaveCalendarMonthRequestsholidays(companyId: string, engagement_id: string, userId: string, end: string, start: string): Promise<(LeaveListRow)[]>;
    readLeaveCalendarMonthRequestsholidays2(companyId: string, engagement_id: string, end: string, start: string, end2: string, start2: string, start3: string, end3: string): Promise<({
        holiday_date: string;
        name: string;
    })[]>;
    readSaveSelfServiceLeaveEffect(engagement_id: string): Promise<Record<string, unknown> | null>;
    readSaveSelfServiceLeaveRow(id: string, companyId: string, userId: string): Promise<{
        version: number;
        state: string;
        engagement_id: string;
    } | null>;
    readSaveSelfServiceLeaveEmployee(person_id: string, companyId: string): Promise<{
        id: string;
    } | null>;
    stageSaveSelfServiceLeaveEffect(employeeId: string, person_id: string, companyId: string, value: string, department_id: string | null, legal_name: string, now: number): Effect;
    stageSaveSelfServiceLeaveEffect2(id: string, value: string, employeeId: string, person_id: string, engagement_id: string, companyId: string, value2: string, department_id: string | null, startDate: string, value3: number, handoverNote: string, userId: string, now: number, now2: number): Effect;
    stageSaveSelfServiceLeaveEffect3(startDate: string, value: number, handoverNote: string, leaveTypeId: string, endDate: string, startSegment: "full" | "first_half" | "second_half", endSegment: "full" | "first_half" | "second_half", id: string, units: number, value2: string, value3: string, evidenceDueAt: number | null, state: "draft" | "pending_approval", now: number, state2: "draft" | "pending_approval", version: number, userId: string, key: string, now2: number, id2: string): Effect;
    stageSaveSelfServiceLeaveEffect4(id: string): Effect;
    stageSaveSelfServiceLeaveEffect5(value: string, id: string, companyId: string, date: string, segment: "full" | "first_half" | "second_half", units: number, source: "roster" | "workweek"): Effect;
    stageSaveSelfServiceLeaveEffect6(value: string, companyId: string, engagement_id: string, leaveTypeId: string, id: string, value2: number, value3: string, year: number, value4: string, value5: "Pending leave reservation", userId: string, now: number): Effect;
    readNormalizeLegacyLeaveExisting(recordId: string, companyId: string, userId: string): Promise<{
        leave_type_id: string;
        end_date: string;
        start_segment: 'full' | 'first_half' | 'second_half';
        end_segment: 'full' | 'first_half' | 'second_half';
    } | null>;
    readAdministerLeaveEffect(leaveTypeId: string, companyId: string): Promise<Record<string, unknown> | null>;
    readAdministerLeaveEffect2(engagementId: string, companyId: string): Promise<Record<string, unknown> | null>;
    readAdministerLeaveEffect3(leaveTypeId: string, companyId: string): Promise<Record<string, unknown> | null>;
    readAdministerLeaveEffect4(calendarId: string, companyId: string): Promise<Record<string, unknown> | null>;
    readAdministerLeaveEffect5(engagementId: string, companyId: string): Promise<Record<string, unknown> | null>;
    stageAdministerLeaveEffect(id: string, companyId: string, code: string, name: string, description: string, tracked: boolean, evidenceAfterUnits: number | null, evidenceNote: string, value: string, userId: string, now: number, now2: number): Promise<{
        success: boolean;
        meta: {
            changes: number;
            duration?: number;
            last_row_id?: number | string;
        };
    }>;
    stageAdministerLeaveEffect2(id: string, companyId: string, name: string, timezone: string, userId: string, now: number): Promise<{
        success: boolean;
        meta: {
            changes: number;
            duration?: number;
            last_row_id?: number | string;
        };
    }>;
    stageAdministerLeaveEffect3(id: string, companyId: string, policyKey: string, version: number, name: string, effectiveFrom: string, effectiveTo: string | null, timezone: string, value: string, allowNegative: boolean, userId: string, now: number): Effect;
    stageAdministerLeaveEffect4(id: string, leaveTypeId: string, grantKind: "annual" | "period" | "accrual" | "untracked", grantUnits: number, accrualUnits: number, accrualPeriod: "annual" | "monthly" | "quarterly" | null, carryoverLimit: number | null, employeeEligible: boolean, contractorEligible: boolean, evidenceMinUnits: number | null, evidencePurpose: "medical_certificate" | "leave_supporting_photo" | null, evidenceMinCount: number, evidenceDueStage: "before_submit" | "before_final_approval", evidenceGraceDays: number, evidenceNote: string, leaveTypeId2: string, companyId: string): Effect;
    stageAdministerLeaveEffect5(id: string, calendarId: string, date: string, name: string, calendarId2: string, companyId: string): Promise<{
        success: boolean;
        meta: {
            changes: number;
            duration?: number;
            last_row_id?: number | string;
        };
    }>;
    readAdministerLeaveOverlaps(companyId: string, engagementId: string, value: string, effectiveFrom: string): Promise<({
        id: string;
        policy_key: string;
        effective_from: string;
    })[]>;
    stageAdministerLeaveEffect6(value: string, id: string): Promise<{
        success: boolean;
        meta: {
            changes: number;
            duration?: number;
            last_row_id?: number | string;
        };
    }>;
    readAdministerLeaveEffect6(policyVersionId: string, companyId: string): Promise<Record<string, unknown> | null>;
    stageAdministerLeaveEffect7(id: string, companyId: string, engagementId: string, policyVersionId: string, calendarId: string | null, effectiveFrom: string, effectiveTo: string | null, userId: string, now: number, engagementId2: string, companyId2: string): Promise<{
        success: boolean;
        meta: {
            changes: number;
            duration?: number;
            last_row_id?: number | string;
        };
    }>;
    readAdministerLeaveRow(id: string, companyId: string): Promise<{
        effective_from: string;
        version: number;
    } | null>;
    stageAdministerLeaveEffect8(effectiveTo: string, id: string, companyId: string): Promise<{
        success: boolean;
        meta: {
            changes: number;
            duration?: number;
            last_row_id?: number | string;
        };
    }>;
    stageAdministerLeaveEffect9(id: string, companyId: string, engagementId: string, leaveTypeId: string, units: number, effectiveDate: string, key: string, note: string, userId: string, now: number): Promise<{
        success: boolean;
        meta: {
            changes: number;
            duration?: number;
            last_row_id?: number | string;
        };
    }>;
    readRunLeaveAccrualsDue(effectiveDate: string, effectiveDate2: string, effectiveDate3: string, effectiveDate4: string, effectiveDate5: string, value: string | null, value2: string | null): Promise<({
        company_id: string;
        engagement_id: string;
        leave_type_id: string;
        assignment_id: string;
        policy_version_id: string;
        grant_kind: string;
        grant_units: number;
        accrual_units: number;
        accrual_period: string | null;
        carryover_limit: number | null;
        first_day: string;
        last_day: string;
    })[]>;
    stageRunLeaveAccrualsResult(value: string, company_id: string, engagement_id: string, leave_type_id: string, kind: string, units: number, date: string, value2: string, value3: string, now: number, value4: string[]): Promise<{
        success: boolean;
        meta: {
            changes: number;
            duration?: number;
            last_row_id?: number | string;
        };
    }>;
    readLeaveAdministrationTypespoliciescalendarsengagementsassignmentseligibilities(companyId: string): Promise<({
        id: string;
        name: string;
        tracked: boolean;
    })[]>;
    readLeaveAdministrationTypespoliciescalendarsengagementsassignmentseligibilities2(companyId: string): Promise<({
        id: string;
        name: string;
        version: number;
        effective_from: string;
        effective_to: string | null;
    })[]>;
    readLeaveAdministrationTypespoliciescalendarsengagementsassignmentseligibilities3(companyId: string): Promise<({
        id: string;
        name: string;
    })[]>;
    readLeaveAdministrationTypespoliciescalendarsengagementsassignmentseligibilities4(companyId: string): Promise<({
        id: string;
        name: string;
        worker_type: string;
    })[]>;
    readLeaveAdministrationTypespoliciescalendarsengagementsassignmentseligibilities5(companyId: string): Promise<({
        id: string;
        name: string;
        policy: string;
        effective_from: string;
        effective_to: string | null;
        version: number;
    })[]>;
    readLeaveAdministrationTypespoliciescalendarsengagementsassignmentseligibilities6(companyId: string): Promise<({
        engagement_id: string;
        eligible: boolean;
        version: number;
        top_level: boolean;
    })[]>;
}
export interface LeaveSelfServicePorts<Effect> {
    repository: LeaveSelfServiceRepository<Effect>;
    clock: Clock;
    identifiers: IdentifierSource;
    prepare(holdingId: string): Promise<void>;
    commands: RosterCommands<Effect>;
    production: boolean;
    batch(effects: Effect[]): Promise<unknown>;
    decode(value: string): string;
    encode(value: string): string;
    listOwn(context: WorkContext, engagementId: string, input: z.infer<typeof leaveListPageSchema>, cursor: z.infer<typeof leaveCursorSchema> | null): Promise<LeaveListRow[]>;
    activeStaff(...args: Tail<Parameters<typeof activeStaff>>): PortResult<ReturnType<typeof activeStaff>, Effect>;
    requireCompanyAuthority(...args: Tail<Parameters<typeof requireCompanyAuthority>>): PortResult<ReturnType<typeof requireCompanyAuthority>, Effect>;
    openApproval(...args: Tail<Parameters<typeof openApproval>>): PortResult<ReturnType<typeof openApproval>, Effect>;
    peopleRequestRoute(...args: Tail<Parameters<typeof peopleRequestRoute>>): PortResult<ReturnType<typeof peopleRequestRoute>, Effect>;
    availableLeaveEvidence(...args: Tail<Parameters<typeof availableLeaveEvidence>>): PortResult<ReturnType<typeof availableLeaveEvidence>, Effect>;
    transaction<Result>(run: (ports: LeaveSelfServicePorts<Effect>) => Promise<Result>): Promise<Result>;
}
export function createLeaveSelfServiceApplication<Effect>(ports: LeaveSelfServicePorts<Effect>) {
    const repository = ports.repository;
    async function policyFor(context: WorkContext, date: string) {
        const staff = await ports.activeStaff(context.companyId, context.userId);
        const matches = await repository.readPolicyForMatches(context.companyId, staff.engagement_id, date, date, date, date);
        if (matches.length > 1 && matches[0].assignment_id !== matches[1].assignment_id)
            throw new BusinessRuleError(409, "LEAVE_POLICY_AMBIGUOUS", "HR must resolve overlapping leave policy assignments.");
        return { staff, policy: matches[0] ?? null };
    }
    async function balance(companyId: string, engagementId: string, leaveTypeId: string, year = new Date(ports.clock.now()).getUTCFullYear()) {
        return await repository.readBalanceEffect(companyId, engagementId, leaveTypeId, year);
    }
    async function previewLeave(context: WorkContext, raw: z.infer<typeof leavePreviewSchema>, excludeId: string | null = null) {
        const input = leavePreviewSchema.parse(raw);
        if (input.endDate < input.startDate)
            throw new BusinessRuleError(422, "LEAVE_DATE_RANGE", "End date must be on or after the start date.");
        if (Date.parse(input.endDate) - Date.parse(input.startDate) > 365 * 86400000)
            throw new BusinessRuleError(422, "LEAVE_DATE_RANGE", "A request can cover at most 366 calendar dates. Split longer periods into separate requests.");
        await ports.prepare(context.holdingId);
        const { staff, policy } = await policyFor(context, input.startDate);
        if (!policy)
            throw new BusinessRuleError(409, "LEAVE_POLICY_REQUIRED", "HR setup required: ask HR to assign a leave policy to your active engagement.");
        const type = await repository.readPreviewLeaveType(policy.policy_version_id, input.leaveTypeId, context.companyId, input.startDate, input.endDate);
        if (!type)
            throw new BusinessRuleError(422, "LEAVE_TYPE_UNAVAILABLE", "This leave type is not available under the assigned policy for those dates.");
        if (!(staff.worker_type === "contractor" ? type.contractor_eligible : type.employee_eligible))
            throw new BusinessRuleError(409, "LEAVE_INELIGIBLE", "This leave type is not available to your engagement. Ask HR to review your policy eligibility.");
        if (input.startDate < staff.start_date || staff.end_date && input.endDate > staff.end_date)
            throw new BusinessRuleError(422, "LEAVE_ENGAGEMENT_DATES", "The dates must fall within your engagement.");
        for (const date of dateRange(input.startDate, input.endDate)) {
            const applicable = await policyFor(context, date);
            if (applicable.policy?.policy_version_id !== policy.policy_version_id || applicable.policy.assignment_id !== policy.assignment_id)
                throw new BusinessRuleError(409, "LEAVE_POLICY_BOUNDARY", "A policy changes during this range. Create a separate request for each policy period so each calculation can be reviewed independently.");
        }
        const holidays = new Set((policy.holiday_calendar_id ? await repository.readPreviewLeaveHolidays(policy.holiday_calendar_id, input.startDate, input.endDate) : []).map((x) => x.holiday_date));
        const published = await repository.readPreviewLeavePublished(context.companyId, staff.department_id, input.endDate, input.startDate);
        const rosterPublishedDates = new Set<string>();
        for (const week of published)
            for (let i = 0; i < 7; i++)
                rosterPublishedDates.add(new Date(Date.parse(`${week.week_start}T12:00:00Z`) + i * 86400000).toISOString().slice(0, 10));
        const rosterDates = new Set((await repository.readPreviewLeaveRosterDates(context.companyId, staff.engagement_id, input.startDate, input.endDate)).map((x) => x.local_date));
        let days: LeaveCalculationDay[];
        try {
            days = calculateLeaveDays({ ...input, workweek: workweek(policy.workweek_json), holidays, rosterDates, rosterPublishedDates });
        }
        catch (error) {
            throw new BusinessRuleError(422, "LEAVE_DATE_RANGE", error instanceof Error ? error.message.replaceAll("_", " ").toLowerCase() : "Invalid date range.");
        }
        const units = days.reduce((sum, day) => sum + day.units, 0);
        if (!units)
            throw new BusinessRuleError(422, "NO_WORKING_DAYS", "The selected range contains no scheduled working days.");
        const current = (await balance(context.companyId, staff.engagement_id, type.id, Number(input.startDate.slice(0, 4)))) ?? { available: 0, reserved: 0, used: 0, accrued: 0 };
        const candidates = await repository.readPreviewLeaveCandidates(context.companyId, staff.engagement_id, excludeId, input.startDate, input.endDate);
        const legacy = await repository.readPreviewLeaveLegacy(context.companyId, staff.engagement_id, excludeId, input.endDate, input.startDate);
        const overlaps = [...new Map([...legacy, ...candidates.filter(other => days.some(day => day.date === other.leave_date && (day.segment === 'full' || other.segment === 'full' || day.segment === other.segment)))].map(row => [row.id, { id: row.id, number: row.number, state: row.state }])).values()];
        const accounts = await Promise.all([...new Set(days.map(d => Number(d.date.slice(0, 4))))].map(async (year) => ({ year, units: days.filter(d => Number(d.date.slice(0, 4)) === year).reduce((n, d) => n + d.units, 0), ...(await balance(context.companyId, staff.engagement_id, type.id, year))! })));
        const route = await ports.peopleRequestRoute(context.companyId, context.userId);
        const approvers = route.steps.length ? await repository.readPreviewLeaveApprovers(context.companyId, `{${route.steps.map((x) => x.userId).join(",")}}`) : [];
        const blockers = [
            ...overlaps.map((item) => `Overlaps ${item.number} (${item.state.replaceAll("_", " ")}).`),
            ...(route.blockedReason ? [route.blockedReason] : []),
            ...accounts.filter(a => type.tracked && !policy.allow_negative && a.available < a.units).map(a => `${a.year}: insufficient balance (${a.available} available, ${a.units} requested).`),
        ];
        const evidence = evidencePolicy(type, units);
        return {
            accounts,
            leaveType: type,
            policy: { id: policy.policy_version_id, name: policy.policy_name, version: policy.policy_version, timezone: policy.timezone, workweek: workweek(policy.workweek_json), allowNegative: policy.allow_negative, holidayCalendarId: policy.holiday_calendar_id },
            days,
            units,
            balanceBefore: current.available,
            balanceAfter: type.tracked ? current.available - units : null,
            evidence,
            evidenceRequired: evidence.required,
            evidenceNote: evidence.note,
            overlaps,
            approverRoute: route.steps.map((step) => ({
                stage: step.authority === "hr_admin" ? "HR review" : "Manager review",
                userId: step.userId,
                name: approvers.find((person) => person.user_id === step.userId)?.legal_name ?? "Assigned reviewer",
            })),
            blockers,
            canSubmit: blockers.length === 0,
        };
    }
    async function leaveSummary(context: WorkContext) {
        await ports.prepare(context.holdingId);
        let staff;
        try {
            staff = await ports.activeStaff(context.companyId, context.userId);
        }
        catch (error) {
            if (!(error instanceof BusinessRuleError) || error.code !== "STAFF_LINK_REQUIRED")
                throw error;
            return {
                asOf: ports.clock.now(), setupRequired: true,
                setupMessage: "HR setup required: your account must be linked to one active employee or contractor engagement in this company.",
                policy: null, balances: [], upcoming: [], pending: [], history: [], holidays: [],
            };
        }
        const assigned = await policyFor(context, new Date(ports.clock.now()).toISOString().slice(0, 10));
        const types = assigned.policy ? await repository.readLeaveSummaryTypes(assigned.policy.policy_version_id) : [];
        const balances = await Promise.all(types.map(async (type) => ({ type, ...((await balance(context.companyId, staff.engagement_id, type.id)) ?? { available: 0, reserved: 0, used: 0, accrued: 0 }) })));
        const requests = await repository.readLeaveSummaryRequests(context.companyId, staff.engagement_id);
        const holidays = assigned.policy?.holiday_calendar_id ? await repository.readLeaveSummaryHolidays(assigned.policy.holiday_calendar_id) : [];
        return {
            asOf: ports.clock.now(),
            setupRequired: !assigned.policy,
            setupMessage: !assigned.policy ? "HR setup required: a leave policy has not been assigned to your active engagement." : null,
            policy: assigned.policy ? { id: assigned.policy.policy_version_id, name: assigned.policy.policy_name, version: assigned.policy.policy_version, timezone: assigned.policy.timezone } : null,
            balances,
            upcoming: requests.filter((r) => r.state === "approved" && String(r.end_date ?? r.start_date) >= new Date(ports.clock.now()).toISOString().slice(0, 10)),
            pending: requests.filter((r) => r.state === "pending_approval"),
            history: requests,
            holidays,
        };
    }
    function decodeLeaveCursor(value: string | undefined) {
        if (!value)
            return null;
        try {
            return leaveCursorSchema.parse(JSON.parse(ports.decode(value)));
        }
        catch {
            throw new BusinessRuleError(422, "LEAVE_CURSOR_INVALID", "Use a cursor returned by this leave request list.");
        }
    }
    async function listOwnLeavePage(context: WorkContext, raw: {
        filter?: string;
        limit?: number | string;
        cursor?: string;
    }) {
        const input = leaveListPageSchema.parse(raw);
        await ports.prepare(context.holdingId);
        let staff;
        try {
            staff = await ports.activeStaff(context.companyId, context.userId);
        }
        catch (error) {
            if (!(error instanceof BusinessRuleError) || error.code !== "STAFF_LINK_REQUIRED")
                throw error;
            return { items: [] as LeaveListRow[], hasMore: false, nextCursor: null };
        }
        const cursor = decodeLeaveCursor(input.cursor);
        const page = await ports.listOwn(context, staff.engagement_id, input, cursor);
        const items = page.slice(0, input.limit), last = items.at(-1), hasMore = page.length > input.limit;
        return {
            items,
            hasMore,
            nextCursor: hasMore && last
                ? ports.encode(JSON.stringify({ at: Number(last.created_at), id: last.id }))
                : null,
        };
    }
    async function leaveCalendarMonth(context: WorkContext, rawMonth: string): Promise<LeaveCalendarMonth> {
        const month = leaveMonthSchema.parse(rawMonth), start = `${month}-01`, end = new Date(Date.UTC(Number(month.slice(0, 4)), Number(month.slice(5, 7)), 0))
            .toISOString()
            .slice(0, 10);
        await ports.prepare(context.holdingId);
        let staff;
        try {
            staff = await ports.activeStaff(context.companyId, context.userId);
        }
        catch (error) {
            if (!(error instanceof BusinessRuleError) || error.code !== "STAFF_LINK_REQUIRED")
                throw error;
            return { month, requests: [], holidays: [] };
        }
        const [requests, holidays] = await Promise.all([
            repository.readLeaveCalendarMonthRequestsholidays(context.companyId, staff.engagement_id, context.userId, end, start),
            repository.readLeaveCalendarMonthRequestsholidays2(context.companyId, staff.engagement_id, end, start, end, start, start, end),
        ]);
        return { month, requests, holidays };
    }
    async function createSelfServiceLeave(context: WorkContext, raw: z.infer<typeof leaveCreateSchema>, key: string) {
        return saveSelfServiceLeave(context, null, leaveCreateSchema.parse(raw), key);
    }
    async function editSelfServiceLeave(context: WorkContext, id: string, raw: z.infer<typeof leaveEditSchema>, key: string) {
        return saveSelfServiceLeave(context, id, leaveEditSchema.parse(raw), key);
    }
    async function saveSelfServiceLeave(context: WorkContext, recordId: string | null, input: z.infer<typeof leaveCreateSchema> & {
        expectedVersion?: number;
    }, key: string) {
        await ports.prepare(context.holdingId);
        const id = recordId ?? `leave-${key}`, operation = recordId ? (input.saveOnly ? 'leave.self.save' : 'leave.self.submit') : 'leave.self.create';
        return ports.transaction(contextPorts => createLeaveSelfServiceApplication(contextPorts).applySelfService(context, recordId, input, key, id, operation));
    }
    async function normalizeLegacyLeave(context: WorkContext, input: {
        startDate: string;
        days: number;
        handoverNote: string;
    }, recordId?: string) {
        const summary = await leaveSummary(context);
        await ports.prepare(context.holdingId);
        const existing = recordId ? await repository.readNormalizeLegacyLeaveExisting(recordId, context.companyId, context.userId) : null;
        const type = summary.balances.find(b => b.type.id === existing?.leave_type_id) ?? summary.balances.find(b => b.type.code === 'annual' || b.type.code === 'legacy-annual') ?? summary.balances[0];
        if (!type)
            throw new BusinessRuleError(409, 'LEAVE_POLICY_REQUIRED', summary.setupMessage ?? 'Ask HR to assign a leave policy.');
        const command = { leaveTypeId: type.type.id, startDate: input.startDate, endDate: addLeaveDays(input.startDate, input.days - 1), startSegment: 'full' as const, endSegment: 'full' as const, handoverNote: input.handoverNote };
        // The old command specifies working days. Extend its range using the same
        // roster/policy calculation until that many whole days are accounted for.
        for (let attempt = 0; attempt < 366; attempt++) {
            try {
                const preview = await previewLeave(context, command, recordId ?? null);
                if (preview.units === input.days)
                    return command;
                command.endDate = addLeaveDays(command.endDate, input.days - preview.units);
            }
            catch (error) {
                if (!(error instanceof BusinessRuleError) || error.code !== 'NO_WORKING_DAYS')
                    throw error;
                command.endDate = addLeaveDays(command.endDate, 1);
            }
        }
        throw new BusinessRuleError(422, 'LEAVE_DATE_RANGE', 'Choose an explicit date range for this request.');
    }
    async function listOwnLeave(context: WorkContext, state?: string) {
        const summary = await leaveSummary(context);
        return state ? summary.history.filter((row) => row.state === state) : summary.history;
    }
    async function administerLeave(context: WorkContext, raw: z.infer<typeof leaveAdminSchema>, key: string) {
        const input = leaveAdminSchema.parse(raw);
        await ports.prepare(context.holdingId);
        return ports.transaction(contextPorts => createLeaveSelfServiceApplication(contextPorts).applyAdministration(context, input, key));
    }
    async function runLeaveAccruals(effectiveDate: string, companyId?: string) {
        z.iso.date().parse(effectiveDate);
        const due = await repository.readRunLeaveAccrualsDue(effectiveDate, effectiveDate, effectiveDate, effectiveDate, effectiveDate, companyId ?? null, companyId ?? null);
        let inserted = 0;
        for (const item of due) {
            if (item.first_day > item.last_day)
                continue;
            const insert = async (kind: string, units: number, date: string) => {
                if (!units)
                    return;
                const result = await repository.stageRunLeaveAccrualsResult(ports.identifiers.next(), item.company_id, item.engagement_id, item.leave_type_id, kind, units, date, `leave:${item.grant_kind === 'period' ? item.assignment_id : item.engagement_id}:${kind}:${kind === 'grant' || kind === 'carryover' ? date.slice(0, 4) : date}`, `Policy ${item.policy_version_id}: ${kind}`, ports.clock.now(), (ports.production ? [item.company_id] : []));
                inserted += result.meta.changes;
            };
            if (item.grant_kind === 'period')
                await insert('grant', item.grant_units, item.first_day);
            for (let year = Number(item.first_day.slice(0, 4)); year <= Number(item.last_day.slice(0, 4)); year++) {
                const yearStart = `${year}-01-01`, first = yearStart < item.first_day ? item.first_day : yearStart;
                if (item.grant_kind === 'annual')
                    await insert('grant', item.grant_units, first);
                if (yearStart >= item.first_day && item.carryover_limit !== null) {
                    const previous = await balance(item.company_id, item.engagement_id, item.leave_type_id, year - 1);
                    await insert('carryover', Math.min(item.carryover_limit, Math.max(0, previous?.available ?? 0)), yearStart);
                }
                if (item.grant_kind === 'accrual')
                    for (let month = 1; month <= 12; month++) {
                        const date = `${year}-${String(month).padStart(2, '0')}-01`;
                        if (date < item.first_day || date > item.last_day)
                            continue;
                        if (item.accrual_period === 'monthly' || item.accrual_period === 'quarterly' && [1, 4, 7, 10].includes(month) || item.accrual_period === 'annual' && month === 1)
                            await insert('accrual', item.accrual_units, date);
                    }
            }
        }
        return inserted;
    }
    async function leaveAdministration(context: WorkContext) {
        await ports.prepare(context.holdingId);
        await ports.requireCompanyAuthority(context, 'leave.policy.manage');
        const [types, policies, calendars, engagements, assignments, eligibilities] = await Promise.all([
            repository.readLeaveAdministrationTypespoliciescalendarsengagementsassignmentseligibilities(context.companyId),
            repository.readLeaveAdministrationTypespoliciescalendarsengagementsassignmentseligibilities2(context.companyId),
            repository.readLeaveAdministrationTypespoliciescalendarsengagementsassignmentseligibilities3(context.companyId),
            repository.readLeaveAdministrationTypespoliciescalendarsengagementsassignmentseligibilities4(context.companyId),
            repository.readLeaveAdministrationTypespoliciescalendarsengagementsassignmentseligibilities5(context.companyId),
            repository.readLeaveAdministrationTypespoliciescalendarsengagementsassignmentseligibilities6(context.companyId),
        ]);
        return { types, policies, calendars, engagements, assignments, eligibilities };
    }
    async function applySelfService(context: WorkContext, recordId: string | null, input: z.infer<typeof leaveCreateSchema> & {
        expectedVersion?: number;
    }, key: string, id: string, operation: string) {
        const prior = await ports.commands.replay(context, key, operation, id, input);
        if (prior)
            return prior;
        const staff = await ports.activeStaff(context.companyId, context.userId), now = ports.clock.now();
        await repository.readSaveSelfServiceLeaveEffect(staff.engagement_id);
        const row = recordId ? await repository.readSaveSelfServiceLeaveRow(id, context.companyId, context.userId) : null;
        if (recordId && !row)
            throw new BusinessRuleError(404, 'NOT_FOUND', 'Leave request not found.');
        if (row && (row.version !== input.expectedVersion || !['draft', 'rejected'].includes(row.state)))
            throw new BusinessRuleError(409, 'STALE_VERSION', 'This request changed. Reload to review the current version; your entered values are retained.');
        if (row && row.engagement_id !== staff.engagement_id)
            throw new BusinessRuleError(409, 'ENGAGEMENT_CHANGED', 'Start a new request for your current engagement.');
        const preview = await previewLeave(context, input, id);
        if (!input.saveOnly && !preview.canSubmit)
            throw new BusinessRuleError(409, 'LEAVE_BLOCKED', preview.blockers.join(' '));
        if (!input.saveOnly && preview.evidence.required && preview.evidence.dueStage === 'before_submit') {
            const evidence = await ports.availableLeaveEvidence(id, preview.evidence);
            if (evidence.count < preview.evidence.minimumCount)
                throw new BusinessRuleError(409, 'LEAVE_EVIDENCE_REQUIRED', `Attach ${preview.evidence.minimumCount} clean ${preview.evidence.purposeKey?.replaceAll('_', ' ') ?? 'evidence'} file${preview.evidence.minimumCount === 1 ? '' : 's'} before submission.`);
        }
        const version = (row?.version ?? 0) + 1, state = input.saveOnly ? 'draft' : 'pending_approval', statements = [];
        if (!row) {
            const employee = await repository.readSaveSelfServiceLeaveEmployee(staff.person_id, context.companyId);
            const employeeId = employee?.id ?? `employee-${staff.person_id}`;
            if (!employee)
                statements.push(repository.stageSaveSelfServiceLeaveEffect(employeeId, staff.person_id, context.companyId, staff.branch_id ?? '', staff.department_id, staff.legal_name, now));
            statements.push(repository.stageSaveSelfServiceLeaveEffect2(id, `LR-${key}`, employeeId, staff.person_id, staff.engagement_id, context.companyId, staff.branch_id ?? '', staff.department_id, input.startDate, Math.ceil(preview.units), input.handoverNote, context.userId, now, now));
        }
        const evidenceDueAt = !input.saveOnly && preview.evidence.required
            ? now + preview.evidence.graceDays * 86400000
            : null;
        statements.push(repository.stageSaveSelfServiceLeaveEffect3(input.startDate, Math.ceil(preview.units), input.handoverNote, input.leaveTypeId, input.endDate, input.startSegment, input.endSegment, preview.policy.id, preview.units, JSON.stringify(preview), JSON.stringify(preview.evidence), evidenceDueAt, state, now, state, version, context.userId, key, now, id), repository.stageSaveSelfServiceLeaveEffect4(id), ...preview.days.map(day => repository.stageSaveSelfServiceLeaveEffect5(ports.identifiers.next(), id, context.companyId, day.date, day.segment, day.units, day.source)));
        if (!input.saveOnly) {
            const route = await ports.peopleRequestRoute(context.companyId, context.userId);
            statements.push(...(await ports.openApproval(key, 'leave', id, 'request', context.userId, null, route, version)).statements);
            if (preview.leaveType.tracked)
                for (const account of preview.accounts)
                    statements.push(repository.stageSaveSelfServiceLeaveEffect6(ports.identifiers.next(), context.companyId, staff.engagement_id, input.leaveTypeId, id, -account.units, `${account.year}-01-01`, account.year, `${key}:${account.year}`, 'Pending leave reservation', context.userId, now));
        }
        return ports.commands.commit(context, key, operation, input, id, version, { id, state, version, preview }, statements, { companyId: context.companyId, branchId: staff.branch_id, departmentId: staff.department_id });
    }
    async function applyAdministration(context: WorkContext, input: z.infer<typeof leaveAdminSchema>, key: string) {
        const prior = await ports.commands.replay<{id:string}>(context, key, "leave.policy.manage", context.companyId, input);
        if (prior)
            return prior;
        await ports.requireCompanyAuthority(context, "leave.policy.manage");
        if ('effectiveFrom' in input && input.effectiveTo && input.effectiveTo < input.effectiveFrom)
            throw new BusinessRuleError(422, 'LEAVE_DATE_RANGE', 'Effective end must not precede the start.');
        if ('timezone' in input) {
            try {
                new Intl.DateTimeFormat('en', { timeZone: input.timezone });
            }
            catch {
                throw new BusinessRuleError(422, 'LEAVE_TIMEZONE', 'Choose a valid timezone.');
            }
        }
        if (input.resource === 'policy' && new Set(input.rules.map(r => r.leaveTypeId)).size !== input.rules.length)
            throw new BusinessRuleError(422, 'LEAVE_RULE_DUPLICATE', 'Use one rule per leave type in this policy.');
        if (input.resource === 'policy')
            for (const rule of input.rules) {
                if (!await repository.readAdministerLeaveEffect(rule.leaveTypeId, context.companyId))
                    throw new BusinessRuleError(404, 'NOT_FOUND', 'Leave type not found in this Company.');
                if (rule.grantKind === 'accrual' && (!rule.accrualPeriod || rule.accrualUnits <= 0))
                    throw new BusinessRuleError(422, 'LEAVE_ACCRUAL', 'Choose a positive accrual amount and period.');
            }
        if (input.resource === 'adjustment' && (!await repository.readAdministerLeaveEffect2(input.engagementId, context.companyId) || !await repository.readAdministerLeaveEffect3(input.leaveTypeId, context.companyId)))
            throw new BusinessRuleError(404, 'NOT_FOUND', 'Engagement or leave type not found in this Company.');
        if ((input.resource === 'holiday' || input.resource === 'assignment') && input.calendarId && !await repository.readAdministerLeaveEffect4(input.calendarId, context.companyId))
            throw new BusinessRuleError(404, 'NOT_FOUND', 'Holiday calendar not found in this Company.');
        if (input.resource === 'assignment' && !await repository.readAdministerLeaveEffect5(input.engagementId, context.companyId))
            throw new BusinessRuleError(404, 'NOT_FOUND', 'Engagement not found in this Company.');
        const id = `leave-${input.resource}-${ports.identifiers.next()}`, now = ports.clock.now();
        if (input.resource === "type")
            await repository.stageAdministerLeaveEffect(id, context.companyId, input.code, input.name, input.description, input.tracked, input.evidenceAfterUnits, input.evidenceNote, new Date(ports.clock.now()).toISOString().slice(0, 10), context.userId, now, now);
        else if (input.resource === "calendar")
            await repository.stageAdministerLeaveEffect2(id, context.companyId, input.name, input.timezone, context.userId, now);
        else if (input.resource === "policy")
            await ports.batch([
                repository.stageAdministerLeaveEffect3(id, context.companyId, input.policyKey, input.version, input.name, input.effectiveFrom, input.effectiveTo, input.timezone, JSON.stringify(input.workweek), input.allowNegative, context.userId, now),
                ...input.rules.map((rule) => repository.stageAdministerLeaveEffect4(id, rule.leaveTypeId, rule.grantKind, rule.grantUnits, rule.accrualUnits, rule.accrualPeriod, rule.carryoverLimit, rule.employeeEligible, rule.contractorEligible, rule.evidenceMinUnits, rule.evidencePurpose, rule.evidenceMinCount, rule.evidenceDueStage, rule.evidenceGraceDays, rule.evidenceNote, rule.leaveTypeId, context.companyId)),
            ]);
        else if (input.resource === "holiday")
            await repository.stageAdministerLeaveEffect5(id, input.calendarId, input.date, input.name, input.calendarId, context.companyId);
        else if (input.resource === "assignment") {
            const overlaps = await repository.readAdministerLeaveOverlaps(context.companyId, input.engagementId, input.effectiveTo ?? '9999-12-31', input.effectiveFrom);
            if (overlaps.some(a => a.policy_key !== 'legacy-untracked' || a.effective_from >= input.effectiveFrom))
                throw new BusinessRuleError(409, 'LEAVE_POLICY_OVERLAP', 'End the existing assignment before assigning a new policy.');
            for (const row of overlaps)
                await repository.stageAdministerLeaveEffect6(addLeaveDays(input.effectiveFrom, -1), row.id);
            await repository.readAdministerLeaveEffect6(input.policyVersionId, context.companyId).then(row => {
                if (!row)
                    throw new BusinessRuleError(404, 'NOT_FOUND', 'Policy not found in this Company.');
            });
            await repository.stageAdministerLeaveEffect7(id, context.companyId, input.engagementId, input.policyVersionId, input.calendarId, input.effectiveFrom, input.effectiveTo, context.userId, now, input.engagementId, context.companyId);
        }
        else if (input.resource === 'assignment_end') {
            const row = await repository.readAdministerLeaveRow(input.id, context.companyId);
            if (!row)
                throw new BusinessRuleError(404, 'NOT_FOUND', 'Policy assignment not found.');
            if (row.version !== input.expectedVersion)
                throw new BusinessRuleError(409, 'STALE_VERSION', 'The assignment changed. Reload before editing.');
            if (input.effectiveTo < row.effective_from)
                throw new BusinessRuleError(422, 'LEAVE_DATE_RANGE', 'End must not precede the assignment start.');
            await repository.stageAdministerLeaveEffect8(input.effectiveTo, input.id, context.companyId);
        }
        else
            await repository.stageAdministerLeaveEffect9(id, context.companyId, input.engagementId, input.leaveTypeId, input.units, input.effectiveDate, key, input.note, context.userId, now);
        await runLeaveAccruals(new Date(ports.clock.now()).toISOString().slice(0, 10));
        return ports.commands.commit(context, key, "leave.policy.manage", input, context.companyId, 1, { id }, []);
    }
    return { previewLeave, leaveSummary, listOwnLeavePage, leaveCalendarMonth, createSelfServiceLeave, editSelfServiceLeave, normalizeLegacyLeave, listOwnLeave, administerLeave, runLeaveAccruals, leaveAdministration, applyAdministration, applySelfService };
}
