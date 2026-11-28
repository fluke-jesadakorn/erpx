//! sqlx execution for the native media-publication read/create slice.
//!
//! Every statement below is owner-owned and fully parameterized. The list and
//! source statements mirror the retired TypeScript repository SQL with `?`
//! markers replaced by `$n`; the four create statements are the same SR_022,
//! SR_023, SR_024 and SR_025 writes that the TS unit planned. No caller can
//! supply SQL, a predicate or a table name.

use erpx_platform_runtime_server::public::postgres::{
    DatabasePool, PgTransactionError, PgUnitOfWork, TransactionOptions,
};
use erpx_platform_shared_server::public::{BusinessError, FailureKind};
use sha2::{Digest, Sha256};
use sqlx::Row;

use super::application::{
    PreparedCreate, PublicationFileStore, PublicationFuture, PublicationStore, SourceLink,
};
use super::domain::{PublicationList, PublicationRow};

/// Approved-only list: an anonymous manager sees just approved public media.
pub const LIST_APPROVED_SQL: &str = "\
SELECT id,source_link_id,target_type,target_id,derivative_media_type,derivative_checksum_sha256,derivative_bytes,alt_text,ordinal,revision,state,created_by,created_at,approved_by,approved_at,revoked_by,revoked_at,revocation_reason \
FROM attachment_publications publication \
WHERE company_id=$1 AND target_type=$2 AND target_id=$3 \
AND EXISTS(SELECT 1 FROM attachment_links link JOIN attachment_sets set_row ON set_row.id=link.set_id WHERE link.id=publication.source_link_id AND set_row.company_id=publication.company_id) \
AND state='approved' ORDER BY ordinal,revision,id";

/// Publisher list: includes the caller's draft publications.
pub const LIST_MANAGE_SQL: &str = "\
SELECT id,source_link_id,target_type,target_id,derivative_media_type,derivative_checksum_sha256,derivative_bytes,alt_text,ordinal,revision,state,created_by,created_at,approved_by,approved_at,revoked_by,revoked_at,revocation_reason \
FROM attachment_publications publication \
WHERE company_id=$1 AND target_type=$2 AND target_id=$3 \
AND EXISTS(SELECT 1 FROM attachment_links link JOIN attachment_sets set_row ON set_row.id=link.set_id WHERE link.id=publication.source_link_id AND set_row.company_id=publication.company_id) \
ORDER BY ordinal,revision,id";

pub const ORDER_VERSION_SQL: &str =
    "SELECT version FROM attachment_publication_orders WHERE company_id=$1 AND target_type=$2 AND target_id=$3";

pub const SOURCE_SQL: &str = "\
SELECT l.id,l.attachment_id,COALESCE(a.preview_media_type,a.detected_media_type,a.media_type) media_type, \
s.target_type source_target_type,s.target_id source_target_id,s.purpose_key, \
COALESCE(a.preview_object_key,'') preview_object_key,COALESCE(a.source_state,'') source_state \
FROM attachment_links l JOIN attachment_sets s ON s.id=l.set_id \
JOIN attachments a ON a.id=l.attachment_id AND a.company_id=s.company_id \
WHERE l.id=$1 AND s.company_id=$2 AND l.state='bound'";

pub const BY_ID_SQL: &str = "SELECT * FROM attachment_publications WHERE id=$1 AND company_id=$2";
pub const NEXT_REVISION_SQL: &str = "SELECT COALESCE(max(revision),0)+1 FROM attachment_publications WHERE company_id=$1 AND target_type=$2 AND target_id=$3";
pub const LOCK_SQL: &str = "SELECT pg_advisory_xact_lock(hashtextextended($1,0))";

/// SR_022: one immutable draft publication row.
pub const INSERT_PUBLICATION_SQL: &str = "\
INSERT INTO attachment_publications(id,company_id,source_link_id,target_type,target_id,derivative_object_key,derivative_media_type,derivative_checksum_sha256,derivative_bytes,alt_text,ordinal,revision,state,created_by,created_at) \
VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,'draft',$13,$14)";

