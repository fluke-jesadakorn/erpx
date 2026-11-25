use erpx_server_commercial::public::owner_read::{
    OWNER_READ_OPERATION, PRINCIPAL_EVENTS, SQL_PRINCIPAL_EVENTS, SQL_USAGE_METER, SQL_USAGE_SEAT,
    SQL_USAGE_SNAPSHOT, USAGE_METER, USAGE_SEAT, USAGE_SNAPSHOT, is_owner_read, statement,
};

#[test]
fn registry_returns_the_retired_owner_selects() {
    assert_eq!(OWNER_READ_OPERATION, "commercial.owner-read.v1");
    assert_eq!(statement(USAGE_SNAPSHOT), Some(SQL_USAGE_SNAPSHOT));
    assert_eq!(statement(USAGE_METER), Some(SQL_USAGE_METER));
    assert_eq!(statement(USAGE_SEAT), Some(SQL_USAGE_SEAT));
    assert_eq!(statement(PRINCIPAL_EVENTS), Some(SQL_PRINCIPAL_EVENTS));

    assert_eq!(
        SQL_USAGE_SNAPSHOT,
        r#"SELECT "holding_id","month","snapshot_id","refreshed_at","source_count","ai_coverage","storage_coverage","unreconciled_originals","seat_coverage","seat_source_count","active_seat_count","active_seat_observed_at" FROM billing_usage_snapshots"#
    );
    assert_eq!(
        SQL_USAGE_METER,
        r#"SELECT "holding_id","month","source_key","company_id","company_name","branch_id","branch_name","meter_kind","funding_source","pricing_status","quantity","input_tokens","output_tokens","average_daily_bytes","active_at_period_end","snapshot_id","request_status","gateway_status","provider","model","units_json","cost_status","verified_cost_satang","cost_evidence_key","cost_reconciled_at","active_from","active_until","byte_milliseconds" FROM billing_usage_meters"#
    );
    assert_eq!(
        SQL_USAGE_SEAT,
        r#"SELECT "holding_id","month","user_key","intervals_json","snapshot_id" FROM billing_seat_usage"#
    );
    assert_eq!(
        SQL_PRINCIPAL_EVENTS,
        "SELECT id,user_id,status,effective_at FROM billing_principal_status_events"
    );
}

#[test]
fn closed_kind_set_rejects_writes_and_unknown_kinds() {
    for kind in [USAGE_SNAPSHOT, USAGE_METER, USAGE_SEAT, PRINCIPAL_EVENTS] {
        assert!(is_owner_read(kind));
    }
    for kind in [
        "bogus",
        "usage.snapshot-ensure",
        "usage.snapshot-finish",
        "usage.meter-prune",
        "usage.seat-prune",
    ] {
        assert!(!is_owner_read(kind));
        assert_eq!(statement(kind), None);
    }
}

#[test]
fn owner_selects_keep_filters_and_locks_with_the_caller() {
    for kind in [USAGE_SNAPSHOT, USAGE_METER, USAGE_SEAT, PRINCIPAL_EVENTS] {
        let sql = statement(kind).expect("registered read kind");
        assert!(sql.trim_start().starts_with("SELECT"), "{kind}");
        assert!(!sql.contains("FOR UPDATE"), "{kind}: caller owns the lock");
        assert!(!sql.contains('$'), "{kind}: caller owns the binds");
        assert!(!sql.contains('?'), "{kind}: caller owns the binds");
    }
}
