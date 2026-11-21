//! OCR uses two fresh owner units. Files/record/purchase/release witnesses stay
//! local to each unit; computation receives owned stable input data only.
use super::{
    postgres_processing::{PgLegalProcessingBridge, PgLegalProcessingRepository},
    postgres_processing_source::CurrentLegalProcessingSource,
};
use crate::{
    application::document_extraction::require_extraction_report,
    domain::{
        document_extraction::{
            DOCUMENT_INPUT_LIMIT, DocumentExtractionInput, DocumentExtractionKind,
            DocumentExtractionSource, LegalDocumentMediaType,
        },
        document_processing::{
            ComputedLegalExtraction, LegalExtractionRetention, PreparedLegalExtraction,
            extraction_changed,
        },
        pdf_verification::{LegalInspectionDocument, LegalOriginalRead},
        processing::LegalProcessingKind,
    },
};
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::{
    BusinessError, Clock, CompanyId, FailureKind, IdentifierSource, RecordId,
};
use sha2::{Digest, Sha256};
use sqlx::PgConnection;
use std::future::Future;
type Error = PgTransactionError<BusinessError>;

pub trait PgLegalDocumentExtractionBridge {
    fn authorize_document(
        &mut self,
        c: &mut PgConnection,
        source: &CurrentLegalProcessingSource,
        document: &LegalInspectionDocument,
    ) -> impl Future<Output = Result<String, Error>> + Send;
    fn read_original(
        &mut self,
        c: &mut PgConnection,
        source: &CurrentLegalProcessingSource,
        document: &LegalInspectionDocument,
        max_bytes: usize,
    ) -> impl Future<Output = Result<LegalOriginalRead, Error>> + Send;
    fn revalidate_original(
        &mut self,
        c: &mut PgConnection,
        source: &CurrentLegalProcessingSource,
        document: &LegalInspectionDocument,
        witness: &str,
    ) -> impl Future<Output = Result<(), Error>> + Send;
}
struct Loaded {
    document: LegalInspectionDocument,
    input: DocumentExtractionInput,
    source_data_sha256: String,
    authority: String,
    file_witness: Option<String>,
}
impl Loaded {
    fn kind(&self) -> DocumentExtractionKind {
        match &self.input.source {
            DocumentExtractionSource::InlineText { .. } => DocumentExtractionKind::InlineText,
            DocumentExtractionSource::Original { media_type, .. } => {
                DocumentExtractionKind::Original(*media_type)
            }
        }
    }
}

pub async fn prepare_current_legal_extraction<
    Bridge: PgLegalProcessingBridge + PgLegalDocumentExtractionBridge + Send,
    ClockSource: Clock + Sync,
    Identifiers: IdentifierSource + Send,
>(
    c: &mut PgConnection,
    bridge: &mut Bridge,
    clock: &ClockSource,
    ids: &mut Identifiers,
    source: &CurrentLegalProcessingSource,
) -> Result<PreparedLegalExtraction, Error> {
    authorize_work(c, bridge, clock, ids, source).await?;
    let loaded = load(c, bridge, source).await?;
    revalidate(c, bridge, source, &loaded).await?;
    // Neither loaded.authority nor loaded.file_witness escapes this unit.
    Ok(PreparedLegalExtraction {
        lease: source.lease().clone(),
        document: loaded.document,
        source_data_sha256: loaded.source_data_sha256,
        prepared_at: clock.now().get(),
        input: loaded.input,
    })
}

pub async fn retain_current_legal_extraction<
    Bridge: PgLegalProcessingBridge + PgLegalDocumentExtractionBridge + Send,
    ClockSource: Clock + Sync,
    Identifiers: IdentifierSource + Send,
