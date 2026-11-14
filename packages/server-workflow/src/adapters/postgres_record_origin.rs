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
/// SERIALIZABLE snapshot. ROW SHARE preserves normal receipt writers and does
/// not take a global table lock. A positive row is separately held FOR SHARE.
/// Approved customer-return lookup must reconstruct its candidate command key
/// from joined evidence and then call the exact-key retained origin below; it
/// must not scan command_receipts for a unique matching row, because a
/// concurrent matching insert can be invisible to this snapshot under ROW SHARE.
/// A missing row can never prove provenance.
pub async fn lock_record_origin_sources(connection: &mut PgConnection) -> Result<(), Error> {
    if !connection.is_in_transaction() {
        return Err(PersistenceError::new("WORKFLOW_OWNER_TRANSACTION_REQUIRED").into());
    }
    sqlx::query("LOCK TABLE command_receipts IN ROW SHARE MODE")
        .execute(connection)
        .await?;
    Ok(())
}

/// Require the existing record-origin ROW SHARE utility fence without taking
/// or upgrading a lock. This checks current lock coverage only: it does not
/// prove acquisition before the first snapshot, protect absent receipts from
/// inserts, or grant source authority.
pub async fn require_record_origin_sources_held(connection: &mut PgConnection) -> Result<(), Error> {
    if !connection.is_in_transaction() {
        return Err(PersistenceError::new("WORKFLOW_OWNER_TRANSACTION_REQUIRED").into());
    }
    let held: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM pg_locks WHERE pid=pg_backend_pid() AND locktype='relation' AND relation='command_receipts'::regclass AND granted AND mode IN ('RowShareLock','RowExclusiveLock','ShareUpdateExclusiveLock','ShareLock','ShareRowExclusiveLock','ExclusiveLock','AccessExclusiveLock'))")
        .fetch_one(connection).await?;
    if !held { return Err(PersistenceError::new("WORKFLOW_ORIGIN_LOCK_REQUIRED").into()); }
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
fn valid_request_hash(hash: &str) -> bool {
    hash.len() == 64
        && hash
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
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
    // Exact-key provenance only. A corrupt or legacy receipt with an invalid
    // request hash is an explicit denial, not an absent or positive proof.
    let row: Option<(String,String,i32,String,String)> = sqlx::query_as("SELECT encode(sha256(convert_to(to_jsonb(r)::text,'UTF8')),'hex'),r.request_hash,pg_backend_pid(),pg_current_xact_id()::text,current_database()||'/'||pg_postmaster_start_time()::text FROM command_receipts r WHERE r.command_id=$1 AND r.operation=$2 AND r.record_id=$3 AND r.actor_id=$4 AND r.authority_json::jsonb->>'companyId'=$5 FOR SHARE OF r")
        .bind(&request.command_key).bind(&request.operation).bind(&request.record_id).bind(&request.creator_user_id).bind(request.company_id.as_str()).fetch_optional(c).await?;
    let Some((image, request_hash, backend_pid, transaction_id, database_identity)) = row else {
        return Ok(None);
    };
    if !valid_request_hash(&request_hash) {
        return Err(invalid_origin().into());
    }
    Ok(Some(PgRawRecordOrigin {
        request,
        image,
        backend_pid,
        transaction_id,
        database_identity,
    }))
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
    let row:Option<(String,String,Option<String>,bool,i32,String,String)>=sqlx::query_as("SELECT encode(sha256(convert_to(to_jsonb(r)::text,'UTF8')),'hex'),r.request_hash,CASE WHEN octet_length(result_json)<=$6 THEN result_json END,COALESCE(octet_length(result_json),0)>$6,pg_backend_pid(),pg_current_xact_id()::text,current_database()||'/'||pg_postmaster_start_time()::text FROM command_receipts r WHERE r.command_id=$1 AND r.operation=$2 AND r.record_id=$3 AND r.actor_id=$4 AND r.authority_json::jsonb->>'companyId'=$5 FOR SHARE OF r")
        .bind(&request.command_key).bind(&request.operation).bind(&request.record_id).bind(&request.creator_user_id).bind(request.company_id.as_str()).bind(i64::try_from(limit_bytes).unwrap_or(i64::MAX)).fetch_optional(c).await?;
    let Some((image, request_hash, result, oversized, backend_pid, transaction_id, database_identity)) = row
    else {
        return Ok(None);
    };
    if !valid_request_hash(&request_hash) {
        return Err(invalid_origin().into());
    }
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

/// Bounded discovery of the actual controlled receipt creation, not a source
/// label supplied by a caller. The consuming command retains the returned proof.
pub async fn read_finance_receipt_creation_origin(
    c: &mut PgConnection,
    company: &CompanyId,
    id: &str,
    creator: &str,
) -> Result<Option<(PgRawRecordOrigin, CommandJson)>, Error> {
    require_unit(c).await?;
    let keys: Vec<String> = sqlx::query_scalar("SELECT command_id FROM command_receipts WHERE operation='finance.payment.create' AND record_id=$1 AND actor_id=$2 AND authority_json::jsonb->>'companyId'=$3 ORDER BY command_id LIMIT 2 FOR SHARE")
        .bind(id).bind(creator).bind(company.as_str()).fetch_all(&mut *c).await?;
    if keys.len() != 1 {
        return Ok(None);
    }
    read_raw_record_origin_result_with_limit(
        c,
        RawRecordOriginRequest {
            command_key: keys[0].clone(),
            operation: "finance.payment.create".into(),
            record_id: id.into(),
            creator_user_id: creator.into(),
            company_id: company.clone(),
        },
        2 * 1024 * 1024,
    )
    .await
}

/// Retained command-origin membership for the bank statements created for one
/// controlled receipt. This is historical evidence, not settlement permission.
pub struct CurrentServiceReceiptBankOrigins {
    unit: (String, i32, String, String),
    company: CompanyId,
    payment: String,
    members: Vec<(String, String, String)>,
    origins: Vec<PgRawRecordOrigin>,
}
async fn service_bank_unit(c: &mut PgConnection) -> Result<(String, i32, String, String), Error> {
    require_unit(c).await?;
    Ok(sqlx::query_as("SELECT current_database(),pg_backend_pid(),pg_current_xact_id()::text,pg_postmaster_start_time()::text").fetch_one(c).await?)
}
async fn service_bank_members(
    c: &mut PgConnection,
    company: &CompanyId,
    payment: &str,
) -> Result<Vec<(String, String, String)>, Error> {
    let rows=sqlx::query_as("SELECT command_id,record_id,actor_id FROM command_receipts WHERE operation='finance.bank.import' AND authority_json::jsonb->>'companyId'=$1 AND result_json::jsonb->>'settlementPaymentId'=$2 AND result_json::jsonb->>'id'=record_id ORDER BY command_id LIMIT 10001 FOR SHARE").bind(company.as_str()).bind(payment).fetch_all(c).await?;
    if rows.len() > 10000 {
        return Err(PersistenceError::new("SERVICE_BANK_ORIGINS_LIMIT").into());
    }
    Ok(rows)
}
impl CurrentServiceReceiptBankOrigins {
    pub fn statement_ids(&self) -> Vec<String> {
        self.members.iter().map(|v| v.1.clone()).collect()
    }
    pub async fn require_current(&self, c: &mut PgConnection) -> Result<(), Error> {
        if service_bank_unit(c).await? != self.unit
            || service_bank_members(c, &self.company, &self.payment).await? != self.members
        {
            return Err(BusinessError::new(
                FailureKind::Conflict,
                "SERVICE_PAYMENT_SOURCE_CHANGED",
                "The controlled receipt bank evidence changed.",
            )
            .into());
        }
        for origin in &self.origins {
            verify_raw_record_origin(c, origin).await?;
        }
        Ok(())
    }
}
pub async fn read_service_receipt_bank_origins(
    c: &mut PgConnection,
    company: &CompanyId,
    payment: &str,
) -> Result<CurrentServiceReceiptBankOrigins, Error> {
    let unit = service_bank_unit(c).await?;
    let members = service_bank_members(c, company, payment).await?;
    let mut origins = Vec::with_capacity(members.len());
    for (key, id, creator) in &members {
        let (origin, result) = read_raw_record_origin_result_with_limit(
            c,
            RawRecordOriginRequest {
                command_key: key.clone(),
                operation: "finance.bank.import".into(),
                record_id: id.clone(),
                creator_user_id: creator.clone(),
                company_id: company.clone(),
            },
            2 * 1024 * 1024,
        )
        .await?
        .ok_or_else(|| {
            BusinessError::new(
                FailureKind::Conflict,
                "SERVICE_PAYMENT_SOURCE_CHANGED",
                "The controlled receipt bank evidence changed.",
            )
        })?;
        let expected = payment.encode_utf16().collect::<Vec<_>>();
        if result
            .property("settlementPaymentId")?
            .and_then(|v| v.string_utf16_units().ok().flatten())
            .as_deref()
            != Some(expected.as_slice())
        {
            return Err(BusinessError::new(
                FailureKind::Conflict,
                "SERVICE_PAYMENT_SOURCE_CHANGED",
                "The controlled receipt bank evidence changed.",
            )
            .into());
        }
        origins.push(origin);
    }
    let source = CurrentServiceReceiptBankOrigins {
        unit,
        company: company.clone(),
        payment: payment.into(),
        members,
        origins,
    };
    source.require_current(c).await?;
    Ok(source)
}

/// A missing origin and a corrupt/ambiguous origin are distinct routing facts.
/// The latter may never authorize a legacy transport fallback.
pub async fn observe_finance_payment_creation_origin(
    c: &mut PgConnection,
    company: &CompanyId,
    id: &str,
    creator: &str,
) -> Result<Option<(PgRawRecordOrigin, CommandJson)>, Error> {
    require_unit(c).await?;
    let keys:Vec<String>=sqlx::query_scalar("SELECT command_id FROM command_receipts WHERE operation='finance.payment.create' AND record_id=$1 AND actor_id=$2 AND authority_json::jsonb->>'companyId'=$3 ORDER BY command_id LIMIT 2 FOR SHARE").bind(id).bind(creator).bind(company.as_str()).fetch_all(&mut *c).await?;
    if keys.is_empty() {
        return Ok(None);
    }
    let invalid = || {
        BusinessError::new(
            FailureKind::Conflict,
            "SERVICE_PAYMENT_SOURCE_CHANGED",
            "The controlled payment creation origin is ambiguous or changed.",
        )
    };
    if keys.len() != 1 {
        return Err(invalid().into());
    }
    let source = read_raw_record_origin_result_with_limit(
        c,
        RawRecordOriginRequest {
            command_key: keys[0].clone(),
            operation: "finance.payment.create".into(),
            record_id: id.into(),
            creator_user_id: creator.into(),
            company_id: company.clone(),
        },
        2 * 1024 * 1024,
    )
    .await?
    .ok_or_else(invalid)?;
    Ok(Some(source))
}

/// Workflow receipt candidates for the global 1092 payment lineage predicate.
/// The relation identifier is generated internally, is transaction-local, and
/// carries no personal authority. The coordinator must lock all owners before
/// the first snapshot and must prevent lock release/reacquisition.
pub struct PaymentOriginCandidates {
    payment_id:String, relation:String, relation_oid:i64,
    unit:(String,i32,String,String,String),
}

pub async fn lock_payment_origin_candidate_sources(c:&mut PgConnection)->Result<(),Error>{
    if !c.is_in_transaction(){return Err(PersistenceError::new("WORKFLOW_PAYMENT_ORIGIN_TRANSACTION_REQUIRED").into());}
    sqlx::query("LOCK TABLE command_receipts IN SHARE MODE NOWAIT").execute(c).await.map_err(PersistenceError::pre_snapshot_lock_conflict)?;Ok(())
}
async fn payment_origin_unit(c:&mut PgConnection)->Result<(String,i32,String,String,String),Error>{
    if !c.is_in_transaction() || sqlx::query_scalar::<_,String>("SHOW transaction_isolation").fetch_one(&mut *c).await? != "serializable" {return Err(PersistenceError::new("WORKFLOW_PAYMENT_ORIGIN_SERIALIZABLE_REQUIRED").into());}
    let held:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM pg_locks WHERE pid=pg_backend_pid() AND relation='command_receipts'::regclass AND granted AND mode IN ('ShareLock','ShareRowExclusiveLock','ExclusiveLock','AccessExclusiveLock'))").fetch_one(&mut *c).await?;
    if !held{return Err(PersistenceError::new("WORKFLOW_PAYMENT_ORIGIN_LOCK_REQUIRED").into());}
    Ok(sqlx::query_as("SELECT current_database(),pg_backend_pid(),pg_current_xact_id()::text,pg_postmaster_start_time()::text,current_user::text").fetch_one(c).await?)
}
/// Materialize every qualifying receipt in PostgreSQL, without transferring
/// arbitrarily long candidate text into application memory. SQL/TOAST resource
/// bounds still depend on PostgreSQL and are not asserted by this port.
pub async fn inspect_payment_origin_candidates(c:&mut PgConnection,payment_id:&str)->Result<PaymentOriginCandidates,Error>{
    let unit=payment_origin_unit(&mut *c).await?;
    let nonce:String=sqlx::query_scalar("SELECT replace(gen_random_uuid()::text,'-','')").fetch_one(&mut *c).await?;
    if nonce.len()!=32 || !nonce.bytes().all(|b|b.is_ascii_hexdigit() && !b.is_ascii_uppercase()) {return Err(PersistenceError::new("WORKFLOW_PAYMENT_ORIGIN_IDENTIFIER_INVALID").into());}
    // Only a fixed prefix and validated PostgreSQL-generated lowercase hex are interpolated.
    let name=format!("erpx_payment_origin_{nonce}");
    let create=format!("CREATE TEMPORARY TABLE {name} (command_id text NOT NULL, invoice_id text) ON COMMIT DROP");
    sqlx::query(sqlx::AssertSqlSafe(create.as_str())).execute(&mut *c).await?;
    let relation=format!("pg_temp.{name}");
    let insert=format!("INSERT INTO {relation} SELECT command_id,result_json::jsonb->>'settlementInvoiceId' FROM command_receipts WHERE operation='finance.payment.create' AND (record_id=$1 OR result_json::jsonb->>'id'=$1)");
    sqlx::query(sqlx::AssertSqlSafe(insert.as_str())).bind(payment_id).execute(&mut *c).await?;
    let oid:i64=sqlx::query_scalar("SELECT c.oid::bigint FROM pg_class c WHERE c.relnamespace=pg_my_temp_schema() AND c.relname=$1 AND c.relkind='r'").bind(&name).fetch_one(&mut *c).await?;
    Ok(PaymentOriginCandidates{payment_id:payment_id.to_owned(),relation,relation_oid:oid,unit})
}
impl PaymentOriginCandidates {
    /// Exact payment identity used to capture these receipts.
    pub fn payment_id(&self)->&str{&self.payment_id}
    /// SQL identifier generated only by this owner, never from request text.
    /// The consumer must call `require_current` around its same-unit join.
    pub fn relation_identifier(&self)->&str{&self.relation}
    pub async fn require_current(&self,c:&mut PgConnection)->Result<(),Error>{
        if payment_origin_unit(&mut *c).await? != self.unit{return Err(BusinessError::new(FailureKind::Conflict,"WORKFLOW_PAYMENT_ORIGIN_CHANGED","The payment origin source changed.").into());}
        let oid:Option<i64>=sqlx::query_scalar("SELECT c.oid::bigint FROM pg_class c WHERE c.relnamespace=pg_my_temp_schema() AND c.oid=to_regclass($1) AND c.relkind='r'").bind(&self.relation).fetch_optional(&mut *c).await?;
        if oid!=Some(self.relation_oid){return Err(BusinessError::new(FailureKind::Conflict,"WORKFLOW_PAYMENT_ORIGIN_CHANGED","The payment origin source changed.").into());}
        let shape:bool=sqlx::query_scalar("SELECT c.relpersistence='t' AND c.relowner=(current_user::regrole)::oid AND (SELECT count(*)=2 AND bool_and(a.atttypid='text'::regtype AND a.attcollation=(SELECT typcollation FROM pg_type WHERE oid='text'::regtype) AND NOT a.attisdropped AND ((a.attnum=1 AND a.attname='command_id' AND a.attnotnull) OR (a.attnum=2 AND a.attname='invoice_id' AND NOT a.attnotnull))) FROM pg_attribute a WHERE a.attrelid=c.oid AND a.attnum>0) FROM pg_class c WHERE c.oid=$1::bigint::oid").bind(self.relation_oid).fetch_one(&mut *c).await?;
        if !shape{return Err(BusinessError::new(FailureKind::Conflict,"WORKFLOW_PAYMENT_ORIGIN_CHANGED","The payment origin source changed.").into());}
        let sql=format!("WITH source AS (SELECT convert_to(command_id,'UTF8') AS command_id,convert_to(result_json::jsonb->>'settlementInvoiceId','UTF8') AS invoice_id FROM command_receipts WHERE operation='finance.payment.create' AND (record_id=$1 OR result_json::jsonb->>'id'=$1)), delta AS ((SELECT * FROM source EXCEPT ALL SELECT convert_to(command_id,'UTF8'),convert_to(invoice_id,'UTF8') FROM {}) UNION ALL (SELECT convert_to(command_id,'UTF8'),convert_to(invoice_id,'UTF8') FROM {} EXCEPT ALL SELECT * FROM source)) SELECT NOT EXISTS(SELECT 1 FROM delta)",self.relation,self.relation);
        let same:bool=sqlx::query_scalar(sqlx::AssertSqlSafe(sql.as_str())).bind(&self.payment_id).fetch_one(c).await?;
        if !same{return Err(BusinessError::new(FailureKind::Conflict,"WORKFLOW_PAYMENT_ORIGIN_CHANGED","The payment origin source changed.").into());}Ok(())
    }
}

