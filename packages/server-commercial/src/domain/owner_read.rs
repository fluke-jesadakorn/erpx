//! Pure domain registry for native Commercial owner reads.
//!
//! The three owner-owned usage projections (snapshot/meter/seat) that Billing
//! consumes are planned here as exact static SELECTs. The native route returns
//! the registered SELECT for a closed read kind; the TypeScript caller runs it
//! as a subquery on its own already-open unit and keeps its own filters and
//! `FOR UPDATE`. No SQL crosses the wire from a caller and no TS fallback exists.
use erpx_server_shared::public::BusinessError;

pub const OWNER_READ_OPERATION: &str = "commercial.owner-read.v1";

pub const USAGE_SNAPSHOT: &str = "usage.snapshot";
pub const USAGE_METER: &str = "usage.meter";
pub const USAGE_SEAT: &str = "usage.seat";

/// Exact retired `billing_usage_snapshots` projection literal.
pub const SQL_USAGE_SNAPSHOT: &str = r#"SELECT "holding_id","month","snapshot_id","refreshed_at","source_count","ai_coverage","storage_coverage","unreconciled_originals","seat_coverage","seat_source_count","active_seat_count","active_seat_observed_at" FROM billing_usage_snapshots"#;

/// Exact retired `billing_usage_meters` projection literal.
pub const SQL_USAGE_METER: &str = r#"SELECT "holding_id","month","source_key","company_id","company_name","branch_id","branch_name","meter_kind","funding_source","pricing_status","quantity","input_tokens","output_tokens","average_daily_bytes","active_at_period_end","snapshot_id","request_status","gateway_status","provider","model","units_json","cost_status","verified_cost_satang","cost_evidence_key","cost_reconciled_at","active_from","active_until","byte_milliseconds" FROM billing_usage_meters"#;

/// Exact retired `billing_seat_usage` projection literal.
pub const SQL_USAGE_SEAT: &str =
    r#"SELECT "holding_id","month","user_key","intervals_json","snapshot_id" FROM billing_seat_usage"#;

/// Closed owner read kind -> exact static SELECT. No caller binds or locks are
/// planned here: those stay inside the caller's transaction.
pub fn statement(kind: &str) -> Option<&'static str> {
    match kind {
        USAGE_SNAPSHOT => Some(SQL_USAGE_SNAPSHOT),
        USAGE_METER => Some(SQL_USAGE_METER),
        USAGE_SEAT => Some(SQL_USAGE_SEAT),
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
    fn closed_registry_of_three_usage_reads() {
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
        assert_eq!(statement("bogus"), None);
        assert!(is_owner_read(USAGE_METER));
        assert!(!is_owner_read("usage.meter-prune"));
    }

    #[test]
    fn reads_are_select_only_and_call_unit_lock_free() {
        for kind in [USAGE_SNAPSHOT, USAGE_METER, USAGE_SEAT] {
            let sql = statement(kind).expect("owner read kind");
            assert!(sql.trim_start().starts_with("SELECT"), "{kind}");
            assert!(!sql.contains("FOR UPDATE"), "{kind}: caller owns the lock");
            assert!(!sql.contains('$'), "{kind}: no caller binds in owner SQL");
            assert!(!sql.contains('?'), "{kind}: no caller binds in owner SQL");
        }
    }
}
