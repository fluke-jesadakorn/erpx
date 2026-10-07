import { commercialOwnerProjectionSources as organizationProjectionSources } from "../../organization/public/commercial-owner-projection-sources";
import { readMetricSources } from '@/server/modules/runtime/public/executive/metric-sources';
import "server-only";
import { type Breakdown, type Filters, type MetricId } from "@/lib/executive/canvas";
import type { WorkContext } from "@/server/modules/identity_access/public/authorization";
import type { SqlDatabase } from '@/server/modules/runtime/public/runtime/types';
import { specs } from '@/server/modules/runtime/public/executive/metric-projection-contract';
import type { MetricSnapshot, RawMetricRow } from '@/server/modules/runtime/public/executive/metric-projection-contract';
export async function readExecutiveMetricProjection(sourceDb: SqlDatabase, a: WorkContext, id: MetricId, f: Filters, breakdown: Breakdown): Promise<MetricSnapshot> {
    const spec = specs[id], scopedAlias = (alias: string) => `${alias}.company_id=? AND (?::text IS NULL OR ${alias}.branch_id=?) AND (?::text IS NULL OR ${alias}.department_id=?)`, scopeArgs = [a.companyId, a.branchId, a.branchId, a.departmentId, a.departmentId];
    let sql = '', values: unknown[] = [];
    const referenceAtOverride: number | null = null;
    if (id === "open-ecommerce-carts") {
        const label = breakdown === "status" ? "c.status"
            : breakdown === "branch" ? `COALESCE((SELECT n.display_name FROM (${organizationProjectionSources.organization_nodes}) n WHERE n.id=c.branch_id AND n.company_id=c.company_id),'Branch')`
                : breakdown === "department" ? `COALESCE((SELECT n.display_name FROM (${organizationProjectionSources.organization_nodes}) n WHERE n.id=c.department_id AND n.company_id=c.company_id),'Unassigned')`
                    : "to_char(to_timestamp(c.created_at/1000.0) AT TIME ZONE 'UTC','YYYY-MM')";
        sql = `SELECT ${label} label,count(*)::text value FROM ecommerce_carts c WHERE ${scopedAlias("c")} AND c.status='open' GROUP BY 1 ORDER BY 1`;
        values = scopeArgs;
    }
    if (id === "completed-ecommerce-checkouts") {
        const label = breakdown === "status" ? "x.status"
            : breakdown === "branch" ? `COALESCE((SELECT n.display_name FROM (${organizationProjectionSources.organization_nodes}) n WHERE n.id=c.branch_id AND n.company_id=c.company_id),'Branch')`
                : breakdown === "department" ? `COALESCE((SELECT n.display_name FROM (${organizationProjectionSources.organization_nodes}) n WHERE n.id=c.department_id AND n.company_id=c.company_id),'Unassigned')`
                    : "to_char(to_timestamp(x.created_at/1000.0) AT TIME ZONE 'UTC','YYYY-MM')";
        sql = `SELECT ${label} label,count(*)::text value FROM ecommerce_checkouts x JOIN ecommerce_carts c ON c.id=x.cart_id WHERE ${scopedAlias("c")} AND x.status IN ('confirmed','partially_fulfilled','fulfilled') AND x.created_at>=? AND x.created_at<? GROUP BY 1 ORDER BY 1`;
        values = [...scopeArgs, Date.parse(f.from), Date.parse(f.to) + 86400000];
    }
    if (["ecommerce-return-links", "ecommerce-fulfillment-links"].includes(id)) {
        const returns = id === "ecommerce-return-links";
        const label = breakdown === "status" ? `${returns ? "r" : "l"}.status`
            : breakdown === "branch" ? `COALESCE((SELECT n.display_name FROM (${organizationProjectionSources.organization_nodes}) n WHERE n.id=c.branch_id AND n.company_id=c.company_id),'Branch')`
                : breakdown === "department" ? `COALESCE((SELECT n.display_name FROM (${organizationProjectionSources.organization_nodes}) n WHERE n.id=c.department_id AND n.company_id=c.company_id),'Unassigned')`
                    : `to_char(to_timestamp(${returns ? "r" : "l"}.created_at/1000.0) AT TIME ZONE 'UTC','YYYY-MM')`;
        const relation = returns
            ? "ecommerce_return_links r JOIN ecommerce_checkouts x ON x.id=r.checkout_id JOIN ecommerce_carts c ON c.id=x.cart_id"
            : "ecommerce_fulfillment_links l JOIN ecommerce_checkouts x ON x.id=l.checkout_id JOIN ecommerce_carts c ON c.id=x.cart_id";
        const alias = returns ? "r" : "l";
        sql = `SELECT ${label} label,count(*)::text value FROM ${relation} WHERE ${scopedAlias("c")} AND ${alias}.created_at>=? AND ${alias}.created_at<? GROUP BY 1 ORDER BY 1`;
        values = [...scopeArgs, Date.parse(f.from), Date.parse(f.to) + 86400000];
    }
    const raw = (await sourceDb.prepare(sql).bind(...values).all<RawMetricRow>()).results;
    return { raw, referenceAtOverride, source: (group, reported) => readMetricSources(sourceDb, id, sql, values, group, raw, `${spec.dateBasis} · ${f.from} to ${f.to} · ${f.currency}`, reported), watermarks: async () => [] };
}
