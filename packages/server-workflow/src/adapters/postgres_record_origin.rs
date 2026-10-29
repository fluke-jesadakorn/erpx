//! Positive durable record provenance, never current authority or replay admission.
use crate::domain::command_json::{CommandJson, LIMIT, OrderedJson};
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::{BusinessError, CompanyId, FailureKind, UserId};
use sqlx::{Connection, PgConnection, Row};

type Error = PgTransactionError<BusinessError>;

/// Supplied by a closed owner source inspector, not a browser request. Matching
/// this historical identity grants no present permission, purchase, or release.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RecordOriginRequest {
    pub command_key: String,
    pub operation: String,
    pub record_id: String,
    pub creator_user_id: UserId,
    pub company_id: CompanyId,
}

#[derive(Debug, Eq, PartialEq)]
struct ReceiptImage {
    operation: String,
    actor_id: String,
    record_id: String,
    request_hash: String,
    authority_json: String,
    result_json: String,
    created_at: i64,
}

/// A retained positive source in one real PostgreSQL transaction. Private
/// construction and no Clone/Deserialize prevent a caller fabricating a proof.
/// Owners still check their exact result shape and retain current authority.
pub struct PgRecordOrigin {
    request: RecordOriginRequest,
    image: ReceiptImage,
    result: CommandJson,
    backend_pid: i32,
    transaction_id: String,
    database_identity: String,
    read_limit: Option<usize>,
}
impl PgRecordOrigin {
    pub fn result(&self) -> &CommandJson {
        &self.result
    }

    /// Account for all retained owned text without exposing receipt authority.
    /// Chain readers charge a shared budget before retaining the next proof;
    /// each individual read remains independently SQL-bounded.
    pub fn retained_text_bytes(&self) -> usize {
        [
            self.request.command_key.as_str(),
            self.request.operation.as_str(),
            self.request.record_id.as_str(),
            self.request.creator_user_id.as_str(),
            self.request.company_id.as_str(),
            self.image.operation.as_str(),
            self.image.actor_id.as_str(),
            self.image.record_id.as_str(),
            self.image.request_hash.as_str(),
            self.image.authority_json.as_str(),
            self.image.result_json.as_str(),
            self.result.as_str(),
            self.transaction_id.as_str(),
            self.database_identity.as_str(),
        ]
        .into_iter()
        .fold(0_usize, |total, text| total.saturating_add(text.len()))
    }
}

/// Utility command only: call with all source preparation before the first
/// SERIALIZABLE snapshot. ROW SHARE preserves normal receipt writers. A positive
/// row is separately held FOR SHARE; a missing row can never prove provenance.
pub async fn lock_record_origin_sources(connection: &mut PgConnection) -> Result<(), Error> {
    if !connection.is_in_transaction() {
        return Err(PersistenceError::new("WORKFLOW_OWNER_TRANSACTION_REQUIRED").into());
    }
    sqlx::query("LOCK TABLE command_receipts IN ROW SHARE MODE")
        .execute(connection)
        .await?;
    Ok(())
}

