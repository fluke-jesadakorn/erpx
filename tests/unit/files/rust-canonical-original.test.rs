use erpx_server_files::public::canonical_original::*;
use erpx_server_shared::public::{CompanyId, HoldingId, RecordId, UserId};
use sha2::{Digest, Sha256};
use std::{fs, os::unix::fs::symlink};

#[test]
fn source_witness_cannot_be_reconstructed_from_untrusted_json() {
    trait WireBoundary<Marker> {
        fn requires_current_owner_read() {}
    }
    impl<T: ?Sized> WireBoundary<()> for T {}
    impl<T: serde::de::DeserializeOwned> WireBoundary<u8> for T {}
    let _ = <OriginalReadWitness as WireBoundary<_>>::requires_current_owner_read;
}

fn actor() -> OriginalReadActor {
    OriginalReadActor {
        holding_id: HoldingId::new("holding").unwrap(),
        company_id: CompanyId::new("company").unwrap(),
        user_id: UserId::new("user").unwrap(),
        selection_revision: "selection-1".into(),
    }
}
fn query() -> LegalOriginalQuery {
    LegalOriginalQuery {
        holding_id: actor().holding_id,
        company_id: actor().company_id,
        item_id: RecordId::new("item").unwrap(),
        document_id: RecordId::new("document").unwrap(),
        link_id: RecordId::new("link").unwrap(),
        expected_checksum_sha256: hex::encode(Sha256::digest(b"%PDF-synthetic")),
        expected_media_type: LegalOriginalMediaType::Pdf,
        max_bytes: ORIGINAL_LIMIT,
    }
}
fn facts() -> CanonicalOriginalFacts {
    CanonicalOriginalFacts {
        company_id: actor().company_id,
        link_id: RecordId::new("link").unwrap(),
        source_id: RecordId::new("source").unwrap(),
        set_id: RecordId::new("set").unwrap(),
        target_type: "legal_item".into(),
        target_id: RecordId::new("item").unwrap(),
        purpose_key: "legal_document".into(),
        component_kind: "record".into(),
        component_ref: Some("command".into()),
        policy_version: 1,
        classification: "restricted".into(),
        audience_json: "{}".into(),
        retention_policy_key: Some("legal-managed".into()),
        set_state: "bound".into(),
        link_state: "bound".into(),
        binding_type: Some("legal_document".into()),
        binding_id: Some("document".into()),
        source_state: "available".into(),
        source_company_id: Some("company".into()),
        source_owner_type: "legal_item".into(),
        source_owner_id: "item".into(),
        source_purpose: "legal_document".into(),
        object_key: "legal/item/source.pdf".into(),
        media_type: "application/pdf".into(),
        detected_media_type: Some("application/pdf".into()),
        bytes: 14,
        checksum_sha256: query().expected_checksum_sha256,
        revoked_at: None,
        destroyed_at: None,
        scanned_at: Some(10),
        scanner_definition_at: Some(9),
    }
}

#[test]
fn requests_require_exact_scope_selection_checksum_and_bounded_size() {
    assert!(query().validate(&actor()).is_ok());
    let mut q = query();
    q.company_id = CompanyId::new("other").unwrap();
    assert!(q.validate(&actor()).is_err());
    let mut q = query();
    q.holding_id = HoldingId::new("other").unwrap();
    assert!(q.validate(&actor()).is_err());
    for max in [0, ORIGINAL_LIMIT + 1, usize::MAX] {
        let mut q = query();
        q.max_bytes = max;
        assert!(q.validate(&actor()).is_err());
    }
    for hash in [
        "".to_owned(),
        "A".repeat(64),
        "0".repeat(63),
        "g".repeat(64),
    ] {
        let mut q = query();
        q.expected_checksum_sha256 = hash;
        assert!(q.validate(&actor()).is_err());
    }
    let mut a = actor();
    a.selection_revision.clear();
    assert!(query().validate(&a).is_err());
}

#[test]
fn no_unscanned_quarantined_destroyed_or_unbound_original_can_pass() {
    assert!(facts().validate(&query()).is_ok());
    for state in [
        "legacy_unscanned",
        "quarantined",
        "scanning",
        "scan_error",
        "malware_rejected",
        "destroyed",
        "",
    ] {
        let mut f = facts();
        f.source_state = state.into();
        assert!(f.validate(&query()).is_err());
    }
    for state in ["draft", "withdrawn", "expired"] {
        let mut f = facts();
        f.link_state = state.into();
        assert!(f.validate(&query()).is_err());
        let mut f = facts();
        f.set_state = state.into();
        assert!(f.validate(&query()).is_err());
    }
    let mut f = facts();
    f.revoked_at = Some(1);
    assert!(f.validate(&query()).is_err());
    let mut f = facts();
    f.destroyed_at = Some(1);
    assert!(f.validate(&query()).is_err());
    let mut f = facts();
    f.scanned_at = None;
    assert!(f.validate(&query()).is_err());
    let mut f = facts();
    f.scanner_definition_at = None;
    assert!(f.validate(&query()).is_err());
}

