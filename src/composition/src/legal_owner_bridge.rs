//! Concrete PDF worker assembly. Queue administration and scheduling remain
//! separate workflows; no successful implementation is supplied for them here.
#[cfg(feature = "legal-document-worker")]
use crate::legal_document_worker::{
    LegalDocumentBridgeFactory, run_legal_extraction_short_transactions,
};
#[cfg(feature = "legal-pdf-worker")]
use crate::legal_pdf_worker::{LegalPdfBridgeFactory, run_legal_pdf_short_transactions};
use crate::{
    legal_owner_policy::{
        ActualExternalPolicy, LegalOwnerConfiguration, actor, authority_revision, scope_error,
    },
    legal_queue_source::LegalQueuedSource,
};
use erpx_server_files::public::canonical_original::*;
use erpx_server_identity_access::public::effective_authority::{
    AuthorizedWorkContext, PgQueuedWorkAuthorizer, QueuedWorkBinding,
};
#[cfg(feature = "legal-pdf-worker")]
use erpx_server_identity_access::public::release_reviewers::PgSamePersonReader;
use erpx_server_legal::public::record_policy::*;
use erpx_server_legal::public::*;
use erpx_server_runtime::public::postgres::PgTransactionError;
use erpx_server_shared::public::{
    BusinessError, Clock, CommandKey, FailureKind, IdentifierSource, RecordId, TimestampMillis,
};
#[cfg(feature = "legal-pdf-worker")]
use erpx_server_shared::public::{CompanyId, UserId};
use sqlx::PgConnection;
use std::sync::Arc;
type Error = PgTransactionError<BusinessError>;

/// Always prepares the actual owner/Commercial/release/Files sources before
/// the fresh coordinator's first data read. No production factory constructor
/// exists that could accidentally pair it with an unprepared source.
#[cfg(feature = "legal-pdf-worker")]
pub async fn run_current_legal_pdf<
    Store: OriginalByteStore + 'static,
    Verifier: PdfVerifierProcess + Sync,
    ClockSource: Clock + Send + Sync + 'static,
