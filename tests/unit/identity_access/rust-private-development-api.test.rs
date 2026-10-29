use erpx_service::private_development_api::DevelopmentApiConfiguration;
fn document() -> serde_json::Value {
    let database = |token: &str| serde_json::json!({"url":format!("postgres://erpx_test_admin:synthetic@127.0.0.1:5432/erpx_dev_rust_{token}"),"leaseToken":token});
    serde_json::json!({"version":1,"mode":"development","transport":"direct-loopback","listen":"127.0.0.1:43117","accountHost":"account.example.test","requireEmailVerification":true,"passwordWorkers":1,"maximumRequests":8,"requestTimeoutMs":5000,"control":database("00000000000000000000000000000000"),"holdings":[{"holdingId":"h001","slug":"acme","host":"acme.example.test","writeBinding":"HOLDING_H001_DB","readBinding":"HOLDING_H001_READ_DB","filesBinding":"HOLDING_H001_FILES","database":database("11111111111111111111111111111111")},{"holdingId":"h002","slug":"northstar","host":"northstar.example.test","writeBinding":"HOLDING_H002_DB","readBinding":"HOLDING_H002_READ_DB","filesBinding":"HOLDING_H002_FILES","database":database("22222222222222222222222222222222")}]})
}
fn parses(value: &serde_json::Value) -> bool {
    DevelopmentApiConfiguration::parse(&value.to_string()).is_ok()
}
#[test]
fn configuration_is_explicit_private_development_and_never_selects_installed_or_reserved_resources()
{
    assert!(parses(&document()));
    for (field, value) in [
        ("mode", "production"),
        ("mode", "staging"),
        ("transport", "proxy"),
        ("listen", "0.0.0.0:43117"),
        ("listen", "127.0.0.1:0"),
        ("listen", "127.0.0.1:3000"),
        ("listen", "127.0.0.1:3001"),
        ("listen", "127.0.0.1:3100"),
        ("listen", "127.0.0.1:3101"),
        ("listen", "127.0.0.1:3103"),
        ("listen", "127.0.0.1:3204"),
        ("listen", "127.0.0.1:5432"),
        ("listen", "127.0.0.1:5433"),
        ("listen", "127.0.0.1:5435"),
        ("listen", "127.0.0.1:8080"),
    ] {
        let mut d = document();
        d[field] = value.into();
        assert!(!parses(&d), "{field}/{value}");
    }
    for url in [
        "postgres://erpx_test_admin:synthetic@127.0.0.1:5432/erpx_production",
        "postgres://erpx_test_admin:synthetic@127.0.0.1:5432/erpx_stage_control",
        "postgres://erpx_test_admin:synthetic@database.example.test:5432/erpx_dev_rust_00000000000000000000000000000000",
        "postgres://postgres:synthetic@127.0.0.1:5432/erpx_dev_rust_00000000000000000000000000000000",
        "postgres://erpx_test_admin:synthetic@127.0.0.1:5433/erpx_dev_rust_00000000000000000000000000000000",
    ] {
        let mut d = document();
        d["control"]["url"] = url.into();
        assert!(!parses(&d));
    }
}
#[test]
fn all_three_resources_and_both_registrations_are_distinct_and_bounded_with_no_unknown_escape_flags()
 {
    for field in ["control", "holdings", "accountHost", "requestTimeoutMs"] {
        let mut d = document();
        d.as_object_mut().unwrap().remove(field);
        assert!(!parses(&d));
    }
    let mut d = document();
    d["holdings"][1]["database"] = d["holdings"][0]["database"].clone();
    assert!(!parses(&d));
    let mut d = document();
    d["holdings"][0]["database"] = d["control"].clone();
    assert!(!parses(&d));
    for field in [
        "holdingId",
        "host",
        "slug",
        "writeBinding",
        "readBinding",
        "filesBinding",
    ] {
        let mut d = document();
        d["holdings"][1][field] = d["holdings"][0][field].clone();
        assert!(!parses(&d), "duplicate {field}");
    }
    let mut d = document();
    d["holdings"][1]["holdingId"] = "h003".into();
    assert!(!parses(&d));
    for field in [
        "allowIncomplete",
        "autoMigrate",
        "environment",
        "productionOverride",
    ] {
        let mut d = document();
        d[field] = false.into();
        assert!(!parses(&d));
    }
    let mut d = document();
    d["holdings"][0]["database"]["options"] = "protected".into();
    assert!(!parses(&d));
    let mut d = document();
    d["passwordWorkers"] = 0.into();
    assert!(!parses(&d));
    let mut d = document();
    d["maximumRequests"] = 65.into();
    assert!(!parses(&d));
    assert!(DevelopmentApiConfiguration::parse(&" ".repeat(32 * 1024 + 1)).is_err());
    let config = DevelopmentApiConfiguration::parse(&document().to_string()).unwrap();
    assert_eq!(
        format!("{config:?}"),
        "DevelopmentApiConfiguration([REDACTED])"
    );
}
#[test]
fn configuration_file_is_an_explicit_absolute_private_regular_file_without_leaf_symlink_following()
{
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("api.json");
    std::fs::write(&path, document().to_string()).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::{PermissionsExt, symlink};
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).unwrap();
        assert!(DevelopmentApiConfiguration::from_file(&path).is_err());
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
        assert!(DevelopmentApiConfiguration::from_file(&path).is_ok());
        let link = root.path().join("link.json");
        symlink(&path, &link).unwrap();
        assert!(DevelopmentApiConfiguration::from_file(&link).is_err());
    }
    assert!(DevelopmentApiConfiguration::from_file(std::path::Path::new("api.json")).is_err());
    std::fs::write(&path, b"\xff").unwrap();
    assert!(DevelopmentApiConfiguration::from_file(&path).is_err());
}

