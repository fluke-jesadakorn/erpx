import { readMetricSources } from '@erpx/server-runtime/public/executive/metric-sources';
import "server-only";
import { type Breakdown, type Filters, type MetricId } from "@erpx/feature-shared/lib/executive/canvas";
import type { WorkContext } from "@erpx/server-identity_access/public/authorization";
import type { SqlDatabase } from '@erpx/server-runtime/public/runtime/types';
import { specs } from '@erpx/server-runtime/public/executive/metric-projection-contract';
import type { MetricSnapshot, RawMetricRow } from '@erpx/server-runtime/public/executive/metric-projection-contract';
/** Parameterized Agent executive metric count projection shared by the approved-version
 * and invocation-outcome reads. Only the label expression varies; each call site keeps
 * its own source, predicates and binds; the native metric owner remains the authority. */
const agentMetricCountSql = (label: string) =>
  `SELECT ${label} label,count(*)::text value FROM `;
export async function readExecutiveMetricProjection(sourceDb: SqlDatabase, a: WorkContext, id: MetricId, f: Filters, breakdown: Breakdown): Promise<MetricSnapshot> {
    const spec = specs[id];
    let sql = '', values: unknown[] = [];
    const referenceAtOverride: number | null = null;
    if (id === "approved-agent-versions") {
        sql = agentMetricCountSql("'Approved versions'") +
          "agent_versions v JOIN agent_catalog c ON c.id=v.agent_id WHERE c.company_id=? AND c.status='approved' AND v.status='approved'";
        values = [a.companyId];
    }
    if (id === "agent-invocation-outcomes") {
        const label = breakdown === "status" ? "e.outcome"
            : "to_char(to_timestamp(e.created_at/1000.0) AT TIME ZONE 'UTC','YYYY-MM')";
        sql = agentMetricCountSql(label) +
          "ai_invocation_evidence e WHERE e.company_id=? AND e.created_at>=? AND e.created_at<? GROUP BY 1 ORDER BY 1";
        values = [a.companyId, Date.parse(f.from), Date.parse(f.to) + 86400000];
    }
    const raw = (await sourceDb.prepare(sql).bind(...values).all<RawMetricRow>()).results;
    return { raw, referenceAtOverride, source: (group, reported) => readMetricSources(sourceDb, id, sql, values, group, raw, `${spec.dateBasis} · ${f.from} to ${f.to} · ${f.currency}`, reported), watermarks: async () => [] };
}
