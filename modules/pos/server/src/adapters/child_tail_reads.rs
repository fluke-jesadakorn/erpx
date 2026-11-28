//! Native POS child-tail read kinds (round 30). These closed, `SELECT`-only
//! owner statements replace the byte-identical inline TypeScript literals for
//! the line sum, line created-by, completed line and refund-payment tails in
//! `pos-core.ts`. The bridge sends a typed kind and positional values; this
//! module selects the SQL. There is no TypeScript SQL, mirror or fallback for
//! this family. Row locks, PMS projection fragments, the day-report registry
//! and the Stripe registry stay inline in their callers and modules.

pub const LINES_SUM: &str = "pos_child_tail_lines_sum";
pub const CREATED_BY: &str = "pos_child_tail_created_by";
pub const COMPLETED_LINE: &str = "pos_child_tail_completed_line";
pub const REFUND_PAYMENT: &str = "pos_child_tail_refund_payment";

pub const KINDS: &[&str] = &[
    LINES_SUM,
    CREATED_BY,
    COMPLETED_LINE,
    REFUND_PAYMENT,
];

pub fn supported(kind: &str) -> bool {
    KINDS.contains(&kind)
}

/// Authoritative SQL for the four child-tail kinds, byte-identical to the
/// retired TypeScript literals and the transport parity oracle. Every statement
/// is `SELECT`-only, parameterized and lock-free; no statement interpolates a
/// cross-owner projection fragment.
pub fn sql(kind: &str) -> Option<&'static str> {
    Some(match kind {
        LINES_SUM => "SELECT COALESCE(SUM(quantity_milli),0) n FROM pos_lines WHERE fulfillment_id=? AND id<?",
        CREATED_BY => "SELECT s.created_by FROM pos_lines l JOIN pos_sales s ON s.id=l.sale_id WHERE l.fulfillment_id=? AND s.created_by=? LIMIT 1",
        COMPLETED_LINE => "SELECT l.id FROM pos_lines l JOIN pos_sales s ON s.id=l.sale_id WHERE l.fulfillment_id=? AND s.state='completed' AND l.returned_milli<l.quantity_milli",
        REFUND_PAYMENT => "SELECT rp.id FROM pos_refund_payments rp JOIN pos_payments p ON p.id=rp.payment_id WHERE p.sale_id=? AND rp.state<>'succeeded'",
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Expected strings are the exact interpolated TypeScript literals retired
    /// from `pos-core.ts`; the transport parity suite executes the same registry
    /// statement against the caller's unit.
    const EXPECTED: &[(&str, &str, usize)] = &[
        (LINES_SUM, "SELECT COALESCE(SUM(quantity_milli),0) n FROM pos_lines WHERE fulfillment_id=? AND id<?", 2),
        (CREATED_BY, "SELECT s.created_by FROM pos_lines l JOIN pos_sales s ON s.id=l.sale_id WHERE l.fulfillment_id=? AND s.created_by=? LIMIT 1", 2),
        (COMPLETED_LINE, "SELECT l.id FROM pos_lines l JOIN pos_sales s ON s.id=l.sale_id WHERE l.fulfillment_id=? AND s.state='completed' AND l.returned_milli<l.quantity_milli", 1),
        (REFUND_PAYMENT, "SELECT rp.id FROM pos_refund_payments rp JOIN pos_payments p ON p.id=rp.payment_id WHERE p.sale_id=? AND rp.state<>'succeeded'", 1),
    ];

    #[test]
    fn child_tail_sql_is_byte_identical_with_expected_binds() {
        assert_eq!(KINDS.len(), 4);
        assert_eq!(EXPECTED.len(), 4);
        for (kind, expected, binds) in EXPECTED {
            assert!(supported(kind), "child-tail kind must be allow listed: {kind}");
            let stmt = sql(kind).unwrap_or_else(|| panic!("missing SQL for {kind}"));
            assert_eq!(stmt, *expected, "child-tail SQL drifted for {kind}");
            assert_eq!(stmt.matches('?').count(), *binds, "bind count drifted for {kind}");
        }
    }

    #[test]
    fn child_tail_kinds_are_select_only_and_lock_free() {
        for kind in KINDS {
            let stmt = sql(kind).unwrap_or_else(|| panic!("missing SQL for {kind}"));
            assert!(stmt.starts_with("SELECT "), "kind must be SELECT-only: {kind}");
            assert!(stmt.contains('?'), "kind must be parameterized: {kind}");
            for forbidden in [
                "INSERT", "UPDATE ", "DELETE", "RETURNING", ";", "FOR UPDATE", "FOR SHARE",
                "FOR NO KEY UPDATE", "LOCK TABLE", "pg_advisory", "pg_try_advisory",
                "pms_sources", "pms_properties", "commerce_", "pos_day_closes", "pos_stripe_routes",
            ] {
                assert!(
                    !stmt.contains(forbidden),
                    "child-tail kind must not mutate, lock or cross owners: {kind} contains {forbidden}",
                );
            }
        }
    }

    #[test]
    fn unknown_child_tail_kind_fails_closed() {
        assert!(!supported("pos_child_tail_unlisted"));
        assert!(sql("pos_child_tail_unlisted").is_none());
    }
}
