import "server-only";
import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
import type { Count, Totals } from "@erpx/server-pms/public/hospitality-day-control-contracts";
import { nativePosOne } from "./native-owner-write";

/**
 * POS-owned day-control facts. Every statement is issued by the native POS
 * owner (`erpx-server-pos::adapters::day_control_reads`) and executed on the
 * caller's serializable unit: no TypeScript SQL, no mirror and no fallback.
 * The kind is a closed owner label; only positional values come from facts.
 */
export function pmsDayControlPosFacts(db: SqlDatabase) {
    return {
        openShifts: (facts: {
            propertyId: string;
            scope: [
                string,
                string
            ];
        }) => nativePosOne<Count>(db, facts.scope[0], 'pos_daycontrol_open_shifts', [...facts.scope, facts.propertyId]).then(row => Number(row?.n ?? 0)),
        unreviewedVariances: (facts: {
            propertyId: string;
            scope: [
                string,
                string
            ];
        }) => nativePosOne<Count>(db, facts.scope[0], 'pos_daycontrol_unreviewed_variances', [...facts.scope, facts.propertyId]).then(row => Number(row?.n ?? 0)),
        openBills: (facts: {
            propertyId: string;
            scope: [
                string,
                string
            ];
            businessDate: string;
        }) => nativePosOne<Count>(db, facts.scope[0], 'pos_daycontrol_open_bills', [...facts.scope, facts.businessDate, facts.propertyId]).then(row => Number(row?.n ?? 0)),
        pendingPayments: (facts: {
            propertyId: string;
            scope: [
                string,
                string
            ];
            businessDate: string;
        }) => nativePosOne<Count>(db, facts.scope[0], 'pos_daycontrol_pending_payments', [...facts.scope, facts.businessDate, facts.propertyId]).then(row => Number(row?.n ?? 0)),
        unresolvedRefunds: (facts: {
            propertyId: string;
            scope: [
                string,
                string
            ];
            businessDate: string;
        }) => nativePosOne<Count>(db, facts.scope[0], 'pos_daycontrol_unresolved_refunds', [...facts.scope, facts.businessDate, facts.propertyId]).then(row => Number(row?.n ?? 0)),
        failedJobs: (facts: {
            propertyId: string;
            companyId: string;
        }) => nativePosOne<Count>(db, facts.companyId, 'pos_daycontrol_failed_jobs', [facts.companyId, facts.propertyId]).then(row => Number(row?.n ?? 0)),
        missingRoomCharges: (facts: {
            propertyId: string;
            scope: [
                string,
                string
            ];
            businessDate: string;
        }) => nativePosOne<Count>(db, facts.scope[0], 'pos_daycontrol_missing_room_charges', [...facts.scope, facts.businessDate, facts.propertyId, facts.propertyId]).then(row => Number(row?.n ?? 0)),
        totals: (facts: {
            propertyId: string;
            scope: [
                string,
                string
            ];
            businessDate: string;
        }) => nativePosOne<Totals>(db, facts.scope[0], 'pos_daycontrol_totals', [...facts.scope, facts.businessDate, facts.propertyId]),
        stockMoves: (facts: {
            propertyId: string;
            scope: [
                string,
                string
            ];
            businessDate: string;
        }) => nativePosOne<Count>(db, facts.scope[0], 'pos_daycontrol_stock_moves', [...facts.scope, facts.businessDate, facts.propertyId])
    };
}

export async function pendingHotelRoomChargeReversals(db: SqlDatabase, companyId: string, propertyId: string) {
    const row = await nativePosOne<Count>(db, companyId, 'pos_daycontrol_pending_reversals', [companyId, propertyId]);
    return Number(row?.n ?? 0);
}