pub async fn read_record_origin(
    connection: &mut PgConnection,
    request: RecordOriginRequest,
) -> Result<Option<PgRecordOrigin>, Error> {
    read_record_origin_impl(connection, request, None).await
}
/// Restrict the complete retained receipt text before PostgreSQL transfers it.
pub async fn read_record_origin_with_limit(
    connection: &mut PgConnection,
    request: RecordOriginRequest,
    limit_bytes: usize,
) -> Result<Option<PgRecordOrigin>, Error> {
    let limit_bytes = limit_bytes.min(LIMIT);
    if limit_bytes == 0 {
        return Err(BusinessError::new(
            FailureKind::Unavailable,
            "COMMAND_RECEIPT_TOO_LARGE",
            "The retained command exceeds the bounded receipt reader.",
        )
        .into());
    }
    read_record_origin_impl(connection, request, Some(limit_bytes)).await
}
async fn read_record_origin_impl(
    connection: &mut PgConnection,
    request: RecordOriginRequest,
    read_limit: Option<usize>,
) -> Result<Option<PgRecordOrigin>, Error> {
    validate(&request)?;
    require_unit(connection).await?;
    let Some((image, backend_pid, transaction_id, database_identity)) =
        read(connection, &request.command_key, read_limit).await?
    else {
        return Ok(None);
    };
    if image.operation != request.operation
        || image.actor_id != request.creator_user_id.as_str()
        || image.record_id != request.record_id
    {
        return Ok(None);
    }
    if image.request_hash.len() != 64
        || !image
            .request_hash
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        return Err(invalid_origin().into());
    }
    let authority = OrderedJson::parse(&image.authority_json).map_err(|_| invalid_origin())?;
    if authority.get("companyId").and_then(OrderedJson::string) != Some(request.company_id.as_str())
    {
        return Ok(None);
    }
    let result = CommandJson::parse(&image.result_json).map_err(|_| invalid_origin())?;
    Ok(Some(PgRecordOrigin {
        request,
        image,
        result,
        backend_pid,
        transaction_id,
        database_identity,
        read_limit,
    }))
}

/// Retain the original bytes, including metadata not exposed to consumers, on
/// the same backend AND transaction. An identical receipt in another unit does
/// not revive an expired proof. Same-unit source changes fail closed as well.
pub async fn verify_record_origin(
    connection: &mut PgConnection,
    proof: &PgRecordOrigin,
) -> Result<(), Error> {
    require_unit(connection).await?;
    let Some((image, pid, xid, database_identity)) =
        read(connection, &proof.request.command_key, proof.read_limit).await?
    else {
        return Err(changed().into());
    };
    if pid != proof.backend_pid
        || xid != proof.transaction_id
        || database_identity != proof.database_identity
    {
        return Err(BusinessError::new(
            FailureKind::Conflict,
            "COMMAND_ORIGIN_UNIT_CHANGED",
            "Retain record provenance in its original transaction.",
        )
        .into());
    }
    if image != proof.image {
        return Err(changed().into());
    }
    Ok(())
}

async fn require_unit(connection: &mut PgConnection) -> Result<(), Error> {
    if !connection.is_in_transaction() {
        return Err(PersistenceError::new("WORKFLOW_OWNER_TRANSACTION_REQUIRED").into());
    }
    let isolation: String = sqlx::query_scalar("SHOW transaction_isolation")
        .fetch_one(connection)
        .await?;
    if isolation != "serializable" {
        return Err(PersistenceError::new("WORKFLOW_SERIALIZABLE_REQUIRED").into());
    }
    Ok(())
}

