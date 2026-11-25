import {identitySnapshotSources as organizationSnapshotSources} from '@erpx/server-organization/public/identity-snapshot-sources';
const organizationSources=organizationSnapshotSources();
import { readMetricSources } from '@erpx/server-runtime/public/executive/metric-sources';
import "server-only";
import { type Breakdown, type Filters, type MetricId } from "@erpx/feature-shared/lib/executive/canvas";
import type { WorkContext } from "@erpx/server-identity_access/public/authorization";
import type { SqlDatabase } from '@erpx/server-runtime/public/runtime/types';
import { specs } from '@erpx/server-runtime/public/executive/metric-projection-contract';
import type { MetricSnapshot, RawMetricRow } from '@erpx/server-runtime/public/executive/metric-projection-contract';
import { scopeLabel, branchScopeClauseSql } from './read-tail-family-r38';
import { execMetricLabelHead } from './read-tail-family-r40';
export async function readExecutiveMetricProjection(sourceDb: SqlDatabase, a: WorkContext, id: MetricId, f: Filters, breakdown: Breakdown): Promise<MetricSnapshot> {
    const spec = specs[id];
    let sql = '', values: unknown[] = [];
    const referenceAtOverride: number | null = null;
    if (id === "active-assignments") {
        const scope = scopeLabel('x', {
            nFrom: organizationSources.executive_metric_projections_organization_nodes_n_from,
            dFrom: organizationSources.executive_metric_projections_organization_nodes_d_from,
            dJoin: organizationSources.executive_metric_projections_organization_nodes_b_join,
            from: organizationSources.executive_metric_projections_organization_nodes_from,
        });
        const label = breakdown === "status" ? "x.scope_kind"
            : breakdown === "branch" ? scope.branch
                : breakdown === "department" ? scope.department
                    : "x.role";
        const scopeClause = a.departmentId
            ? "AND (x.scope_kind='holding' OR (x.scope_kind='branch' AND x.scope_node_id=?) OR (x.scope_kind='department' AND x.scope_node_id=?))"
            : a.branchId
                ? branchScopeClauseSql('x', organizationSources.executive_metric_projections_organization_nodes_from)
                : "";
        const now = Date.now();
        sql = `${execMetricLabelHead(label)}assignments x WHERE x.company_id=? AND x.revoked_at IS NULL AND x.valid_from<=? AND x.valid_until>? AND iam_assignment_active(x.id,?) ${scopeClause} GROUP BY 1 ORDER BY 1`;
        values = [a.companyId, now, now, now, ...(a.departmentId ? [a.branchId, a.departmentId] : a.branchId ? [a.branchId, a.branchId] : [])];
    }
    if (id === "pending-access-reviews") {
        const scope = scopeLabel('i', {
            nFrom: organizationSources.executive_metric_projections_organization_nodes_n_from,
            dFrom: organizationSources.executive_metric_projections_organization_nodes_d_from,
            dJoin: organizationSources.executive_metric_projections_organization_nodes_b_join,
            from: organizationSources.executive_metric_projections_organization_nodes_from,
        });
        const label = breakdown === "status" ? "i.scope_kind"
            : breakdown === "branch" ? scope.branch
                : breakdown === "department" ? scope.department
                    : "'Pending reviews'";
        const scopeClause = a.departmentId
            ? "AND (i.scope_kind='holding' OR (i.scope_kind='branch' AND i.scope_node_id=?) OR (i.scope_kind='department' AND i.scope_node_id=?))"
            : a.branchId
                ? branchScopeClauseSql('i', organizationSources.executive_metric_projections_organization_nodes_from)
                : "";
        sql = `${execMetricLabelHead(label)}iam_access_review_items i JOIN iam_access_review_campaigns c ON c.id=i.campaign_id AND c.company_id=i.company_id WHERE i.company_id=? AND i.decision='pending' AND c.status='open' ${scopeClause} GROUP BY 1 ORDER BY 1`;
        values = [a.companyId, ...(a.departmentId ? [a.branchId, a.departmentId] : a.branchId ? [a.branchId, a.branchId] : [])];
    }
    const raw = (await sourceDb.prepare(sql).bind(...values).all<RawMetricRow>()).results;
    return { raw, referenceAtOverride, source: (group, reported) => readMetricSources(sourceDb, id, sql, values, group, raw, `${spec.dateBasis} · ${f.from} to ${f.to} · ${f.currency}`, reported), watermarks: async () => [] };
}