/// Trusted owner-to-owner source only. A request must never choose an
/// implementation or SQL identifier. This is routing evidence, not authority.
/// The implementing owner must retain its complete relation, transaction and
/// source locks, and reject relation replacement or role changes.
pub trait FieldPaymentTargets: Sync {
    fn relation_identifier(&self) -> &str;
    fn require_current<'a>(&'a self, c: &'a mut PgConnection)
        -> erpx_server_runtime::public::postgres::PgWorkFuture<'a, (), BusinessError>;
}

fn valid_files_ancestry_relation(name: &str) -> bool {
    let Some(hex) = name.strip_prefix("pg_temp.\"erpx_files_ancestry_").and_then(|v| v.strip_suffix('"')) else { return false; };
    hex.len() == 32 && hex.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

/// Complete Workflow receipt half for the six 1093 source/linked paths.
/// Direct selected-link routing remains a separate obligation. PostgreSQL
/// TOAST, temporary storage and work-memory usage are not bounded by this API.
pub struct FieldPaymentReceiptBatch<'a, T: FieldPaymentTargets + ?Sized> {
    source: &'a T,
    direct: bool,
    relation: String,
    relation_oid: i64,
    collations: Vec<i64>,
    search_path: String,
    unit: (String, i32, String, String, String),
}

