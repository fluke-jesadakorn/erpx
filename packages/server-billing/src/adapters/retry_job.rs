//! PostgreSQL adapter for the native Billing job retry command.
//!
//! The caller owns the serializable Control unit. This module locks the
//! Holding account and command key, replays an exact receipt, requeues one
//! Holding-owned job or pending Stripe inbox event, and writes the
//! account-version bump, audit and receipt in the same transaction.

use crate::domain::actor::BillingActor;
use crate::domain::command::{
    CommandReceipt, account_required, receipt_replay, request_invalid, result_json, valid_hash,
};
use crate::domain::retry_job::{RetryJob, ValidatedRetryJob};
use erpx_server_runtime::public::postgres::PgTransactionError;
use erpx_server_shared::public::{BusinessError, FailureKind, HoldingId, TimestampMillis};
use sqlx::{PgConnection, Row};

type Error = PgTransactionError<BusinessError>;

fn missing() -> BusinessError {
    BusinessError::new(FailureKind::NotFound, "JOB_MISSING", "Job not found.")
}

fn reconciliation_required() -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        "MANUAL_RECONCILIATION_REQUIRED",
        "Reconcile the ambiguous Stripe operation before retrying. Do not create another invoice.",
    )
}

fn unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "BILLING_RETRY_UNAVAILABLE",
        "The Billing retry is unavailable.",
    )
}

pub async fn execute_retry_job(
    connection: &mut PgConnection,
    actor: &BillingActor,
    target: &HoldingId,
    command: &RetryJob,
    now: TimestampMillis,
) -> Result<CommandReceipt, Error> {
    actor.require_target_command(target, now)?;
    let command = command.validate()?;
    let identity = serde_json::to_string(&serde_json::json!([
        actor.user_id.as_str(),
        target.as_str(),
        "retry",
        command.job_id,
        command.reason,
    ]))
    .map_err(|_| unavailable())?;
    run(connection, actor, target, &command, now, &identity).await
}

async fn run(
    connection: &mut PgConnection,
    actor: &BillingActor,
    target: &HoldingId,
    command: &ValidatedRetryJob,
    now: TimestampMillis,
    identity: &str,
) -> Result<CommandReceipt, Error> {
    let request_hash: String =
        sqlx::query_scalar("SELECT encode(sha256(convert_to($1,'UTF8')),'hex')")
            .bind(identity)
            .fetch_one(&mut *connection)
            .await?;
    if !valid_hash(&request_hash) {
        return Err(unavailable().into());
    }
    sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1,0))")
        .bind(command.key.as_str())
        .execute(&mut *connection)
        .await?;
    let receipt =
        sqlx::query("SELECT digest,result_json FROM billing_receipts WHERE id=$1 FOR UPDATE")
            .bind(command.key.as_str())
            .fetch_optional(&mut *connection)
            .await?;
    if let Some(receipt) = receipt {
        let digest: String = receipt.try_get("digest")?;
        let result_json: String = receipt.try_get("result_json")?;
        let replay = CommandReceipt {
            digest,
            result_json,
        };
        if receipt_replay(Some(replay.clone()), &request_hash)?.is_some() {
            return Ok(replay);
        }
    }
    let account =
        sqlx::query("SELECT version FROM billing_accounts WHERE holding_id=$1 FOR UPDATE")
            .bind(target.as_str())
            .fetch_optional(&mut *connection)
            .await?;
    if account.is_none() {
        return Err(account_required().into());
    }
    let job = sqlx::query(
        "SELECT id,last_error FROM billing_jobs WHERE id=$1 AND holding_id=$2 FOR UPDATE",
    )
    .bind(&command.job_id)
    .bind(target.as_str())
    .fetch_optional(&mut *connection)
    .await?;
    let mut job_id: Option<String> = None;
    if let Some(job) = job {
        let last_error: Option<String> = job.try_get("last_error")?;
        if last_error.as_deref() == Some("IDEMPOTENCY_WINDOW_EXPIRED") {
            return Err(reconciliation_required().into());
        }
        job_id = Some(job.try_get("id")?);
    } else {
        let updated = sqlx::query(
            "UPDATE stripe_inbox SET next_at=$2 WHERE id=$1 AND state='pending' \
             AND payload_json::jsonb #>> '{data,object,customer}'=(\
               SELECT customer_id FROM billing_accounts WHERE holding_id=$3\
             ) RETURNING id",
        )
        .bind(&command.job_id)
        .bind(now.get())
        .bind(target.as_str())
        .fetch_optional(&mut *connection)
        .await?;
        if updated.is_none() {
            return Err(missing().into());
        }
    }
    if let Some(job_id) = job_id {
        sqlx::query("UPDATE billing_jobs SET state='pending',next_at=$2 WHERE id=$1")
            .bind(&job_id)
            .bind(now.get())
            .execute(&mut *connection)
            .await?;
    }
    sqlx::query("UPDATE billing_accounts SET version=version+1 WHERE holding_id=$1")
        .bind(target.as_str())
        .execute(&mut *connection)
        .await?;
    let details = serde_json::json!({
        "jobId": command.job_id,
        "reason": command.reason,
        "action": "retry",
    })
    .to_string();
    sqlx::query(
        "INSERT INTO billing_audit(id,holding_id,actor,action,reason,details_json,created_at) \
         VALUES($1,$2,$3,'retry',$4,$5,$6)",
    )
    .bind(format!("billing-retry-{}", command.key.as_str()))
    .bind(target.as_str())
    .bind(actor.user_id.as_str())
    .bind(&command.reason)
    .bind(&details)
    .bind(now.get())
    .execute(&mut *connection)
    .await?;
    let result = result_json().to_owned();
    sqlx::query(
        "INSERT INTO billing_receipts(id,digest,result_json,created_at) VALUES($1,$2,$3,$4)",
    )
    .bind(command.key.as_str())
    .bind(&request_hash)
    .bind(&result)
    .bind(now.get())
    .execute(&mut *connection)
    .await?;
    Ok(CommandReceipt {
        digest: request_hash,
        result_json: result,
    })
}

pub fn retry_job_command_key(
    key: &str,
) -> Result<erpx_server_shared::public::CommandKey, BusinessError> {
    erpx_server_shared::public::CommandKey::new(key).map_err(|_| request_invalid())
}

pub fn retry_job_command(
    key: erpx_server_shared::public::CommandKey,
    reason: String,
    job_id: erpx_server_shared::public::RecordId,
) -> RetryJob {
    RetryJob {
        key,
        reason,
        job_id,
    }
}

pub fn retry_job_id(value: &str) -> Result<erpx_server_shared::public::RecordId, BusinessError> {
    erpx_server_shared::public::RecordId::new(value).map_err(|_| request_invalid())
}
