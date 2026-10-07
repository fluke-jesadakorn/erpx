import "server-only";
import type { SqlDatabase } from "../../runtime/public/runtime/types";
import { hospitalityDayControls as control, type HospitalityDayControlRepository, type Count } from "../application/hospitality-day-controls";
import { pmsDayControlPosFacts } from "../../pos/public/pms-day-control-facts";
import { pmsDayControlFinanceFacts } from "../../finance/public/pms-day-control-facts";
function repository(db: SqlDatabase): HospitalityDayControlRepository {
    const pos = pmsDayControlPosFacts(db), finance = pmsDayControlFinanceFacts(db);
    return {
        openShifts: facts => pos.openShifts(facts),
        unreviewedVariances: facts => pos.unreviewedVariances(facts),
        openBills: facts => pos.openBills(facts),
        pendingPayments: facts => pos.pendingPayments(facts),
        pendingHotelTransfers: (facts: {
            companyId: string;
            propertyId: string;
            businessDate: string;
        }) => db.prepare("SELECT count(*) n FROM pms_transfer_requests WHERE company_id=? AND property_id=? AND business_date=? AND state='pending'").bind(facts.companyId, facts.propertyId, facts.businessDate).first<Count>().then(row => Number(row?.n ?? 0)),
        pendingChargeCorrections: (facts: {
            companyId: string;
            propertyId: string;
        }) => db.prepare("SELECT count(*) n FROM pms_charge_corrections WHERE company_id=? AND property_id=? AND status='pending'").bind(facts.companyId, facts.propertyId).first<Count>().then(row => Number(row?.n ?? 0)),
        property: (facts: {
            propertyId: string;
            companyId: string;
        }) => db.prepare('SELECT config_json FROM pms_properties WHERE id=? AND company_id=?').bind(facts.propertyId, facts.companyId).first<{
            config_json: string;
        }>(),
        unpostedBookingReceipts: (facts: {
            propertyId: string;
            companyId: string;
            timezone: string;
            businessDate: string;
        }) => db.prepare(`SELECT count(*) n FROM pms_booking_payments
    WHERE property_id=? AND company_id=? AND paid_at IS NOT NULL AND status IN ('payment_pending','expired')
    AND to_char(to_timestamp(paid_at/1000.0) AT TIME ZONE ?,'YYYY-MM-DD')<=?`).bind(facts.propertyId, facts.companyId, facts.timezone, facts.businessDate).first<Count>().then(row => Number(row?.n ?? 0)),
        failedBookingRefunds: (facts: {
            propertyId: string;
            companyId: string;
            timezone: string;
            businessDate: string;
        }) => db.prepare(`SELECT count(*) n FROM pms_booking_refund_requests r
    JOIN pms_booking_payments p ON p.id=r.payment_id WHERE p.property_id=? AND p.company_id=?
    AND (r.state='failed' OR (r.state IN ('approved','processing','requires_action') AND r.last_error IS NOT NULL))
    AND to_char(to_timestamp(r.created_at/1000.0) AT TIME ZONE ?,'YYYY-MM-DD')<=?`).bind(facts.propertyId, facts.companyId, facts.timezone, facts.businessDate).first<Count>().then(row => Number(row?.n ?? 0)),
        unpostedCounterReceipts: (facts: {
            propertyId: string;
            companyId: string;
            timezone: string;
            businessDate: string;
        }) => db.prepare(`SELECT count(*) n FROM pms_stripe_payments
    WHERE property_id=? AND company_id=? AND received_at IS NOT NULL AND state IN ('requested','pending','cancelled')
    AND to_char(to_timestamp(received_at/1000.0) AT TIME ZONE ?,'YYYY-MM-DD')<=?`).bind(facts.propertyId, facts.companyId, facts.timezone, facts.businessDate).first<Count>().then(row => Number(row?.n ?? 0)),
        failedCounterRefunds: (facts: {
            propertyId: string;
            companyId: string;
            timezone: string;
            businessDate: string;
        }) => db.prepare(`SELECT count(*) n FROM pms_stripe_payments
    WHERE property_id=? AND company_id=? AND (refund_state='failed' OR (refund_state IN ('approved','processing','requires_action') AND last_error IS NOT NULL))
    AND to_char(to_timestamp(COALESCE(received_at,created_at)/1000.0) AT TIME ZONE ?,'YYYY-MM-DD')<=?`).bind(facts.propertyId, facts.companyId, facts.timezone, facts.businessDate).first<Count>().then(row => Number(row?.n ?? 0)),
        unresolvedRefunds: facts => pos.unresolvedRefunds(facts),
        failedJobs: facts => pos.failedJobs(facts),
        missingRoomCharges: facts => pos.missingRoomCharges(facts),
        missingDirectInvoices: facts => finance.missingDirectInvoices(facts),
        unmatchedVerifiedTransfers: facts => finance.unmatchedVerifiedTransfers(facts),
        unmatchedVerifiedHotelTransfers: facts => finance.unmatchedVerifiedHotelTransfers(facts),
        totals: facts => pos.totals(facts),
        stockMoves: facts => pos.stockMoves(facts),
        stockLedger: facts => finance.stockLedger(facts),
        unbalancedJournals: facts => finance.unbalancedJournals(facts)
    };
}
export function hospitalityDayControls(db: SqlDatabase, companyId: string, branchId: string, propertyId: string, businessDate: string) { return control(repository(db), companyId, branchId, propertyId, businessDate); }
