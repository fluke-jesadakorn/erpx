import {workTaskProjectionReadScope} from "@erpx/server-operations/adapters/record-purchases";
import {operationsOverdueAlertSql} from './executive-task-cohort';
import "server-only";
import type { WorkContext } from "@erpx/server-identity_access/public/authorization";
import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
import { operationsTaskCohort, operationsTaskVisibility } from "@erpx/server-runtime/public/executive/operations-access";
import { OperationsEvidenceRow } from '@erpx/server-runtime/public/executive/evidence-contract';
const overdueAlert = `(${operationsOverdueAlertSql('min(n.created_at)', 't')})`;
/** Canonical Operations evidence projection shared by the search and read
 * readers; one fixed text keeps the two SELECTs from drifting. Binds unchanged. */
const evidenceSelect = (metric: string, alertSelect: string, extra = '') =>
    `SELECT t.id,t.title,t.state,t.due_at,t.created_at,t.updated_at,t.version,t.branch_id,t.department_id,
      ${metric === "exception-resolution-time" ? "(t.updated_at-t.due_at)::text" : "NULL::text"} exception_duration_ms,
      ${alertSelect} alert_created_at${extra}`;
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
            const visibility = operationsTaskVisibility(actor, referenceAt),purchase=await workTaskProjectionReadScope(db,actor);
            const cohort = operationsTaskCohort(metric, "t", actor, input.filters, referenceAt);
            const sortColumn = metric === "exception-resolution-time" ? "updated_at" : "due_at";
            const conditions = [visibility.sql, cohort.sql,purchase.where];
            const values: unknown[] = [...visibility.values, ...cohort.values,...purchase.args];
            if (input.query) {
                conditions.push("position(lower(?) in lower(t.title))>0");
                values.push(input.query);
            }
            if (cursor) {
                conditions.push(`(t.${sortColumn},t.id)<(?,?)`);
                values.push(cursor.sortAt, cursor.id);
            }
            const alertSelect = metric === "operations-escalations"
                ? overdueAlert
                : "NULL::bigint";
            const queryValues = metric === "operations-escalations" ? [actor.userId, ...values] : values;
            const rows = (await db.prepare(`${evidenceSelect(metric, alertSelect, `,t.${sortColumn} sort_at`)}
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
            const visibility = operationsTaskVisibility(actor, referenceAt),purchase=await workTaskProjectionReadScope(db,actor);
            const cohort = operationsTaskCohort(metric, "t", actor, filters, referenceAt);
            const alertSelect = metric === "operations-escalations"
                ? overdueAlert
                : "NULL::bigint";
            const queryValues = metric === "operations-escalations" ? [actor.userId, ...visibility.values, ...cohort.values,...purchase.args, taskId] : [...visibility.values, ...cohort.values,...purchase.args, taskId];
            const row = await db.prepare(`${evidenceSelect(metric, alertSelect)}
    FROM staff_tasks t WHERE ${visibility.sql} AND ${cohort.sql} AND ${purchase.where} AND t.id=? LIMIT 1`)
                .bind(...queryValues).first<OperationsEvidenceRow>();
            return { row };
        } };
}
