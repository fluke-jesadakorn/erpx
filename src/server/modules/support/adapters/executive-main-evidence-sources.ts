import "server-only";
import type { WorkContext } from "@/server/modules/identity_access/public/authorization";
import type { SqlDatabase } from "@/server/modules/runtime/public/runtime/types";
import { SupportTicketMetricId, SupportIncidentEvidenceRow, SupportTicketEvidenceRow } from '@/server/modules/runtime/public/executive/evidence-contract';
function supportTicketStatePredicate(metric: SupportTicketMetricId, alias = "t") {
    const base = `${alias}.status NOT IN ('closed','cancelled')`;
    if (metric === "open-support-tickets")
        return base;
    return `${base} AND ${alias}.sla_clock_json->>'state'='running' AND CASE
    WHEN ${alias}.sla_clock_json->>'dueAt' ~ '^[0-9]+$'
      THEN (${alias}.sla_clock_json->>'dueAt')::numeric < floor(extract(epoch FROM clock_timestamp())*1000)::numeric
    ELSE FALSE END`;
}
export function executiveMainEvidenceSources(db: SqlDatabase) {
    return { async factsSearchSupportIncidentEvidence(cursor: {
            updatedAt: number;
            id: string;
        } | null, input: {
            filters: {
                from: string;
                to: string;
                currency: string;
            };
            query: string;
            limit: number;
            metric: "active-support-incidents";
            cursor?: string | undefined;
        }, actor: WorkContext): Promise<{
            rows: SupportIncidentEvidenceRow[];
        }> {
            const conditions = [
                "t.company_id=?", "(?::text IS NULL OR t.branch_id=?)", "(?::text IS NULL OR t.department_id=?)",
                "t.status IN ('open','investigating','mitigated')",
            ];
            const values: unknown[] = [actor.companyId, actor.branchId, actor.branchId, actor.departmentId, actor.departmentId];
            if (input.query) {
                conditions.push("position(lower(?) in lower(t.title||' '||t.severity||' '||t.status||' '||t.id))>0");
                values.push(input.query);
            }
            if (cursor) {
                conditions.push("(t.updated_at<? OR (t.updated_at=? AND t.id>?))");
                values.push(cursor.updatedAt, cursor.updatedAt, cursor.id);
            }
            const rows = (await db.prepare(`SELECT t.id,t.title,t.severity,t.status,t.due_date,t.created_at,t.updated_at,t.version
    FROM support_incidents t WHERE ${conditions.join(" AND ")} ORDER BY t.updated_at DESC,t.id ASC LIMIT ?`)
                .bind(...values, input.limit + 1).all<SupportIncidentEvidenceRow>()).results;
            return { rows };
        }, async factsSearchSupportTicketEvidence(input: {
            filters: {
                from: string;
                to: string;
                currency: string;
            };
            query: string;
            limit: number;
            metric: "open-support-tickets" | "overdue-support-sla";
            cursor?: string | undefined;
        }, cursor: {
            updatedAt: number;
            id: string;
        } | null, overdue: boolean, actor: WorkContext): Promise<{
            rows: SupportTicketEvidenceRow[];
        }> {
            const conditions = [
                "t.company_id=?", "(?::text IS NULL OR t.branch_id=?)", "(?::text IS NULL OR t.department_id=?)",
                supportTicketStatePredicate(input.metric),
            ];
            const values: unknown[] = [actor.companyId, actor.branchId, actor.branchId, actor.departmentId, actor.departmentId];
            if (input.query) {
                const querySource = overdue
                    ? "concat_ws(' ',t.id,t.title,t.status,t.due_date,t.sla_clock_json->>'state',t.sla_clock_json->>'dueAt')"
                    : "concat_ws(' ',t.id,t.title,t.status,t.due_date)";
                conditions.push(`position(lower(?) in lower(${querySource}))>0`);
                values.push(input.query);
            }
            if (cursor) {
                conditions.push("(t.updated_at<? OR (t.updated_at=? AND t.id>?))");
                values.push(cursor.updatedAt, cursor.updatedAt, cursor.id);
            }
            const rows = (await db.prepare(`SELECT t.id,t.title,t.status,t.due_date,t.updated_at,t.version${overdue ? ",t.sla_clock_json->>'state' sla_state,t.sla_clock_json->>'dueAt' sla_due_at" : ""}
    FROM support_tickets t WHERE ${conditions.join(" AND ")} ORDER BY t.updated_at DESC,t.id ASC LIMIT ?`)
                .bind(...values, input.limit + 1).all<SupportTicketEvidenceRow>()).results;
            return { rows };
        }, async factsReadSupportTicketEvidence(overdue: boolean, metric: "open-support-tickets" | "overdue-support-sla", id: string, actor: WorkContext): Promise<{
            row: SupportTicketEvidenceRow | null;
        }> {
            const row = await db.prepare(`SELECT t.id,t.title,t.status,t.due_date,t.updated_at,t.version${overdue ? ",t.sla_clock_json->>'state' sla_state,t.sla_clock_json->>'dueAt' sla_due_at" : ""}
    FROM support_tickets t WHERE t.id=? AND t.company_id=? AND (?::text IS NULL OR t.branch_id=?) AND (?::text IS NULL OR t.department_id=?) AND ${supportTicketStatePredicate(metric)} LIMIT 1`)
                .bind(id, actor.companyId, actor.branchId, actor.branchId, actor.departmentId, actor.departmentId).first<SupportTicketEvidenceRow>();
            return { row };
        } };
}
