//! Native final-root projection source port: closed SQL set and TS-authority parity.
use erpx_server_field_service::public::final_root_projection::FinalRootProjectionSource;

/// The retained TS authority. Customer 360 timeline now reads the agreement,
/// agreement-event, order-event and order sources through the final-root
/// witness in composition, so only the still-shared fragments are pinned.
const TS_AUTHORITY: &str = include_str!(
    "../../../packages/server-field_service/src/adapters/final-root-owner-projection-sources.ts"
);

#[test]
fn native_sql_matches_the_retained_ts_authority_statements() {
    for source in [
        FinalRootProjectionSource::Agreements,
        FinalRootProjectionSource::BillingReleases,
    ] {
        // TS source escapes embedded double quotes as \".
        let escaped = source.sql().replace('"', "\\\"");
        assert!(
            TS_AUTHORITY.contains(&escaped),
            "native SQL for {source:?} is absent from the TS authority: {}",
            source.sql()
        );
    }
    for source in [
        FinalRootProjectionSource::AgreementEvents,
        FinalRootProjectionSource::OrderEvents,
        FinalRootProjectionSource::Orders,
    ] {
        let escaped = source.sql().replace('"', "\\\"");
        assert!(
            !TS_AUTHORITY.contains(&escaped),
            "retired TS authority for {source:?} must not be restored"
        );
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
