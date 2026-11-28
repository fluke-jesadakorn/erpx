//! Native POS audit actor read kinds mounted in the owner statement registry.
//! The two lock-free, SELECT-only owner reads in `pos-core.ts`
//! (`readPosAudit1` / `readPosAudit2`) send these typed kinds with the caller's
//! record id and operation; the authoritative SQL is selected here. Only
//! `pos_audit` is read — no row lock, advisory/`LOCK TABLE`, PMS cross-owner
//! fragment, day-report registry or Stripe registry statement is part of this
//! family. Those stay inline in their callers and dedicated modules.

pub const AUDIT_ACTOR_CHARGE_TO_ROOM: &str = "pos_audit_actor_charge_to_room";
pub const AUDIT_ACTOR_KITCHEN_START: &str = "pos_audit_actor_kitchen_start";

pub const KINDS: &[&str] = &[
    AUDIT_ACTOR_CHARGE_TO_ROOM,
    AUDIT_ACTOR_KITCHEN_START,
];

pub fn supported(kind: &str) -> bool {
    KINDS.contains(&kind)
}

/// Authoritative SQL for the two audit actor reads, byte-identical to the
/// retired TypeScript literals. Each is a caller-transaction `SELECT` on the
/// owner's `pos_audit` table, scoped by the caller's record id and a closed
/// operation value. No statement carries a row lock, an advisory lock, a
/// `LOCK TABLE` clause or a cross-owner projection fragment.
pub fn sql(kind: &str) -> Option<&'static str> {
    Some(match kind {
        AUDIT_ACTOR_CHARGE_TO_ROOM => "SELECT actor_id FROM pos_audit WHERE operation='sale.charge_to_room' AND record_id=? ORDER BY created_at DESC LIMIT 1",
        AUDIT_ACTOR_KITCHEN_START => "SELECT actor_id FROM pos_audit WHERE operation='kitchen.start' AND record_id=? ORDER BY created_at DESC LIMIT 1",
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Byte-for-byte SQL and placeholder count for each kind. Expected strings
    /// are the exact interpolated TypeScript literals retired from
    /// `pos-core.ts`; the transport parity suite executes the same registry
    /// statement against the caller's unit.
    const EXPECTED: &[(&str, &str, usize)] = &[
        (AUDIT_ACTOR_CHARGE_TO_ROOM, "SELECT actor_id FROM pos_audit WHERE operation='sale.charge_to_room' AND record_id=? ORDER BY created_at DESC LIMIT 1", 1),
        (AUDIT_ACTOR_KITCHEN_START, "SELECT actor_id FROM pos_audit WHERE operation='kitchen.start' AND record_id=? ORDER BY created_at DESC LIMIT 1", 1),
    ];

    #[test]
    fn audit_sql_is_byte_identical_with_expected_binds() {
        assert_eq!(KINDS.len(), 2);
        assert_eq!(EXPECTED.len(), 2);
        for (kind, expected, binds) in EXPECTED {
            assert!(supported(kind), "audit read kind must be allow listed: {kind}");
            let stmt = sql(kind).unwrap_or_else(|| panic!("missing SQL for {kind}"));
            assert_eq!(stmt, *expected, "audit SQL drifted for {kind}");
            assert_eq!(stmt.matches('?').count(), *binds, "bind count drifted for {kind}");
        }
    }

    #[test]
    fn audit_kinds_are_select_only_and_lock_free() {
        for kind in KINDS {
            let stmt = sql(kind).unwrap_or_else(|| panic!("missing SQL for {kind}"));
            assert!(stmt.starts_with("SELECT "), "kind must be SELECT-only: {kind}");
            for forbidden in [
                "INSERT", "UPDATE ", "DELETE", "RETURNING", ";", "FOR UPDATE", "FOR SHARE",
                "FOR NO KEY UPDATE", "LOCK TABLE", "pg_advisory", "pg_try_advisory",
                "pms_sources", "pms_properties", "commerce_",
            ] {
                assert!(
                    !stmt.contains(forbidden),
                    "audit read kind must not mutate, lock or cross owners: {kind} contains {forbidden}",
                );
            }
        }
    }

    #[test]
    fn unknown_audit_kind_fails_closed() {
        assert!(!supported("pos_audit_unlisted"));
        assert!(sql("pos_audit_unlisted").is_none());
    }
}
