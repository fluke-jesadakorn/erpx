import { hospitalityOwnerProjectionSources as commerceProjectionSources } from "@erpx/server-commerce/public/hospitality-owner-projection-sources";
import { hospitalityOwnerProjectionSources as pmsProjectionSources } from "@erpx/server-pms/public/hospitality-owner-projection-sources";
import "server-only";
import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
import type { Count, Totals } from "@erpx/server-pms/public/hospitality-day-control-contracts";
export function pmsDayControlPosFacts(db: SqlDatabase) {
    return {
        openShifts: (facts: {
            propertyId: string;
            scope: [
                string,
                string
            ];
        }) => db.prepare("SELECT count(*) n FROM pos_shifts WHERE company_id=? AND branch_id=? AND register_id IN (SELECT id FROM pos_registers WHERE property_id=?) AND state='open'").bind(...facts.scope, facts.propertyId).first<Count>().then(row => Number(row?.n ?? 0)),
        unreviewedVariances: (facts: {
            propertyId: string;
            scope: [
                string,
                string
            ];
        }) => db.prepare("SELECT count(*) n FROM pos_shifts WHERE company_id=? AND branch_id=? AND register_id IN (SELECT id FROM pos_registers WHERE property_id=?) AND state='closed' AND COALESCE(variance_minor,0)<>0").bind(...facts.scope, facts.propertyId).first<Count>().then(row => Number(row?.n ?? 0)),
        openBills: (facts: {
            propertyId: string;
            scope: [
                string,
                string
            ];
            businessDate: string;
        }) => db.prepare("SELECT count(*) n FROM pos_sales WHERE company_id=? AND branch_id=? AND business_date=? AND register_id IN (SELECT id FROM pos_registers WHERE property_id=?) AND state IN ('open','held','payment_pending')").bind(...facts.scope, facts.businessDate, facts.propertyId).first<Count>().then(row => Number(row?.n ?? 0)),
        pendingPayments: (facts: {
            propertyId: string;
            scope: [
                string,
                string
            ];
            businessDate: string;
        }) => db.prepare("SELECT count(*) n FROM pos_payments p JOIN pos_sales s ON s.id=p.sale_id WHERE s.company_id=? AND s.branch_id=? AND s.business_date=? AND s.register_id IN (SELECT id FROM pos_registers WHERE property_id=?) AND p.state IN ('requested','pending','exception')").bind(...facts.scope, facts.businessDate, facts.propertyId).first<Count>().then(row => Number(row?.n ?? 0)),
        unresolvedRefunds: (facts: {
            propertyId: string;
            scope: [
                string,
                string
            ];
            businessDate: string;
        }) => db.prepare("SELECT count(*) n FROM pos_refunds WHERE company_id=? AND branch_id=? AND accounting_date=? AND sale_id IN (SELECT s.id FROM pos_sales s JOIN pos_registers r ON r.id=s.register_id WHERE r.property_id=?) AND state IN ('requested','approved','processing','exception')").bind(...facts.scope, facts.businessDate, facts.propertyId).first<Count>().then(row => Number(row?.n ?? 0)),
        failedJobs: (facts: {
            propertyId: string;
            companyId: string;
        }) => db.prepare("SELECT count(*) n FROM pos_jobs WHERE company_id=? AND EXISTS(SELECT 1 FROM pos_sales s JOIN pos_registers r ON r.id=s.register_id WHERE r.property_id=? AND (s.id=pos_jobs.record_id OR EXISTS(SELECT 1 FROM pos_payments pay WHERE pay.sale_id=s.id AND pay.id=pos_jobs.record_id) OR EXISTS(SELECT 1 FROM pos_refunds refund WHERE refund.sale_id=s.id AND refund.id=pos_jobs.record_id) OR EXISTS(SELECT 1 FROM pos_refund_payments rp JOIN pos_refunds rf ON rf.id=rp.refund_id WHERE rp.id=pos_jobs.record_id AND rf.sale_id=s.id))) AND state='exception' AND kind NOT IN ('connect','connect_refresh')").bind(facts.companyId, facts.propertyId).first<Count>().then(row => Number(row?.n ?? 0)),
        missingRoomCharges: (facts: {
            propertyId: string;
            scope: [
                string,
                string
            ];
            businessDate: string;
        }) => db.prepare(`SELECT count(*) n FROM pos_sales s WHERE s.company_id=? AND s.branch_id=? AND s.business_date=? AND s.register_id IN (SELECT id FROM pos_registers WHERE property_id=?) AND s.state='completed' AND s.invoice_id IS NULL
      AND NOT EXISTS(SELECT 1 FROM pos_room_charge_reversals rr WHERE rr.sale_id=s.id)
      AND NOT EXISTS(SELECT 1 FROM (${pmsProjectionSources.pms_sources}) ps JOIN (${pmsProjectionSources.pms_postings}) pp ON pp.property_id=ps.property_id AND pp.record_id=ps.record_id
        JOIN (${pmsProjectionSources.pms_records}) e ON e.id=ps.record_id AND e.property_id=ps.property_id AND e.kind='entry'
        WHERE ps.property_id=? AND ps.source_key='pos:'||s.id||':'||s.id AND (e.data_json::jsonb->>'total')::bigint=s.total_minor)`).bind(...facts.scope, facts.businessDate, facts.propertyId, facts.propertyId).first<Count>().then(row => Number(row?.n ?? 0)),
        totals: (facts: {
            propertyId: string;
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
    FROM pos_sales s WHERE s.company_id=? AND s.branch_id=? AND s.business_date=? AND s.register_id IN (SELECT id FROM pos_registers WHERE property_id=?) AND s.state='completed'
      AND NOT EXISTS(SELECT 1 FROM pos_room_charge_reversals rr WHERE rr.sale_id=s.id)`).bind(...facts.scope, facts.businessDate, facts.propertyId).first<Totals>(),
        stockMoves: (facts: {
            propertyId: string;
            scope: [
                string,
                string
            ];
            businessDate: string;
        }) => db.prepare(`SELECT COALESCE(SUM(m.value_minor),0) n FROM pos_stock_moves m JOIN (${commerceProjectionSources.commerce_warehouses}) w ON w.id=m.warehouse_id
    WHERE m.company_id=? AND w.branch_id=? AND m.movement_date=? AND EXISTS(SELECT 1 FROM pos_sales s JOIN pos_registers r ON r.id=s.register_id WHERE r.property_id=? AND (s.id=m.source_id OR EXISTS(SELECT 1 FROM pos_lines l WHERE l.sale_id=s.id AND l.fulfillment_id=m.fulfillment_id)))`).bind(...facts.scope, facts.businessDate, facts.propertyId).first<Count>()
    };
}

export async function pendingHotelRoomChargeReversals(db: SqlDatabase, companyId: string, propertyId: string) {
    const row=await db.prepare("SELECT count(*) n FROM pos_room_charge_reversal_requests WHERE company_id=? AND property_id=? AND state IN ('requested','pos_approved')").bind(companyId,propertyId).first<Count>();
    return Number(row?.n??0);
}
