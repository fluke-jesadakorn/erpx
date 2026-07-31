#[path = "../src/object_storage.rs"]
#[allow(dead_code)]
mod object_storage;

use chrono::{TimeZone, Utc};
use object_storage::{MinioObjectStore, MinioObjectStoreConfig, ObjectStorageError};

fn store(endpoint: &str) -> MinioObjectStore {
    MinioObjectStore::new(MinioObjectStoreConfig {
        endpoint: endpoint.into(),
        access_key: "access-key".into(),
        secret_key: "secret-key".into(),
        region: "us-east-1".into(),
    })
    .expect("store configuration")
}

#[test]
fn operational_urls_are_tls_only_and_namespace_bound() {
    let at = Utc
        .with_ymd_and_hms(2026, 7, 27, 12, 0, 0)
        .single()
        .expect("fixed timestamp");
    let secure = store("https://storage.example.test:9000");
    let signed = secure
        .presign_download("tenant-abc", "backup/object.dump", 300, at)
        .expect("presigned URL");
    assert!(
        signed
            .url
            .starts_with("https://storage.example.test:9000/tenant-abc/")
    );
    assert!(signed.url.contains("X-Amz-Signature="));
    assert!(
        secure
            .presign_download("Tenant-abc", "backup/object.dump", 300, at)
            .is_err()
    );

    assert!(matches!(
        MinioObjectStore::new(MinioObjectStoreConfig {
            endpoint: "http://storage.example.test:9000".into(),
            access_key: "access-key".into(),
            secret_key: "secret-key".into(),
            region: "us-east-1".into(),
        }),
        Err(ObjectStorageError::InvalidConfiguration(message))
            if message == "object storage requires an HTTPS endpoint"
    ));
}

#[test]
fn http_object_storage_configuration_fails_closed_before_http() {
    let error = MinioObjectStore::new(MinioObjectStoreConfig {
        endpoint: "http://storage.example.test:9000".into(),
        access_key: "access-key".into(),
        secret_key: "secret-key".into(),
        region: "us-east-1".into(),
    })
    .expect_err("HTTP object storage configuration must fail closed");
    assert!(matches!(
        error,
        ObjectStorageError::InvalidConfiguration(message)
            if message == "object storage requires an HTTPS endpoint"
    ));
}

#[test]
fn endpoint_userinfo_and_error_text_are_redacted() {
    let result = MinioObjectStore::new(MinioObjectStoreConfig {
        endpoint: "https://storage-user:storage-secret@storage.example.test".into(),
        access_key: "access-key".into(),
        secret_key: "secret-key".into(),
        region: "us-east-1".into(),
    });
    let error = result.expect_err("endpoint userinfo must be rejected");
    let rendered = error.to_string();
    assert!(!rendered.contains("storage-user"));
    assert!(!rendered.contains("storage-secret"));
    assert!(!rendered.contains("access-key"));
    assert!(!rendered.contains("secret-key"));
}
