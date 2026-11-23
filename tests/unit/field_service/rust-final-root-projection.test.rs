//! Native final-root projection source port: closed SQL set and TS-authority parity.
use erpx_server_field_service::public::final_root_projection::FinalRootProjectionSource;

/// The TS authority that still serves Customer 360 timeline. Its statements are
/// the oracle until the consumer imports are cut over by lead integration.
const TS_AUTHORITY: &str = include_str!(
    "../../../packages/server-field_service/src/adapters/final-root-owner-projection-sources.ts"
);

#[test]
fn native_sql_matches_the_ts_authority_statement_for_statement() {
    for source in FinalRootProjectionSource::ALL {
        // TS source escapes embedded double quotes as \".
        let escaped = source.sql().replace('"', "\\\"");
        assert!(
            TS_AUTHORITY.contains(&escaped),
            "native SQL for {source:?} is absent from the TS authority: {}",
            source.sql()
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
