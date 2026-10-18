import "server-only";
import { posMetricHistoryAccess } from "./metric-purchases";
import { z } from "zod";
import { type Filters } from "@/lib/shared/executive/canvas";
import type { WorkContext } from "@/server/modules/identity_access/public/authorization";
import type { SqlDatabase } from "@/server/modules/runtime/public/runtime/types";
import { type PosEvidenceMetric, inputSchema, type EvidenceRow, parseCursor } from '@/server/modules/runtime/public/executive/pos-evidence-contract';
function sourceQuery(metric: PosEvidenceMetric) {
    if (metric === "pos-sales")
        return `SELECT 'sale:'||s.id id,s.id source_id,'sale' source_type,s.id sale_id,s.business_date event_date,
      s.number receipt_number,s.state status,s.state sale_status,s.currency,NULL::text method,
      s.total_minor::numeric amount_minor,s.total_minor::numeric contribution_minor,0::numeric refunded_minor,
      s.version revision,s.version sale_version
    FROM pos_sales s WHERE s.company_id=? AND (?::text IS NULL OR s.branch_id=?) AND s.currency=?
      AND s.business_date BETWEEN ? AND ? AND s.state='completed'
      AND (?='' OR position(lower(?) in lower(s.number))>0)`;
    if (metric === "net-pos-sales")
        return `SELECT 'sale:'||s.id id,s.id source_id,'sale' source_type,s.id sale_id,s.business_date event_date,
      s.number receipt_number,s.state status,s.state sale_status,s.currency,NULL::text method,
      s.total_minor::numeric amount_minor,s.total_minor::numeric contribution_minor,0::numeric refunded_minor,
      s.version revision,s.version sale_version
    FROM pos_sales s WHERE s.company_id=? AND (?::text IS NULL OR s.branch_id=?) AND s.currency=?
      AND s.business_date BETWEEN ? AND ? AND s.state='completed'
      AND (?='' OR position(lower(?) in lower(s.number))>0)
    UNION ALL
    SELECT 'refund:'||r.id id,r.id source_id,'refund' source_type,s.id sale_id,r.accounting_date event_date,
      s.number receipt_number,r.state status,s.state sale_status,s.currency,NULL::text method,
      r.amount_minor::numeric amount_minor,-r.amount_minor::numeric contribution_minor,0::numeric refunded_minor,
      r.version revision,s.version sale_version
    FROM pos_refunds r JOIN pos_sales s ON s.id=r.sale_id AND s.company_id=r.company_id
    WHERE r.company_id=? AND (?::text IS NULL OR r.branch_id=?) AND s.currency=?
      AND r.accounting_date BETWEEN ? AND ? AND r.kind='sale' AND r.state='completed'
      AND (?='' OR position(lower(?) in lower(s.number))>0)`;
    return `SELECT 'payment:'||p.id id,p.id source_id,'payment' source_type,s.id sale_id,s.business_date event_date,
      s.number receipt_number,p.state status,s.state sale_status,s.currency,p.method,
      p.amount_minor::numeric amount_minor,
      (p.amount_minor::numeric-COALESCE((SELECT sum(rp.amount_minor)::numeric FROM pos_refund_payments rp
        JOIN pos_refunds r ON r.id=rp.refund_id AND r.company_id=p.company_id
        WHERE rp.payment_id=p.id AND rp.state='succeeded' AND r.kind='sale'),0)) contribution_minor,
      COALESCE((SELECT sum(rp.amount_minor)::numeric FROM pos_refund_payments rp
        JOIN pos_refunds r ON r.id=rp.refund_id AND r.company_id=p.company_id
        WHERE rp.payment_id=p.id AND rp.state='succeeded' AND r.kind='sale'),0) refunded_minor,
      p.version revision,s.version sale_version
    FROM pos_payments p JOIN pos_sales s ON s.id=p.sale_id AND s.company_id=p.company_id
    WHERE p.company_id=? AND (?::text IS NULL OR s.branch_id=?) AND s.currency=?
      AND s.business_date BETWEEN ? AND ? AND s.state='completed' AND p.state='succeeded'
      AND (?='' OR position(lower(?) in lower(s.number))>0 OR position(lower(?) in lower(p.method))>0)`;
}
function sourceQueryValues(metric: PosEvidenceMetric, actor: WorkContext, filters: Filters, query: string) {
    const base = [actor.companyId, actor.branchId, actor.branchId, filters.currency, filters.from, filters.to];
    if (metric === "pos-sales" || metric === "net-pos-sales") {
        const search = [query, query];
        return metric === "pos-sales" ? [...base, ...search] : [...base, ...search, ...base, ...search];
    }
    return [...base, ...[query, query, query]];
}
export function executiveEvidenceSources(database: SqlDatabase) {
    const db = database;
    return { async searchRows0(actor: WorkContext, input: z.infer<typeof inputSchema>, cursor: ReturnType<typeof parseCursor>): Promise<EvidenceRow[]> {
            if (!(await posMetricHistoryAccess(db, actor))[input.metric]) return [];
            const values: unknown[] = sourceQueryValues(input.metric, actor, input.filters, input.query);
            const cursorSql = cursor ? "WHERE (event_date<? OR (event_date=? AND id<?))" : "";
            if (cursor)
                values.push(cursor.eventDate, cursor.eventDate, cursor.id);
            values.push(input.limit + 1);
            const rows = (await db.prepare(`WITH events AS (${sourceQuery(input.metric)})
    SELECT * FROM events ${cursorSql} ORDER BY event_date DESC,id DESC LIMIT ?`)
                .bind(...values).all<EvidenceRow>()).results;
            return rows;
        } };
}
