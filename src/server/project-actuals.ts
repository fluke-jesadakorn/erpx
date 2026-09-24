import "server-only";
import { z } from "zod";
import { samePerson } from "@/lib/iam";
import { hasPermission } from "@/lib/permissions";
import { decimalToMinor, minorToDecimal } from "@/lib/finance-money";
import { requirePermission } from "./iam-permissions";
import { ApiError } from "./http";
import { assertion, executeCommand, replayCommand } from "./commands";
import type { WorkContext } from "./authorization";
import type { SqlDatabase } from "./runtime/types";
import { getProject, type ProjectRow } from "./staff-tasks";
import { activeStaff, staffGuard, type StaffActor } from "./staff-policy";
import { holdingResources } from "./runtime";

export const projectWorkLogSchema = z.object({
  workDate: z.string().date(),
  minutes: z.number().int().min(1).max(1440),
  expectedVersion: z.number().int().positive().optional(),
}).strict();

export const projectWorkLogDecisionSchema = z.object({
  action: z.enum(["approve", "reject"]),
  expectedVersion: z.number().int().positive(),
  reason: z.string().trim().min(3).max(2000),
}).strict();

export const projectCostAllocationSchema = z.object({
  projectId: z.string().min(1).max(200),
  sourceLineId: z.string().min(1).max(200),
  amount: z.string().regex(/^\d+(\.\d{1,2})?$/),
  expectedVersion: z.number().int().positive().optional(),
}).strict();

export const projectCostAllocationDecisionSchema = z.object({
  action: z.enum(["approve", "reject"]),
  expectedVersion: z.number().int().positive(),
  reason: z.string().trim().min(3).max(2000),
}).strict();

export type ProjectWorkLogRow = {
  id: string; project_id: string; user_id: string; work_date: string; minutes: number;
  state: "submitted" | "approved" | "rejected"; version: number; reviewed_by: string | null;
  rejection_reason: string; created_at: number; updated_at: number; is_actor: boolean; can_review: boolean;
};

export type ProjectCostAllocationRow = {
  id: string; project_id: string; journal_line_id: string; amount_minor: string; amount: string; state: string; version: number;
  created_by: string; approved_by: string | null; rejection_reason: string; created_at: number; updated_at: number;
  journal_id: string; journal_number: string; entry_date: string; currency: string; journal_status: string;
  account_code: string; account_name: string; can_review: boolean; is_creator: boolean;
};
export type ProjectCostSourceRow = {
  source_line_id: string; journal_id: string; journal_number: string; entry_date: string; currency: string;
  account_id: string; account_code: string; account_name: string; description: string; debit_minor: string;
  remaining_minor: string; debit: string; remaining: string;
};

type ProjectFinanceScope = ProjectRow & { branch_id: string | null; department_id: string | null };

async function assertFinanceProjectAccess(actor: WorkContext, project: ProjectFinanceScope) {
  if ((actor.branchId && actor.branchId !== project.branch_id)
      || (actor.departmentId && actor.departmentId !== project.department_id))
    throw new ApiError(404, "PROJECT_NOT_FOUND", "Project not found in the current finance scope.");
}

function event(db: SqlDatabase, table: "work_log" | "cost_allocation", id: string, version: number,
  action: string, actor: StaffActor, reason: string, snapshot: Record<string, unknown>, now: number) {
  const allocation = table === "cost_allocation";
  const eventTable = allocation ? "staff_project_cost_allocation_events" : "staff_project_work_log_events";
  const idField = allocation ? "allocation_id" : "work_log_id";
  const versionField = allocation ? "allocation_version" : "work_log_version";
  return db.prepare(`INSERT INTO ${eventTable}(id,${idField},${versionField},action,actor_id,reason,snapshot_json,created_at)
    VALUES(?,?,?,?,?,?,?::jsonb,?)`).bind(`${id}:${version}`, id, version, action, actor.userId, reason, JSON.stringify(snapshot), now);
}

