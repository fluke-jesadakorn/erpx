use std::future::Future;

use base64::{Engine, engine::general_purpose::STANDARD};
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::{
    BusinessError, Clock, CompanyId, FailureKind, HoldingId, IdentifierSource, RecordId, UserId,
};
use sha2::{Digest, Sha256};
use sqlx::{Connection, PgConnection};

use crate::{
    application::pdf_verification::PdfVerificationRepository,
    domain::pdf_verification::{
        ApprovedCompanyTrustMaterial, AuthorizedPdfSnapshot, LegalInspectionDocument,
        LegalOriginalRead, PDF_BUSINESS_LIMIT, PdfVerificationEvidence, PdfVerificationFailure,
        PdfVerificationJob, PdfVerificationSnapshotStamp, TRUST_ITEM_BYTES_LIMIT,
        TrustMaterialKind, TrustMaterialReceipt, TrustMaterialState, VERIFICATION_ADMISSION_LIMIT,
    },
};

type Error = PgTransactionError<BusinessError>;

/// Current Identity, Commercial and Files owner bridges on the same retained
/// request/transaction. No default grants, direct storage fallback or private
/// cross-owner SQL is provided by Legal.
pub trait PgLegalPdfVerificationBridge {
    fn authorize_document(
        &mut self,
        connection: &mut PgConnection,
        job: &PdfVerificationJob,
        document: &LegalInspectionDocument,
    ) -> impl Future<Output = Result<String, Error>> + Send;
    fn independent_person_review(
        &mut self,
        connection: &mut PgConnection,
        company: &CompanyId,
        created_by: &UserId,
        approved_by: &UserId,
    ) -> impl Future<Output = Result<bool, Error>> + Send;
    /// Must bound the underlying read before allocation and retain the current
    /// canonical attachment/source witness. Never read an object-key-only file.
    fn read_original(
        &mut self,
        connection: &mut PgConnection,
        job: &PdfVerificationJob,
        document: &LegalInspectionDocument,
        max_bytes: usize,
    ) -> impl Future<Output = Result<LegalOriginalRead, Error>> + Send;
    fn revalidate_original(
        &mut self,
        connection: &mut PgConnection,
        job: &PdfVerificationJob,
        document: &LegalInspectionDocument,
        witness: &str,
    ) -> impl Future<Output = Result<(), Error>> + Send;
}

struct LoadedSource {
    job: PdfVerificationJob,
    document: LegalInspectionDocument,
    stamp: PdfVerificationSnapshotStamp,
    file_witness: Option<String>,
}

/// A single inspection inside the caller-owned serializable Holding transaction.
/// Company/preservation locks remain held through the bounded offline child and
/// retention. Composition must commit the surrounding job state/owner effects.
pub struct PgPdfVerificationRepository<'c, Bridge, ClockSource, Identifiers> {
    holding_id: HoldingId,
    company_id: CompanyId,
    connection: &'c mut PgConnection,
    bridge: &'c mut Bridge,
    clock: &'c ClockSource,
    identifiers: &'c mut Identifiers,
    loaded: Option<LoadedSource>,
}
impl<'c, Bridge, ClockSource, Identifiers>
    PgPdfVerificationRepository<'c, Bridge, ClockSource, Identifiers>
{
    pub fn new(
        holding_id: HoldingId,
        company_id: CompanyId,
        connection: &'c mut PgConnection,
        bridge: &'c mut Bridge,
        clock: &'c ClockSource,
        identifiers: &'c mut Identifiers,
    ) -> Self {
        Self {
            holding_id,
            company_id,
            connection,
            bridge,
            clock,
            identifiers,
            loaded: None,
        }
    }
}

impl<
    Bridge: PgLegalPdfVerificationBridge + Send,
    ClockSource: Clock + Sync,
    Identifiers: IdentifierSource + Send,