fn field_payment_batch_source(relation: &str, direct: bool) -> String {
    let paths=if direct {"'direct-set','direct-upload','direct-upload-set'"} else {"'source-attachment','source-upload','source-upload-set','linked-set','linked-upload','linked-upload-set'"};
    format!("SELECT t.path,t.link_id,t.attachment_id,t.upload_intent_id,t.set_id,t.upload_set_id,t.kind,t.target,r.command_id,r.result_json::jsonb->>'settlementInvoiceId' AS invoice_id FROM {relation} t JOIN command_receipts r ON r.operation='finance.payment.create' AND (r.record_id=t.target OR r.result_json::jsonb->>'id'=t.target) WHERE t.path IN ({paths}) AND t.kind='finance_payment'")
}

pub async fn inspect_field_payment_receipt_batch<'a, T: FieldPaymentTargets + ?Sized>(
    c: &mut PgConnection, source: &'a T,
) -> Result<FieldPaymentReceiptBatch<'a, T>, Error> {
    inspect_payment_batch(c,source,false).await
}
async fn inspect_payment_batch<'a,T:FieldPaymentTargets + ?Sized>(
    c:&mut PgConnection,source:&'a T,direct:bool,
)->Result<FieldPaymentReceiptBatch<'a,T>,Error>{
    let unit = payment_origin_unit(&mut *c).await?;
    let visible: bool = sqlx::query_scalar("SELECT NOT row_security_active('command_receipts'::regclass)").fetch_one(&mut *c).await?;
    if !visible { return Err(PersistenceError::new("WORKFLOW_PAYMENT_BATCH_RLS_UNSUPPORTED").into()); }
    let search_path: String = sqlx::query_scalar("SELECT current_setting('search_path')").fetch_one(&mut *c).await?;
    source.require_current(&mut *c).await?;
    let relation = source.relation_identifier();
    if !valid_files_ancestry_relation(relation) { return Err(PersistenceError::new("WORKFLOW_PAYMENT_BATCH_IDENTIFIER_INVALID").into()); }
    let nonce: String = sqlx::query_scalar("SELECT replace(gen_random_uuid()::text,'-','')").fetch_one(&mut *c).await?;
    if nonce.len()!=32 || !nonce.bytes().all(|b|b.is_ascii_digit() || (b'a'..=b'f').contains(&b)) { return Err(PersistenceError::new("WORKFLOW_PAYMENT_BATCH_IDENTIFIER_INVALID").into()); }
    let name = format!("erpx_payment_batch_{nonce}");
    let sql = format!("CREATE TEMP TABLE {name} ON COMMIT DROP AS {}", field_payment_batch_source(relation,direct));
    sqlx::query(sqlx::AssertSqlSafe(sql)).execute(&mut *c).await?;
    let oid: i64 = sqlx::query_scalar("SELECT oid::bigint FROM pg_class WHERE relnamespace=pg_my_temp_schema() AND relname=$1 AND relkind='r'").bind(&name).fetch_one(&mut *c).await?;
    let collations: Vec<i64> = sqlx::query_scalar("SELECT attcollation::bigint FROM pg_attribute WHERE attrelid=$1::bigint::oid AND attnum>0 ORDER BY attnum").bind(oid).fetch_all(&mut *c).await?;
    source.require_current(&mut *c).await?;
    let result = FieldPaymentReceiptBatch { source, direct, relation: format!("pg_temp.\"{name}\""), relation_oid: oid, collations, search_path, unit };
    result.require_current(c).await?;
    Ok(result)
}