export async function listProjectWorkLogs(db: SqlDatabase, actor: StaffActor, projectId: string) {
  const project = await getProject(db, actor, projectId);
  return (await db.prepare(`SELECT l.id,l.project_id,l.user_id,l.work_date,l.minutes,l.state,l.version,l.reviewed_by,
      l.rejection_reason,l.created_at,l.updated_at,
      iam_same_person(l.user_id,?,p.company_id) is_actor,
      (iam_same_person(p.owner_user_id,?,p.company_id) AND NOT iam_same_person(l.user_id,?,p.company_id)) can_review
    FROM staff_project_work_logs l JOIN staff_projects p ON p.id=l.project_id AND p.company_id=l.company_id
    WHERE l.company_id=? AND l.project_id=? AND (iam_same_person(p.owner_user_id,?,p.company_id) OR iam_same_person(l.user_id,?,p.company_id))
    ORDER BY l.work_date DESC,l.id`)
    .bind(actor.userId, actor.userId, actor.userId, actor.companyId, project.id, actor.userId, actor.userId).all<ProjectWorkLogRow>()).results;
}

export async function saveProjectWorkLog(db: SqlDatabase, actor: StaffActor, projectId: string,
  input: z.infer<typeof projectWorkLogSchema>, key: string) {
  const project = await getProject(db, actor, projectId);
  const staff = await activeStaff(db, actor.companyId, actor.userId);
  const today = new Date().toISOString().slice(0, 10);
  if (input.workDate > today || input.workDate < staff.start_date || (staff.end_date && input.workDate > staff.end_date))
    throw new ApiError(422, "PROJECT_WORK_DATE_INVALID", "Project work date must fall within your active engagement and cannot be in the future.");
  const existing = await db.prepare(`SELECT * FROM staff_project_work_logs WHERE company_id=? AND project_id=? AND iam_same_person(user_id,?,company_id) AND work_date=? LIMIT 1`)
    .bind(actor.companyId, projectId, actor.userId, input.workDate).first<ProjectWorkLogRow>();
  const id = existing?.id ?? `project-time-${key}`;
  const operation = existing ? "project.actual-time.revise" : "project.actual-time.submit";
  const prior = await replayCommand<{ id: string; version: number; state: string }>(db, actor, key, operation, id, { projectId, ...input });
  if (prior) return prior;
  if (existing && (existing.state !== "rejected" || existing.version !== input.expectedVersion))
    throw new ApiError(409, "PROJECT_TIME_NOT_EDITABLE", "Only a rejected project time entry can be revised. Refresh the project and try again.");
  const priorMinutes = await db.prepare(`SELECT COALESCE(SUM(minutes),0)::text minutes FROM staff_project_work_logs
    WHERE company_id=? AND iam_same_person(user_id,?,company_id) AND work_date=? AND state IN ('submitted','approved') AND id!=?`)
    .bind(actor.companyId, actor.userId, input.workDate, id).first<{ minutes: string }>();
  if (Number(priorMinutes?.minutes ?? "0") + input.minutes > 1440)
    throw new ApiError(422, "PROJECT_TIME_DAILY_LIMIT", "Total submitted and approved project time cannot exceed 24 hours in one work day.");
  const now = Date.now(), version = existing ? input.expectedVersion! + 1 : 1;
  const snapshot = { projectId, workDate: input.workDate, minutes: input.minutes, state: "submitted" };
  return executeCommand(db, actor, key, operation, { projectId, ...input }, id, version,
    { id, version, state: "submitted" }, [
      staffGuard(db, key, actor.companyId, actor.userId),
      assertion(db, key, "project-member", `EXISTS(SELECT 1 FROM staff_projects p WHERE p.id=? AND p.company_id=?
        AND (iam_same_person(p.owner_user_id,?,p.company_id) OR EXISTS(SELECT 1 FROM staff_project_members m WHERE m.project_id=p.id AND iam_same_person(m.user_id,?,p.company_id))))`,
        [project.id, actor.companyId, actor.userId, actor.userId]),
      ...(existing ? [
        assertion(db, key, "project-time-version", "EXISTS(SELECT 1 FROM staff_project_work_logs WHERE id=? AND company_id=? AND version=? AND state='rejected' AND iam_same_person(user_id,?,company_id))", [id, actor.companyId, input.expectedVersion!, actor.userId]),
        assertion(db, key, "project-time-daily-limit", `COALESCE((SELECT SUM(minutes) FROM staff_project_work_logs WHERE company_id=? AND iam_same_person(user_id,?,company_id)
          AND work_date=? AND state IN ('submitted','approved') AND id!=?),0)+?<=1440`, [actor.companyId, actor.userId, input.workDate, id, input.minutes]),
        db.prepare(`UPDATE staff_project_work_logs SET minutes=?,state='submitted',reviewed_by=NULL,reviewed_at=NULL,rejection_reason='',version=version+1,updated_at=? WHERE id=?`)
          .bind(input.minutes, now, id),
      ] : [
        assertion(db, key, "project-time-date", "NOT EXISTS(SELECT 1 FROM staff_project_work_logs WHERE project_id=? AND iam_same_person(user_id,?,company_id) AND work_date=?)", [project.id, actor.userId, input.workDate]),
        assertion(db, key, "project-time-daily-limit", `COALESCE((SELECT SUM(minutes) FROM staff_project_work_logs WHERE company_id=? AND iam_same_person(user_id,?,company_id)
          AND work_date=? AND state IN ('submitted','approved') AND id!=?),0)+?<=1440`, [actor.companyId, actor.userId, input.workDate, id, input.minutes]),
        db.prepare(`INSERT INTO staff_project_work_logs(id,company_id,project_id,user_id,work_date,minutes,state,version,created_at,updated_at)
          VALUES(?,?,?,?,?,?,'submitted',1,?,?)`).bind(id, actor.companyId, project.id, actor.userId, input.workDate, input.minutes, now, now),
      ]),
      db.prepare("UPDATE staff_projects SET version=version+1,updated_at=? WHERE id=? AND company_id=?").bind(now, project.id, actor.companyId),
      event(db, "work_log", id, version, existing ? "revised" : "submitted", actor,
        existing ? "Revised rejected project time entry." : "Submitted project time entry for owner review.", snapshot, now),
    ], { companyId: project.company_id, branchId: project.branch_id, departmentId: project.department_id });
}

