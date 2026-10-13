import { readMetricSources } from '@/server/modules/runtime/public/executive/metric-sources';
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
    if (["receivables", "payables", "invoiced-sales"].includes(id)) {
        const sales = id === "invoiced-sales";
        sql = `SELECT ${breakdown === "month" || breakdown === "default" && sales ? "substring(invoice_date,1,7)" : "status"} label,sum(${sales ? "total_minor" : "outstanding_minor"})::text value FROM finance_invoices WHERE company_id=? AND kind=? AND currency=? AND invoice_date BETWEEN ? AND ? AND status IN (${sales ? "'posted','partially_paid','paid'" : "'posted','partially_paid'"}) GROUP BY 1 ORDER BY 1`;
        values = [a.companyId, id === "payables" ? "payable" : "receivable", f.currency, f.from, f.to];
    }
    if (id === "cash-receipts") {
        const label = breakdown === "month" || breakdown === "default"
            ? "to_char(to_timestamp(j.posted_at/1000.0) AT TIME ZONE 'UTC','YYYY-MM')"
            : "'Posted receipts'";
        sql = `SELECT ${label} label,sum(p.amount_minor)::text value FROM finance_payments p JOIN finance_journals j ON j.id=p.journal_id
      WHERE p.company_id=? AND p.kind='receipt' AND p.currency=? AND p.status IN ('released','settled') AND j.status='posted'
      AND j.posted_at>=? AND j.posted_at<? AND NOT EXISTS(SELECT 1 FROM finance_payment_reversals r WHERE r.payment_id=p.id)
      GROUP BY 1 ORDER BY 1`;
        values = [a.companyId, f.currency, Date.parse(f.from), Date.parse(f.to) + 86400000];
    }
    if (id === "close-blockers") {
        const label = breakdown === "month"
            ? "substring(latest.end_date,1,7)"
            : breakdown === "status" ? "latest.check_key" : "'Open close blockers'";
        sql = `WITH current_checks AS (
        SELECT DISTINCT ON (c.period_id,c.check_key) c.period_id,c.check_key,c.status,c.resolved_at,c.checked_at,c.id,p.end_date,p.state
        FROM finance_close_checks c JOIN finance_periods p ON p.id=c.period_id
        WHERE c.company_id=? AND p.company_id=? AND p.end_date BETWEEN ? AND ? AND p.state IN ('open','closing')
        ORDER BY c.period_id,c.check_key,c.checked_at DESC,c.id DESC
      )
      SELECT ${label} label,count(*)::text value FROM current_checks latest
      WHERE latest.status='block' AND latest.resolved_at IS NULL GROUP BY 1 ORDER BY 1`;
        values = [a.companyId, a.companyId, f.from, f.to];
    }
    if (id === "active-customers") {
        sql = "SELECT 'Active customers' label,count(*)::text value FROM finance_parties WHERE company_id=? AND kind='customer' AND status='active' AND merged_into_id IS NULL";
        values = [a.companyId];
    }
    const raw = (await sourceDb.prepare(sql).bind(...values).all<RawMetricRow>()).results;
    return { raw, referenceAtOverride, source: (group, reported) => readMetricSources(sourceDb, id, sql, values, group, raw, `${spec.dateBasis} · ${f.from} to ${f.to} · ${f.currency}`, reported), watermarks: async () => [] };
}