impl<T: FieldPaymentTargets + ?Sized> FieldPaymentReceiptBatch<'_, T> {
    /// Internal validated identifier; Finance must revalidate around its join.
    pub fn relation_identifier(&self) -> &str { &self.relation }
    pub fn source(&self) -> &T { self.source }
    pub async fn require_current(&self, c: &mut PgConnection) -> Result<(), Error> {
        let changed = || BusinessError::new(FailureKind::Conflict,"WORKFLOW_PAYMENT_ORIGIN_CHANGED","The payment origin source changed.");
        if payment_origin_unit(&mut *c).await? != self.unit { return Err(changed().into()); }
        let (visible,search_path):(bool,String)=sqlx::query_as("SELECT NOT row_security_active('command_receipts'::regclass),current_setting('search_path')").fetch_one(&mut *c).await?;
        if !visible || search_path!=self.search_path { return Err(changed().into()); }
        self.source.require_current(&mut *c).await?;
        if !valid_files_ancestry_relation(self.source.relation_identifier()) { return Err(changed().into()); }
        let shape: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM pg_class c WHERE c.oid=to_regclass($1) AND c.oid=$2::bigint::oid AND c.relnamespace=pg_my_temp_schema() AND c.relkind='r' AND c.relpersistence='t' AND c.relowner=(current_user::regrole)::oid AND NOT c.relrowsecurity AND NOT EXISTS(SELECT 1 FROM pg_trigger t WHERE t.tgrelid=c.oid AND NOT t.tgisinternal) AND NOT EXISTS(SELECT 1 FROM pg_rewrite r WHERE r.ev_class=c.oid) AND (SELECT count(*)=10 AND bool_and(NOT a.attisdropped AND a.attgenerated='' AND a.atttypid='text'::regtype AND a.attcollation::bigint=($3::bigint[])[a.attnum] AND a.attname=(ARRAY['path','link_id','attachment_id','upload_intent_id','set_id','upload_set_id','kind','target','command_id','invoice_id'])[a.attnum]) FROM pg_attribute a WHERE a.attrelid=c.oid AND a.attnum>0))").bind(&self.relation).bind(self.relation_oid).bind(&self.collations).fetch_one(&mut *c).await?;
        if !shape { return Err(changed().into()); }
        let fields = "convert_to(path,'UTF8'),convert_to(link_id,'UTF8'),convert_to(attachment_id,'UTF8'),convert_to(upload_intent_id,'UTF8'),convert_to(set_id,'UTF8'),convert_to(upload_set_id,'UTF8'),convert_to(kind,'UTF8'),convert_to(target,'UTF8'),convert_to(command_id,'UTF8'),convert_to(invoice_id,'UTF8')";
        let sql = format!("WITH source AS ({}), delta AS ((SELECT {fields} FROM source EXCEPT ALL SELECT {fields} FROM {}) UNION ALL (SELECT {fields} FROM {} EXCEPT ALL SELECT {fields} FROM source)) SELECT NOT EXISTS(SELECT 1 FROM delta)",field_payment_batch_source(self.source.relation_identifier(),self.direct),self.relation,self.relation);
        let same: bool = sqlx::query_scalar(sqlx::AssertSqlSafe(sql)).fetch_one(&mut *c).await?;
        self.source.require_current(c).await?;
        if !same { return Err(changed().into()); }
        Ok(())
    }
}