>(
    c: &mut PgConnection,
    bridge: &mut Bridge,
    clock: &ClockSource,
    ids: &mut Identifiers,
    source: &CurrentLegalProcessingSource,
    computed: &ComputedLegalExtraction,
) -> Result<LegalExtractionRetention, Error> {
    authorize_work(c, bridge, clock, ids, source).await?;
    let loaded = load(c, bridge, source).await?;
    if source.lease() != &computed.lease
        || loaded.document != computed.document
        || loaded.source_data_sha256 != computed.source_data_sha256
        || computed.prepared_at > computed.computed_at
        || computed.computed_at > clock.now().get()
    {
        return Err(extraction_changed().into());
    }
    revalidate(c, bridge, source, &loaded).await?;
    match &computed.outcome {
        Err(failure) => {
            PgLegalProcessingRepository::new(
                source.holding_id().clone(),
                source.company_id().clone(),
                c,
                bridge,
                clock,
                ids,
            )
            .fail_current_source(source, failure.code())
            .await?;
            Ok(LegalExtractionRetention::Failed {
                code: failure.code().to_owned(),
                retryable: failure.retryable(),
            })
        }
        Ok(report) => {
            require_extraction_report(&loaded.document.checksum_sha256, loaded.kind(), report)
                .map_err(provider_error)?;
            let inspection = ids.next();
            let now = clock.now().get();
            let mut result = serde_json::to_value(report)
                .map_err(|_| PersistenceError::new("LEGAL_INSPECTION_ENCODING_FAILED"))?;
            let object = result
                .as_object_mut()
                .ok_or_else(|| PersistenceError::new("LEGAL_INSPECTION_ENCODING_FAILED"))?;
            object.insert(
                "inspectionId".into(),
                serde_json::json!(inspection.as_str()),
            );
            object.insert(
                "processingJobId".into(),
                serde_json::json!(source.job().id.as_str()),
            );
            object.insert("computedInput".into(),serde_json::json!({"format":"erpx-document-extraction/v1","jobId":source.job().id.as_str(),"attempt":source.lease().attempts,"documentId":loaded.document.id.as_str(),"documentVersion":loaded.document.version,"checksum":loaded.document.checksum_sha256,"sourceDataSha256":loaded.source_data_sha256,"preparedAt":computed.prepared_at,"computedAt":computed.computed_at}));
            let json = serde_json::to_string(&result)
                .map_err(|_| PersistenceError::new("LEGAL_INSPECTION_ENCODING_FAILED"))?;
            if json.len() > crate::domain::processing::PROCESSING_RESULT_LIMIT {
                return Err(PersistenceError::new("LEGAL_INSPECTION_OUTPUT_LIMIT").into());
            }
            let inserted=sqlx::query("INSERT INTO legal_document_inspections(id,document_id,kind,result_json,created_by,created_at) SELECT $1,$2,'ocr',$3,$4,$5 WHERE NOT EXISTS(SELECT 1 FROM legal_destructions WHERE item_id=$6 AND state IN ('executing','completed'))")
                .bind(inspection.as_str()).bind(loaded.document.id.as_str()).bind(json).bind(source.user_id().as_str()).bind(now).bind(loaded.document.item_id.as_str()).execute(&mut *c).await?;
            if inserted.rows_affected() != 1 {
                return Err(extraction_changed().into());
            }
            sqlx::query("INSERT INTO legal_access_events(id,item_id,document_id,actor_id,action,created_at) VALUES($1,$2,$3,$4,'document.ocr',$5)")
                .bind(ids.next().as_str()).bind(loaded.document.item_id.as_str()).bind(loaded.document.id.as_str()).bind(source.user_id().as_str()).bind(now).execute(&mut *c).await?;
            revalidate(c, bridge, source, &loaded).await?;
            let result = PgLegalProcessingRepository::new(
                source.holding_id().clone(),
                source.company_id().clone(),
                c,
                bridge,
                clock,
                ids,
            )
            .complete_current_source(source, &inspection)
            .await?;
            Ok(LegalExtractionRetention::Completed(result))
        }
    }
}
async fn authorize_work<
    Bridge: PgLegalProcessingBridge + Send,
    ClockSource: Clock + Sync,
    Identifiers: IdentifierSource + Send,
