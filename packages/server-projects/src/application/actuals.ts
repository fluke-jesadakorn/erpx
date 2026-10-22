import { z } from 'zod';
import { samePerson } from "@erpx/feature-identity_access/lib/iam";
import { hasPermission } from "@erpx/feature-identity_access/lib/permissions";
import type { WorkContext } from '@erpx/server-identity_access/public/authorization';
import type { StaffActor, StaffPerson } from '@erpx/server-operations/public/staff-policy';
import type { ProjectRow } from '@erpx/server-projects/domain/staff-projects';
import { BusinessRuleError, type Clock } from '@erpx/server-shared/index';
import { projectWorkLogSchema, projectWorkLogDecisionSchema, projectCostAllocationSchema, projectCostAllocationDecisionSchema, type ProjectWorkLogRow, type ProjectCostAllocationRow, type ProjectCostSourceRow, type ProjectFinanceScope } from '@erpx/server-projects/domain/actuals';
import { decimalToMinor, minorToDecimal } from "@erpx/feature-finance/lib/finance-money";
export interface ProjectActualsRepository<Effect> {
    prepareActualEvent(value: string, id: string, version: number, action: string, userId: string, reason: string, value2: string, now: number, table: string): Effect;
    workLogs(userId: string, userId2: string, userId3: string, companyId: string, id: string, userId4: string, userId5: string): Promise<{
        results: ProjectWorkLogRow[];
    }>;
    commandReceipt(key: string): Promise<{
        operation: string;
        record_id: string;
    } | null>;
    workLogForDate(companyId: string, projectId: string, userId: string, workDate: string): Promise<ProjectWorkLogRow | null>;
    dailyMinutes(companyId: string, userId: string, workDate: string, id: string): Promise<{
        minutes: string;
    } | null>;
    guardWorkMember(key: string, value: "project-member", id: string, companyId: string, userId: string, userId2: string): Effect;
    guardTimeRevision(key: string, value: "project-time-version", id: string, companyId: string, value2: number, userId: string): Effect;
    guardDailyRevision(key: string, value: "project-time-daily-limit", companyId: string, userId: string, workDate: string, id: string, minutes: number): Effect;
    prepareTimeRevision(minutes: number, now: number, id: string): Effect;
    guardUniqueDate(key: string, value: "project-time-date", id: string, userId: string, workDate: string): Effect;
    guardDailySubmission(key: string, value: "project-time-daily-limit", companyId: string, userId: string, workDate: string, id: string, minutes: number): Effect;
    prepareTime(id: string, companyId: string, id2: string, userId: string, workDate: string, minutes: number, now: number, now2: number): Effect;
    prepareTimeProjectVersion(now: number, id: string, companyId: string): Effect;
    workLog(logId: string, companyId: string, projectId: string): Promise<ProjectWorkLogRow | null>;
    guardTimeReviewer(key: string, value: "project-owner", id: string, companyId: string, version: number, userId: string): Effect;
    guardTimeVersion(key: string, value: "project-time-version", logId: string, companyId: string, projectId: string, expectedVersion: number, userId: string): Effect;
    prepareTimeDecision(state: "approved" | "rejected", value: string | null, value2: number | null, value3: string, now: number, logId: string): Effect;
    prepareTimeDecisionProjectVersion(now: number, id: string, companyId: string): Effect;
    costAllocations(userId: string, userId2: string, companyId: string, id: string): Promise<{
        results: Record<string, string | number | boolean | null>[];
    }>;
    costSources(companyId: string, branchId: string | null, branchId2: string | null, departmentId: string | null, departmentId2: string | null, branch_id: string | null, branch_id2: string | null, department_id: string | null, department_id2: string | null): Promise<{
        results: Record<string, string | number>[];
    }>;
    costCommandReceipt(key: string): Promise<{
        operation: string;
        record_id: string;
    } | null>;
    expenseLine(sourceLineId: string, companyId: string, branchId: string | null, branchId2: string | null, departmentId: string | null, departmentId2: string | null, branch_id: string | null, branch_id2: string | null, department_id: string | null, department_id2: string | null): Promise<{
        id: string;
        debit_minor: string;
        currency: string;
        entry_date: string;
        journal_status: string;
        category: string;
    } | null>;
    existingAllocation(id: string, id2: string): Promise<Record<string, string | number | null> | null>;
    guardCostScope(key: string, value: "project-finance-scope", id: string, companyId: string, branchId: string | null, branchId2: string | null, departmentId: string | null, departmentId2: string | null): Effect;
    guardExpenseSource(key: string, value: "posted-expense-source", id: string, companyId: string, id2: string, companyId2: string, branchId: string | null, branchId2: string | null, branch_id: string | null, departmentId: string | null, departmentId2: string | null, department_id: string | null): Effect;
    guardAllocationCap(key: string, value: "project-cost-cap", id: string, amountMinor: number, id2: string): Effect;
    guardAllocationRevision(key: string, value: "allocation-version", id: string, companyId: string, value2: number, userId: string): Effect;
    prepareAllocationRevision(amountMinor: number, now: number, id: string): Effect;
    guardUniqueAllocation(key: string, value: "allocation-unique", id: string, id2: string): Effect;
    prepareAllocation(id: string, companyId: string, id2: string, id3: string, amountMinor: number, userId: string, now: number, now2: number): Effect;
    allocationReviewFacts(allocationId: string, companyId: string): Promise<Record<string, string | number | null> | null>;
    guardAllocationReviewVersion(key: string, value: "allocation-version", allocationId: string, companyId: string, expectedVersion: number, userId: string, action: "approve" | "reject"): Effect;
    prepareAllocationDecision(state: "approved" | "rejected", value: string | null, value2: string | null, value3: string, now: number, allocationId: string): Effect;
}
export interface OwnerCommands<Effect> {
    replay<Result = Record<string, unknown>>(actor: StaffActor, key: string, operation: string, id: string, input: unknown): Promise<Result | null>;
    commit<Result>(actor: StaffActor, key: string, operation: string, input: unknown, id: string, version: number, result: Result, effects: Effect[], scope?: Pick<StaffActor, 'companyId' | 'branchId' | 'departmentId'>): Promise<Result>;
}
export interface ProjectActualsPorts<Effect> {
    repository: ProjectActualsRepository<Effect>;
    clock: Clock;
    commands: OwnerCommands<Effect>;
    getProject(actor: StaffActor, id: string): Promise<ProjectRow>;
    activeStaff(companyId: string, userId: string): Promise<StaffPerson>;
    staffGuard(key: string, companyId: string, userId: string, suffix?: string): Effect;
    requirePermission(actor: WorkContext, permission: string): WorkContext;
}
export function createProjectActualsApplication<Effect>(ports: ProjectActualsPorts<Effect>) {
    const r = ports.repository;
    async function assertFinanceProjectAccess(actor: WorkContext, project: ProjectFinanceScope) {
        if ((actor.branchId && actor.branchId !== project.branch_id)
            || (actor.departmentId && actor.departmentId !== project.department_id))
            throw new BusinessRuleError(404, "PROJECT_NOT_FOUND", "Project not found in the current finance scope.");
    }
    function event(table: "work_log" | "cost_allocation", id: string, version: number, action: string, actor: StaffActor, reason: string, snapshot: Record<string, unknown>, now: number) {
        return r.prepareActualEvent(`${id}:${version}`, id, version, action, actor.userId, reason, JSON.stringify(snapshot), now, table);
    }
    async function listProjectWorkLogs(actor: StaffActor, projectId: string) {
        const project = await ports.getProject(actor, projectId);
        return (await r.workLogs(actor.userId, actor.userId, actor.userId, actor.companyId, project.id, actor.userId, actor.userId)).results;
    }
    async function saveProjectWorkLog(actor: StaffActor, projectId: string, input: z.infer<typeof projectWorkLogSchema>, key: string) {
        const project = await ports.getProject(actor, projectId);
        const staff = await ports.activeStaff(actor.companyId, actor.userId);
        const receipt = await r.commandReceipt(key);
        if (receipt && ['project.actual-time.submit', 'project.actual-time.revise'].includes(receipt.operation)) {
            const prior = await ports.commands.replay<{
                id: string;
                version: number;
                state: string;
            }>(actor, key, receipt.operation, receipt.record_id, { projectId, ...input });
            if (prior)
                return prior;
        }
        if (project.lifecycle_status !== 'active')
            throw new BusinessRuleError(409, 'PROJECT_STRUCTURE_FROZEN', 'Reopen the project before changing actual time.');
        const today = new Date(ports.clock.now()).toISOString().slice(0, 10);
        if (input.workDate > today || input.workDate < staff.start_date || (staff.end_date && input.workDate > staff.end_date))
            throw new BusinessRuleError(422, "PROJECT_WORK_DATE_INVALID", "Project work date must fall within your active engagement and cannot be in the future.");
        const existing = await r.workLogForDate(actor.companyId, projectId, actor.userId, input.workDate);
        const id = existing?.id ?? `project-time-${key}`;
        const operation = existing ? "project.actual-time.revise" : "project.actual-time.submit";
        const prior = await ports.commands.replay<{
            id: string;
            version: number;
            state: string;
        }>(actor, key, operation, id, { projectId, ...input });
        if (prior)
            return prior;
        if (existing && (existing.state !== "rejected" || existing.version !== input.expectedVersion))
            throw new BusinessRuleError(409, "PROJECT_TIME_NOT_EDITABLE", "Only a rejected project time entry can be revised. Refresh the project and try again.");
        const priorMinutes = await r.dailyMinutes(actor.companyId, actor.userId, input.workDate, id);
        if (Number(priorMinutes?.minutes ?? "0") + input.minutes > 1440)
            throw new BusinessRuleError(422, "PROJECT_TIME_DAILY_LIMIT", "Total submitted and approved project time cannot exceed 24 hours in one work day.");
        const now = ports.clock.now(), version = existing ? input.expectedVersion! + 1 : 1;
        const snapshot = { projectId, workDate: input.workDate, minutes: input.minutes, state: "submitted" };
        return ports.commands.commit(actor, key, operation, { projectId, ...input }, id, version, { id, version, state: "submitted" }, [
            ports.staffGuard(key, actor.companyId, actor.userId),
            r.guardWorkMember(key, "project-member", project.id, actor.companyId, actor.userId, actor.userId),
            ...(existing ? [
                r.guardTimeRevision(key, "project-time-version", id, actor.companyId, input.expectedVersion!, actor.userId),
                r.guardDailyRevision(key, "project-time-daily-limit", actor.companyId, actor.userId, input.workDate, id, input.minutes),
                r.prepareTimeRevision(input.minutes, now, id),
            ] : [
                r.guardUniqueDate(key, "project-time-date", project.id, actor.userId, input.workDate),
                r.guardDailySubmission(key, "project-time-daily-limit", actor.companyId, actor.userId, input.workDate, id, input.minutes),
                r.prepareTime(id, actor.companyId, project.id, actor.userId, input.workDate, input.minutes, now, now),
            ]),
            r.prepareTimeProjectVersion(now, project.id, actor.companyId),
            event("work_log", id, version, existing ? "revised" : "submitted", actor, existing ? "Revised rejected project time entry." : "Submitted project time entry for owner review.", snapshot, now),
        ], { companyId: project.company_id, branchId: project.branch_id, departmentId: project.department_id });
    }
    async function decideProjectWorkLog(actor: StaffActor, projectId: string, logId: string, input: z.infer<typeof projectWorkLogDecisionSchema>, key: string) {
        const operation = `project.actual-time.${input.action}`;
        const prior = await ports.commands.replay<{
            id: string;
            version: number;
            state: string;
        }>(actor, key, operation, logId, { projectId, ...input });
        if (prior)
            return prior;
        const project = await ports.getProject(actor, projectId);
        if (!samePerson(actor, project.owner_user_id))
            throw new BusinessRuleError(403, "PROJECT_OWNER_REQUIRED", "Only the project owner can review project time.");
        await ports.activeStaff(actor.companyId, actor.userId);
        const log = await r.workLog(logId, actor.companyId, projectId);
        if (!log)
            throw new BusinessRuleError(404, "PROJECT_TIME_NOT_FOUND", "Project time entry not found.");
        if (samePerson(actor, log.user_id))
            throw new BusinessRuleError(409, "SELF_REVIEW_FORBIDDEN", "A different project owner must review this time entry.");
        if (log.state !== "submitted" || log.version !== input.expectedVersion)
            throw new BusinessRuleError(409, "PROJECT_TIME_REVIEW_STALE", "Refresh the project time entry before reviewing it.");
        const state = input.action === "approve" ? "approved" : "rejected", version = input.expectedVersion + 1, now = ports.clock.now();
        const snapshot = { projectId, userId: log.user_id, workDate: log.work_date, minutes: log.minutes, state };
        return ports.commands.commit(actor, key, operation, { projectId, ...input }, logId, version, { id: logId, version, state }, [
            ports.staffGuard(key, actor.companyId, actor.userId),
            r.guardTimeReviewer(key, "project-owner", project.id, actor.companyId, project.version, actor.userId),
            r.guardTimeVersion(key, "project-time-version", logId, actor.companyId, projectId, input.expectedVersion, actor.userId),
            r.prepareTimeDecision(state, state === "approved" ? actor.userId : null, state === "approved" ? now : null, state === "rejected" ? input.reason : "", now, logId),
            r.prepareTimeDecisionProjectVersion(now, project.id, actor.companyId),
            event("work_log", logId, version, state, actor, input.reason, snapshot, now),
        ], { companyId: project.company_id, branchId: project.branch_id, departmentId: project.department_id });
    }
    function financeProject(actor: WorkContext, permission: "finance.read" | "finance.write" | "finance.approve") {
        if (!hasPermission(actor, permission))
            throw new BusinessRuleError(403, "FINANCE_PERMISSION_REQUIRED", `The ${permission} permission is required for project cost actuals.`);
        return ports.requirePermission(actor, permission);
    }
    async function visibleFinanceProject(actor: WorkContext, projectId: string) {
        const project = await ports.getProject(actor, projectId) as ProjectFinanceScope;
        await assertFinanceProjectAccess(actor, project);
        return project;
    }
    async function listProjectCostAllocations(actor: WorkContext, projectId: string) {
        actor = financeProject(actor, "finance.read");
        const project = await visibleFinanceProject(actor, projectId);
        return (await r.costAllocations(actor.userId, actor.userId, actor.companyId, project.id)).results.map((row) => ({
            ...row, amount: minorToDecimal(Number(row.amount_minor)),
        })) as ProjectCostAllocationRow[];
    }
    async function listProjectCostSources(actor: WorkContext, projectId: string) {
        actor = financeProject(actor, "finance.read");
        const project = await visibleFinanceProject(actor, projectId);
        const rows = (await r.costSources(actor.companyId, actor.branchId, actor.branchId, actor.departmentId, actor.departmentId, project.branch_id, project.branch_id, project.department_id, project.department_id)).results;
        return rows.map((row) => ({ ...row, debit: minorToDecimal(Number(row.debit_minor)), remaining: minorToDecimal(Number(row.remaining_minor)) })) as ProjectCostSourceRow[];
    }
    async function createProjectCostAllocation(actor: WorkContext, input: z.infer<typeof projectCostAllocationSchema>, key: string) {
        actor = financeProject(actor, "finance.write");
        const project = await visibleFinanceProject(actor, input.projectId);
        const receipt = await r.costCommandReceipt(key);
        if (receipt && ['project.cost-allocation.create', 'project.cost-allocation.revise'].includes(receipt.operation)) {
            const prior = await ports.commands.replay<{
                id: string;
                version: number;
                state: string;
            }>(actor, key, receipt.operation, receipt.record_id, input);
            if (prior)
                return prior;
        }
        if (project.lifecycle_status !== 'active')
            throw new BusinessRuleError(409, 'PROJECT_STRUCTURE_FROZEN', 'Reopen the project before changing actual cost.');
        const amountMinor = decimalToMinor(input.amount);
        if (amountMinor <= 0)
            throw new BusinessRuleError(422, "INVALID_PROJECT_COST", "Project cost allocations must be greater than zero.");
        const line = await r.expenseLine(input.sourceLineId, actor.companyId, actor.branchId, actor.branchId, actor.departmentId, actor.departmentId, project.branch_id, project.branch_id, project.department_id, project.department_id);
        if (!line || line.journal_status !== "posted" || !["expense", "other_expense"].includes(line.category))
            throw new BusinessRuleError(422, "PROJECT_COST_SOURCE_INVALID", "Choose a debit line from a posted expense journal in the current Company.");
        const existing = await r.existingAllocation(line.id, project.id);
        const id = existing ? String(existing.id) : `project-cost-${key}`;
        const operation = existing ? "project.cost-allocation.revise" : "project.cost-allocation.create";
        const prior = await ports.commands.replay<{
            id: string;
            version: number;
            state: string;
        }>(actor, key, operation, id, input);
        if (prior)
            return prior;
        if (existing && (existing.state !== "rejected" || Number(existing.version) !== input.expectedVersion || !samePerson(actor, String(existing.created_by))))
            throw new BusinessRuleError(409, "PROJECT_COST_NOT_EDITABLE", "Only the finance user who created a rejected allocation can revise it.");
        const version = existing ? input.expectedVersion! + 1 : 1, now = ports.clock.now();
        const reason = existing ? "Revised rejected project cost allocation." : "Submitted posted expense line for project cost review.";
        const snapshot = { projectId: project.id, journalLineId: line.id, amountMinor: String(amountMinor), currency: line.currency, state: "draft" };
        return ports.commands.commit(actor, key, operation, input, id, version, { id, version, state: "draft" }, [
            r.guardCostScope(key, "project-finance-scope", project.id, actor.companyId, actor.branchId, actor.branchId, actor.departmentId, actor.departmentId),
            r.guardExpenseSource(key, "posted-expense-source", project.id, actor.companyId, line.id, actor.companyId, actor.branchId, actor.branchId, project.branch_id, actor.departmentId, actor.departmentId, project.department_id),
            r.guardAllocationCap(key, "project-cost-cap", line.id, amountMinor, id),
            ...(existing ? [
                r.guardAllocationRevision(key, "allocation-version", id, actor.companyId, input.expectedVersion!, actor.userId),
                r.prepareAllocationRevision(amountMinor, now, id),
            ] : [
                r.guardUniqueAllocation(key, "allocation-unique", line.id, project.id),
                r.prepareAllocation(id, actor.companyId, project.id, line.id, amountMinor, actor.userId, now, now),
            ]),
            event("cost_allocation", id, version, existing ? "revised" : "created", actor, reason, snapshot, now),
        ], { companyId: project.company_id, branchId: project.branch_id, departmentId: project.department_id });
    }
    async function decideProjectCostAllocation(actor: WorkContext, allocationId: string, input: z.infer<typeof projectCostAllocationDecisionSchema>, key: string) {
        actor = financeProject(actor, "finance.approve");
        const operation = `project.cost-allocation.${input.action}`;
        const prior = await ports.commands.replay<{
            id: string;
            version: number;
            state: string;
        }>(actor, key, operation, allocationId, input);
        if (prior)
            return prior;
        const row = await r.allocationReviewFacts(allocationId, actor.companyId);
        if (!row)
            throw new BusinessRuleError(404, "PROJECT_COST_NOT_FOUND", "Project cost allocation not found.");
        if ((actor.branchId && actor.branchId !== row.branch_id) || (actor.departmentId && actor.departmentId !== row.department_id))
            throw new BusinessRuleError(404, "PROJECT_COST_NOT_FOUND", "Project cost allocation is not in the current finance scope.");
        if (samePerson(actor, String(row.created_by)))
            throw new BusinessRuleError(409, "SELF_REVIEW_FORBIDDEN", "A different Finance approver must review this allocation.");
        if (row.state !== "draft" || Number(row.version) !== input.expectedVersion)
            throw new BusinessRuleError(409, "PROJECT_COST_REVIEW_STALE", "Refresh the project cost allocation before reviewing it.");
        if (input.action === "approve" && row.journal_status !== "posted")
            throw new BusinessRuleError(409, "PROJECT_COST_SOURCE_STALE", "The source journal is no longer posted; this allocation cannot be approved.");
        const state = input.action === "approve" ? "approved" : "rejected", version = input.expectedVersion + 1, now = ports.clock.now();
        const snapshot = { projectId: row.project_id, journalLineId: row.journal_line_id, amountMinor: String(row.amount_minor), state };
        return ports.commands.commit(actor, key, operation, input, allocationId, version, { id: allocationId, version, state }, [
            r.guardAllocationReviewVersion(key, "allocation-version", allocationId, actor.companyId, input.expectedVersion, actor.userId, input.action),
            r.prepareAllocationDecision(state, state === "approved" ? actor.userId : null, state === "rejected" ? actor.userId : null, state === "rejected" ? input.reason : "", now, allocationId),
            event("cost_allocation", allocationId, version, state, actor, input.reason, snapshot, now),
        ], { companyId: String(row.company_id), branchId: row.branch_id as string | null, departmentId: row.department_id as string | null });
    }
    return { listProjectWorkLogs, saveProjectWorkLog, decideProjectWorkLog, listProjectCostAllocations, listProjectCostSources, createProjectCostAllocation, decideProjectCostAllocation };
}