> PdfVerificationRepository for PgPdfVerificationRepository<'_, Bridge, ClockSource, Identifiers>
{
    type Error = Error;
    async fn load_authorized(
        &mut self,
        job: &PdfVerificationJob,
    ) -> Result<AuthorizedPdfSnapshot, Error> {
        if self.loaded.is_some() {
            return Err(PdfVerificationFailure::InvalidSnapshot.into());
        }
        if job.holding_id != self.holding_id || job.company_id != self.company_id {
            return Err(PdfVerificationFailure::InvalidScope.into());
        }
        require_transaction(self.connection).await?;
        sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended('legal-files',0))")
            .execute(&mut *self.connection)
            .await?;
        sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1,0))")
            .bind(format!("legal:{}", self.company_id.as_str()))
            .execute(&mut *self.connection)
            .await?;
        let (document, body) =
            load_document(self.connection, &self.company_id, &job.document_version_id).await?;
        let authorization_revision = self
            .bridge
            .authorize_document(self.connection, job, &document)
            .await?;
        check_witness(&authorization_revision)?;
        let (pdf, file_witness) = if document.attachment_link_id.is_some() {
            let source = self
                .bridge
                .read_original(self.connection, job, &document, PDF_BUSINESS_LIMIT)
                .await?;
            check_witness(&source.witness)?;
            (source.bytes, Some(source.witness))
        } else if document.object_key.is_some() {
            return Err(PgTransactionError::Business(BusinessError::new(
                FailureKind::NotFound,
                "LEGAL_ATTACHMENT_LINK_REQUIRED",
                "The document requires its canonical attachment lifecycle.",
            )));
        } else {
            (body.into_bytes(), None)
        };
        if pdf.len() > PDF_BUSINESS_LIMIT {
            return Err(PdfVerificationFailure::DocumentSizeLimit.into());
        }
        if hex::encode(Sha256::digest(&pdf)) != document.checksum_sha256 {
            return Err(PdfVerificationFailure::SnapshotChanged.into());
        }
        let (trust, trust_revision) =
            load_trust(self.connection, self.bridge, &self.company_id, pdf.len()).await?;
        let stamp = PdfVerificationSnapshotStamp {
            document_checksum_sha256: document.checksum_sha256.clone(),
            authorization_revision: authorization_revision.clone(),
            trust_revision: trust_revision.clone(),
            trust: trust
                .iter()
                .map(|item| TrustMaterialReceipt {
                    id: item.id.clone(),
                    version: item.version,
                    kind: item.kind,
                    checksum_sha256: item.checksum_sha256.clone(),
                })
                .collect(),
        };
        self.loaded = Some(LoadedSource {
            job: job.clone(),
            document,
            stamp,
            file_witness,
        });
        Ok(AuthorizedPdfSnapshot {
            holding_id: job.holding_id.clone(),
            company_id: job.company_id.clone(),
            document_version_id: job.document_version_id.clone(),
            document_checksum_sha256: self
                .loaded
                .as_ref()
                .ok_or(PdfVerificationFailure::InvalidSnapshot)?
                .document
                .checksum_sha256
                .clone(),
            authorization_revision,
            trust_revision,
            pdf,
            trust,
        })
    }

    async fn retain_if_current(
        &mut self,
        job: &PdfVerificationJob,
        mut evidence: PdfVerificationEvidence,
    ) -> Result<PdfVerificationEvidence, Error> {
        let source = self
            .loaded
            .take()
            .ok_or(PdfVerificationFailure::InvalidSnapshot)?;
        if source.job != *job
            || source.stamp != evidence.snapshot
            || evidence.inspection_id.is_some()
        {
            return Err(PdfVerificationFailure::SnapshotChanged.into());
        }
        let (document, _) =
            load_document(self.connection, &self.company_id, &job.document_version_id).await?;
        if document.item_id != source.document.item_id
            || document.version != source.document.version
            || document.checksum_sha256 != source.document.checksum_sha256
            || document.attachment_link_id != source.document.attachment_link_id
            || document.object_key != source.document.object_key
        {
            return Err(PdfVerificationFailure::SnapshotChanged.into());
        }
        if self
            .bridge
            .authorize_document(self.connection, job, &document)
            .await?
            != source.stamp.authorization_revision
        {
            return Err(PdfVerificationFailure::SnapshotChanged.into());
        }
        if let Some(witness) = source.file_witness {
            self.bridge
                .revalidate_original(self.connection, job, &document, &witness)
                .await?;
        }
        let (_, revision) = load_trust(self.connection, self.bridge, &self.company_id, 0).await?;
        if revision != source.stamp.trust_revision {
            return Err(PdfVerificationFailure::SnapshotChanged.into());
        }
        let inspection_id = self.identifiers.next();
        let now = self.clock.now().get();
        let materials:Vec<_>=evidence.snapshot.trust.iter().map(|item|serde_json::json!({"id":item.id.as_str(),"kind":kind_name(item.kind),"version":item.version,"checksum":item.checksum_sha256})).collect();
        let mut result = serde_json::to_value(&evidence.report)
            .map_err(|_| PersistenceError::new("LEGAL_INSPECTION_ENCODING_FAILED"))?;
        let object = result
            .as_object_mut()
            .ok_or_else(|| PersistenceError::new("LEGAL_INSPECTION_ENCODING_FAILED"))?;
        object.insert(
            "checksum".to_owned(),
            serde_json::json!(document.checksum_sha256),
        );
        object.insert(
            "inspectionId".to_owned(),
            serde_json::json!(inspection_id.as_str()),
        );
        object.insert("trustMaterials".to_owned(), serde_json::json!(materials));
        let json = serde_json::to_string(&result)
            .map_err(|_| PersistenceError::new("LEGAL_INSPECTION_ENCODING_FAILED"))?;
        let inserted=sqlx::query("INSERT INTO legal_document_inspections(id,document_id,kind,result_json,created_by,created_at) SELECT $1,$2,'verify',$3,$4,$5 WHERE NOT EXISTS(SELECT 1 FROM legal_destructions WHERE item_id=$6 AND state IN ('executing','completed'))")
            .bind(inspection_id.as_str()).bind(document.id.as_str()).bind(json).bind(job.requested_by.as_str()).bind(now).bind(document.item_id.as_str()).execute(&mut *self.connection).await?;
        if inserted.rows_affected() != 1 {
            return Err(PdfVerificationFailure::SnapshotChanged.into());
        }
        sqlx::query("INSERT INTO legal_access_events(id,item_id,document_id,actor_id,action,created_at) VALUES($1,$2,$3,$4,'document.verify',$5)")
            .bind(self.identifiers.next().as_str()).bind(document.item_id.as_str()).bind(document.id.as_str()).bind(job.requested_by.as_str()).bind(now).execute(&mut *self.connection).await?;
        evidence.inspection_id = Some(inspection_id);
        Ok(evidence)
    }
}

