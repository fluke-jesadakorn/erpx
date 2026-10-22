import {identitySnapshotSources as organizationSnapshotSources} from '@erpx/server-organization/public/identity-snapshot-sources';
const organizationSources=organizationSnapshotSources();
import { readMetricSources } from '@erpx/server-runtime/public/executive/metric-sources';
import "server-only";
import { type Breakdown, type Filters, type MetricId } from "@erpx/feature-shared/lib/executive/canvas";
import type { WorkContext } from "@erpx/server-identity_access/public/authorization";
import type { SqlDatabase } from '@erpx/server-runtime/public/runtime/types';
import { specs } from '@erpx/server-runtime/public/executive/metric-projection-contract';
import type { MetricSnapshot, RawMetricRow } from '@erpx/server-runtime/public/executive/metric-projection-contract';
export async function readExecutiveMetricProjection(sourceDb: SqlDatabase, a: WorkContext, id: MetricId, f: Filters, breakdown: Breakdown): Promise<MetricSnapshot> {
    const spec = specs[id];
    let sql = '', values: unknown[] = [];
    const referenceAtOverride: number | null = null;
    if (id === "active-assignments") {
        const label = breakdown === "status" ? "x.scope_kind"
            : breakdown === "branch" ? `CASE WHEN x.scope_kind='holding' THEN 'Holding scope' WHEN x.scope_kind='branch' THEN COALESCE((SELECT n.display_name ${organizationSources.executive_metric_projections_organization_nodes_n_from} WHERE n.id=x.scope_node_id AND n.company_id=x.company_id),'Branch') ELSE COALESCE((SELECT b.display_name ${organizationSources.executive_metric_projections_organization_nodes_d_from} ${organizationSources.executive_metric_projections_organization_nodes_b_join} ON b.id=d.branch_id AND b.company_id=d.company_id WHERE d.id=x.scope_node_id AND d.company_id=x.company_id),'Unassigned') END`
                : breakdown === "department" ? `CASE WHEN x.scope_kind='department' THEN COALESCE((SELECT display_name ${organizationSources.executive_metric_projections_organization_nodes_from} WHERE id=x.scope_node_id AND company_id=x.company_id),'Department') ELSE 'Other scope' END`
                    : "x.role";
        const scopeClause = a.departmentId
            ? "AND (x.scope_kind='holding' OR (x.scope_kind='branch' AND x.scope_node_id=?) OR (x.scope_kind='department' AND x.scope_node_id=?))"
            : a.branchId
                ? `AND (x.scope_kind='holding' OR (x.scope_kind='branch' AND x.scope_node_id=?) OR (x.scope_kind='department' AND x.scope_node_id IN (SELECT id ${organizationSources.executive_metric_projections_organization_nodes_from} WHERE company_id=x.company_id AND branch_id=?)))`
                : "";
        const now = Date.now();
        sql = `SELECT ${label} label,count(*)::text value FROM assignments x WHERE x.company_id=? AND x.revoked_at IS NULL AND x.valid_from<=? AND x.valid_until>? AND iam_assignment_active(x.id,?) ${scopeClause} GROUP BY 1 ORDER BY 1`;
        values = [a.companyId, now, now, now, ...(a.departmentId ? [a.branchId, a.departmentId] : a.branchId ? [a.branchId, a.branchId] : [])];
    }
    if (id === "pending-access-reviews") {
        const label = breakdown === "status" ? "i.scope_kind"
            : breakdown === "branch" ? `CASE WHEN i.scope_kind='holding' THEN 'Holding scope' WHEN i.scope_kind='branch' THEN COALESCE((SELECT n.display_name ${organizationSources.executive_metric_projections_organization_nodes_n_from} WHERE n.id=i.scope_node_id AND n.company_id=i.company_id),'Branch') ELSE COALESCE((SELECT b.display_name ${organizationSources.executive_metric_projections_organization_nodes_d_from} ${organizationSources.executive_metric_projections_organization_nodes_b_join} ON b.id=d.branch_id AND b.company_id=d.company_id WHERE d.id=i.scope_node_id AND d.company_id=i.company_id),'Unassigned') END`
                : breakdown === "department" ? `CASE WHEN i.scope_kind='department' THEN COALESCE((SELECT display_name ${organizationSources.executive_metric_projections_organization_nodes_from} WHERE id=i.scope_node_id AND company_id=i.company_id),'Department') ELSE 'Other scope' END`
                    : "'Pending reviews'";
        const scopeClause = a.departmentId
            ? "AND (i.scope_kind='holding' OR (i.scope_kind='branch' AND i.scope_node_id=?) OR (i.scope_kind='department' AND i.scope_node_id=?))"
            : a.branchId
                ? `AND (i.scope_kind='holding' OR (i.scope_kind='branch' AND i.scope_node_id=?) OR (i.scope_kind='department' AND i.scope_node_id IN (SELECT id ${organizationSources.executive_metric_projections_organization_nodes_from} WHERE company_id=i.company_id AND branch_id=?)))`
                : "";
        sql = `SELECT ${label} label,count(*)::text value FROM iam_access_review_items i JOIN iam_access_review_campaigns c ON c.id=i.campaign_id AND c.company_id=i.company_id WHERE i.company_id=? AND i.decision='pending' AND c.status='open' ${scopeClause} GROUP BY 1 ORDER BY 1`;
        values = [a.companyId, ...(a.departmentId ? [a.branchId, a.departmentId] : a.branchId ? [a.branchId, a.branchId] : [])];
    }
    const raw = (await sourceDb.prepare(sql).bind(...values).all<RawMetricRow>()).results;
    return { raw, referenceAtOverride, source: (group, reported) => readMetricSources(sourceDb, id, sql, values, group, raw, `${spec.dateBasis} · ${f.from} to ${f.to} · ${f.currency}`, reported), watermarks: async () => [] };
}