export async function decideProjectWorkLog(db: SqlDatabase, actor: StaffActor, projectId: string, logId: string,
  input: z.infer<typeof projectWorkLogDecisionSchema>, key: string) {
  const operation = `project.actual-time.${input.action}`;
  const prior = await replayCommand<{ id: string; version: number; state: string }>(db, actor, key, operation, logId, { projectId, ...input });
  if (prior) return prior;
  const project = await getProject(db, actor, projectId);
  if (!samePerson(actor, project.owner_user_id)) throw new ApiError(403, "PROJECT_OWNER_REQUIRED", "Only the project owner can review project time.");
  await activeStaff(db, actor.companyId, actor.userId);
  const log = await db.prepare("SELECT * FROM staff_project_work_logs WHERE id=? AND company_id=? AND project_id=?")
    .bind(logId, actor.companyId, projectId).first<ProjectWorkLogRow>();
  if (!log) throw new ApiError(404, "PROJECT_TIME_NOT_FOUND", "Project time entry not found.");
  if (samePerson(actor, log.user_id)) throw new ApiError(409, "SELF_REVIEW_FORBIDDEN", "A different project owner must review this time entry.");
  if (log.state !== "submitted" || log.version !== input.expectedVersion)
    throw new ApiError(409, "PROJECT_TIME_REVIEW_STALE", "Refresh the project time entry before reviewing it.");
  const state = input.action === "approve" ? "approved" : "rejected", version = input.expectedVersion + 1, now = Date.now();
  const snapshot = { projectId, userId: log.user_id, workDate: log.work_date, minutes: log.minutes, state };
  return executeCommand(db, actor, key, operation, { projectId, ...input }, logId, version, { id: logId, version, state }, [
    staffGuard(db, key, actor.companyId, actor.userId),
    assertion(db, key, "project-owner", "EXISTS(SELECT 1 FROM staff_projects WHERE id=? AND company_id=? AND version=? AND iam_same_person(owner_user_id,?,company_id))", [project.id, actor.companyId, project.version, actor.userId]),
    assertion(db, key, "project-time-version", "EXISTS(SELECT 1 FROM staff_project_work_logs WHERE id=? AND company_id=? AND project_id=? AND version=? AND state='submitted' AND NOT iam_same_person(user_id,?,company_id))", [logId, actor.companyId, projectId, input.expectedVersion, actor.userId]),
    db.prepare(`UPDATE staff_project_work_logs SET state=?,reviewed_by=?,reviewed_at=?,rejection_reason=?,version=version+1,updated_at=? WHERE id=?`)
      .bind(state, state === "approved" ? actor.userId : null, state === "approved" ? now : null,
        state === "rejected" ? input.reason : "", now, logId),
    db.prepare("UPDATE staff_projects SET version=version+1,updated_at=? WHERE id=? AND company_id=?").bind(now, project.id, actor.companyId),
    event(db, "work_log", logId, version, state, actor, input.reason, snapshot, now),
  ], { companyId: project.company_id, branchId: project.branch_id, departmentId: project.department_id });
}