/// SR_023: the ordered-target row, created once per (company,target).
pub const INSERT_ORDER_SQL: &str = "\
INSERT INTO attachment_publication_orders(company_id,target_type,target_id,version,updated_by,updated_at) \
VALUES($1,$2,$3,1,$4,$5) ON CONFLICT(company_id,target_type,target_id) DO NOTHING";

/// SR_024: immutable public-byte ledger entry.
pub const INSERT_LEDGER_SQL: &str = "\
INSERT INTO attachment_storage_ledger(id,company_id,attachment_id,variant,object_key,bytes,state,created_at) \
VALUES($1,$2,$3,'publication',$4,$5,'stored',$6)";

/// SR_025: publication.created audit event.
pub const INSERT_EVENT_SQL: &str = "\
INSERT INTO attachment_events(id,company_id,attachment_id,link_id,actor_id,action,details_json,created_at) \
VALUES($1,$2,$3,$4,$5,'publication.created',$6,$7)";

fn unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "MEDIA_PUBLICATIONS_UNAVAILABLE",
        "The native media publication service is unavailable.",
    )
}

fn project(row: sqlx::postgres::PgRow, with_company: bool) -> Result<PublicationRow, BusinessError> {
    Ok(PublicationRow {
        id: row.try_get("id").map_err(|_| unavailable())?,
        company_id: if with_company {
            Some(row.try_get("company_id").map_err(|_| unavailable())?)
        } else {
            None
        },
        source_link_id: row.try_get("source_link_id").map_err(|_| unavailable())?,
        target_type: row.try_get("target_type").map_err(|_| unavailable())?,
        target_id: row.try_get("target_id").map_err(|_| unavailable())?,
        derivative_object_key: row
            .try_get("derivative_object_key")
            .map_err(|_| unavailable())?,
        derivative_media_type: row
            .try_get("derivative_media_type")
            .map_err(|_| unavailable())?,
        derivative_checksum_sha256: row
            .try_get("derivative_checksum_sha256")
            .map_err(|_| unavailable())?,
        derivative_bytes: row.try_get("derivative_bytes").map_err(|_| unavailable())?,
        alt_text: row.try_get("alt_text").map_err(|_| unavailable())?,
        ordinal: row.try_get("ordinal").map_err(|_| unavailable())?,
        revision: row.try_get("revision").map_err(|_| unavailable())?,
        state: row.try_get("state").map_err(|_| unavailable())?,
        created_by: row.try_get("created_by").map_err(|_| unavailable())?,
        created_at: row.try_get("created_at").map_err(|_| unavailable())?,
        approved_by: row.try_get("approved_by").map_err(|_| unavailable())?,
        approved_at: row.try_get("approved_at").map_err(|_| unavailable())?,
        revoked_by: row.try_get("revoked_by").map_err(|_| unavailable())?,
        revoked_at: row.try_get("revoked_at").map_err(|_| unavailable())?,
        revocation_reason: row.try_get("revocation_reason").map_err(|_| unavailable())?,
    })
}

/// Postgres-backed owner store. One unit of work per request; the create path
/// uses `run_once` because the object-store write is an external effect and
/// must never be replayed by the transaction's retry policy.
pub struct PgPublicationStore {
    pool: DatabasePool,
}

impl PgPublicationStore {
    pub fn new(pool: DatabasePool) -> Self {
        Self { pool }
    }
}