>(
    authorizer: &PgQueuedWorkAuthorizer,
    source: LegalQueuedSource,
    configuration: LegalOwnerConfiguration,
    store: Arc<Store>,
    verifier: &Verifier,
    clock: Arc<ClockSource>,
) -> Result<serde_json::Value, Error> {
    let source = configuration.queued_source(source);
    let factory = ConcreteLegalOwnerFactory {
        configuration,
        store,
        clock: SharedClock(clock.clone()),
    };
    run_legal_pdf_short_transactions(authorizer, source, factory, verifier, clock).await
}
#[derive(Clone)]
struct SharedClock(Arc<dyn Clock + Send + Sync>);
impl Clock for SharedClock {
    fn now(&self) -> TimestampMillis {
        self.0.now()
    }
}
pub struct LegalWorkerIdentifiers;
impl IdentifierSource for LegalWorkerIdentifiers {
    fn next(&mut self) -> RecordId {
        RecordId::new(uuid::Uuid::new_v4().to_string())
            .expect("a generated UUID is a valid record identifier")
    }
}
pub struct ConcreteLegalOwnerFactory<Store> {
    configuration: LegalOwnerConfiguration,
    store: Arc<Store>,
    clock: SharedClock,
}
impl<Store> Clone for ConcreteLegalOwnerFactory<Store> {
    fn clone(&self) -> Self {
        Self {
            configuration: self.configuration.clone(),
            store: self.store.clone(),
            clock: self.clock.clone(),
        }
    }
}
#[cfg(feature = "legal-pdf-worker")]
impl<Store: OriginalByteStore + 'static> LegalPdfBridgeFactory
    for ConcreteLegalOwnerFactory<Store>
{
    type Bridge<'c>
        = ConcreteLegalOwnerBridge<'c, Store>
    where
        Self: 'c;
    type Identifiers = LegalWorkerIdentifiers;
    fn bridge<'c>(&'c self, current: &'c AuthorizedWorkContext) -> Self::Bridge<'c> {
        ConcreteLegalOwnerBridge {
            factory: self,
            current,
            original: None,
        }
    }
    fn identifiers(&self) -> Self::Identifiers {
        LegalWorkerIdentifiers
    }
}
struct RetainedOriginal {
    actor: OriginalReadActor,
    query: LegalOriginalQuery,
    witness: OriginalReadWitness,
}
pub struct ConcreteLegalOwnerBridge<'c, Store> {
    factory: &'c ConcreteLegalOwnerFactory<Store>,
    current: &'c AuthorizedWorkContext,
    original: Option<RetainedOriginal>,
}
fn worker_only() -> Error {
    BusinessError::new(
        FailureKind::Unavailable,
        "LEGAL_WORKER_OPERATION_ONLY",
        "This assembly implements current claimed document work only.",
    )
    .into()
}
fn request(
    item: RecordId,
    document: RecordId,
    configuration: &LegalOwnerConfiguration,
) -> LegalRecordRequest {
    LegalRecordRequest {
        item_id: item,
        document_id: Some(document),
        operation: LegalRecordOperation::Processing,
        mode: configuration.commercial_mode,
    }
}
async fn access(
    c: &mut PgConnection,
    current: &AuthorizedWorkContext,
    config: &LegalOwnerConfiguration,
    item: RecordId,
    document: RecordId,
    now: i64,
) -> Result<LegalRecordAccess, Error> {
    current.require_permission("legal.read")?;
    if current.holding_id() != &config.holding_id
        || current.selection().workspace().company_id != config.company_id
    {
        return Err(scope_error().into());
    }
    let mut external = ActualExternalPolicy(config.clone());
    PgLegalRecordPolicy::new(
        config.holding_id.clone(),
        config.company_id.clone(),
        c,
        &mut external,
    )
    .inspect(&actor(current), &request(item, document, config), now)
    .await
}
fn original_actor(current: &AuthorizedWorkContext) -> Result<OriginalReadActor, BusinessError> {
    Ok(OriginalReadActor {
        holding_id: current.holding_id().clone(),
        company_id: current.selection().workspace().company_id.clone(),
        user_id: current.user_id().clone(),
        selection_revision: QueuedWorkBinding::from_current(current)
            .to_json()
            .map(|value| {
                use sha2::{Digest, Sha256};
                hex::encode(Sha256::digest(value))
            })?,
    })
}
struct OriginalPolicy<'c> {
    current: &'c AuthorizedWorkContext,
    configuration: &'c LegalOwnerConfiguration,
    clock: &'c SharedClock,
}
impl PgOriginalReadPolicy for OriginalPolicy<'_> {
    async fn prepare(
        &mut self,
        c: &mut PgConnection,
        a: &OriginalReadActor,
        q: &LegalOriginalQuery,
    ) -> Result<(), Error> {
        if a != &original_actor(self.current)?
            || q.holding_id != self.configuration.holding_id
            || q.company_id != self.configuration.company_id
        {
            return Err(scope_error().into());
        }
        self.configuration.lock_sources(c).await
    }
    async fn authorize(
        &mut self,
        c: &mut PgConnection,
        a: &OriginalReadActor,
        q: &LegalOriginalQuery,
        facts: &CanonicalOriginalFacts,
    ) -> Result<String, Error> {
        if a != &original_actor(self.current)? {
            return Err(scope_error().into());
        }
        let read = access(
            c,
            self.current,
            self.configuration,
            q.item_id.clone(),
            q.document_id.clone(),
            self.clock.now().get(),
        )
        .await?;
        let document = read.document.as_ref().ok_or_else(scope_error)?;
        if document.attachment_link_id.as_ref() != Some(&q.link_id)
            || document.attachment_source_id.as_ref() != Some(&facts.source_id)
            || document.checksum_sha256 != q.expected_checksum_sha256
            || document.media_type != q.expected_media_type.as_mime()
            || document.bytes != facts.bytes
            || facts.source_owner_type != "legal_item"
            || facts.source_owner_id != q.item_id.as_str()
            || facts.source_purpose != "legal_document"
        {
            return Err(scope_error().into());
        }
        Ok(authority_revision(self.current, &read.witness)?)
    }
}
impl<Store: OriginalByteStore + 'static> ConcreteLegalOwnerBridge<'_, Store> {
    async fn authorize_job(
        &mut self,
        c: &mut PgConnection,
        job: &LegalProcessingJob,
    ) -> Result<(), Error> {
        if job.holding_id != self.factory.configuration.holding_id
            || job.company_id != self.factory.configuration.company_id
            || &job.created_by != self.current.user_id()
        {
            return Err(scope_error().into());
        }
        let source = access(
            c,
            self.current,
            &self.factory.configuration,
            job.document.item_id.clone(),
            job.document.document_id.clone(),
            self.factory.clock.now().get(),
        )
        .await?;
        if source.item.version as u64 != job.document.item_version
            || source
                .document
                .as_ref()
                .is_none_or(|d| d.checksum_sha256 != job.document.checksum_sha256)
        {
            return Err(scope_error().into());
        }
        Ok(())
    }
}
impl<Store: OriginalByteStore + 'static> PgLegalProcessingBridge
    for ConcreteLegalOwnerBridge<'_, Store>
{
    async fn authorize_queue(
        &mut self,
        _: &mut PgConnection,
        _: &LegalProcessingActor,
        _: &CommandKey,
        _: &LegalProcessingDocument,
        _: LegalProcessingKind,
    ) -> Result<LegalProcessingQueueAuthority, Error> {
        Err(worker_only())
    }
    async fn replay(
        &mut self,
        _: &mut PgConnection,
        _: &LegalProcessingActor,
        _: &CommandKey,
        _: &RecordId,
        _: LegalProcessingKind,
    ) -> Result<Option<LegalProcessingReceipt>, Error> {
        Err(worker_only())
    }
    async fn record_queued(
        &mut self,
        _: &mut PgConnection,
        _: &LegalProcessingActor,
        _: &CommandKey,
        _: &LegalProcessingDocument,
        _: LegalProcessingKind,
        _: &LegalProcessingReceipt,
    ) -> Result<(), Error> {
        Err(worker_only())
    }
    async fn candidate_eligible(
        &mut self,
        c: &mut PgConnection,
        job: &LegalProcessingJob,
    ) -> Result<bool, Error> {
        self.authorize_job(c, job).await?;
        Ok(true)
    }
    async fn authorize_completion(
        &mut self,
        c: &mut PgConnection,
        lease: &LegalProcessingLease,
    ) -> Result<(), Error> {
        self.authorize_job(c, &lease.job).await
    }
    async fn authorize_status(
        &mut self,
        _: &mut PgConnection,
        _: &LegalProcessingActor,
        _: &RecordId,
    ) -> Result<(), Error> {
        Err(worker_only())
    }
}
impl<Store: OriginalByteStore + 'static> ConcreteLegalOwnerBridge<'_, Store> {
    async fn authorize_document(
        &mut self,
        c: &mut PgConnection,
        job: &PdfVerificationJob,
        document: &LegalInspectionDocument,
    ) -> Result<String, Error> {
        if job.holding_id != self.factory.configuration.holding_id
            || job.company_id != self.factory.configuration.company_id
            || document.company_id != job.company_id
            || document.id != job.document_version_id
        {
            return Err(scope_error().into());
        }
        let read = access(
            c,
            self.current,
            &self.factory.configuration,
            document.item_id.clone(),
            document.id.clone(),
            self.factory.clock.now().get(),
        )
        .await?;
        let actual = read.document.as_ref().ok_or_else(scope_error)?;
        if actual.version as u64 != document.version
            || actual.checksum_sha256 != document.checksum_sha256
            || actual.bytes as u64 != document.bytes
            || actual.media_type != document.media_type
            || actual.attachment_link_id != document.attachment_link_id
        {
            return Err(scope_error().into());
        }
        Ok(authority_revision(self.current, &read.witness)?)
    }
    #[cfg(feature = "legal-pdf-worker")]
    async fn independent_person_review(
        &mut self,
        c: &mut PgConnection,
        company: &CompanyId,
        creator: &UserId,
        approver: &UserId,
    ) -> Result<bool, Error> {
        if company != &self.factory.configuration.company_id {
            return Err(scope_error().into());
        }
        Ok(!PgSamePersonReader::new(c)
            .same_person(company, creator, approver)
            .await?)
    }
    async fn read_original(
        &mut self,
        c: &mut PgConnection,
        job: &PdfVerificationJob,
        document: &LegalInspectionDocument,
        max_bytes: usize,
    ) -> Result<LegalOriginalRead, Error> {
        self.authorize_document(c, job, document).await?;
        let actor = original_actor(self.current)?;
        let query = LegalOriginalQuery {
            holding_id: job.holding_id.clone(),
            company_id: job.company_id.clone(),
            item_id: document.item_id.clone(),
            document_id: document.id.clone(),
            link_id: document
                .attachment_link_id
                .clone()
                .ok_or_else(scope_error)?,
            expected_checksum_sha256: document.checksum_sha256.clone(),
            expected_media_type: LegalOriginalMediaType::try_from(document.media_type.as_str())?,
            max_bytes,
        };
        let mut policy = OriginalPolicy {
            current: self.current,
            configuration: &self.factory.configuration,
            clock: &self.factory.clock,
        };
        let mut ids = LegalWorkerIdentifiers;
        let read = PgCanonicalOriginalReader::new(
            job.holding_id.clone(),
            job.company_id.clone(),
            c,
            &mut policy,
            self.factory.store.as_ref(),
            &self.factory.clock,
            &mut ids,
        )
        .read(&actor, &query)
        .await?;
        let witness = read.witness.as_str().to_owned();
        self.original = Some(RetainedOriginal {
            actor,
            query,
            witness: read.witness,
        });
        Ok(LegalOriginalRead {
            bytes: read.bytes,
            witness,
            source_data_sha256: read.input_identity.source_data_sha256,
        })
    }
    async fn revalidate_original(
        &mut self,
        c: &mut PgConnection,
        job: &PdfVerificationJob,
        document: &LegalInspectionDocument,
        witness: &str,
    ) -> Result<(), Error> {
        let original = self.original.as_ref().ok_or_else(scope_error)?;
        if original.witness.as_str() != witness
            || job.holding_id != original.query.holding_id
            || job.company_id != original.query.company_id
            || job.document_version_id != original.query.document_id
            || document.id != original.query.document_id
            || document.item_id != original.query.item_id
            || document.company_id != original.query.company_id
            || document.attachment_link_id.as_ref() != Some(&original.query.link_id)
            || document.checksum_sha256 != original.query.expected_checksum_sha256
            || document.media_type != original.query.expected_media_type.as_mime()
        {
            return Err(scope_error().into());
        }
        let mut policy = OriginalPolicy {
            current: self.current,
            configuration: &self.factory.configuration,
            clock: &self.factory.clock,
        };
        let mut ids = LegalWorkerIdentifiers;
        PgCanonicalOriginalReader::new(
            self.factory.configuration.holding_id.clone(),
            self.factory.configuration.company_id.clone(),
            c,
            &mut policy,
            self.factory.store.as_ref(),
            &self.factory.clock,
            &mut ids,
        )
        .revalidate(&original.actor, &original.query, &original.witness)
        .await
    }
}