function financeProject(actor: WorkContext, permission: "finance.read" | "finance.write" | "finance.approve") {
  if (!hasPermission(actor, permission)) throw new ApiError(403, "FINANCE_PERMISSION_REQUIRED", `The ${permission} permission is required for project cost actuals.`);
  return requirePermission(actor, permission);
}

async function visibleFinanceProject(actor: WorkContext, projectId: string) {
  const { write } = await holdingResources(actor.holdingId);
  const project = await getProject(write, actor, projectId) as ProjectFinanceScope;
  await assertFinanceProjectAccess(actor, project);
  return { write, project };
}

export async function listProjectCostAllocations(actor: WorkContext, projectId: string) {
  actor = financeProject(actor, "finance.read");
  const { write, project } = await visibleFinanceProject(actor, projectId);
  return (await write.prepare(`SELECT a.id,a.project_id,a.journal_line_id,a.amount_minor::text amount_minor,a.state,a.version,
      a.created_by,a.approved_by,a.rejection_reason,a.created_at,a.updated_at,j.id journal_id,j.journal_number,
      NOT iam_same_person(a.created_by,?,a.company_id) can_review,
      iam_same_person(a.created_by,?,a.company_id) is_creator,
      j.entry_date,j.currency,j.status journal_status,ac.code account_code,ac.name account_name
    FROM staff_project_cost_allocations a JOIN finance_journal_lines l ON l.id=a.journal_line_id
    JOIN finance_journals j ON j.id=l.journal_id JOIN finance_accounts ac ON ac.id=l.account_id
    WHERE a.company_id=? AND a.project_id=? ORDER BY j.entry_date DESC,j.journal_number DESC,a.id`)
    .bind(actor.userId, actor.userId, actor.companyId, project.id).all<Record<string, string | number | boolean | null>>()).results.map((row) => ({
      ...row, amount: minorToDecimal(Number(row.amount_minor)),
    })) as ProjectCostAllocationRow[];
}

