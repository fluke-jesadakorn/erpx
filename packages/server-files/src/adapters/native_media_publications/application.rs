//! Owner ports and the bounded create flow for native media publications.
//!
//! The router never sees SQL or object-store keys. It parses the request, the
//! application resolves the source owner and replay identity, and the Postgres
//! adapter owns the transaction that stores bytes and writes publication rows.

use std::{future::Future, pin::Pin};

use erpx_server_shared::public::BusinessError;

use super::domain::{
    is_publishable_purpose, is_publishable_target, stable_id, CreatePublicationInput,
    PublicationList, PublicationRow,
};

pub type PublicationFuture<'a, T> =
    Pin<Box<dyn Future<Output = Result<T, BusinessError>> + Send + 'a>>;

/// The preview source row required to publish. Selected by the owner adapter in
/// one company-scoped statement; no caller can name a relation or predicate.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SourceLink {
    pub id: String,
    pub attachment_id: String,
    pub preview_object_key: String,
    pub media_type: String,
    pub target_type: String,
    pub target_id: String,
    pub purpose_key: String,
    pub source_state: String,
}

/// A stored object returned by the FileStore port.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PublicationObject {
    pub bytes: Vec<u8>,
    pub media_type: String,
}

/// Object-store port. `put_once` has the retired `onlyIf.etagDoesNotMatch:"*"`
/// semantic: a key already holding identical bytes is a successful replay,
/// a key holding different bytes is a conflict. Implementations must enforce
/// this without trusting a caller-supplied key.
pub trait PublicationFileStore: Send + Sync {
    fn get(&self, key: &str) -> PublicationFuture<'_, Option<PublicationObject>>;
    fn put_once(
        &self,
        key: &str,
        bytes: &[u8],
        media_type: &str,
        checksum: &str,
    ) -> PublicationFuture<'_, ()>;
}

/// Owner database port. Every method is executed entirely inside this crate.
pub trait PublicationStore: Send + Sync {
    fn list(
        &self,
        company: &str,
        target_type: &str,
        target_id: &str,
        can_manage: bool,
    ) -> PublicationFuture<'_, PublicationList>;
    fn source(&self, company: &str, link_id: &str) -> PublicationFuture<'_, Option<SourceLink>>;
    fn publication(&self, company: &str, id: &str) -> PublicationFuture<'_, Option<PublicationRow>>;
    fn create(
        &self,
        files: std::sync::Arc<dyn PublicationFileStore>,
        request: &PreparedCreate,
    ) -> PublicationFuture<'_, PublicationRow>;
}

/// Transport facts collected by the HTTP adapter, already origin/key checked.
#[derive(Clone, Debug)]
pub struct CreateRequest {
    pub company: String,
    pub user: String,
    pub holding_id: String,
    pub input: CreatePublicationInput,
    pub key: String,
    pub now: i64,
}

/// Validated, replay-scoped facts handed to the transactional owner adapter.
#[derive(Clone, Debug)]
pub struct PreparedCreate {
    pub id: String,
    pub company: String,
    pub user: String,
    pub holding_id: String,
    pub input: CreatePublicationInput,
    pub key: String,
    pub now: i64,
    pub source: SourceLink,
}

fn not_found(code: &'static str, message: &'static str) -> BusinessError {
    BusinessError::new(erpx_server_shared::public::FailureKind::NotFound, code, message)
}

fn invalid(code: &'static str, message: &'static str) -> BusinessError {
    BusinessError::invalid(code, message)
}

fn conflict(code: &'static str, message: &'static str) -> BusinessError {
    BusinessError::new(erpx_server_shared::public::FailureKind::Conflict, code, message)
}

