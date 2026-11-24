//! Native POS hotel day-assertion property reads (round 32). These closed,
//! `SELECT`-only owner statements replace the byte-identical inline TypeScript
//! literals in `pos-core.ts` (`assertHotelDay` and `assertFulfillmentHotelDay`).
//! Both resolve a POS-owned register's `property_id,branch_id` pair on the
//! caller's transaction: one directly by register id, one through the sale's
//! lines/fulfillment. The bridge sends a typed kind and positional values; this
//! module selects the SQL. There is no TypeScript SQL, mirror or fallback for
//! this family. Row locks, advisory/LOCK reads, PMS projection fragments, the
//! day-report registry and the Stripe registry stay inline in their callers.

pub const REGISTER_PROPERTY: &str = "pos_hotel_register_property";
pub const FULFILLMENT_PROPERTY: &str = "pos_hotel_fulfillment_property";

pub const KINDS: &[&str] = &[
    REGISTER_PROPERTY,
    FULFILLMENT_PROPERTY,
];

pub fn supported(kind: &str) -> bool {
    KINDS.contains(&kind)
}

/// Authoritative SQL for the two hotel property kinds, byte-identical to the
/// retired TypeScript literals. Every statement is `SELECT`-only, parameterized
/// by a caller-supplied Company id and lock-free; no statement interpolates a
/// cross-owner projection fragment.
pub fn sql(kind: &str) -> Option<&'static str> {
    Some(match kind {
        REGISTER_PROPERTY => "SELECT property_id,branch_id FROM pos_registers WHERE company_id=? AND id=?",
        FULFILLMENT_PROPERTY => "SELECT DISTINCT r.property_id,r.branch_id FROM pos_lines l JOIN pos_sales s ON s.id=l.sale_id JOIN pos_registers r ON r.id=s.register_id WHERE s.company_id=? AND l.fulfillment_id=?",
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Expected strings are the exact inline TypeScript literals retired from
    /// `pos-core.ts` at BEFORE=191; the transport parity suite executes the same
    /// registry statement against the caller's unit.
    const EXPECTED: &[(&str, &str, usize)] = &[
        (REGISTER_PROPERTY, "SELECT property_id,branch_id FROM pos_registers WHERE company_id=? AND id=?", 2),
        (FULFILLMENT_PROPERTY, "SELECT DISTINCT r.property_id,r.branch_id FROM pos_lines l JOIN pos_sales s ON s.id=l.sale_id JOIN pos_registers r ON r.id=s.register_id WHERE s.company_id=? AND l.fulfillment_id=?", 2),
    ];

    #[test]
    fn hotel_property_sql_is_byte_identical_with_expected_binds() {
        assert_eq!(KINDS.len(), 2);
        assert_eq!(EXPECTED.len(), 2);
        for (kind, expected, binds) in EXPECTED {
            assert!(supported(kind), "hotel property kind must be allow listed: {kind}");
            let stmt = sql(kind).unwrap_or_else(|| panic!("missing SQL for {kind}"));
            assert_eq!(stmt, *expected, "hotel property SQL drifted for {kind}");
            assert_eq!(stmt.matches('?').count(), *binds, "bind count drifted for {kind}");
        }
    }

    #[test]
    fn hotel_property_kinds_are_select_only_and_lock_free() {
        for kind in KINDS {
            let stmt = sql(kind).unwrap_or_else(|| panic!("missing SQL for {kind}"));
            assert!(stmt.starts_with("SELECT "), "kind must be SELECT-only: {kind}");
            assert!(stmt.contains("company_id=?"), "kind must be Company-scoped: {kind}");
            for forbidden in [
                "INSERT", "UPDATE ", "DELETE", "RETURNING", ";", "FOR UPDATE", "FOR SHARE",
                "FOR NO KEY UPDATE", "LOCK TABLE", "pg_advisory", "pg_try_advisory",
                "pms_sources", "pms_properties", "commerce_", "pos_day_closes", "pos_stripe_routes",
            ] {
                assert!(
                    !stmt.contains(forbidden),
                    "hotel property kind must not mutate, lock or cross owners: {kind} contains {forbidden}",
                );
            }
        }
    }

    #[test]
    fn unknown_hotel_property_kind_fails_closed() {
        assert!(!supported("pos_hotel_unlisted"));
        assert!(sql("pos_hotel_unlisted").is_none());
    }
}