export async function listProjectCostSources(actor: WorkContext, projectId: string) {
  actor = financeProject(actor, "finance.read");
  const { write, project } = await visibleFinanceProject(actor, projectId);
  const rows = (await write.prepare(`SELECT l.id source_line_id,j.id journal_id,j.journal_number,j.entry_date,j.currency,
      l.account_id,ac.code account_code,ac.name account_name,l.debit_minor::text debit_minor,
      (l.debit_minor-COALESCE((SELECT SUM(a.amount_minor) FROM staff_project_cost_allocations a
        WHERE a.journal_line_id=l.id AND a.state IN ('draft','approved')),0))::text remaining_minor
    FROM finance_journal_lines l JOIN finance_journals j ON j.id=l.journal_id
    JOIN finance_accounts ac ON ac.id=l.account_id
    WHERE j.company_id=? AND j.status='posted' AND l.debit_minor>0 AND ac.category IN ('expense','other_expense')
      AND (?::text IS NULL OR l.dimension_json::jsonb->>'branchId'=?)
      AND (?::text IS NULL OR l.dimension_json::jsonb->>'departmentId'=?)
      AND (l.dimension_json::jsonb->>'branchId' IS NULL OR ?::text IS NULL OR l.dimension_json::jsonb->>'branchId'=?)
      AND (l.dimension_json::jsonb->>'departmentId' IS NULL OR ?::text IS NULL OR l.dimension_json::jsonb->>'departmentId'=?)
      AND l.debit_minor>COALESCE((SELECT SUM(a.amount_minor) FROM staff_project_cost_allocations a
        WHERE a.journal_line_id=l.id AND a.state IN ('draft','approved')),0)
    ORDER BY j.entry_date DESC,j.journal_number DESC,l.ordinal LIMIT 250`)
    .bind(actor.companyId, actor.branchId, actor.branchId, actor.departmentId, actor.departmentId,
      project.branch_id, project.branch_id, project.department_id, project.department_id)
    .all<Record<string, string | number>>()).results;
  return rows.map((row) => ({ ...row, debit: minorToDecimal(Number(row.debit_minor)), remaining: minorToDecimal(Number(row.remaining_minor)) })) as ProjectCostSourceRow[];
}