/// Direct selected-link lineage receipt half; distinct from the six source
/// branches and never a selected authorization.
pub struct FieldDirectPaymentReceiptBatch<'a,T:FieldPaymentTargets + ?Sized>(FieldPaymentReceiptBatch<'a,T>);
pub async fn inspect_field_direct_payment_receipt_batch<'a,T:FieldPaymentTargets + ?Sized>(c:&mut PgConnection,source:&'a T)->Result<FieldDirectPaymentReceiptBatch<'a,T>,Error>{
    Ok(FieldDirectPaymentReceiptBatch(inspect_payment_batch(c,source,true).await?))
}
impl<T:FieldPaymentTargets + ?Sized> FieldDirectPaymentReceiptBatch<'_,T>{
    pub fn relation_identifier(&self)->&str{self.0.relation_identifier()}
    pub fn source(&self)->&T{self.0.source()}
    pub async fn require_current(&self,c:&mut PgConnection)->Result<(),Error>{self.0.require_current(c).await}
}

/// Trusted Files-owned routing source. Implementations are selected by
/// composition, never by request data; this trait is not an authorization.
/// The SQL is an immutable owner constant yielding eight Files ancestry columns
/// with `$1` selected link and `$2` selected Company. It reads canonical public
/// Holding relations only. A malicious Rust implementation can fabricate facts.
pub trait FieldAttachmentRoutingSource: Sync {
    fn selected_link_id(&self) -> &str;
    fn company_id(&self) -> &str;
    fn ancestry_sql(&self) -> &'static str;
    fn require_current<'a>(&'a self,c:&'a mut PgConnection)
        -> erpx_server_runtime::public::postgres::PgWorkFuture<'a,(),BusinessError>;
}

