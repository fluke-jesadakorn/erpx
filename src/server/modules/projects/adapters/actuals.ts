import { projectReadProjection as projectfinanceProjection } from '../../finance/public/project-read-projections';
import 'server-only';
import { z } from 'zod';
import type { SqlDatabase, SqlStatement } from '../../runtime/public/runtime/types';
import type { WorkContext } from '../../identity_access/public/authorization';
import { holdingResources } from '../../runtime/public/runtime';
import { ApiError } from '../../runtime/public/http';
import { assertion, replayCommand, executeCommand } from '../../workflow/public/commands';
import { requirePermission } from '../../identity_access/public/iam-permissions';
import { activeStaff, staffGuard, type StaffActor } from '../../operations/public/staff-policy';
import { getProject } from '../public/staff-projects';
import { BusinessRuleError } from '../../shared';
import { projectWorkLogSchema, projectWorkLogDecisionSchema, projectCostAllocationSchema, projectCostAllocationDecisionSchema, type ProjectWorkLogRow } from '../domain/actuals';
import {requireProjectPurchasedRead,withProjectPurchasedCommand} from './purchases';
import { createProjectActualsApplication, type ProjectActualsRepository } from '../application/actuals';
import { projectactualsSources as owner0 } from '../../workflow/public/project-actuals-sources';
import { projectactualsSources as owner1 } from '../../finance/public/project-actuals-sources';
async function inward<T>(action: () => Promise<T>) { try {
    return await action();
}
catch (error) {
    if (error instanceof ApiError)
        throw new BusinessRuleError(error.status, error.code, error.message);
    throw error;
} }
async function exposed<T>(action: () => Promise<T>) { try {
    return await action();
}
catch (error) {
    if (error instanceof BusinessRuleError) {
        const out = new ApiError(error.status, error.code, error.message);
        out.cause = error.cause;
        throw out;
    }
    throw error;
} }
function application(db: SqlDatabase) {
    const repository: ProjectActualsRepository<SqlStatement> = { prepareActualEvent(value: string, id: string, version: number, action: string, userId: string, reason: string, value2: string, now: number, table: string) {
            const allocation = table === 'cost_allocation', eventTable = allocation ? 'staff_project_cost_allocation_events' : 'staff_project_work_log_events', idField = allocation ? 'allocation_id' : 'work_log_id', versionField = allocation ? 'allocation_version' : 'work_log_version';
            return db.prepare(`INSERT INTO ${eventTable}(id,${idField},${versionField},action,actor_id,reason,snapshot_json,created_at)
    VALUES(?,?,?,?,?,?,?::jsonb,?)`).bind(value, id, version, action, userId, reason, value2, now);
        },
        async workLogs(userId: string, userId2: string, userId3: string, companyId: string, id: string, userId4: string, userId5: string) {
            return db.prepare(`SELECT l.id,l.project_id,l.user_id,l.work_date,l.minutes,l.state,l.version,l.reviewed_by,
      l.rejection_reason,l.created_at,l.updated_at,
      iam_same_person(l.user_id,?,p.company_id) is_actor,
      (iam_same_person(p.owner_user_id,?,p.company_id) AND NOT iam_same_person(l.user_id,?,p.company_id)) can_review
    FROM staff_project_work_logs l JOIN staff_projects p ON p.id=l.project_id AND p.company_id=l.company_id
    WHERE l.company_id=? AND l.project_id=? AND (iam_same_person(p.owner_user_id,?,p.company_id) OR iam_same_person(l.user_id,?,p.company_id))
    ORDER BY l.work_date DESC,l.id`)
                .bind(userId, userId2, userId3, companyId, id, userId4, userId5).all<ProjectWorkLogRow>();
        },
        async workLogForDate(companyId: string, projectId: string, userId: string, workDate: string) {
            return db.prepare(`SELECT * FROM staff_project_work_logs WHERE company_id=? AND project_id=? AND iam_same_person(user_id,?,company_id) AND work_date=? LIMIT 1`)
                .bind(companyId, projectId, userId, workDate).first<ProjectWorkLogRow>();
        },
        async dailyMinutes(companyId: string, userId: string, workDate: string, id: string) {
            return db.prepare(`SELECT COALESCE(SUM(minutes),0)::text minutes FROM staff_project_work_logs
    WHERE company_id=? AND iam_same_person(user_id,?,company_id) AND work_date=? AND state IN ('submitted','approved') AND id!=?`)
                .bind(companyId, userId, workDate, id).first<{
                minutes: string;
            }>();
        },
        guardWorkMember(key: string, value: "project-member", id: string, companyId: string, userId: string, userId2: string) {
            return assertion(db, key, value, `EXISTS(SELECT 1 FROM staff_projects p WHERE p.id=? AND p.company_id=?
        AND (iam_same_person(p.owner_user_id,?,p.company_id) OR EXISTS(SELECT 1 FROM staff_project_members m WHERE m.project_id=p.id AND iam_same_person(m.user_id,?,p.company_id))))`, [id, companyId, userId, userId2]);
        },
        guardTimeRevision(key: string, value: "project-time-version", id: string, companyId: string, value2: number, userId: string) { return assertion(db, key, value, "EXISTS(SELECT 1 FROM staff_project_work_logs WHERE id=? AND company_id=? AND version=? AND state='rejected' AND iam_same_person(user_id,?,company_id))", [id, companyId, value2, userId]); },
        guardDailyRevision(key: string, value: "project-time-daily-limit", companyId: string, userId: string, workDate: string, id: string, minutes: number) {
            return assertion(db, key, value, `COALESCE((SELECT SUM(minutes) FROM staff_project_work_logs WHERE company_id=? AND iam_same_person(user_id,?,company_id)
          AND work_date=? AND state IN ('submitted','approved') AND id!=?),0)+?<=1440`, [companyId, userId, workDate, id, minutes]);
        },
        prepareTimeRevision(minutes: number, now: number, id: string) {
            return db.prepare(`UPDATE staff_project_work_logs SET minutes=?,state='submitted',reviewed_by=NULL,reviewed_at=NULL,rejection_reason='',version=version+1,updated_at=? WHERE id=?`)
                .bind(minutes, now, id);
        },
        guardUniqueDate(key: string, value: "project-time-date", id: string, userId: string, workDate: string) { return assertion(db, key, value, "NOT EXISTS(SELECT 1 FROM staff_project_work_logs WHERE project_id=? AND iam_same_person(user_id,?,company_id) AND work_date=?)", [id, userId, workDate]); },
        guardDailySubmission(key: string, value: "project-time-daily-limit", companyId: string, userId: string, workDate: string, id: string, minutes: number) {
            return assertion(db, key, value, `COALESCE((SELECT SUM(minutes) FROM staff_project_work_logs WHERE company_id=? AND iam_same_person(user_id,?,company_id)
          AND work_date=? AND state IN ('submitted','approved') AND id!=?),0)+?<=1440`, [companyId, userId, workDate, id, minutes]);
        },
        prepareTime(id: string, companyId: string, id2: string, userId: string, workDate: string, minutes: number, now: number, now2: number) {
            return db.prepare(`INSERT INTO staff_project_work_logs(id,company_id,project_id,user_id,work_date,minutes,state,version,created_at,updated_at)
          VALUES(?,?,?,?,?,?,'submitted',1,?,?)`).bind(id, companyId, id2, userId, workDate, minutes, now, now2);
        },
        prepareTimeProjectVersion(now: number, id: string, companyId: string) { return db.prepare("UPDATE staff_projects SET version=version+1,updated_at=? WHERE id=? AND company_id=?").bind(now, id, companyId); },
        async workLog(logId: string, companyId: string, projectId: string) {
            return db.prepare("SELECT * FROM staff_project_work_logs WHERE id=? AND company_id=? AND project_id=?")
                .bind(logId, companyId, projectId).first<ProjectWorkLogRow>();
        },
        guardTimeReviewer(key: string, value: "project-owner", id: string, companyId: string, version: number, userId: string) { return assertion(db, key, value, "EXISTS(SELECT 1 FROM staff_projects WHERE id=? AND company_id=? AND version=? AND iam_same_person(owner_user_id,?,company_id))", [id, companyId, version, userId]); },
        guardTimeVersion(key: string, value: "project-time-version", logId: string, companyId: string, projectId: string, expectedVersion: number, userId: string) { return assertion(db, key, value, "EXISTS(SELECT 1 FROM staff_project_work_logs WHERE id=? AND company_id=? AND project_id=? AND version=? AND state='submitted' AND NOT iam_same_person(user_id,?,company_id))", [logId, companyId, projectId, expectedVersion, userId]); },
        prepareTimeDecision(state: "approved" | "rejected", value: string | null, value2: number | null, value3: string, now: number, logId: string) {
            return db.prepare(`UPDATE staff_project_work_logs SET state=?,reviewed_by=?,reviewed_at=?,rejection_reason=?,version=version+1,updated_at=? WHERE id=?`)
                .bind(state, value, value2, value3, now, logId);
        },
        prepareTimeDecisionProjectVersion(now: number, id: string, companyId: string) { return db.prepare("UPDATE staff_projects SET version=version+1,updated_at=? WHERE id=? AND company_id=?").bind(now, id, companyId); },
        async costAllocations(userId: string, userId2: string, companyId: string, id: string) {
            return db.prepare(`SELECT a.id,a.project_id,a.journal_line_id,a.amount_minor::text amount_minor,a.state,a.version,
      a.created_by,a.approved_by,a.rejection_reason,a.created_at,a.updated_at,j.id journal_id,j.journal_number,
      NOT iam_same_person(a.created_by,?,a.company_id) can_review,
      iam_same_person(a.created_by,?,a.company_id) is_creator,
      j.entry_date,j.currency,j.status journal_status,ac.code account_code,ac.name account_name
    FROM staff_project_cost_allocations a ${projectfinanceProjection.allocationLine}
    ${projectfinanceProjection.expenseJournal} ${projectfinanceProjection.expenseAccount}
    WHERE a.company_id=? AND a.project_id=? ORDER BY j.entry_date DESC,j.journal_number DESC,a.id`)
                .bind(userId, userId2, companyId, id).all<Record<string, string | number | boolean | null>>();
        },
        async existingAllocation(id: string, id2: string) {
            return db.prepare("SELECT * FROM staff_project_cost_allocations WHERE journal_line_id=? AND project_id=?")
                .bind(id, id2).first<Record<string, string | number | null>>();
        },
        guardCostScope(key: string, value: "project-finance-scope", id: string, companyId: string, branchId: string | null, branchId2: string | null, departmentId: string | null, departmentId2: string | null) {
            return assertion(db, key, value, `EXISTS(SELECT 1 FROM staff_projects WHERE id=? AND company_id=?
      AND (?::text IS NULL OR branch_id=?) AND (?::text IS NULL OR department_id=?))`, [id, companyId, branchId, branchId2, departmentId, departmentId2]);
        },
        guardAllocationRevision(key: string, value: "allocation-version", id: string, companyId: string, value2: number, userId: string) { return assertion(db, key, value, "EXISTS(SELECT 1 FROM staff_project_cost_allocations WHERE id=? AND company_id=? AND version=? AND state='rejected' AND created_by=?)", [id, companyId, value2, userId]); },
        prepareAllocationRevision(amountMinor: number, now: number, id: string) {
            return db.prepare(`UPDATE staff_project_cost_allocations SET amount_minor=?,state='draft',approved_by=NULL,rejected_by=NULL,rejection_reason='',version=version+1,updated_at=? WHERE id=?`)
                .bind(amountMinor, now, id);
        },
        guardUniqueAllocation(key: string, value: "allocation-unique", id: string, id2: string) { return assertion(db, key, value, "NOT EXISTS(SELECT 1 FROM staff_project_cost_allocations WHERE journal_line_id=? AND project_id=?)", [id, id2]); },
        prepareAllocation(id: string, companyId: string, id2: string, id3: string, amountMinor: number, userId: string, now: number, now2: number) {
            return db.prepare(`INSERT INTO staff_project_cost_allocations(id,company_id,project_id,journal_line_id,amount_minor,state,version,created_by,created_at,updated_at)
        VALUES(?,?,?,?,?,'draft',1,?,?,?)`).bind(id, companyId, id2, id3, amountMinor, userId, now, now2);
        },
        async allocationReviewFacts(allocationId: string, companyId: string) {
            return db.prepare(`SELECT a.*,p.owner_user_id,p.branch_id,p.department_id,j.status journal_status
    FROM staff_project_cost_allocations a JOIN staff_projects p ON p.id=a.project_id AND p.company_id=a.company_id
    ${projectfinanceProjection.allocationLine} ${projectfinanceProjection.expenseJournal}
    WHERE a.id=? AND a.company_id=?`).bind(allocationId, companyId)
                .first<Record<string, string | number | null>>();
        },
        guardAllocationReviewVersion(key: string, value: "allocation-version", allocationId: string, companyId: string, expectedVersion: number, userId: string, action: "approve" | "reject") {
            return assertion(db, key, value, `EXISTS(SELECT 1 FROM staff_project_cost_allocations a
      ${projectfinanceProjection.allocationLine} ${projectfinanceProjection.expenseJournal}
      WHERE a.id=? AND a.company_id=? AND a.version=? AND a.state='draft' AND NOT iam_same_person(a.created_by,?,a.company_id)
      AND (?='reject' OR j.status='posted'))`, [allocationId, companyId, expectedVersion, userId, action]);
        },
        prepareAllocationDecision(state: "approved" | "rejected", value: string | null, value2: string | null, value3: string, now: number, allocationId: string) {
            return db.prepare(`UPDATE staff_project_cost_allocations SET state=?,approved_by=?,rejected_by=?,rejection_reason=?,version=version+1,updated_at=? WHERE id=?`)
                .bind(state, value, value2, value3, now, allocationId);
        }, ...owner0(db), ...owner1(db) };
    return createProjectActualsApplication({ repository, clock: { now: () => Date.now() }, commands: { replay: (...args) => inward(() => replayCommand(db, ...args)), commit: (...args) => inward(() => executeCommand(db, ...args)) }, getProject: (...args) => inward(() => getProject(db, ...args)), activeStaff: (...args) => inward(() => activeStaff(db, ...args)), staffGuard: (...args) => staffGuard(db, ...args), requirePermission: (...args) => requirePermission(...args) });
}
export async function listProjectWorkLogs(db: SqlDatabase, actor: StaffActor, projectId: string) {await requireProjectPurchasedRead(db,actor,projectId,'projects.time');return exposed(() => application(db).listProjectWorkLogs(actor, projectId)); }
export async function saveProjectWorkLog(db: SqlDatabase, actor: StaffActor, projectId: string, input: z.infer<typeof projectWorkLogSchema>, key: string) {return withProjectPurchasedCommand(db,actor,{key,operations:['project.actual-time.submit','project.actual-time.revise'],projectId,capability:'projects.time',purpose:'new-work',existing:true,target:{kind:'work_date',date:input.workDate}},tx=>exposed(() => application(tx).saveProjectWorkLog(actor, projectId, input, key))); }
export async function decideProjectWorkLog(db: SqlDatabase, actor: StaffActor, projectId: string, logId: string, input: z.infer<typeof projectWorkLogDecisionSchema>, key: string) {return withProjectPurchasedCommand(db,actor,{key,operations:[`project.actual-time.${input.action}`],projectId,capability:'projects.time',purpose:'settle-existing',existing:true,target:{kind:'work_log',id:logId}},tx=>exposed(() => application(tx).decideProjectWorkLog(actor, projectId, logId, input, key))); }
export async function listProjectCostAllocations(actor: WorkContext, projectId: string) { const { write: db } = await holdingResources(actor.holdingId);await requireProjectPurchasedRead(db,actor,projectId,'projects.costs');return exposed(() => application(db).listProjectCostAllocations(actor, projectId)); }
export async function listProjectCostSources(actor: WorkContext, projectId: string) { const { write: db } = await holdingResources(actor.holdingId);await requireProjectPurchasedRead(db,actor,projectId,'projects.costs');return exposed(() => application(db).listProjectCostSources(actor, projectId)); }
export async function createProjectCostAllocation(actor: WorkContext, input: z.infer<typeof projectCostAllocationSchema>, key: string) { const { write: db } = await holdingResources(actor.holdingId);return withProjectPurchasedCommand(db,actor,{key,operations:['project.cost-allocation.create','project.cost-allocation.revise'],projectId:input.projectId,capability:'projects.costs',purpose:'new-work',permission:'finance.write',existing:true,target:{kind:'cost_line',id:input.sourceLineId}},tx=>exposed(() => application(tx).createProjectCostAllocation(actor, input, key))); }
export async function decideProjectCostAllocation(actor: WorkContext, allocationId: string, input: z.infer<typeof projectCostAllocationDecisionSchema>, key: string) {
 const {write:db}=await holdingResources(actor.holdingId),row=await db.prepare('SELECT project_id FROM staff_project_cost_allocations WHERE id=? AND company_id=?').bind(allocationId,actor.companyId).first<{project_id:string}>();
 if(!row)throw new ApiError(404,'PROJECT_COST_NOT_FOUND','Project cost allocation not found.');
 return withProjectPurchasedCommand(db,actor,{key,operations:[`project.cost-allocation.${input.action}`],projectId:row.project_id,capability:'projects.costs',purpose:'settle-existing',permission:'finance.approve',existing:true,member:false,target:{kind:'allocation',id:allocationId}},async tx=>{
  const actual=await tx.prepare('SELECT project_id FROM staff_project_cost_allocations WHERE id=? AND company_id=?').bind(allocationId,actor.companyId).first<{project_id:string}>();
  if(actual?.project_id!==row.project_id)throw new ApiError(409,'PROJECT_SOURCE_CHANGED','The Project cost source changed.');
  return exposed(()=>application(tx).decideProjectCostAllocation(actor,allocationId,input,key));
 });
}
