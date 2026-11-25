//! Native POS core workspace read kinds (shifts/register/table family). These
//! closed, `SELECT`-only owner statements are the runtime authority for the
//! retained lock-free keyed reads in the POS adapters. The bridge sends a typed
//! kind and positional values; this module selects the SQL. There is no
//! TypeScript SQL, mirror or fallback for this family. The integration oracle
//! imports these statements so the runtime registry and the parity test cannot
//! drift apart. Row locks, advisory/`LOCK TABLE`, PMS projection fragments,
//! the day-report registry and the Stripe registry are deliberately not part
//! of this family and stay inline in their callers and modules.

pub const SHIFTS: &str = "pos_core_shifts";
pub const TABLES: &str = "pos_core_tables";
pub const CASH_MOVEMENTS: &str = "pos_core_cash_movements";
pub const REGISTER_BY_NAME: &str = "pos_core_register_by_name";
pub const OPEN_SHIFT_BY_REGISTER: &str = "pos_core_open_shift_by_register";
pub const TABLE_BY_NAME: &str = "pos_core_table_by_name";
pub const REGISTERS_BY_KIND: &str = "pos_core_registers_by_kind";
pub const RESTAURANT_REGISTER_PROPERTY: &str = "pos_core_restaurant_register_property";
pub const OPEN_SHIFT_BY_ID: &str = "pos_core_open_shift_by_id";
pub const STARTUP_REGISTERS: &str = "pos_core_startup_registers";

pub const KINDS: &[&str] = &[
    SHIFTS,
    TABLES,
    CASH_MOVEMENTS,
    REGISTER_BY_NAME,
    OPEN_SHIFT_BY_REGISTER,
    TABLE_BY_NAME,
    REGISTERS_BY_KIND,
    RESTAURANT_REGISTER_PROPERTY,
    OPEN_SHIFT_BY_ID,
    STARTUP_REGISTERS,
];

pub fn supported(kind: &str) -> bool {
    KINDS.contains(&kind)
}