#[test]
fn finance_read_requires_distinct_registered_holdings_and_valid_companies() {
    let mut d = document();
    d["financeJournalReads"] = serde_json::json!([{ "holdingId": "h001", "companyId": "company" }]);
    assert!(parses(&d));
    d["accountBootstrap"] = true.into();
    assert!(parses(&d));
    d["financeFieldServiceInvoiceReads"] = serde_json::json!(["h001"]);
    assert!(parses(&d));
    d["financeFieldServiceInvoiceReads"] = serde_json::json!(["h002"]);
    assert!(!parses(&d));
    d["financeFieldServiceInvoiceReads"] = serde_json::json!(["h001", "h001"]);
    assert!(!parses(&d));
    d["financeFieldServiceInvoiceReads"] = serde_json::json!([]);
    d["financeJournalPosts"] = serde_json::json!(["h001"]);
    assert!(parses(&d));
    d["financeJournalPosts"] = serde_json::json!(["h002"]);
    assert!(!parses(&d));
    d["financeJournalPosts"] = serde_json::json!(["h001", "h001"]);
    assert!(!parses(&d));
    d["financeJournalPosts"] = serde_json::json!([]);
    d["financeJournalCreates"] = serde_json::json!(["h001"]);
    assert!(parses(&d));
    d["financeJournalCreates"] = serde_json::json!(["h002"]);
    assert!(!parses(&d));
    d["financeJournalCreates"] = serde_json::json!(["h001", "h001"]);
    assert!(!parses(&d));
    d["financeJournalCreates"] = serde_json::json!([]);
    d["financeJournalEdits"] = serde_json::json!(["h001"]);
    assert!(parses(&d));
    d["financeJournalEdits"] = serde_json::json!(["h002"]);
    assert!(!parses(&d));
    d["financeJournalEdits"] = serde_json::json!(["h001", "h001"]);
    assert!(!parses(&d));
    d["financeJournalEdits"] = serde_json::json!([]);
    d["financeJournalReversals"] = serde_json::json!(["h001"]);
    assert!(parses(&d));
    d["financeJournalReversals"] = serde_json::json!(["h002"]);
    assert!(!parses(&d));
    d["financeJournalReversals"] = serde_json::json!(["h001", "h001"]);
    assert!(!parses(&d));
    d["financeJournalReversals"] = serde_json::json!([]);
    d["fieldServiceReads"] = serde_json::json!([{ "holdingId": "h001", "companyId": "company" }]);
    d["fieldAttachmentLinks"] = serde_json::json!([{ "holdingId": "h001", "mode": "enabled" }]);
    assert!(parses(&d));
    d["fieldAttachmentLinks"] = serde_json::json!([{ "holdingId": "h001", "mode": "read-only" }]);
    assert!(parses(&d));
    d["fieldAttachmentLinks"] = serde_json::json!([{ "holdingId": "h001", "mode": "invalid" }]);
    assert!(!parses(&d));
    d["fieldAttachmentLinks"] = serde_json::json!([{ "holdingId": "h002", "mode": "enabled" }]);
    assert!(!parses(&d));
    d["fieldAttachmentLinks"] = serde_json::json!([{ "holdingId": "h001", "mode": "enabled" },{ "holdingId": "h001", "mode": "enabled" }]);
    assert!(!parses(&d));
    d["fieldAttachmentLinks"] = serde_json::json!([]);
    d["fieldAttachmentBinds"] = serde_json::json!([{ "holdingId": "h001", "mode": "enabled" }]);
    assert!(parses(&d));
    d["fieldAttachmentBinds"] = serde_json::json!([{ "holdingId": "h001", "mode": "read-only" }]);
    assert!(parses(&d));
    d["fieldAttachmentBinds"] = serde_json::json!([{ "holdingId": "h001", "mode": "invalid" }]);
    assert!(!parses(&d));
    d["fieldAttachmentBinds"] = serde_json::json!([{ "holdingId": "h002", "mode": "enabled" }]);
    assert!(!parses(&d));
    d["fieldAttachmentBinds"] = serde_json::json!([{ "holdingId": "h001", "mode": "enabled" },{ "holdingId": "h001", "mode": "enabled" }]);
    assert!(!parses(&d));
    d["fieldAttachmentBinds"] = serde_json::json!([]);
    for mode in ["disabled", "shadow", "enforced"] {
        d["fieldServiceSourceRecordReads"] =
            serde_json::json!([{ "holdingId":"h001", "mode":mode }]);
        assert!(parses(&d));
    }
    d["fieldServiceSourceRecordReads"] =
        serde_json::json!([{ "holdingId":"h001", "mode":"invalid" }]);
    assert!(!parses(&d));
    d["fieldServiceSourceRecordReads"] =
        serde_json::json!([{ "holdingId":"h002", "mode":"enforced" }]);
    assert!(!parses(&d));
    d["fieldServiceSourceRecordReads"] = serde_json::json!([{ "holdingId":"h001", "mode":"enforced" }, { "holdingId":"h001", "mode":"disabled" }]);
    assert!(!parses(&d));
    d["fieldServiceSourceRecordReads"] = serde_json::json!([]);
    d["fieldServiceWorkspaceReads"] = serde_json::json!(["h001"]);
    assert!(parses(&d));
    d["fieldServiceWorkspaceReads"] = serde_json::json!(["h002"]);
    assert!(!parses(&d));
    d["fieldServiceWorkspaceReads"] = serde_json::json!(["h001", "h001"]);
    assert!(!parses(&d));
    d["fieldServiceWorkspaceReads"] = serde_json::json!([]);
    d["fieldServicePartsPostingSetups"] = serde_json::json!(["h001"]);
    assert!(parses(&d));
    d["fieldServicePartsPostingSetups"] = serde_json::json!(["h002"]);
    assert!(!parses(&d));
    d["fieldServicePartsPostingSetups"] = serde_json::json!(["h001", "h001"]);
    assert!(!parses(&d));
    d["fieldServicePartsPostingSetups"] = serde_json::json!([]);
    d["fieldServicePartsPrerequisites"] = serde_json::json!(["h001"]);
    assert!(parses(&d));
    d["fieldServicePartsPrerequisites"] = serde_json::json!(["h002"]);
    assert!(!parses(&d));
    d["fieldServicePartsPrerequisites"] = serde_json::json!(["h001", "h001"]);
    assert!(!parses(&d));
    d["fieldServicePartsPrerequisites"] = serde_json::json!([]);
    d["fieldServiceStockSetups"] = serde_json::json!(["h001"]);
    assert!(parses(&d));
    d["fieldServiceStockSetups"] = serde_json::json!(["h002"]);
    assert!(!parses(&d));
    d["fieldServiceStockSetups"] = serde_json::json!(["h001", "h001"]);
    assert!(!parses(&d));
    d["fieldServiceStockSetups"] = serde_json::json!([]);
    d["fieldServiceWorkReads"] = serde_json::json!(["h001"]);
    assert!(parses(&d));
    d["fieldServiceWorkReads"] = serde_json::json!(["h002"]);
    assert!(!parses(&d));
    d["fieldServiceWorkReads"] = serde_json::json!(["h001", "h001"]);
    assert!(!parses(&d));
    d["fieldServiceWorkReads"] = serde_json::json!([]);
    d["fieldServiceDetailReads"] = serde_json::json!(["h001"]);
    assert!(parses(&d));
    d["fieldServiceDetailReads"] = serde_json::json!(["h002"]);
    assert!(!parses(&d));
    d["fieldServiceDetailReads"] = serde_json::json!(["h001", "h001"]);
    assert!(!parses(&d));
    d["fieldServiceDetailReads"] = serde_json::json!([]);
    d["fieldServiceRequests"] = serde_json::json!(["h001"]);
    assert!(parses(&d));
    d["fieldServiceRequests"] = serde_json::json!(["h001", "h001"]);
    assert!(!parses(&d));
    d["fieldServiceRequests"] = serde_json::json!(["h002"]);
    assert!(!parses(&d));
    d["fieldServiceRequests"] = serde_json::json!([]);
    d["fieldServiceOrderCreates"] = serde_json::json!(["h001"]);
    assert!(parses(&d));
    d["fieldServiceOrderCreates"] = serde_json::json!(["h001", "h001"]);
    assert!(!parses(&d));
    d["fieldServiceOrderCreates"] = serde_json::json!(["h002"]);
    assert!(!parses(&d));
    d["fieldServiceOrderCreates"] = serde_json::json!([]);
    d["fieldServiceVisits"] = serde_json::json!(["h001"]);
    assert!(DevelopmentApiConfiguration::parse(&d.to_string()).is_ok());
    d["fieldServiceVisits"] = serde_json::json!(["h001", "h001"]);
    assert!(DevelopmentApiConfiguration::parse(&d.to_string()).is_err());
    d["fieldServiceVisits"] = serde_json::json!(["h002"]);
    assert!(DevelopmentApiConfiguration::parse(&d.to_string()).is_err());
    d["fieldServiceVisits"] = serde_json::json!([]);
    d["fieldServiceVisitCompletions"] = serde_json::json!(["h001"]);
    assert!(DevelopmentApiConfiguration::parse(&d.to_string()).is_ok());
    d["fieldServiceVisitCompletions"] = serde_json::json!(["h001", "h001"]);
    assert!(DevelopmentApiConfiguration::parse(&d.to_string()).is_err());
    d["fieldServiceVisitCompletions"] = serde_json::json!(["h002"]);
    assert!(DevelopmentApiConfiguration::parse(&d.to_string()).is_err());
    d["fieldServiceVisitCompletions"] = serde_json::json!([]);
    d["fieldServiceVisitRecoveries"] = serde_json::json!(["h001"]);
    assert!(DevelopmentApiConfiguration::parse(&d.to_string()).is_ok());
    d["fieldServiceVisitRecoveries"] = serde_json::json!(["h001", "h001"]);
    assert!(DevelopmentApiConfiguration::parse(&d.to_string()).is_err());
    d["fieldServiceVisitRecoveries"] = serde_json::json!(["h002"]);
    assert!(DevelopmentApiConfiguration::parse(&d.to_string()).is_err());
    d["fieldServiceVisitRecoveries"] = serde_json::json!([]);
    d["fieldServiceOrderCancellations"] = serde_json::json!(["h001"]);
    assert!(DevelopmentApiConfiguration::parse(&d.to_string()).is_ok());
    d["fieldServiceOrderCancellations"] = serde_json::json!(["h001", "h001"]);
    assert!(DevelopmentApiConfiguration::parse(&d.to_string()).is_err());
    d["fieldServiceOrderCancellations"] = serde_json::json!(["h002"]);
    assert!(DevelopmentApiConfiguration::parse(&d.to_string()).is_err());
    d["fieldServiceOrderCancellations"] = serde_json::json!([]);
    d["fieldServiceAgreementReviews"] = serde_json::json!(["h001"]);
    assert!(DevelopmentApiConfiguration::parse(&d.to_string()).is_ok());
    d["fieldServiceAgreementReviews"] = serde_json::json!(["h001", "h001"]);
    assert!(DevelopmentApiConfiguration::parse(&d.to_string()).is_err());
    d["fieldServiceAgreementReviews"] = serde_json::json!(["h002"]);
    assert!(DevelopmentApiConfiguration::parse(&d.to_string()).is_err());
    d["fieldServiceAgreementReviews"] = serde_json::json!([]);
    d["fieldServiceAgreementCreates"] = serde_json::json!(["h001"]);
    assert!(DevelopmentApiConfiguration::parse(&d.to_string()).is_ok());
    d["fieldServiceAgreementCreates"] = serde_json::json!(["h001", "h001"]);
    assert!(DevelopmentApiConfiguration::parse(&d.to_string()).is_err());
    d["fieldServiceAgreementCreates"] = serde_json::json!(["h002"]);
    assert!(DevelopmentApiConfiguration::parse(&d.to_string()).is_err());
    d["fieldServiceAgreementCreates"] = serde_json::json!([]);
    d["fieldServiceReservations"] = serde_json::json!(["h001"]);
    assert!(parses(&d));
    d["fieldServiceReservations"] = serde_json::json!(["h002"]);
    assert!(!parses(&d));
    d["fieldServiceReservations"] = serde_json::json!(["h001", "h001"]);
    assert!(!parses(&d));
    d["fieldServiceReservations"] = serde_json::json!(["h001"]);
    d["fieldServiceIssues"] = serde_json::json!(["h001"]);
    assert!(parses(&d));
    d["fieldServiceIssues"] = serde_json::json!(["h001", "h001"]);
    assert!(!parses(&d));
    d["fieldServiceIssues"] = serde_json::json!(["h002"]);
    assert!(!parses(&d));
    d["fieldServiceIssues"] = serde_json::json!(["h001"]);
    d["fieldServiceReservations"] = serde_json::json!([]);
    assert!(!parses(&d));
    d["fieldServiceReservations"] = serde_json::json!(["h001"]);
    d["fieldServiceReads"] = serde_json::json!([{ "holdingId": "h001", "companyId": "different" }]);
    assert!(!parses(&d));
    d["fieldServiceReads"] = serde_json::json!([{ "holdingId": "h001", "companyId": "company" }]);
    d["financeJournalPosts"] = serde_json::json!(["h001"]);
    d["financeFieldServiceJournalPosts"] = serde_json::json!(["h001"]);
    assert!(parses(&d));
    d["financeFieldServiceJournalPosts"] = serde_json::json!(["h001", "h001"]);
    assert!(!parses(&d));
    d["financeFieldServiceJournalPosts"] = serde_json::json!(["h002"]);
    assert!(!parses(&d));
    d["financeFieldServiceJournalPosts"] = serde_json::json!(["h001"]);
    d["financeJournalPosts"] = serde_json::json!([]);
    assert!(!parses(&d));
    d["financeJournalPosts"] = serde_json::json!(["h001"]);
    d["fieldServiceIssues"] = serde_json::json!([]);
    assert!(!parses(&d));
    d["financeFieldServiceJournalPosts"] = serde_json::json!([]);
    let mut no_finance = document();
    no_finance["accountBootstrap"] = true.into();
    assert!(!parses(&no_finance));
    for entries in [
        serde_json::json!([{ "holdingId": "h003", "companyId": "company" }]),
        serde_json::json!([{ "holdingId": "h001", "companyId": "" }]),
        serde_json::json!([{ "holdingId": "h001", "companyId": "company" }, { "holdingId": "h001", "companyId": "other" }]),
        serde_json::json!([{ "holdingId": "h001", "companyId": "company", "host": "other" }]),
    ] {
        d["financeJournalReads"] = entries;
        assert!(!parses(&d));
    }
}

