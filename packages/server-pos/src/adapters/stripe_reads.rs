//! Native POS Stripe settlement/provider read kinds mounted in the owner
//! statement registry. The TypeScript Stripe bridge sends these typed kinds
//! with positional company/account/livemode + id values; the authoritative SQL
//! is selected here. These re-exports and the registry assertions let the
//! transport parity oracle assert byte equality with the runtime registry.
//!
//! Only the eight lock-free, SELECT-only owner reads in `pos-stripe.ts` are
//! part of this family. Row-lock (`FOR UPDATE`/`FOR SHARE`), advisory/`LOCK
//! TABLE`, PMS cross-owner reads and the day-report registry stay inline in
//! their TypeScript callers and dedicated modules.

pub const SETTLEMENT_BY_KEY: &str = "pos_stripe_settlement_by_key";
pub const SETTLEMENT_BALANCE_STATE: &str = "pos_stripe_settlement_balance_state";
pub const SETTLEMENT_PAYOUT_JOURNAL: &str = "pos_stripe_settlement_payout_journal";
pub const SETTLEMENT_BALANCE_ROW: &str = "pos_stripe_settlement_balance_row";
pub const PAYMENT_BY_ID: &str = "pos_stripe_payment_by_id";
pub const PAYMENT_ID: &str = "pos_stripe_payment_id";
pub const REFUND_BY_ID: &str = "pos_stripe_refund_by_id";
pub const REFUND_ID_BY_PROVIDER: &str = "pos_stripe_refund_id_by_provider";

pub const KINDS: &[&str] = &[
    SETTLEMENT_BY_KEY,
    SETTLEMENT_BALANCE_STATE,
    SETTLEMENT_PAYOUT_JOURNAL,
    SETTLEMENT_BALANCE_ROW,
    PAYMENT_BY_ID,
    PAYMENT_ID,
    REFUND_BY_ID,
    REFUND_ID_BY_PROVIDER,
];

pub fn supported(kind: &str) -> bool {
    KINDS.contains(&kind)
}

/// Authoritative SQL for the eight Stripe settlement/provider reads,
/// byte-identical to the retired TypeScript literals. Every statement is a
/// caller-transaction `SELECT` scoped by caller-supplied company/account/
/// livemode and id values. None carries a row lock, advisory lock or `LOCK
/// TABLE` clause, and none interpolates a cross-owner projection fragment.
pub fn sql(kind: &str) -> Option<&'static str> {
    Some(match kind {
        SETTLEMENT_BY_KEY => "SELECT state FROM pos_settlements WHERE company_id=? AND account_id=? AND livemode=? AND kind='balance' AND provider_id=?",
        SETTLEMENT_BALANCE_STATE => "SELECT state FROM pos_settlements WHERE account_id=? AND livemode=? AND kind='balance' AND provider_id=?",
        SETTLEMENT_PAYOUT_JOURNAL => "SELECT journal_id FROM pos_settlements WHERE company_id=? AND account_id=? AND livemode=? AND kind='payout' AND provider_id=? AND journal_id IS NOT NULL",
        SETTLEMENT_BALANCE_ROW => "SELECT * FROM pos_settlements WHERE account_id=? AND livemode=? AND kind='balance' AND provider_id=?",
        PAYMENT_BY_ID => "SELECT * FROM pos_payments WHERE id=? AND company_id=? AND stripe_account=? AND livemode=?",
        PAYMENT_ID => "SELECT id FROM pos_payments WHERE id=? AND company_id=? AND stripe_account=? AND livemode=?",
        REFUND_BY_ID => "SELECT rp.* FROM pos_refund_payments rp JOIN pos_payments p ON p.id=rp.payment_id WHERE rp.id=? AND p.company_id=? AND p.stripe_account=? AND p.livemode=?",
        REFUND_ID_BY_PROVIDER => "SELECT rp.id FROM pos_refund_payments rp JOIN pos_payments p ON p.id=rp.payment_id WHERE rp.provider_id=? AND p.company_id=? AND p.stripe_account=? AND p.livemode=?",
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Byte-for-byte SQL and placeholder count for each kind. Expected strings
    /// are the exact interpolated TypeScript literals retired from
    /// `pos-stripe.ts`; the transport parity suite executes the same registry
    /// statement against the caller's unit.
    const EXPECTED: &[(&str, &str, usize)] = &[
        (SETTLEMENT_BY_KEY, "SELECT state FROM pos_settlements WHERE company_id=? AND account_id=? AND livemode=? AND kind='balance' AND provider_id=?", 4),
        (SETTLEMENT_BALANCE_STATE, "SELECT state FROM pos_settlements WHERE account_id=? AND livemode=? AND kind='balance' AND provider_id=?", 3),
        (SETTLEMENT_PAYOUT_JOURNAL, "SELECT journal_id FROM pos_settlements WHERE company_id=? AND account_id=? AND livemode=? AND kind='payout' AND provider_id=? AND journal_id IS NOT NULL", 4),
        (SETTLEMENT_BALANCE_ROW, "SELECT * FROM pos_settlements WHERE account_id=? AND livemode=? AND kind='balance' AND provider_id=?", 3),
        (PAYMENT_BY_ID, "SELECT * FROM pos_payments WHERE id=? AND company_id=? AND stripe_account=? AND livemode=?", 4),
        (PAYMENT_ID, "SELECT id FROM pos_payments WHERE id=? AND company_id=? AND stripe_account=? AND livemode=?", 4),
        (REFUND_BY_ID, "SELECT rp.* FROM pos_refund_payments rp JOIN pos_payments p ON p.id=rp.payment_id WHERE rp.id=? AND p.company_id=? AND p.stripe_account=? AND p.livemode=?", 4),
        (REFUND_ID_BY_PROVIDER, "SELECT rp.id FROM pos_refund_payments rp JOIN pos_payments p ON p.id=rp.payment_id WHERE rp.provider_id=? AND p.company_id=? AND p.stripe_account=? AND p.livemode=?", 4),
    ];

    #[test]
    fn stripe_sql_is_byte_identical_with_expected_binds() {
        assert_eq!(KINDS.len(), 8);
        assert_eq!(EXPECTED.len(), 8);
        for (kind, expected, binds) in EXPECTED {
            assert!(supported(kind), "stripe read kind must be allow listed: {kind}");
            let stmt = sql(kind).unwrap_or_else(|| panic!("missing SQL for {kind}"));
            assert_eq!(stmt, *expected, "stripe SQL drifted for {kind}");
            assert_eq!(stmt.matches('?').count(), *binds, "bind count drifted for {kind}");
        }
    }

    #[test]
    fn stripe_kinds_are_select_only_and_lock_free() {
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
                    "stripe read kind must not mutate, lock or cross owners: {kind} contains {forbidden}",
                );
            }
        }
    }

    #[test]
    fn unknown_stripe_kind_fails_closed() {
        assert!(!supported("pos_stripe_unlisted"));
        assert!(sql("pos_stripe_unlisted").is_none());
    }
}
