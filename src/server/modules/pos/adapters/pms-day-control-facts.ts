import { hospitalityOwnerProjectionSources as commerceProjectionSources } from "../../commerce/public/hospitality-owner-projection-sources";
import { hospitalityOwnerProjectionSources as pmsProjectionSources } from "../../pms/public/hospitality-owner-projection-sources";
import "server-only";
import type { SqlDatabase } from "../../runtime/public/runtime/types";
import type { Count, Totals } from "../../pms/public/hospitality-day-control-contracts";
export function pmsDayControlPosFacts(db: SqlDatabase) {
    return {
        openShifts: (facts: {
            scope: [
                string,
                string
            ];
        }) => db.prepare("SELECT count(*) n FROM pos_shifts WHERE company_id=? AND branch_id=? AND state='open'").bind(...facts.scope).first<Count>().then(row => Number(row?.n ?? 0)),
        unreviewedVariances: (facts: {
            scope: [
                string,
                string
            ];
        }) => db.prepare("SELECT count(*) n FROM pos_shifts WHERE company_id=? AND branch_id=? AND state='closed' AND COALESCE(variance_minor,0)<>0").bind(...facts.scope).first<Count>().then(row => Number(row?.n ?? 0)),
        openBills: (facts: {
            scope: [
                string,
                string
            ];
            businessDate: string;
        }) => db.prepare("SELECT count(*) n FROM pos_sales WHERE company_id=? AND branch_id=? AND business_date=? AND state IN ('open','held','payment_pending')").bind(...facts.scope, facts.businessDate).first<Count>().then(row => Number(row?.n ?? 0)),
        pendingPayments: (facts: {
            scope: [
                string,
                string
            ];
            businessDate: string;
        }) => db.prepare("SELECT count(*) n FROM pos_payments p JOIN pos_sales s ON s.id=p.sale_id WHERE s.company_id=? AND s.branch_id=? AND s.business_date=? AND p.state IN ('requested','pending','exception')").bind(...facts.scope, facts.businessDate).first<Count>().then(row => Number(row?.n ?? 0)),
        unresolvedRefunds: (facts: {
            scope: [
                string,
                string
            ];
            businessDate: string;
        }) => db.prepare("SELECT count(*) n FROM pos_refunds WHERE company_id=? AND branch_id=? AND accounting_date=? AND state IN ('requested','approved','processing','exception')").bind(...facts.scope, facts.businessDate).first<Count>().then(row => Number(row?.n ?? 0)),
        failedJobs: (facts: {
            companyId: string;
        }) => db.prepare("SELECT count(*) n FROM pos_jobs WHERE company_id=? AND state='exception' AND kind NOT IN ('connect','connect_refresh')").bind(facts.companyId).first<Count>().then(row => Number(row?.n ?? 0)),
        missingRoomCharges: (facts: {
            scope: [
                string,
                string
            ];
            businessDate: string;
            propertyId: string;
        }) => db.prepare(`SELECT count(*) n FROM pos_sales s WHERE s.company_id=? AND s.branch_id=? AND s.business_date=? AND s.state='completed' AND s.invoice_id IS NULL
      AND NOT EXISTS(SELECT 1 FROM pos_room_charge_reversals rr WHERE rr.sale_id=s.id)
      AND NOT EXISTS(SELECT 1 FROM (${pmsProjectionSources.pms_sources}) ps JOIN (${pmsProjectionSources.pms_postings}) pp ON pp.property_id=ps.property_id AND pp.record_id=ps.record_id
        JOIN (${pmsProjectionSources.pms_records}) e ON e.id=ps.record_id AND e.property_id=ps.property_id AND e.kind='entry'
        WHERE ps.property_id=? AND ps.source_key='pos:'||s.id||':'||s.id AND (e.data_json::jsonb->>'total')::bigint=s.total_minor)`).bind(...facts.scope, facts.businessDate, facts.propertyId).first<Count>().then(row => Number(row?.n ?? 0)),
        totals: (facts: {
            scope: [
                string,
                string
            ];
            businessDate: string;
        }) => db.prepare(`SELECT
    COALESCE(SUM(CASE WHEN s.invoice_id IS NOT NULL THEN s.total_minor ELSE 0 END),0) direct_total,
    COALESCE(SUM(CASE WHEN s.invoice_id IS NULL THEN s.total_minor ELSE 0 END),0) room_total,
    COALESCE(SUM((SELECT SUM(p.amount_minor) FROM pos_payments p WHERE p.sale_id=s.id AND p.method='cash' AND p.state='succeeded')),0) cash_received,
    COALESCE(SUM((SELECT SUM(p.amount_minor) FROM pos_payments p WHERE p.sale_id=s.id AND p.method='transfer' AND p.state='succeeded')),0) transfer_verified
    FROM pos_sales s WHERE s.company_id=? AND s.branch_id=? AND s.business_date=? AND s.state='completed'
      AND NOT EXISTS(SELECT 1 FROM pos_room_charge_reversals rr WHERE rr.sale_id=s.id)`).bind(...facts.scope, facts.businessDate).first<Totals>(),
        stockMoves: (facts: {
            scope: [
                string,
                string
            ];
            businessDate: string;
        }) => db.prepare(`SELECT COALESCE(SUM(m.value_minor),0) n FROM pos_stock_moves m JOIN (${commerceProjectionSources.commerce_warehouses}) w ON w.id=m.warehouse_id
    WHERE m.company_id=? AND w.branch_id=? AND m.movement_date=?`).bind(...facts.scope, facts.businessDate).first<Count>()
    };
}
