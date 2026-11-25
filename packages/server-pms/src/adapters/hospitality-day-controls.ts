import { pendingHotelRoomChargeReversals } from "@erpx/server-pos/public/pms-day-control-facts";
import "server-only";
import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
import { hospitalityDayControls as control, type HospitalityDayControlRepository, type Count } from "@erpx/server-pms/application/hospitality-day-controls";
import { pmsDayControlPosFacts } from "@erpx/server-pos/public/pms-day-control-facts";
import { pmsDayControlFinanceFacts } from "@erpx/server-finance/public/pms-day-control-facts";
import { nativePmsReadFirst, PMS_RD_CHARGE_CORRECTION_PENDING_COUNT, PMS_RD_PROPERTY_CONFIG, PMS_RD_PENDING_HOTEL_TRANSFERS, PMS_RD_UNPOSTED_BOOKING_RECEIPTS, PMS_RD_UNPOSTED_COUNTER_RECEIPTS, PMS_RD_FAILED_COUNTER_REFUNDS } from "./native-pms-read";
import { pmsRefundPaymentJoinHeadSql } from "./pms-read-tail-r34-sql";
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
        }) => nativePmsReadFirst<Count>(PMS_RD_PENDING_HOTEL_TRANSFERS, [facts.companyId, facts.propertyId, facts.businessDate], db).then(row => Number(row?.n ?? 0)),
        pendingChargeCorrections: (facts: {
            companyId: string;
            propertyId: string;
        }) => nativePmsReadFirst<Count>(PMS_RD_CHARGE_CORRECTION_PENDING_COUNT, [facts.companyId, facts.propertyId], db).then(async row => Number(row?.n ?? 0) + await pendingHotelRoomChargeReversals(db,facts.companyId,facts.propertyId)),
        property: (facts: {
            propertyId: string;
            companyId: string;
        }) => nativePmsReadFirst<{ config_json: string }>(PMS_RD_PROPERTY_CONFIG, [facts.propertyId, facts.companyId], db),
        unpostedBookingReceipts: (facts: {
            propertyId: string;
            companyId: string;
            timezone: string;
            businessDate: string;
        }) => nativePmsReadFirst<Count>(PMS_RD_UNPOSTED_BOOKING_RECEIPTS, [facts.propertyId, facts.companyId, facts.timezone, facts.businessDate], db).then(row => Number(row?.n ?? 0)),
        failedBookingRefunds: (facts: {
            propertyId: string;
            companyId: string;
            timezone: string;
            businessDate: string;
        }) => db.prepare(`${pmsRefundPaymentJoinHeadSql('count')} WHERE p.property_id=? AND p.company_id=?
    AND (r.state='failed' OR (r.state IN ('approved','processing','requires_action') AND r.last_error IS NOT NULL))
    AND to_char(to_timestamp(r.created_at/1000.0) AT TIME ZONE ?,'YYYY-MM-DD')<=?`).bind(facts.propertyId, facts.companyId, facts.timezone, facts.businessDate).first<Count>().then(row => Number(row?.n ?? 0)),
        unpostedCounterReceipts: (facts: {
            propertyId: string;
            companyId: string;
            timezone: string;
            businessDate: string;
        }) => nativePmsReadFirst<Count>(PMS_RD_UNPOSTED_COUNTER_RECEIPTS, [facts.propertyId, facts.companyId, facts.timezone, facts.businessDate], db).then(row => Number(row?.n ?? 0)),
        failedCounterRefunds: (facts: {
            propertyId: string;
            companyId: string;
            timezone: string;
            businessDate: string;
        }) => nativePmsReadFirst<Count>(PMS_RD_FAILED_COUNTER_REFUNDS, [facts.propertyId, facts.companyId, facts.timezone, facts.businessDate], db).then(row => Number(row?.n ?? 0)),
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