export async function createProjectCostAllocation(actor: WorkContext, input: z.infer<typeof projectCostAllocationSchema>, key: string) {
  actor = financeProject(actor, "finance.write");
  const { write, project } = await visibleFinanceProject(actor, input.projectId);
  const amountMinor = decimalToMinor(input.amount);
  if (amountMinor <= 0) throw new ApiError(422, "INVALID_PROJECT_COST", "Project cost allocations must be greater than zero.");
  const line = await write.prepare(`SELECT l.id,l.debit_minor,j.currency,j.entry_date,j.status journal_status,ac.category
    FROM finance_journal_lines l JOIN finance_journals j ON j.id=l.journal_id JOIN finance_accounts ac ON ac.id=l.account_id
    WHERE l.id=? AND j.company_id=? AND (?::text IS NULL OR l.dimension_json::jsonb->>'branchId'=?)
      AND (?::text IS NULL OR l.dimension_json::jsonb->>'departmentId'=?)
      AND (l.dimension_json::jsonb->>'branchId' IS NULL OR ?::text IS NULL OR l.dimension_json::jsonb->>'branchId'=?)
      AND (l.dimension_json::jsonb->>'departmentId' IS NULL OR ?::text IS NULL OR l.dimension_json::jsonb->>'departmentId'=?)`)
    .bind(input.sourceLineId, actor.companyId, actor.branchId, actor.branchId, actor.departmentId, actor.departmentId,
      project.branch_id, project.branch_id, project.department_id, project.department_id)
    .first<{ id: string; debit_minor: string; currency: string; entry_date: string; journal_status: string; category: string }>();
  if (!line || line.journal_status !== "posted" || !["expense", "other_expense"].includes(line.category))
    throw new ApiError(422, "PROJECT_COST_SOURCE_INVALID", "Choose a debit line from a posted expense journal in the current Company.");
  const existing = await write.prepare("SELECT * FROM staff_project_cost_allocations WHERE journal_line_id=? AND project_id=?")
    .bind(line.id, project.id).first<Record<string, string | number | null>>();
  const id = existing ? String(existing.id) : `project-cost-${key}`;
  const operation = existing ? "project.cost-allocation.revise" : "project.cost-allocation.create";
  const prior = await replayCommand<{ id: string; version: number; state: string }>(write, actor, key, operation, id, input);
  if (prior) return prior;
  if (existing && (existing.state !== "rejected" || Number(existing.version) !== input.expectedVersion || !samePerson(actor, String(existing.created_by))))
    throw new ApiError(409, "PROJECT_COST_NOT_EDITABLE", "Only the finance user who created a rejected allocation can revise it.");
  const version = existing ? input.expectedVersion! + 1 : 1, now = Date.now();
  const reason = existing ? "Revised rejected project cost allocation." : "Submitted posted expense line for project cost review.";
  const snapshot = { projectId: project.id, journalLineId: line.id, amountMinor: String(amountMinor), currency: line.currency, state: "draft" };
  return executeCommand(write, actor, key, operation, input, id, version, { id, version, state: "draft" }, [
    assertion(write, key, "project-finance-scope", `EXISTS(SELECT 1 FROM staff_projects WHERE id=? AND company_id=?
      AND (?::text IS NULL OR branch_id=?) AND (?::text IS NULL OR department_id=?))`,
      [project.id, actor.companyId, actor.branchId, actor.branchId, actor.departmentId, actor.departmentId]),
    assertion(write, key, "posted-expense-source", `EXISTS(SELECT 1 FROM finance_journal_lines l JOIN finance_journals j ON j.id=l.journal_id
      JOIN finance_accounts ac ON ac.id=l.account_id JOIN staff_projects p ON p.id=? AND p.company_id=?
      WHERE l.id=? AND j.company_id=? AND j.status='posted'
      AND (?::text IS NULL OR l.dimension_json::jsonb->>'branchId'=? AND p.branch_id=?)
      AND (?::text IS NULL OR l.dimension_json::jsonb->>'departmentId'=? AND p.department_id=?)
      AND (l.dimension_json::jsonb->>'branchId' IS NULL OR p.branch_id IS NULL OR l.dimension_json::jsonb->>'branchId'=p.branch_id)
      AND (l.dimension_json::jsonb->>'departmentId' IS NULL OR p.department_id IS NULL OR l.dimension_json::jsonb->>'departmentId'=p.department_id)
      AND l.debit_minor>0 AND ac.category IN ('expense','other_expense'))`, [project.id, actor.companyId, line.id, actor.companyId,
      actor.branchId, actor.branchId, project.branch_id, actor.departmentId, actor.departmentId, project.department_id]),
    assertion(write, key, "project-cost-cap", `EXISTS(SELECT 1 FROM finance_journal_lines l JOIN finance_journals j ON j.id=l.journal_id
      WHERE l.id=? AND j.status='posted' AND l.debit_minor>=?+COALESCE((SELECT SUM(a.amount_minor)
        FROM staff_project_cost_allocations a WHERE a.journal_line_id=l.id AND a.state IN ('draft','approved') AND a.id!=?),0))`,
      [line.id, amountMinor, id]),
    ...(existing ? [
      assertion(write, key, "allocation-version", "EXISTS(SELECT 1 FROM staff_project_cost_allocations WHERE id=? AND company_id=? AND version=? AND state='rejected' AND created_by=?)", [id, actor.companyId, input.expectedVersion!, actor.userId]),
      write.prepare(`UPDATE staff_project_cost_allocations SET amount_minor=?,state='draft',approved_by=NULL,rejected_by=NULL,rejection_reason='',version=version+1,updated_at=? WHERE id=?`)
        .bind(amountMinor, now, id),
    ] : [
      assertion(write, key, "allocation-unique", "NOT EXISTS(SELECT 1 FROM staff_project_cost_allocations WHERE journal_line_id=? AND project_id=?)", [line.id, project.id]),
      write.prepare(`INSERT INTO staff_project_cost_allocations(id,company_id,project_id,journal_line_id,amount_minor,state,version,created_by,created_at,updated_at)
        VALUES(?,?,?,?,?,'draft',1,?,?,?)`).bind(id, actor.companyId, project.id, line.id, amountMinor, actor.userId, now, now),
    ]),
    event(write, "cost_allocation", id, version, existing ? "revised" : "created", actor, reason, snapshot, now),
  ], { companyId: project.company_id, branchId: project.branch_id, departmentId: project.department_id });
}

