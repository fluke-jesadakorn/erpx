//! Native POS sale-child detail reads keyed by sale id. These closed,
//! `SELECT`-only owner statements are the runtime authority for the thirteen
//! lock-free child reads in `pos.ts` (`readPosLines2`, `readPosPayments2`) and
//! `pos-core.ts` (`readPosLines1`, `readPosPayments2`, `readPosPayments3`,
//! `readPosPayments4`, `readPosPayments6`, `readPosLines2`, `readPosPayments7`,
//! `readPosLines5`, `readPosPayments17`) and `pos-daily.ts` (`readPosLines1`,
//! `readPosPayments1`). Every statement is byte-identical to the retired
//! TypeScript literal it replaces. The bridge sends a typed kind
//! and one positional sale id; this module selects the SQL. There is no
//! TypeScript SQL, mirror or fallback for this family. The lock-bearing sale
//! reads, advisory/LOCK reads, PMS projection fragments, sale-document reads
//! (also Company-scoped), room-charge reversal reads and refund-child reads are
//! deliberately not part of this family and stay inline in the caller.

pub const LINES_WITH_FULFILLMENT: &str = "pos_sale_child_lines_with_fulfillment";
pub const PAYMENTS_ORDERED: &str = "pos_sale_child_payments_ordered";
pub const LINES_ALL: &str = "pos_sale_child_lines_all";
pub const PAYMENTS_SUCCEEDED_TOTAL: &str = "pos_sale_child_payments_succeeded_total";
pub const PAYMENT_PENDING_ID: &str = "pos_sale_child_payment_pending_id";
pub const PAYMENTS_REFUNDABLE: &str = "pos_sale_child_payments_refundable";
pub const PAYMENT_ID: &str = "pos_sale_child_payment_id";
pub const LINES_FULFILLMENT_DETAIL: &str = "pos_sale_child_lines_fulfillment_detail";
pub const PAYMENT_FIRST_ID: &str = "pos_sale_child_payment_first_id";
pub const LINES_TOTAL: &str = "pos_sale_child_lines_total";
pub const PAYMENT_OPEN_ID: &str = "pos_sale_child_payment_open_id";
pub const DAY_PLANNER_LINES: &str = "pos_sale_child_day_planner_lines";
pub const DAY_PLANNER_PAYMENTS: &str = "pos_sale_child_day_planner_payments";

pub const KINDS: &[&str] = &[
    LINES_WITH_FULFILLMENT,
    PAYMENTS_ORDERED,
    LINES_ALL,
    PAYMENTS_SUCCEEDED_TOTAL,
    PAYMENT_PENDING_ID,
    PAYMENTS_REFUNDABLE,
    PAYMENT_ID,
    LINES_FULFILLMENT_DETAIL,
    PAYMENT_FIRST_ID,
    LINES_TOTAL,
    PAYMENT_OPEN_ID,
    DAY_PLANNER_LINES,
    DAY_PLANNER_PAYMENTS,
];

pub fn supported(kind: &str) -> bool {
    KINDS.contains(&kind)
}

