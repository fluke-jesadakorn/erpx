//! Native POS day-report business-date list reads. These closed, `SELECT`-only
//! owner statements are the runtime authority for the ten lock-free list reads
//! in `pos-daily.ts` (`readPosDayCloses1`, `readPosSales2`, `readPosShifts1`,
//! `readPosPayments2`, `readPosRefunds1`, `readPosRefundPayments1`,
//! `readPosStockMoves1`, `readPosFulfillments1`, `readPosCashMovements1`,
//! `readPosSettlements1`). Every statement is byte-identical to the retired
//! TypeScript literal it replaces, including the two that inline closed
//! cross-owner projections (`pms_sources`, `commerce_warehouses`). The bridge
//! sends a typed kind and positional values; this module selects the SQL. There
//! is no TypeScript SQL, mirror or fallback for this family. The lock-bearing
//! `readPosDayCloses2` (`FOR UPDATE`) and the PMS point-lookup `readPmsSources1`
//! are deliberately not part of this family and stay inline.

pub const DAY_CLOSES: &str = "pos_day_report_day_closes";
pub const SALES: &str = "pos_day_report_sales";
pub const SHIFTS: &str = "pos_day_report_shifts";
pub const PAYMENTS: &str = "pos_day_report_payments";
pub const REFUNDS: &str = "pos_day_report_refunds";
pub const REFUND_PAYMENTS: &str = "pos_day_report_refund_payments";
pub const STOCK_MOVES: &str = "pos_day_report_stock_moves";
pub const FULFILLMENTS: &str = "pos_day_report_fulfillments";
pub const CASH_MOVEMENTS: &str = "pos_day_report_cash_movements";
pub const SETTLEMENTS: &str = "pos_day_report_settlements";

pub const KINDS: &[&str] = &[
    DAY_CLOSES,
    SALES,
    SHIFTS,
    PAYMENTS,
    REFUNDS,
    REFUND_PAYMENTS,
    STOCK_MOVES,
    FULFILLMENTS,
    CASH_MOVEMENTS,
    SETTLEMENTS,
];

pub fn supported(kind: &str) -> bool {
    KINDS.contains(&kind)
}

