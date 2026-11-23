//! Native POS core workspace read kinds (shifts family). These closed,
//! `SELECT`-only owner statements are the runtime authority for the retained
//! `pos.ts` shifts/tables/cash-movement reads. The bridge sends a typed kind
//! and positional values; this module selects the SQL. There is no TypeScript
//! SQL, mirror or fallback for this family. The integration oracle imports
//! these statements so the runtime registry and the parity test cannot drift
//! apart.

pub const SHIFTS: &str = "pos_core_shifts";
pub const TABLES: &str = "pos_core_tables";
pub const CASH_MOVEMENTS: &str = "pos_core_cash_movements";

pub const KINDS: &[&str] = &[SHIFTS, TABLES, CASH_MOVEMENTS];

pub fn supported(kind: &str) -> bool {
    KINDS.contains(&kind)
}

/// Authoritative SQL for the three core read kinds, byte-identical to the
/// retired TypeScript literals and the integration oracle. Every statement is
/// a caller-transaction `SELECT` scoped by the caller-supplied Company and
/// branch values.
pub fn sql(kind: &str) -> Option<&'static str> {
    Some(match kind {
        SHIFTS => r#"SELECT s.*,r.name register_name,
   s.opening_minor+COALESCE((SELECT SUM(p.amount_minor) FROM pos_payments p JOIN pos_sales b ON b.id=p.sale_id WHERE b.shift_id=s.id AND p.method='cash' AND p.state='succeeded'),0)
   -COALESCE((SELECT SUM(rp.amount_minor) FROM pos_refund_payments rp JOIN pos_refunds rf ON rf.id=rp.refund_id JOIN pos_payments p ON p.id=rp.payment_id WHERE rf.payout_shift_id=s.id AND p.method='cash' AND rp.state='succeeded'),0)
   +COALESCE((SELECT SUM(m.amount_minor) FROM pos_cash_movements m WHERE m.shift_id=s.id AND m.approved_by IS NOT NULL),0) cash_expected_minor
   FROM pos_shifts s JOIN pos_registers r ON r.id=s.register_id WHERE s.company_id=? AND (?::text IS NULL OR s.branch_id=?) ORDER BY s.opened_at DESC LIMIT 100"#,
        TABLES => "SELECT t.* FROM pos_tables t JOIN pos_registers r ON r.id=t.register_id WHERE r.company_id=? AND (?::text IS NULL OR r.branch_id=?) ORDER BY t.name",
        CASH_MOVEMENTS => "SELECT m.*,s.register_id FROM pos_cash_movements m JOIN pos_shifts s ON s.id=m.shift_id WHERE s.company_id=? AND (?::text IS NULL OR s.branch_id=?) ORDER BY m.created_at DESC LIMIT 100",
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn core_kinds_are_select_only_and_parameterized() {
        assert_eq!(KINDS.len(), 3);
        for kind in KINDS {
            assert!(supported(kind), "kind must be allow listed: {kind}");
            let stmt = sql(kind).unwrap_or_else(|| panic!("missing SQL for {kind}"));
            assert!(stmt.starts_with("SELECT "), "kind must be SELECT-only: {kind}");
            assert!(stmt.contains('?'), "kind must be parameterized: {kind}");
            assert_eq!(stmt.matches('?').count(), 3, "kind must carry Company/branch scope: {kind}");
            for forbidden in ["INSERT", "UPDATE ", "DELETE", "RETURNING", ";"] {
                assert!(!stmt.contains(forbidden), "kind must not mutate: {kind} contains {forbidden}");
            }
        }
    }

    #[test]
    fn unknown_core_kind_fails_closed() {
        assert!(!supported("pos_core_unlisted"));
        assert!(sql("pos_core_unlisted").is_none());
    }
}