>(
    c: &mut PgConnection,
    bridge: &mut Bridge,
    clock: &ClockSource,
    ids: &mut Identifiers,
    source: &CurrentLegalProcessingSource,
) -> Result<(), Error> {
    source.require_current(c, clock.now().get()).await?;
    if source.job().kind != LegalProcessingKind::Ocr {
        return Err(BusinessError::invalid(
            "LEGAL_PROCESSING_KIND_MISMATCH",
            "Text extraction requires an OCR job.",
        )
        .into());
    }
    PgLegalProcessingRepository::new(
        source.holding_id().clone(),
        source.company_id().clone(),
        c,
        bridge,
        clock,
        ids,
    )
    .authorize_work(source.lease())
    .await
}
async fn load(
    c: &mut PgConnection,
    bridge: &mut (impl PgLegalDocumentExtractionBridge + Send),
    source: &CurrentLegalProcessingSource,
) -> Result<Loaded, Error> {
    let (document, body) =
        document(c, source.company_id(), &source.job().document.document_id).await?;
    let authority = bridge.authorize_document(c, source, &document).await?;
    witness(&authority)?;
    let (input, file_witness, source_data_sha256) = if document.attachment_link_id.is_some() {
        let media_type = LegalDocumentMediaType::try_from(document.media_type.as_str())
            .map_err(provider_error)?;
        let read = bridge
            .read_original(c, source, &document, DOCUMENT_INPUT_LIMIT)
            .await?;
        witness(&read.witness)?;
        digest(&read.source_data_sha256)?;
        (
            DocumentExtractionSource::Original {
                media_type,
                bytes: read.bytes,
            },
            Some(read.witness),
            read.source_data_sha256,
        )
    } else if document.object_key.is_some() {
        return Err(BusinessError::new(
            FailureKind::NotFound,
            "LEGAL_ATTACHMENT_LINK_REQUIRED",
            "The document requires its canonical attachment lifecycle.",
        )
        .into());
    } else {
        if document.media_type != "text/plain" {
            return Err(provider_error(
                crate::domain::document_extraction::DocumentExtractionFailure::UnsupportedInput,
            ));
        }
        let data = serde_json::json!({"format":"erpx-inline-legal-extraction/v1","holding":source.holding_id().as_str(),"company":source.company_id().as_str(),"item":document.item_id.as_str(),"document":document.id.as_str(),"version":document.version,"checksum":document.checksum_sha256,"mediaType":"text/plain","bytes":document.bytes});
        (
            DocumentExtractionSource::InlineText { body },
            None,
            hex::encode(Sha256::digest(data.to_string().as_bytes())),
        )
    };
    let input = DocumentExtractionInput {
        checksum_sha256: document.checksum_sha256.clone(),
        source: input,
    };
    if input.bytes().len() > DOCUMENT_INPUT_LIMIT
        || input.bytes().len() as u64 != document.bytes
        || hex::encode(Sha256::digest(input.bytes())) != document.checksum_sha256
    {
        return Err(extraction_changed().into());
    }
    Ok(Loaded {
        document,
        input,
        source_data_sha256,
        authority,
        file_witness,
    })
}
async fn revalidate(
    c: &mut PgConnection,
    bridge: &mut (impl PgLegalDocumentExtractionBridge + Send),
    source: &CurrentLegalProcessingSource,
    loaded: &Loaded,
) -> Result<(), Error> {
    let (current, _) = document(c, source.company_id(), &loaded.document.id).await?;
    if current != loaded.document
        || bridge.authorize_document(c, source, &current).await? != loaded.authority
    {
        return Err(extraction_changed().into());
    }
    if let Some(witness) = &loaded.file_witness {
        bridge
            .revalidate_original(c, source, &current, witness)
            .await?;
    }
    Ok(())
}
async fn document(
    c: &mut PgConnection,
    company: &CompanyId,
    id: &RecordId,
) -> Result<(LegalInspectionDocument, String), Error> {
    let row=sqlx::query_as::<_,(String,i64,String,i64,String,Option<String>,Option<String>,i64,String)>("SELECT d.item_id,d.version::bigint,d.checksum,d.bytes,d.media_type,d.object_key,d.attachment_link_id,octet_length(d.body)::bigint,CASE WHEN octet_length(d.body)<=$3 THEN d.body ELSE '' END FROM legal_documents d JOIN legal_items i ON i.id=d.item_id WHERE d.id=$1 AND i.company_id=$2 FOR SHARE OF d,i")
        .bind(id.as_str()).bind(company.as_str()).bind(DOCUMENT_INPUT_LIMIT as i64).fetch_optional(&mut *c).await?.ok_or_else(extraction_changed)?;
    let (item, version, checksum, bytes, media_type, object_key, link, body_bytes, body) = row;
    if version <= 0
        || bytes < 0
        || bytes > DOCUMENT_INPUT_LIMIT as i64
        || body_bytes > DOCUMENT_INPUT_LIMIT as i64
    {
        return Err(extraction_changed().into());
    }
    digest(&checksum)?;
    let blocked=sqlx::query_scalar::<_,bool>("SELECT EXISTS(SELECT 1 FROM legal_disposals WHERE document_id=$1) OR EXISTS(SELECT 1 FROM legal_destructions WHERE item_id=$2 AND state IN ('executing','completed'))").bind(id.as_str()).bind(&item).fetch_one(&mut *c).await?;
    if blocked {
        return Err(extraction_changed().into());
    }
    Ok((
        LegalInspectionDocument {
            id: id.clone(),
            item_id: RecordId::new(item)?,
            company_id: company.clone(),
            version: version as u64,
            checksum_sha256: checksum,
            bytes: bytes as u64,
            media_type,
            object_key,
            attachment_link_id: link.map(RecordId::new).transpose()?,
        },
        body,
    ))
}
fn witness(value: &str) -> Result<(), Error> {
    if value.is_empty() || value.len() > 200 {
        return Err(extraction_changed().into());
    }
    Ok(())
}
fn digest(value: &str) -> Result<(), Error> {
    if value.len() != 64
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(extraction_changed().into());
    }
    Ok(())
}
fn provider_error(error: crate::domain::document_extraction::DocumentExtractionFailure) -> Error {
    BusinessError::invalid(
        error.code(),
        "The document extraction provider returned an unsupported or invalid result.",
    )
    .into()
}