async fn load_document(
    connection: &mut PgConnection,
    company: &CompanyId,
    id: &RecordId,
) -> Result<(LegalInspectionDocument, String), Error> {
    let row=sqlx::query_as::<_,(String,i64,String,i64,String,Option<String>,Option<String>,i64,String)>(
        "SELECT d.item_id,d.version::bigint,d.checksum,d.bytes,d.media_type,d.object_key,d.attachment_link_id,octet_length(d.body)::bigint,CASE WHEN octet_length(d.body)<=$3 THEN d.body ELSE '' END FROM legal_documents d JOIN legal_items i ON i.id=d.item_id WHERE d.id=$1 AND i.company_id=$2 FOR SHARE OF d,i")
        .bind(id.as_str()).bind(company.as_str()).bind(PDF_BUSINESS_LIMIT as i64).fetch_optional(&mut *connection).await?;
    let Some((item, version, checksum, bytes, media_type, object_key, link, body_bytes, body)) =
        row
    else {
        return Err(PgTransactionError::Business(BusinessError::new(
            FailureKind::NotFound,
            "LEGAL_NOT_FOUND",
            "Document unavailable.",
        )));
    };
    if media_type != "application/pdf" {
        return Err(PgTransactionError::Business(BusinessError::invalid(
            "LEGAL_INVALID",
            "Signature inspection requires a PDF.",
        )));
    }
    if bytes < 0 || bytes > PDF_BUSINESS_LIMIT as i64 || body_bytes > PDF_BUSINESS_LIMIT as i64 {
        return Err(PdfVerificationFailure::DocumentSizeLimit.into());
    }
    let blocked=sqlx::query_scalar::<_,bool>("SELECT EXISTS(SELECT 1 FROM legal_disposals WHERE document_id=$1) OR EXISTS(SELECT 1 FROM legal_destructions WHERE item_id=$2 AND state IN ('executing','completed'))")
        .bind(id.as_str()).bind(&item).fetch_one(&mut *connection).await?;
    if blocked {
        return Err(PdfVerificationFailure::SnapshotChanged.into());
    }
    let corrupt = || PersistenceError::new("LEGAL_DOCUMENT_CORRUPT_METADATA");
    if version <= 0 {
        return Err(corrupt().into());
    }
    Ok((
        LegalInspectionDocument {
            id: id.clone(),
            item_id: RecordId::new(item).map_err(|_| corrupt())?,
            company_id: company.clone(),
            version: u64::try_from(version).map_err(|_| corrupt())?,
            checksum_sha256: checksum,
            bytes: bytes as u64,
            media_type,
            object_key,
            attachment_link_id: link.map(RecordId::new).transpose().map_err(|_| corrupt())?,
        },
        body,
    ))
}

