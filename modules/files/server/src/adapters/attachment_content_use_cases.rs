//! Application for the bounded Files attachment content read.
//!
//! The use case opens no transaction and reads no HTTP request. The owning
//! record's authority is checked through a composition port, the exact company
//! row is loaded by the SQL adapter, and the verified byte store is the only
//! source of response bytes. No TypeScript operation prepares or executes any
//! part of this read.
use crate::application::canonical_original::OriginalByteStore;
use crate::domain::attachment_content::{
    ContentFacts, MAX_CONTENT_BYTES, select, validate_identifier,
};
use erpx_platform_runtime_server::public::postgres::{PgTransactionError, PgWorkFuture};
use erpx_platform_shared_server::public::{BusinessError, FailureKind};
use sha2::{Digest, Sha256};

pub type Error = PgTransactionError<BusinessError>;

/// Verified bytes plus the exact disposition facts the HTTP adapter returns.
#[derive(Clone, Debug)]
pub struct AttachmentContentRead {
    pub media_type: String,
    pub filename: String,
    pub inline: bool,
    pub body: Vec<u8>,
    pub checksum_sha256: String,
}

/// Composition-owned owner-unit port. `C` is a `PgConnection` in production;
/// tests use a harmless stand-in so no database is needed for the application.
pub trait AttachmentContentPorts<C>: Send + Sync {
    fn authorize<'a>(
        &'a self,
        c: &'a mut C,
        facts: &'a ContentFacts,
    ) -> PgWorkFuture<'a, (), BusinessError>;

    fn load<'a>(
        &'a self,
        c: &'a mut C,
        company_id: &'a str,
        link_id: &'a str,
    ) -> PgWorkFuture<'a, Option<ContentFacts>, BusinessError>;

    fn record_access<'a>(
        &'a self,
        c: &'a mut C,
        facts: &'a ContentFacts,
        variant: &'static str,
        now: i64,
    ) -> PgWorkFuture<'a, (), BusinessError>;
}

