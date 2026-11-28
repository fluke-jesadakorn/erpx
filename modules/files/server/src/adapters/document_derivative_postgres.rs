//! PostgreSQL adapter for the bounded native document-page derivative command.
//!
//! Only Files-owned relations are read and written (`attachments`,
//! `attachment_derivatives`, `attachment_documents`,
//! `attachment_storage_ledger`, `command_receipts`). The owning expense
//! authority is a composition port; this adapter never grants preparation and
//! never opens, commits or retries a transaction.
use crate::application::document_derivative::{
    DocumentDerivativeByteStore, DocumentDerivativePorts, SaveDocumentPage,
};
use crate::domain::document_derivative::{
    DERIVATIVE_OPERATION, DerivativeAuthority, DocumentDerivativeResult, DocumentPageSource,
};
use erpx_platform_runtime_server::public::postgres::{PersistenceError, PgTransactionError, PgWorkFuture};
use erpx_platform_shared_server::public::{BusinessError, FailureKind};
use erpx_workflow_server::public::commands::CommandActor;
use sha2::{Digest, Sha256};
use sqlx::{Connection, PgConnection};

type Error = PgTransactionError<BusinessError>;

const ATTACHMENT_SQL: &str = r#"
SELECT owner_type,owner_id,checksum_sha256,media_type
FROM attachments
WHERE id=$1 AND company_id=$2 AND revoked_at IS NULL AND source_state='available'
"#;

const RECEIPT_SQL: &str = r#"
SELECT operation,request_hash,result_json FROM command_receipts WHERE command_id=$1
"#;

const EXISTING_SQL: &str = r#"
SELECT id,page_number FROM attachment_derivatives
WHERE attachment_id=$1 AND page_number=$2 AND original_checksum=$3
"#;

const RECEIPT_INSERT_SQL: &str = r#"
INSERT INTO command_receipts(
  command_id,operation,actor_id,request_hash,record_id,result_json,created_at,authority_json
) VALUES($1,$2,$3,$4,$5,$6,$7,$8)
ON CONFLICT(command_id) DO UPDATE SET command_id=excluded.command_id
RETURNING request_hash
"#;

const DOCUMENT_SQL: &str = r#"
INSERT INTO attachment_documents(attachment_id,company_id,original_checksum,page_count,created_at)
VALUES($1,$2,$3,$4,$5) ON CONFLICT(attachment_id) DO NOTHING
"#;

const DERIVATIVE_SQL: &str = r#"
INSERT INTO attachment_derivatives(
  id,attachment_id,company_id,page_number,object_key,checksum_sha256,
  original_checksum,bytes,created_by,created_at
) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10) ON CONFLICT(id) DO NOTHING
"#;

const STORAGE_SQL: &str = r#"
INSERT INTO attachment_storage_ledger(
  id,company_id,attachment_id,variant,object_key,bytes,state,created_at
) VALUES($1,$2,$3,'derivative',$4,$5,'stored',$6) ON CONFLICT(id) DO NOTHING
"#;

fn unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "DOCUMENT_DERIVATIVE_UNAVAILABLE",
        "The native document writer is unavailable.",
    )
}

fn conflict() -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        "IDEMPOTENCY_CONFLICT",
        "This command key belongs to another request.",
    )
}

async fn require_serializable(c: &mut PgConnection) -> Result<(), Error> {
    if !c.is_in_transaction() {
        return Err(PersistenceError::new("FILES_TRANSACTION_REQUIRED").into());
    }
    let isolation: String = sqlx::query_scalar("SHOW transaction_isolation")
        .fetch_one(&mut *c)
        .await?;
    if isolation != "serializable" {
        return Err(PersistenceError::new("FILES_SERIALIZABLE_REQUIRED").into());
    }
    Ok(())
}

/// Composition supplies the current owning expense authority. It receives only
/// Files-owned owner facts and returns no request-derived grant.
pub trait ExpenseDerivativeAuthority: Send + Sync {
    fn prepare<'a>(
        &'a self,
        c: &'a mut PgConnection,
        company_id: &'a str,
        owner_id: &'a str,
        payment_id: Option<&'a str>,
        actor_id: &'a str,
    ) -> PgWorkFuture<'a, DerivativeAuthority, BusinessError>;
}

