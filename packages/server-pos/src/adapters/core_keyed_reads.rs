//! Native POS keyed core reads (round 29). These closed, `SELECT`-only owner
//! statements replace the byte-identical inline TypeScript literals for
//! settings, receipt, prepared day close and room charge reversal lookups. The
//! bridge sends a typed kind and positional values; this module selects the
//! SQL. There is no TypeScript SQL, mirror or fallback for this family. Row
//! locks, advisory/`LOCK TABLE`, PMS projection fragments, the day-report
//! registry and the Stripe registry stay inline in their callers and modules.

pub const SETTINGS: &str = "pos_core_keyed_settings";
pub const SETTINGS_CURRENCY_ACCOUNTS: &str = "pos_core_keyed_settings_currency_accounts";
pub const SETTINGS_PREPARED_STATE: &str = "pos_core_keyed_settings_prepared_state";
pub const RECEIPT_BY_ID: &str = "pos_core_keyed_receipt_by_id";
pub const SALE_DOCUMENT: &str = "pos_core_keyed_sale_document";
pub const PREPARED_DAY_CLOSE: &str = "pos_core_keyed_prepared_day_close";
pub const PENDING_REVERSAL_REQUEST: &str = "pos_core_keyed_pending_reversal_request";
pub const REVERSAL_BY_SALE: &str = "pos_core_keyed_reversal_by_sale";
pub const COMMERCE_ATTACHMENT_IMAGE: &str = "pos_core_keyed_commerce_attachment_image";

pub const KINDS: &[&str] = &[
    SETTINGS,
    SETTINGS_CURRENCY_ACCOUNTS,
    SETTINGS_PREPARED_STATE,
    RECEIPT_BY_ID,
    SALE_DOCUMENT,
    PREPARED_DAY_CLOSE,
    PENDING_REVERSAL_REQUEST,
    REVERSAL_BY_SALE,
    COMMERCE_ATTACHMENT_IMAGE,
];

pub fn supported(kind: &str) -> bool {
    KINDS.contains(&kind)
}

/// Authoritative SQL for the keyed core read kinds, byte-identical to the
/// retired TypeScript literals and the transport/integration oracles. Every
/// statement is `SELECT`-only, parameterized and lock-free.
pub fn sql(kind: &str) -> Option<&'static str> {
    Some(match kind {
        SETTINGS => "SELECT * FROM pos_settings WHERE company_id=?",
        SETTINGS_CURRENCY_ACCOUNTS => "SELECT currency,accounts_json FROM pos_settings WHERE company_id=?",
        SETTINGS_PREPARED_STATE => "SELECT version,accounting_prepared_by,accounting_approved_by FROM pos_settings WHERE company_id=?",
        RECEIPT_BY_ID => "SELECT * FROM pos_receipts WHERE id=?",
        SALE_DOCUMENT => "SELECT snapshot_json::text FROM pos_sale_documents WHERE sale_id=? AND company_id=?",
        PREPARED_DAY_CLOSE => "SELECT id FROM pos_day_closes WHERE company_id=? AND branch_id=? AND business_date=? AND state='prepared'",
        PENDING_REVERSAL_REQUEST => "SELECT id FROM pos_room_charge_reversal_requests WHERE sale_id=? AND state IN ('requested','pos_approved')",
        REVERSAL_BY_SALE => "SELECT sale_id FROM pos_room_charge_reversals WHERE sale_id=?",
        COMMERCE_ATTACHMENT_IMAGE => "SELECT jsonb_build_object('catalog',(SELECT jsonb_agg(to_jsonb(c) ORDER BY c.product_id) FROM pos_catalog c WHERE c.product_id=? AND c.company_id=?),'registers',(SELECT jsonb_agg(jsonb_build_object('selection',to_jsonb(s),'register',to_jsonb(r)) ORDER BY s.register_id) FROM pos_register_catalog s JOIN pos_registers r ON r.id=s.register_id AND r.company_id=? WHERE s.product_id=?))::text image",
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
        (SETTINGS, "SELECT * FROM pos_settings WHERE company_id=?", 1),
        (SETTINGS_CURRENCY_ACCOUNTS, "SELECT currency,accounts_json FROM pos_settings WHERE company_id=?", 1),
        (SETTINGS_PREPARED_STATE, "SELECT version,accounting_prepared_by,accounting_approved_by FROM pos_settings WHERE company_id=?", 1),
        (RECEIPT_BY_ID, "SELECT * FROM pos_receipts WHERE id=?", 1),
        (SALE_DOCUMENT, "SELECT snapshot_json::text FROM pos_sale_documents WHERE sale_id=? AND company_id=?", 2),
        (PREPARED_DAY_CLOSE, "SELECT id FROM pos_day_closes WHERE company_id=? AND branch_id=? AND business_date=? AND state='prepared'", 3),
        (PENDING_REVERSAL_REQUEST, "SELECT id FROM pos_room_charge_reversal_requests WHERE sale_id=? AND state IN ('requested','pos_approved')", 1),
        (REVERSAL_BY_SALE, "SELECT sale_id FROM pos_room_charge_reversals WHERE sale_id=?", 1),
        (COMMERCE_ATTACHMENT_IMAGE, "SELECT jsonb_build_object('catalog',(SELECT jsonb_agg(to_jsonb(c) ORDER BY c.product_id) FROM pos_catalog c WHERE c.product_id=? AND c.company_id=?),'registers',(SELECT jsonb_agg(jsonb_build_object('selection',to_jsonb(s),'register',to_jsonb(r)) ORDER BY s.register_id) FROM pos_register_catalog s JOIN pos_registers r ON r.id=s.register_id AND r.company_id=? WHERE s.product_id=?))::text image", 4),
    ];

    #[test]
    fn keyed_core_sql_is_byte_identical_with_expected_binds() {
        assert_eq!(KINDS.len(), 9);
        assert_eq!(EXPECTED.len(), 9);
        for (kind, expected, binds) in EXPECTED {
            assert!(supported(kind), "keyed core kind must be allow listed: {kind}");
            let stmt = sql(kind).unwrap_or_else(|| panic!("missing SQL for {kind}"));
            assert_eq!(stmt, *expected, "keyed core SQL drifted for {kind}");
            assert_eq!(stmt.matches('?').count(), *binds, "bind count drifted for {kind}");
        }
    }

    #[test]
    fn keyed_core_kinds_are_select_only_and_lock_free() {
        for kind in KINDS {
            let stmt = sql(kind).unwrap_or_else(|| panic!("missing SQL for {kind}"));
            assert!(stmt.starts_with("SELECT "), "kind must be SELECT-only: {kind}");
            assert!(stmt.contains('?'), "kind must be parameterized: {kind}");
            for forbidden in [
                "INSERT", "UPDATE ", "DELETE", "RETURNING", ";", "FOR UPDATE", "FOR SHARE",
                "FOR NO KEY UPDATE", "LOCK TABLE", "pg_advisory", "pms_sources", "pms_properties",
                "commerce_",
            ] {
                assert!(
                    !stmt.contains(forbidden),
                    "keyed core kind must not mutate, lock or cross owners: {kind} contains {forbidden}",
                );
            }
        }
    }

    #[test]
    fn unknown_keyed_core_kind_fails_closed() {
        assert!(!supported("pos_core_keyed_unlisted"));
        assert!(sql("pos_core_keyed_unlisted").is_none());
    }
}
