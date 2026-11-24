//! Native POS list-read kinds (round 31). These closed, `SELECT`-only owner
//! statements replace the byte-identical inline TypeScript literals for the
//! sales list, refunds list, PMS booking connection row and settlements list in
//! `pos.ts`. The bridge sends a typed kind and positional values; this module
//! selects the SQL. There is no TypeScript SQL, mirror or fallback for this
//! family. Row locks, PMS projection fragments, the day-report registry and
//! the Stripe registry stay inline in their callers and modules.

pub const SALES: &str = "pos_list_sales";
pub const REFUNDS: &str = "pos_list_refunds";
pub const CONNECTIONS: &str = "pos_list_connections";
pub const SETTLEMENTS: &str = "pos_list_settlements";

pub const KINDS: &[&str] = &[
    SALES,
    REFUNDS,
    CONNECTIONS,
    SETTLEMENTS,
];

pub fn supported(kind: &str) -> bool {
    KINDS.contains(&kind)
}

/// Authoritative SQL for the four list-read kinds, byte-identical to the
/// retired TypeScript literals and the transport parity oracle. Every statement
/// is `SELECT`-only, parameterized and lock-free; no statement interpolates a
/// cross-owner projection fragment.
pub fn sql(kind: &str) -> Option<&'static str> {
    Some(match kind {
        SALES => "SELECT s.*,r.name register_name FROM pos_sales s JOIN pos_registers r ON r.id=s.register_id WHERE s.company_id=? AND (?::text IS NULL OR s.branch_id=?) AND (?='' OR s.number ILIKE ?) AND (?=0 OR s.state IN ('open','held','payment_pending')) ORDER BY s.created_at DESC LIMIT 150",
        REFUNDS => "SELECT f.*,s.number sale_number,s.currency FROM pos_refunds f JOIN pos_sales s ON s.id=f.sale_id WHERE f.company_id=? AND (?::text IS NULL OR f.branch_id=?) AND (?::text IS NULL OR f.sale_id=?) ORDER BY f.created_at DESC LIMIT 100",
        CONNECTIONS => "SELECT * FROM pos_connections WHERE company_id=? AND state='ready'",
        SETTLEMENTS => "SELECT * FROM pos_settlements WHERE company_id=? ORDER BY created_at DESC LIMIT 100",
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Expected strings are the exact interpolated TypeScript literals retired
    /// from `pos.ts`; the transport parity suite executes the same registry
    /// statement against the caller's unit.
    const EXPECTED: &[(&str, &str, usize)] = &[
        (SALES, "SELECT s.*,r.name register_name FROM pos_sales s JOIN pos_registers r ON r.id=s.register_id WHERE s.company_id=? AND (?::text IS NULL OR s.branch_id=?) AND (?='' OR s.number ILIKE ?) AND (?=0 OR s.state IN ('open','held','payment_pending')) ORDER BY s.created_at DESC LIMIT 150", 6),
        (REFUNDS, "SELECT f.*,s.number sale_number,s.currency FROM pos_refunds f JOIN pos_sales s ON s.id=f.sale_id WHERE f.company_id=? AND (?::text IS NULL OR f.branch_id=?) AND (?::text IS NULL OR f.sale_id=?) ORDER BY f.created_at DESC LIMIT 100", 5),
        (CONNECTIONS, "SELECT * FROM pos_connections WHERE company_id=? AND state='ready'", 1),
        (SETTLEMENTS, "SELECT * FROM pos_settlements WHERE company_id=? ORDER BY created_at DESC LIMIT 100", 1),
    ];

    #[test]
    fn list_sql_is_byte_identical_with_expected_binds() {
        assert_eq!(KINDS.len(), 4);
        assert_eq!(EXPECTED.len(), 4);
        for (kind, expected, binds) in EXPECTED {
            assert!(supported(kind), "list-read kind must be allow listed: {kind}");
            let stmt = sql(kind).unwrap_or_else(|| panic!("missing SQL for {kind}"));
            assert_eq!(stmt, *expected, "list-read SQL drifted for {kind}");
            assert_eq!(stmt.matches('?').count(), *binds, "bind count drifted for {kind}");
        }
    }

    #[test]
    fn list_kinds_are_select_only_and_lock_free() {
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
                    "list-read kind must not mutate, lock or cross owners: {kind} contains {forbidden}",
                );
            }
        }
    }

    #[test]
    fn unknown_list_kind_fails_closed() {
        assert!(!supported("pos_list_unlisted"));
        assert!(sql("pos_list_unlisted").is_none());
    }
}