/// Acquire before the first Holding snapshot. The enclosing coordinator must
/// establish a compatible global order and retry the entire unit after 55P03.
/// This route supports the migration-owned `public` Holding schema only.
pub async fn lock_field_payment_routing_sources(c:&mut PgConnection)->Result<(),Error>{
    if !c.is_in_transaction(){return Err(PersistenceError::new("WORKFLOW_PAYMENT_ORIGIN_TRANSACTION_REQUIRED").into());}
    sqlx::query("LOCK TABLE public.command_receipts IN SHARE MODE NOWAIT").execute(c).await.map_err(PersistenceError::pre_snapshot_lock_conflict)?;
    Ok(())
}
async fn routing_identity(c:&mut PgConnection)->Result<((String,i32,String,String,String),String,i64),Error>{
    if !c.is_in_transaction() || sqlx::query_scalar::<_,String>("SHOW transaction_isolation").fetch_one(&mut *c).await?!="serializable" {return Err(PersistenceError::new("WORKFLOW_PAYMENT_ORIGIN_SERIALIZABLE_REQUIRED").into());}
    let (held,visible,oid,path):(bool,bool,i64,String)=sqlx::query_as("SELECT EXISTS(SELECT 1 FROM pg_locks WHERE pid=pg_backend_pid() AND locktype='relation' AND relation='public.command_receipts'::regclass AND granted AND mode IN ('ShareLock','ShareRowExclusiveLock','ExclusiveLock','AccessExclusiveLock')),NOT row_security_active('public.command_receipts'::regclass),'public.command_receipts'::regclass::oid::bigint,current_setting('search_path')").fetch_one(&mut *c).await?;
    if !held{return Err(PersistenceError::new("WORKFLOW_PAYMENT_ORIGIN_LOCK_REQUIRED").into());}
    if !visible{return Err(PersistenceError::new("WORKFLOW_PAYMENT_BATCH_RLS_UNSUPPORTED").into());}
    let unit=sqlx::query_as("SELECT current_database(),pg_backend_pid(),pg_current_xact_id()::text,pg_postmaster_start_time()::text,current_user::text").fetch_one(c).await?;
    Ok((unit,path,oid))
}

