//! Pure domain registry for native Commercial owner reads.
//!
//! The owner-owned usage projections (snapshot/meter/seat) that Billing
//! consumes, the principal-status event base read that the usage projection
//! composes, and the Company purchase-facts snapshot that purchase-command
//! decisions consume are planned here as exact static SELECTs. The native route returns
//! the registered SELECT for a closed read kind; the TypeScript caller runs it
//! as a subquery on its own already-open unit and keeps its own filters and
//! `FOR UPDATE`. No SQL crosses the wire from a caller and no TS fallback exists.
use erpx_server_shared::public::BusinessError;

pub const OWNER_READ_OPERATION: &str = "commercial.owner-read.v1";

pub const USAGE_SNAPSHOT: &str = "usage.snapshot";
pub const USAGE_METER: &str = "usage.meter";
pub const USAGE_SEAT: &str = "usage.seat";
pub const PRINCIPAL_EVENTS: &str = "principal.events";
/// Company purchase facts consumed by the purchase-command read family.
pub const PURCHASE_FACTS: &str = "purchase.facts";

/// Exact retired `billing_usage_snapshots` projection literal.
pub const SQL_USAGE_SNAPSHOT: &str = r#"SELECT "holding_id","month","snapshot_id","refreshed_at","source_count","ai_coverage","storage_coverage","unreconciled_originals","seat_coverage","seat_source_count","active_seat_count","active_seat_observed_at" FROM billing_usage_snapshots"#;

/// Exact retired `billing_usage_meters` projection literal.
pub const SQL_USAGE_METER: &str = r#"SELECT "holding_id","month","source_key","company_id","company_name","branch_id","branch_name","meter_kind","funding_source","pricing_status","quantity","input_tokens","output_tokens","average_daily_bytes","active_at_period_end","snapshot_id","request_status","gateway_status","provider","model","units_json","cost_status","verified_cost_satang","cost_evidence_key","cost_reconciled_at","active_from","active_until","byte_milliseconds" FROM billing_usage_meters"#;

/// Exact retired `billing_seat_usage` projection literal.
pub const SQL_USAGE_SEAT: &str =
    r#"SELECT "holding_id","month","user_key","intervals_json","snapshot_id" FROM billing_seat_usage"#;

/// Exact retired `billing_principal_status_events` base projection. The caller
/// appends its own `effective_at` bound, variable-arity user filter and order.
pub const SQL_PRINCIPAL_EVENTS: &str =
    "SELECT id,user_id,status,effective_at FROM billing_principal_status_events";

/// Exact retired `readCompanyPurchaseFacts` snapshot SELECT. The owner plans
/// both ordered Company binds; the caller executes this on its own transaction
/// and maps the private grant/history rows without any local SQL.
pub const SQL_PURCHASE_FACTS: &str = concat!(
    "SELECT COALESCE((SELECT jsonb_agg(to_jsonb(g) ORDER BY package_id) FROM company_package_grants g WHERE company_id=?),'[]'::jsonb) grants, ",
    "COALESCE((SELECT jsonb_agg(to_jsonb(h) ORDER BY package_id,package_version) FROM company_package_history_rights h WHERE company_id=?),'[]'::jsonb) history"
);

/// Closed owner read kind -> exact static SELECT. No caller binds or locks are
/// planned here: those stay inside the caller's transaction.
pub fn statement(kind: &str) -> Option<&'static str> {
    match kind {
        USAGE_SNAPSHOT => Some(SQL_USAGE_SNAPSHOT),
        USAGE_METER => Some(SQL_USAGE_METER),
        USAGE_SEAT => Some(SQL_USAGE_SEAT),
        PRINCIPAL_EVENTS => Some(SQL_PRINCIPAL_EVENTS),
        PURCHASE_FACTS => Some(SQL_PURCHASE_FACTS),
        _ => None,
    }
}

pub fn is_owner_read(kind: &str) -> bool {
    statement(kind).is_some()
}

pub fn invalid() -> BusinessError {
    BusinessError::invalid(
        "COMMERCIAL_OWNER_READ_INVALID",
        "Review the Commercial owner read.",
    )
}

pub fn unavailable() -> BusinessError {
    BusinessError::new(
        erpx_server_shared::public::FailureKind::Unavailable,
        "COMMERCIAL_OWNER_READ_UNAVAILABLE",
        "The native Commercial service is unavailable.",
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn closed_registry_of_owner_reads() {
        assert_eq!(
            statement(USAGE_SNAPSHOT),
            Some(r#"SELECT "holding_id","month","snapshot_id","refreshed_at","source_count","ai_coverage","storage_coverage","unreconciled_originals","seat_coverage","seat_source_count","active_seat_count","active_seat_observed_at" FROM billing_usage_snapshots"#)
        );
        assert_eq!(
            statement(USAGE_METER),
            Some(r#"SELECT "holding_id","month","source_key","company_id","company_name","branch_id","branch_name","meter_kind","funding_source","pricing_status","quantity","input_tokens","output_tokens","average_daily_bytes","active_at_period_end","snapshot_id","request_status","gateway_status","provider","model","units_json","cost_status","verified_cost_satang","cost_evidence_key","cost_reconciled_at","active_from","active_until","byte_milliseconds" FROM billing_usage_meters"#)
        );
        assert_eq!(
            statement(USAGE_SEAT),
            Some(r#"SELECT "holding_id","month","user_key","intervals_json","snapshot_id" FROM billing_seat_usage"#)
        );
        assert_eq!(
            statement(PRINCIPAL_EVENTS),
            Some("SELECT id,user_id,status,effective_at FROM billing_principal_status_events")
        );
        assert_eq!(
            statement(PURCHASE_FACTS),
            Some(concat!(
                "SELECT COALESCE((SELECT jsonb_agg(to_jsonb(g) ORDER BY package_id) FROM company_package_grants g WHERE company_id=?),'[]'::jsonb) grants, ",
                "COALESCE((SELECT jsonb_agg(to_jsonb(h) ORDER BY package_id,package_version) FROM company_package_history_rights h WHERE company_id=?),'[]'::jsonb) history"
            ))
        );
        assert_eq!(statement("bogus"), None);
        assert!(is_owner_read(USAGE_METER));
        assert!(is_owner_read(PRINCIPAL_EVENTS));
        assert!(is_owner_read(PURCHASE_FACTS));
        assert!(!is_owner_read("usage.meter-prune"));
    }

    #[test]
    fn reads_are_select_only_and_call_unit_lock_free() {
        for kind in [USAGE_SNAPSHOT, USAGE_METER, USAGE_SEAT, PRINCIPAL_EVENTS] {
            let sql = statement(kind).expect("owner read kind");
            assert!(sql.trim_start().starts_with("SELECT"), "{kind}");
            assert!(!sql.contains("FOR UPDATE"), "{kind}: caller owns the lock");
            assert!(!sql.contains('$'), "{kind}: no caller binds in owner SQL");
            assert!(!sql.contains('?'), "{kind}: no caller binds in owner SQL");
        }
        // The purchase-facts read is the sole bound statement: its two Company
        // binds stay in the caller's snapshot, never in a request-built SQL text.
        let facts = statement(PURCHASE_FACTS).expect("owner read kind");
        assert!(facts.starts_with("SELECT "));
        assert_eq!(facts.matches('?').count(), 2);
        assert!(!facts.contains("FOR UPDATE"));
        assert!(!facts.contains("FOR SHARE"));
        assert!(!facts.contains('$'));
    }
}