export async function decideProjectCostAllocation(actor: WorkContext, allocationId: string,
  input: z.infer<typeof projectCostAllocationDecisionSchema>, key: string) {
  actor = financeProject(actor, "finance.approve");
  const { write } = await holdingResources(actor.holdingId);
  const operation = `project.cost-allocation.${input.action}`;
  const prior = await replayCommand<{ id: string; version: number; state: string }>(write, actor, key, operation, allocationId, input);
  if (prior) return prior;
  const row = await write.prepare(`SELECT a.*,p.owner_user_id,p.branch_id,p.department_id,j.status journal_status
    FROM staff_project_cost_allocations a JOIN staff_projects p ON p.id=a.project_id AND p.company_id=a.company_id
    JOIN finance_journal_lines l ON l.id=a.journal_line_id JOIN finance_journals j ON j.id=l.journal_id
    WHERE a.id=? AND a.company_id=?`).bind(allocationId, actor.companyId)
    .first<Record<string, string | number | null>>();
  if (!row) throw new ApiError(404, "PROJECT_COST_NOT_FOUND", "Project cost allocation not found.");
  if ((actor.branchId && actor.branchId !== row.branch_id) || (actor.departmentId && actor.departmentId !== row.department_id))
    throw new ApiError(404, "PROJECT_COST_NOT_FOUND", "Project cost allocation is not in the current finance scope.");
  if (samePerson(actor, String(row.created_by))) throw new ApiError(409, "SELF_REVIEW_FORBIDDEN", "A different Finance approver must review this allocation.");
  if (row.state !== "draft" || Number(row.version) !== input.expectedVersion)
    throw new ApiError(409, "PROJECT_COST_REVIEW_STALE", "Refresh the project cost allocation before reviewing it.");
  if (input.action === "approve" && row.journal_status !== "posted")
    throw new ApiError(409, "PROJECT_COST_SOURCE_STALE", "The source journal is no longer posted; this allocation cannot be approved.");
  const state = input.action === "approve" ? "approved" : "rejected", version = input.expectedVersion + 1, now = Date.now();
  const snapshot = { projectId: row.project_id, journalLineId: row.journal_line_id, amountMinor: String(row.amount_minor), state };
  return executeCommand(write, actor, key, operation, input, allocationId, version, { id: allocationId, version, state }, [
    assertion(write, key, "allocation-version", `EXISTS(SELECT 1 FROM staff_project_cost_allocations a
      JOIN finance_journal_lines l ON l.id=a.journal_line_id JOIN finance_journals j ON j.id=l.journal_id
      WHERE a.id=? AND a.company_id=? AND a.version=? AND a.state='draft' AND NOT iam_same_person(a.created_by,?,a.company_id)
      AND (?='reject' OR j.status='posted'))`, [allocationId, actor.companyId, input.expectedVersion, actor.userId, input.action]),
    write.prepare(`UPDATE staff_project_cost_allocations SET state=?,approved_by=?,rejected_by=?,rejection_reason=?,version=version+1,updated_at=? WHERE id=?`)
      .bind(state, state === "approved" ? actor.userId : null, state === "rejected" ? actor.userId : null,
        state === "rejected" ? input.reason : "", now, allocationId),
    event(write, "cost_allocation", allocationId, version, state, actor, input.reason, snapshot, now),
  ], { companyId: String(row.company_id), branchId: row.branch_id as string | null, departmentId: row.department_id as string | null });
}