async fn load_trust(
    connection: &mut PgConnection,
    bridge: &mut impl PgLegalPdfVerificationBridge,
    company: &CompanyId,
    pdf_bytes: usize,
) -> Result<(Vec<ApprovedCompanyTrustMaterial>, String), Error> {
    let mut rows=sqlx::query_as::<_,(String,String,i64,String,Option<String>,String,i64)>(
        "SELECT id,kind,version::bigint,created_by,approved_by,checksum,octet_length(content)::bigint FROM legal_trust_material WHERE company_id=$1 AND state='approved' ORDER BY id LIMIT 101 FOR SHARE")
        .bind(company.as_str()).fetch_all(&mut *connection).await?;
    if rows.len() > 100 {
        return Err(PdfVerificationFailure::AdmissionLimit.into());
    }
    // Match the worker's identifier ordering independently of database locale.
    rows.sort_by(|left, right| left.0.cmp(&right.0));
    let mut materials = Vec::new();
    let mut manifest = Vec::new();
    let mut total = pdf_bytes + 2048;
    for (id, kind, version, created_by, approved_by, checksum, length) in rows {
        if !(0..=6 * 1024 * 1024).contains(&length) {
            return Err(PdfVerificationFailure::AdmissionLimit.into());
        }
        let corrupt = || PersistenceError::new("LEGAL_TRUST_CORRUPT_RECORD");
        let id = RecordId::new(id).map_err(|_| corrupt())?;
        let created_by = UserId::new(created_by).map_err(|_| corrupt())?;
        let approved_by = UserId::new(approved_by.ok_or(PdfVerificationFailure::UnapprovedTrust)?)
            .map_err(|_| corrupt())?;
        if created_by == approved_by
            || !bridge
                .independent_person_review(&mut *connection, company, &created_by, &approved_by)
                .await?
        {
            return Err(PdfVerificationFailure::UnapprovedTrust.into());
        }
        let kind = match kind.as_str() {
            "root" => TrustMaterialKind::Root,
            "intermediate" => TrustMaterialKind::Intermediate,
            "crl" => TrustMaterialKind::Crl,
            "ocsp" => TrustMaterialKind::Ocsp,
            _ => return Err(corrupt().into()),
        };
        let encoded=sqlx::query_scalar::<_,Option<String>>("SELECT CASE WHEN octet_length(content)<=$3 THEN content ELSE NULL END FROM legal_trust_material WHERE id=$1 AND company_id=$2 AND state='approved'")
            .bind(id.as_str()).bind(company.as_str()).bind(6*1024*1024_i64).fetch_optional(&mut *connection).await?.flatten().ok_or(PdfVerificationFailure::SnapshotChanged)?;
        let compact: Vec<_> = encoded
            .bytes()
            .filter(|byte| !matches!(byte, b'\r' | b'\n'))
            .collect();
        let content = STANDARD
            .decode(compact)
            .map_err(|_| PdfVerificationFailure::InvalidTrust)?;
        total = total
            .checked_add(content.len() + 1024)
            .ok_or(PdfVerificationFailure::AdmissionLimit)?;
        if content.len() > TRUST_ITEM_BYTES_LIMIT || total > VERIFICATION_ADMISSION_LIMIT {
            return Err(PdfVerificationFailure::AdmissionLimit.into());
        }
        let actual = hex::encode(Sha256::digest(&content));
        if actual != checksum.to_ascii_lowercase() {
            return Err(PdfVerificationFailure::TrustChecksum.into());
        }
        let version = u64::try_from(version).map_err(|_| corrupt())?;
        manifest.push(serde_json::json!([
            id.as_str(),
            kind_name(kind),
            version,
            created_by.as_str(),
            approved_by.as_str(),
            actual
        ]));
        materials.push(ApprovedCompanyTrustMaterial {
            id,
            company_id: company.clone(),
            version,
            kind,
            state: TrustMaterialState::Approved,
            created_by,
            approved_by: Some(approved_by),
            independent_person_review: true,
            checksum_sha256: actual,
            content,
        });
    }
    let json = serde_json::to_vec(&manifest)
        .map_err(|_| PersistenceError::new("LEGAL_TRUST_MANIFEST_ENCODING_FAILED"))?;
    Ok((materials, hex::encode(Sha256::digest(json))))
}