#[cfg(feature = "legal-pdf-worker")]
impl<Store: OriginalByteStore + 'static> PgLegalPdfVerificationBridge
    for ConcreteLegalOwnerBridge<'_, Store>
{
    async fn authorize_document(
        &mut self,
        c: &mut PgConnection,
        job: &PdfVerificationJob,
        document: &LegalInspectionDocument,
    ) -> Result<String, Error> {
        ConcreteLegalOwnerBridge::authorize_document(self, c, job, document).await
    }
    async fn independent_person_review(
        &mut self,
        c: &mut PgConnection,
        company: &CompanyId,
        creator: &UserId,
        approver: &UserId,
    ) -> Result<bool, Error> {
        ConcreteLegalOwnerBridge::independent_person_review(self, c, company, creator, approver)
            .await
    }
    async fn read_original(
        &mut self,
        c: &mut PgConnection,
        job: &PdfVerificationJob,
        document: &LegalInspectionDocument,
        max_bytes: usize,
    ) -> Result<LegalOriginalRead, Error> {
        ConcreteLegalOwnerBridge::read_original(self, c, job, document, max_bytes).await
    }
    async fn revalidate_original(
        &mut self,
        c: &mut PgConnection,
        job: &PdfVerificationJob,
        document: &LegalInspectionDocument,
        witness: &str,
    ) -> Result<(), Error> {
        ConcreteLegalOwnerBridge::revalidate_original(self, c, job, document, witness).await
    }
}
#[cfg(feature = "legal-document-worker")]
impl<Store: OriginalByteStore + 'static> LegalDocumentBridgeFactory
    for ConcreteLegalOwnerFactory<Store>
{
    type Bridge<'c>
        = ConcreteLegalOwnerBridge<'c, Store>
    where
        Self: 'c;
    type Identifiers = LegalWorkerIdentifiers;
    fn bridge<'c>(&'c self, current: &'c AuthorizedWorkContext) -> Self::Bridge<'c> {
        ConcreteLegalOwnerBridge {
            factory: self,
            current,
            original: None,
        }
    }
    fn identifiers(&self) -> Self::Identifiers {
        LegalWorkerIdentifiers
    }
}
#[cfg(feature = "legal-document-worker")]
fn extraction_job(source: &CurrentLegalProcessingSource) -> PdfVerificationJob {
    PdfVerificationJob {
        job_id: source.job().id.clone(),
        holding_id: source.holding_id().clone(),
        company_id: source.company_id().clone(),
        document_version_id: source.job().document.document_id.clone(),
        requested_by: source.user_id().clone(),
    }
}
#[cfg(feature = "legal-document-worker")]
impl<Store: OriginalByteStore + 'static>
    erpx_server_legal::public::document_processing::PgLegalDocumentExtractionBridge
    for ConcreteLegalOwnerBridge<'_, Store>
{
    async fn authorize_document(
        &mut self,
        c: &mut PgConnection,
        source: &CurrentLegalProcessingSource,
        document: &LegalInspectionDocument,
    ) -> Result<String, Error> {
        self.authorize_job(c, source.job()).await?;
        ConcreteLegalOwnerBridge::authorize_document(self, c, &extraction_job(source), document)
            .await
    }
    async fn read_original(
        &mut self,
        c: &mut PgConnection,
        source: &CurrentLegalProcessingSource,
        document: &LegalInspectionDocument,
        max_bytes: usize,
    ) -> Result<LegalOriginalRead, Error> {
        self.authorize_job(c, source.job()).await?;
        ConcreteLegalOwnerBridge::read_original(
            self,
            c,
            &extraction_job(source),
            document,
            max_bytes,
        )
        .await
    }
    async fn revalidate_original(
        &mut self,
        c: &mut PgConnection,
        source: &CurrentLegalProcessingSource,
        document: &LegalInspectionDocument,
        witness: &str,
    ) -> Result<(), Error> {
        self.authorize_job(c, source.job()).await?;
        ConcreteLegalOwnerBridge::revalidate_original(
            self,
            c,
            &extraction_job(source),
            document,
            witness,
        )
        .await
    }
}
/// The configured source and factory are assembled together; OCR alone does
/// not enable signature parsers, OpenSSL or PDF trust dependencies.
#[cfg(feature = "legal-document-worker")]
pub async fn run_current_legal_extraction<
    Store: OriginalByteStore + 'static,
    Extractor: DocumentExtractor + Sync,
    ClockSource: Clock + Send + Sync + 'static,
>(
    authorizer: &PgQueuedWorkAuthorizer,
    source: LegalQueuedSource,
    configuration: LegalOwnerConfiguration,
    store: Arc<Store>,
    extractor: &Extractor,
    clock: Arc<ClockSource>,
) -> Result<erpx_server_legal::public::document_processing::LegalExtractionRetention, Error> {
    let source = configuration.queued_source(source);
    let factory = ConcreteLegalOwnerFactory {
        configuration,
        store,
        clock: SharedClock(clock.clone()),
    };
    run_legal_extraction_short_transactions(authorizer, source, factory, extractor, clock).await
}