#[test]
fn manual_create_configuration_never_admits_production() {
    use erpx_server_commercial::public::purchases::current_package_catalogue;
    use erpx_server_shared::public::{CompanyId, HoldingId};
    use erpx_service::{
        finance_journal_create::FinanceJournalCreateConfiguration,
        finance_journal_review::FinanceJournalEnvironment,
    };
    assert!(
        FinanceJournalCreateConfiguration::new(
            FinanceJournalEnvironment::Production,
            HoldingId::new("h001").unwrap(),
            CompanyId::new("company").unwrap(),
            std::sync::Arc::new(current_package_catalogue().unwrap())
        )
        .is_err()
    );
}

#[test]
fn service_reservation_read_never_admits_production() {
    use erpx_server_commercial::public::purchases::current_package_catalogue;
    use erpx_server_shared::public::{CompanyId, HoldingId};
    use erpx_service::{
        field_service_parts::FieldServicePartsEnvironment,
        field_service_reservation_read::FieldServiceReservationReadConfiguration,
    };
    assert!(
        FieldServiceReservationReadConfiguration::new(
            FieldServicePartsEnvironment::Production,
            HoldingId::new("h001").unwrap(),
            CompanyId::new("company").unwrap(),
            std::sync::Arc::new(current_package_catalogue().unwrap())
        )
        .is_err()
    );
}