async fn require_transaction(connection: &mut PgConnection) -> Result<(), Error> {
    if !connection.is_in_transaction() {
        return Err(PersistenceError::new("LEGAL_OWNER_TRANSACTION_REQUIRED").into());
    }
    if sqlx::query_scalar::<_, String>("SHOW transaction_isolation")
        .fetch_one(connection)
        .await?
        != "serializable"
    {
        return Err(PersistenceError::new("LEGAL_SERIALIZABLE_TRANSACTION_REQUIRED").into());
    }
    Ok(())
}
fn check_witness(witness: &str) -> Result<(), PdfVerificationFailure> {
    if witness.is_empty() || witness.len() > 200 {
        Err(PdfVerificationFailure::InvalidSnapshot)
    } else {
        Ok(())
    }
}
fn kind_name(kind: TrustMaterialKind) -> &'static str {
    match kind {
        TrustMaterialKind::Root => "root",
        TrustMaterialKind::Intermediate => "intermediate",
        TrustMaterialKind::Crl => "crl",
        TrustMaterialKind::Ocsp => "ocsp",
    }
}

impl From<PdfVerificationFailure> for Error {
    fn from(failure: PdfVerificationFailure) -> Self {
        let kind = match failure {
            PdfVerificationFailure::Authorization
            | PdfVerificationFailure::InvalidScope
            | PdfVerificationFailure::UnapprovedTrust => FailureKind::Forbidden,
            PdfVerificationFailure::SnapshotChanged
            | PdfVerificationFailure::TrustChecksum
            | PdfVerificationFailure::InvalidSnapshot => FailureKind::Conflict,
            PdfVerificationFailure::DocumentSizeLimit
            | PdfVerificationFailure::AdmissionLimit
            | PdfVerificationFailure::InvalidTrust
            | PdfVerificationFailure::InvalidValidationTime => FailureKind::InvalidInput,
            _ => FailureKind::Unavailable,
        };
        Self::Business(BusinessError::new(
            kind,
            failure.code(),
            "Verification cannot proceed with the current document, authority or trust evidence.",
        ))
    }
}
