import {legalMetricReadScope} from "@erpx/legal-server/adapters/metric-purchases";
import { readMetricSources } from '@erpx/platform-runtime-server/public/executive/metric-sources';
import { visibility as legalVisibility } from "@erpx/legal-server/public/legal/access";
import { legalMetricCountPrefix } from './legal-read-tail-r38-sources';
import { legalExecutiveObligationSpineHead, legalExecutiveHoldSpineHead } from './legal-read-tail-r46-sources';
import "server-only";
import { type Breakdown, type Filters, type MetricId } from "@erpx/platform-shared-ui/lib/executive/canvas";
import type { WorkContext } from "@erpx/identity-access-server/public/authorization";
import type { SqlDatabase } from '@erpx/platform-runtime-server/public/runtime/types';
import { specs } from '@erpx/platform-runtime-server/public/executive/metric-projection-contract';
import type { MetricSnapshot, RawMetricRow } from '@erpx/platform-runtime-server/public/executive/metric-projection-contract';
export async function readExecutiveMetricProjection(sourceDb: SqlDatabase, a: WorkContext, id: MetricId, f: Filters, breakdown: Breakdown): Promise<MetricSnapshot> {
    const spec = specs[id],purchase=await legalMetricReadScope(sourceDb,a,id);
    let sql = '', values: unknown[] = [];
    const referenceAtOverride: number | null = null;
    if (["legal-records", "legal-contract-value", "legal-exposure"].includes(id)) {
        const visibility = legalVisibility(a);
        const table = id === "legal-contract-value" ? "legal_contracts" : id === "legal-exposure" ? "legal_matters" : null;
        const field = id === "legal-contract-value" ? "value" : "exposure";
        sql = `SELECT i.state label,${table ? `coalesce(sum(coalesce((t.data->>'${field}')::bigint,0)),0)::text` : "count(*)::text"} value FROM legal_items i ${table ? `JOIN ${table} t ON t.item_id=i.id` : ""} WHERE ${visibility.sql} AND ${purchase.where} AND i.updated_at>=? AND i.updated_at<? ${table ? "AND i.currency=?" : ""} GROUP BY i.state ORDER BY i.state`;
        values = [...visibility.values, Date.parse(f.from), Date.parse(f.to) + 86400000, ...(table ? [f.currency] : [])];
    }
    if (id === "active-legal-obligations") {
        const visibility = legalVisibility(a);
        const label = breakdown === "status" ? "s.state"
            : breakdown === "month" ? "coalesce(to_char(to_date(s.due_date,'YYYY-MM-DD'),'YYYY-MM'),'No due date')"
                : "s.category";
        sql = `${legalExecutiveObligationSpineHead(legalMetricCountPrefix(label,'s.id'))}${visibility.sql} AND ${purchase.where} AND i.kind IN ('contract','obligation','license','ip','matter')
        AND i.state NOT IN ('archived','closed','cancelled','terminated','expired','completed')
        AND s.state NOT IN ('completed','waived','cancelled','closed','acknowledged','accepted','billed')
      GROUP BY 1 ORDER BY 1`;
        values = visibility.values;
    }
    if (id === "active-legal-holds") {
        const visibility = legalVisibility(a);
        const label = breakdown === "status" ? "h.state" : "i.kind";
        sql = `${legalExecutiveHoldSpineHead(legalMetricCountPrefix(label,'h.id'))}${visibility.sql} AND ${purchase.where} AND h.state IN ('active','release_requested')
      GROUP BY 1 ORDER BY 1`;
        values = visibility.values;
    }
    const raw = (await sourceDb.prepare(sql).bind(...values).all<RawMetricRow>()).results;
    return { raw, referenceAtOverride, source: async (group, reported) => {await legalMetricReadScope(sourceDb,a,id);return readMetricSources(sourceDb, id, sql, values, group, raw, `${spec.dateBasis} · ${f.from} to ${f.to} · ${f.currency}`, reported);}, watermarks: async () => [] };
}
