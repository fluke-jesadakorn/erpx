//! Native final-root projection source port: closed SQL set with a native-only oracle.
//! The TS authority was retired with the last cross-owner imports; the native
//! enum is the sole owner of these fixed statements and the literals below are
//! the independent test oracle that detects any accidental drift.
use erpx_field_service_server::public::final_root_projection::FinalRootProjectionSource;

const NATIVE_SQL: [(FinalRootProjectionSource, &str); 5] = [
    (
        FinalRootProjectionSource::AgreementEvents,
        r#"SELECT "id","agreement_id","version","action","actor_id","reason","status","created_at" FROM field_service_agreement_events"#,
    ),
    (
        FinalRootProjectionSource::Agreements,
        r#"SELECT "id","company_id","branch_id","department_id","customer_id","asset_id","title","start_date","end_date","currency","visit_rate_minor","status","created_by","reviewed_by","reviewed_at","version","created_at","updated_at","warranty_discount_bps" FROM field_service_agreements"#,
    ),
    (
        FinalRootProjectionSource::BillingReleases,
        r#"SELECT "id","order_id","agreement_id","customer_id","currency","amount_minor","source_order_version","status","released_by","reason","created_at","warranty_applied","warranty_discount_bps","warranty_end_snapshot","pricing_date" FROM field_service_billing_releases"#,
    ),
    (
        FinalRootProjectionSource::OrderEvents,
        r#"SELECT "id","order_id","version","action","actor_id","reason","status","technician_id","scheduled_at","details_json","created_at" FROM field_service_order_events"#,
    ),
    (
        FinalRootProjectionSource::Orders,
        r#"SELECT "id","company_id","branch_id","department_id","ticket_id","ticket_version","agreement_id","agreement_version","asset_id","issue","site","technician_id","scheduled_at","status","exception_reason","visit_started_at","visit_completed_at","labor_minutes","completion_evidence","customer_confirmation","version","created_by","created_at","updated_at" FROM field_service_orders"#,
    ),
];

#[test]
fn native_sql_matches_the_native_owned_oracle_statements() {
    for (source, expected) in NATIVE_SQL {
        assert_eq!(source.sql(), expected, "native SQL drift for {source:?}");
    }
}

#[test]
fn the_source_set_is_closed_and_each_fragment_is_distinct_and_qualified() {
    assert_eq!(FinalRootProjectionSource::ALL.len(), 5);
    let mut seen = std::collections::BTreeSet::new();
    for source in FinalRootProjectionSource::ALL {
        let sql = source.sql();
        assert!(seen.insert(sql), "duplicate projection SQL for {source:?}");
        assert!(sql.starts_with("SELECT "), "non-select source {source:?}");
        assert!(
            sql.contains(" FROM ") && !sql.contains(';'),
            "compound or unterminated source {source:?}"
        );
        assert!(
            source.relation().starts_with("public.field_service_"),
            "unqualified fence relation for {source:?}"
        );
        assert!(
            sql.contains(source.relation().trim_start_matches("public.")),
            "fence relation does not match fragment for {source:?}"
        );
    }
}
