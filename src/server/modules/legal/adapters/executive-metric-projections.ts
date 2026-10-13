import { readMetricSources } from '@/server/modules/runtime/public/executive/metric-sources';
import { visibility as legalVisibility } from "@/server/modules/legal/public/legal/access";
import "server-only";
import { type Breakdown, type Filters, type MetricId } from "@/lib/shared/executive/canvas";
import type { WorkContext } from "@/server/modules/identity_access/public/authorization";
import type { SqlDatabase } from '@/server/modules/runtime/public/runtime/types';
import { specs } from '@/server/modules/runtime/public/executive/metric-projection-contract';
import type { MetricSnapshot, RawMetricRow } from '@/server/modules/runtime/public/executive/metric-projection-contract';
export async function readExecutiveMetricProjection(sourceDb: SqlDatabase, a: WorkContext, id: MetricId, f: Filters, breakdown: Breakdown): Promise<MetricSnapshot> {
    const spec = specs[id];
    let sql = '', values: unknown[] = [];
    const referenceAtOverride: number | null = null;
    if (["legal-records", "legal-contract-value", "legal-exposure"].includes(id)) {
        const visibility = legalVisibility(a);
        const table = id === "legal-contract-value" ? "legal_contracts" : id === "legal-exposure" ? "legal_matters" : null;
        const field = id === "legal-contract-value" ? "value" : "exposure";
        sql = `SELECT i.state label,${table ? `coalesce(sum(coalesce((t.data->>'${field}')::bigint,0)),0)::text` : "count(*)::text"} value FROM legal_items i ${table ? `JOIN ${table} t ON t.item_id=i.id` : ""} WHERE ${visibility.sql} AND i.updated_at>=? AND i.updated_at<? ${table ? "AND i.currency=?" : ""} GROUP BY i.state ORDER BY i.state`;
        values = [...visibility.values, Date.parse(f.from), Date.parse(f.to) + 86400000, ...(table ? [f.currency] : [])];
    }
    if (id === "active-legal-obligations") {
        const visibility = legalVisibility(a);
        const label = breakdown === "status" ? "s.state"
            : breakdown === "month" ? "coalesce(to_char(to_date(s.due_date,'YYYY-MM-DD'),'YYYY-MM'),'No due date')"
                : "s.category";
        sql = `SELECT ${label} label,count(DISTINCT s.id)::text value
      FROM legal_obligation_schedule s JOIN legal_items i ON i.id=s.item_id
      WHERE ${visibility.sql} AND i.kind IN ('contract','obligation','license','ip','matter')
        AND i.state NOT IN ('archived','closed','cancelled','terminated','expired','completed')
        AND s.state NOT IN ('completed','waived','cancelled','closed','acknowledged','accepted','billed')
      GROUP BY 1 ORDER BY 1`;
        values = visibility.values;
    }
    if (id === "active-legal-holds") {
        const visibility = legalVisibility(a);
        const label = breakdown === "status" ? "h.state" : "i.kind";
        sql = `SELECT ${label} label,count(DISTINCT h.id)::text value
      FROM legal_holds h JOIN legal_items i ON i.id=h.item_id
      WHERE ${visibility.sql} AND h.state IN ('active','release_requested')
      GROUP BY 1 ORDER BY 1`;
        values = visibility.values;
    }
    const raw = (await sourceDb.prepare(sql).bind(...values).all<RawMetricRow>()).results;
    return { raw, referenceAtOverride, source: (group, reported) => readMetricSources(sourceDb, id, sql, values, group, raw, `${spec.dateBasis} · ${f.from} to ${f.to} · ${f.currency}`, reported), watermarks: async () => [] };
}
