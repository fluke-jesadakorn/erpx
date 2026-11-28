//! Native Finance attachment target-version guard.
//!
//! This is the owner-owned counterpart of the retired dynamic `UPDATE`
//! writers in `attachments-finance-facts.ts` and
//! `attachments-service-facts.ts`. It runs entirely on the caller's
//! serializable unit: it never opens a pool, commits, retries or mints
//! authority. The composition supplies the already-authorized actor and
//! command key; this adapter rechecks Company scope and the expected target
//! version inside the owner boundary before a single version step, and writes
//! the receipt, audit event and outbox event in the same unit.
use crate::domain::attachment_version::{
    AttachmentVersionTarget, attachment_version_target, require_actor_scope, target_changed,
    target_not_found, unavailable, version_fingerprint,
};
use erpx_platform_runtime_server::public::postgres::{PersistenceError, PgTransactionError};
use erpx_platform_shared_server::public::{BusinessError, CommandKey, FailureKind};
use serde::{Deserialize, Serialize};
use serde_json::json;
use sqlx::{AssertSqlSafe, Connection, PgConnection, Row};

type Error = PgTransactionError<BusinessError>;

pub const VERSION_STEP_OPERATION: &str = "finance.attachment.version-step";

/// Current command identity. The composition supplies this after the retained
/// finance.write capability proof; the adapter never widens it.
#[derive(Clone, Debug)]
pub struct AttachmentVersionActor {
    pub user_id: String,
    pub assignment_id: String,
    pub company_id: String,
    pub role: String,
    pub scope_kind: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct AttachmentVersionStep {
    pub target_type: String,
    pub target_id: String,
    pub changed: bool,
    pub version: i64,
}

async fn serializable(c: &mut PgConnection) -> Result<(), Error> {
    if !c.is_in_transaction() {
        return Err(
            PersistenceError::new("FINANCE_ATTACHMENT_VERSION_TRANSACTION_REQUIRED").into(),
        );
    }
    let isolation: String = sqlx::query_scalar("SHOW transaction_isolation")
        .fetch_one(&mut *c)
        .await?;
    if isolation != "serializable" {
        return Err(
            PersistenceError::new("FINANCE_ATTACHMENT_VERSION_SERIALIZABLE_REQUIRED").into(),
        );
    }
    Ok(())
}

fn resolve(target_type: &str) -> Result<AttachmentVersionTarget, Error> {
    attachment_version_target(target_type)
        .ok_or_else(|| crate::domain::attachment_version::unsupported_target().into())
}

/// Lock and read the live version of an allow-listed target under the caller's
/// Company scope. A foreign or missing row is absent, never a fabricated
/// version. All SQL fragments come from the owner allow-list.
async fn current_version(
    c: &mut PgConnection,
    target: AttachmentVersionTarget,
    company_id: &str,
    target_id: &str,
) -> Result<Option<i64>, Error> {
    let extra = target
        .extra
        .map(|extra| format!(" AND {extra}"))
        .unwrap_or_default();
    let sql = format!(
        "SELECT company_id,{} AS version FROM {} WHERE id=$1 AND company_id=$2{} FOR UPDATE",
        target.column, target.table, extra
    );
    let row = sqlx::query(AssertSqlSafe(sql))
        .bind(target_id)
        .bind(company_id)
        .fetch_optional(&mut *c)
        .await?;
    row.map(|row| {
        let company: String = row.try_get("company_id").map_err(|_| unavailable())?;
        if company != company_id {
            return Err(crate::domain::attachment_version::scope_forbidden().into());
        }
        row.try_get::<i64, _>("version")
            .map_err(|_| unavailable())
            .map_err(Into::into)
    })
    .transpose()
}

fn authority_json(actor: &AttachmentVersionActor, capability: &str) -> String {
    json!({
        "userId": actor.user_id,
        "assignmentId": actor.assignment_id,
        "companyId": actor.company_id,
        "role": actor.role,
        "scopeKind": actor.scope_kind,
        "capability": capability,
    })
    .to_string()
}

#[allow(clippy::too_many_arguments)]
async fn retain(
    c: &mut PgConnection,
    actor: &AttachmentVersionActor,
    authority: &str,
    command: &str,
    request_hash: &str,
    record: &str,
    result: &AttachmentVersionStep,
    now: i64,
) -> Result<(), Error> {
    let result_json = serde_json::to_value(result).map_err(|_| unavailable())?;
    sqlx::query(
        "INSERT INTO command_receipts(command_id,operation,actor_id,request_hash,record_id,result_json,created_at,authority_json) \
         VALUES($1,$2,$3,$4,$5,$6,$7,$8)",
    )
    .bind(command)
    .bind(VERSION_STEP_OPERATION)
    .bind(&actor.user_id)
    .bind(request_hash)
    .bind(record)
    .bind(result_json.to_string())
    .bind(now)
    .bind(authority)
    .execute(&mut *c)
    .await?;
    let record_type = VERSION_STEP_OPERATION
        .split('.')
        .take(2)
        .collect::<Vec<_>>()
        .join("_");
    sqlx::query(
        "INSERT INTO audit_events(id,command_id,actor_id,assignment_id,action,record_type,record_id,record_version,purpose,created_at,authority_json) \
         VALUES(gen_random_uuid()::text,$1,$2,$3,$4,$5,$6,$7,$4,$8,$9)",
    )
    .bind(command)
    .bind(&actor.user_id)
    .bind(&actor.assignment_id)
    .bind(VERSION_STEP_OPERATION)
    .bind(&record_type)
    .bind(record)
    .bind(result.version)
    .bind(now)
    .bind(authority)
    .execute(&mut *c)
    .await?;
    let payload = json!({
        "authority": serde_json::from_str::<serde_json::Value>(authority).unwrap_or(serde_json::Value::Null),
        "targetType": result.target_type,
        "targetId": result.target_id,
        "companyId": actor.company_id,
        "version": result.version,
    });
    sqlx::query(
        "INSERT INTO outbox_events(id,command_id,event_type,aggregate_type,aggregate_id,aggregate_version,payload_json,created_at) \
         VALUES(gen_random_uuid()::text,$1,$2,$3,$4,$5,$6,$7)",
    )
    .bind(command)
    .bind(VERSION_STEP_OPERATION)
    .bind(&record_type)
    .bind(record)
    .bind(result.version)
    .bind(payload.to_string())
    .bind(now)
    .execute(&mut *c)
    .await?;
    Ok(())
}

/// Native mirror of the Files `versionGuard` assertion. The guard is checked
/// here, on the caller transaction, against the live row and the caller's
/// Company scope; no transport-supplied assertion substitutes for it.
pub async fn assert_attachment_version(
    c: &mut PgConnection,
    actor: &AttachmentVersionActor,
    target_type: &str,
    target_id: &str,
    expected_version: i64,
) -> Result<(), Error> {
    serializable(c).await?;
    require_actor_scope(
        &actor.company_id,
        &actor.assignment_id,
        &actor.user_id,
        &actor.scope_kind,
    )
    .map_err(Error::Business)?;
    let target = resolve(target_type)?;
    if expected_version <= 0 {
        return Err(crate::domain::attachment_version::invalid_expected_version().into());
    }
    let Some(version) = current_version(c, target, &actor.company_id, target_id).await? else {
        return Err(target_not_found().into());
    };
    if version != expected_version {
        return Err(target_changed().into());
    }
    Ok(())
}

/// Execute the allow-listed, single version step on the caller transaction.
/// A stale expected version is a no-op (no row change, no receipt), matching
/// the retired SQL statement's zero-row result. A successful step writes the
/// receipt, audit event and outbox event in this same unit.
pub async fn bump_attachment_version(
    c: &mut PgConnection,
    actor: &AttachmentVersionActor,
    command: &CommandKey,
    target_type: &str,
    target_id: &str,
    expected_version: i64,
    now: i64,
) -> Result<AttachmentVersionStep, Error> {
    serializable(c).await?;
    require_actor_scope(
        &actor.company_id,
        &actor.assignment_id,
        &actor.user_id,
        &actor.scope_kind,
    )
    .map_err(Error::Business)?;
    let target = resolve(target_type)?;
    if expected_version <= 0 {
        return Err(crate::domain::attachment_version::invalid_expected_version().into());
    }
    if target_id.is_empty() || target_id.len() > 400 {
        return Err(crate::domain::attachment_version::target_not_found().into());
    }
    let request_hash = version_fingerprint(
        &actor.user_id,
        &actor.assignment_id,
        &actor.company_id,
        VERSION_STEP_OPERATION,
        target_id,
        expected_version,
        target_type,
    );

    let existing = sqlx::query(
        "SELECT request_hash,result_json FROM command_receipts WHERE command_id=$1 FOR UPDATE",
    )
    .bind(command.as_str())
    .fetch_optional(&mut *c)
    .await?;
    if let Some(row) = existing {
        let saved: String = row.try_get("request_hash").map_err(|_| unavailable())?;
        if saved != request_hash {
            return Err(BusinessError::new(
                FailureKind::Conflict,
                "IDEMPOTENCY_CONFLICT",
                "This command key belongs to a different request.",
            )
            .into());
        }
        let raw: Option<String> = row.try_get("result_json").map_err(|_| unavailable())?;
        return raw
            .and_then(|value| serde_json::from_str::<AttachmentVersionStep>(&value).ok())
            .ok_or_else(|| unavailable().into());
    }

    let Some(version) = current_version(c, target, &actor.company_id, target_id).await? else {
        return Err(target_not_found().into());
    };
    if version != expected_version {
        return Ok(AttachmentVersionStep {
            target_type: target_type.to_owned(),
            target_id: target_id.to_owned(),
            changed: false,
            version,
        });
    }

    let sql = target.step_update();
    let updated = if target.updated_at {
        sqlx::query(AssertSqlSafe(sql))
            .bind(now)
            .bind(target_id)
            .bind(expected_version)
            .bind(&actor.company_id)
            .execute(&mut *c)
            .await?
            .rows_affected()
    } else {
        sqlx::query(AssertSqlSafe(sql))
            .bind(target_id)
            .bind(expected_version)
            .bind(&actor.company_id)
            .execute(&mut *c)
            .await?
            .rows_affected()
    };
    if updated == 0 {
        return Ok(AttachmentVersionStep {
            target_type: target_type.to_owned(),
            target_id: target_id.to_owned(),
            changed: false,
            version,
        });
    }
    let result = AttachmentVersionStep {
        target_type: target_type.to_owned(),
        target_id: target_id.to_owned(),
        changed: true,
        version: expected_version + 1,
    };
    retain(
        c,
        actor,
        &authority_json(actor, target.capability),
        command.as_str(),
        &request_hash,
        target_id,
        &result,
        now,
    )
    .await?;
    Ok(result)
}