/// Complete create flow. Preconditions are external to the transaction; the
/// adapter repeats the idempotency check under the owner advisory lock.
pub async fn create_publication(
    store: &dyn PublicationStore,
    files: std::sync::Arc<dyn PublicationFileStore>,
    request: CreateRequest,
) -> Result<PublicationRow, BusinessError> {
    let source = store
        .source(&request.company, &request.input.source_link_id)
        .await?
        .ok_or_else(|| {
            not_found(
                "PUBLICATION_SOURCE_NOT_FOUND",
                "Choose an available image with a sanitized preview.",
            )
        })?;
    if source.source_state != "available" || source.preview_object_key.is_empty() {
        return Err(not_found(
            "PUBLICATION_SOURCE_NOT_FOUND",
            "Choose an available image with a sanitized preview.",
        ));
    }
    if !is_publishable_target(&request.input.target_type)
        || !is_publishable_target(&source.target_type)
        || !is_publishable_purpose(&source.purpose_key)
    {
        return Err(invalid(
            "PUBLICATION_NOT_ALLOWED",
            "This file purpose cannot be published.",
        ));
    }
    if request.input.target_type != source.target_type
        || request.input.target_id != source.target_id
    {
        return Err(invalid(
            "PUBLICATION_TARGET_MISMATCH",
            "Published media must remain attached to its authorized source owner.",
        ));
    }
    let id = stable_id(&request.company, &request.user, &request.key);
    if let Some(existing) = store.publication(&request.company, &id).await? {
        if existing.source_link_id != request.input.source_link_id
            || existing.target_type != request.input.target_type
            || existing.target_id != request.input.target_id
            || existing.alt_text != request.input.alt_text
            || existing.ordinal != request.input.ordinal
        {
            return Err(conflict(
                "IDEMPOTENCY_CONFLICT",
                "This command key was already used for another publication.",
            ));
        }
        return Ok(existing);
    }
    store
        .create(
            files,
            &PreparedCreate {
                id,
                company: request.company,
                user: request.user,
                holding_id: request.holding_id,
                input: request.input,
                key: request.key,
                now: request.now,
                source,
            },
        )
        .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::native_media_publications::domain::CreatePublicationInput;

    fn inputs() -> CreateRequest {
        CreateRequest {
            company: "company-1".into(),
            user: "user-1".into(),
            holding_id: "holding-1".into(),
            input: CreatePublicationInput {
                source_link_id: "link-1".into(),
                target_type: "holding_brand".into(),
                target_id: "acme".into(),
                alt_text: "Acme mark".into(),
                ordinal: 0,
            },
            key: "key-00000001".into(),
            now: 1_760_000_000_000,
        }
    }

    fn source() -> SourceLink {
        SourceLink {
            id: "link-1".into(),
            attachment_id: "attachment-1".into(),
            preview_object_key: "holding-1/quarantine/upload-1/original".into(),
            media_type: "image/webp".into(),
            target_type: "holding_brand".into(),
            target_id: "acme".into(),
            purpose_key: "primary".into(),
            source_state: "available".into(),
        }
    }

    fn row(id: &str) -> PublicationRow {
        PublicationRow {
            id: id.into(),
            company_id: Some("company-1".into()),
            source_link_id: "link-1".into(),
            target_type: "holding_brand".into(),
            target_id: "acme".into(),
            derivative_object_key: "holding-1/published/pub-1/r1".into(),
            derivative_media_type: "image/webp".into(),
            derivative_checksum_sha256: "abc".into(),
            derivative_bytes: 8,
            alt_text: "Acme mark".into(),
            ordinal: 0,
            revision: 1,
            state: "draft".into(),
            created_by: "user-1".into(),
            created_at: 1_760_000_000_000,
            approved_by: None,
            approved_at: None,
            revoked_by: None,
            revoked_at: None,
            revocation_reason: None,
        }
    }

    struct Probe {
        source: Option<SourceLink>,
        publication: Option<PublicationRow>,
        created: std::sync::Arc<std::sync::Mutex<u32>>,
    }

    impl PublicationStore for Probe {
        fn list(
            &self,
            _: &str,
            _: &str,
            _: &str,
            _: bool,
        ) -> PublicationFuture<'_, PublicationList> {
            Box::pin(async { Err(BusinessError::invalid("UNUSED", "Unused.")) })
        }
        fn source(&self, _: &str, _: &str) -> PublicationFuture<'_, Option<SourceLink>> {
            let source = self.source.clone();
            Box::pin(async move { Ok(source) })
        }
        fn publication(&self, _: &str, _: &str) -> PublicationFuture<'_, Option<PublicationRow>> {
            let row = self.publication.clone();
            Box::pin(async move { Ok(row) })
        }
        fn create(
            &self,
            _: std::sync::Arc<dyn PublicationFileStore>,
            request: &PreparedCreate,
        ) -> PublicationFuture<'_, PublicationRow> {
            *self.created.lock().unwrap() += 1;
            let row = row(&request.id);
            Box::pin(async move { Ok(row) })
        }
    }

    struct Files;

    impl PublicationFileStore for Files {
        fn get(&self, _: &str) -> PublicationFuture<'_, Option<PublicationObject>> {
            Box::pin(async { Ok(None) })
        }
        fn put_once(&self, _: &str, _: &[u8], _: &str, _: &str) -> PublicationFuture<'_, ()> {
            Box::pin(async { Ok(()) })
        }
    }

    #[tokio::test]
    async fn replay_returns_existing_row_without_writing() {
        let id = stable_id("company-1", "user-1", "key-00000001");
        let created = std::sync::Arc::new(std::sync::Mutex::new(0));
        let store = Probe {
            source: Some(source()),
            publication: Some(row(&id)),
            created: created.clone(),
        };
        let result = create_publication(&store, std::sync::Arc::new(Files), inputs())
            .await
            .expect("replay succeeds");
        assert_eq!(result.id, id);
        assert_eq!(*created.lock().unwrap(), 0);
    }

    #[tokio::test]
    async fn mismatched_source_owner_fails_before_write() {
        let created = std::sync::Arc::new(std::sync::Mutex::new(0));
        let mut wrong = source();
        wrong.target_id = "other".into();
        let store = Probe {
            source: Some(wrong),
            publication: None,
            created: created.clone(),
        };
        let error = create_publication(&store, std::sync::Arc::new(Files), inputs())
            .await
            .expect_err("mismatch fails");
        assert_eq!(error.code, "PUBLICATION_TARGET_MISMATCH");
        assert_eq!(*created.lock().unwrap(), 0);
    }

    #[tokio::test]
    async fn unavailable_source_fails_before_write() {
        let created = std::sync::Arc::new(std::sync::Mutex::new(0));
        let mut source = source();
        source.source_state = "quarantined".into();
        let store = Probe {
            source: Some(source),
            publication: None,
            created: created.clone(),
        };
        let error = create_publication(&store, std::sync::Arc::new(Files), inputs())
            .await
            .expect_err("unavailable source fails");
        assert_eq!(error.code, "PUBLICATION_SOURCE_NOT_FOUND");
        assert_eq!(*created.lock().unwrap(), 0);
    }
}