async fn read(
    connection: &mut PgConnection,
    key: &str,
    read_limit: Option<usize>,
) -> Result<Option<(ReceiptImage, i32, String, String)>, Error> {
    // CASE bounds before transfer, not after allocating arbitrary legacy TEXT.
    let row = sqlx::query("SELECT
      CASE WHEN bounded AND octet_length(operation)<=200 THEN operation END operation,
      CASE WHEN bounded AND octet_length(actor_id)<=200 THEN actor_id END actor_id,
      CASE WHEN bounded AND octet_length(record_id)<=200 THEN record_id END record_id,
      CASE WHEN bounded AND octet_length(request_hash)<=64 THEN request_hash END request_hash,
      CASE WHEN bounded AND octet_length(authority_json)<=$2 THEN authority_json END authority_json,
      CASE WHEN bounded AND octet_length(result_json)<=$2 THEN result_json END result_json,
      (COALESCE(octet_length(operation),0)>200 OR COALESCE(octet_length(actor_id),0)>200
       OR COALESCE(octet_length(record_id),0)>200 OR COALESCE(octet_length(request_hash),0)>64
       OR COALESCE(octet_length(authority_json),0)>$2 OR COALESCE(octet_length(result_json),0)>$2 OR NOT bounded) oversized,
      created_at, pg_backend_pid() backend_pid, pg_current_xact_id()::text transaction_id,
      current_database()||'/'||pg_postmaster_start_time()::text database_identity
      FROM command_receipts r CROSS JOIN LATERAL (SELECT $3::bigint IS NULL OR (COALESCE(octet_length(operation),0)::bigint+COALESCE(octet_length(actor_id),0)+COALESCE(octet_length(record_id),0)+COALESCE(octet_length(request_hash),0)+COALESCE(octet_length(authority_json),0)+2::bigint*COALESCE(octet_length(result_json),0))<=$3 AS bounded) b WHERE command_id=$1 FOR SHARE OF r")
        .bind(key).bind(LIMIT as i32).bind(read_limit.map(|v| i64::try_from(v).unwrap_or(i64::MAX))).fetch_optional(connection).await?;
    let Some(row) = row else {
        return Ok(None);
    };
    if row.try_get::<bool, _>("oversized")? {
        return Err(BusinessError::new(
            FailureKind::Unavailable,
            "COMMAND_RECEIPT_TOO_LARGE",
            "The retained command exceeds the bounded receipt reader.",
        )
        .into());
    }
    let authority_json: Option<String> = row.try_get("authority_json")?;
    let Some(authority_json) = authority_json else {
        return Err(invalid_origin().into());
    };
    Ok(Some((
        ReceiptImage {
            operation: row.try_get("operation")?,
            actor_id: row.try_get("actor_id")?,
            record_id: row.try_get("record_id")?,
            request_hash: row.try_get("request_hash")?,
            authority_json,
            result_json: row.try_get("result_json")?,
            created_at: row.try_get("created_at")?,
        },
        row.try_get("backend_pid")?,
        row.try_get("transaction_id")?,
        row.try_get("database_identity")?,
    )))
}

fn validate(request: &RecordOriginRequest) -> Result<(), BusinessError> {
    if !(8..=180).contains(&request.command_key.len())
        || !request
            .command_key
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b':' | b'_' | b'.' | b'-'))
        || request.operation.is_empty()
        || request.operation.len() > 200
        || request.operation.chars().any(char::is_control)
        || request.record_id.is_empty()
        || request.record_id.len() > 200
        || request.record_id.chars().any(char::is_control)
    {
        return Err(BusinessError::invalid(
            "COMMAND_ORIGIN_REQUEST_INVALID",
            "Use a bounded owner record origin identity.",
        ));
    }
    Ok(())
}
fn invalid_origin() -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        "COMMAND_ORIGIN_INVALID",
        "The retained command does not establish bounded record provenance.",
    )
}
fn changed() -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        "COMMAND_ORIGIN_CHANGED",
        "Record provenance changed during the command.",
    )
}

