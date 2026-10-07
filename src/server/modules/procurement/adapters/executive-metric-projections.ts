import { platformOwnerProjectionSources as organizationPlatformSources } from "../../organization/public/platform-owner-projection-sources";
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
    if (id === "invoice-match-exceptions") {
        const label = breakdown === "status"
            ? "CASE WHEN latest.status='returned' THEN 'Returned for correction' WHEN NOT latest.quantity_matches THEN 'Quantity variance' ELSE 'Subtotal variance' END"
            : breakdown === "branch" ? `COALESCE((SELECT n.display_name FROM (${organizationPlatformSources.organization_nodes}) n WHERE n.id=latest.branch_id AND n.company_id=latest.company_id),'Company-wide')`
                : breakdown === "month" ? "substring(latest.invoice_date,1,7)" : "'Supplier invoice match exceptions'";
        sql = `SELECT ${label} label,count(*)::text value FROM procurement_bill_matches latest
      WHERE ${scopedAlias("latest")} AND latest.invoice_date BETWEEN ? AND ? AND latest.currency=?
        AND (
          (latest.status='review' AND (NOT latest.quantity_matches OR latest.subtotal_minor<>latest.expected_subtotal_minor))
          OR (latest.status='returned' AND NOT EXISTS(
            SELECT 1 FROM procurement_bill_match_receipts returned_receipt
            JOIN procurement_bill_match_receipts replacement_receipt
              ON replacement_receipt.fulfillment_id=returned_receipt.fulfillment_id AND replacement_receipt.active=TRUE
            WHERE returned_receipt.match_id=latest.id AND replacement_receipt.match_id<>latest.id
          ))
        )
      GROUP BY 1 ORDER BY 1`;
        values = [...scopeArgs, f.from, f.to, f.currency];
    }
    const raw = (await sourceDb.prepare(sql).bind(...values).all<RawMetricRow>()).results;
    return { raw, referenceAtOverride, source: (group, reported) => readMetricSources(sourceDb, id, sql, values, group, raw, `${spec.dateBasis} · ${f.from} to ${f.to} · ${f.currency}`, reported), watermarks: async () => [] };
}