/// Workflow owns the receipt CTE, while Files retains the selected source.
/// This is routing input only: consumers must recompute their complete result
/// after deferred effects and separately retain selected target authority.
/// No complete immutable receipt or ancestry image is asserted.
pub struct CurrentFieldPaymentRouting<'a,T:FieldAttachmentRoutingSource + ?Sized>{
    source:&'a T,identity:((String,i32,String,String,String),String,i64),
    link:String,company:String,ancestry_sql:&'static str,
}
pub async fn inspect_field_payment_routing<'a,T:FieldAttachmentRoutingSource + ?Sized>(c:&mut PgConnection,source:&'a T)->Result<CurrentFieldPaymentRouting<'a,T>,Error>{
    let identity=routing_identity(&mut *c).await?;
    source.require_current(&mut *c).await?;
    let witness=CurrentFieldPaymentRouting{source,identity,link:source.selected_link_id().to_owned(),company:source.company_id().to_owned(),ancestry_sql:source.ancestry_sql()};
    witness.require_current(c).await?;Ok(witness)
}
impl<T:FieldAttachmentRoutingSource + ?Sized> CurrentFieldPaymentRouting<'_,T>{
    pub fn source(&self)->&T{self.source}
    pub fn selected_link_id(&self)->&str{&self.link}
    pub fn company_id(&self)->&str{&self.company}
    /// Fixed owner SQL only, composed with the trusted Files owner's static
    /// SELECT. Bind $1=selected link and $2=Company; never append user SQL.
    /// The consumer must revalidate this witness around its own query.
    pub fn payment_cte(&self)->String{
        format!("WITH file_ancestry AS ({}), payment_receipts AS (SELECT t.path,t.link_id,t.attachment_id,t.upload_intent_id,t.set_id,t.upload_set_id,t.kind,t.target,r.command_id,r.result_json::jsonb->>'settlementInvoiceId' AS invoice_id FROM file_ancestry t JOIN public.command_receipts r ON r.operation='finance.payment.create' AND (r.record_id=t.target OR r.result_json::jsonb->>'id'=t.target) WHERE t.path IN ('direct-set','direct-upload','direct-upload-set','source-attachment','source-upload','source-upload-set','linked-set','linked-upload','linked-upload-set') AND t.kind='finance_payment')",self.ancestry_sql)
    }
    pub async fn require_current(&self,c:&mut PgConnection)->Result<(),Error>{
        let changed=||BusinessError::new(FailureKind::Conflict,"WORKFLOW_PAYMENT_ORIGIN_CHANGED","The payment routing source changed.");
        if routing_identity(&mut *c).await?!=self.identity || self.source.selected_link_id()!=self.link || self.source.company_id()!=self.company || self.source.ancestry_sql()!=self.ancestry_sql {return Err(changed().into());}
        self.source.require_current(&mut *c).await?;
        if routing_identity(c).await?!=self.identity{return Err(changed().into());}Ok(())
    }
}

/// Workflow-owned SQL source for the generic Field/Finance History joins.
/// This fixed, non-materialized CTE never returns receipt payloads to the
/// application, imposes no candidate cap and does not use a TEMP relation.
/// The raw JSON text is intentional: consuming joins must apply the same casts
/// and predicates as their legacy source, without coalescing invalid JSON.
const FIELD_HISTORY_RECEIPTS_CTE:&str="field_history_receipts AS (SELECT command_id,operation,record_id,actor_id,authority_json,result_json FROM public.command_receipts WHERE operation IN ('field.order.release-billing','field.order.issue-part','finance.payment.create'))";
type FieldHistoryReceiptUnit=((String,i32,String,String,String),String,i64);