/// Authoritative SQL for the thirteen sale-child detail kinds, byte-identical to
/// the retired TypeScript literals and the transport parity oracle. Every
/// statement is a caller-transaction `SELECT` scoped by one caller-supplied
/// sale id. No statement carries a row lock, an advisory lock or a `LOCK TABLE`
/// clause, and none interpolates a cross-owner projection fragment.
pub fn sql(kind: &str) -> Option<&'static str> {
    Some(match kind {
        LINES_WITH_FULFILLMENT => "SELECT l.*,f.state fulfillment_state,f.version fulfillment_version,f.kind,f.cost_minor fulfillment_cost FROM pos_lines l JOIN pos_fulfillments f ON f.id=l.fulfillment_id WHERE l.sale_id=? ORDER BY l.id",
        PAYMENTS_ORDERED => "SELECT * FROM pos_payments WHERE sale_id=? ORDER BY created_at",
        LINES_ALL => "SELECT * FROM pos_lines WHERE sale_id=?",
        PAYMENTS_SUCCEEDED_TOTAL => "SELECT COALESCE(SUM(amount_minor-refunded_minor),0) amount FROM pos_payments WHERE sale_id=? AND state='succeeded'",
        PAYMENT_PENDING_ID => "SELECT id FROM pos_payments WHERE sale_id=? AND state IN ('requested','pending','exception')",
        PAYMENTS_REFUNDABLE => "SELECT * FROM pos_payments WHERE sale_id=? AND state='succeeded' AND amount_minor>refunded_minor",
        PAYMENT_ID => "SELECT id FROM pos_payments WHERE sale_id=?",
        LINES_FULFILLMENT_DETAIL => "SELECT l.*,f.kind,f.state fulfillment_state,f.cost_minor fulfillment_cost,f.quantity_milli fulfillment_quantity,f.warehouse_id,f.product_id FROM pos_lines l JOIN pos_fulfillments f ON f.id=l.fulfillment_id WHERE l.sale_id=? ORDER BY l.id",
        PAYMENT_FIRST_ID => "SELECT id FROM pos_payments WHERE sale_id=? LIMIT 1",
        LINES_TOTAL => "SELECT COALESCE(SUM(subtotal_minor+tax_minor+discount_minor),0) n FROM pos_lines WHERE sale_id=?",
        PAYMENT_OPEN_ID => "SELECT id FROM pos_payments WHERE sale_id=? AND (state IN ('requested','pending','exception') OR state='succeeded' AND amount_minor>refunded_minor)",
        DAY_PLANNER_LINES => "SELECT id,name,quantity_milli,price_minor,tax_bps,tax_inclusive,discount_minor,subtotal_minor,tax_minor,modifiers_json,note FROM pos_lines WHERE sale_id=? ORDER BY id",
        DAY_PLANNER_PAYMENTS => "SELECT id,method,amount_minor,tendered_minor,reference,finance_payment_id FROM pos_payments WHERE sale_id=? AND state='succeeded' ORDER BY created_at,id",
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Byte-for-byte SQL and placeholder count for each kind. Expected strings
    /// are the exact interpolated TypeScript literals retired from `pos.ts`
    /// and `pos-core.ts`; the transport parity suite executes the same
    /// registry statement against the caller's unit.
    const EXPECTED: &[(&str, &str, usize)] = &[
        (LINES_WITH_FULFILLMENT, "SELECT l.*,f.state fulfillment_state,f.version fulfillment_version,f.kind,f.cost_minor fulfillment_cost FROM pos_lines l JOIN pos_fulfillments f ON f.id=l.fulfillment_id WHERE l.sale_id=? ORDER BY l.id", 1),
        (PAYMENTS_ORDERED, "SELECT * FROM pos_payments WHERE sale_id=? ORDER BY created_at", 1),
        (LINES_ALL, "SELECT * FROM pos_lines WHERE sale_id=?", 1),
        (PAYMENTS_SUCCEEDED_TOTAL, "SELECT COALESCE(SUM(amount_minor-refunded_minor),0) amount FROM pos_payments WHERE sale_id=? AND state='succeeded'", 1),
        (PAYMENT_PENDING_ID, "SELECT id FROM pos_payments WHERE sale_id=? AND state IN ('requested','pending','exception')", 1),
        (PAYMENTS_REFUNDABLE, "SELECT * FROM pos_payments WHERE sale_id=? AND state='succeeded' AND amount_minor>refunded_minor", 1),
        (PAYMENT_ID, "SELECT id FROM pos_payments WHERE sale_id=?", 1),
        (LINES_FULFILLMENT_DETAIL, "SELECT l.*,f.kind,f.state fulfillment_state,f.cost_minor fulfillment_cost,f.quantity_milli fulfillment_quantity,f.warehouse_id,f.product_id FROM pos_lines l JOIN pos_fulfillments f ON f.id=l.fulfillment_id WHERE l.sale_id=? ORDER BY l.id", 1),
        (PAYMENT_FIRST_ID, "SELECT id FROM pos_payments WHERE sale_id=? LIMIT 1", 1),
        (LINES_TOTAL, "SELECT COALESCE(SUM(subtotal_minor+tax_minor+discount_minor),0) n FROM pos_lines WHERE sale_id=?", 1),
        (PAYMENT_OPEN_ID, "SELECT id FROM pos_payments WHERE sale_id=? AND (state IN ('requested','pending','exception') OR state='succeeded' AND amount_minor>refunded_minor)", 1),
        (DAY_PLANNER_LINES, "SELECT id,name,quantity_milli,price_minor,tax_bps,tax_inclusive,discount_minor,subtotal_minor,tax_minor,modifiers_json,note FROM pos_lines WHERE sale_id=? ORDER BY id", 1),
        (DAY_PLANNER_PAYMENTS, "SELECT id,method,amount_minor,tendered_minor,reference,finance_payment_id FROM pos_payments WHERE sale_id=? AND state='succeeded' ORDER BY created_at,id", 1),
    ];

    #[test]
    fn sale_child_sql_is_byte_identical_with_expected_binds() {
        assert_eq!(KINDS.len(), 13);
        assert_eq!(EXPECTED.len(), 13);
        for (kind, expected, binds) in EXPECTED {
            assert!(supported(kind), "sale-child kind must be allow listed: {kind}");
            let stmt = sql(kind).unwrap_or_else(|| panic!("missing SQL for {kind}"));
            assert_eq!(stmt, *expected, "sale-child SQL drifted for {kind}");
            assert_eq!(stmt.matches('?').count(), *binds, "bind count drifted for {kind}");
        }
    }

    #[test]
    fn sale_child_kinds_are_select_only_and_lock_free() {
        for kind in KINDS {
            let stmt = sql(kind).unwrap_or_else(|| panic!("missing SQL for {kind}"));
            assert!(stmt.starts_with("SELECT "), "kind must be SELECT-only: {kind}");
            assert!(stmt.contains("sale_id=?"), "kind must be scoped by sale id: {kind}");
            for forbidden in [
                "INSERT", "UPDATE ", "DELETE", "RETURNING", ";", "FOR UPDATE", "FOR SHARE",
                "FOR NO KEY UPDATE", "LOCK TABLE", "pg_advisory", "pg_try_advisory",
                "pms_sources", "pms_properties", "commerce_",
            ] {
                assert!(
                    !stmt.contains(forbidden),
                    "sale-child kind must not mutate, lock or cross owners: {kind} contains {forbidden}",
                );
            }
        }
    }

    #[test]
    fn unknown_sale_child_kind_fails_closed() {
        assert!(!supported("pos_sale_child_unlisted"));
        assert!(sql("pos_sale_child_unlisted").is_none());
    }
}
