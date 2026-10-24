import {peoplePayrollFinalOwnerProjections as financeFinalOwnerProjections} from "@erpx/server-finance/public/people-payroll-final-owner-projections";
const financeFinalSources=financeFinalOwnerProjections();
import "server-only";
import { type Filters } from "@erpx/feature-shared/lib/executive/canvas";
import type { WorkContext } from "@erpx/server-identity_access/public/authorization";
import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
import {participantSql} from '@erpx/server-workflow/public/approval-participant-projection';
import { PayrollSummaryEvidenceRow } from '@erpx/server-runtime/public/executive/evidence-contract';
function payrollSummaryRows(db: SqlDatabase, actor: WorkContext, filters: Filters, query = "", cursor?: {
    periodEnd: string;
    id: string;
}, recordId?: string, limit = 25) {
    const conditions = [
        "r.company_id=?",
        "r.currency=?",
        "r.period_end BETWEEN ? AND ?",
        "r.status='posted'",
        `EXISTS(SELECT 1 FROM hr_payroll_postings p ${financeFinalSources.executive_main_evidence_sources_finance_journals_j_join} ON j.id=p.journal_id WHERE p.run_id=r.id AND j.company_id=r.company_id AND j.status='posted')`,
        "EXISTS(SELECT 1 FROM hr_payroll_items posted_item WHERE posted_item.run_id=r.id AND posted_item.status='posted')",
    ];
    const values: unknown[] = [actor.companyId, filters.currency, filters.from, filters.to];
    if (query) {
        conditions.push("(position(lower(?) in lower(r.id||' '||r.period_start||' '||r.period_end||' '||r.pay_date||' '||r.currency||' '||r.status))>0)");
        values.push(query);
    }
    if (cursor) {
        conditions.push("(r.period_end,r.id)<(?,?)");
        values.push(cursor.periodEnd, cursor.id);
    }
    if (recordId) {
        conditions.push("r.id=?");
        values.push(recordId);
    }
    return db.prepare(`SELECT r.id,r.period_start,r.period_end,r.pay_date,r.currency,r.status,r.version,
      COALESCE((SELECT sum(i.net_minor::numeric)::text FROM hr_payroll_items i WHERE i.run_id=r.id AND i.status='posted'),'0') net_minor
    FROM hr_payroll_runs r WHERE ${conditions.join(" AND ")}
    ORDER BY r.period_end DESC,r.id DESC LIMIT ?`).bind(...values, recordId ? 1 : limit);
}
function approvedLeaveEvidenceRows(db: SqlDatabase, actor: WorkContext, filters: Filters, query = "", cursor?: {
    date: string;
    id: string;
}, recordId?: string, limit = 25) {
    const conditions = [
        "l.company_id=?", "l.state='approved'", "l.start_date BETWEEN ? AND ?",
        "(?::text IS NULL OR l.branch_id=?)", "(?::text IS NULL OR l.department_id=?)",
        `(l.created_by=? OR l.decided_by=? OR ${participantSql("leave", "l", "?", "?")} OR ${participantSql("leave_cancellation", "l", "?", "?")})`,
    ];
    const values: unknown[] = [actor.companyId, filters.from, filters.to, actor.branchId, actor.branchId, actor.departmentId, actor.departmentId,
        actor.userId, actor.userId, actor.userId, actor.assignmentId, actor.userId, actor.assignmentId];
    if (query) {
        conditions.push("position(lower(?) in lower(l.id||' '||l.number||' '||l.start_date||' '||COALESCE(l.end_date,l.start_date)||' '||l.state))>0");
        values.push(query);
    }
    if (cursor) {
        conditions.push("(l.start_date,l.id)<(?,?)");
        values.push(cursor.date, cursor.id);
    }
    if (recordId) {
        conditions.push("l.id=?");
        values.push(recordId);
    }
    return db.prepare(`SELECT l.id,l.number,l.start_date,COALESCE(l.end_date,l.start_date) end_date,
      COALESCE(l.calculated_units::numeric,l.days::numeric)::text units,l.days,l.state,l.version
    FROM leave_requests l WHERE ${conditions.join(" AND ")}
    ORDER BY l.start_date DESC,l.id DESC LIMIT ?`).bind(...values, recordId ? 1 : limit);
}
function authorizedOvertimeEvidenceRows(db: SqlDatabase, actor: WorkContext, filters: Filters, query = "", cursor?: {
    date: string;
    id: string;
}, recordId?: string, limit = 25) {
    const conditions = [
        "o.company_id=?", "o.state='approved'", "h.work_date BETWEEN ? AND ?",
        "(?::text IS NULL OR o.branch_id=?)", "(?::text IS NULL OR o.department_id=?)",
        `(o.created_by=? OR ${participantSql("overtime", "o", "?", "?")})`,
    ];
    const values: unknown[] = [actor.companyId, filters.from, filters.to, actor.branchId, actor.branchId, actor.departmentId, actor.departmentId,
        actor.userId, actor.userId, actor.assignmentId];
    if (query) {
        conditions.push("position(lower(?) in lower(o.id||' '||h.work_date||' '||o.state))>0");
        values.push(query);
    }
    if (cursor) {
        conditions.push("(h.work_date,o.id)<(?,?)");
        values.push(cursor.date, cursor.id);
    }
    if (recordId) {
        conditions.push("o.id=?");
        values.push(recordId);
    }
    return db.prepare(`SELECT o.id,h.work_date,h.minutes::text minutes,h.request_version,o.state,o.version
    FROM overtime_payroll_handoffs h JOIN overtime_requests o ON o.id=h.request_id AND o.company_id=h.company_id
    WHERE ${conditions.join(" AND ")}
    ORDER BY h.work_date DESC,o.id DESC LIMIT ?`).bind(...values, recordId ? 1 : limit);
}
export function executiveMainEvidenceSources(db: SqlDatabase) {
    return { async factsSearchPayrollSummaryEvidence(actor: WorkContext, input: {
            filters: {
                from: string;
                to: string;
                currency: string;
            };
            query: string;
            limit: number;
            metric: "payroll-summary";
            cursor?: string | undefined;
        }, cursor: {
            periodEnd: string;
            id: string;
        } | null): Promise<{
            rows: PayrollSummaryEvidenceRow[];
        }> { const rows = (await payrollSummaryRows(db, actor, input.filters, input.query, cursor ?? undefined, undefined, input.limit + 1).all<PayrollSummaryEvidenceRow>()).results; return { rows }; }, async factsReadPayrollSummaryEvidence(actor: WorkContext, filters: {
            from: string;
            to: string;
            currency: string;
        }, recordId: string): Promise<{
            row: PayrollSummaryEvidenceRow | null;
        }> { const row = await payrollSummaryRows(db, actor, filters, "", undefined, recordId, 1).first<PayrollSummaryEvidenceRow>(); return { row }; }, async factsSearchPeopleLeaveOvertimeEvidence(leave: boolean, actor: WorkContext, input: {
            filters: {
                from: string;
                to: string;
                currency: string;
            };
            query: string;
            limit: number;
            metric: "approved-leave-duration" | "authorized-overtime-duration";
            cursor?: string | undefined;
        }, cursor: {
            date: string;
            id: string;
        } | null): Promise<{
            rows: {
                id: string;
                number?: string;
                start_date?: string;
                end_date?: string;
                units?: string;
                days?: number;
                work_date?: string;
                minutes?: string;
                request_version?: number;
                state: string;
                version: number;
            }[];
        }> {
            const rows = (await (leave
                ? approvedLeaveEvidenceRows(db, actor, input.filters, input.query, cursor ?? undefined, undefined, input.limit + 1)
                : authorizedOvertimeEvidenceRows(db, actor, input.filters, input.query, cursor ?? undefined, undefined, input.limit + 1))
                .all<{
                id: string;
                number?: string;
                start_date?: string;
                end_date?: string;
                units?: string;
                days?: number;
                work_date?: string;
                minutes?: string;
                request_version?: number;
                state: string;
                version: number;
            }>()).results;
            return { rows };
        }, async leaveEvidenceRow(actor: WorkContext, filters: Filters, recordId: string): Promise<{
            id: string;
            number: string;
            start_date: string;
            end_date: string;
            units: string;
            days: number;
            state: string;
            version: number;
        } | null> {
            return await approvedLeaveEvidenceRows(db, actor, filters, "", undefined, recordId, 1)
                .first<{
                id: string;
                number: string;
                start_date: string;
                end_date: string;
                units: string;
                days: number;
                state: string;
                version: number;
            }>();
        }, async leaveEvidenceVisibility(actor: WorkContext, recordId: string): Promise<Record<string, unknown> | null> {
            return await db.prepare(`SELECT 1 FROM leave_requests l
        WHERE l.id=? AND l.company_id=?
          AND (l.created_by=? OR l.decided_by=? OR ${participantSql("leave", "l", "?", "?")}
            OR ${participantSql("leave_cancellation", "l", "?", "?")})`)
                .bind(recordId, actor.companyId, actor.userId, actor.userId, actor.userId, actor.assignmentId, actor.userId, actor.assignmentId)
                .first();
        } };
}