#[test]
fn original_link_binding_policy_and_integrity_are_exact() {
    let mutations: Vec<fn(&mut CanonicalOriginalFacts)> = vec![
        |f| f.source_company_id = Some("foreign".into()),
        |f| f.target_type = "project".into(),
        |f| f.target_id = RecordId::new("other").unwrap(),
        |f| f.purpose_key = "legal_portal_submission".into(),
        |f| f.binding_type = None,
        |f| f.binding_id = Some("other".into()),
        |f| f.classification = "public".into(),
        |f| f.retention_policy_key = None,
        |f| f.policy_version = 2,
        |f| f.component_kind = "action".into(),
        |f| f.bytes = -1,
        |f| f.bytes = ORIGINAL_LIMIT as i64 + 1,
        |f| f.checksum_sha256 = "0".repeat(64),
        |f| f.media_type = "text/plain".into(),
        |f| f.detected_media_type = None,
    ];
    for mutate in mutations {
        let mut f = facts();
        mutate(&mut f);
        assert!(f.validate(&query()).is_err());
    }
}

#[test]
fn each_legal_original_media_requires_its_exact_scanned_source_mime() {
    for media in [
        LegalOriginalMediaType::Pdf,
        LegalOriginalMediaType::Png,
        LegalOriginalMediaType::Jpeg,
        LegalOriginalMediaType::Docx,
    ] {
        let mut q = query();
        q.expected_media_type = media;
        let mut f = facts();
        f.media_type = media.as_mime().into();
        f.detected_media_type = Some(media.as_mime().into());
        assert!(f.validate(&q).is_ok());
        f.detected_media_type = Some("application/octet-stream".into());
        assert!(f.validate(&q).is_err());
        assert_eq!(
            LegalOriginalMediaType::try_from(media.as_mime()).unwrap(),
            media
        );
    }
    for unsupported in ["image/svg+xml", "text/csv", "text/plain", ""] {
        assert!(LegalOriginalMediaType::try_from(unsupported).is_err());
    }
}

fn disk(root: &std::path::Path, body: &[u8]) -> (String, String, std::path::PathBuf, String) {
    let key = "legal/item/source.pdf".to_owned();
    let hash = hex::encode(Sha256::digest(body));
    let version = uuid::Uuid::new_v4().to_string();
    let dir = root.join(hex::encode(Sha256::digest(key.as_bytes())));
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join(&version), body).unwrap();
    fs::write(dir.join("current.json"),serde_json::to_vec(&serde_json::json!({"key":key,"version":version,"size":body.len(),"etag":hash,"httpEtag":format!("\"{hash}\""),"uploaded":"2026-01-01T00:00:00.000Z","httpMetadata":{"contentType":"application/pdf"},"customMetadata":{}})).unwrap()).unwrap();
    (key, hash, dir, version)
}

#[tokio::test]
async fn native_store_reads_existing_hashed_directory_and_manifest_format() {
    let root = tempfile::tempdir().unwrap();
    let bytes = b"%PDF-synthetic";
    let (key, hash, _, _) = disk(root.path(), bytes);
    let store = LocalOriginalStore::open(root.path(), 2).unwrap();
    assert_eq!(
        store
            .read_verified(&key, bytes.len(), &hash, ORIGINAL_LIMIT)
            .await
            .unwrap(),
        bytes
    );
    assert!(
        store
            .read_verified(&key, bytes.len(), &"0".repeat(64), ORIGINAL_LIMIT)
            .await
            .is_err()
    );
    assert!(
        store
            .read_verified(&key, bytes.len() - 1, &hash, ORIGINAL_LIMIT)
            .await
            .is_err()
    );
}

#[tokio::test]
async fn native_store_rejects_symlink_directory_manifest_and_body() {
    for component in ["directory", "manifest", "body"] {
        let root = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        let bytes = b"%PDF-synthetic";
        let (key, hash, dir, version) = disk(root.path(), bytes);
        if component == "directory" {
            let moved = outside.path().join("moved");
            fs::rename(&dir, &moved).unwrap();
            symlink(&moved, &dir).unwrap();
        } else {
            let target = if component == "manifest" {
                dir.join("current.json")
            } else {
                dir.join(version)
            };
            let moved = outside.path().join("moved");
            fs::rename(&target, &moved).unwrap();
            symlink(&moved, &target).unwrap();
        }
        let store = LocalOriginalStore::open(root.path(), 1).unwrap();
        assert!(
            store
                .read_verified(&key, bytes.len(), &hash, ORIGINAL_LIMIT)
                .await
                .is_err()
        );
    }
}

#[tokio::test]
async fn native_store_bounds_manifest_and_rejects_mutated_bytes_or_version_paths() {
    let root = tempfile::tempdir().unwrap();
    let bytes = b"%PDF-synthetic";
    let (key, hash, dir, version) = disk(root.path(), bytes);
    let store = LocalOriginalStore::open(root.path(), 1).unwrap();
    fs::write(dir.join(&version), b"%PDF-tampered!").unwrap();
    assert!(
        store
            .read_verified(&key, bytes.len(), &hash, ORIGINAL_LIMIT)
            .await
            .is_err()
    );
    fs::write(dir.join("current.json"), vec![b' '; 65537]).unwrap();
    assert!(
        store
            .read_verified(&key, bytes.len(), &hash, ORIGINAL_LIMIT)
            .await
            .is_err()
    );
    fs::write(dir.join("current.json"),serde_json::to_vec(&serde_json::json!({"key":key,"size":bytes.len(),"etag":hash,"version":"../../outside"})).unwrap()).unwrap();
    assert!(
        store
            .read_verified(&key, bytes.len(), &hash, ORIGINAL_LIMIT)
            .await
            .is_err()
    );
    for invalid in ["../outside", "a/./b", "a\\..\\b", ""] {
        assert!(
            store
                .read_verified(invalid, bytes.len(), &hash, ORIGINAL_LIMIT)
                .await
                .is_err()
        );
    }
    assert!(LocalOriginalStore::open(root.path(), 0).is_err());
}