/// Historical owner data only. It neither constructs an Actor nor grants access.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RawRecordOriginRequest {
    pub command_key: String,
    pub operation: String,
    pub record_id: String,
    pub creator_user_id: String,
    pub company_id: CompanyId,
}
/// Sealed full-row digest retained in one actual serializable unit.
pub struct PgRawRecordOrigin {
    request: RawRecordOriginRequest,
    image: String,
    backend_pid: i32,
    transaction_id: String,
    database_identity: String,
}
impl PgRawRecordOrigin {
    /// Fingerprint of the complete receipt row observed in this transaction.
    pub fn receipt_fingerprint(&self) -> &str {
        &self.image
    }
}
pub async fn read_raw_record_origin(
    c: &mut PgConnection,
    request: RawRecordOriginRequest,
) -> Result<Option<PgRawRecordOrigin>, Error> {
    require_unit(c).await?;
    let row: Option<(String,i32,String,String)> = sqlx::query_as("SELECT encode(sha256(convert_to(to_jsonb(r)::text,'UTF8')),'hex'),pg_backend_pid(),pg_current_xact_id()::text,current_database()||'/'||pg_postmaster_start_time()::text FROM command_receipts r WHERE r.command_id=$1 AND r.operation=$2 AND r.record_id=$3 AND r.actor_id=$4 AND r.authority_json::jsonb->>'companyId'=$5 FOR SHARE OF r")
        .bind(&request.command_key).bind(&request.operation).bind(&request.record_id).bind(&request.creator_user_id).bind(request.company_id.as_str()).fetch_optional(c).await?;
    Ok(row.map(
        |(image, backend_pid, transaction_id, database_identity)| PgRawRecordOrigin {
            request,
            image,
            backend_pid,
            transaction_id,
            database_identity,
        },
    ))
}
/// Read the actual bounded result and sealed full receipt row together. This
/// carries historical evidence only and never constructs current authority.
pub async fn read_raw_record_origin_result_with_limit(
    c: &mut PgConnection,
    request: RawRecordOriginRequest,
    limit_bytes: usize,
) -> Result<Option<(PgRawRecordOrigin, CommandJson)>, Error> {
    require_unit(c).await?;
    let overhead = request
        .command_key
        .len()
        .saturating_add(request.operation.len())
        .saturating_add(request.record_id.len())
        .saturating_add(request.creator_user_id.len())
        .saturating_add(request.company_id.as_str().len())
        .saturating_add(1024);
    let limit_bytes = (limit_bytes.saturating_sub(overhead) / 2).min(LIMIT);
    if limit_bytes == 0 {
        return Err(BusinessError::new(
            FailureKind::Unavailable,
            "COMMAND_RECEIPT_TOO_LARGE",
            "The retained command exceeds the bounded receipt reader.",
        )
        .into());
    }
    let row:Option<(String,Option<String>,bool,i32,String,String)>=sqlx::query_as("SELECT encode(sha256(convert_to(to_jsonb(r)::text,'UTF8')),'hex'),CASE WHEN octet_length(result_json)<=$6 THEN result_json END,COALESCE(octet_length(result_json),0)>$6,pg_backend_pid(),pg_current_xact_id()::text,current_database()||'/'||pg_postmaster_start_time()::text FROM command_receipts r WHERE r.command_id=$1 AND r.operation=$2 AND r.record_id=$3 AND r.actor_id=$4 AND r.authority_json::jsonb->>'companyId'=$5 AND r.request_hash~'^[0-9a-f]{64}$' FOR SHARE OF r")
        .bind(&request.command_key).bind(&request.operation).bind(&request.record_id).bind(&request.creator_user_id).bind(request.company_id.as_str()).bind(i64::try_from(limit_bytes).unwrap_or(i64::MAX)).fetch_optional(c).await?;
    let Some((image, result, oversized, backend_pid, transaction_id, database_identity)) = row
    else {
        return Ok(None);
    };
    if oversized {
        return Err(BusinessError::new(
            FailureKind::Unavailable,
            "COMMAND_RECEIPT_TOO_LARGE",
            "The retained command exceeds the bounded receipt reader.",
        )
        .into());
    }
    let result =
        CommandJson::parse(&result.ok_or_else(invalid_origin)?).map_err(|_| invalid_origin())?;
    Ok(Some((
        PgRawRecordOrigin {
            request,
            image,
            backend_pid,
            transaction_id,
            database_identity,
        },
        result,
    )))
}
pub async fn verify_raw_record_origin(
    c: &mut PgConnection,
    proof: &PgRawRecordOrigin,
) -> Result<(), Error> {
    require_unit(c).await?;
    let row: Option<(String,i32,String,String)> = sqlx::query_as("SELECT encode(sha256(convert_to(to_jsonb(r)::text,'UTF8')),'hex'),pg_backend_pid(),pg_current_xact_id()::text,current_database()||'/'||pg_postmaster_start_time()::text FROM command_receipts r WHERE r.command_id=$1 FOR SHARE OF r").bind(&proof.request.command_key).fetch_optional(c).await?;
    let Some((image, pid, xid, database)) = row else {
        return Err(changed().into());
    };
    if pid != proof.backend_pid
        || xid != proof.transaction_id
        || database != proof.database_identity
    {
        return Err(BusinessError::new(
            FailureKind::Conflict,
            "COMMAND_ORIGIN_UNIT_CHANGED",
            "Retain record provenance in its original transaction.",
        )
        .into());
    }
    if image != proof.image {
        return Err(changed().into());
    }
    Ok(())
}