pub struct PgDocumentDerivativePorts<A> {
    authority: A,
    read_only: bool,
    uploads_frozen: bool,
}

impl<A: ExpenseDerivativeAuthority> PgDocumentDerivativePorts<A> {
    pub fn new(authority: A, read_only: bool, uploads_frozen: bool) -> Self {
        Self {
            authority,
            read_only,
            uploads_frozen,
        }
    }
}

impl<A: ExpenseDerivativeAuthority> DocumentDerivativePorts<PgConnection>
    for PgDocumentDerivativePorts<A>
{
    fn read_only(&self) -> bool {
        self.read_only
    }

    fn uploads_frozen(&self) -> bool {
        self.uploads_frozen
    }

    fn attachment<'a>(
        &'a self,
        c: &'a mut PgConnection,
        company_id: &'a str,
        attachment_id: &'a str,
    ) -> PgWorkFuture<'a, Option<DocumentPageSource>, BusinessError> {
        Box::pin(async move {
            let row: Option<(String, String, String, String)> =
                sqlx::query_as(ATTACHMENT_SQL)
                    .bind(attachment_id)
                    .bind(company_id)
                    .fetch_optional(&mut *c)
                    .await
                    .map_err(|_| PgTransactionError::Business(unavailable()))?;
            Ok(row.map(
                |(owner_type, owner_id, checksum_sha256, media_type)| DocumentPageSource {
                    owner_type,
                    owner_id,
                    checksum_sha256,
                    media_type,
                },
            ))
        })
    }

    fn authority<'a>(
        &'a self,
        c: &'a mut PgConnection,
        company_id: &'a str,
        owner_id: &'a str,
        payment_id: Option<&'a str>,
        actor_id: &'a str,
    ) -> PgWorkFuture<'a, DerivativeAuthority, BusinessError> {
        self.authority
            .prepare(c, company_id, owner_id, payment_id, actor_id)
    }

    fn replay<'a>(
        &'a self,
        c: &'a mut PgConnection,
        key: &'a str,
        fingerprint: &'a str,
    ) -> PgWorkFuture<'a, Option<DocumentDerivativeResult>, BusinessError> {
        Box::pin(async move {
            let row: Option<(String, String, Option<String>)> = sqlx::query_as(RECEIPT_SQL)
                .bind(key)
                .fetch_optional(&mut *c)
                .await
                .map_err(|_| PgTransactionError::Business(unavailable()))?;
            let Some((operation, request_hash, result_json)) = row else {
                return Ok(None);
            };
            if operation != DERIVATIVE_OPERATION || request_hash != fingerprint {
                return Err(PgTransactionError::Business(conflict()));
            }
            let result_json = result_json.ok_or_else(|| PgTransactionError::Business(unavailable()))?;
            let value: serde_json::Value =
                serde_json::from_str(&result_json).map_err(|_| PgTransactionError::Business(unavailable()))?;
            let id = value
                .get("id")
                .and_then(serde_json::Value::as_str)
                .filter(|value| !value.is_empty())
                .ok_or_else(|| PgTransactionError::Business(unavailable()))?;
            let page = value
                .get("page")
                .and_then(serde_json::Value::as_u64)
                .ok_or_else(|| PgTransactionError::Business(unavailable()))?;
            Ok(Some(DocumentDerivativeResult {
                id: id.to_owned(),
                page: page as u32,
            }))
        })
    }

    fn existing<'a>(
        &'a self,
        c: &'a mut PgConnection,
        attachment_id: &'a str,
        page: u32,
        original_checksum: &'a str,
    ) -> PgWorkFuture<'a, Option<DocumentDerivativeResult>, BusinessError> {
        Box::pin(async move {
            let row: Option<(String, i64)> = sqlx::query_as(EXISTING_SQL)
                .bind(attachment_id)
                .bind(page as i64)
                .bind(original_checksum)
                .fetch_optional(&mut *c)
                .await
                .map_err(|_| PgTransactionError::Business(unavailable()))?;
            Ok(row.map(|(id, page)| DocumentDerivativeResult {
                id,
                page: page as u32,
            }))
        })
    }

    fn commit<'a, 'b>(
        &'a self,
        c: &'a mut PgConnection,
        actor: &'a CommandActor,
        record_id: &'a str,
        version: i64,
        fingerprint: &'a str,
        result: &'a DocumentDerivativeResult,
        object_key: &'a str,
        command: &'a SaveDocumentPage<'b>,
        now: i64,
    ) -> PgWorkFuture<'a, (), BusinessError> {
        Box::pin(async move {
            require_serializable(c).await?;
            let authority_json = actor.receipt_authority().map_err(|_| PgTransactionError::Business(unavailable()))?;
            let result_json = serde_json::to_string(result).map_err(|_| PgTransactionError::Business(unavailable()))?;
            let stored: String = sqlx::query_scalar(RECEIPT_INSERT_SQL)
                .bind(command.key)
                .bind(DERIVATIVE_OPERATION)
                .bind(actor.user_id.as_str())
                .bind(fingerprint)
                .bind(record_id)
                .bind(&result_json)
                .bind(now)
                .bind(authority_json.as_str())
                .fetch_one(&mut *c)
                .await
                .map_err(|_| PgTransactionError::Business(unavailable()))?;
            if stored != fingerprint {
                return Err(conflict().into());
            }
            // The source claim version must be a real current revision; a zero
            // or negative value is an unfenced authority and fails closed.
            if version < 1 {
                return Err(BusinessError::new(
                    FailureKind::Forbidden,
                    "DOCUMENT_PREPARATION_FORBIDDEN",
                    "Only the person preparing or reviewing the current stage can prepare document pages.",
                )
                .into());
            }
            let page_count_ok: bool = sqlx::query_scalar(
                "SELECT NOT EXISTS(SELECT 1 FROM attachment_documents \
                 WHERE attachment_id=$1 AND (original_checksum!=$2 OR page_count!=$3))",
            )
            .bind(command.attachment_id)
            .bind(command.original_checksum)
            .bind(command.page_count as i64)
            .fetch_one(&mut *c)
            .await
            .map_err(|_| PgTransactionError::Business(unavailable()))?;
            if !page_count_ok {
                return Err(BusinessError::new(
                    FailureKind::Conflict,
                    "DOCUMENT_PAGE_COUNT_CHANGED",
                    "The document page count changed. Reload before preparing pages.",
                )
                .into());
            }
            let checksum = hex::encode(Sha256::digest(command.bytes));
            let bytes = command.bytes.len() as i64;
            sqlx::query(DOCUMENT_SQL)
                .bind(command.attachment_id)
                .bind(actor.company_id.as_str())
                .bind(command.original_checksum)
                .bind(command.page_count as i64)
                .bind(now)
                .execute(&mut *c)
                .await
                .map_err(|_| PgTransactionError::Business(unavailable()))?;
            sqlx::query(DERIVATIVE_SQL)
                .bind(&result.id)
                .bind(command.attachment_id)
                .bind(actor.company_id.as_str())
                .bind(command.page as i64)
                .bind(object_key)
                .bind(&checksum)
                .bind(command.original_checksum)
                .bind(bytes)
                .bind(actor.user_id.as_str())
                .bind(now)
                .execute(&mut *c)
                .await
                .map_err(|_| PgTransactionError::Business(unavailable()))?;
            sqlx::query(STORAGE_SQL)
                .bind(format!("attachment-storage-derivative-{}", result.id))
                .bind(actor.company_id.as_str())
                .bind(command.attachment_id)
                .bind(object_key)
                .bind(bytes)
                .bind(now)
                .execute(&mut *c)
                .await
                .map_err(|_| PgTransactionError::Business(unavailable()))?;
            Ok(())
        })
    }
}