#[test]
fn initial_count_requires_registered_company_and_explicit_trusted_file_policy() {
    let target = serde_json::json!({"holdingId":"h001","filesEnabled":true,"filesReadOnly":false,"uploadsFrozen":false});
    let mut d = document();
    d["fieldServiceInitialCounts"] = serde_json::json!([target.clone()]);
    assert!(
        !parses(&d),
        "Count cannot introduce an unbound Field Company"
    );
    d["fieldServiceReads"] = serde_json::json!([{"holdingId":"h001","companyId":"company"}]);
    assert!(parses(&d));
    for field in ["filesEnabled", "filesReadOnly", "uploadsFrozen"] {
        let mut missing = d.clone();
        missing["fieldServiceInitialCounts"][0]
            .as_object_mut()
            .unwrap()
            .remove(field);
        assert!(!parses(&missing), "Explicit file policy required: {field}");
        let mut wrong = d.clone();
        wrong["fieldServiceInitialCounts"][0][field] = "true".into();
        assert!(!parses(&wrong));
    }
    d["fieldServiceInitialCounts"] = serde_json::json!([target.clone(), target.clone()]);
    assert!(!parses(&d));
    d["fieldServiceInitialCounts"] = serde_json::json!([target]);
    d["fieldServiceInitialCounts"][0]["holdingId"] = "h002".into();
    assert!(!parses(&d));
    d["fieldServiceInitialCounts"][0]["holdingId"] = "h001".into();
    d["fieldServiceInitialCounts"][0]["mode"] = "production".into();
    assert!(!parses(&d));
}