/// Authoritative SQL for the core read kinds, byte-identical to the retired
/// TypeScript literals and the integration oracle. The first three statements
/// are caller-transaction workspace lists scoped by Company/branch values; the
/// remaining seven are register/shift/table reads scoped by the caller's
/// positional values, including the bounded startup register list. Every
/// statement is `SELECT`-only and lock-free.
pub fn sql(kind: &str) -> Option<&'static str> {
    Some(match kind {
        SHIFTS => r#"SELECT s.*,r.name register_name,
   s.opening_minor+COALESCE((SELECT SUM(p.amount_minor) FROM pos_payments p JOIN pos_sales b ON b.id=p.sale_id WHERE b.shift_id=s.id AND p.method='cash' AND p.state='succeeded'),0)
   -COALESCE((SELECT SUM(rp.amount_minor) FROM pos_refund_payments rp JOIN pos_refunds rf ON rf.id=rp.refund_id JOIN pos_payments p ON p.id=rp.payment_id WHERE rf.payout_shift_id=s.id AND p.method='cash' AND rp.state='succeeded'),0)
   +COALESCE((SELECT SUM(m.amount_minor) FROM pos_cash_movements m WHERE m.shift_id=s.id AND m.approved_by IS NOT NULL),0) cash_expected_minor
   FROM pos_shifts s JOIN pos_registers r ON r.id=s.register_id WHERE s.company_id=? AND (?::text IS NULL OR s.branch_id=?) ORDER BY s.opened_at DESC LIMIT 100"#,
        TABLES => "SELECT t.* FROM pos_tables t JOIN pos_registers r ON r.id=t.register_id WHERE r.company_id=? AND (?::text IS NULL OR r.branch_id=?) ORDER BY t.name",
        CASH_MOVEMENTS => "SELECT m.*,s.register_id FROM pos_cash_movements m JOIN pos_shifts s ON s.id=m.shift_id WHERE s.company_id=? AND (?::text IS NULL OR s.branch_id=?) ORDER BY m.created_at DESC LIMIT 100",
        REGISTER_BY_NAME => "SELECT id FROM pos_registers WHERE company_id=? AND branch_id=? AND name=?",
        OPEN_SHIFT_BY_REGISTER => "SELECT id FROM pos_shifts WHERE register_id=? AND state='open'",
        TABLE_BY_NAME => "SELECT id FROM pos_tables WHERE register_id=? AND name=?",
        REGISTERS_BY_KIND => "SELECT id FROM pos_registers WHERE company_id=? AND (?::text IS NULL OR branch_id=?) AND (?<>'recipe' OR mode='restaurant')",
        RESTAURANT_REGISTER_PROPERTY => "SELECT id,property_id FROM pos_registers WHERE id=? AND mode='restaurant'",
        OPEN_SHIFT_BY_ID => "SELECT id FROM pos_shifts WHERE id=? AND state='open'",
        STARTUP_REGISTERS => "SELECT id,branch_id,warehouse_id,name,mode,active,version FROM pos_registers WHERE company_id=? AND (?::text IS NULL OR branch_id=?) ORDER BY id",
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Expected strings are the exact TypeScript literals retired from the POS
    /// adapters; the transport parity suite executes the same registry
    /// statement against the caller's unit.
    const EXPECTED: &[(&str, &str, usize)] = &[
        (SHIFTS, r#"SELECT s.*,r.name register_name,
   s.opening_minor+COALESCE((SELECT SUM(p.amount_minor) FROM pos_payments p JOIN pos_sales b ON b.id=p.sale_id WHERE b.shift_id=s.id AND p.method='cash' AND p.state='succeeded'),0)
   -COALESCE((SELECT SUM(rp.amount_minor) FROM pos_refund_payments rp JOIN pos_refunds rf ON rf.id=rp.refund_id JOIN pos_payments p ON p.id=rp.payment_id WHERE rf.payout_shift_id=s.id AND p.method='cash' AND rp.state='succeeded'),0)
   +COALESCE((SELECT SUM(m.amount_minor) FROM pos_cash_movements m WHERE m.shift_id=s.id AND m.approved_by IS NOT NULL),0) cash_expected_minor
   FROM pos_shifts s JOIN pos_registers r ON r.id=s.register_id WHERE s.company_id=? AND (?::text IS NULL OR s.branch_id=?) ORDER BY s.opened_at DESC LIMIT 100"#, 3),
        (TABLES, "SELECT t.* FROM pos_tables t JOIN pos_registers r ON r.id=t.register_id WHERE r.company_id=? AND (?::text IS NULL OR r.branch_id=?) ORDER BY t.name", 3),
        (CASH_MOVEMENTS, "SELECT m.*,s.register_id FROM pos_cash_movements m JOIN pos_shifts s ON s.id=m.shift_id WHERE s.company_id=? AND (?::text IS NULL OR s.branch_id=?) ORDER BY m.created_at DESC LIMIT 100", 3),
        (REGISTER_BY_NAME, "SELECT id FROM pos_registers WHERE company_id=? AND branch_id=? AND name=?", 3),
        (OPEN_SHIFT_BY_REGISTER, "SELECT id FROM pos_shifts WHERE register_id=? AND state='open'", 1),
        (TABLE_BY_NAME, "SELECT id FROM pos_tables WHERE register_id=? AND name=?", 2),
        (REGISTERS_BY_KIND, "SELECT id FROM pos_registers WHERE company_id=? AND (?::text IS NULL OR branch_id=?) AND (?<>'recipe' OR mode='restaurant')", 4),
        (RESTAURANT_REGISTER_PROPERTY, "SELECT id,property_id FROM pos_registers WHERE id=? AND mode='restaurant'", 1),
        (OPEN_SHIFT_BY_ID, "SELECT id FROM pos_shifts WHERE id=? AND state='open'", 1),
        (STARTUP_REGISTERS, "SELECT id,branch_id,warehouse_id,name,mode,active,version FROM pos_registers WHERE company_id=? AND (?::text IS NULL OR branch_id=?) ORDER BY id", 3),
    ];

    #[test]
    fn core_sql_is_byte_identical_with_expected_binds() {
        assert_eq!(KINDS.len(), 10);
        assert_eq!(EXPECTED.len(), 10);
        for (kind, expected, binds) in EXPECTED {
            assert!(supported(kind), "core kind must be allow listed: {kind}");
            let stmt = sql(kind).unwrap_or_else(|| panic!("missing SQL for {kind}"));
            assert_eq!(stmt, *expected, "core SQL drifted for {kind}");
            assert_eq!(stmt.matches('?').count(), *binds, "bind count drifted for {kind}");
        }
    }

    #[test]
    fn core_kinds_are_select_only_and_lock_free() {
        for kind in KINDS {
            let stmt = sql(kind).unwrap_or_else(|| panic!("missing SQL for {kind}"));
            assert!(stmt.starts_with("SELECT "), "kind must be SELECT-only: {kind}");
            assert!(stmt.contains('?'), "kind must be parameterized: {kind}");
            for forbidden in [
                "INSERT", "UPDATE ", "DELETE", "RETURNING", ";", "FOR UPDATE", "FOR SHARE",
                "FOR NO KEY UPDATE", "LOCK TABLE", "pg_advisory", "pg_try_advisory",
                "pms_sources", "pms_properties", "commerce_",
            ] {
                assert!(
                    !stmt.contains(forbidden),
                    "core kind must not mutate, lock or cross owners: {kind} contains {forbidden}",
                );
            }
        }
    }

    #[test]
    fn unknown_core_kind_fails_closed() {
        assert!(!supported("pos_core_unlisted"));
        assert!(sql("pos_core_unlisted").is_none());
    }
}
