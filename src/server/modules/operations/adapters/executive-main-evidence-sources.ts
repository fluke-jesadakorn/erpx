import { finalRootOwnerProjectionSources as collaborationProjectionSources } from "../../collaboration/public/final-root-owner-projection-sources";
import "server-only";
import type { WorkContext } from "@/server/modules/identity_access/public/authorization";
import type { SqlDatabase } from "@/server/modules/runtime/public/runtime/types";
import { operationsTaskCohort, operationsTaskVisibility } from "@/server/modules/runtime/public/executive/operations-access";
import { OperationsEvidenceRow } from '@/server/modules/runtime/public/executive/evidence-contract';
export function executiveMainEvidenceSources(db: SqlDatabase) {
    return { async factsSearchOperationsEvidence(actor: WorkContext, referenceAt: number, metric: "open-operations-exceptions" | "exception-resolution-time" | "operations-escalations", input: {
            filters: {
                from: string;
                to: string;
                currency: string;
            };
            query: string;
            limit: number;
            metric: "open-operations-exceptions" | "exception-resolution-time" | "operations-escalations";
            cursor?: string | undefined;
        }, cursor: {
            sortAt: number;
            id: string;
        } | null): Promise<{
            rows: OperationsEvidenceRow[];
        }> {
            const visibility = operationsTaskVisibility(actor, referenceAt);
            const cohort = operationsTaskCohort(metric, "t", actor, input.filters, referenceAt);
            const sortColumn = metric === "exception-resolution-time" ? "updated_at" : "due_at";
            const conditions = [visibility.sql, cohort.sql];
            const values: unknown[] = [...visibility.values, ...cohort.values];
            if (input.query) {
                conditions.push("position(lower(?) in lower(t.title))>0");
                values.push(input.query);
            }
            if (cursor) {
                conditions.push(`(t.${sortColumn},t.id)<(?,?)`);
                values.push(cursor.sortAt, cursor.id);
            }
            const alertSelect = metric === "operations-escalations"
                ? `(SELECT min(n.created_at) FROM (${collaborationProjectionSources.staff_notifications}) n WHERE n.company_id=t.company_id AND n.user_id=? AND n.record_type='staff_task_alert' AND n.record_id=t.id AND n.record_version=t.version AND n.id LIKE 'staff-task-alert-overdue-%')`
                : "NULL::bigint";
            const queryValues = metric === "operations-escalations" ? [actor.userId, ...values] : values;
            const rows = (await db.prepare(`SELECT t.id,t.title,t.state,t.due_at,t.created_at,t.updated_at,t.version,t.branch_id,t.department_id,
      ${metric === "exception-resolution-time" ? "(t.updated_at-t.due_at)::text" : "NULL::text"} exception_duration_ms,
      ${alertSelect} alert_created_at,t.${sortColumn} sort_at
    FROM staff_tasks t WHERE ${conditions.join(" AND ")} ORDER BY t.${sortColumn} DESC,t.id DESC LIMIT ?`)
                .bind(...queryValues, input.limit + 1).all<OperationsEvidenceRow>()).results;
            return { rows };
        }, async factsReadOperationsEvidence(actor: WorkContext, referenceAt: number, metric: "open-operations-exceptions" | "exception-resolution-time" | "operations-escalations", filters: {
            from: string;
            to: string;
            currency: string;
        }, taskId: string): Promise<{
            row: OperationsEvidenceRow | null;
        }> {
            const visibility = operationsTaskVisibility(actor, referenceAt);
            const cohort = operationsTaskCohort(metric, "t", actor, filters, referenceAt);
            const alertSelect = metric === "operations-escalations"
                ? `(SELECT min(n.created_at) FROM (${collaborationProjectionSources.staff_notifications}) n WHERE n.company_id=t.company_id AND n.user_id=? AND n.record_type='staff_task_alert' AND n.record_id=t.id AND n.record_version=t.version AND n.id LIKE 'staff-task-alert-overdue-%')`
                : "NULL::bigint";
            const queryValues = metric === "operations-escalations" ? [actor.userId, ...visibility.values, ...cohort.values, taskId] : [...visibility.values, ...cohort.values, taskId];
            const row = await db.prepare(`SELECT t.id,t.title,t.state,t.due_at,t.created_at,t.updated_at,t.version,t.branch_id,t.department_id,
      ${metric === "exception-resolution-time" ? "(t.updated_at-t.due_at)::text" : "NULL::text"} exception_duration_ms,
      ${alertSelect} alert_created_at
    FROM staff_tasks t WHERE ${visibility.sql} AND ${cohort.sql} AND t.id=? LIMIT 1`)
                .bind(...queryValues).first<OperationsEvidenceRow>();
            return { row };
        } };
}
