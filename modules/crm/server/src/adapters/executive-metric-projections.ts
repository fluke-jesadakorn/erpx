import {crmMetricSource} from "@erpx/crm-server/adapters/metric-purchases";
import { commercialOwnerProjectionSources as commerceProjectionSources } from "@erpx/commerce-server/public/commercial-owner-projection-sources";
import { commercialOwnerProjectionSources as organizationProjectionSources } from "@erpx/organization-server/public/commercial-owner-projection-sources";
import { readMetricSources } from '@erpx/platform-runtime-server/public/executive/metric-sources';
import "server-only";
import { type Breakdown, type Filters, type MetricId } from "@erpx/platform-shared-ui/lib/executive/canvas";
import type { WorkContext } from "@erpx/identity-access-server/public/authorization";
import type { SqlDatabase } from '@erpx/platform-runtime-server/public/runtime/types';
import { specs } from '@erpx/platform-runtime-server/public/executive/metric-projection-contract';
import type { MetricSnapshot, RawMetricRow } from '@erpx/platform-runtime-server/public/executive/metric-projection-contract';
import {crmMetricBreakdownSql} from './read-tail-r37-sql';
import {crmScalarSubquerySql} from './read-tail-r43-sql';
/** Byte-identical org label fragment shared by the branch/department breakdowns.
 * The alias, column and fallback stay caller-owned; the rendered SQL is unchanged. */
const orgLabelSql = (alias: string, column: string, fallback: string) =>
    `COALESCE(${crmScalarSubquerySql('n.display_name',`(${organizationProjectionSources.organization_nodes}) n WHERE n.id=${alias}.${column} AND n.company_id=${alias}.company_id`)},'${fallback}')`;
export async function readExecutiveMetricProjection(sourceDb: SqlDatabase, a: WorkContext, id: MetricId, f: Filters, breakdown: Breakdown): Promise<MetricSnapshot> {
    const spec = specs[id], scopedAlias = (alias: string) => `${alias}.company_id=? AND (?::text IS NULL OR ${alias}.branch_id=?) AND (?::text IS NULL OR ${alias}.department_id=?)`, scopeArgs = [a.companyId, a.branchId, a.branchId, a.departmentId, a.departmentId];
    const purchased=await crmMetricSource(sourceDb,a,id);
    let sql = '', values: unknown[] = [];
    const referenceAtOverride: number | null = null;
    if (id === "opportunity-pipeline") {
        const label = breakdown === "status" ? "v.stage"
            : breakdown === "branch" ? orgLabelSql("l","branch_id","Company-wide")
                : breakdown === "month" ? "substring(v.close_date,1,7)" : "v.stage";
        sql = crmMetricBreakdownSql(label,'sum(v.expected_minor)::text',` FROM crm_opportunities v JOIN (${purchased.sql}) l ON l.id=v.lead_id WHERE ${scopedAlias("l")} AND v.currency=? AND v.close_date BETWEEN ? AND ? AND v.stage IN ('discovery','proposal','negotiation') GROUP BY 1 ORDER BY 1`);
        values = [...purchased.args,...scopeArgs, f.currency, f.from, f.to];
    }
    if (id === "overdue-followups") {
        const label = breakdown === "status" ? "c.status"
            : breakdown === "branch" ? orgLabelSql("o","branch_id","Company-wide")
                : breakdown === "department" ? orgLabelSql("o","department_id","Unassigned")
                    : breakdown === "month" ? "substring(c.due_date,1,7)" : "'Overdue follow-ups'";
        sql = crmMetricBreakdownSql(label,'count(*)::text',` FROM (${purchased.sql}) c JOIN (${commerceProjectionSources.commerce_orders}) o ON o.id=c.order_id AND o.company_id=c.company_id WHERE ${scopedAlias("o")} AND o.kind IN ('sales','quote') AND c.status='open' AND c.due_date BETWEEN ? AND ? AND c.due_date<to_char(current_timestamp AT TIME ZONE 'UTC','YYYY-MM-DD') GROUP BY 1 ORDER BY 1`);
        values = [...purchased.args,...scopeArgs, f.from, f.to];
    }
    const raw = (await sourceDb.prepare(sql).bind(...values).all<RawMetricRow>()).results;
    return { raw, referenceAtOverride, source: async (group, reported) => {await crmMetricSource(sourceDb,a,id);return readMetricSources(sourceDb, id, sql, values, group, raw, `${spec.dateBasis} · ${f.from} to ${f.to} · ${f.currency}`, reported);}, watermarks: async () => [] };
}
