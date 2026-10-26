use erpx_server_runtime::public::postgres::{
    DatabaseBinding, DatabaseConfig, DatabaseManifest, MigrationFile, MigrationTrack,
    load_migrations,
};
fn manifest() -> serde_json::Value {
    serde_json::json!({"version":1,"databases":{"CONTROL_DB":"postgres://user:private@localhost/control","CONTROL_READ_DB":"postgres://reader:private@localhost/control_read","CUSTOM_WRITE":"postgres://writer:private@localhost/holding","CUSTOM_READ":"postgres://reader:private@localhost/holding_read"},"slots":[{"id":"acme01","write":"CUSTOM_WRITE","read":"CUSTOM_READ","files":"CUSTOM_FILES"}]})
}
#[test]
fn manifest_binds_holding_slots_explicitly_and_never_derives_a_binding_from_the_id() {
    let manifest = DatabaseManifest::parse(&manifest().to_string()).unwrap();
    assert!(manifest.database(DatabaseBinding::ControlWrite).is_ok());
    assert!(
        manifest
            .database(DatabaseBinding::HoldingWrite("acme01".into()))
            .is_ok()
    );
    assert_eq!(
        manifest
            .database(DatabaseBinding::HoldingRead("ACME01".into()))
            .unwrap_err()
            .code,
        "RESOURCE_SLOT_UNKNOWN"
    );
    assert!(!format!("{manifest:?}").contains("private"));
    assert!(
        !format!(
            "{:?}",
            manifest.database(DatabaseBinding::ControlWrite).unwrap()
        )
        .contains("private")
    );
}
#[test]
fn invalid_manifest_and_duplicate_resource_bindings_fail_without_echoing_secrets() {
    for change in 0..4 {
        let mut value = manifest();
        match change {
            0 => value["version"] = 2.into(),
            1 => value["slots"][0]["write"] = "CONTROL_DB".into(),
            2 => value["slots"][0]["id"] = "../bad".into(),
            _ => value["databases"]["CONTROL_DB"] = "private_password invalid url".into(),
        };
        let error = DatabaseManifest::parse(&value.to_string()).unwrap_err();
        assert!(!format!("{error:?}").contains("private_password"));
    }
    assert!(DatabaseConfig::from_url("postgres://user:private@localhost/").is_err());
}
#[tokio::test]
async fn manifest_and_migration_paths_require_explicit_absolute_configuration() {
    assert_eq!(
        DatabaseManifest::load("relative.json")
            .await
            .unwrap_err()
            .code,
        "ABSOLUTE_DATABASE_MANIFEST_REQUIRED"
    );
    assert_eq!(
        load_migrations("migrations", MigrationTrack::Control)
            .await
            .unwrap_err()
            .code,
        "ABSOLUTE_MIGRATION_ROOT_REQUIRED"
    );
}
#[test]
fn migration_hashes_preserve_raw_newline_bytes_and_reject_path_names() {
    let first = MigrationFile::from_sql("0001_test.sql", "SELECT 1;").unwrap();
    let second = MigrationFile::from_sql("0001_test.sql", "SELECT 1;\n").unwrap();
    assert_ne!(first.checksum(), second.checksum());
    assert_eq!(first.checksum().len(), 64);
    for name in ["../test.sql", "a/b.sql", ".hidden.sql", "a'quote.sql"] {
        assert!(MigrationFile::from_sql(name, "SELECT 1").is_err());
    }
}

#[test]
fn validated_manifest_cannot_deserialize_around_its_constructor_checks() {
    // If the public type implements DeserializeOwned, these two implementations
    // make the inferred marker ambiguous and this regression stops compiling.
    trait ValidatedBoundary<Marker> {
        fn public_type_requires_validation() {}
    }
    impl<T: ?Sized> ValidatedBoundary<()> for T {}
    struct UnvalidatedDeserialize;
    impl<T: serde::de::DeserializeOwned> ValidatedBoundary<UnvalidatedDeserialize> for T {}
    let _ = <DatabaseManifest as ValidatedBoundary<_>>::public_type_requires_validation;

    let mut value = manifest();
    value["unvalidatedExtra"] = true.into();
    assert_eq!(
        DatabaseManifest::parse(&value.to_string())
            .unwrap_err()
            .code,
        "INVALID_DATABASE_MANIFEST"
    );
    value.as_object_mut().unwrap().remove("unvalidatedExtra");
    value["version"] = 2.into();
    assert_eq!(
        DatabaseManifest::parse(&value.to_string())
            .unwrap_err()
            .code,
        "INVALID_DATABASE_MANIFEST"
    );
}
