import { commercialOwnerProjectionSources as organizationProjectionSources } from "@erpx/server-organization/public/commercial-owner-projection-sources";
import { readMetricSources } from '@erpx/server-runtime/public/executive/metric-sources';
import "server-only";
import { type Breakdown, type Filters, type MetricId } from "@erpx/feature-shared/lib/executive/canvas";
import type { WorkContext } from "@erpx/server-identity_access/public/authorization";
import type { SqlDatabase } from '@erpx/server-runtime/public/runtime/types';
import { specs } from '@erpx/server-runtime/public/executive/metric-projection-contract';
import type { MetricSnapshot, RawMetricRow } from '@erpx/server-runtime/public/executive/metric-projection-contract';
import { ecommerceScalar } from './read-tail-sql';
/** r8: one parameterized source-node label; branch and department callers are byte-identical after binding. Text only: binds and locks are unchanged. */
const nodeNameLabel = (col: string, fallback: string) => `COALESCE((${ecommerceScalar(`n.display_name FROM (${organizationProjectionSources.organization_nodes}) n WHERE n.id=c.${col} AND n.company_id=c.company_id`)}),'${fallback}')`;
const branchNameLabel = nodeNameLabel('branch_id','Branch');
const departmentNameLabel = nodeNameLabel('department_id','Unassigned');
export async function readExecutiveMetricProjection(sourceDb: SqlDatabase, a: WorkContext, id: MetricId, f: Filters, breakdown: Breakdown): Promise<MetricSnapshot> {
    const spec = specs[id], scopedAlias = (alias: string) => `${alias}.company_id=? AND (?::text IS NULL OR ${alias}.branch_id=?) AND (?::text IS NULL OR ${alias}.department_id=?)`, scopeArgs = [a.companyId, a.branchId, a.branchId, a.departmentId, a.departmentId];
    let sql = '', values: unknown[] = [];
    const referenceAtOverride: number | null = null;
    const countBy = (label: string, relation: string, extra: string) =>
        ecommerceScalar(`${label} label,count(*)::text value FROM ${relation} WHERE ${scopedAlias("c")} AND ${extra} GROUP BY 1 ORDER BY 1`);
    if (id === "open-ecommerce-carts") {
        const label = breakdown === "status" ? "c.status"
            : breakdown === "branch" ? branchNameLabel
                : breakdown === "department" ? departmentNameLabel
                    : "to_char(to_timestamp(c.created_at/1000.0) AT TIME ZONE 'UTC','YYYY-MM')";
        sql = countBy(label, "ecommerce_carts c", "c.status='open'");
        values = scopeArgs;
    }
    if (id === "completed-ecommerce-checkouts") {
        const label = breakdown === "status" ? "x.status"
            : breakdown === "branch" ? branchNameLabel
                : breakdown === "department" ? departmentNameLabel
                    : "to_char(to_timestamp(x.created_at/1000.0) AT TIME ZONE 'UTC','YYYY-MM')";
        sql = countBy(label, "ecommerce_checkouts x JOIN ecommerce_carts c ON c.id=x.cart_id", "x.status IN ('confirmed','partially_fulfilled','fulfilled') AND x.created_at>=? AND x.created_at<?");
        values = [...scopeArgs, Date.parse(f.from), Date.parse(f.to) + 86400000];
    }
    if (["ecommerce-return-links", "ecommerce-fulfillment-links"].includes(id)) {
        const returns = id === "ecommerce-return-links";
        const label = breakdown === "status" ? `${returns ? "r" : "l"}.status`
            : breakdown === "branch" ? branchNameLabel
                : breakdown === "department" ? departmentNameLabel
                    : `to_char(to_timestamp(${returns ? "r" : "l"}.created_at/1000.0) AT TIME ZONE 'UTC','YYYY-MM')`;
        const relation = returns
            ? "ecommerce_return_links r JOIN ecommerce_checkouts x ON x.id=r.checkout_id JOIN ecommerce_carts c ON c.id=x.cart_id"
            : "ecommerce_fulfillment_links l JOIN ecommerce_checkouts x ON x.id=l.checkout_id JOIN ecommerce_carts c ON c.id=x.cart_id";
        const alias = returns ? "r" : "l";
        sql = countBy(label, relation, `${alias}.created_at>=? AND ${alias}.created_at<?`);
        values = [...scopeArgs, Date.parse(f.from), Date.parse(f.to) + 86400000];
    }
    const raw = (await sourceDb.prepare(sql).bind(...values).all<RawMetricRow>()).results;
    return { raw, referenceAtOverride, source: (group, reported) => readMetricSources(sourceDb, id, sql, values, group, raw, `${spec.dateBasis} · ${f.from} to ${f.to} · ${f.currency}`, reported), watermarks: async () => [] };
}