/// Acquire before any protected snapshot, with other owner fences in a
/// coordinator-reviewed order. A conflict retries the entire unit. This lock
/// alone is not receipt provenance, a global negative or personal authority.
pub async fn lock_field_history_receipt_join_sources(c:&mut PgConnection)->Result<(),Error>{
    if !c.is_in_transaction(){return Err(PersistenceError::new("FIELD_HISTORY_RECEIPT_TRANSACTION_REQUIRED").into());}
    sqlx::query("LOCK TABLE public.command_receipts IN SHARE MODE NOWAIT").execute(c).await.map_err(PersistenceError::pre_snapshot_lock_conflict)?;Ok(())
}
async fn field_history_receipt_join_unit(c:&mut PgConnection)->Result<FieldHistoryReceiptUnit,Error>{
    if !c.is_in_transaction() || sqlx::query_scalar::<_,String>("SHOW transaction_isolation").fetch_one(&mut *c).await?!="serializable"{return Err(PersistenceError::new("FIELD_HISTORY_RECEIPT_SERIALIZABLE_REQUIRED").into());}
    let (held,visible,path,oid):(bool,bool,String,i64)=sqlx::query_as("SELECT c.relnamespace='public'::regnamespace AND c.relkind IN ('r','p') AND c.relpersistence='p' AND EXISTS(SELECT 1 FROM pg_locks l WHERE l.pid=pg_backend_pid() AND l.locktype='relation' AND l.relation=c.oid AND l.granted AND l.mode IN ('ShareLock','ShareRowExclusiveLock','ExclusiveLock','AccessExclusiveLock')),NOT row_security_active(c.oid),current_setting('search_path'),c.oid::bigint FROM pg_class c WHERE c.oid='public.command_receipts'::regclass").fetch_one(&mut *c).await?;
    if !held{return Err(PersistenceError::new("FIELD_HISTORY_RECEIPT_LOCK_REQUIRED").into());}
    if !visible{return Err(PersistenceError::new("FIELD_HISTORY_RECEIPT_RLS_UNSUPPORTED").into());}
    let transaction=sqlx::query_as("SELECT current_database(),pg_backend_pid(),pg_current_xact_id()::text,pg_postmaster_start_time()::text,current_user::text").fetch_one(c).await?;Ok((transaction,path,oid))
}
/// A same-unit, canonical Workflow join source. `require_current` validates
/// its identity and fences, not the receipt rows or the result of any join.
/// Consumers must rerun and compare the complete positive/negative joined
/// classification after their own late effects, retaining all owner evidence.
pub struct CurrentFieldHistoryReceiptJoin{unit:FieldHistoryReceiptUnit}
impl CurrentFieldHistoryReceiptJoin{
    /// Trusted, fixed CTE fragment. Bind all candidate values separately; never
    /// substitute request-supplied SQL or use this as an authorization grant.
    pub fn cte_sql(&self)->&'static str{FIELD_HISTORY_RECEIPTS_CTE}
    pub async fn require_current(&self,c:&mut PgConnection)->Result<(),Error>{if field_history_receipt_join_unit(c).await?!=self.unit{return Err(BusinessError::new(FailureKind::Conflict,"FIELD_HISTORY_RECEIPT_UNIT_CHANGED","The Workflow History join source changed.").into());}Ok(())}
}
pub async fn inspect_field_history_receipt_join(c:&mut PgConnection)->Result<CurrentFieldHistoryReceiptJoin,Error>{Ok(CurrentFieldHistoryReceiptJoin{unit:field_history_receipt_join_unit(c).await?})}

// Fixed SQL source for all actual Count-create receipts. Inventory and Finance
// join by stored record_id using PostgreSQL semantics and choose marker keys;
// this source never caps candidates or parses/replaces raw JSON in Rust.
const INITIAL_COUNT_HISTORY_RECEIPTS_CTE:&str="initial_count_history_receipts AS (SELECT command_id,operation,record_id,actor_id,request_hash,authority_json,result_json FROM public.command_receipts WHERE operation='inventory.count.create'), initial_count_history_support_receipts AS (SELECT command_id,operation,record_id,actor_id,request_hash,authority_json,result_json FROM public.command_receipts WHERE operation IN ('commerce.service-parts-posting-setup.initialize','commerce.service-parts-posting-setup.confirm-existing','inventory.count.post','inventory.count.reject','inventory.count.cancel','attachment.record.bind','attachment.compatibility.bind'))";
/// The same physical Workflow table fence as the Field invoice/payment join.
/// Acquire this before any source snapshot and retry the whole unit on 55P03.
pub async fn lock_initial_count_history_receipt_sources(c:&mut PgConnection)->Result<(),Error>{lock_field_history_receipt_join_sources(c).await}
/// A canonical same-unit raw receipt join source only. It does not retain
/// receipt contents, Count ownership, Company authority, a marker decision,
/// or a global negative. Consumer must recompute its complete joined decision
/// after all late effects and compare to its separately retained outcome.
pub struct CurrentInitialCountHistoryReceiptJoin{unit:FieldHistoryReceiptUnit}
impl CurrentInitialCountHistoryReceiptJoin{
    pub fn cte_sql(&self)->&'static str{INITIAL_COUNT_HISTORY_RECEIPTS_CTE}
    pub async fn require_current(&self,c:&mut PgConnection)->Result<(),Error>{if field_history_receipt_join_unit(c).await?!=self.unit{return Err(BusinessError::new(FailureKind::Conflict,"INITIAL_COUNT_HISTORY_RECEIPT_UNIT_CHANGED","The Workflow Count History join source changed.").into());}Ok(())}
}
pub async fn inspect_initial_count_history_receipt_join(c:&mut PgConnection)->Result<CurrentInitialCountHistoryReceiptJoin,Error>{Ok(CurrentInitialCountHistoryReceiptJoin{unit:field_history_receipt_join_unit(c).await?})}