/// Read one link's exact content variant. Authority precedes every byte read;
/// the store result is re-hashed and its length rechecked before access is
/// recorded and the complete reply is returned.
pub async fn read_attachment_content<C, P, S>(
    c: &mut C,
    ports: &P,
    store: &S,
    company_id: &str,
    user_id: &str,
    link_id: &str,
    variant: crate::domain::attachment_content::ContentVariant,
    now: i64,
) -> Result<AttachmentContentRead, Error>
where
    P: AttachmentContentPorts<C>,
    S: OriginalByteStore,
{
    validate_identifier(link_id, "ATTACHMENT_LINK_NOT_FOUND")?;
    let facts = ports.load(c, company_id, link_id).await?.ok_or_else(|| {
        BusinessError::new(
            FailureKind::NotFound,
            "ATTACHMENT_LINK_NOT_FOUND",
            "File link not found.",
        )
    })?;
    if facts.company_id != company_id {
        return Err(BusinessError::new(
            FailureKind::Forbidden,
            "ATTACHMENT_CROSS_COMPANY",
            "Files cannot cross Companies.",
        )
        .into());
    }
    ports.authorize(c, &facts).await?;
    let selection = select(&facts, variant, user_id)?;
    let body = store
        .read_verified(
            &selection.object_key,
            selection.bytes,
            &selection.checksum_sha256,
            MAX_CONTENT_BYTES as usize,
        )
        .await?;
    if body.len() != selection.bytes
        || hex::encode(Sha256::digest(&body)) != selection.checksum_sha256
    {
        return Err(BusinessError::new(
            FailureKind::Conflict,
            "ATTACHMENT_READ_CHANGED",
            "The file changed. Reload before opening it.",
        )
        .into());
    }
    ports
        .record_access(c, &facts, selection.variant, now)
        .await?;
    Ok(AttachmentContentRead {
        media_type: selection.media_type,
        filename: selection.filename,
        inline: selection.inline,
        body,
        checksum_sha256: selection.checksum_sha256,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::attachment_content::ContentVariant;
    use std::future::Future;
    use std::pin::Pin;

    const BODY: &[u8] = b"%PDF-1.4 native attachment";

    struct FakePorts {
        facts: ContentFacts,
        denied: bool,
        accesses: std::sync::Arc<std::sync::Mutex<Vec<String>>>,
    }

    fn future<T: Send + 'static>(
        value: Result<T, BusinessError>,
    ) -> Pin<Box<dyn Future<Output = Result<T, PgTransactionError<BusinessError>>> + Send + 'static>>
    {
        Box::pin(async move { value.map_err(PgTransactionError::Business) })
    }

    impl AttachmentContentPorts<()> for FakePorts {
        fn authorize<'a>(
            &'a self,
            _: &'a mut (),
            _: &'a ContentFacts,
        ) -> PgWorkFuture<'a, (), BusinessError> {
            if self.denied {
                future(Err(BusinessError::new(
                    FailureKind::Forbidden,
                    "DENIED",
                    "Denied.",
                )))
            } else {
                future(Ok(()))
            }
        }
        fn load<'a>(
            &'a self,
            _: &'a mut (),
            company_id: &'a str,
            link_id: &'a str,
        ) -> PgWorkFuture<'a, Option<ContentFacts>, BusinessError> {
            let mut facts = self.facts.clone();
            facts.company_id = company_id.into();
            facts.link_id = link_id.into();
            future(Ok(Some(facts)))
        }
        fn record_access<'a>(
            &'a self,
            _: &'a mut (),
            facts: &'a ContentFacts,
            variant: &'static str,
            now: i64,
        ) -> PgWorkFuture<'a, (), BusinessError> {
            self.accesses.lock().unwrap().push(format!(
                "{}|{}|{}|{now}",
                facts.company_id, facts.link_id, variant
            ));
            future(Ok(()))
        }
    }

    struct FakeStore {
        bytes: Vec<u8>,
    }
    impl OriginalByteStore for FakeStore {
        async fn read_verified(
            &self,
            key: &str,
            size: usize,
            checksum: &str,
            limit: usize,
        ) -> Result<Vec<u8>, BusinessError> {
            assert!(!key.is_empty());
            assert_eq!(size, self.bytes.len());
            assert_eq!(checksum, hex::encode(Sha256::digest(&self.bytes)));
            assert!(self.bytes.len() <= limit);
            Ok(self.bytes.clone())
        }
    }

    fn facts(checksum: String) -> ContentFacts {
        ContentFacts {
            link_id: "link-1".into(),
            set_id: "set-1".into(),
            company_id: "co-1".into(),
            target_type: "staff_task".into(),
            target_id: "task-1".into(),
            purpose_key: None,
            component_kind: None,
            created_by: "actor".into(),
            link_state: "bound".into(),
            source_id: Some("attachment-1".into()),
            source_state: Some("available".into()),
            scanned_at: Some(1),
            scanner_definition_at: Some(1),
            revoked_at: None,
            destroyed_at: None,
            original_name: Some("evidence.pdf".into()),
            media_type: Some("application/pdf".into()),
            bytes: Some(BODY.len() as i64),
            checksum_sha256: Some(checksum),
            object_key: Some("objects/original".into()),
            preview_object_key: None,
            preview_bytes: None,
            preview_checksum_sha256: None,
            preview_media_type: None,
        }
    }

    #[tokio::test]
    async fn reads_verified_bytes_and_records_the_exact_variant() {
        let accesses = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let ports = FakePorts {
            facts: facts(hex::encode(Sha256::digest(BODY))),
            denied: false,
            accesses: accesses.clone(),
        };
        let store = FakeStore {
            bytes: BODY.to_vec(),
        };
        let reply = read_attachment_content(
            &mut (),
            &ports,
            &store,
            "co-1",
            "actor",
            "link-1",
            ContentVariant::Original,
            7,
        )
        .await
        .unwrap();
        assert_eq!(reply.media_type, "application/pdf");
        assert_eq!(reply.filename, "evidence.pdf");
        assert!(!reply.inline);
        assert_eq!(reply.body, BODY);
        assert_eq!(reply.checksum_sha256, hex::encode(Sha256::digest(BODY)));
        assert_eq!(*accesses.lock().unwrap(), vec!["co-1|link-1|original|7"]);
    }

    #[tokio::test]
    async fn authority_denial_precedes_the_byte_store() {
        let ports = FakePorts {
            facts: facts(hex::encode(Sha256::digest(BODY))),
            denied: true,
            accesses: std::sync::Arc::new(std::sync::Mutex::new(Vec::new())),
        };
        let store = FakeStore {
            bytes: BODY.to_vec(),
        };
        assert!(
            read_attachment_content(
                &mut (),
                &ports,
                &store,
                "co-1",
                "actor",
                "link-1",
                ContentVariant::Original,
                7,
            )
            .await
            .is_err()
        );
    }
}
