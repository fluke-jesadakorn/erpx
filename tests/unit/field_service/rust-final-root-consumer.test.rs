//! Native consumer of the Field final-root projection sources: the Field-owned
//! `scoped_service` agreement composition embeds the native fixed statement.
//! The retired TS authority is gone; the native-owned agreement statement is
//! pinned here as the independent test oracle.
use erpx_field_service_server::public::final_root_consumer::FinalRootConsumerScope;
use erpx_field_service_server::public::final_root_projection::FinalRootProjectionSource;
use erpx_platform_shared_server::public::CompanyId;

const NATIVE_AGREEMENTS_SQL: &str = r#"SELECT "id","company_id","branch_id","department_id","customer_id","asset_id","title","start_date","end_date","currency","visit_rate_minor","status","created_by","reviewed_by","reviewed_at","version","created_at","updated_at","warranty_discount_bps" FROM field_service_agreements"#;

/// The `scoped_service` composition shape retained from the former Customer 360
/// consumer. Pinned so the native port cannot drift from the reviewed binding
/// shape without an explicit, reviewed update.
const CONSUMER_SCOPED_SERVICE_PREFIX: &str =
    "SELECT g.id,g.customer_id,g.branch_id,g.department_id FROM (";
const CONSUMER_SCOPED_SERVICE_SUFFIX: &str =
    ") g WHERE g.company_id=? AND (?::text IS NULL OR g.branch_id=?) AND (?::text IS NULL OR g.department_id=?)";

fn scope() -> FinalRootConsumerScope {
    FinalRootConsumerScope::new(
        CompanyId::new("company").unwrap(),
        Some("branch-1".into()),
        Some("dept-1".into()),
    )
    .unwrap()
}

#[test]
fn scoped_service_embeds_the_native_fixed_agreement_source() {
    let sql = scope().scoped_service_sql();
    let source = FinalRootProjectionSource::Agreements.sql();
    assert!(sql.starts_with(CONSUMER_SCOPED_SERVICE_PREFIX));
    assert!(sql.ends_with(CONSUMER_SCOPED_SERVICE_SUFFIX));
    assert!(sql.contains(source), "native fixed source not embedded");
    // The native statement is the sole authority; pin it against drift here.
    assert_eq!(source, NATIVE_AGREEMENTS_SQL, "native agreement source drift");
}

#[test]
fn scope_binding_order_matches_the_ts_consumer() {
    let s = scope();
    assert_eq!(
        s.scoped_service_args(),
        [
            Some("company"),
            Some("branch-1"),
            Some("branch-1"),
            Some("dept-1"),
            Some("dept-1"),
        ]
    );
    // NULL branch/department widen the scope without changing the statement.
    let wide = FinalRootConsumerScope::new(CompanyId::new("company").unwrap(), None, None).unwrap();
    assert_eq!(
        wide.scoped_service_args(),
        [Some("company"), None, None, None, None]
    );
    assert_eq!(wide.scoped_service_sql(), s.scoped_service_sql());
}

#[test]
fn malformed_scope_is_rejected_before_sql_composition() {
    let company = || CompanyId::new("company").unwrap();
    assert!(FinalRootConsumerScope::new(company(), Some("".into()), None).is_none());
    assert!(FinalRootConsumerScope::new(company(), None, Some("x\n1".into())).is_none());
    assert!(FinalRootConsumerScope::new(company(), None, Some("x".repeat(201))).is_none());
}