#[test]
fn contributor_profile_requires_three_exact_origins_and_canonical_holding_slugs() {
    let mut d = document();
    d["accountHost"] = "account.erpx.localhost:3443".into();
    d["holdings"][0]["host"] = "acme.erpx.localhost:3443".into();
    d["holdings"][1]["host"] = "northstar.erpx.localhost:3443".into();
    let mut third = d["holdings"][1].clone();
    third["holdingId"] = "h003".into();
    third["slug"] = "erpx".into();
    third["host"] = "erpx-internal.erpx.localhost:3443".into();
    third["writeBinding"] = "HOLDING_H003_DB".into();
    third["readBinding"] = "HOLDING_H003_READ_DB".into();
    third["filesBinding"] = "HOLDING_H003_FILES".into();
    third["database"] = serde_json::json!({"url":"postgres://erpx_test_admin:synthetic@127.0.0.1:5432/erpx_dev_rust_33333333333333333333333333333333","leaseToken":"33333333333333333333333333333333"});
    d["holdings"].as_array_mut().unwrap().push(third);
    assert!(parses(&d));
    for (field, value) in [("host", "acme.erpx.localhost:3444"), ("slug", "other")] {
        let mut wrong = d.clone();
        wrong["holdings"][0][field] = value.into();
        assert!(!parses(&wrong));
    }
    let mut partial = d.clone();
    partial["holdings"].as_array_mut().unwrap().pop();
    assert!(!parses(&partial));
    let mut public = d.clone();
    public["accountHost"] = "account.example.test".into();
    assert!(!parses(&public));
}
