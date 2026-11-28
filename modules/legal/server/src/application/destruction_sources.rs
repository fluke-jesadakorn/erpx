//! Revalidate the exact destruction manifest against current Legal and Files facts.
//!
//! This routine does not authorize or delete bytes. Its caller must hold the
//! retention execution unit and Files source locks through validation, deletion,
//! source sealing and transaction completion.
use erpx_platform_shared_server::public::{BusinessError, FailureKind};
use serde::{Deserialize, Serialize};
use std::future::Future;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DestructionDocument {
    pub id: String,
    pub checksum: String,
    pub object_key: Option<String>,
    pub attachment_source_id: Option<String>,
    pub attachment_link_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DestructionUpload {
    pub id: String,
    pub checksum: String,
    pub object_key: String,
    pub attachment_source_id: Option<String>,
    pub attachment_link_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DestructionLedgerEntry {
    pub id: String,
    pub object_key: String,
    pub bytes: i64,
    pub variant: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DestructionCanonicalSource {
    pub id: String,
    pub object_key: String,
    pub checksum: String,
    pub link_ids: Vec<String>,
    pub ledger: Vec<DestructionLedgerEntry>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LegalDestructionManifest {
    pub documents: Vec<DestructionDocument>,
    pub uploads: Vec<DestructionUpload>,
    pub keys: Vec<String>,
    #[serde(default)]
    pub canonical_sources: Vec<DestructionCanonicalSource>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LockedDestructionSource {
    pub object_key: String,
    pub checksum: String,
    pub state: String,
    pub legal_hold: bool,
}

/// Implementations use the caller's already-locked owner transaction. A Files
/// implementation must protect source/link/ledger facts against concurrent edits
/// until the same transaction seals them after physical deletion.
pub trait LegalDestructionSourceFacts {
    type Error: From<BusinessError> + Send;
    fn document_current(
        &mut self,
        item_id: &str,
        document: &DestructionDocument,
    ) -> impl Future<Output = Result<bool, Self::Error>> + Send;
    fn upload_current(
        &mut self,
        item_id: &str,
        upload: &DestructionUpload,
    ) -> impl Future<Output = Result<bool, Self::Error>> + Send;
    fn locked_source(
        &mut self,
        company_id: &str,
        item_id: &str,
        source_id: &str,
    ) -> impl Future<Output = Result<Option<LockedDestructionSource>, Self::Error>> + Send;
    fn link_current(
        &mut self,
        company_id: &str,
        item_id: &str,
        source_id: &str,
        link_id: &str,
    ) -> impl Future<Output = Result<bool, Self::Error>> + Send;
    fn source_shared(
        &mut self,
        company_id: &str,
        item_id: &str,
        source_id: &str,
    ) -> impl Future<Output = Result<bool, Self::Error>> + Send;
    /// Returns retained ledger entries in the same stable order as the manifest.
    fn retained_ledger(
        &mut self,
        source_id: &str,
    ) -> impl Future<Output = Result<Vec<DestructionLedgerEntry>, Self::Error>> + Send;
    fn object_shared(
        &mut self,
        source_id: &str,
        object_key: &str,
    ) -> impl Future<Output = Result<bool, Self::Error>> + Send;
}

/// Current Files source facts used to build the exact immutable request manifest.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CurrentDestructionSource {
    pub object_key: String,
    pub checksum: String,
    pub state: String,
    pub legal_hold: bool,
}

/// Manifest creation and validation share the same caller-owned transaction and
/// locks. The caller must acquire Files, Legal and authority fences before reads.
pub trait LegalDestructionManifestFacts: LegalDestructionSourceFacts {
    fn documents(
        &mut self,
        item_id: &str,
    ) -> impl Future<Output = Result<Vec<DestructionDocument>, Self::Error>> + Send;
    fn uploads(
        &mut self,
        item_id: &str,
    ) -> impl Future<Output = Result<Vec<DestructionUpload>, Self::Error>> + Send;
    fn canonical_source(
        &mut self,
        company_id: &str,
        item_id: &str,
        source_id: &str,
    ) -> impl Future<Output = Result<Option<CurrentDestructionSource>, Self::Error>> + Send;
    fn stored_ledger(
        &mut self,
        source_id: &str,
    ) -> impl Future<Output = Result<Vec<DestructionLedgerEntry>, Self::Error>> + Send;
}

/// Snapshot all documents, external uploads and canonical Files ledger objects.
/// Unlike the older native projection, the result is compatible with the exact
/// retained manifest consumed by the destruction worker, and is validated before
/// the caller records it as part of the request.
pub async fn legal_destruction_manifest<P: LegalDestructionManifestFacts>(
    facts: &mut P,
    item_id: &str,
    company_id: &str,
) -> Result<LegalDestructionManifest, P::Error> {
    let documents = facts.documents(item_id).await?;
    let uploads = facts.uploads(item_id).await?;
    let mut source_ids = Vec::<String>::new();
    for source_id in documents
        .iter()
        .filter_map(|row| row.attachment_source_id.as_ref())
        .chain(
            uploads
                .iter()
                .filter_map(|row| row.attachment_source_id.as_ref()),
        )
    {
        if !source_ids.contains(source_id) {
            source_ids.push(source_id.clone());
        }
    }
    let mut canonical_sources = Vec::new();
    for source_id in source_ids {
        let source = facts
            .canonical_source(company_id, item_id, &source_id)
            .await?
            .ok_or_else(|| invalid("LEGAL_DESTRUCTION_SOURCE_UNAVAILABLE"))?;
        if source.state != "available" || source.legal_hold {
            return Err(invalid("LEGAL_DESTRUCTION_SOURCE_UNAVAILABLE").into());
        }
        let mut links = Vec::new();
        for row in documents
            .iter()
            .filter(|row| row.attachment_source_id.as_deref() == Some(source_id.as_str()))
        {
            if row.object_key.as_deref() != Some(source.object_key.as_str())
                || row.checksum != source.checksum
            {
                return Err(invalid("LEGAL_DESTRUCTION_SOURCE_MISMATCH").into());
            }
            let link = row
                .attachment_link_id
                .as_ref()
                .filter(|id| !id.is_empty())
                .ok_or_else(|| invalid("LEGAL_DESTRUCTION_SOURCE_MISMATCH"))?;
            links.push(link.clone());
        }
        for row in uploads
            .iter()
            .filter(|row| row.attachment_source_id.as_deref() == Some(source_id.as_str()))
        {
            if row.object_key != source.object_key || row.checksum != source.checksum {
                return Err(invalid("LEGAL_DESTRUCTION_SOURCE_MISMATCH").into());
            }
            let link = row
                .attachment_link_id
                .as_ref()
                .filter(|id| !id.is_empty())
                .ok_or_else(|| invalid("LEGAL_DESTRUCTION_SOURCE_MISMATCH"))?;
            links.push(link.clone());
        }
        let ledger = facts.stored_ledger(&source_id).await?;
        if !ledger
            .iter()
            .any(|row| row.variant == "original" && row.object_key == source.object_key)
        {
            return Err(invalid("LEGAL_DESTRUCTION_LEDGER_MISSING").into());
        }
        canonical_sources.push(DestructionCanonicalSource {
            id: source_id,
            object_key: source.object_key,
            checksum: source.checksum,
            link_ids: links,
            ledger,
        });
    }
    let mut keys = Vec::<String>::new();
    for key in documents
        .iter()
        .filter_map(|row| row.object_key.as_ref())
        .chain(uploads.iter().map(|row| &row.object_key))
        .chain(
            canonical_sources
                .iter()
                .flat_map(|source| source.ledger.iter().map(|row| &row.object_key)),
        )
    {
        if !key.is_empty() && !keys.contains(key) {
            keys.push(key.clone());
        }
    }
    let manifest = LegalDestructionManifest {
        documents,
        uploads,
        keys,
        canonical_sources,
    };
    validate_legal_destruction_sources(facts, item_id, company_id, &manifest).await?;
    Ok(manifest)
}

fn invalid(code: &'static str) -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        code,
        "The retained destruction source changed or is shared.",
    )
}

/// Check every manifest reference, including source sharing and exact ledger
/// contents, before physical deletion. A retry permits a previously destroyed
/// source only when the original references and retention facts still match.
pub async fn validate_legal_destruction_sources<P: LegalDestructionSourceFacts>(
    facts: &mut P,
    item_id: &str,
    company_id: &str,
    manifest: &LegalDestructionManifest,
) -> Result<(), P::Error> {
    for document in &manifest.documents {
        if !facts.document_current(item_id, document).await? {
            return Err(invalid("LEGAL_DESTRUCTION_DOCUMENT_CHANGED").into());
        }
    }
    for upload in &manifest.uploads {
        if !facts.upload_current(item_id, upload).await? {
            return Err(invalid("LEGAL_DESTRUCTION_UPLOAD_CHANGED").into());
        }
    }
    for source in &manifest.canonical_sources {
        let current = facts.locked_source(company_id, item_id, &source.id).await?;
        if !current.is_some_and(|current| {
            current.object_key == source.object_key
                && current.checksum == source.checksum
                && matches!(current.state.as_str(), "available" | "destroyed")
                && !current.legal_hold
        }) {
            return Err(invalid("LEGAL_DESTRUCTION_SOURCE_CHANGED").into());
        }
        for link in &source.link_ids {
            if !facts
                .link_current(company_id, item_id, &source.id, link)
                .await?
            {
                return Err(invalid("LEGAL_DESTRUCTION_LINK_CHANGED").into());
            }
        }
        if facts.source_shared(company_id, item_id, &source.id).await? {
            return Err(invalid("LEGAL_DESTRUCTION_SOURCE_SHARED").into());
        }
        if facts.retained_ledger(&source.id).await? != source.ledger {
            return Err(invalid("LEGAL_DESTRUCTION_LEDGER_CHANGED").into());
        }
        for entry in &source.ledger {
            if !manifest.keys.contains(&entry.object_key)
                || facts.object_shared(&source.id, &entry.object_key).await?
            {
                return Err(invalid("LEGAL_DESTRUCTION_OBJECT_SHARED").into());
            }
        }
    }
    let prefix = format!("legal/{company_id}/{item_id}/");
    for key in &manifest.keys {
        let canonical = manifest
            .canonical_sources
            .iter()
            .any(|source| source.ledger.iter().any(|entry| entry.object_key == *key));
        let legacy_document = manifest
            .documents
            .iter()
            .any(|row| row.object_key.as_ref() == Some(key) && row.attachment_source_id.is_none());
        let legacy_upload = manifest
            .uploads
            .iter()
            .any(|row| row.object_key == *key && row.attachment_source_id.is_none());
        if !canonical && (!(legacy_document || legacy_upload) || !key.starts_with(&prefix)) {
            return Err(invalid("LEGAL_DESTRUCTION_SCOPE").into());
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Facts {
        source: LockedDestructionSource,
        ledger: Vec<DestructionLedgerEntry>,
        shared_source: bool,
        shared_object: bool,
        documents: Vec<DestructionDocument>,
        uploads: Vec<DestructionUpload>,
    }
    impl LegalDestructionSourceFacts for Facts {
        type Error = BusinessError;
        async fn document_current(
            &mut self,
            _: &str,
            _: &DestructionDocument,
        ) -> Result<bool, Self::Error> {
            Ok(true)
        }
        async fn upload_current(
            &mut self,
            _: &str,
            _: &DestructionUpload,
        ) -> Result<bool, Self::Error> {
            Ok(true)
        }
        async fn locked_source(
            &mut self,
            _: &str,
            _: &str,
            _: &str,
        ) -> Result<Option<LockedDestructionSource>, Self::Error> {
            Ok(Some(self.source.clone()))
        }
        async fn link_current(
            &mut self,
            _: &str,
            _: &str,
            _: &str,
            _: &str,
        ) -> Result<bool, Self::Error> {
            Ok(true)
        }
        async fn source_shared(&mut self, _: &str, _: &str, _: &str) -> Result<bool, Self::Error> {
            Ok(self.shared_source)
        }
        async fn retained_ledger(
            &mut self,
            _: &str,
        ) -> Result<Vec<DestructionLedgerEntry>, Self::Error> {
            Ok(self.ledger.clone())
        }
        async fn object_shared(&mut self, _: &str, _: &str) -> Result<bool, Self::Error> {
            Ok(self.shared_object)
        }
    }
    impl LegalDestructionManifestFacts for Facts {
        async fn documents(&mut self, _: &str) -> Result<Vec<DestructionDocument>, Self::Error> {
            Ok(self.documents.clone())
        }
        async fn uploads(&mut self, _: &str) -> Result<Vec<DestructionUpload>, Self::Error> {
            Ok(self.uploads.clone())
        }
        async fn canonical_source(
            &mut self,
            _: &str,
            _: &str,
            _: &str,
        ) -> Result<Option<CurrentDestructionSource>, Self::Error> {
            Ok(Some(CurrentDestructionSource {
                object_key: self.source.object_key.clone(),
                checksum: self.source.checksum.clone(),
                state: self.source.state.clone(),
                legal_hold: self.source.legal_hold,
            }))
        }
        async fn stored_ledger(
            &mut self,
            _: &str,
        ) -> Result<Vec<DestructionLedgerEntry>, Self::Error> {
            Ok(self.ledger.clone())
        }
    }
    fn fixture() -> (Facts, LegalDestructionManifest) {
        let ledger = vec![DestructionLedgerEntry {
            id: "l1".into(),
            object_key: "private/original".into(),
            bytes: 42,
            variant: "original".into(),
        }];
        let source = LockedDestructionSource {
            object_key: "private/original".into(),
            checksum: "sha".into(),
            state: "available".into(),
            legal_hold: false,
        };
        let manifest = LegalDestructionManifest {
            documents: vec![],
            uploads: vec![],
            keys: vec!["private/original".into()],
            canonical_sources: vec![DestructionCanonicalSource {
                id: "source".into(),
                object_key: source.object_key.clone(),
                checksum: source.checksum.clone(),
                link_ids: vec!["link".into()],
                ledger: ledger.clone(),
            }],
        };
        (
            Facts {
                source,
                ledger,
                shared_source: false,
                shared_object: false,
                documents: vec![],
                uploads: vec![],
            },
            manifest,
        )
    }
    #[tokio::test]
    async fn snapshot_captures_external_uploads_links_and_original_ledger() {
        let (mut facts, _) = fixture();
        facts.documents.push(DestructionDocument {
            id: "doc".into(),
            checksum: "sha".into(),
            object_key: Some("private/original".into()),
            attachment_source_id: Some("source".into()),
            attachment_link_id: Some("doc-link".into()),
        });
        facts.uploads.push(DestructionUpload {
            id: "upload".into(),
            checksum: "sha".into(),
            object_key: "private/original".into(),
            attachment_source_id: Some("source".into()),
            attachment_link_id: Some("upload-link".into()),
        });
        let manifest = legal_destruction_manifest(&mut facts, "item", "company")
            .await
            .unwrap();
        assert_eq!(manifest.documents.len(), 1);
        assert_eq!(manifest.uploads.len(), 1);
        assert_eq!(manifest.keys, vec!["private/original"]);
        assert_eq!(
            manifest.canonical_sources[0].link_ids,
            vec!["doc-link", "upload-link"]
        );
        let wire = serde_json::to_value(&manifest).unwrap();
        assert_eq!(wire["documents"][0]["object_key"], "private/original");
        assert_eq!(wire["canonicalSources"][0]["objectKey"], "private/original");
        facts.uploads[0].checksum = "changed".into();
        assert_eq!(
            legal_destruction_manifest(&mut facts, "item", "company")
                .await
                .unwrap_err()
                .code,
            "LEGAL_DESTRUCTION_SOURCE_MISMATCH"
        );
    }
    #[tokio::test]
    async fn validated_source_retry_and_legal_hold() {
        let (mut facts, manifest) = fixture();
        assert!(
            validate_legal_destruction_sources(&mut facts, "item", "company", &manifest)
                .await
                .is_ok()
        );
        facts.source.state = "destroyed".into();
        assert!(
            validate_legal_destruction_sources(&mut facts, "item", "company", &manifest)
                .await
                .is_ok()
        );
        facts.source.legal_hold = true;
        assert_eq!(
            validate_legal_destruction_sources(&mut facts, "item", "company", &manifest)
                .await
                .unwrap_err()
                .code,
            "LEGAL_DESTRUCTION_SOURCE_CHANGED"
        );
    }
    #[tokio::test]
    async fn shared_sources_changed_ledger_and_out_of_scope_keys_are_rejected() {
        let (mut facts, mut manifest) = fixture();
        facts.shared_source = true;
        assert_eq!(
            validate_legal_destruction_sources(&mut facts, "item", "company", &manifest)
                .await
                .unwrap_err()
                .code,
            "LEGAL_DESTRUCTION_SOURCE_SHARED"
        );
        facts.shared_source = false;
        facts.ledger[0].bytes += 1;
        assert_eq!(
            validate_legal_destruction_sources(&mut facts, "item", "company", &manifest)
                .await
                .unwrap_err()
                .code,
            "LEGAL_DESTRUCTION_LEDGER_CHANGED"
        );
        facts.ledger = manifest.canonical_sources[0].ledger.clone();
        facts.shared_object = true;
        assert_eq!(
            validate_legal_destruction_sources(&mut facts, "item", "company", &manifest)
                .await
                .unwrap_err()
                .code,
            "LEGAL_DESTRUCTION_OBJECT_SHARED"
        );
        facts.shared_object = false;
        manifest.keys.push("other-company/private".into());
        assert_eq!(
            validate_legal_destruction_sources(&mut facts, "item", "company", &manifest)
                .await
                .unwrap_err()
                .code,
            "LEGAL_DESTRUCTION_SCOPE"
        );
    }
}
