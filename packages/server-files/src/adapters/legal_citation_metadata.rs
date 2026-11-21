//! Metadata-only Files half of the legacy Legal citation availability check.
//! Composition must derive the request from an actual retained Legal source,
//! then separately retain IAM, purchase, Legal and audit witnesses.
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::{BusinessError, CompanyId, FailureKind};
use sqlx::{Connection, PgConnection};

type Error = PgTransactionError<BusinessError>;
type Unit = (String, i32, String, String, String, String, Vec<i64>);

#[derive(Clone)]
pub struct LegalCitationMetadataRequest {
    pub company: CompanyId,
    pub object_key: Option<String>,
    pub attachment_link_id: Option<String>,
    pub checksum: String,
}

pub async fn lock_legal_citation_metadata(c: &mut PgConnection) -> Result<(), Error> {
    if !c.is_in_transaction() {
        return Err(PersistenceError::new("LEGAL_CITATION_FILES_TRANSACTION_REQUIRED").into());
    }
    sqlx::query("LOCK TABLE public.attachment_links,public.attachments IN SHARE MODE NOWAIT")
        .execute(c)
        .await
        .map_err(PersistenceError::pre_snapshot_lock_conflict)?;
    Ok(())
}

pub struct CurrentLegalCitationMetadata {
    request: LegalCitationMetadataRequest,
    unit: Unit,
    available: bool,
    image: String,
}
impl CurrentLegalCitationMetadata {
    pub fn available(&self) -> bool {
        self.available
    }
    /// Recheck the exact selected rows, including missing-link absence, in the
    /// original fenced unit. Never reads file bytes or writes an access audit.
    pub async fn require_current(&self, c: &mut PgConnection) -> Result<(), Error> {
        if unit(c).await? != self.unit {
            return Err(changed());
        }
        let (available, image) = read(c, &self.request).await?;
        if available != self.available || image != self.image {
            return Err(changed());
        }
        Ok(())
    }
}

pub async fn inspect_legal_citation_metadata(
    c: &mut PgConnection,
    request: LegalCitationMetadataRequest,
) -> Result<CurrentLegalCitationMetadata, Error> {
    let identity = unit(c).await?;
    let (available, image) = read(c, &request).await?;
    if unit(c).await? != identity {
        return Err(changed());
    }
    Ok(CurrentLegalCitationMetadata {
        request,
        unit: identity,
        available,
        image,
    })
}

async fn read(
    c: &mut PgConnection,
    request: &LegalCitationMetadataRequest,
) -> Result<(bool, String), Error> {
    // The legacy predicate does not compare document.attachment_source_id or
    // file object key; a caller must not infer those additional constraints.
    Ok(sqlx::query_as("SELECT ($1::text IS NULL OR EXISTS(SELECT 1 FROM public.attachment_links l JOIN public.attachments a ON a.id=l.attachment_id WHERE l.id=$2 AND l.state='bound' AND a.company_id=$3 AND a.source_state='available' AND a.revoked_at IS NULL AND a.checksum_sha256=$4)),CASE WHEN $1::text IS NULL THEN '{}' ELSE jsonb_build_object('link',(SELECT jsonb_build_array(l.id,l.state,l.attachment_id) FROM public.attachment_links l WHERE l.id=$2),'attachment',(SELECT jsonb_build_array(a.id,a.company_id,a.source_state,a.revoked_at,a.checksum_sha256) FROM public.attachments a JOIN public.attachment_links l ON l.attachment_id=a.id WHERE l.id=$2))::text END")
        .bind(request.object_key.as_deref()).bind(request.attachment_link_id.as_deref()).bind(request.company.as_str()).bind(&request.checksum).fetch_one(c).await?)
}

async fn unit(c: &mut PgConnection) -> Result<Unit, Error> {
    if !c.is_in_transaction()
        || sqlx::query_scalar::<_, String>("SHOW transaction_isolation")
            .fetch_one(&mut *c)
            .await?
            != "serializable"
    {
        return Err(PersistenceError::new("LEGAL_CITATION_FILES_TRANSACTION_REQUIRED").into());
    }
    let valid:bool=sqlx::query_scalar("SELECT count(*)=2 AND bool_and(p.relnamespace='public'::regnamespace AND p.relkind IN ('r','p') AND p.relpersistence='p' AND to_regclass(p.relname::text)=p.oid AND NOT row_security_active(p.oid) AND EXISTS(SELECT 1 FROM pg_locks l WHERE l.pid=pg_backend_pid() AND l.granted AND l.relation=p.oid AND l.mode IN ('ShareLock','ShareRowExclusiveLock','ExclusiveLock','AccessExclusiveLock'))) FROM pg_class p WHERE p.oid IN ('public.attachment_links'::regclass,'public.attachments'::regclass)").fetch_one(&mut *c).await?;
    if !valid {
        return Err(PersistenceError::new("LEGAL_CITATION_FILES_FENCE_REQUIRED").into());
    }
    Ok(sqlx::query_as("SELECT current_database(),pg_backend_pid(),pg_current_xact_id()::text,pg_postmaster_start_time()::text,current_user::text,current_setting('search_path'),ARRAY['public.attachment_links'::regclass::oid::bigint,'public.attachments'::regclass::oid::bigint]").fetch_one(c).await?)
}
fn changed() -> Error {
    BusinessError::new(
        FailureKind::Conflict,
        "LEGAL_CITATION_FILES_CHANGED",
        "The cited Files source changed.",
    )
    .into()
}
