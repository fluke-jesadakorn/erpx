//! Owner receipt, audit and outbox rows shared by the native collaboration
//! units. The caller owns the serializable transaction; nothing here commits,
//! retries or mints authority.
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::{BusinessError, FailureKind};
use serde_json::Value;
use sqlx::{Connection, PgConnection, Row};

pub(crate) type Error = PgTransactionError<BusinessError>;

pub(crate) fn source_changed(code: &'static str, message: &'static str) -> Error {
    BusinessError::new(FailureKind::Conflict, code, message).into()
}

pub(crate) async fn require_serializable(
    connection: &mut PgConnection,
    required: &'static str,
) -> Result<(), Error> {
    if !connection.is_in_transaction()
        || sqlx::query_scalar::<_, String>("SHOW transaction_isolation")
            .fetch_one(&mut *connection)
            .await?
            != "serializable"
    {
        return Err(PersistenceError::new(required).into());
    }
    Ok(())
}

/// Read the exact command receipt and reject a key that belongs to another
/// operation, actor, record or request body.
pub(crate) async fn replay(
    connection: &mut PgConnection,
    key: &str,
    operation: &str,
    actor: &str,
    record_id: &str,
    fingerprint: &str,
) -> Result<Option<Value>, Error> {
    let row = sqlx::query(
        "SELECT operation,actor_id,record_id,request_hash,result_json FROM command_receipts WHERE command_id=$1 FOR SHARE",
    )
    .bind(key)
    .fetch_optional(&mut *connection)
    .await?;
    let Some(row) = row else {
        return Ok(None);
    };
    let stored_operation: String = row.try_get(0)?;
    let stored_actor: String = row.try_get(1)?;
    let stored_record: String = row.try_get(2)?;
    let stored_fingerprint: String = row.try_get(3)?;
    if stored_operation != operation
        || stored_actor != actor
        || stored_record != record_id
        || stored_fingerprint != fingerprint
    {
        return Err(BusinessError::new(
            FailureKind::Conflict,
            "IDEMPOTENCY_CONFLICT",
            "This command key belongs to another request.",
        )
        .into());
    }
    let raw: Option<String> = row.try_get(4)?;
    let raw = raw.ok_or_else(|| {
        source_changed(
            "COLLABORATION_RECEIPT_INVALID",
            "The stored receipt is unavailable.",
        )
    })?;
    serde_json::from_str(&raw).map(Some).map_err(|_| {
        source_changed(
            "COLLABORATION_RECEIPT_INVALID",
            "The stored receipt is unavailable.",
        )
    })
}

pub(crate) struct LedgerRows<'a> {
    pub key: &'a str,
    pub operation: &'a str,
    pub actor: &'a str,
    pub assignment: &'a str,
    pub record_id: &'a str,
    pub record_version: i64,
    pub fingerprint: &'a str,
    pub result: &'a Value,
    pub record_type: &'a str,
    pub purpose: &'a str,
    pub event_type: &'a str,
    pub now: i64,
}

/// Stage receipt, audit and outbox rows for an already applied owner plan.
/// Row images must have been retained before this call.
pub(crate) async fn write_ledger(
    connection: &mut PgConnection,
    rows: LedgerRows<'_>,
) -> Result<(), Error> {
    let raw = serde_json::to_string(rows.result).map_err(|_| {
        source_changed(
            "COLLABORATION_RECEIPT_INVALID",
            "The owner result could not be encoded.",
        )
    })?;
    sqlx::query(
        "INSERT INTO command_receipts(command_id,operation,actor_id,request_hash,record_id,result_json,created_at) VALUES($1,$2,$3,$4,$5,$6,$7)",
    )
    .bind(rows.key)
    .bind(rows.operation)
    .bind(rows.actor)
    .bind(rows.fingerprint)
    .bind(rows.record_id)
    .bind(&raw)
    .bind(rows.now)
    .execute(&mut *connection)
    .await?;
    sqlx::query(
        "INSERT INTO audit_events(id,command_id,actor_id,assignment_id,action,record_type,record_id,record_version,purpose,created_at) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10)",
    )
    .bind(format!("audit-{}", rows.key))
    .bind(rows.key)
    .bind(rows.actor)
    .bind(rows.assignment)
    .bind(rows.operation)
    .bind(rows.record_type)
    .bind(rows.record_id)
    .bind(rows.record_version)
    .bind(rows.purpose)
    .bind(rows.now)
    .execute(&mut *connection)
    .await?;
    sqlx::query(
        "INSERT INTO outbox_events(id,command_id,event_type,aggregate_type,aggregate_id,aggregate_version,payload_json,created_at) VALUES($1,$2,$3,$4,$5,$6,$7,$8)",
    )
    .bind(format!("outbox-{}", rows.key))
    .bind(rows.key)
    .bind(rows.event_type)
    .bind(rows.record_type)
    .bind(rows.record_id)
    .bind(rows.record_version)
    .bind(&raw)
    .bind(rows.now)
    .execute(&mut *connection)
    .await?;
    Ok(())
}

/// Deterministic request fingerprint for a typed owner command. The Debug form
/// is stable for one binary and is intentionally not a replacement for the
/// transport's JSON hash on the legacy TypeScript path.
pub(crate) fn fingerprint<T: std::fmt::Debug>(operation: &str, value: &T) -> String {
    format!("erpx-collaboration-command-v1|{operation}|{value:?}")
}
