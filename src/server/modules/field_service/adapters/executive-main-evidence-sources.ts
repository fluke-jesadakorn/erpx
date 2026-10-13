import "server-only";
import { type Filters } from "@/lib/shared/executive/canvas";
import type { WorkContext } from "@/server/modules/identity_access/public/authorization";
import type { SqlDatabase } from "@/server/modules/runtime/public/runtime/types";
import { FieldVisitEvidenceRow } from '@/server/modules/runtime/public/executive/evidence-contract';
async function fieldVisitEvidenceRows(db: SqlDatabase, actor: WorkContext, filters: Filters, query: string, cursor?: {
    scheduledAt: number;
    id: string;
}, limit = 25, recordId?: string) {
    const scoped = fieldVisitConditions(actor, filters, query, cursor, recordId);
    return (await db.prepare(`SELECT o.id,o.scheduled_at,o.visit_completed_at,o.status,o.version
    FROM field_service_orders o WHERE ${scoped.conditions.join(" AND ")}
    ORDER BY o.scheduled_at DESC,o.id DESC LIMIT ?`).bind(...scoped.values, recordId ? 1 : limit + 1).all<FieldVisitEvidenceRow>()).results;
}
function fieldVisitConditions(actor: WorkContext, filters: Filters, query: string, cursor?: {
    scheduledAt: number;
    id: string;
}, recordId?: string) {
    const conditions = [
        "o.company_id=?",
        "(?::text IS NULL OR o.branch_id=?)",
        "(?::text IS NULL OR o.department_id=?)",
        "o.scheduled_at IS NOT NULL",
        "o.status IN ('scheduled','dispatched','in_visit','exception','completed','billed')",
        "o.scheduled_at>=?",
        "o.scheduled_at<?",
    ];
    const values: unknown[] = [actor.companyId, actor.branchId, actor.branchId, actor.departmentId, actor.departmentId, Date.parse(filters.from), Date.parse(filters.to) + 86400000];
    if (query) {
        conditions.push("(position(lower(?) in lower(o.id))>0 OR position(lower(?) in lower(o.status))>0)");
        values.push(query, query);
    }
    if (cursor) {
        conditions.push("(o.scheduled_at,o.id)<(?,?)");
        values.push(cursor.scheduledAt, cursor.id);
    }
    if (recordId) {
        conditions.push("o.id=?");
        values.push(recordId);
    }
    return { conditions, values };
}
export function executiveMainEvidenceSources(db: SqlDatabase) { return { async factsSearchFieldVisitEvidence(actor: WorkContext, input: {
        filters: {
            from: string;
            to: string;
            currency: string;
        };
        query: string;
        limit: number;
        metric: "field-visit-completion";
        cursor?: string | undefined;
    }, cursor: {
        scheduledAt: number;
        id: string;
    } | null): Promise<{
        rows: FieldVisitEvidenceRow[];
    }> { const rows = await fieldVisitEvidenceRows(db, actor, input.filters, input.query, cursor ?? undefined, input.limit); return { rows }; } }; }
