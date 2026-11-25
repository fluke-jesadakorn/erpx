//! Native POS Stripe settlement/provider read kinds mounted in the owner
//! statement registry. The TypeScript Stripe bridge sends these typed kinds
//! with positional company/account/livemode + id values; the authoritative SQL
//! is selected here. These re-exports and the registry assertions let the
//! transport parity oracle assert byte equality with the runtime registry.
//!
//! Only the fourteen lock-free, SELECT-only owner reads in `pos-stripe.ts` and
//! `pos-core.ts` are part of this family. Row-lock (`FOR UPDATE`/`FOR SHARE`),
//! advisory/`LOCK TABLE`, PMS cross-owner reads and the day-report registry stay
//! inline in their TypeScript callers and dedicated modules.

pub const SETTLEMENT_BY_KEY: &str = "pos_stripe_settlement_by_key";
pub const SETTLEMENT_BALANCE_STATE: &str = "pos_stripe_settlement_balance_state";
pub const SETTLEMENT_PAYOUT_JOURNAL: &str = "pos_stripe_settlement_payout_journal";
pub const SETTLEMENT_BALANCE_ROW: &str = "pos_stripe_settlement_balance_row";
pub const PAYMENT_BY_ID: &str = "pos_stripe_payment_by_id";
pub const PAYMENT_ID: &str = "pos_stripe_payment_id";
pub const REFUND_BY_ID: &str = "pos_stripe_refund_by_id";
pub const REFUND_ID_BY_PROVIDER: &str = "pos_stripe_refund_id_by_provider";
pub const STRIPE_EVENT_BY_ID: &str = "pos_stripe_event_by_id";
pub const CONNECTIONS_READY: &str = "pos_stripe_connections_ready";
pub const SALE_NUMBER_BY_ID: &str = "pos_stripe_sale_number_by_id";
pub const REFUND_PAYMENT_BY_REFUND: &str = "pos_stripe_refund_payment_by_refund";
pub const SETTLEMENT_BY_JOURNAL: &str = "pos_stripe_settlement_by_journal";
pub const JOBS_CONTEXT: &str = "pos_stripe_jobs_context";

pub const KINDS: &[&str] = &[
    SETTLEMENT_BY_KEY,
    SETTLEMENT_BALANCE_STATE,
    SETTLEMENT_PAYOUT_JOURNAL,
    SETTLEMENT_BALANCE_ROW,
    PAYMENT_BY_ID,
    PAYMENT_ID,
    REFUND_BY_ID,
    REFUND_ID_BY_PROVIDER,
    STRIPE_EVENT_BY_ID,
    CONNECTIONS_READY,
    SALE_NUMBER_BY_ID,
    REFUND_PAYMENT_BY_REFUND,
    SETTLEMENT_BY_JOURNAL,
    JOBS_CONTEXT,
];

pub fn supported(kind: &str) -> bool {
    KINDS.contains(&kind)
}

/// Authoritative SQL for the fourteen Stripe settlement/provider reads,
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
        STRIPE_EVENT_BY_ID => "SELECT * FROM pos_stripe_events WHERE id=?",
        CONNECTIONS_READY => "SELECT company_id FROM pos_connections WHERE company_id=? AND account_id=? AND livemode=? AND state='ready'",
        SALE_NUMBER_BY_ID => "SELECT number FROM pos_sales WHERE id=?",
        REFUND_PAYMENT_BY_REFUND => "SELECT rp.* FROM pos_refund_payments rp JOIN pos_refunds r ON r.id=rp.refund_id WHERE rp.id=? AND r.company_id=?",
        SETTLEMENT_BY_JOURNAL => "SELECT id FROM pos_settlements WHERE journal_id=?",
        // The retired context probe inlined 'pos_jobs' in the catalog lookup.
        // The owner plan binds the same closed table name so every registry
        // statement stays parameterized; the projected column and result are
        // unchanged.
        JOBS_CONTEXT => "SELECT to_regclass(?) name",
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Byte-for-byte SQL and placeholder count for each kind. Expected strings
    /// are the exact interpolated TypeScript literals retired from
    /// `pos-stripe.ts` / `pos-core.ts`, except `JOBS_CONTEXT`: the retired
    /// catalog probe inlined `'pos_jobs'` and the owner plan binds that same
    /// closed table name so every registry statement is parameterized. The
    /// transport parity suite executes the same registry statement.
    const EXPECTED: &[(&str, &str, usize)] = &[
        (SETTLEMENT_BY_KEY, "SELECT state FROM pos_settlements WHERE company_id=? AND account_id=? AND livemode=? AND kind='balance' AND provider_id=?", 4),
        (SETTLEMENT_BALANCE_STATE, "SELECT state FROM pos_settlements WHERE account_id=? AND livemode=? AND kind='balance' AND provider_id=?", 3),
        (SETTLEMENT_PAYOUT_JOURNAL, "SELECT journal_id FROM pos_settlements WHERE company_id=? AND account_id=? AND livemode=? AND kind='payout' AND provider_id=? AND journal_id IS NOT NULL", 4),
        (SETTLEMENT_BALANCE_ROW, "SELECT * FROM pos_settlements WHERE account_id=? AND livemode=? AND kind='balance' AND provider_id=?", 3),
        (PAYMENT_BY_ID, "SELECT * FROM pos_payments WHERE id=? AND company_id=? AND stripe_account=? AND livemode=?", 4),
        (PAYMENT_ID, "SELECT id FROM pos_payments WHERE id=? AND company_id=? AND stripe_account=? AND livemode=?", 4),
        (REFUND_BY_ID, "SELECT rp.* FROM pos_refund_payments rp JOIN pos_payments p ON p.id=rp.payment_id WHERE rp.id=? AND p.company_id=? AND p.stripe_account=? AND p.livemode=?", 4),
        (REFUND_ID_BY_PROVIDER, "SELECT rp.id FROM pos_refund_payments rp JOIN pos_payments p ON p.id=rp.payment_id WHERE rp.provider_id=? AND p.company_id=? AND p.stripe_account=? AND p.livemode=?", 4),
        (STRIPE_EVENT_BY_ID, "SELECT * FROM pos_stripe_events WHERE id=?", 1),
        (CONNECTIONS_READY, "SELECT company_id FROM pos_connections WHERE company_id=? AND account_id=? AND livemode=? AND state='ready'", 3),
        (SALE_NUMBER_BY_ID, "SELECT number FROM pos_sales WHERE id=?", 1),
        (REFUND_PAYMENT_BY_REFUND, "SELECT rp.* FROM pos_refund_payments rp JOIN pos_refunds r ON r.id=rp.refund_id WHERE rp.id=? AND r.company_id=?", 2),
        (SETTLEMENT_BY_JOURNAL, "SELECT id FROM pos_settlements WHERE journal_id=?", 1),
        (JOBS_CONTEXT, "SELECT to_regclass(?) name", 1),
    ];

    #[test]
    fn stripe_sql_is_byte_identical_with_expected_binds() {
        assert_eq!(KINDS.len(), 14);
        assert_eq!(EXPECTED.len(), 14);
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
