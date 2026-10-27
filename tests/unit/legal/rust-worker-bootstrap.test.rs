use erpx_service::legal_worker_bootstrap::DevelopmentLegalWorkerConfiguration;
fn document() -> serde_json::Value {
    let mut doc = serde_json::json!({"mode":"development","commercialMode":"enforced","holdingId":"h001","companyId":"company","databaseUrl":"postgres://erpx_test_admin:private@127.0.0.1:5432/erpx_dev_rust_00000000000000000000000000000000","leaseToken":"00000000000000000000000000000000","filesRoot":"/owned/files","temporaryRoot":"/owned/temp","documentBinary":"/owned/document","tools":{"pdftotext":"/owned/pdftotext","pdfinfo":"/owned/pdfinfo","pdftoppm":"/owned/pdftoppm","tesseract":"/owned/tesseract","tessdata":"/owned/tessdata","languages":"eng+tha"}});
    doc["controlDatabaseUrl"]="postgres://erpx_test_admin:private@127.0.0.1:5432/erpx_dev_rust_11111111111111111111111111111111".into();
    doc["controlLeaseToken"] = "11111111111111111111111111111111".into();
    doc["accountHost"] = "account.example.test".into();
    doc["holdingBindings"] = serde_json::json!([{"holdingId":"h001","slug":"acme","host":"acme.example.test","writeBinding":"HOLDING_H001_DB","readBinding":"HOLDING_H001_READ_DB","filesBinding":"HOLDING_H001_FILES"}]);
    doc
}
#[test]
fn bootstrap_has_no_production_remote_or_implicit_enforcement_mode() {
    assert!(DevelopmentLegalWorkerConfiguration::parse(&document().to_string()).is_ok());
    for (field, value) in [
        ("mode", "production"),
        ("commercialMode", "disabled"),
        (
            "databaseUrl",
            "postgres://erpx_test_admin:private@example.invalid/erpx_test",
        ),
        ("documentBinary", "relative"),
    ] {
        let mut doc = document();
        doc[field] = value.into();
        assert!(
            DevelopmentLegalWorkerConfiguration::parse(&doc.to_string()).is_err(),
            "{field}"
        );
    }
    let mut unknown = document();
    unknown["allowIncomplete"] = true.into();
    assert!(DevelopmentLegalWorkerConfiguration::parse(&unknown.to_string()).is_err());
    assert!(DevelopmentLegalWorkerConfiguration::parse(&" ".repeat(32 * 1024 + 1)).is_err());
}
#[test]
fn configuration_load_is_bounded_private_and_does_not_follow_symlinks() {
    use std::fs;
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("worker.json");
    fs::write(&path, document().to_string()).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::{PermissionsExt, symlink};
        fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).unwrap();
        assert!(DevelopmentLegalWorkerConfiguration::load(&path).is_err());
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
        assert!(DevelopmentLegalWorkerConfiguration::load(&path).is_ok());
        let link = root.path().join("link.json");
        symlink(&path, &link).unwrap();
        assert!(DevelopmentLegalWorkerConfiguration::load(&link).is_err());
    }
    fs::write(&path, " ".repeat(32 * 1024 + 1)).unwrap();
    assert!(DevelopmentLegalWorkerConfiguration::load(&path).is_err());
    assert!(
        DevelopmentLegalWorkerConfiguration::load(std::path::Path::new("worker.json")).is_err()
    );
}

#[test]
fn compiled_cli_refuses_a_protected_database_without_probing_tools_or_echoing_credential() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("worker.json");
    let mut doc = document();
    doc["databaseUrl"] =
        "postgres://erpx_test_admin:private-sentinel@127.0.0.1:5432/erpx_production".into();
    std::fs::write(&path, doc.to_string()).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
    }
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_erpx-server"))
        .env_clear()
        .args(["legal-worker", "--once", "--development-config"])
        .arg(&path)
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    let diagnostic = String::from_utf8(output.stderr).unwrap();
    assert!(diagnostic.contains("EXACT_LEASED_DEVELOPMENT_DATABASE_REQUIRED"));
    assert!(!diagnostic.contains("private-sentinel"));
    assert!(!diagnostic.contains("erpx_production"));
}