impl PublicationStore for PgPublicationStore {
    fn list(
        &self,
        company: &str,
        target_type: &str,
        target_id: &str,
        can_manage: bool,
    ) -> PublicationFuture<'_, PublicationList> {
        let unit = PgUnitOfWork::new(self.pool.clone());
        let company = company.to_owned();
        let target_type = target_type.to_owned();
        let target_id = target_id.to_owned();
        Box::pin(async move {
            unit.run::<_, BusinessError, _>(TransactionOptions::default(), move |connection| {
                let company = company.clone();
                let target_type = target_type.clone();
                let target_id = target_id.clone();
                Box::pin(async move {
                    let rows = sqlx::query(if can_manage { LIST_MANAGE_SQL } else { LIST_APPROVED_SQL })
                        .bind(company.as_str())
                        .bind(target_type.as_str())
                        .bind(target_id.as_str())
                        .fetch_all(&mut *connection)
                        .await
                        .map_err(|_| unavailable())?;
                    let items = rows
                        .into_iter()
                        .map(|row| project(row, false))
                        .collect::<Result<Vec<_>, _>>()?;
                    let order_version: Option<i64> = sqlx::query_scalar(ORDER_VERSION_SQL)
                        .bind(company.as_str())
                        .bind(target_type.as_str())
                        .bind(target_id.as_str())
                        .fetch_optional(&mut *connection)
                        .await
                        .map_err(|_| unavailable())?;
                    Ok(PublicationList {
                        target_type,
                        target_id,
                        order_version: order_version.unwrap_or(1),
                        items,
                    })
                })
            })
            .await
            .map_err(transaction_error)
        })
    }

    fn source(&self, company: &str, link_id: &str) -> PublicationFuture<'_, Option<SourceLink>> {
        let unit = PgUnitOfWork::new(self.pool.clone());
        let company = company.to_owned();
        let link_id = link_id.to_owned();
        Box::pin(async move {
            unit.run::<_, BusinessError, _>(TransactionOptions::default(), move |connection| {
                let company = company.clone();
                let link_id = link_id.clone();
                Box::pin(async move {
                    let row = sqlx::query(SOURCE_SQL)
                        .bind(link_id.as_str())
                        .bind(company.as_str())
                        .fetch_optional(&mut *connection)
                        .await
                        .map_err(|_| unavailable())?;
                    row.map(|row| {
                        Ok(SourceLink {
                            id: row.try_get("id").map_err(|_| unavailable())?,
                            attachment_id: row.try_get("attachment_id").map_err(|_| unavailable())?,
                            preview_object_key: row
                                .try_get("preview_object_key")
                                .map_err(|_| unavailable())?,
                            media_type: row.try_get("media_type").map_err(|_| unavailable())?,
                            target_type: row
                                .try_get("source_target_type")
                                .map_err(|_| unavailable())?,
                            target_id: row.try_get("source_target_id").map_err(|_| unavailable())?,
                            purpose_key: row.try_get("purpose_key").map_err(|_| unavailable())?,
                            source_state: row.try_get("source_state").map_err(|_| unavailable())?,
                        })
                    })
                    .transpose()
                    .map_err(PgTransactionError::Business)
                })
            })
            .await
            .map_err(transaction_error)
        })
    }

    fn publication(&self, company: &str, id: &str) -> PublicationFuture<'_, Option<PublicationRow>> {
        let unit = PgUnitOfWork::new(self.pool.clone());
        let company = company.to_owned();
        let id = id.to_owned();
        Box::pin(async move {
            unit.run::<_, BusinessError, _>(TransactionOptions::default(), move |connection| {
                let company = company.clone();
                let id = id.clone();
                Box::pin(async move {
                    let row = sqlx::query(BY_ID_SQL)
                        .bind(id.as_str())
                        .bind(company.as_str())
                        .fetch_optional(&mut *connection)
                        .await
                        .map_err(|_| unavailable())?;
                    row.map(|row| project(row, true))
                        .transpose()
                        .map_err(PgTransactionError::Business)
                })
            })
            .await
            .map_err(transaction_error)
        })
    }

    fn create(
        &self,
        files: std::sync::Arc<dyn PublicationFileStore>,
        request: &PreparedCreate,
    ) -> PublicationFuture<'_, PublicationRow> {
        let unit = PgUnitOfWork::new(self.pool.clone());
        let request = request.clone();
        Box::pin(async move {
            unit.run_once::<_, BusinessError, _>(TransactionOptions::default(), move |connection| {
                let request = request.clone();
                Box::pin(async move {
                    let outcome = async move {
                    let lock = format!(
                        "attachment-publication:{}:{}:{}",
                        request.company, request.input.target_type, request.input.target_id
                    );
                    sqlx::query(LOCK_SQL)
                        .bind(lock.as_str())
                        .execute(&mut *connection)
                        .await
                        .map_err(|_| unavailable())?;
                    if let Some(row) = sqlx::query(BY_ID_SQL)
                        .bind(request.id.as_str())
                        .bind(request.company.as_str())
                        .fetch_optional(&mut *connection)
                        .await
                        .map_err(|_| unavailable())?
                    {
                        return project(row, true);
                    }
                    let revision: i64 = sqlx::query_scalar(NEXT_REVISION_SQL)
                        .bind(request.company.as_str())
                        .bind(request.input.target_type.as_str())
                        .bind(request.input.target_id.as_str())
                        .fetch_one(&mut *connection)
                        .await
                        .map_err(|_| unavailable())?;
                    let object_key = format!(
                        "{}/published/{}/r{}",
                        request.holding_id, request.id, revision
                    );
                    let object = files
                        .get(&request.source.preview_object_key)
                        .await?
                        .filter(|object| {
                            matches!(object.media_type.as_str(), "image/jpeg" | "image/webp")
                                && !object.bytes.is_empty()
                                && object.bytes.len() <= 2 * 1024 * 1024
                        })
                        .ok_or_else(|| {
                            BusinessError::new(
                                FailureKind::NotFound,
                                "PUBLICATION_SOURCE_NOT_FOUND",
                                "Choose an available image with a sanitized preview.",
                            )
                        })?;
                    let checksum = hex::encode(Sha256::digest(&object.bytes));
                    files
                        .put_once(&object_key, &object.bytes, &object.media_type, &checksum)
                        .await?;
                    let bytes = object.bytes.len() as i64;
                    let details = serde_json::json!({
                        "publicationId": request.id,
                        "targetType": request.input.target_type,
                        "targetId": request.input.target_id,
                        "revision": revision,
                    })
                    .to_string();
                    sqlx::query(INSERT_PUBLICATION_SQL)
                        .bind(request.id.as_str())
                        .bind(request.company.as_str())
                        .bind(request.source.id.as_str())
                        .bind(request.input.target_type.as_str())
                        .bind(request.input.target_id.as_str())
                        .bind(object_key.as_str())
                        .bind(object.media_type.as_str())
                        .bind(checksum.as_str())
                        .bind(bytes)
                        .bind(request.input.alt_text.as_str())
                        .bind(request.input.ordinal)
                        .bind(revision)
                        .bind(request.user.as_str())
                        .bind(request.now)
                        .execute(&mut *connection)
                        .await
                        .map_err(|_| unavailable())?;
                    sqlx::query(INSERT_ORDER_SQL)
                        .bind(request.company.as_str())
                        .bind(request.input.target_type.as_str())
                        .bind(request.input.target_id.as_str())
                        .bind(request.user.as_str())
                        .bind(request.now)
                        .execute(&mut *connection)
                        .await
                        .map_err(|_| unavailable())?;
                    sqlx::query(INSERT_LEDGER_SQL)
                        .bind(format!("attachment-storage-{}", uuid::Uuid::new_v4()))
                        .bind(request.company.as_str())
                        .bind(request.source.attachment_id.as_str())
                        .bind(object_key.as_str())
                        .bind(bytes)
                        .bind(request.now)
                        .execute(&mut *connection)
                        .await
                        .map_err(|_| unavailable())?;
                    sqlx::query(INSERT_EVENT_SQL)
                        .bind(format!("attachment-event-{}", uuid::Uuid::new_v4()))
                        .bind(request.company.as_str())
                        .bind(request.source.attachment_id.as_str())
                        .bind(request.source.id.as_str())
                        .bind(request.user.as_str())
                        .bind(details.as_str())
                        .bind(request.now)
                        .execute(&mut *connection)
                        .await
                        .map_err(|_| unavailable())?;
                    let row = sqlx::query(BY_ID_SQL)
                        .bind(request.id.as_str())
                        .bind(request.company.as_str())
                        .fetch_one(&mut *connection)
                        .await
                        .map_err(|_| unavailable())?;
                    project(row, true)
                    };
                    Ok::<_, PgTransactionError<BusinessError>>(outcome.await?)
                })
            })
            .await
            .map_err(transaction_error)
        })
    }
}

fn transaction_error(error: PgTransactionError<BusinessError>) -> BusinessError {
    match error {
        PgTransactionError::Business(error) => error,
        PgTransactionError::Persistence(_) => unavailable(),
    }
}
