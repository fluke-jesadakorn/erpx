//! Native POS purchase-boundary read kinds. These closed, `SELECT`-only owner
//! statements are the runtime authority for the TypeScript purchase boundary
//! (`purchase-boundary.ts`). The bridge sends a typed kind and positional
//! values; this module selects the SQL. There is no TypeScript SQL, mirror or
//! fallback for this family. The integration oracle imports these statements so
//! the runtime registry and the parity test cannot drift apart.

pub const SALE_CREATE_MODE: &str = "pos_boundary_sale_create_mode";
pub const SHIFT_OPEN_MODE: &str = "pos_boundary_shift_open_mode";
pub const SALE_ADD_MODE: &str = "pos_boundary_sale_add_mode";
pub const SALE_MODE: &str = "pos_boundary_sale_mode";
pub const CASH_APPROVE_MODE: &str = "pos_boundary_cash_approve_mode";
pub const SHIFT_MODE: &str = "pos_boundary_shift_mode";
pub const PAYMENT_MODE: &str = "pos_boundary_payment_mode";
pub const REFUND_MODE: &str = "pos_boundary_refund_mode";
pub const RECEIPT_REPLAY: &str = "pos_boundary_receipt_replay";

pub const KINDS: &[&str] = &[
    SALE_CREATE_MODE,
    SHIFT_OPEN_MODE,
    SALE_ADD_MODE,
    SALE_MODE,
    CASH_APPROVE_MODE,
    SHIFT_MODE,
    PAYMENT_MODE,
    REFUND_MODE,
    RECEIPT_REPLAY,
];

pub fn supported(kind: &str) -> bool {
    KINDS.contains(&kind)
}

/// Authoritative SQL for the nine boundary kinds, byte-identical to the
/// retired TypeScript literals and the integration oracle. Every statement is
/// a caller-transaction `SELECT` scoped by the caller-supplied Company value.
pub fn sql(kind: &str) -> Option<&'static str> {
    Some(match kind {
        SALE_CREATE_MODE => "SELECT r.mode FROM pos_shifts s JOIN pos_registers r ON r.id=s.register_id WHERE s.id=? AND s.company_id=?",
        SHIFT_OPEN_MODE => "SELECT mode FROM pos_registers WHERE id=? AND company_id=?",
        SALE_ADD_MODE => "SELECT CASE WHEN r.mode='restaurant' OR c.kind='recipe' THEN 'restaurant' ELSE r.mode END mode FROM pos_sales s JOIN pos_registers r ON r.id=s.register_id LEFT JOIN pos_catalog c ON c.product_id=? AND c.company_id=s.company_id WHERE s.id=? AND s.company_id=?",
        SALE_MODE => "SELECT CASE WHEN r.mode='restaurant' OR EXISTS(SELECT 1 FROM pos_lines l JOIN pos_fulfillments f ON f.id=l.fulfillment_id WHERE l.sale_id=s.id AND f.kind='recipe') THEN 'restaurant' ELSE r.mode END mode FROM pos_sales s JOIN pos_registers r ON r.id=s.register_id WHERE s.id=? AND s.company_id=?",
        CASH_APPROVE_MODE => "SELECT r.mode FROM pos_cash_movements m JOIN pos_shifts s ON s.id=m.shift_id JOIN pos_registers r ON r.id=s.register_id WHERE m.id=? AND s.company_id=?",
        SHIFT_MODE => "SELECT r.mode FROM pos_shifts s JOIN pos_registers r ON r.id=s.register_id WHERE s.id=? AND s.company_id=?",
        PAYMENT_MODE => "SELECT r.mode FROM pos_payments p JOIN pos_sales s ON s.id=p.sale_id JOIN pos_registers r ON r.id=s.register_id WHERE p.id=? AND s.company_id=?",
        REFUND_MODE => "SELECT r.mode FROM pos_refunds f JOIN pos_sales s ON s.id=f.sale_id JOIN pos_registers r ON r.id=s.register_id WHERE f.id=? AND s.company_id=?",
        RECEIPT_REPLAY => "SELECT 1 FROM pos_receipts WHERE id=? AND company_id=?",
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn boundary_kinds_are_select_only_and_parameterized() {
        assert_eq!(KINDS.len(), 9);
        for kind in KINDS {
            assert!(supported(kind), "kind must be allow listed: {kind}");
            let stmt = sql(kind).unwrap_or_else(|| panic!("missing SQL for {kind}"));
            assert!(stmt.starts_with("SELECT"), "kind must be SELECT-only: {kind}");
            assert!(stmt.contains('?'), "kind must be parameterized: {kind}");
            for forbidden in ["INSERT", "DELETE", "RETURNING", ";"] {
                assert!(!stmt.contains(forbidden), "kind must not mutate: {kind} contains {forbidden}");
            }
            assert!(!stmt.contains("UPDATE "), "kind must not mutate: {kind}");
        }
    }

    #[test]
    fn unknown_boundary_kind_fails_closed() {
        assert!(!supported("pos_boundary_unlisted"));
        assert!(sql("pos_boundary_unlisted").is_none());
    }
}