/// Authoritative SQL for the ten day-report list kinds, byte-identical to the
/// retired TypeScript literals and the integration parity oracle. Every
/// statement is a caller-transaction `SELECT` scoped by the caller-supplied
/// Company/branch/business-date values. No statement carries a row lock, an
/// advisory lock or a `LOCK TABLE` clause.
pub fn sql(kind: &str) -> Option<&'static str> {
    Some(match kind {
        DAY_CLOSES => "SELECT id FROM pos_day_closes WHERE company_id=? AND branch_id=? AND business_date=?",
        SALES => concat!(
            "SELECT s.*,ps.record_id room_charge_id,rr.reversal_entry_id FROM pos_sales s ",
            "LEFT JOIN (SELECT \"property_id\",\"source_key\",\"record_id\" FROM pms_sources) ps ",
            "ON ps.source_key='pos:'||s.id||':'||s.id ",
            "LEFT JOIN pos_room_charge_reversals rr ON rr.sale_id=s.id ",
            "WHERE s.company_id=? AND s.branch_id=? AND s.business_date=? ORDER BY s.id",
        ),
        SHIFTS => "SELECT * FROM pos_shifts WHERE company_id=? AND branch_id=? AND business_date=? ORDER BY id",
        PAYMENTS => "SELECT p.*,s.number sale_number FROM pos_payments p JOIN pos_sales s ON s.id=p.sale_id WHERE s.company_id=? AND s.branch_id=? AND s.business_date=? ORDER BY p.id",
        REFUNDS => "SELECT r.*,s.number sale_number FROM pos_refunds r JOIN pos_sales s ON s.id=r.sale_id WHERE r.company_id=? AND r.branch_id=? AND r.accounting_date=? ORDER BY r.id",
        REFUND_PAYMENTS => "SELECT rp.*,p.method FROM pos_refund_payments rp JOIN pos_refunds r ON r.id=rp.refund_id JOIN pos_payments p ON p.id=rp.payment_id WHERE r.company_id=? AND r.branch_id=? AND r.accounting_date=? ORDER BY rp.id",
        STOCK_MOVES => concat!(
            "SELECT * FROM pos_stock_moves WHERE company_id=? AND movement_date=? ",
            "AND warehouse_id IN (SELECT id FROM ",
            "(SELECT \"id\",\"company_id\",\"branch_id\",\"code\",\"name\",\"status\",\"version\" FROM commerce_warehouses) commerce_warehouses ",
            "WHERE company_id=? AND branch_id=?) ORDER BY id",
        ),
        FULFILLMENTS => "SELECT id,state FROM pos_fulfillments WHERE company_id=? AND branch_id=? AND business_date=? AND state IN ('queued','preparing','ready') ORDER BY id",
        CASH_MOVEMENTS => "SELECT m.id FROM pos_cash_movements m JOIN pos_shifts s ON s.id=m.shift_id WHERE s.company_id=? AND s.branch_id=? AND s.business_date=? AND m.approved_by IS NULL ORDER BY m.id",
        SETTLEMENTS => "SELECT id,kind,state,amount_minor,fee_minor,amount_minor-fee_minor net_minor FROM pos_settlements WHERE company_id=? AND ((kind='payout' AND state<>'matched') OR (kind='balance' AND state='unmapped')) ORDER BY id",
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Byte-for-byte SQL and placeholder count for each of the ten kinds. The
    /// expected strings are the exact interpolated TypeScript literals retired
    /// from `pos-daily.ts`; the integration parity suite executes the same
    /// registry statement against a seeded database.
    const EXPECTED: &[(&str, &str, usize)] = &[
        (DAY_CLOSES, "SELECT id FROM pos_day_closes WHERE company_id=? AND branch_id=? AND business_date=?", 3),
        (
            SALES,
            concat!(
                "SELECT s.*,ps.record_id room_charge_id,rr.reversal_entry_id FROM pos_sales s ",
                "LEFT JOIN (SELECT \"property_id\",\"source_key\",\"record_id\" FROM pms_sources) ps ",
                "ON ps.source_key='pos:'||s.id||':'||s.id ",
                "LEFT JOIN pos_room_charge_reversals rr ON rr.sale_id=s.id ",
                "WHERE s.company_id=? AND s.branch_id=? AND s.business_date=? ORDER BY s.id",
            ),
            3,
        ),
        (SHIFTS, "SELECT * FROM pos_shifts WHERE company_id=? AND branch_id=? AND business_date=? ORDER BY id", 3),
        (PAYMENTS, "SELECT p.*,s.number sale_number FROM pos_payments p JOIN pos_sales s ON s.id=p.sale_id WHERE s.company_id=? AND s.branch_id=? AND s.business_date=? ORDER BY p.id", 3),
        (REFUNDS, "SELECT r.*,s.number sale_number FROM pos_refunds r JOIN pos_sales s ON s.id=r.sale_id WHERE r.company_id=? AND r.branch_id=? AND r.accounting_date=? ORDER BY r.id", 3),
        (REFUND_PAYMENTS, "SELECT rp.*,p.method FROM pos_refund_payments rp JOIN pos_refunds r ON r.id=rp.refund_id JOIN pos_payments p ON p.id=rp.payment_id WHERE r.company_id=? AND r.branch_id=? AND r.accounting_date=? ORDER BY rp.id", 3),
        (
            STOCK_MOVES,
            concat!(
                "SELECT * FROM pos_stock_moves WHERE company_id=? AND movement_date=? ",
                "AND warehouse_id IN (SELECT id FROM ",
                "(SELECT \"id\",\"company_id\",\"branch_id\",\"code\",\"name\",\"status\",\"version\" FROM commerce_warehouses) commerce_warehouses ",
                "WHERE company_id=? AND branch_id=?) ORDER BY id",
            ),
            4,
        ),
        (FULFILLMENTS, "SELECT id,state FROM pos_fulfillments WHERE company_id=? AND branch_id=? AND business_date=? AND state IN ('queued','preparing','ready') ORDER BY id", 3),
        (CASH_MOVEMENTS, "SELECT m.id FROM pos_cash_movements m JOIN pos_shifts s ON s.id=m.shift_id WHERE s.company_id=? AND s.branch_id=? AND s.business_date=? AND m.approved_by IS NULL ORDER BY m.id", 3),
        (SETTLEMENTS, "SELECT id,kind,state,amount_minor,fee_minor,amount_minor-fee_minor net_minor FROM pos_settlements WHERE company_id=? AND ((kind='payout' AND state<>'matched') OR (kind='balance' AND state='unmapped')) ORDER BY id", 1),
    ];

    #[test]
    fn day_report_sql_is_byte_identical_with_expected_binds() {
        assert_eq!(KINDS.len(), 10);
        assert_eq!(EXPECTED.len(), 10);
        for (kind, expected, binds) in EXPECTED {
            assert!(supported(kind), "day-report kind must be allow listed: {kind}");
            let stmt = sql(kind).unwrap_or_else(|| panic!("missing SQL for {kind}"));
            assert_eq!(stmt, *expected, "day-report SQL drifted for {kind}");
            assert_eq!(stmt.matches('?').count(), *binds, "bind count drifted for {kind}");
        }
    }

    #[test]
    fn day_report_kinds_are_select_only_and_lock_free() {
        for kind in KINDS {
            let stmt = sql(kind).unwrap_or_else(|| panic!("missing SQL for {kind}"));
            assert!(stmt.starts_with("SELECT "), "kind must be SELECT-only: {kind}");
            assert!(stmt.contains('?'), "kind must be parameterized: {kind}");
            for forbidden in [
                "INSERT", "UPDATE ", "DELETE", "RETURNING", ";", "FOR UPDATE", "FOR SHARE",
                "FOR NO KEY UPDATE", "LOCK TABLE", "pg_advisory", "pg_try_advisory",
            ] {
                assert!(
                    !stmt.contains(forbidden),
                    "day-report kind must not mutate or lock: {kind} contains {forbidden}",
                );
            }
        }
    }

    #[test]
    fn unknown_day_report_kind_fails_closed() {
        assert!(!supported("pos_day_report_unlisted"));
        assert!(sql("pos_day_report_unlisted").is_none());
    }
}
